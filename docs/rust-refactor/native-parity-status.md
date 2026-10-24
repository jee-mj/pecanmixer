# Functional native parity matrix

This is a source-level implementation matrix, not a hardware/desktop acceptance
report. The full functional JS port remains the goal. The legacy React/Tauri
application is retained; the current native executable is still observational.
"Candidate" means independently reviewable source exists, not live adoption.

| Original behavior and source | Rust/native state | Remaining gate |
|---|---|---|
| Graph subscription, disconnect/reconnect, rate/quantum (`src/api.ts`, `src/App.tsx`) | Shared backend + native read-only inbox/presentation | Adopt one production native owner; preserve exact owned origin frames |
| Apps/hardware/destination grouping, numbered channel pairs, stable ordering (`src/mixer.ts`, `src/indeling.ts`) | Initial pure audio projection and reviewed fixtures | Complete locale/corner-case parity; bind domain strips into production shell |
| Per-strip gain, preserve unselected channels (`stripDb`, `metDb`, `Kaart` in `src/Mixer.tsx`) | Tested controller/GTK candidate through exact native receipts | Independent UI review, production ownership and observed-result integration |
| Whole-node mute (`Kaart`, `muteStap`) | Tested controller/GTK candidate | Same production/history gates as gain |
| Partial-pair mute and restore prior gain (`voorMute`, `Kaart`) | Unimplemented in native controls | Port gain-memory lifecycle, external edits, zero/unmute and history semantics |
| Drag/fine drag, wheel grouping, rounding, double-click reset (`Fader`) | Fader curve is pure Rust; candidate safely captures/cancels input origins | Continuous gesture coalescing, one history step, exact rounding/reset and physical input acceptance |
| Independent output destination and Rec-bus selections (`bewerkt`, `bewerkteBus`, `kiesBewerkt` in `src/App.tsx`) | Unimplemented native selection state | Keep both choices independent through reconnect/layout changes |
| Normal/manual/custom route states and green/R toggles (`BronStrip`, `schakel`, `schakelRoute`) | Pure classification/plans and fixtures | Native selectors/buttons, same-owner intents, receipt/history integration |
| Back to auto / manual route count (`telHandwerk`, `terugNaarAutoStap`) | Pure reset/count plans | Native action, observed completion, serialized history |
| Cable create/remove/move, selection/delete/cancel (`src/Patchbay.tsx`, `src/handelingen.ts`) | Read-only native diagram/list; separate actions candidate | Shared ownership/history wiring, independent review and pointer editing parity |
| Patch module positioning and reconnect behavior (`Plekken`, `ModuleKaart`) | Native observation geometry only | Mutable positions, drag/selection behavior, state persistence policy |
| Undo/redo queue and keyboard shortcuts (`src/geschiedenis.ts`, `src/handelingen.ts`, `src/App.tsx`) | Pure reducer + native result bridge candidate | Wire one serialized controller to real receipts/graph barriers; deduplicate delayed results and group gestures |
| Add/remove/link buses, Rec gain/mute/arming (`src/bussen.ts`, `Mixer`) | Pure bus layout/state policy | Native bus UI and deliberate command contract; dynamic arming while recording |
| Record/stop, per-bus start/stop, elapsed time, folder action (`src/Transport.tsx`, `src/api.ts`) | Existing backend preserved; lifecycle completion integrated for legacy tray | Native recording command admission/completion, controls and folder action; recordings not supported by current native intent API |
| MIDI clock/start-stop, MIDI tabs/route/activity (`src/MidiMixer.tsx`, `src/midi.ts`, `src/MidiMeter.tsx`) | Legacy MIDI feature flag remains disabled | Preserve disabled status; separately define/verify any enabled MIDI parity scope |
| Meter ballistics/peak hold, per-channel rendering (`src/meters.ts`, `src/Meter.tsx`) | Reviewed isolated meter widget source, not mounted/live | Main-context feed, channel mapping, capture identity correction, animation acceptance and production ownership |
| Hide/show strips/rows and reconnect names (`src/App.tsx`, `Mixer`) | Validated contract + display-only filtering candidate; full graph retained | Native hide/show menus and production application; transient IDs must not become durable identity |
| Row height and destination split preferences (`useRijGewichten`, `Greep`, `Deling`) | Saved-weight/width presentation + explicit serialized worker candidate | Native resizing/reset controls, startup adoption and save-trigger wiring |
| Saved buses, hidden keys, clock preference and layout (`leesBussen`, `leesKlok`, `leesVerborgen`) | Versioned contract + storage and serialized off-UI worker candidate | Native shell effects, user-selected migration/recovery and startup adoption; no profile access performed |
| Tray open/close/quit, autostart/single instance (`src-tauri/src/tray.rs`, `autostart.rs`, `lib.rs`) | Legacy preserved; quit now waits for explicit backend/writer completion | Native desktop integration and actual tray/desktop acceptance |
| Graceful recorder shutdown/error/timeout handling | Backend completion + legacy tray tests | Adopt in native owner before exit; no fsync/crash-durability claim |
| Accessible controls, keyboard navigation, responsive layout (`src/*.tsx`, `src/styles.css`) | Synthetic native names/focus/layout checks; control candidate | End-to-end keyboard/AT-SPI/Orca, physical Wayland/X11, scale/theme/distro acceptance |
| JS/Node/Tauri/WebKit removal and distribution | Deferred deliberately | All functional/acceptance gates, measured performance, packaging/license closure before cutover |

Candidate tests use hand-authored synthetic data. No actual device identifiers,
topology, audio captures, private runtime logs or user profile data belong in
this public matrix or its fixtures. Real audio correctness, live restart behavior,
real-time performance, memory/CPU measurements and physical desktop acceptance
remain distinct, unperformed gates.

The [display settings candidate](./native-display-settings.md) preserves graph/origin state and applies no audio settings. Its original base focus defect was separately corrected; the combined source integration gate remains required before live controls.

The [combined native UI source batch](./native-ui-source-batch.md) records constructor reconciliation, the remaining publication gate and the separate pending backend capture-identity correction.
