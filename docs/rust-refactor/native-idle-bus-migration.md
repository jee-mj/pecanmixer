# Idle staged bus migration candidate

This backend-only candidate adds split, join and remove operations to the native
contract. It does not wire GTK controls, change legacy serialized IPC, or migrate
active recording layouts. Active and finalizing native ownership, legacy
recording, and an arming operation reject structural admission. Active layout
parity remains unfinished; no take segmentation policy is implemented.

## Caller contract

`Graaf::migrate_idle_bus(LayoutIntent)` accepts a ticket, the exact retained native
graph origin, expected `ConfigRevision` and `RecordingRevision`, and
`LayoutAction::{Split, Join, Remove} { bus }`. Use `recording_snapshot()` for the
two revisions and the original graph publication for the origin. Numeric IDs
restamped against a newer graph are not equivalent. The config readback's local
node selects the owned Rec instance; another same-name node is never selected by
the pure planner. Original node/port generations and source identities remain
bound throughout the operation.

Admission returns `MigrationReceipt`, not success. `try_report()` does not wait;
`wait_timeout()` is an off-UI polling convenience with a caller-supplied bound.
Timeout of that convenience leaves the receipt pending. Keep the receipt through
a decided result and reconcile the current graph/configuration before retrying a
failed plan. `cancel_bus_migration(&receipt)` is idempotent for a decided receipt
from the same owner. Cancellation is not rollback.

The remove sequence is:

1. Connect the desired endpoint pairs on the existing bounded FIFO.
2. Observe all desired pairs together in the same-epoch graph mirror, with a
   publication after the last real connect submission.
3. Disconnect obsolete pairs. Initial/desired overlap is retained; recreated
   obsolete links are handled using their current ID and generation.
4. Observe obsolete pairs absent and desired pairs still present, after the last
   real disconnect submission.
5. Apply the typed channel shrink, gains and transport options; verify actual
   local readback. Retain `ChannelResizeResult`, including partial allocation.
6. Observe surviving input identities and the removed tail absent. Only then
   publish the proposed logical configuration as complete.

Split/join do not change physical ports. They complete from matching local
gain/options readback, without waiting for a graph change that need not happen.
Removing the sole bus is the existing policy's no-op and preserves the revision.
The pure bus policy still supplies layout/state transformations. Its scoped graph
contains only the exact owner's captured audio inputs and links.

`MigrationReport` retains phase, source stamp, expected revisions, proposed and
actual configurations, attempted/submitted/no-op counts, in-progress flag,
observation stamps, typed resize/readback, cancellation and failure. Outcomes:

- `AppliedLocally` / `NoOpLocally`: local readback for an unchanged allocation.
- `Observed`: the local readback and staged graph predicates were satisfied.
- `Rejected`: failure before any known effects.
- `PartiallyApplied`: local configuration effects/readback exist, but the full
  operation failed or was cancelled. Logical metadata may be unavailable.
- `Unconfirmed`: submitted/possibly executed routing lacks a complete observed
  result. Dropping unfinished worker ownership also reports unconfirmed.

These are graph-mirror predicates, not causal server transaction acknowledgements,
audio arrival, or hardware acceptance. There is no automatic rollback: other
clients may change the graph. New unexpected affected routes, lost desired pairs,
identity replacement, changed revisions and disconnect stop destructive progress.
An already claimed callback may finish after cancellation; the receipt stays
cancelling until its actual local result is retained. Stop-all, legacy stop,
disconnect and backend shutdown fence later stages and discard queued migration
continuations atomically. Unrelated graph commands retain FIFO dispatch behavior;
overlapping mutations are rejected while the migration owns its scope.

## Bounds and unfinished integration

One migration is owned at a time. Admission retains at most 256 initial owned
inbound links; routing is limited to 512 attempts, uses at most one queued
continuation, and shares the existing 32-command dispatch budget. The normal FIFO
and reserved stop capacity are unchanged. Each graph barrier has a two-second
limit, with a 30-second total lifetime. Excess routes, saturation and elapsed
barriers produce explicit errors; these candidate limits are not permanent
product restrictions. No writer, queue or audio callback thread is added.

Synthetic tests cover multiple Rec instances, endpoint overlap, staged publication,
recreated links, identity replacement, conflicts, stale origins/revisions,
partial resize, readback mismatch, stop/cancel/disconnect during claimed work,
timeouts, non-audio name collisions, route limits and 100,000 bounded polls. No
PipeWire service, user hardware, real audio, latency or performance acceptance was
used. The legacy adapter remains supported and unchanged outside overlapping
opt-in native work.

This candidate descends from the separate arming candidate `8a33878`. That arming
candidate has an independently reported final-options/write-failure blocker and
must not be integrated as cleared. The correction is a separate priority slice;
this idle implementation rejects the active path and does not resolve that bug.

## Cloud checks

`cargo test -p pecan-core -p pecan-pipewire -p pecanmixer --lib --tests --locked`
passes 64 core, 178 backend (22 new migration regressions) and 10 legacy tests.
The GTK binary builds, all five legacy examples check, and changed-file rustfmt
and whitespace checks pass. One earlier full parallel run hit the existing
multitake test's snapshot/admission race (`StaleRecording` after a stopped writer
joined); subsequent full runs passed. This candidate does not change that test
or weaken stale-membership rejection. Synthetic boundedness probes are not memory
or real-time performance acceptance.
