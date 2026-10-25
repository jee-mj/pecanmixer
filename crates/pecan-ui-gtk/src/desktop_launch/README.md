# Explicit native desktop launch boundary

This imports the previously reviewed helper `c9c4e00f8207e474b14ca98b9c04972cd9e3fa4c`
onto PR2 `d0d8f23fa77d1930dd5401addb2d1b1d990c50dc`, then narrows ownership to the
native identity and preserves the two explicit native path options. Its staged
writer, path-bound snapshots, bounded reads, error/reload rules and personal-only
disable behavior are reused. No startup, preferences-worker, shell, tray, backend
or packaging caller is added. It does not install or launch anything implicitly.

## Commands and identity

```rust,ignore
let command = LaunchCommand::resolve(&supplied_current_exe, supplied_appimage.as_deref())?
    .with_paths(
        Some("/synthetic/preferences/native.json".into()),
        Some("/synthetic/recordings".into()),
    )?;
let normal = command.argv(LaunchMode::Visible);
let hidden = command.argv(LaunchMode::Hidden);
```

The supplied AppImage path takes precedence over its temporary mounted executable.
There is no APPIMAGE, HOME or XDG lookup, path canonicalization, process creation,
or check that the supplied artifact actually starts the native application.
Future callers must establish that native executable/bundle authority themselves.

`with_paths(settings, recording_root)` accepts only these known, absolute UTF-8
paths, retaining their exact values. Settings needs a filename. Controls/NUL and
overlong paths return typed errors. Missing options stay missing: this does not
choose a profile or recording directory. In the current native parser, omission
means memory-only settings and no selected recording root.

Both launch modes retain `--settings PATH` and `--recording-root PATH` when supplied.
Visible launch adds neither hidden nor tray flags. Hidden launch adds exactly
`--hidden --tray`, requesting the existing native tray service and its no-host
visibility recovery. It does not enable autostart. `argv()` is an inspection vector;
`exec_value()` is a desktop FILE value, not a shell command to execute.

The sole autostart target is `autostart/nl.jetse.pecanmixer.Native.desktop`, matching
the current native GApplication identity rather than the legacy basename. Current
native entries have a distinct Name and `X-PecanMixer-Native-Autostart=1` marker.
Only exact current generated templates are mutable. The legacy filename
`nl.jetse.pecanmixer.desktop` is never read, renamed, replaced or removed by this
adapter. If legacy-template bytes appear at the native filename, they are reported
as `Managed(Legacy)` for explanation and all mutating operations reject them.
Custom/unknown contents remain conflicts. The marker/template is recognition,
not authenticated ownership of a file someone deliberately made identical.

## Escaping and bounds

Commands follow the [Desktop Entry Exec rules](https://specifications.freedesktop.org/desktop-entry/latest/exec-variables.html)
and [desktop string escaping](https://specifications.freedesktop.org/desktop-entry/latest/value-types.html).
Every supplied executable/path is one quoted argument; quotes, backticks, dollar
signs and backslashes receive both required escaping layers. Literal percent
characters in argument paths become `%%`. A supplied `literal%U` path remains
literal data after the launcher's single expansion; it requests no URI/file field.
An equals sign is valid in an argument path, but remains invalid in the executable.

The original GIO executable-percent limitation remains: `%` in the executable
returns `PercentInExecutable`, rather than adding an implicit wrapper dependency.
This is separate from percent characters in non-executable arguments. Paths are
bounded to 8192 UTF-8 bytes each, and construction also checks the complete escaped
entry against the reader's 64 KiB bound. It cannot write an entry too large to reload.

Recognition parses only the exact generated native template with the two known
options and hidden/tray suffix, then requires byte-identical regeneration. Unknown
or duplicated options, missing markers, raw field-code requests and edited entries
are preserved. GLib key-file/argv and GIO preflight fixtures never launch a program;
they do not establish physical desktop/locale or runtime field-expansion acceptance.

## Explicit file operations

Construct `AutostartFile::new(explicit_config_root)` with an absolute root without
`..`. Enable requires the root already to exist and may create only its autostart
child. All filesystem calls belong on the future serialized off-UI worker.

1. `load()` returns the path-bound byte snapshot: Missing, Managed(Native),
   Managed(Legacy), or Conflict. Errors/symlinks/non-regular targets are not false
   disabled results. Missing says nothing about legacy or system-wide entries.
   `snapshot.launch_command()` exposes the captured native command and its path
   getters without duplicating the parser; it is not a fresh filesystem read.
2. Pass that exact snapshot to `enable`, `disable` or explicit `refresh`. Only
   Missing/current Native are writable. Successful updates return a replacement
   snapshot and `changed`; identical enable and missing disable are no-ops.
3. Release refresh changes only an existing native entry's executable. Its stored
   settings/recording paths survive even if this ordinary launch omits or changes
   them; changing those options requires explicit enable/update. It never creates
   a missing entry or takes over legacy/custom content. Debug refresh
   performs no I/O and returns its supplied snapshot, so use load for fresh state.
4. Reload after ChangedOnDisk. There is no automatic replacement/recovery of a
   conflict. `Io { published: true, .. }` means rename/unlink happened before a
   durability error; reload before deciding what to display or retry.

Writes use an owner-only `create_new` sibling, file sync, same-directory rename
and directory sync. Checks detect stale input before publication; own stages are
removed on ordinary errors. Crash leftovers and unrelated files are not promoted,
enumerated for cleanup, or removed. Disable removes only the recognized native
personal entry, never a system-wide entry or `Hidden=true` override.

As in the reviewed adapter, this is a serialized single-owner boundary, not a
cross-process filesystem CAS. External edits in the final check/rename/unlink gap
and hostile ancestor replacement are outside its guarantees. The explicit root,
autostart child and target reject symlinks at checks; ancestors of that supplied
root remain the caller's responsibility. Injected failures do not prove crash
durability or successful login launch.

## Owner seams and remaining decisions

No live autostart is mounted. Future adoption must supply the explicit native
launch identity, settings/recording paths, config root and refresh mode. Reuse the
existing bounded preferences worker for typed desktop requests/results, independent
of SettingsV1 persistence/readiness and file errors. Keep an admitted request until
its actual result, report conflicts/pending/errors truthfully, and drain/join that
same worker through the existing quit lifecycle. No GTK/ksni callback should do I/O.

Default profile/root discovery, native packaging/AppImage entrypoint verification,
ordinary launcher installation, simultaneous legacy/native startup and eventual
cutover remain decisions outside this API. No takeover policy or system-wide
autostart masking is inferred. The helper does not establish that native and legacy
processes arbitrate as one application or that a tray host/login manager accepts it.

Run the existing focused suite with
`cargo test -p pecan-ui-gtk --locked --lib desktop_launch::`.
Fixtures use authored generic values and `/tmp` directories. They cover argument
preservation through the actual native option parser, both escaping layers,
percent data, combined size bounds, native/legacy coexistence, stale snapshots,
idempotence, relocation, conflicts, symlinks, interruption and post-publish errors.
No real profile, generated process, login session, audio server or hardware is used.
