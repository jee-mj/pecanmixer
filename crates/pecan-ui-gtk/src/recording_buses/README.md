# Recording-bus UI model candidate

`recording_buses::Model<O,R,T>` is a pure presentation/request boundary for the
bus portion of `App.tsx` / `Mixer.tsx`. It creates no GTK widgets, workers, timers,
files or backend. Main startup, Session, recording and tray remain untouched.
The legacy implementation remains available. Version/dependencies are unchanged.

## Model and requests

The owner supplies one ordered authenticated Snapshot containing an opaque
configuration origin `O`, known logical BusConfig or None, the full core Mixer
projection, connection/activity status, typed local operation progress, stable
track status and explicit capabilities. Projection must use current known core
bus definitions and graph; it must not be pruned by display settings. Model
retains one latest snapshot, with explicit row/track input budgets (default
64/128). Oversized/invalid publications are rejected, not silently truncated or
interpreted as default configurations. Rejection retires old bindings and clears
the projection until a valid publication arrives; publication_error retains the
typed diagnostic. The caller must surface it. This does not drop any completion
or finalization obligation, which remains owned outside this presentation model.

`bind` captures an immutable UI presentation and exact opaque origin clone.
Bind at interaction begin and retain its action/key; a delayed release must not
construct a fresh binding or read a new selected bus to retarget old intent.
`request` emits AddStereo, Split, Join, Remove or exclusive/additive Arm intents.
Add/coupling/arming previews delegate to existing core BusConfig methods. They
never replace the observed configuration, start recording or create pending
success. The owner must acknowledge admission and publish later typed status.
New publications, replacement/reconnect and another model instance reject stale
bindings. UI context tokens do not manufacture backend epochs or registry identity.

Removal deliberately emits no UI-built migration/configuration/command list.
The owner must call existing core BusConfig::remove with its exact graph, retain
its source identity, and run a reviewed staged migration executor. No migration,
link ordering or track remapping is implemented here. The last bus has no remove
action. Split/join availability comes from core BusDef::coupling, including the
left-only mono join control. Configured state/gain/arming inheritance remains core
policy; this module does not duplicate those rules.

Output and Rec choices are independent, session-only UI state. Output fallback
matches App.tsx: retained selected output, targetable default sink, first output.
Rec fallback is its retained selected graph-projected bus, then the first bus.
Missing choices fall back without erasing their retained keys, matching JS if
they return. Disconnect exposes neither choice. Unknown configuration does not
invent Rec grouping. Selection itself emits no audio/configuration/history work.

## Existing backend adapter dependencies

The independently reviewed idle configuration API is at
`5cadf0ba331387f95428285f0879dd6b6092f831`; this GTK-only branch does not import or
modify its backend sources. The future thin adapter must retain the original
ConfigSnapshot::revision inside `O`, and bind the relevant exact graph/witness
where required. It must not stamp old frontend configuration with a new revision.
Never reconstruct local logical grouping from BusReadback gains when the native
configuration is None. A busy `config_snapshot()` / `try_report()` read is not a
disconnected/default publication.

Operation<R> uses the adapter's real request identity. Its Progress is UI
presentation, not a second receipt protocol: ConfigProgress::Queued/ Applying
stay pending; Completed Applied/NoOp become AppliedLocally/NoOpLocally; rejection,
partial application and unconfirmed outcomes stay explicit. The adapter must
correlate the real report and install its authoritative resulting snapshot.
The adapter filters stale/delayed reports and must not publish an older result
under a freshly captured origin; this generic UI model cannot authenticate it.
Partial/unknown effects require reconciliation before requests resume. Rejection
does not automatically retry or modify current state. Local configuration
success is neither graph-observed port readiness nor writer/recording success.
Rows therefore separately expose a graph-projected destination key or None.

Native idle configuration currently requires MIDI off and rejects clock/MIDI
start-stop. This model never silently rewrites preferences to satisfy it; the
owner must provide deliberate native-valid input and surface rejection.

Capabilities default to Pending, including the idle adapter itself. Once that
adapter is reviewed/mounted, idle unrouted growth/coupling/arming may be exposed.
Routed layout, channel shrink/removal, active recording layout and dynamic arming
retain separate PendingCapability reasons. They are unfinished integration work,
not permanent product restrictions: explicit future support enables the same
intents without changing core plans. Declaring support requires an actual
reviewed executor; the cleared idle API alone rejects active updates, shrink and
routed layout changes. Active arming ultimately needs the JS behavior of starting
newly armed tracks and stopping disarmed tracks while preserving others, through
real recording receipts. This module does not issue those unsupported opcodes.

Recording controls remain PendingCapability(RecordingReceipts); no recording
start/stop contract is invented. All admitted work and finalization obligations
remain owned by the application even if a binding or view is replaced.

## Typed recording presentation

`Track<T>` retains opaque stable take/track identity supplied by the adapter and
an explicit current bus association. Multiple tracks per bus remain visible,
including an older finalizing/error track beside a new one. Unmatched tracks
remain accessible separately; filenames/renumbered bus keys cannot retarget them.
Any future active-layout capability must supply validated logical-bus/track
associations after migration. The UI does not infer that mapping.

Writer status at `86fdd51014d3e78f2036325296ffef29fa3dcea4` distinguishes Opening,
Ready, Finalizing, Finished and Joined, with separate per-track results. Map
writer readiness to WriterReady, not Recording. Preserve explicit finalization
results separately from joined outcomes; an idle transport or a finalized WAV
does not prove the writer thread joined. Recording itself needs the still-pending
native recording/capture evidence. Unknown activity disables configuration rather
than assuming idle. Track elapsed values use Duration, with source validation
remaining the producer's responsibility. No writer receipt is polled here.

## Synthetic evidence and remaining work

Tests reuse the documented public synthetic `fixtures/domain` inputs, traced
from original JS and already reviewed for core parity. They test UI/core contracts
and the App.tsx selection fallback; they do not newly execute the browser oracle.
Checks cover preview-only add/split/join/arming, inherited state, removal intent,
separate selections, missing/default fallback, pending/local/partial/error states,
active/routed capabilities, stable multiple-track presentation, stale bindings,
reconnect, missing configuration, graph-port absence and input budgets.

```sh
cargo test -p pecan-ui-gtk --locked recording_buses
```

The pending real adapter, GTK widgets, gain/mute controls, migration, recording
receipts, transport/folder actions and production adoption require later slices.
No physical desktop, AT, live server, hardware/audio, WAV recording or performance
acceptance is established. No user profile/device data is read or published.

Cloud validation: twelve focused tests pass in default and `a11y-tests` feature
modes, without a display or audio server. The optional GTK feature build does
not establish accessibility interaction or widget acceptance.
