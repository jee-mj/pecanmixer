# Native backend receipt contract (draft)

This reviewed backend-only increment follows the explicit settings storage
boundary at `9f29418025b50fbe56c04f7b0144059f2d151fc7`. It integrates the compatible
native command contract and delayed-result suppression. The GTK executable still
starts its separate read-only observer; no production owner or controls are
activated. Mixer gestures, display-settings effects/worker and meter candidates
are separate and excluded. The complete functional JS port remains the goal.

## Implemented boundary

`Graaf::start_native(inbox)` supplies exact owned origin frames for future native
intents. `dispatch_native(NativeIntent)` binds commands to that original graph Arc
and its published object generations, rather than reconstructing a graph or
adopting identities at admission. Other-owner, copied, evicted, stale-epoch and
replaced-object origins are rejected. Unchanged legacy IPC remains admission-
stamped and retains its earlier pre-admission stale-intent limitation.

The supported native commands are connect, disconnect, target, volume and mute.
The native API rejects recording/bus/transport commands. Its bounds are 32 commands
per ordered intent, 32 in-flight intents, 64 retained origin publications, and a
two-second observation window. Existing FIFO limits, stop-all reservation,
reconnect invalidation and cancellation behavior remain in place. Batches are
ordered, not atomic; earlier effects may exist after a later failure.

Receipts distinguish Queued, ExecutedLocally, Observed, ObservedNoOp, Rejected and
Unconfirmed. Admission and local RPC submission are not server success. Observed
means the requested postcondition appeared in the server-fed graph mirror; it is
not a causal PipeWire/WirePlumber transaction acknowledgement. No-op handling
publishes fresh graph progress. Timeout/cancellation can report unconfirmed
partial effects; one already-dequeued command may still finish.

`HistoryResults` emits successful results only for observed states. A constant-
size monotonic watermark suppresses duplicates and arbitrarily delayed older
results, including ticket wrap ordering at one source stamp. Use one bridge per
serialized history controller and recreate them together. A successful history
result still requires delivery of its sufficiently new graph. This is not full
undo integration: old history entries' node/port identity still needs validation
before native undo can be enabled.

Native command rejection does not establish recorder finalization. The separate
shutdown handle continues to require joined graph/writer threads and explicit
writer results, preserving the legacy tray completion behavior and its limits.

## Final integration checks

| Check | Result |
|---|---|
| `cargo test -p pecan-core -p pecan-pipewire --locked` | PASS: 42 core + 53 backend tests |
| `cargo test -p pecanmixer --lib --locked` | PASS: 10 legacy tests (105 total non-GTK tests) |
| `cargo build -p pecan-ui-gtk --locked` | PASS |
| `cargo check -p pecanmixer --examples --locked` | PASS: all five existing examples |
| Reviewed-tree, source/privacy/scope and whitespace checks | PASS: core/backend source matches independently reviewed candidate; only synthetic fixtures |

Independent review separately covered delayed deliveries, mixed queues,
reconnects and lifecycle probes. The integration retains the later settings
storage commit and changes no GTK/startup source, dependency/version/lockfile,
legacy JS, IPC payload or settings storage implementation. Build/test commands
ran without Node/npm on PATH. GTK widget/fixture suites were not repeated for
this backend-only increment.

Not run: live PipeWire/WirePlumber, hardware/restart/audio or recording acceptance,
physical desktop/Wayland/AT-SPI/Orca workflows, real-time/performance benchmarks,
crash/power-loss behavior, release packaging or final license audit. No merge,
release, version bump, JS removal or full-parity claim is included.

The [identity follow-up](./native-identity-batch.md) now supplies registry-backed retained history and selector guards; the owning native shell still needs to deliver witnesses and wire history/controls.
