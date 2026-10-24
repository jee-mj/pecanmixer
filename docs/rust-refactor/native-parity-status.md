# Functional native parity matrix

This is a source-level implementation matrix, not a hardware/desktop acceptance
report. The full functional JS port remains the goal. The legacy React/Tauri
application is retained. Reviewed entrypoint adoption is public at `20e9f90c`. This branch adds
an independently unreviewed bus/configuration/recording owner adapter.
"Candidate" means independently reviewable source exists, not live adoption.

| Original behavior and source | Rust/native state | Remaining gate |
|---|---|---|
| Graph subscription, disconnect/reconnect, rate/quantum (`src/api.ts`, `src/App.tsx`) | Reviewed exact-frame owner/coordinator/finalization shell source | Bus-adapter integration review and live audio/desktop acceptance |
| Apps/hardware/destination grouping, numbered channel pairs, stable ordering (`src/mixer.ts`, `src/indeling.ts`) | Initial pure audio projection and reviewed fixtures | Complete locale/corner-case parity; entrypoint review and live acceptance |
| Per-strip gain, preserve unselected channels (`stripDb`, `metDb`, `Kaart` in `src/Mixer.tsx`) | Reviewed one-flight controller, latest desired target and observed final full-vector history | Bus-lease integration review and audio/input acceptance |
| Whole-node mute (`Kaart`, `muteStap`) | Reviewed immutable mute action/registry identity and canceled held-activation source | Bus-lease integration review |
| Partial-pair mute and restore prior gain (`voorMute`, `Kaart`) | Reviewed policy plus serialized receipt/graph memory commit and complete-vector undo | Bus-lease integration review and audio/input acceptance |
| Drag/fine drag, wheel grouping, rounding, double-click reset (`Fader`) | Reviewed policy and owner coordinator; ordinary refresh preserves desired keyboard position; unconfirmed held input requires release | Bus-lease integration review and physical input acceptance |
| Independent output destination and Rec-bus selections (`bewerkt`, `bewerkteBus`, `kiesBewerkt` in `src/App.tsx`) | Reviewed isolated bus model/widgets keep separate choices | Mount and reconcile through the single native owner; retain choices through reconnect/layout changes |
| Normal/manual/custom route states and green/R toggles (`BronStrip`, `schakel`, `schakelRoute`) | Pure classification/plans and fixtures | Native selectors/buttons, same-owner intents, receipt/history integration |
| Back to auto / manual route count (`telHandwerk`, `terugNaarAutoStap`) | Pure reset/count plans | Native action, observed completion, serialized history |
| Cable create/remove/move, selection/delete/cancel (`src/Patchbay.tsx`, `src/handelingen.ts`) | Reviewed keyboard/pointer libraries attached to reviewed coordinator source | Bus-adapter integration review and physical-input acceptance |
| Patch module positioning and reconnect behavior (`Plekken`, `ModuleKaart`) | Reviewed session movement/rearrange and reconnect aliases on retained view | Session-layout and physical-input acceptance |
| Undo/redo queue and keyboard shortcuts (`src/geschiedenis.ts`, `src/handelingen.ts`, `src/App.tsx`) | Reviewed generation/selector guards, shared receipts, immutable history activation and one-step observed gesture history | Bus-lease integration review, queued input and complete shortcuts |
| Add/remove/link buses, Rec gain/mute/arming (`src/bussen.ts`, `Mixer`) | Owner adapter candidate mounts typed idle config/readback and correlated bus controls | Independent adapter review; observed ports, editable bus gain/mute, main Rec-model/routing and saved config adoption, dynamic arming and migration |
| Record/stop, per-bus start/stop, elapsed time, folder action (`src/Transport.tsx`, `src/api.ts`) | Owner adapter candidate adds explicit-root all-armed/per-bus start, captured stable stop IDs, retained receipts and quit diagnostics | Independent adapter review; directory picker/folder action, elapsed/alignment, dynamic arming and live recording acceptance |
| MIDI clock/start-stop, MIDI tabs/route/activity (`src/MidiMixer.tsx`, `src/midi.ts`, `src/MidiMeter.tsx`) | Legacy MIDI feature flag remains disabled | Preserve disabled status; separately define/verify any enabled MIDI parity scope |
| Meter ballistics/peak hold, per-channel rendering (`src/meters.ts`, `src/Meter.tsx`) | Reviewed retained widget/accepted-batch adapter, exact graph before peaks and explicit authority/visibility resets | Candidate mounts the adapter with authenticated node/port witnesses and viewport gating; independent wiring review, capture freshness protocol and live/animation acceptance remain |
| Hide/show strips/rows and reconnect names (`src/App.tsx`, `Mixer`) | Reviewed display panel/filter/save bridge; full graph/origin/history retained | Bus-lease integration review; transient IDs must not become durable identity |
| Row height and destination split preferences (`useRijGewichten`, `Greep`, `Deling`) | Reviewed adjacent-pair/split resize/reset input and request-aware save triggers | Bus-lease integration review and physical input acceptance |
| Saved buses, hidden keys, clock preference and layout (`leesBussen`, `leesKlok`, `leesVerborgen`) | Versioned contract/storage plus display result bridge and final unsent-document drain | Explicit path/memory selection is implemented in the entrypoint candidate; migration, audio settings effects and startup review remain; no profile access performed |
| Tray open/close/quit, autostart/single instance (`src-tauri/src/tray.rs`, `autostart.rs`, `lib.rs`) | Legacy preserved; reviewed lifetime/tray/finalization shell source | Entrypoint review, autostart/session management, complete process arbitration and actual desktop acceptance |
| Graceful recorder shutdown/error/timeout handling | Reviewed backend/legacy completion and native hold/tagged-handle/off-UI settings drain | Entrypoint review and recording integration; no fsync/crash-durability claim |
| Accessible controls, keyboard navigation, responsive layout (`src/*.tsx`, `src/styles.css`) | Synthetic native names/focus/layout checks; control candidate | End-to-end keyboard/AT-SPI/Orca, physical Wayland/X11, scale/theme/distro acceptance |
| JS/Node/Tauri/WebKit removal and distribution | Deferred deliberately | All functional/acceptance gates, measured performance, packaging/license closure before cutover |

Candidate tests use hand-authored synthetic data. No actual device identifiers,
topology, audio captures, private runtime logs or user profile data belong in
this public matrix or its fixtures. Real audio correctness, live restart behavior,
real-time performance, memory/CPU measurements and physical desktop acceptance
remain distinct, unperformed gates.

The [display settings candidate](./native-display-settings.md) preserves graph/origin state and applies no audio settings. Its original base focus defect was separately corrected; the combined source reconciliation is cleared while production ownership, history and live-control gates remain.

The [combined native UI source batch](./native-ui-source-batch.md) records constructor reconciliation, the cleared source publication gate and the backend capture-identity correction now integrated in the identity follow-up.

The [identity follow-up](./native-identity-batch.md) closes retained history selector/generation and backend capture-retention source gates. It does not enable native ownership/undo/capture or complete audio/desktop acceptance.

The [finalization-aware shell](./native-finalization-shell.md) adds one retained owner/window/clock, shared admission shutdown and real completion/settings-drain wiring. Its reviewed source is retained; it is not installed by main startup.

The [held history activation correction](./native-held-history-activation.md) prevents a held/retired Undo or Redo callback from adopting a newly exposed entry. Independent owner re-review cleared its native-input and same-graph before/after regressions.

The [earlier reviewed source integration batch](./reviewed-native-source-batch.md) supersedes pending-review markers for its listed slices. Later reviewed startup adopts those controllers; the current bus adapter below has its own review gate.

The [continuous gain/partial mute candidate](./native-gesture-coordinator.md) composes the pure policies with owner identity, one-flight samples, observed memory and one grouped history step. Its original three input findings were independently reproduced and corrected. The corrected source is independently cleared; executable startup and acceptance status are unchanged.

The [gesture review correction candidate](./native-gesture-review-correction.md) addresses held-mute semantic substitution, unconfirmed held-key retries and lost keyboard accumulation. Independent re-review clears `5608d2f4`, including the former failures and fresh-input controls. The combined source retains its guards and the separate settings display hooks.

The [settings result/display bridge candidate](./native-settings-bridge.md) retains local/disk revisions, wires display-only edits and admits the final dirty document before worker close. Its settings-only delta is independently cleared. The [next combined source candidate](./native-parity-source-batch.md) reconciles it with the corrected gesture source and records the separate combined-tree gate. No executable or audio-settings adoption is implied.

The [native entrypoint adoption candidate](./native-startup-adoption.md) wires one owner/shell, explicit path or memory settings, functional mixer/patchbay and authenticated accepted-batch telemetry. Its entrypoint wiring has independently cleared and is public at `20e9f90c`. The separate bus/configuration/recording owner wiring below remains unreviewed.

The [bus/configuration/recording owner candidate](./native-bus-owner-adapter.md) adds a shared ticket/admission lease, exact edited revision handoffs, retained native receipts and common-quit diagnostic audit. It remains independently unreviewed. Active arming/structural edits and migration stay unavailable. Module positions match the original session-only behavior; durable persistence is not required for parity.
