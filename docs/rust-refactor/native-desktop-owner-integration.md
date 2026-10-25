# Reviewed explicit-context desktop owner integration

This integrates independently reviewed desktop source `bc438072` onto PR2 head
`becb4f7d`, preserving the reviewed reconnect observation/retirement path. It adds
mounted native login controls only when the caller supplies both an absolute
`--autostart-config-root` and trusted `--native-executable`. Ordinary launch only
inspects that explicit context; enabling/updating, disabling or refreshing an
entry requires an accepted user control action. No root, profile or executable
is discovered or guessed. Legacy and other-root entries remain protected.

The same serialized settings worker, request allocator and off-UI quit drain
handle desktop operations and optional preference storage. Memory-only preferences
stay independently usable. The shell and optional tray retain actual matching
results and immutable control tokens. Pending, unknown, conflict and error states
cannot report successful enablement. A newer queued presentation source survives
retired callbacks; same-source actions retain their last choice. No queue storage
or background worker is added to the existing mailbox, and reserved Quit remains
independent.

## Source and independent review

The three reviewed commits apply in order: `58ba2501` (worker capability),
`b01614ce` (shell/tray/CLI) and `bc438072` (mailbox callback ordering). The only
integration conflict is an appended parity-document section, resolved by keeping
both reconnect and desktop notes. All desktop runtime/test files match
`bc438072` byte for byte. The bus/reconnect runtime/test files match `becb4f7d`;
core, backend, legacy application, dependency manifests, lockfile and versions
are unchanged. No novel runtime reconciliation is introduced.

Independent re-review passes 41 desktop checks, 14 default tray checks, four
worker checks, 11 preference checks and three extra callback/conflict/quit
probes. The original stale-callback loss reproduces before correction and passes
afterward. The probe acknowledges the ignored stale post as false; its action
and file assertions remain unchanged. Exact candidate
[run 40171438619](https://github.com/jee-mj/pecanmixer/actions/runs/40171438619)
passes 86 validator controls, 76 core, 230 backend and 524 feature GTK tests plus
native build, fixtures and dependency checks. Filters overlap and are not added
as separate totals.

## Combined local validation

Fresh combined checks pass 86 validator controls, 76 core, 230 backend and 541
all-feature GTK tests, plus 487 default GTK tests and all ten legacy library
tests. All five legacy examples compile; none is executed. Native build,
formatting, Cargo/ELF dependency audits, three headless fixtures and three
controlled smokes pass. Host and explicit-target workspace packages were cleaned
and rebuilt before the source gate; zero Node-tool attempts are recorded.
Whitespace, source/privacy and unchanged dependencies/versions pass. There is no
final local failure or dependency/environment blocker. Exact combined PR-head
hosted checks are verified separately after publication; candidate hosted success
alone is not combined-head evidence.

## Remaining user-facing boundary

The generated native login command carries explicit settings/recording paths and
hidden/tray flags, but not the desktop-management context. A login-started process
has no management controls until quit and relaunch with that explicit context.
Context/session arbitration, native artifact/packaging proof and actual login
acceptance remain unfinished. Executable percent is explicitly unsupported.
This integration is not full autostart/session parity.

Fixture/headless/smoke modes reject desktop context before constructing resources.
The tests use authored synthetic executable paths, unique temporary directories
and injected owners. They do not change an actual user profile/autostart entry,
launch a program, operate a desktop session or start PipeWire. Physical desktop,
audio, real-time/performance, crash-durability and packaging acceptance remain
unperformed. No legacy JS removal, merge, release or version change is included.
