# Pecan Mixer issue 1: initial native GTK refactor proposal

Date: 2026-10-24 (UTC)

Scope: research and proposal only; no implementation

Repository: <https://github.com/jee-mj/pecanmixer>

Issue: <https://github.com/jee-mj/pecanmixer/issues/1>

Verified fork revision: `d8d473818331342ab53a0e1e4d3ced7977293449`

## Decision in one page

Use **GTK 4 through direct `gtk4-rs` bindings** for the native application. Keep the existing PipeWire graph, metering, recorder, disabled MIDI code, StatusNotifierItem implementation, and XDG autostart code, but move them behind owned Rust commands/events that have no Tauri or GTK types. Port the TypeScript domain policy into a pure Rust crate and keep GTK as a view/controller layer.

Do **not** put libadwaita, Relm4, Lua, Blueprint, or a new UI DSL in the first PR:

- libadwaita is useful for GNOME-style preferences and adaptive navigation, but the main Pecan Mixer surface is a dense, desktop-first mixer/patchbay. It adds the native `libadwaita-1` runtime/build dependency and a second version floor without solving the hard mixer widgets.
- Relm4's message/update ideas fit the application, but a small reducer plus typed channels provides those properties without another framework commitment. Reconsider only if hand-written GTK synchronization becomes demonstrably costly.
- Lua preserves no current feature and would add a policy/security/test boundary. Omit it from the 0.1.1 path.
- GtkBuilder XML and composite templates remain available later; they are native resources and do not introduce JavaScript. The first slice can build one small view in Rust to minimize variables.

The **smallest credible first PR** is a read-only connectivity slice:

1. introduce a Cargo workspace with `pecan-core`, `pecan-pipewire`, and `pecan-ui-gtk`;
2. move, rather than copy, the current Rust graph/meter/recorder worker into `pecan-pipewire`;
3. keep the current Tauri application working through a thin adapter;
4. add a separate, non-installed GTK preview that shows connection state, rate/quantum, and the live node/port list; and
5. add display-free fixture tests for DTO ownership, event coalescing, reconnect epochs, and the read-only projection.

This first PR intentionally **does not** claim issue 1 completion. JavaScript, Node, Tauri, and WebKit coexist temporarily and the existing app remains the supported UI. The final issue acceptance is a later cutover gate: one GTK application, parity tests passing, old web artifacts removed, no WebKit/JavaScriptCore/Tauri dependency path, and a build/test/run workflow that does not contain Node. Version `0.1.1` changes only in that eventual merge-ready cutover, never in the first PR.

## Evidence and scope controls

This proposal reused, rather than repeated, the supplied artifacts:

- [`pecanmixer-js-node-inventory.md`](./pecanmixer-js-node-inventory.md)
- [`pecanmixer-js-node-inventory.json`](./pecanmixer-js-node-inventory.json)
- [`pecanmixer-issue-1-rd.md`](./pecanmixer-issue-1-rd.md)

The JSON inventory pins the same revision and enumerates the 18 frontend files, 13 IPC commands, four events, npm dependency closure, and Tauri/WebKit/JavaScriptCore Cargo closure. This report therefore concentrates on the initial GTK seam, concrete module boundaries, state flow, first-PR acceptance, and staged removal.

Freshness was independently checked in an isolated clone on 2026-10-24:

| Check | Result |
|---|---|
| Local `HEAD` | `d8d473818331342ab53a0e1e4d3ced7977293449` |
| Freshly fetched `origin/main` | `d8d473818331342ab53a0e1e4d3ced7977293449` |
| Working tree before report work | clean |
| Prior inventory revision | same SHA |
| Applicable repository instructions | no `AGENTS.md`; current tree has no `CLAUDE.md` |

The inspection was limited to this public repository, the companion research artifacts, issue 1, and primary GTK/GNOME/freedesktop documentation. No dependency was installed; no build, test, GUI, audio, packaging, or release command was run.

## Verified code facts that shape the design

These are static source facts at the pinned revision, not runtime observations:

- `src-tauri/src/graaf.rs` already isolates PipeWire on a named worker thread because its loop is not `Send` and the UI must not wait on graph traffic. It emits a full graph snapshot at most every 40 ms and retries after two seconds on disconnect. See [`graaf.rs` lines 1–19, 45–48, 234–293](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/graaf.rs).
- Its callback boundary is `Melding<'a>` with borrowed graph, peak, MIDI, and transport payloads. That is convenient for immediate Tauri serialization but is not a safe GTK cross-thread ownership boundary. See [`graaf.rs` lines 147–204](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/graaf.rs#L147-L204).
- Backend commands are already typed as `Opdracht`, sent non-blockingly through a PipeWire channel. The public `Graaf::doe` discards send failure. See [`graaf.rs` lines 206–263](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/graaf.rs#L206-L263).
- The 40 ms timer coalesces dirty graph changes, harvests meters, and emits transport every fifth tick. See [`graaf.rs` lines 786–890](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/graaf.rs#L786-L890).
- Meter capture is already UI-independent and passive. Peak values are harvested and reset; the frontend currently adds −60 dB mapping, 24 dB/s fall, 1.5 s peak hold, and 2 s clip indication. See [`meter.rs`](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/meter.rs) and [`src/meters.ts`](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src/meters.ts).
- Recording already uses writer threads and produces 32-bit-float WAVs. The graph thread periodically joins completed writers, but tray quit currently sends Stop, sleeps a fixed 600 ms, and exits. A native refactor must replace that guess with an explicit finalization acknowledgement. See [`opname.rs` lines 41–149](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/opname.rs#L41-L149), [`graaf.rs` lines 893–1045](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/graaf.rs#L893-L1045), and [`tray.rs` lines 101–111](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/tray.rs#L101-L111).
- `src-tauri/src/lib.rs` is the Tauri-specific shell: 13 commands, four emitted events, window hide-on-close, single-instance activation, tray/autostart setup, and worker startup. This is the seam to shrink, not reusable backend logic. See [`lib.rs`](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/lib.rs).
- The current production policy is still in TypeScript: graph projection (`indeling.ts`), route semantics (`mixer.ts`), buses (`bussen.ts`), actions and undo (`handelingen.ts`, `geschiedenis.ts`), and meter presentation (`meters.ts`). These must become deterministic Rust modules, not GTK callbacks.
- Persistent web preferences use `pecanmixer.bussen`, `pecanmixer.verborgen`, `pecanmixer.klok`, `pecanmixer.rijen`, and `pecanmixer.deling`. Clock port IDs are deliberately reset each session, and patchbay positions are session-only. See [`App.tsx` lines 29–146](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src/App.tsx#L29-L146), [`bussen.ts` lines 42–72](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src/bussen.ts#L42-L72), and [`Mixer.tsx` lines 280–390](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src/Mixer.tsx#L280-L390).
- MIDI remains authored but deliberately disabled at both Rust and TypeScript gates. It is not part of supported 0.1.1 parity. See [`graaf.rs` lines 162–166](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/graaf.rs#L162-L166) and [`api.ts` lines 79–84](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src/api.ts#L79-L84).
- There are six Rust unit tests and no frontend tests or committed CI. A visual rewrite alone cannot establish parity.

## UI architecture decision matrix

| Choice | No JS/WebView | Native/system cost | Fit for this UI | Accessibility basis | Cross-desktop posture | Decision |
|---|---:|---|---|---|---|---|
| **Direct GTK 4 + `gtk4-rs`** | Yes | GTK 4, GLib/GIO, GDK/GSK, Cairo, Pango, Graphene plus Rust bindings | Native `Scale`, toggles, paned/scrolled layouts, list models, actions, DrawingArea/custom widgets | GTK roles, states, relations and AT-SPI on Linux | Neutral GTK base; works on Wayland/X11 | **Use for first slice and default 0.1.1 architecture** |
| GTK 4 + libadwaita | Yes | Everything above plus Rust Adwaita bindings and native `libadwaita-1`; binding versions must align | Strong preferences, banners, view switchers, breakpoints and GNOME appearance; limited benefit for dense mixer/patchbay | Same GTK accessibility foundation; useful standard widgets and high-contrast/color preference support | GNOME-oriented visual language and extra distro/package floor | Keep the shell boundary compatible, but do not ship an optional dual path in 0.1.1; reconsider before full UI styling if GNOME HIG is a product goal |
| GTK 4 + Relm4 | Yes | GTK plus component/runtime/macro dependency | Elm-like messages and components fit stateful screens | Same GTK foundation | Depends on chosen widgets | Defer; use its pattern, not its dependency, until complexity proves the need |
| GTK 4 + GtkBuilder/composite templates | Yes | No JS; embedded XML/GResource tooling only | Good once views stabilize; separates layout from Rust | Standard GTK widgets remain accessible | Same as GTK | Use selectively after the first slice; no Blueprint compiler required |

Why direct GTK wins:

- Pecan Mixer is explicitly Linux/PipeWire software. Cross-platform GTK availability does not make the application cross-platform while PipeWire, StatusNotifierItem, XDG autostart, and the recording path remain Linux-specific.
- Direct GTK imposes one native UI runtime instead of GTK plus a second UI library. `gtk4-rs` exposes version feature gates; choose the lowest GTK API floor supported by the agreed distro matrix rather than enabling the newest feature set by habit.
- libadwaita's adaptive widgets are real value for narrow/mobile navigation, but issue 1's hard work is channel strips, patch geometry, meters, input semantics, and backend parity. Adwaita does not remove that work.
- Do not make libadwaita a compile-time user option in 0.1.1. Two shells double UI, accessibility and packaging validation. Keeping core/controller APIs GTK-only or toolkit-free preserves the option to adopt it once, deliberately, before release.

Current documentation inspected on 2026-10-24 lists `gtk4` 0.11.5 (MSRV 1.83) and `libadwaita` 0.9.2, with the latter depending on the matching GTK/GLib generation. Those are research facts, not a requested dependency change. The implementation PR should pin compatible crate versions in `Cargo.lock` and state an explicit native GTK/libadwaita minimum after the distro matrix is chosen.

## Proposed runtime architecture

```text
GtkApplication / GTK main thread
  ├─ AppController + pure AppState reducer
  ├─ native mixer / patchbay / recorder widgets
  ├─ GTK actions and keyboard/accessibility bindings
  └─ coalesced wake receiver
             ↑ wake token (capacity 1)
             │
      LatestEventInbox (owned Rust data only)
      ├─ latest graph snapshot + epoch/revision
      ├─ latest meter frame
      ├─ latest transport/connection state
      └─ bounded low-rate notices/command results
             ↑ publish; never touch GTK
             │
  dedicated pecan-pipewire worker thread
  ├─ current PipeWire main loop and two-second reconnect
  ├─ graph mirror and passive meter streams
  ├─ Rec bus / recorder coordination
  └─ writer threads
             ↑
        PipeWire / WirePlumber

Tray StatusNotifierItem thread
  └─ AppAction sender → same main-thread inbox (never owns a GTK object)
```

GTK/GDK objects are neither `Send` nor `Sync`; all GTK calls stay on the thread that initialized GTK. The worker crosses the boundary only with owned plain-Rust data. Signal/tick callbacks should not contain `unwrap` paths: current `gtk4-rs` documentation warns that a panic in a GTK callback aborts the process.

### Main-loop and backpressure pattern

Do not resurrect the removed `glib::MainContext::channel()` API. Use:

1. a custom latest-value inbox for self-contained snapshots;
2. a capacity-one async wake channel;
3. `glib::MainContext::spawn_local` to drain the inbox on the GTK thread; and
4. a GTK tick callback only for visible meter animation.

Publishing replaces an older unread graph, peak, transport, or connection snapshot. A capacity-one wake channel means at most one wake is pending even if the worker publishes repeatedly. The GTK receiver drains all current slots, reduces them into `AppState`, updates structural widgets once, and queues at most one meter redraw. This gives a bounded backlog without blocking the PipeWire thread.

Graph and transport messages are complete current-state snapshots, so superseding an unread older snapshot is safe. User-visible errors and command completions are not latest-value state: store them in a small ordered queue with explicit overflow reporting. Do not silently drop them.

The current backend already limits both dirty graph snapshots and peak harvesting to 25 Hz. Keep capture at that cadence initially. The view can render at the display frame rate using elapsed monotonic time for the existing 24 dB/s decay, 1.5 s hold, and 2 s clip behavior. When hidden, stop frame callbacks; the latest peak frame may continue to replace itself with constant memory.

### Reconnect and stale-ID semantics

Every successful PipeWire session gets a monotonically increasing `ConnectionEpoch`. Every graph gets a `GraphRevision`. UI intents capture the epoch/revision they were based on. The executor rejects commands from an old epoch rather than applying an object ID to a new PipeWire session.

On disconnect:

- publish `ConnectionState::Disconnected` with a sanitized error and next retry state;
- clear the graph and meter view;
- disable object-ID actions;
- finish any recorder shutdown the backend already performs;
- do not queue routing/fader commands for replay, because IDs are session-local; and
- retain only stable user configuration (bus layout, hidden stable keys, row sizes, split position).

On reconnect:

- increment the epoch;
- publish a fresh complete graph;
- reapply only validated stable configuration (`SetBus` and transport options), never old node/port/link IDs;
- rebuild mixer/patchbay projections; and
- clear transient selection, drag, undo transactions that reference the old epoch.

This makes the two-second retry loop observable and prevents a subtle class of stale-action bugs.

## API sketches

These sketches define ownership and responsibility. They are not compiled code and exact names may change.

```rust
// pecan-core/src/backend.rs

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionEpoch(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphRevision(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestId(pub u64);

#[derive(Clone, Debug)]
pub struct Stamped<T> {
    pub epoch: ConnectionEpoch,
    pub revision: GraphRevision,
    pub value: T,
}

#[derive(Clone, Debug)]
pub enum BackendCommand {
    Connect { request: RequestId, epoch: ConnectionEpoch, from: PortId, to: PortId },
    Disconnect { request: RequestId, epoch: ConnectionEpoch, links: Vec<LinkId> },
    SetTarget { request: RequestId, epoch: ConnectionEpoch, node: NodeId, target: Option<String> },
    SetVolume { request: RequestId, epoch: ConnectionEpoch, node: NodeId, linear: Vec<f32> },
    SetMute { request: RequestId, epoch: ConnectionEpoch, node: NodeId, muted: bool },
    ConfigureBus(BusConfigForBackend),
    ConfigureTransport(RecordingOptions),
    StartAll { request: RequestId },
    StartBus { request: RequestId, bus: BusRecording },
    StopAll { request: RequestId },
    StopBus { request: RequestId, name: String },
    Shutdown { request: RequestId },
}

#[derive(Clone, Debug)]
pub enum BackendEvent {
    Connection(ConnectionState),
    Graph(Stamped<std::sync::Arc<GraphSnapshot>>),
    Peaks(Stamped<std::sync::Arc<PeakFrame>>),
    Transport(Stamped<std::sync::Arc<TransportState>>),
    CommandFinished { request: RequestId, result: Result<(), BackendError> },
    Stopped { result: Result<(), BackendError> },
    #[cfg(feature = "midi-experimental")]
    Midi(Stamped<std::sync::Arc<MidiActivity>>),
}

pub trait Backend: Send + Sync {
    fn current_graph(&self) -> std::sync::Arc<GraphSnapshot>;
    fn dispatch(&self, command: BackendCommand) -> Result<(), DispatchError>;
}
```

Do not expose GTK, GLib, Tauri, PipeWire proxy objects, borrowed callbacks, or serialized command names through this API. `GraphSnapshot` can retain `serde` derives for fixtures, settings migration, and the temporary Tauri adapter.

```rust
// pecan-core/src/controller.rs

pub enum Intent {
    ToggleRoute { source: StripKey, destination: StripKey },
    SetFader { strip: StripKey, db: f32, phase: GesturePhase },
    SetMute { strip: StripKey, muted: bool },
    ConnectPorts { from: PortKey, to: PortKey },
    DisconnectConnection(ConnectionKey),
    ChangeBusLayout(BusLayoutChange),
    ArmBus { bus: BusKey, additive: bool },
    Record,
    Stop,
    Undo,
    Redo,
    SaveSettings,
    Quit,
}

pub enum Effect {
    Backend(BackendCommand),
    PersistSettings(SettingsV1),
    OpenRecordingFolder(Option<std::path::PathBuf>),
    RequestQuitAfterBackendStops,
}

pub fn reduce(state: &mut AppState, input: Input) -> Vec<Effect>;
```

GTK callbacks translate widget actions to `Intent`. They do not decide route semantics, mutate PipeWire, perform file I/O, or sleep. The reducer and effect planner can therefore be tested without GTK, a display server, PipeWire, or Node.

Undo/redo should replace the current arbitrary 250 ms delay with an explicit state machine: submit one effect group, wait for its command result plus a graph revision at or after that result, then allow the next group. History stores stable port pairs and intended values, not link IDs that change when recreated.

## Target module and file boundaries

```text
Cargo.toml                         # workspace; resolver/MSRV/profile only
crates/
  pecan-core/
    Cargo.toml                     # no GTK, Tauri or PipeWire
    src/
      lib.rs
      graph.rs                     # Node/Port/Link/GraphSnapshot IDs and DTOs
      backend.rs                   # commands, events, errors, epochs/revisions
      inbox.rs                     # latest-value slots + ordered notices
      controller.rs                # AppState, Intent, Effect, reducer
      projection.rs                # port of indeling.ts
      mixer.rs                     # port of mixer.ts, fader law
      routing.rs                   # route plans/back-to-auto/action resolution
      buses.rs                     # bus layout/arming/remap plans
      history.rs                   # serialized undo/redo transactions
      meters.rs                    # dB mapping/decay/hold/clip model
      settings.rs                  # SettingsV1, validation, atomic persistence
    tests/
      fixtures.rs
      routing.rs
      buses.rs
      reconnect.rs
      settings.rs

  pecan-pipewire/
    Cargo.toml                     # depends on pecan-core + pipewire/hound/midly/libc
    src/
      lib.rs                       # spawn() -> BackendHandle
      worker.rs                    # session/reconnect/shutdown lifecycle
      graph.rs                     # current graaf.rs graph mirror and commands
      meter.rs                     # current passive meter implementation
      recording.rs                 # current opname.rs DSP/writers/clock
      midi_meter.rs                # current midimeter.rs; feature remains off

  pecan-ui-gtk/
    Cargo.toml                     # depends on core, pipewire, gtk4, ksni
    src/
      main.rs                      # GtkApplication lifecycle
      app.rs                       # activation, window, actions, close policy
      controller.rs                # GTK binding to the pure reducer
      tray.rs                      # ksni adapter, no GTK objects off-thread
      autostart.rs                 # XDG adapter
      platform.rs                  # folder open/portal decisions
      views/
        shell.rs
        overview.rs
        mixer.rs
        patchbay.rs
        recording.rs
        settings.rs
      widgets/
        level_meter.rs
        patch_canvas.rs
    resources/
      style.css
      icons/
      ui/                           # optional GtkBuilder templates once stable

fixtures/
  graph/
  expected/

src-tauri/                          # temporary only
  src/
    lib.rs                          # Tauri commands/events over shared crates
    backend_adapter.rs
    tray.rs                         # remains Tauri-specific during coexistence
    autostart.rs
```

Avoid an early crate per screen. Three dependency boundaries are enough:

- `pecan-core`: deterministic and headless;
- `pecan-pipewire`: Linux audio/thread/FFI ownership; and
- `pecan-ui-gtk`: all GTK and desktop presentation.

## Feature parity map

| Capability | Reuse or port | GTK/native shape | Required proof before cutover |
|---|---|---|---|
| Process lifecycle / single instance | Replace Tauri plugin with `GtkApplication`/`GApplication` ID `nl.jetse.pecanmixer` | `activate` presents existing window; `hold` keeps process alive for tray/recording | Second launch activates one primary instance; hidden and visible cases |
| PipeWire graph / reconnect | Reuse current worker and two-second reconnect; add owned events, epoch and shutdown | Connection banner and disabled controls while disconnected | Isolated PipeWire restart test; no stale-ID action; bounded inbox |
| Mixer grouping | Port `indeling.ts` and `mixer.ts` into `pecan-core` | GTK list/box models; standard buttons/toggles/scales | Golden fixture parity for app/hardware/destination grouping and ordering |
| Route semantics | Port auto/manual/custom, `target.object`, extra link, and Back to auto planning | Toggle/action sends `Intent`; confirmation dialog for destructive reset | Exact expected command sequences, custom routes preserved, undo/redo |
| Fader and mute | Port dB law and per-channel replacement | `gtk::Scale` with keyboard/wheel/fine adjustment and native accessible value; `ToggleButton` for mute | Boundary values −90…+6 dB, channel isolation, gesture grouping, AT-SPI |
| Meters | Reuse Rust capture; port smoothing model | Latest-frame slot + tick-driven custom meter or `LevelBar`; one queued redraw | Decay/hold/clip unit tests; large graph backlog/frame-time test |
| Patchbay | Port deterministic grouping/layout/geometry/actions; do not port DOM hit testing | Native module/port widgets over an `Overlay`; `DrawingArea` draws decorative cables; gestures for pointer path | Connect/move/disconnect fixture tests; keyboard-accessible connection list and actions; Wayland/X11 pointer checks |
| Rec bus configuration | Port `bussen.ts` including legacy shape, split/join, arm, gain and ordered removal plan | Native bus cards and actions | Connect-new → disconnect-old → resize ordering; route preservation; mono/stereo gains |
| Recording | Reuse Rec filter, plan, WAV/MIDI writer code; MIDI still gated off | Native transport/status/error surface | Synthetic sample WAV tests; simultaneous alignment; dynamic arm/disarm; I/O errors |
| Graceful quit | Add worker `Shutdown` and explicit `Stopped` after all writers join | Disable UI, show finalizing state, quit only after ack or user-confirmed failure path | Slow writer fixture; valid WAV header after quit; no fixed sleep |
| Tray | Retain `ksni`, replace Tauri `AppHandle` with `AppAction` sender | Optional StatusNotifierItem adapter; full commands also in window | KDE and GNOME+extension manual matrix; safe no-host behavior |
| Close-to-tray | Final GTK shell hides on close only if tray registration is usable or user explicitly enabled background mode | `close-request` and application hold | No invisible, unreachable process when no tray host exists |
| Autostart | Reuse and harden current XDG code | Settings action writes desktop entry for final native executable | Temp-XDG tests, quoting, AppImage path, enable/disable |
| Settings | Port five localStorage domains to `SettingsV1` JSON | `$XDG_CONFIG_HOME/pecanmixer/settings.json`, atomic write; transient IDs excluded | Defaults, round trip, corrupt file, schema migration, failed write |
| Folder open | Keep path confinement; decide portal-first versus `xdg-open` fallback | Platform effect, not GTK callback | Confined path tests; sandbox/package manual test |
| MIDI | Keep backend and UI gates off | Disabled affordance with explanation or omit controls | Prove no MIDI filter/port/clock activation; hardware support is separate work |

### Patchbay accessibility rule

The cable canvas must never be the only way to understand or operate routing. Each port is a focusable native control with a stable accessible name. A parallel connections list exposes “source → destination,” selection and Disconnect/Move actions. Keyboard users can choose a source, choose a compatible destination, connect, enumerate current connections, and remove one without spatial hit testing. The drawn curve is presentation only.

### Tray rule

GTK 4 has no general tray widget. StatusNotifierItem is a separate D-Bus protocol and a host may be absent. Retain `ksni` behind a `TrayAdapter`, but never make the tray the only route to Quit, stop recording, autostart, or reopen the main window. If registration fails, close exits or minimizes according to an explicit visible setting; it must not hide an unreachable process.

## Settings design

Prefer a small schema-versioned JSON document over GSettings for 0.1.1. It reuses `serde_json`, needs no installed/compiled schema, works in development and AppImage layouts, and is straightforward to fixture-test.

```json
{
  "schema": 1,
  "buses": {
    "widths": [2],
    "state": { "rec-1-2": { "db": 0.0, "muted": false, "armed": true } },
    "midi_armed": false
  },
  "hidden": [],
  "row_weights": { "apps": 1.0, "hardware": 1.0, "destinations": 1.0 },
  "destination_split_px": null,
  "clock": { "start_stop": false }
}
```

Do not persist graph object IDs, selected destination IDs, current routes, running recording state, undo history, drag state, or clock source port ID. Patchbay positions are currently session-only; keeping them session-only is parity, not a regression.

The old WebKit localStorage location and portability were not audited. The implementation decision must be explicit:

- safest default: document that native 0.1.1 starts with defaults and leave old data untouched; or
- provide a one-time, separately reviewed importer only if a reliable storage location and schema can be proven across the supported WebKit/Tauri packages.

Do not silently guess paths or delete old data.

## Smallest first PR: live read-only connectivity slice

### Purpose

Prove the irreversible decisions early:

- the existing PipeWire worker can be Tauri-free;
- GTK remains responsive while graph traffic/reconnect lives off-thread;
- events are owned and bounded;
- current Tauri behavior still uses the same backend; and
- the native binary has no Tauri/WebKit/Node dependency path.

A fake-only GTK mock would be fewer lines but would not test the architectural seam. A full mixer slice would mix backend extraction, policy porting, styling, input, persistence and accessibility in one unreviewable PR. The connectivity slice is the smallest useful middle.

### Exact first-PR scope

In:

1. Root Cargo workspace; keep package/application version `0.1.0`.
2. `pecan-core` with existing graph/transport DTOs, typed IDs, epoch/revision, `BackendCommand`, `BackendEvent`, inbox, and read-only `OverviewModel`.
3. `pecan-pipewire` with the current graph, meter, recorder and disabled MIDI modules moved mechanically, then adapted to owned events and explicit worker lifecycle.
4. Existing Tauri app switched to path dependencies and a thin serialization adapter; current frontend API remains unchanged.
5. `pecan-ui-gtk` development binary with application ID `nl.jetse.pecanmixer.GtkPreview`, a connected/disconnected banner, rate/quantum labels, and node/port list.
6. `--fixture <path>` mode for the GTK preview and controller development, plus display-free tests that exercise the same reducer/projection.
7. Dependency assertions showing `pecan-ui-gtk` has no `tauri`, `wry`, WebKit, JavaScriptCore, or JS serialization crates.
8. Documentation that the preview is experimental, non-installed and not parity-complete.

Out:

- route changes, faders, patch editing, meters on screen, recording controls, tray, autostart, settings writes, old localStorage import, final styling, packaging, Tauri removal, JS deletion, release notes, tag/release work, and version bump;
- MIDI enablement; and
- libadwaita/Relm4/Lua.

### Proposed first-PR file changes

| Path | Change |
|---|---|
| `Cargo.toml` | Add workspace members and shared dependency versions; no package version |
| `crates/pecan-core/**` | Add DTO/backend/inbox/overview model plus unit tests |
| `crates/pecan-pipewire/**` | Move/adapt `graaf.rs`, `meter.rs`, `opname.rs`, `midimeter.rs` |
| `crates/pecan-ui-gtk/**` | Add small preview shell and read-only overview |
| `fixtures/graph/{disconnected,audio-basic,reconnect-after}.json` | Add sanitized committed inputs derived from the current browser fixture/source contract |
| `fixtures/expected/audio-basic-overview.json` | Add expected stable read-only projection |
| `src-tauri/Cargo.toml` | Replace direct backend deps with workspace path deps; retain Tauri dependencies |
| `src-tauri/src/lib.rs` | Keep command names/events; forward through shared backend |
| `src-tauri/src/backend_adapter.rs` | Own/serialize shared events for the existing frontend |
| `src-tauri/src/{graaf,meter,opname,midimeter}.rs` | Remove after their code is moved, not duplicated |
| `README.md` or a migration note | Explain preview build/run separately; do not claim no-JS completion |

`tray.rs` and `autostart.rs` remain in the Tauri shell for this PR because porting them does not help validate the PipeWire-to-GTK seam and would expand review risk.

### First-PR automated acceptance

All of these tests are designed to be display-free and audio-server-free unless explicitly marked otherwise:

1. `pecan-core` compiles without GTK, Tauri or PipeWire dependencies.
2. Every current graph/transport field round-trips through the shared DTO fixture format.
3. The `audio-basic` fixture projects a deterministic ordered node/port overview matching the committed expectation.
4. Publishing 10,000 graph snapshots leaves one latest graph; publishing 10,000 meter frames leaves one latest frame; one unread wake token remains; memory/backlog is bounded.
5. Low-rate notices preserve order and an overflow becomes a visible `InboxOverflow` error rather than silent loss.
6. A disconnect/reconnect increments epoch; commands stamped with the old epoch are rejected before reaching the fake backend.
7. The fake backend proves the GTK controller/reducer path without importing or initializing GTK.
8. The temporary Tauri adapter maps all 13 existing commands and all four existing event names without changing their serialized payload shape.
9. `cargo tree -p pecan-ui-gtk` contains GTK 4 but none of `tauri`, `tauri-runtime-wry`, `wry`, `webkit2gtk`, `javascriptcore-rs`, or `serialize-to-javascript`.
10. A static repository check confirms versions remain `0.1.0`.

Future CI commands should use locked dependencies and a job with no Node on `PATH` for the native packages. They were not run during this research.

### First-PR manual acceptance (future implementation only)

1. Run the GTK preview against a fixture on Wayland and X11; verify window activation, keyboard navigation, resizing at 900×600 and 1400×900, and accessible names.
2. Run live against PipeWire; verify connected state, rate/quantum, and node/port changes.
3. Restart PipeWire; verify disconnected state, no frozen UI or growing backlog, and a rebuilt graph after reconnect.
4. Launch the current Tauri application from the same extracted backend and smoke-check its existing read-only graph display.

No audio mutation or recording is part of the first PR manual test.

## Headless fixture strategy for later parity

The browser's current static `nepGraaf()` is useful source material, but do not keep a JavaScript oracle forever. Commit sanitized, hand-reviewed JSON inputs and expected Rust outputs.

| Fixture | Purpose | Expected assertions |
|---|---|---|
| `audio-basic.json` | Apps, stereo hardware, HDMI, Rec bus, standard routes | grouping/order, labels, channel pairing, default destination |
| `audio-custom-route.json` | partial/mismatched link plus manual and WirePlumber links | `custom` explanation, preservation, exact Back-to-auto plan |
| `rec-bus-shift.json` | several stereo/mono buses with routes above removed bus | new links first, old links removed second, bus resize last; gains/arming retained |
| `disconnected.json` | empty disconnected snapshot | disabled intents, no stale view state |
| `reconnect-before.json` / `reconnect-after.json` | same logical devices with different object IDs | stable preference retention, selection/history invalidation, old-epoch rejection |
| `transport-recording.json` | multiple active buses and error state | elapsed/status projection and tray state |

Tests by layer:

- `pecan-core`: projection, routing, bus layout, fader law, meter math, settings, reducer, history and reconnect epochs.
- `pecan-pipewire` with fake ports/commands: command validation, owned event conversion, graceful shutdown state machine. An isolated PipeWire/WirePlumber harness comes later and is not a substitute for these fast tests.
- `pecan-ui-gtk` controller library: widget-independent bindings from `AppState` to view properties. GTK accessibility tests may additionally use GTK's `test` accessibility backend when native GTK is installed in CI.
- recorder: deterministic sample blocks into writer functions, then inspect WAV channel/rate/format/frame count/finalized header and errors.

## Staged coexistence and cutover

### Stage 1 — backend seam and GTK connectivity slice

The first PR above. The React/Tauri app remains the supported product. The GTK binary has a distinct application ID and is not packaged. There is one Rust backend implementation, used by two shells.

### Stage 2 — pure Rust domain policy

Port `indeling.ts`, `mixer.ts`, `bussen.ts`, `handelingen.ts`, `geschiedenis.ts`, meter math, settings schema and deterministic patch geometry into `pecan-core`. Use frozen fixtures to compare behavior. Temporary TypeScript/Rust duplication is explicit and short-lived; do not add a new Rust-to-React IPC model solely to avoid it, because that bridge would immediately be deleted.

During duplication:

- TypeScript remains the production oracle;
- Rust modules are used by GTK and tested against fixtures;
- behavior fixes change the fixtures first and are applied to both implementations; and
- no feature is declared ported until its fixture/acceptance row is green.

### Stage 3 — GTK mixer

Port native strips, destinations, faders, mute, edit target, manual/custom status, hide/row resizing, split pane, Back to auto and history. Standard controls get keyboard and accessibility behavior by construction; custom interaction still needs explicit tests.

### Stage 4 — GTK patchbay and meters

Add native module/port widgets, decorative cable canvas, pointer gestures, full non-spatial connection UI, coalesced level meters, rearrange/session positions, stress and accessibility tests.

### Stage 5 — recording, tray, settings and lifecycle

Add Rec bus editing/arming, transport, per-bus/all recording, explicit writer shutdown, folder opening, schema-versioned settings, single-instance behavior, close/background policy, `ksni` tray adapter and autostart. MIDI stays disabled.

### Stage 6 — cutover and removal

Make GTK the only product binary only after the parity matrix passes. Then remove the old shell and prove the final dependency graph/runtime.

## Final JS-free removal and acceptance checklist

This checklist is intentionally later than the first PR.

### Remove web/Tauri source and build inputs

- [ ] Remove `src/**/*.ts`, `src/**/*.tsx`, and `src/vite-env.d.ts` after every supported behavior has a Rust replacement or documented retirement.
- [ ] Remove `index.html`, `src/styles.css` after native resources/styles replace them.
- [ ] Remove `package.json`, `package-lock.json`, Vite config, TypeScript configs, npm scripts and Node documentation.
- [ ] Remove Tauri config, capability files, `build.rs`, command attributes, event serialization adapter, Tauri single-instance plugin and all Tauri crates.
- [ ] Convert or remove executable script/design code in `docs/ontwerp/project/*.dc.html`; screenshots and non-executable design notes may remain.
- [ ] Remove stale `node_modules`/`dist` ignores and generated artifacts from documented workflows.

### Prove repository and dependency state

- [ ] No first-party `.js`, `.jsx`, `.ts`, `.tsx`, `.mjs`, `.cjs`, executable HTML script, package manager manifest/lock, Node invocation, or generated web bundle.
- [ ] `cargo metadata`/`cargo tree` contains no `tauri*`, `wry`, `webkit2gtk*`, `javascriptcore*`, or `serialize-to-javascript`.
- [ ] Distribution/binary inspection shows no WebKit or JavaScriptCore dynamic dependency. GTK 4 is expected; accidental GTK 3 is not.
- [ ] `cargo test --workspace --locked` and release build succeed with no Node/npm on `PATH`.
- [ ] A clean checkout can build and run the native app with documented Rust/C/pkg-config/GTK/PipeWire requirements.

### Prove supported behavior

- [ ] Mixer auto/manual/custom routes, Back to auto, faders, mute, hidden/resized layout and undo/redo.
- [ ] Patchbay connect/move/disconnect/rearrange, mini meters, pointer and keyboard/non-visual workflows.
- [ ] Rec bus add/remove/split/join/arm, route migration, simultaneous/dynamic recording, WAV finalization and folder action.
- [ ] Tray show/hide/status/stop/quit/autostart, plus safe behavior when no tray host exists.
- [ ] Disconnect/reconnect with bounded events and stale-ID protection.
- [ ] Settings defaults, persistence, migration/corruption handling and transient-ID exclusion.
- [ ] MIDI remains visibly unsupported and inactive.
- [ ] AT-SPI/Orca names, roles, values, focus order, status/error announcements and keyboard-only operation.
- [ ] Wayland/X11, high DPI, 900×600 and 1400×900, light/dark/high-contrast, large graph and peak-rate stress.

### License and release gate

- [ ] Keep the combined application GPL-3.0-or-later unless maintainers deliberately decide otherwise.
- [ ] Audit the final Cargo lock and native libraries; generate notices/source-compliance material for distributed GTK/other LGPL components and retained fonts/assets.
- [ ] If IBM Plex Mono is retained, bundle only needed font files with OFL text/attribution; otherwise use a system monospace family.
- [ ] Update README/build/usage/release notes for the native architecture.
- [ ] Only now update every surviving application/version source to **0.1.1** in the same merge-ready change.
- [ ] Do not create a tag, publish artifacts, or release automatically.

## Accessibility, platform and packaging position

### Accessibility

GTK exposes platform accessibility through `GtkAccessible`; on Linux/BSD that means AT-SPI. Standard GTK controls should be preferred for all semantic actions. Custom widgets need explicit roles, properties, state and keyboard behavior; assigning a role does not supply the promised interaction automatically.

Minimum acceptance:

- named, focusable faders with current dB value and keyboard increments;
- named mute/route/record/arm toggles with state;
- visible focus in all supported themes;
- text/status equivalents for color and meter clip state;
- patch connections operable outside the spatial canvas;
- errors and recording state presented in a screen-reader-observable surface without announcing 25 Hz meter noise; and
- GTK `test` accessibility backend in CI plus manual Orca/AT-SPI validation.

### Platform support

The supported product remains **Linux with PipeWire and WirePlumber**, on Wayland and X11. GTK's other platform backends are not an application support promise. Windows/macOS lack the current audio, tray, autostart and packaging implementation; they are out of scope unless a separate portability project replaces those layers.

Select the GTK API feature floor from supported distributions before implementation. Higher gtk-rs version features reduce system availability. Do not infer that an AppImage makes every GTK plugin/theme/portal interaction portable; test the actual bundle on the distro matrix.

### License/dependency cost

- The project is GPL-3.0-or-later.
- GTK is LGPL-2.1-or-later; `gtk4-rs` bindings are MIT. Dynamic/system use is compatible with the project, but binary distribution still needs applicable LGPL/source/notice compliance.
- Libadwaita is LGPL-2.1-or-later; its Rust binding adds `libadwaita-sys` and the matching GTK/GLib Rust generation, while the native build uses the `libadwaita-1` pkg-config package.
- Direct GTK already brings the native GTK/GLib/GIO/GDK/GSK/Cairo/Pango/Graphene stack. “No JavaScript” is not “no native dependencies.”
- Retained `ksni`, PipeWire, hound, midly, libc, assets and any new async-channel crate remain in the final audit.

This report is technical planning, not legal advice.

## Lua decision

Omit Lua from issue 1 and 0.1.1. There is no current scripting feature to preserve, and the migration already needs one new UI framework, a domain port, accessibility work and a backend lifecycle seam.

Only reconsider Lua for a separately requested, user-facing benefit such as reusable routing macros or bus templates. If added later:

- run it off the real-time and GTK threads;
- expose immutable sanitized snapshots and declarative proposed actions;
- validate all IDs, ranges, conflicts and permissions in Rust;
- require confirmation for destructive routing plans;
- impose time/instruction and memory limits; and
- expose no filesystem, process, network, FFI, raw PipeWire object or ambient host authority by default.

Lua must never own DSP, recorder finalization, PipeWire callbacks, persistence validation, or GTK objects.

## Risk register

| Risk | Level | Mitigation / gate |
|---|---|---|
| Temporary TS/Rust policy divergence | High | Freeze fixtures first; short coexistence; mirrored behavior changes; per-slice parity gate |
| Stale PipeWire object IDs after reconnect | High | Epoch/revision stamp every intent/event; reject old epoch; clear transient UI/history |
| Recorder corruption on quit | High | Explicit Shutdown/Stopped protocol; stop takes and join every writer; slow-writer test |
| Patchbay becomes pointer-only/inaccessible again | High | Native port controls plus connections list/action surface; keyboard/AT-SPI acceptance before merge |
| High-rate updates block worker or grow memory | High | Latest-value slots, capacity-one wake, no GTK on worker, stress/backlog assertions |
| GTK/libadwaita version excludes supported distro | High until matrix chosen | Set distro/architecture matrix first; use lowest necessary GTK feature floor; defer Adwaita |
| Tray host absent leaves invisible process | High | Tray registration is optional; never hide-on-close without a reachable reopen/quit path |
| AppImage GTK/theme/plugin/portal behavior differs | Medium–high | Package smoke matrix; inspect runtime libraries/resources; keep source build contract separate |
| Settings lost at cutover | Medium | Decide reset versus proven importer; preserve old storage; document; test schema/atomic writes |
| GTK callback panic aborts process | Medium | No unchecked UI callback paths; convert failures to state/notices; targeted malformed fixture tests |
| Custom CSS loses contrast/theme behavior | Medium | Use standard widgets/style classes and system colors; light/dark/high-contrast tests |
| Tauri adapter extraction accidentally changes current app | Medium | Preserve serialized API; adapter contract tests; manual regression smoke before merge |
| Folder/autostart behavior under sandbox differs | Medium | Portal decision, confined paths, temp-XDG tests, package-specific manual checks |

## Decisions needed before implementation

1. Supported Linux distributions, architectures, and minimum GTK/PipeWire/WirePlumber versions.
2. Whether native 0.1.1 resets WebKit-local settings or attempts a separately audited one-time import.
3. System monospace font versus vendored IBM Plex Mono with OFL materials.
4. Exact non-spatial patchbay interaction and how it is presented in the main UI.
5. Portal-first folder opening versus `xdg-open` fallback order.
6. Whether visual pixel similarity or native desktop semantics takes priority where they conflict.

The first PR does not require decisions 2–6; it requires only the GTK/system version floor and review of the proposed crate boundary.

## Explicitly unverified runtime claims

No runtime claim in this section was verified during this research:

- the proposed crates/API sketches have not been compiled;
- no GTK/libadwaita/PipeWire development package availability was checked on target distributions;
- no current or proposed UI was launched on Wayland/X11;
- current README claims for audio routing, reconnect, meters, recording alignment, tray behavior and settings were not exercised;
- the proposed channel capacity, redraw cadence, CPU usage and memory behavior were not benchmarked;
- no real PipeWire/WirePlumber restart, device graph, recording, slow filesystem, screen reader, tray host, AppImage, portal or autostart environment was tested;
- libadwaita appearance across non-GNOME desktops was not tested; and
- no final license scanner or binary dependency inspection was run.

The code facts, dependency facts and documentation comparisons are high confidence; performance, package availability, desktop integration and behavioral parity remain implementation-time tests.

## Primary references

Repository evidence is pinned to `d8d473818331342ab53a0e1e4d3ced7977293449`.

- [Issue 1 goal, scope and acceptance](https://github.com/jee-mj/pecanmixer/issues/1)
- [Current Rust/Tauri shell](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/lib.rs)
- [Current PipeWire graph worker](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/graaf.rs)
- [Current passive meter backend](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/meter.rs)
- [Current recording implementation](https://github.com/jee-mj/pecanmixer/blob/d8d473818331342ab53a0e1e4d3ced7977293449/src-tauri/src/opname.rs)
- [gtk4-rs crate, threading, version features and MIT license](https://docs.rs/crate/gtk4/latest)
- [gtk4-rs main event loop guidance](https://gtk-rs.org/gtk4-rs/git/book/main_event_loop.html)
- [gtk-rs removal of GLib channels and async-channel guidance](https://gtk-rs.org/blog/2024/06/01/new-release.html)
- [GTK frame tick callback](https://docs.gtk.org/gtk4/method.Widget.add_tick_callback.html)
- [GTK accessibility architecture and authoring guidance](https://docs.gtk.org/gtk4/section-accessibility.html)
- [GTK runtime accessibility test backend](https://docs.gtk.org/gtk4/running.html#gtk_a11y)
- [GApplication lifecycle, single-instance and exported actions](https://docs.gtk.org/gio/class.Application.html)
- [Libadwaita overview, dependency and LGPL license](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/)
- [Libadwaita adaptive layouts](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.5/adaptive-layouts.html)
- [Libadwaita styles, dark preference and high contrast](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.9/styles-and-appearance.html)
- [Libadwaita build dependency `libadwaita-1`](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.7/build-howto.html)
- [gtk-rs libadwaita integration/version alignment](https://gtk-rs.org/gtk4-rs/git/book/libadwaita.html)
- [StatusNotifierItem D-Bus protocol](https://specifications.freedesktop.org/status-notifier-item/latest-single/)
- [XDG base directories](https://specifications.freedesktop.org/basedir-spec/latest/)
- [XDG desktop autostart](https://specifications.freedesktop.org/autostart-spec/latest/)
