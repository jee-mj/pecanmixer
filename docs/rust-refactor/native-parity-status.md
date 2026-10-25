# Functional native parity matrix

This is a source-level implementation matrix, not a hardware/desktop acceptance
report. The full functional JS port remains the goal. The legacy React/Tauri
application is retained. Reviewed entrypoint adoption starts at `20e9f90c`. The
[bus/backend integration record](./native-bus-backend-integration.md) identifies the
subsequent owner adapter, corrected dynamic-arming and idle-migration libraries,
their independent gates and exact cloud checks. "Candidate" means independently
reviewable source exists, not completed live acceptance. The corrected bus owner
has independently cleared its original probes; this reviewed batch preserves the
existing CI driver. The [routing owner follow-up](./native-routing-owner-candidate.md)
now mounts routing through the same Session and BusOwner, with independently
cleared receipt retention and reset-witness correction. The reviewed meter
follow-up now reprojects adopted layouts through the same accepted graph/batch.
Reviewed arming, idle migration, Folder and Rec-level owner adapters are now mounted.
Rec gains/mute use original registry authority and shared one-flight admission
while preserving unrelated channels and stored muted dB; these local edits do
not enter graph Undo. Immediate configuration-driven history reconciliation
remains separate.

| Original behavior and source | Rust/native state | Remaining gate |
|---|---|---|
| Graph subscription, disconnect/reconnect, rate/quantum (`src/api.ts`, `src/App.tsx`) | Reviewed exact-frame owner/coordinator/finalization shell source | Live audio/desktop acceptance |
| Apps/hardware/destination grouping, numbered channel pairs, stable ordering (`src/mixer.ts`, `src/indeling.ts`) | Initial pure audio projection and reviewed fixtures | Complete locale/corner-case parity and live acceptance |
| Per-strip gain, preserve unselected channels (`stripDb`, `metDb`, `Kaart` in `src/Mixer.tsx`) | Reviewed one-flight controller, latest desired target and observed final full-vector history | Audio/input acceptance |
| Whole-node mute (`Kaart`, `muteStap`) | Reviewed immutable mute action/registry identity and canceled held-activation source | Audio/input acceptance |
| Partial-pair mute and restore prior gain (`voorMute`, `Kaart`) | Reviewed policy plus serialized receipt/graph memory commit and complete-vector undo | Audio/input acceptance |
| Drag/fine drag, wheel grouping, rounding, double-click reset (`Fader`) | Reviewed policy and owner coordinator; ordinary refresh preserves desired keyboard position; unconfirmed held input requires release | Physical input acceptance |
| Independent output destination and Rec-bus selections (`bewerkt`, `bewerkteBus`, `kiesBewerkt` in `src/App.tsx`) | Bus owner and routing page share independent choices without audio commands; Session and meters use the same adopted definitions | Immediate configuration-driven history reconciliation; reconnect/layout acceptance |
| Normal/manual/custom route states and green/R toggles (`BronStrip`, `schakel`, `schakelRoute`) | Mounted routing shares selections/configuration, history and actual reports; explicit recovery checks original-receipt closure, current authenticated inspection and released history; custom routes open the patchbay | Busy/unknown inspection remains gated; server convergence and audio/input acceptance are unproven |
| Back to auto / manual route count (`telHandwerk`, `terugNaarAutoStap`) | Mounted count/confirmation uses shared observed history and explicit failure recovery; original-generation restore witnesses survive target-clear transitions | Reset batches over 32 commands and audio/input acceptance |
| Cable create/remove/move, selection/delete/cancel (`src/Patchbay.tsx`, `src/handelingen.ts`) | Reviewed keyboard/pointer libraries attached to reviewed coordinator source | Physical-input acceptance |
| Patch module positioning and reconnect behavior (`Plekken`, `ModuleKaart`) | Reviewed session movement/rearrange and reconnect aliases on retained view | Session-layout and physical-input acceptance |
| Undo/redo queue and keyboard shortcuts (`src/geschiedenis.ts`, `src/handelingen.ts`, `src/App.tsx`) | Reviewed generation/selector guards, shared receipts, immutable button activation and observed gesture history; standard window Ctrl/Meta+Z, Shift+Z and Y bindings preserve native editor history and allow later autorepeat activations | Busy/stale shortcut activations are dropped; JS queued input and physical/audio acceptance remain |
| Add/remove/link buses, Rec gain/mute/arming (`src/bussen.ts`, `Mixer`) | Bus owner mounts typed idle config/readback and correlated controls; Session/routing/meters use adopted definitions; corrected dynamic arming is wired through shared owner admission/retained receipts; idle routed split/join/remove use reviewed retained migration receipts; reviewed Rec gain/mute owner retains original witness, one-flight/coalesced targets and final flush for idle/known native recording, without graph Undo | General saved-layout restore/clock effects and immediate configuration-driven history reconciliation; routed Add and observed-port/desktop acceptance; active structural edits remain a user-decision gate |
| Record/stop, per-bus start/stop, elapsed time, folder action (`src/Transport.tsx`, `src/api.ts`) | Bus owner adds explicit-root all-armed/per-bus start, captured stable stop IDs, retained receipts and quit diagnostics; Folder is mounted with latest-start authority and off-UI preparation/launch/reaping; reviewed elapsed producer/adapter preserve measurement age and use the existing clock/oldest-take formatter | Live elapsed/recording/desktop acceptance and sample alignment remain unproven; all-armed membership is preflight only, not atomic CAS |
| MIDI clock/start-stop, MIDI tabs/route/activity (`src/MidiMixer.tsx`, `src/midi.ts`, `src/MidiMeter.tsx`) | Legacy MIDI feature flag remains disabled | Preserve disabled status; separately define/verify any enabled MIDI parity scope |
| Meter ballistics/peak hold, per-channel rendering (`src/meters.ts`, `src/Meter.tsx`) | Entrypoint uses Session-adopted definitions for same-graph split/add/join/remove reprojection; retained wrappers, registry bindings, graph-before-peaks and visibility gating remain | Capture freshness/availability protocol and live/animation acceptance remain |
| Hide/show strips/rows and reconnect names (`src/App.tsx`, `Mixer`) | Reviewed display panel/filter/save bridge; full graph/origin/history retained | Transient IDs must not become durable identity; reconnect/desktop acceptance |
| Row height and destination split preferences (`useRijGewichten`, `Greep`, `Deling`) | Reviewed adjacent-pair/split resize/reset input and request-aware save triggers | Physical input acceptance |
| Saved buses, hidden keys, clock preference and layout (`leesBussen`, `leesKlok`, `leesVerborgen`) | Versioned contract/storage plus display bridge and accepted Rec layout/dB/mute/arming writeback through the existing coalesced worker/final drain; latest intent survives backend failure and older saves, while saved MIDI/clock/display values are preserved; independently reviewed matching-capacity idle startup/reconnect restore uses original authority and post-report presentation barriers | General growth/shrink/routed restore, pending/uncertain prior-work recovery, explicit reload/recovery policy, import and clock effects remain gated; no user profile access performed |
| Tray open/close/quit, autostart/single instance (`src-tauri/src/tray.rs`, `autostart.rs`, `lib.rs`) | Reviewed shell and explicit-context desktop worker/tray/CLI controls; one retained worker/drain and immutable result-bound control tokens; ordinary launch only inspects, while explicit enable/disable/refresh preserve legacy entries | Generated login arguments do not retain desktop-management context; complete session/process arbitration, native packaging identity and actual desktop/login acceptance remain |
| Graceful recorder shutdown/error/timeout handling | Reviewed backend/legacy completion and native hold/tagged-handle/off-UI settings drain | Live recording/quit acceptance; no fsync/crash-durability claim |
| Accessible controls, keyboard navigation, responsive layout (`src/*.tsx`, `src/styles.css`) | Synthetic native names/focus/layout checks; control candidate | End-to-end keyboard/AT-SPI/Orca, physical Wayland/X11, scale/theme/distro acceptance |
| JS/Node/Tauri/WebKit removal and distribution | Deferred deliberately | All functional/acceptance gates, measured performance, packaging/license closure before cutover |

Candidate tests use hand-authored synthetic data. No actual device identifiers,
topology, audio captures, private runtime logs or user profile data belong in
this public matrix or its fixtures. Real audio correctness, live restart behavior,
real-time performance, memory/CPU measurements and physical desktop acceptance
remain distinct, unperformed gates.

The [display settings candidate](./native-display-settings.md) preserves graph/origin state and applies no audio settings. Its original base focus defect was separately corrected; the combined source reconciliation is cleared while production ownership, history and live-control gates remain.

The [combined native UI source batch](./native-ui-source-batch.md) records constructor reconciliation, the cleared source publication gate and the backend capture-identity correction now integrated in the identity follow-up.

The [identity follow-up](./native-identity-batch.md) closes retained history selector/generation and backend capture-retention source gates. Later owner/entrypoint wiring adopts these guards; audio/desktop acceptance remains incomplete.

The [finalization-aware shell](./native-finalization-shell.md) adds one retained owner/window/clock, shared admission shutdown and real completion/settings-drain wiring. Its reviewed source is installed by native startup; fixture mode retains a separate synthetic owner.

The [held history activation correction](./native-held-history-activation.md) prevents a held/retired Undo or Redo callback from adopting a newly exposed entry. Independent owner re-review cleared its native-input and same-graph before/after regressions.

The [earlier reviewed source integration batch](./reviewed-native-source-batch.md) supersedes pending-review markers for its listed slices. Later reviewed startup adopts those controllers; the current bus adapter below has separately cleared its Refresh correction gate.

The [continuous gain/partial mute candidate](./native-gesture-coordinator.md) composes the pure policies with owner identity, one-flight samples, observed memory and one grouped history step. Its original three input findings were independently reproduced and corrected. The corrected source is independently cleared; executable startup and acceptance status are unchanged.

The [gesture review correction candidate](./native-gesture-review-correction.md) addresses held-mute semantic substitution, unconfirmed held-key retries and lost keyboard accumulation. Independent re-review clears `5608d2f4`, including the former failures and fresh-input controls. The combined source retains its guards and the separate settings display hooks.

The [settings result/display bridge candidate](./native-settings-bridge.md) retains local/disk revisions, wires display-only edits and admits the final dirty document before worker close. Its settings-only delta is independently cleared. The [next combined source candidate](./native-parity-source-batch.md) reconciles it with the corrected gesture source and records the separate combined-tree gate. No executable or audio-settings adoption is implied.

The [native entrypoint adoption candidate](./native-startup-adoption.md) wires one owner/shell, explicit path or memory settings, functional mixer/patchbay and authenticated accepted-batch telemetry. Its entrypoint wiring has independently cleared and is public at `20e9f90c`. The subsequent bus/configuration/recording owner wiring below has a separate integration record.

The [bus/configuration/recording owner candidate](./native-bus-owner-adapter.md) adds a shared ticket/admission lease, exact edited revision handoffs, retained native receipts and common-quit diagnostic audit. Its original busy-Refresh finding is corrected at `cb24a2ba`; independent re-review clears all nine original probes and three additional recovery/authentication probes. The reviewed dynamic-arming owner/UI follow-up now adopts the arming library. Reviewed idle routed split/join/remove now use the retained migration owner; routed Add remains unavailable. Structural edits during recording/finalization remain gated pending the user's product decision. Module positions match the original session-only behavior; durable persistence is not required for parity.

The [routing surface](../../crates/pecan-ui-gtk/src/routing/README.md) and correction
`8240f71b` retain configuration uncertainty across zero-effect route rejection.
The [reviewed routing owner](./native-routing-owner-candidate.md) mounts them with
shared selection/history, exact configuration/graph preflight and retained real
reports. It preserves the separately reviewed focus correction `37a79f6c` and
both CI fixture corrections. Replacement/disconnect cannot erase unfinished work;
terminal nonzero in-progress counts cannot prove quiescence. Reset plans above
32 commands remain rejected without truncation. No real backend/audio acceptance
or active-recording structural-policy decision is implied by source publication.

The [meter follow-up](../../crates/pecan-ui-gtk/src/meters/adapter/README.md) combines
independently cleared `6e2daac6` and `a4225824`. Its read-only accessor uses exactly
the configuration behind `volume_model`; normal workspace polling adopts layout
changes without manufacturing a graph revision or adding a consumer/timer. It
preserves unaffected wrappers, retires removed keys and clears shared samples,
hold and clipping before incoming peaks. This closes the startup-default meter
projection gap. It does not establish sample freshness/health or implement
immediate history reconciliation.

The [reviewed receipt-local closure API](./native-execution-closure.md) now exposes
nonblocking Pending/Closed evidence on the original retained receipt. A terminal
report's frozen counts remain historical. Closed prevents further local dispatch
callbacks for that receipt; it does not establish server convergence, rollback,
observation or recorder finalization. The initial library checkpoint did not
reopen recovery latches. The subsequent
[reviewed explicit owner recovery](./native-routing-recovery-candidate.md) now
combines the original receipt proof, current owned graph/configuration inspection
and released history ownership. It retains the original failed/unconfirmed
outcome and adds no failed-prefix undo, command or graph publication. Held input,
retired controls and reentrant inspection cannot recover a later request.
Busy/unknown inspection and finalization remain gated. Reset batching above 32
commands, server-convergence proof and hardware acceptance remain separate.

The [dynamic arming owner/UI candidate](./native-dynamic-arming-owner.md) connects
existing active Arm/Armed controls to the reviewed backend protocol and retains
parent/child outcomes separately. Independent re-review clears `10fb50f5`,
including child-polling reentry, busy-child retention and original-request
validation before lease release. Idle migration adoption and active-recording
structural policy remain separate gates.

The [reviewed idle migration owner](./native-idle-migration-owner.md) adopts the
existing backend for idle routed split/join/remove, retaining actual receipts,
readbacks and graph-before-release evidence. Independent re-review clears
`5c131247`, including reconcilable lost-epoch completion and matched owner/widget
terminal barriers. Routed Add and active-recording structural policy remain
separate gates.

The reviewed [Folder owner adoption](./native-folder-owner.md) mounts
the corrected helper with actual latest-start receipt authority and explicit
worker-based preparation/launch/reaping. Independent correction review clears
`07e6edaf`, including retained cancelled-worker capacity and prepared-plan path
correlation before launch. It is included in the matrix above. Its original
missing elapsed producer is adopted in the separately reviewed follow-up below.

The reviewed [elapsed adoption](./native-elapsed-adoption-candidate.md)
adopts independently cleared producer `38db4706` through retained measurement
instants and the existing transport clock. It compensates different observation
ages without restamping cached reports or rebinding edit controls. This new UI
delta clears independently at `bcdd4553` and is included here. Source adoption
does not establish physical timing, sample alignment or recording acceptance.

The [reviewed Rec-level libraries](./native-rec-level-library-integration.md)
preserve logical gain/mute semantics and provide retained typed controls plus a
gain-only backend path. Original-origin interface `bfad5259` and the subsequent
[reviewed Rec owner](./native-rec-level-owner-candidate.md), corrected at
`594e46eb`, are now adopted. Native callbacks require the original Session
witness and known Rec identity/ownership; one shared lease spans coalesced
samples, actual readback, final flush and matched terminal presentation. Newer
superseding state becomes explicit uncertainty without replay; genuinely older
readback waits. Refresh archives the original outcome before fresh admission;
unknown logical metadata additionally requires explicit initialization. Stored
dB/mute/arming and unselected channels are preserved, with no graph Undo for
these local edits. Opening, unknown ownership and finalizing fresh input remain
gated; active-recording structural edits and hardware acceptance remain separate.

The [reviewed standard GTK history shortcuts](./native-history-shortcuts.md)
now adopt the original Ctrl/Meta+Z, Shift+Z and Y chords outside native inputs.
Each activation selects current guarded history; a later autorepeat may act
after acknowledgement or focus return. Busy/stale activations are dropped with
no queue/retry. Existing pointer/Space/Enter guards remain separate. Independent
review passes 33 targeted history checks and three additional native probe
groups. This does not close JS queued-intent or physical input acceptance.

The [reviewed Rec preference writeback](./native-rec-preference-writeback.md)
merges accepted user fields into the latest settings document, independent of
backend completion. Passive readback and default initialization never save or
restore it. Original MIDI/clock/display values survive, older saves cannot
replace newer edits, and common quit drains the final desired document off UI.
Saved intent is not proof of applied audio. The separately reviewed bounded
restore below adopts original-generation full-config authority; broader metadata
initialization, safe shrink/routed growth and explicit recording/reload policy
remain separate. Writeback and shortcut bindings alone add no audio restore.

The [reviewed full-config origin API](./native-full-config-origin.md) retains
original Rec node/input generations alongside configuration CAS, with no weaker
fallback. It is integrated independently of automatic Rec restore. Source-only
authority does not establish graph convergence or resolve unsupported layouts.

The independently reviewed [bounded Rec restore owner](./native-rec-restore-candidate.md)
uses that strong API for valid initial or memory preferences and matching local
channel capacity. It retains original input identity, actual receipt and shared
lease through post-report readback and accepted presentation. This does not add
general layout migration, active-recording reload/structural policy, clock effects,
profile import or automatic recording. Corrected owner `3c318cee` independently
clears original foreign-route probes while preserving the owned routed-layout
gate. Its source is adopted without runtime reconciliation onto `f9fc8e7c`.

The [reviewed desktop helper library](./native-desktop-library-integration.md)
preserves explicit native settings/recording launch arguments and a distinct
native autostart identity without touching legacy entries. The separately
reviewed worker/tray adoption below now uses it; real desktop/login acceptance
remains unperformed.

The independently reviewed [reconnect-after-completed-edits owner](./native-rec-reconnect-edits-candidate.md)
addresses the retained successful user-operation gate on `d0d8f23f`. It requires
closed original receipts, released ownership and fresh authenticated idle
presentation before archiving old work and capturing current preferences for a
new request. Pending/uncertain work, recording reload and general layouts remain
gated. Corrected `91afa807` is integrated with the unchanged pure desktop helper.
Recording observed by the owner latches its existing epoch skip even if nested
polling publishes joined state before returning; a later epoch can recover.

The independently reviewed [explicit desktop worker/tray source](./native-desktop-adoption-candidate.md)
uses one existing settings worker/drain, separate memory preference capability and
explicit CLI root/native-artifact context. Ordinary launch never enables an entry.
It retains actual results and original control tokens; legacy takeover and profile
discovery are excluded. Generated login arguments do not yet retain management
context, so full autostart/session parity remains unfinished. Correction `bc438072`
preserves a newer queued presentation source against retired callbacks while
retaining same-source ordering, bounded storage and reserved Quit. Independent
re-review clears its unchanged action/file assertions and three additional probes.
The [integration record](./native-desktop-owner-integration.md) distinguishes those
reviewed libraries and mounted explicit-context controls from remaining desktop
acceptance and session work. No actual user profile or login entry is operated by
the synthetic checks.
