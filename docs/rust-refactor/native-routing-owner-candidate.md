# Reviewed native routing owner integration

Candidate `4db58f2c4114086eb602967e6dc2b20cff4aa36e`, descended from `bf799a9`,
mounts the reviewed routing surface in the existing native workspace. Independent
review clears the complete owner bridge and private reset-witness correction:
72 core and 116 application tests plus eight additional probes pass, including
recreated links, replacement generations, Rec preservation, configuration/foreign
authority rejection, reentrant preflight and retained report/recovery cases.
Integration takes its five commits onto focus checkpoint `37a79f6c` without any
source conflict or new reconciliation logic. Both CI fixture corrections and
the reviewed focus correction remain byte-for-byte unchanged. The legacy
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

At this initial routing checkpoint there was no receipt-local closure getter,
so partial/unconfirmed recovery stayed gated. The later
[explicit owner recovery](./native-routing-recovery-candidate.md) now combines
the original receipt's local closure, current authenticated graph/configuration
inspection and released history ownership. It never recovers from a graph update
or apparent idle state alone, rewrites frozen counts or infers rollback/server
convergence. Those readback/acceptance limits remain explicit.

## Checks and remaining scope

The combined integration onto `37a79f6c` passes the locked native aggregate:
**86 validator controls, 72 core, 192 backend and 358 all-feature GTK tests**,
native formatting/build, Cargo/ELF closure checks, three display-free fixtures
and three controlled smokes. It records zero Node-tool attempts. Additional
checks pass **316 default GTK tests, 10 legacy tests and all five retained Rust
example compilations**. Accessibility-only mode was not separately repeated;
the all-feature run includes those assertions. No final local check fails.
The 12 reviewed source files match `4db58f2c` byte-for-byte; the backend WAV and
routing-focus corrections match the integration base exactly. Scope/privacy,
whitespace, unchanged versions/lockfile/dependencies and source checks pass.
Hosted validation of the eventual published head is a separate final gate.

Twelve injected controller/workspace cases plus a reducer-only regression cover shared admission and graph
barriers, one-step undo/redo and observed no-op progress, routing while recording,
authenticated bus-definition adoption and independent shared choices, busy/stale
preflight, reports after disconnect/invalidation, terminal in-progress gating,
1,000 unchanged ticks, stale replacement callbacks, custom navigation, hidden report delivery, reset confirmation/cancel and mounting
in the existing workspace. They control synthetic reports/postconditions, not
actual PipeWire behavior. Fixture definitions use the existing pure coupling
policy and the public owned-Rec-node name; no user hardware fixture is imported.

Original candidate locked local checks pass: **72 core, 192 backend, 10 legacy, 316 default
GTK and 357 all-feature GTK tests**, including all 13 focused routing-owner cases.
Native build and changed-file formatting pass. Earlier 314/315 default and
355/356 all-feature GTK results failed the reset case before the private
correction below. No assertion was removed or ignored. These local GTK results
use the existing isolated GTK 4.18/Xvfb environment; they do not resolve the
hosted GTK 4.14 held-input failure.

All five retained Rust examples compile via the legacy manifest. The connected
headless output matches its committed expected JSON, and the controlled connected
fixture smoke passes after the final core correction. Privacy, scope, unchanged
versions/lockfile/dependencies and whitespace audits pass. The aggregate CI
validator, other fixture smokes and hardware acceptance were not repeated for
this candidate; prior public-head receipts are separate evidence.

That original candidate branched from `bf799a9` and excluded the CI fixture
follow-ups published separately on PR2 at `88fa38b`. That head's hosted run 40142214920
passed 86 validator, 67 core and 192 backend checks, but failed one held-release
test (343/344 GTK): `held=true, blocked=true` after its deadline. The independent
held-input investigation owns those test/input files; this candidate does not
change them or claim to resolve that hosted failure. The integration base now
includes the separately reviewed correction `37a79f6c`; its exact-head PR run
40143901152 and branch run 40143897496 succeed with 345 all-feature GTK tests.

Larger resets remain rejected above 32 commands. Dynamic arming, staged idle
migration UI, active-recording structural policy, bus-edit history, saved audio
settings and complete transport/session/distribution work remain separate.
At this routing checkpoint, telemetry still used startup bus definitions. The
subsequent [meter follow-up](../../crates/pecan-ui-gtk/src/meters/adapter/README.md)
now reads the same adopted definitions as Session's mixer projection. Source
inspection also identifies a
follow-up for same-stamp bus-definition changes: retained history guards revalidate
against new definitions at execution/new graph, but no new configuration-history
reducer/pruning API is introduced. No fictitious graph revision is manufactured.
These are unfinished parity gates, not a claim of complete dynamic-bus behavior.

No hardware/audio/alignment, physical GTK/Wayland/cross-app/accessibility,
real-time/performance, crash/fsync, packaging or full license acceptance is claimed.
All-armed recording membership remains preflight only, not atomic membership CAS;
this bridge adds no recorder operation. No active-recording structural decision,
merge, release, version bump, new runtime dependency or JavaScript removal.


## Reset-history reproduction and private correction

The new end-to-end `reset_cancel_and_confirmation_use_one_real_shared_history_request`
case originally failed in both modes: expected one undo step, observed zero. Cancel correctly
submits nothing; confirming sends one shared native history request. The controlled
native report and graph satisfy the actual command postconditions, but the core
history step disappeared. The unchanged reducer-only reproduction confirmed the
same failure without Session or GTK. Both assertions now pass with the private
core correction and have independently cleared before source integration.

The minimal scenario is a stream with an explicit target serial `"10"`, no links,
and default output serial 10. `BackToAuto` plans target clearing. After an actual
Observed report and same-owner/generation graph with target None and still no
links, history lengths are `(0, 0)` instead of `(1, 0)`. This is the intermediate
window before WirePlumber restores its links; no causal transaction acknowledgement
is assumed. The source/port identities are unchanged. Reset planning after target
clearing can include default-port restoration absent from the initial target-only
plan. Its dependency set therefore grew outside the original history witness and
pruned the pending step. The guard now captures those potential endpoints from
the original graph and original registry generations. A pure policy projection
removes the already-planned manual links/targets to identify that restoration
scope; it never supplies a new backend origin, graph stamp or generation.
Existing generation, selector, scope-expansion and authority guards remain.

Final connected headless comparison and controlled fixture smoke pass with the
shared uncertainty hold included. No actual native backend was started.


### Reducer-only confirmation

A second reproduction, `pure_core_reset_target_clear_before_auto_restore_preserves_history`,
uses only the core Controller/HistoryGraph and runs without GTK,
Session, routing widgets, display or session bus. It confirms the exact plan is
one target-clear command, supplies its completion stamp and the same-identity
post-command graph, then asserts the expected `(1, 0)` history. It failed with
`(0, 0)` before the correction and now passes, isolating the dependency from new
owner wiring.

Run it with:

```sh
cargo test -p pecan-ui-gtk --locked pure_core_reset_target_clear_before_auto_restore_preserves_history -- --test-threads=1
```

Five additional pure core tests cover target-clear undo/redo, potential default
port replacement, ordinary gain/label observations, newly introduced manual
streams and changed defaults to an uncaptured existing destination. All pass;
replacement and expanded replay scope still prune history. There is no public
core DTO/API change, dependency, planner-command change or input-handling edit.
The correction is isolated in `7b866268643a87b16c73b7c4093f5932c2f1ffb2`
after the reducer-only reproduction commit `0967dd8271a0aa841fc333d5b1a233431a470a68`.
Independent review clears both the new owner integration and this core guard
delta; hardware/audio acceptance remains separate.
