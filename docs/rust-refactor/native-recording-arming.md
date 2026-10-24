# Native dynamic audio arming candidate

This separate descendant is based on multitake candidate
`b5c337452896191b1f41500cd0190890bf5dad57`, itself based on cleared recording
`0e24fd9371c34b27d0d8b5f266549efd7fcab18a`. The multitake base and this descendant
both require independent review before integration. Neither commit enables UI
recording. The base is unchanged; this is not structural bus migration.

## Callable boundary

`Graaf::arm_recordings(RecordingArming { ticket, origin, expected_config,
expected_recording, armed, root }) -> Result<ArmingReceipt, RecordingError>`
accepts a complete desired set of configured audio bus keys. The owner can derive
it with the existing core exclusive/additive arming policy. This API does not
duplicate that click policy or accept changes to layout, gain, mute, MIDI or clock.
It requires an existing live native recording and known native configuration.
Idle arming continues to use the idle configuration contract.

Capture the exact graph origin, ConfigRevision and RecordingRevision actually
edited. All three are checked before admission. The bounded input rejects
duplicates, unknown keys, more than 64 keys, keys over 256 bytes and roots over
4096 bytes. The worker retains identity witnesses, not a full old graph or spare
caller allocation. Ordinary graph revisions/cache eviction do not rebind it;
node/port replacement, epoch, format and logical configuration changes reject it.

One arming operation owns one ordinary FIFO entry. Claiming is ordered with the
existing commands and counts toward the existing 32-command dispatch budget.
Later worker ticks perform at most one additional child-writer admission per tick,
alongside that bounded FIFO dispatch. No unbounded continuation queue is created.
Other new start/arming requests return Busy until this local decision is made.
Configuration/legacy start mutations remain locked during claimed ownership.
Graph routing commands can interleave; this is not a server transaction.

`recording_snapshot()` now also exposes `arming: Option<ArmingReceipt>` for the
currently owned operation. A busy snapshot/report try-read returns None, not an
empty/default publication. The owner must retain admitted receipts independently.
There is no completed-arming lookup registry or automatic retry.

## Additions before removals

The delta follows the legacy handler's old/new armed sets, in core bus order:

1. Capture original live track identities and the stable TrackIds eligible for
   removal. Independently started unarmed recordings outside that old armed set
   are preserved.
2. Start each newly armed bus separately, in its own immutable take. A matching
   already-live capture is explicitly reused, with `new_take == false`.
3. Wait across worker ticks for each addition's actual local capture attachment.
   An initialized/Ready writer alone is insufficient. Existing captures continue.
4. Only after every addition remains attached, apply/read back the local armed
   transport options, then request stops for the captured removal TrackIds.
   No numeric bus name is resolved again to choose a replacement track.

This commits only armed metadata and transport options. The worker's local
readback/options accessors perform no I/O and cannot reenter Commands while its
short state lock is held. No channel resize or gain setter is called. A readback
mismatch withholds removals, preserves captures, invalidates uncertain config
metadata and reports an explicit failure requiring reconciliation.

If an addition fails, removals are withheld. Previously recording tracks and
successfully attached additions remain alive. When configuration identity still
matches, readback becomes the old armed set union currently live successful
additions; failed additions are left unarmed so an explicit fresh attempt is
possible. Individual child receipts retain opening/ingress/write/close errors.
These are partial effects, not rollback or an all-or-nothing success.

Native stop-all, disconnect and shutdown cancel queued/future additions and stop
unattached child writers. A real track/take stop also cancels the remaining
arming plan. Retired/already-stopped identities remain no-ops and do not cancel an
unrelated new operation. Legacy Stop/StopBus fence a claimed operation; an earlier
queued legacy stop retains its FIFO precedence and cannot silently start a stale
arming plan afterward. Disconnect/reconnect never replays the plan.

Cancellation leaves configuration readback as observed and reports any admitted
children. Already attached additions can remain alive unless the requested stop
targets them or all recordings. An opener already admitted can finish file setup
after a stop, but its request stays fenced against late capture attachment. Its
writer must still finalize/join. Stop-all returns every affected take receipt;
the owner separately retains the arming receipt.

## Receipt meanings

`ArmingReceipt::try_report()` is a nonblocking status inspection.
`wait_timeout(Duration)` is an off-UI bounded polling helper, never a join or
filesystem wait. Timeout retains the same pending handle and does not cancel it.

- Queued: FIFO admission only.
- Adding: ordered operation claimed; individual additions may be opening/ready
  or attached. Inspect their retained RecordingReceipts.
- ApplyingRemovals: local options/readback and stable-ID stop decision in progress.
- Completed(AppliedLocally): desired armed readback matched and the reported
  stable-ID stops were requested. It does not acknowledge file closure.
- Completed(NoOpLocally): verified local state already matched; no take restart.
- Completed(PartiallyApplied): some local ownership/config effects or cleanup
  obligations exist; inspect failure, additions, withheld IDs and actual readback.
- Completed(Rejected): requested arming was not applied. Retained child receipts
  still describe any attempted opening/cleanup; rejection is not a no-files claim.
- Unconfirmed: abnormal operation-owner loss prevented a trustworthy decision.

`is_decided()` concerns that local decision; `is_success()` accepts only Applied
or NoOp. Neither proves audio arrival, a graph/server acknowledgement, finalized
bytes, actual writer join, durability or successful backend shutdown.

Reports retain the original ticket/source/config/membership revisions, current
local readback/revision, additions, accepted removed IDs, withheld IDs and typed
failure. Children inherit the operation ticket: correlate through the parent
receipt plus bus/stable take identity, not ticket alone. Poll child receipts for
per-track finalization and joined take completion. Keep recording failures even
when all WAVs finalize and common shutdown succeeds. Quit still uses the common
backend lifecycle handle and its explicit error/pending policy.

## Bounds, scope and verification

No resource limit is widened. There is one operation, at most 64 input keys and
planned/attempted additions, and the existing 64 owned-take/writer/open-track limits,
16 recent take lookups and bounded sample queues. Opening/close/join remain off
UI/process threads. A stalled filesystem can leave pending cleanup; capacity and
timeout failure never authorize dropping that obligation or exiting successfully.

Synthetic tests exercise exclusive/additive-set deltas, separate takes, readiness
versus attachment, first/partial/ingress failures, no-ops, stable stops/rearming,
WAV continuity, stale config/membership/origins, replacement and ordinary revisions,
saturation, concurrent stop during opening/admission, disconnect/reconnect,
shutdown and wrong options readback. Original backend/core/legacy contracts remain
covered. Tests use temporary synthetic roots and no server, hardware or user audio.

Structural add/split/join/remove routing and active-layout migration, live UI
adoption, folder/settings integration and MIDI remain unfinished parity work.
No core policy module, GTK control, legacy serialized payload, version, PR2 branch,
release or merge is changed. Hardware/audio alignment, real-time, desktop and
performance acceptance remain untested.
