# Explicit desktop launch boundary

This source-only library has no startup, settings, tray or package caller. It
does not read HOME, XDG or APPIMAGE, install into a profile, start a process,
register a desktop application, or change the application identity. Its tests
write only synthetic temporary directories under `/tmp`.

## Preserved behavior and hardening

The oracle is `src-tauri/src/autostart.rs`, with the shell calls in
`src-tauri/src/lib.rs` and tray action in `src-tauri/src/tray.rs`. Autostart is a
Rust shell effect; there is no corresponding JS preference to migrate here.

- Keep `nl.jetse.pecanmixer.desktop`, Pecan Mixer, `audio-card`, categories and
  existing GNOME/KDE keys. This is not a native identity/packaging adoption.
- Autostart always passes `--hidden`. `exec_value(Visible)` and `argv(Visible)`
  support a normal desktop command without inventing a new command-line flag.
- Prefer the caller's APPIMAGE path to its temporary mounted executable. A moved
  AppImage can refresh an **existing** managed entry on an explicit release-mode
  call. Missing entries stay missing; debug refresh has no filesystem effects.
  Invalid supplied APPIMAGE fails instead of silently pointing at a mount.
- Desktop-file values need two escaping layers.
  Spaces, quotes, backslashes, backticks and dollar signs stay in one argument;
  the command is never handed to a shell. This fixes the old writer's incomplete
  escaping. UTF-8 paths retain the legacy/GLib behavior without lossy conversion.
- Relative, non-UTF-8, control-character, overlong, `%` and `=` executable paths return
  typed errors. In particular a newline cannot inject extra desktop-entry keys.
  This is representability validation, not proof a binary exists or can execute.
- A literal `%` requires `%%` in desktop syntax, but GIO checks the executable
  for existence before field expansion. A real percent-named executable then
  fails that check. The candidate returns `PercentInExecutable`, verified with
  the GIO parser and an executable-bit synthetic temp file without launching it.
  A wrapper command for unusual filenames would add an execution dependency and
  needs a later design decision; this module does not silently add one. Historical
  percent-path entries remain conflicts rather than being called working.
- The adapter changes/removes only an exact recognized native or legacy template
  at that filename. Custom keys, Hidden overrides, corrupt bytes and foreign
  entries are conflicts, preserved verbatim. A legacy entry with bad historical
  escaping can be recognized for repair; recognition does not validate launching
  its old command. Recognition is template compatibility, not authenticated
  ownership of a file someone else deliberately made identical.

The implementation follows the
[Desktop Entry Exec rules](https://specifications.freedesktop.org/desktop-entry/latest/exec-variables.html),
[desktop string escaping](https://specifications.freedesktop.org/desktop-entry/latest/value-types.html),
[personal autostart directory behavior](https://specifications.freedesktop.org/autostart/latest/)
and [APPIMAGE path semantics](https://docs.appimage.org/packaging-guide/environment-variables.html).
GLib KeyFile/argv checks cover quoting without executing any generated command.
They are not evidence that every desktop host accepts every UTF-8 pathname.
The percent limitation is also visible in
[GIO's key-file preflight and expansion](https://github.com/GNOME/glib/blob/main/gio/gdesktopappinfo.c).

## Owner integration contract

1. Choose the final executable and config root explicitly in the future shell.
   `LaunchCommand::resolve(current_exe, appimage)` has no environment access or
   I/O. Keep the mounted executable out of persisted AppImage entries. The caller
   must supply an absolute path; resolution does not canonicalize symlinks.
2. Construct `AutostartFile::new(config_root)` using an explicit absolute
   XDG_CONFIG_HOME directory, with no `..` components. Its target is always that
   root's `autostart/nl.jetse.pecanmixer.desktop`. The root must already exist for
   enable; only the autostart child can be created. No default profile is chosen.
3. On a serialized, off-UI worker, `load()` produces a path-bound byte snapshot:
   Missing, Managed(Native/Legacy), or Conflict. File errors, oversized input,
   symlinks and non-regular targets are errors, never a false "disabled" result.
4. Pass that exact snapshot to `enable`, `disable`, or explicit `refresh`.
   Successful updates return a replacement snapshot and `changed`. Repeated
   enable with identical bytes and repeated disable when missing are no-ops.
   Read/missing disable/refresh do not create directories. Debug refresh returns
   the supplied snapshot unchanged, so it is **not** a fresh enabled-state read.
5. Surface errors and reload after ChangedOnDisk. Conflict has no implicit
   replace/delete/recovery action. A future UI needs an explicit review path for
   customized entries; do not pretend the requested setting was applied.
6. Inspect `FileError::Io.published`. True means rename/unlink already happened,
   but directory durability failed. Reload before deciding what to show/retry.
   A successful no-op reports the inspected file state, not a new durability
   acknowledgement for a prior failed sync. Never equate a file with confirmed
   login launch, successful single-instance activation or a usable tray host.

## Filesystem limits

The file is bounded to 64 KiB. Writes use an owner-only, `create_new` sibling,
file sync, same-directory atomic rename and directory sync. Creating the child
also syncs the config root. A pre-publication byte check detects edits during
staging. Own stages are removed on ordinary errors; crash leftovers are ignored
and never promoted or deleted automatically. Sibling files are never enumerated
for cleanup. Disable unlinks only the recognized target, then syncs the directory.

This is a single-owner adapter, not a cross-process compare-and-swap or lock. The
caller must serialize operations and trust stable directories; external edits
in the final check/rename/unlink gap or hostile ancestor replacement are outside
its guarantees. The config root, its autostart child, and destination reject
symlinks at checks. Ancestors of the explicitly supplied root are the caller's
responsibility; intentional profile symlinks need a caller-resolved, authorized
root. Actual storage crash durability is not established by injected failures.

Disable preserves the original personal-file removal behavior. It neither scans
system XDG directories nor generates `Hidden=true` to mask system installations.
If packaging later supplies a system autostart entry, that becomes an explicit
integration decision. No system entry or real profile is inspected by this slice.

## Validation and remaining adoption

Run `cargo test -p pecan-ui-gtk --locked --lib desktop_launch::` without a display
or audio server. Tests cover exact template compatibility, both launch modes,
reserved characters and UTF-8 parsing, explicit percent-path rejection, direct
GIO key-file preflight without launching, invalid paths, AppImage relocation,
release/debug refresh, idempotence, unknown-file/symlink preservation, stale/wrong
snapshots, bounded reads, staged interruption, external edits, post-publish sync
errors, retry, and unrelated/crash-leftover preservation. Test helpers inject
faults privately; there is no public failure injection API.

Cloud validation on 2026-10-24: 18 focused desktop-launch tests passed without a
display or audio server; all 203 GTK library tests passed with all features under
isolated Xvfb/D-Bus (including the existing synthetic input fixtures). The native
binary build, crate formatting and whitespace checks passed. No generated command
was executed, profile was installed, login session was changed, or real desktop
or hardware acceptance was attempted.

Future adoption still needs a serialized desktop-effect worker, visible pending/
error/retry state, explicit config/executable acquisition, release refresh policy,
coordinated quit/drain, and settings/tray integration. Packaging/identity,
system-wide override policy, installed AppImage relocation and real desktop login
acceptance remain unperformed. No main/startup, package metadata or legacy behavior
has changed in this candidate.
