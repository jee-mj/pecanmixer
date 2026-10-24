# Functional native parity matrix

This is a source-level implementation matrix, not a hardware/desktop acceptance
report. The full functional JS port remains the goal. The legacy React/Tauri
application is retained; the current native executable is still observational.
"Candidate" means independently reviewable source exists, not live adoption.

| Original behavior and source | Rust/native state | Remaining gate |
|---|---|---|
| Graph subscription, disconnect/reconnect, rate/quantum (`src/api.ts`, `src/App.tsx`) | Shared backend + exact-frame owner/coordinator candidate | Independent coordinator/shell review; production startup adoption |
| Apps/hardware/destination grouping, numbered channel pairs, stable ordering (`src/mixer.ts`, `src/indeling.ts`) | Initial pure audio projection and reviewed fixtures | Complete locale/corner-case parity; bind domain strips into production shell |
| Per-strip gain, preserve unselected channels (`stripDb`, `metDb`, `Kaart` in `src/Mixer.tsx`) | Reviewed controller/GTK library; shared receipt/history coordinator candidate | Independent coordinator/shell review and production adoption |
| Whole-node mute (`Kaart`, `muteStap`) | Tested controller/GTK candidate | Same production/history gates as gain |
| Partial-pair mute and restore prior gain (`voorMute`, `Kaart`) | Unimplemented in native controls | Port gain-memory lifecycle, external edits, zero/unmute and history semantics |
| Drag/fine drag, wheel grouping, rounding, double-click reset (`Fader`) | Fader curve is pure Rust; candidate safely captures/cancels input origins | Continuous gesture coalescing, one history step, exact rounding/reset and physical input acceptance |
| Independent output destination and Rec-bus selections (`bewerkt`, `bewerkteBus`, `kiesBewerkt` in `src/App.tsx`) | Unimplemented native selection state | Keep both choices independent through reconnect/layout changes |
| Normal/manual/custom route states and green/R toggles (`BronStrip`, `schakel`, `schakelRoute`) | Pure classification/plans and fixtures | Native selectors/buttons, same-owner intents, receipt/history integration |
| Back to auto / manual route count (`telHandwerk`, `terugNaarAutoStap`) | Pure reset/count plans | Native action, observed completion, serialized history |
| Cable create/remove/move, selection/delete/cancel (`src/Patchbay.tsx`, `src/handelingen.ts`) | Reviewed keyboard/pointer libraries attached to coordinator candidate | Independent application review, production adoption and physical-input acceptance |
| Patch module positioning and reconnect behavior (`Plekken`, `ModuleKaart`) | Native observation geometry only | Mutable positions, drag/selection behavior, state persistence policy |
| Undo/redo queue and keyboard shortcuts (`src/geschiedenis.ts`, `src/handelingen.ts`, `src/App.tsx`) | Generation/selector-guarded reducer, shared receipts and immutable history-button context candidate | Independent application review and production adoption; group gestures and implement shortcuts |
| Add/remove/link buses, Rec gain/mute/arming (`src/bussen.ts`, `Mixer`) | Pure bus layout/state policy | Native bus UI and deliberate command contract; dynamic arming while recording |
| Record/stop, per-bus start/stop, elapsed time, folder action (`src/Transport.tsx`, `src/api.ts`) | Existing backend preserved; lifecycle completion integrated for legacy tray | Native recording command admission/completion, controls and folder action; recordings not supported by current native intent API |
| MIDI clock/start-stop, MIDI tabs/route/activity (`src/MidiMixer.tsx`, `src/midi.ts`, `src/MidiMeter.tsx`) | Legacy MIDI feature flag remains disabled | Preserve disabled status; separately define/verify any enabled MIDI parity scope |
| Meter ballistics/peak hold, per-channel rendering (`src/meters.ts`, `src/Meter.tsx`) | Reviewed isolated meter widget source, not mounted/live | Main-context feed, channel mapping, availability/generation protocol, animation acceptance and production ownership |
| Hide/show strips/rows and reconnect names (`src/App.tsx`, `Mixer`) | Validated contract + display-only filtering candidate; full graph retained | Native hide/show menus and production application; transient IDs must not become durable identity |
| Row height and destination split preferences (`useRijGewichten`, `Greep`, `Deling`) | Saved-weight/width presentation + explicit serialized worker candidate | Native resizing/reset controls, startup adoption and save-trigger wiring |
| Saved buses, hidden keys, clock preference and layout (`leesBussen`, `leesKlok`, `leesVerborgen`) | Versioned contract + storage and serialized off-UI worker candidate | Native shell effects, user-selected migration/recovery and startup adoption; no profile access performed |
| Tray open/close/quit, autostart/single instance (`src-tauri/src/tray.rs`, `autostart.rs`, `lib.rs`) | Legacy preserved; reviewed lifetime/tray libraries and finalization-aware shell candidate | Independent shell review, GApplication/CLI adoption, autostart, process arbitration and actual desktop acceptance |
| Graceful recorder shutdown/error/timeout handling | Backend/legacy completion and native hold/tagged-handle/off-UI settings-drain candidate | Independent combined shell review and startup adoption; no fsync/crash-durability claim |
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
