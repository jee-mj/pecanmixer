# Native continuous gain and partial mute candidate

This candidate follows the reviewed source batch at `fc67e261`. It wires the
reviewed pure `Fader` and partial-channel `Memory` policies into the explicit
native `Session`/`NativeWorkspace` library. **Independent review remains a gate.**
Main startup still opens the earlier observational preview. The legacy application
is retained, native recording is disabled, and this is not desktop/audio acceptance.

## Completion and identity contract

- `Session::begin_volume` accepts only the currently presented exact graph Arc
  and returns an opaque `GestureToken`. Its initial full gain vector is retained.
- Registry owner, epoch, node and all non-monitor audio-port generations bind the
  group. Ordered channel mapping and vector length are checked too. Ordinary
  gain/mute/link/label observations and origin-cache eviction do not cancel an
  unchanged binding. Replacement, disconnect, mapping change or missing witness
  abandon it; old callbacks cannot adopt a replacement's new origin.
- Each update replaces one desired dB target. There is at most one admitted
  volume receipt. After it is observed against a sufficiently new witnessed
  graph, the next sample uses the latest exact owned graph and current full
  vector, preserving current unselected channels. No motion queue is retained.
- End finalizes the last actual update; a click without motion submits nothing.
  Intermediate samples allocate tickets from the same history controller as
  patch/mute/undo. They create no history entries. Final observed completion
  records one initial-to-final full-vector step. Its node/port generations and
  vector mapping remain guarded during undo/redo.
- `Queued` and `ExecutedLocally` never acknowledge history or gain memory.
  `ObservedNoOp` still requires fresh graph progress. Native observation proves
  a server-fed postcondition, not a causal transaction acknowledgement. Rejected
  or unconfirmed final samples do not manufacture a completed group or rollback
  earlier samples. Unconfirmed effects invalidate the affected gain memory.
- Partial mute prepares one memory proposal at dispatch time, applying it only
  after observation. It remembers the selected mean, restores that mean, and
  keeps every unselected channel. Undo retains the exact initial full vector,
  including unequal selected gains. Like the legacy implementation, volume
  undo/redo does not itself edit the remembered-mute map. A no-op fader forget
  also waits for actual completion. Memory is bounded to 256 strip entries.

`HistoryGraph::bind_objects` exposes opaque `RegistryBinding` metadata rather
than deriving authority from numeric IDs, serials, names or serialized DTOs.
`Controller::reserve_ticket` shares correlation; `record_volume_binding` records
an already observed group and retains its vector dependencies. These APIs do not
attest completion on their own. The backend dispatch/receipt and graph DTO
contracts are unchanged.

## GTK adapter

`application::CoordinatedMixer` replaces the earlier one-write mixer only inside
`NativeWorkspace`. The standalone/generic `MixerControls` and observational main
remain available. The new adapter supports relative vertical drag, Shift fine
motion, 0.1 dB rounding, wheel 1 dB / Shift 0.1 dB with 600 ms grouping,
double-click 0 dB reset, partial mute/mean restoration, whole-node mute, and GTK
keyboard/discrete accessibility gain changes. The label always displays observed
gain; a drag thumb retains the legacy local relative position while waiting.

The active row stays usable while its intermediate receipt is pending. Other
mutations and history are serialized until the group ends and completes.
Overlapping input modes are rejected; they cannot retarget each other's tokens.
Pointer cancellation/focus exit finalizes submitted input rather than rolling it
back. A window release observer survives focus loss and replacement so a held key
cannot become a fresh replacement gesture, while a released-off-widget key does
not strand later input. GTK's initial zero-motion drag notification is ignored;
click and drag recognizers share a sequence so a double-click is one reset.

The caller must retain the Session and mixer, polling from the existing main
clock even while hidden. No per-row timer, worker, filesystem access, audio server
or extra inbox consumer is created. Dropping/retiring a row abandons its token;
already admitted effects and their receipt remain Session-owned. The existing
finalization admission gate also closes this input path.

## Synthetic evidence

Focused tests exercise a 100,000-update storm, one-flight/final-target behavior,
full-vector undo/redo, graph-before-receipt and receipt-before-graph, no-op gain
memory, ordinary revision/cache eviction, node/selected/unselected-port
replacement, reconnect, channel remapping, admission failure, overlap, unequal
mute/restoration, fresh input and real GTK events in an explicitly marked Xvfb.
The pointer, double-click, Tab/off-widget release, modifier/focus and held-key
replacement tests use synthetic XTest input, not physical devices.

Final cloud checks (synthetic fixtures; GTK fatal criticals enabled):

| Check | Result |
|---|---|
| Core / backend / legacy Rust tests | PASS: 67 / 96 / 10 (173 total) |
| GTK default / accessibility modes | PASS: 176 each |
| GTK accessibility + XTest + isolated tray D-Bus | PASS: 206; includes 26 XTest and four private-bus tests |
| Native build / five legacy examples | PASS |
| Read-only CLI headless fixture / mixer and patchbay smoke | PASS |
| Scoped formatting, whitespace, source/privacy and native Cargo/ELF audit | PASS; no dependency, backend, main startup or version change |

No final check failed and no dependency/environment blocker remains. Initial
fixture/API construction errors, the GTK zero-motion/double-click regression,
and weak-owner teardown were fixed before that gate. No runtime logs, captures or user hardware identifiers are
published. No new runtime dependency, Node invocation or graph DTO was added.

## Remaining functional gates

Independent candidate review and production startup adoption remain. Bus gain,
arming/configuration, output/Rec selection and routing controls, recording,
transport, meters, settings menus/effects, full shortcuts/queued undo and process
integration remain separate parity work. Real audio correctness, physical
Wayland/X11/AT-SPI/Orca behavior, live restart handling, performance and packaging
have not been accepted. Backend cancellation may allow one already claimed call;
this adapter cannot roll back such effects.
