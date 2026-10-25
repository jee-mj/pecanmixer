# Reviewed native login context continuity

Independently reviewed `20dcebfa` adds generated argument continuity on top of
PR2 `48597f7`. `DesktopContext` retains its already explicit management root in
the generated command, paired with `--native-executable` equal to that command's
executable. Existing settings/recording paths and hidden/tray flags survive.
The existing CLI reconstructs the same capability for a newly started primary;
no profile/root lookup, native executable discovery, automatic enablement or
startup rewrite is added.

Release refresh relocates the executable and its assertion together while
preserving stored settings, recording root and management root. A rootless older
native entry remains readable and is never rewritten merely by inspection.
Explicit enable/update or release refresh can attach the supplied authorized
context. Missing refresh remains missing. Foreign management roots, mismatched
executable assertions, incomplete/duplicate arguments, custom and legacy entries
remain protected. Existing stale snapshot, bounded read/write and post-publication
error contracts are unchanged. Literal percent in the executable remains
explicitly unsupported; literal percent in argument paths stays escaped data.

## Independent review and integration

All 46 focused desktop checks pass in each feature mode, plus native build and
formatting. Three independent probe groups also pass:

- Forty complete argument/preflight combinations cover optional paths, both launch
  modes, spaces, quotes, backslashes, shell metacharacters, percent data and Unicode.
  Expected vectors are constructed independently, decoded through GLib and passed
  through the existing native CLI. GIO preflights a synthetic marker without
  launching it.
- Actual serialized worker inspection leaves an older rootless entry byte for
  byte unchanged. Explicit refresh preserves its stored paths while relocating
  the executable, and reconstructed fresh-worker inspection retains the resulting
  context without rewriting it; an explicit disable still works.
- Foreign roots, changed or duplicated executable assertions, malformed options,
  changed markers and legacy bytes reject mutation and retain their original
  file bytes.

Private probe registration is removed after review; no probe scaffolding or
temporary paths are added to production source. The reviewed commit applies
without conflicts onto `48597f7`; runtime and test files match `20dcebfa` exactly.
Only these adoption notes are added. Backend/reconnect, manifests, dependency
lockfile and versions are unchanged. Exact candidate
[CI40173147428](https://github.com/jee-mj/pecanmixer/actions/runs/40173147428)
passes 86 validator controls, 76 core, 230 backend and 546 feature GTK tests plus
native build, fixtures and dependency audits.

Fresh combined validation passes the same 86 validator controls, 76 core,
230 backend and 546 all-feature GTK tests, plus 492 default GTK tests and all ten
legacy library tests. All five legacy examples compile; none is executed.
Native build, formatting, Cargo/ELF checks, three headless fixtures and three
controlled smokes pass, with zero Node-tool attempts. Host and explicit-target
workspace packages are cleaned and rebuilt first. Source/privacy/whitespace and
unchanged dependencies/versions pass. There is no final local failure or
dependency/environment blocker. Exact combined PR-head hosted receipts are
verified separately after publication.

## Remaining boundary

An already-running primary keeps its original explicit context; secondary
activation introduces no context-switching/arbitration protocol. Older entries
need an explicit accepted update to gain context. Actual desktop/login behavior,
native artifact/packaging proof, simultaneous legacy/native startup and broader
session arbitration remain unverified or unfinished. This is argument and source
adoption, not proof of a successful physical login launch.

All review inputs are authored generic values and temporary directories. No
actual user profile/autostart entry, generated application process, desktop
session, audio backend or hardware is operated. No merge, release, version bump
or legacy JS removal is included.
