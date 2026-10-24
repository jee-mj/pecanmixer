# Functional native parity matrix

This is a source-level implementation matrix, not a hardware/desktop acceptance
report. The full functional JS port remains the goal. The legacy React/Tauri
application is retained; the current native executable is still observational.
"Candidate" means independently reviewable source exists, not live adoption.

| Original behavior and source | Rust/native state | Remaining gate |
|---|---|---|
| Graph subscription, disconnect/reconnect, rate/quantum (`src/api.ts`, `src/App.tsx`) | Reviewed exact-frame owner/coordinator/finalization shell source | Production startup adoption; telemetry and desktop acceptance |
| Apps/hardware/destination grouping, numbered channel pairs, stable ordering (`src/mixer.ts`, `src/indeling.ts`) | Initial pure audio projection and reviewed fixtures | Complete locale/corner-case parity; bind domain strips into production shell |
| Per-strip gain, preserve unselected channels (`stripDb`, `metDb`, `Kaart` in `src/Mixer.tsx`) | Owner-coordinated continuous gain candidate with one-flight/final-observation history | Independent review, production adoption and audio/input acceptance |
| Whole-node mute (`Kaart`, `muteStap`) | Tested controller/GTK candidate | Same production/history gates as gain |
| Partial-pair mute and restore prior gain (`voorMute`, `Kaart`) | Owner/GTK partial-mute candidate with registry identity, observed memory commit and full-vector undo | Independent review, production adoption and audio/input acceptance |
| Drag/fine drag, wheel grouping, rounding, double-click reset (`Fader`) | Pure oracle policy plus owner-coordinated GTK gesture candidate | Independent review, production adoption and physical input acceptance |
| Independent output destination and Rec-bus selections (`bewerkt`, `bewerkteBus`, `kiesBewerkt` in `src/App.tsx`) | Unimplemented native selection state | Keep both choices independent through reconnect/layout changes |
| Normal/manual/custom route states and green/R toggles (`BronStrip`, `schakel`, `schakelRoute`) | Pure classification/plans and fixtures | Native selectors/buttons, same-owner intents, receipt/history integration |
| Back to auto / manual route count (`telHandwerk`, `terugNaarAutoStap`) | Pure reset/count plans | Native action, observed completion, serialized history |
| Cable create/remove/move, selection/delete/cancel (`src/Patchbay.tsx`, `src/handelingen.ts`) | Reviewed keyboard/pointer libraries attached to reviewed coordinator source | Production adoption and physical-input acceptance |
| Patch module positioning and reconnect behavior (`Plekken`, `ModuleKaart`) | Reviewed session movement/rearrange and reconnect aliases on retained view | Durable positions, production tabs and physical-input acceptance |
| Undo/redo queue and keyboard shortcuts (`src/geschiedenis.ts`, `src/handelingen.ts`, `src/App.tsx`) | Reviewed generation/selector guards, shared receipts and immutable history activations | Production adoption; continuous gesture candidate review, queued input and complete shortcuts |
| Add/remove/link buses, Rec gain/mute/arming (`src/bussen.ts`, `Mixer`) | Pure bus policy plus reviewed local idle config/readback and partial-resize contract | Native editor/adoption, observed ports, dynamic recording-time arming and staged migration |
| Record/stop, per-bus start/stop, elapsed time, folder action (`src/Transport.tsx`, `src/api.ts`) | Legacy preserved; reviewed tracked writer readiness/finalization and bounded sender source | Capture attachment, native recording/configuration commands, controls/alignment and folder action |
| MIDI clock/start-stop, MIDI tabs/route/activity (`src/MidiMixer.tsx`, `src/midi.ts`, `src/MidiMeter.tsx`) | Legacy MIDI feature flag remains disabled | Preserve disabled status; separately define/verify any enabled MIDI parity scope |
| Meter ballistics/peak hold, per-channel rendering (`src/meters.ts`, `src/Meter.tsx`) | Reviewed isolated meter widget source, not mounted/live | Main-context feed, channel mapping, availability/generation protocol, animation acceptance and production ownership |
| Hide/show strips/rows and reconnect names (`src/App.tsx`, `Mixer`) | Versioned contract, filtering and shell result/display bridge candidate; full graph retained | Independent bridge/prerequisite review and production application; transient IDs remain excluded |
| Row height and destination split preferences (`useRijGewichten`, `Greep`, `Deling`) | Shell bridge candidate with pointer resize/reset save triggers and final dirty-document drain | Independent bridge/prerequisite review, startup adoption and physical-input acceptance |
| Saved buses, hidden keys, clock preference and layout (`leesBussen`, `leesKlok`, `leesVerborgen`) | Versioned storage/worker plus result-aware display shell bridge candidate | Audio settings adapters, explicit migration and startup adoption; no profile access performed |
| Tray open/close/quit, autostart/single instance (`src-tauri/src/tray.rs`, `autostart.rs`, `lib.rs`) | Legacy preserved; reviewed lifetime/tray/finalization shell source | GApplication/CLI adoption, autostart, process arbitration and actual desktop acceptance |
| Graceful recorder shutdown/error/timeout handling | Reviewed backend/legacy completion and native hold/tagged-handle/off-UI settings drain | Startup adoption and recording integration; no fsync/crash-durability claim |
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

The [finalization-aware shell candidate](./native-finalization-shell.md) adds one retained owner/window/clock, shared admission shutdown and real completion/settings-drain wiring on the separate candidate branch. New application logic remains unreviewed and is not installed by main startup.

The [held history activation correction](./native-held-history-activation.md) prevents a held/retired Undo or Redo callback from adopting a newly exposed entry; native-input and same-graph before/after regressions cover the corrected candidate. Independent owner re-review remains a gate.

The [reviewed source integration batch](./reviewed-native-source-batch.md) supersedes earlier pending-review markers for its listed slices. Main startup remains observational, and pure fader/mute proposals are not yet live controller behavior.

The [continuous gain/partial mute candidate](./native-gesture-coordinator.md) composes the pure policies with owner identity, one-flight samples, observed memory and one grouped history step. It remains independently unreviewed source; executable startup, native recording and acceptance status are unchanged.

The [settings result/display bridge candidate](./native-settings-bridge.md) retains local/disk revisions, wires display-only edits and admits the final dirty document before worker close. It is independently unreviewed and depends on the blocked continuous-gesture prerequisite; no executable or audio-settings adoption is implied.
