# Backend-only idle stereo append

Candidate base: PR2 `48597f762112ef7dd6c5c0280bf92181c76e7027`.
This extends the existing migration protocol with `LayoutAction::AddStereo`.
It adds no sequencer, worker, serialized DTO/IPC, domain policy, dependency or
version change. This backend checkpoint alone enabled no native UI behavior; its
only UI edit supplied the new field/exhaustive variant in a test constructor.
The separately [reviewed routed Add owner](./native-routed-add-owner-candidate.md)
now adopts this protocol. Partial repair and active structural policy remain
separate gates.

## Contract

Call the existing `Graaf::migrate_idle_bus(LayoutIntent)` with the original exact
published graph, config CAS, recording-membership revision and shared ticket.
Admission retains the owned Rec node/serial/generation, exact original `rec_N`
input prefix and incoming route endpoint/source identities. It derives the
proposal using `BusConfig::add()`, preserving existing gain/mute/arming state and
inactive stable-key lookup semantics. New arming policy is not invented.

Idle, known logical metadata and complete authenticated original inputs are
required. Existing migration/arming/recording ownership, stale revisions, replaced
objects and queue saturation keep their existing rejection behavior. A copied or
evicted original graph cannot be silently authenticated as a newer intent.
Generic full configuration still rejects routed growth; the new action is the
specific append-only protocol, not a bypass for arbitrary layout changes.

Add uses the existing `ApplyingLocal -> AwaitingPorts -> Observed` phases. It
issues no connect/disconnect calls: existing owned endpoint pairs stay unchanged,
including across harmless link-ID recreation. Routes on a different same-name
Rec instance do not become owned routes. Unexpected owned routes are conflicts;
the adapter never removes another client's route to recover.

The local target retains typed resize, gains/options readback, existing callback
guards and cancellation semantics. A complete resize alone leaves logical
configuration unpublished. Only a fresh graph observation with the exact complete
extended input map completes the action. The original prefix cannot be replaced.
The two suffix ports may appear out of order or during the resize callback; each
identity is latched at its first coherent publication. Known unpublished registry
replacement, later removal/rename/replacement, duplicate channel names and extra
inputs cannot be rebound. Commands addressing newly published owned ports are
also fenced by the existing production dispatcher while migration owns them.

Existing limits are unchanged: 64 audio channels, 256 initial route records, one
migration, the shared 255 ordinary/one reserved-stop queue, 32 dispatches per tick,
two-second observation waits and 30-second total lifetime. Add stores at most two
extra port records and makes zero routing RPCs. Stop/StopBus remain available.

## Authenticated completion evidence

`MigrationReport` gains this native-only field:

```rust,ignore
pub observed_inputs: Option<pecan_core::history_identity::HistoryGraph>
```

It is `None` during local/pending/failed Add and for existing non-growth actions.
Successful Add returns `Completed(Observed)` and a witness stamped exactly at
`ports_observed`. Its registry-generation map contains only the owned node and
complete final input prefix (at most 65 objects). The graph Arc and private backend
authority are the existing HistoryGraph mechanism; no new shared core type is
introduced. The witness survives ordinary graph revisions/cache eviction and
cannot authenticate a later same-ID replacement.

A future owner must correlate the original report ticket, source, revisions and
proposal, require decided success with zero executing calls, then retain this
witness alongside its original prefix/source proof. Compare registry identities
and the exact channel map against a later authenticated frame, plus actual fresh
configuration/options readback. Do not require an already-coalesced exact frame to
reappear or mint suffix identity from a later numeric-ID-only snapshot. Retain the
original receipt through busy reads and accepted terminal presentation before
releasing shared ownership. A superseded map is uncertainty, not automatic replay.

Observed means matching graph topology and local configuration were read. It is
not causal server acknowledgement, audio arrival, recorded samples or performance
acceptance. The historical report/witness is not rewritten by later graph events.

## Failures and repair gate

Before local effects, rejection/cancellation reports Rejected with no append.
Partial allocation/readback mismatch or loss of authority after local application
reports PartiallyApplied with the original proposal, typed actual allocation and
readback retained. Observation timeout after a successful local grow is likewise
partial, not false success. Local metadata remains unknown when the final graph
proof is missing. Worker unwind/destruction retains existing Unconfirmed behavior.

Cancellation cannot interrupt a claimed synchronous local apply; it finishes its
existing call boundary, retains cancellation/partial evidence and schedules no
future migration calls. No rollback, retry or command replay is introduced.
Late port publications cannot upgrade a frozen failure. An old CAS cannot repeat
the operation, and a fresh generic Add cannot infer logical metadata after a
partial allocation. In particular, repair must target the original proposal;
blindly appending another stereo pair is not recovery. Explicit partial repair
and UI recovery remain separate parity work, not an assumed permanent restriction.
Active-recording structural changes remain rejected without choosing segmentation.

## Synthetic validation

Ten focused regression groups use the existing authored topology/local target
fixture. They cover routed and odd-prefix append, out-of-order/immediate port
publication, original and suffix generations, coalescing/cache eviction, route
preservation/recreation/foreign instances, new-port dispatch fencing, malformed
maps/conflicts, partial allocation, timeout, cancellation, duplicate/stale requests,
generic configuration and recording gates, channel bounds and reserved stop.
An initial fixture republished a replacement before its first observation; it was
corrected to exercise known unpublished replacement, with a positive control for
valid first-publication coalescing. Production assertions were not weakened.

All values are synthetic public-project data. No actual audio/device topology,
capture, user profile or real PipeWire service is used. Final source-matched test
counts and commit/branch are returned separately for independent review. Hardware,
audio, physical desktop and performance acceptance remain unrun.

Local validation passed 76 core, 240 backend (including the ten new Add groups),
ten legacy library and 16 headless GTK migration checks. All-feature GTK test
compilation, scoped Rust formatting and whitespace checks pass. The GTK checks
at this initial backend checkpoint still assert that routed Add has no owner
capability. Subsequent backend/owner review and adoption receipts are recorded
below and in the linked owner record; these initial checks implied no UI adoption.

## Reviewed backend integration

Independent review clears exact candidate `550bd09964f7c0b856d0a1b213d87b64874fb8e5`
with 40 migration checks, five additional synthetic probe groups and 11 owner/widget
checks. Those verify both authenticated suffix inputs before success, original
endpoint-pair preservation without routing calls, truthful partial/replacement
failure and no late upgrade of frozen outcomes. Exact candidate
[CI40173662173](https://github.com/jee-mj/pecanmixer/actions/runs/40173662173) passes.

The backend applies without conflicts onto login-context PR2 `e3a348aa`.
Backend source and its existing owner-test constructor match the reviewed commit
byte for byte. Login-context, reconnect and all other owner runtime source remain
unchanged; only integration coverage notes are added. No dependency, version,
new worker/sequencer or live capability enablement is included. A separate owner
candidate must retain the supplied completion witness directly and pass its own
independent review before routed Add is enabled. The linked owner record now
supplies that separately cleared adoption at `6e0cc8fd`; partial repair is not
included.

Fresh combined checks pass 86 validator controls, 76 core, 240 backend, 546
all-feature GTK and 492 default GTK tests, plus ten legacy library tests and all
five legacy examples compiled (none executed). Native build, formatting,
Cargo/ELF audits, three headless fixtures and three controlled smokes pass. Host
and explicit-target workspace packages are cleaned and rebuilt first; zero
Node-tool attempts are recorded. Source/privacy/whitespace and unchanged
dependencies/versions pass. No final local failure or dependency/environment
blocker remains. Exact combined-head hosted results are verified separately after
publication, without treating candidate success as a substitute.
