# Native routing owner candidate

This separate descendant of reviewed source `bf799a9` mounts the reviewed routing
surface in the existing native workspace. It is a new application integration
candidate and requires independent review before PR2 publication. The legacy
application remains available. No live backend/audio/recording session, user
profile, real tray, device or workstation was used for this implementation.

## One owner, selection model, history and clock

The workspace retains one `Routes` coordinator and one Routing page. It shares
BusOwner's existing bus widget/model and Session; it adds no backend, inbox drain,
periodic source, history controller, native ticket allocator or selection store.
Output/Rec choices use the same bus-model binding and synchronously refresh the
routing projection. Custom patches navigate through the existing patchbay page
without an audio request. Ordinary routing remains available during recording;
the adapter never uses the idle-only structural-configuration gate for routing.

Crate-private BusOwner helpers capture the exact accepted native graph/witness,
validate the retained opaque presentation/configuration and inspect the same
native service before audio handoff. Busy inspection, changed versions or stale
origins submit nothing. The handoff forwards only the reviewed history action
through `Session::execute`; widgets do not dispatch their planned command list.
Preflight is an observation, not an atomic configuration CAS against subsequent
external changes. Native graph/registry admission remains the backend authority.

Session adopts the authenticated current bus definitions instead of planning
with startup defaults. It does not replan outstanding work against a changed
layout: changed definitions wait for the shared pending/gesture lease to release.
The history presentation context changes when definitions are adopted, retaining
the held Undo/Redo activation guards. The mixer reads Session's adopted model.

## Actual report retention and barriers

`Request` privately retains the original `NativeReceipt` alongside presentation
progress. Its actual report stays readable after history invalidation or terminal
presentation delivery. The routing bridge associates the widget's opaque token
with that real shared request; reports are polled separately from replaceable
graph offers, including while hidden/disconnected. No success, stamp, ticket or
completion report is synthesized. Reports are delivered before the corresponding
new capture so observed/no-op graph barriers can progress normally.

One pending token/request and one recent completed diagnostic are retained.
Unchanged ticks retain controls rather than rebind them: captures refresh only
on accepted bus/graph publications, choices or pending-configuration changes.
An initial handoff that produces no actual native request is rejected, not given
an invented no-op receipt. A malformed receipt retains a shared uncertainty hold
instead of granting new admission from a protocol mismatch. Observed history still requires the actual report and
accepted graph; finalization uses the existing common application authority.

Session now retains a shared uncertainty hold when history invalidation abandons
unfinished native work or a terminal report still has in-progress calls. Mixer,
patchbay, routing and configuration admission remain closed for that hold; graph
replacement/reconnect alone cannot release it. The original metadata must match,
and an actual terminal report with zero in-progress calls must establish closure.
Three prior synthetic tests now deliver that real terminal rejection before
expecting fresh admission; their old immediate-release expectation is replaced
by explicit hold/release assertions, not a fabricated completion.

Terminal in-progress counts do not prove quiescence. The current backend has no
per-request quiescence getter; this bridge never calls `reconciled()` from a new
graph, apparent idle state or retry. Partial/unconfirmed recovery therefore stays
gated rather than silently clearing the reviewed routing latch. A separate
quiescence/recovery contract is still required for complete parity.

## Checks and remaining scope

Twelve injected controller/workspace cases cover actual shared admission and graph
barriers, one-step undo/redo and observed no-op progress, routing while recording,
authenticated bus-definition adoption and independent shared choices, busy/stale
preflight, reports after disconnect/invalidation, terminal in-progress gating,
1,000 unchanged ticks, stale replacement callbacks, custom navigation, hidden report delivery, reset confirmation/cancel and mounting
in the existing workspace. They control synthetic reports/postconditions, not
actual PipeWire behavior. Fixture definitions use the existing pure coupling
policy and the public owned-Rec-node name; no user hardware fixture is imported.

Latest full checks are **not green**: 314/315 default GTK and 355/356
all-feature tests pass; the one failing reset-history reproduction below is
preserved and not ignored. Native build, formatting and scope/whitespace checks
pass. Connected headless projection and controlled smoke passed before the
additional shared uncertainty hold; their final rerun is recorded separately.
Early bridge-only tests passed before adding reset coverage and the stronger
Session hold; those earlier counts are not the final candidate gate. Core/backend/legacy suites and examples are not repeated for this GTK-only
delta; their preceding integration receipts remain baseline evidence.

This candidate does not include the separate XTest focus-readiness candidate or
resolve the hosted synthetic arming WAV-length failure reported after `bf799a9`.
Those CI follow-ups and this new owner logic have independent review gates.

Larger resets remain rejected above 32 commands. Dynamic arming, staged idle
migration UI, active-recording structural policy, bus-edit history, saved audio
settings and complete transport/session/distribution work remain separate.
Configuration-driven meter reprojection is not added here; the existing telemetry
factory still uses startup bus definitions. Source inspection also identifies a
follow-up for same-stamp bus-definition changes: retained history guards revalidate
against new definitions at execution/new graph, but no new configuration-history
reducer/pruning API is introduced. No fictitious graph revision is manufactured.
These are unfinished parity gates, not a claim of complete dynamic-bus behavior.

No hardware/audio/alignment, physical GTK/Wayland/cross-app/accessibility,
real-time/performance, crash/fsync, packaging or full license acceptance is claimed.
All-armed recording membership remains preflight only, not atomic membership CAS;
this bridge adds no recorder operation. No active-recording structural decision,
merge, release, version bump, new runtime dependency or JavaScript removal.


## Blocking reset-history reproduction

The new end-to-end `reset_cancel_and_confirmation_use_one_real_shared_history_request`
case fails in both modes: expected one undo step, observed zero. Cancel correctly
submits nothing; confirming sends one shared native history request. The controlled
native report and graph satisfy the actual command postconditions, but the core
history step disappears. Shared core source is deliberately unchanged pending a
separate correction/review. This candidate must not be promoted until resolved.

The minimal scenario is a stream with an explicit target serial `"10"`, no links,
and default output serial 10. `BackToAuto` plans target clearing. After an actual
Observed report and same-owner/generation graph with target None and still no
links, history lengths are `(0, 0)` instead of `(1, 0)`. This is the intermediate
window before WirePlumber restores its links; no causal transaction acknowledgement
is assumed. The source/port identities are unchanged. The likely dependency is
revalidation of dynamically generated reset selectors/restoration scope; that
cause is an inference from source inspection, not yet an independently cleared
core diagnosis. The failing integration assertion remains intact.

Final connected headless comparison and controlled fixture smoke pass with the
shared uncertainty hold included. No actual native backend was started.


### Reducer-only confirmation

A second reproduction, `pure_core_reset_target_clear_before_auto_restore_preserves_history`,
uses only the unchanged core Controller/HistoryGraph and runs without GTK,
Session, routing widgets, display or session bus. It confirms the exact plan is
one target-clear command, supplies its completion stamp and the same-identity
post-command graph, then fails the same `(1, 0)` history assertion with `(0, 0)`.
This independently isolates the dependency from the new owner wiring.

Run it with:

```sh
cargo test -p pecan-ui-gtk --locked pure_core_reset_target_clear_before_auto_restore_preserves_history -- --test-threads=1
```

Result: 0 passed, 1 failed. The full-mode counts above precede this additional
reducer-only test; production code is identical to `0fbe4ba`. Both reset
reproductions remain unignored and block promotion. No shared core production
source or interface was changed to conceal or bypass the failure.
