# Native migration batch 2 (draft)

This cleared increment follows [batch 1](./native-batch-1.md) at
`a0fa112946f7d9bb12d67b88b39be700e916667b`. It adds a read-only native patchbay and
a pure settings contract. It does not activate audio mutations, settings effects
or recording lifecycle changes. The application remains `0.1.0` and the legacy
React/Tauri application remains the supported mixer.

## Implemented and reviewed

- **Read-only patchbay:** native node/port modules, decorative Cairo cables and a
  named connection list. Missing/inconsistent endpoints retain text rows and a
  visible undrawn count. Link-only accepted snapshots update the page; stale
  snapshots and meter/transport-only batches do not rebuild it. Disconnect clears
  diagram/list state, and selection/focus recovery uses native GTK controls.
  Rendering retains one current model/geometry, with widgets proportional to the
  current graph; it is not virtualized or performance-benchmarked.
- **Pure settings contract:** validated schema-version-1 data, source defaults,
  serialization, explicit corrupt/unsupported-input preservation status and
  migration of the five supplied web preference entries. Clock port IDs and
  unstored session state are excluded. Parsing is injected, with no runtime
  dependency additions to the core. No browser/profile access, filesystem write,
  settings widget or runtime effect adapter is included.

The patchbay source and its GTK behavior were independently reviewed. The
settings contract was independently checked with 36 core tests, actual legacy
loader comparisons and two additional probes. Malformed legacy input is
intentionally validated more strictly; compatibility with arbitrary permissive
JavaScript values is not claimed. Name-based hidden keys are retained without
promising that every application's node name remains stable forever.

Details: [GTK observation/patchbay notes](../../crates/pecan-ui-gtk/README.md) and
[settings fixtures/API/source rules](../../fixtures/settings/README.md).

## Combined cloud checks

| Check | Result |
|---|---|
| `cargo test -p pecan-core -p pecan-pipewire --locked` | PASS: 36 core + 21 backend tests |
| `cargo test -p pecanmixer --lib --locked` | PASS: 4 legacy tests; 61 core/backend/legacy tests total |
| `cargo build -p pecan-ui-gtk --locked` | PASS |
| GTK-package library tests under Xvfb/session bus | PASS: six tests in each default / `a11y-tests` mode, with fatal criticals enabled |
| Three headless fixture outputs / three `--patchbay --smoke` runs | PASS; audio-basic output equals committed expected JSON |
| Formatting, reviewed-tree/scope checks, native tree/linked-library checks | PASS; no WebKit/JavaScriptCore/Tauri path in the native preview |

Tests ran with Node/npm absent from `PATH`. The six GTK-package tests include
three display-free geometry tests, the binding fixture and two widget suites.
Coverage includes 10,000 coalesced graph snapshots, link-only updates, dangling
endpoints, reconnect/stale replay, scrolling/focus, long labels, 240 modules/160
links and three allocation sizes. Synthetic Cairo snapshots were inspected;
these checks do not establish physical desktop, screen-reader or performance
acceptance. The lockfile, dependency manifests, legacy JS/IPC and product versions
are unchanged by this increment.

## Remaining boundaries

- Native mixer/patchbay mutation controls remain unavailable. Queue success is
  admission, not PipeWire completion. History still needs an actual completion
  adapter and newer graph progress, including confirmed no-ops; `Action::Batch`
  supports independent operations against one snapshot.
- Cancellation discards queued work, but one already-dequeued command may finish.
  Intent already superseded before admission needs richer caller identity than
  unchanged legacy IPC supplies.
- Recorder completion/finalization and filesystem/settings-effect adapters are
  separate follow-ups and are excluded. The legacy tray's fixed 600ms delay is
  unchanged. This batch makes no graceful recorder-shutdown or durability claim.
- Full routing/gesture parity, partial-strip mute state, locale collation,
  animated meters, tray/autostart, settings persistence and final cutover remain.
- Not run: live PipeWire/WirePlumber or hardware/restart/audio acceptance,
  real-time/latency/CPU/memory benchmarks, physical Wayland/X11 desktop or
  AT-SPI/Orca workflows, packaging/distribution or final license audit.

The legacy JS/Node/Tauri/WebKit stack remains until parity and cutover gates
pass. No merge, release or version bump is included.
