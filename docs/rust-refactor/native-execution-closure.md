# Receipt-local native execution closure

Candidate based on `fafda497c532cf6afd42d4cd6528920302ee62bf`. This adds
backend execution evidence only. Owner recovery, large reset sequencing and
failed-prefix undo policy are separate work. No UI, legacy IPC, serialized DTO,
recording, dependency or version changes are included.

## Caller contract

`NativeReceipt::execution() -> NativeExecution` is a nonblocking atomic poll on
the original receipt. Its states are:

- `Pending`: another queued call remains possible, or a claimed call has not
  returned through local validation, execution and result handling.
- `Closed`: no further dispatch-side validation/executor callbacks can occur for
  this receipt. Graph observation/reporting can continue afterward. This state
  is permanent and shared by receipt clones. Another receipt
  with the same ticket/source does not share its closure state.

`Closed` does **not** establish rollback, server acceptance/convergence, graph
observation, audio arrival, recording finalization or global backend shutdown.
Previously submitted RPCs can still affect the server. A stuck local callback
remains `Pending`; polling neither waits for it nor imposes a new deadline.

`NativeReport` and terminal outcomes remain unchanged. In particular,
`Counts::in_progress` remains the count frozen when the report was produced.
After cancellation it can remain nonzero even when execution later closes. Read
the separate closure poll instead of rewriting or treating that historical
count as current. Closure and report reads are separate, not an atomic combined
snapshot. Callers must retain the actual receipt, never reconstruct closure from
a report, ticket, new graph or elapsed time.

Closure can precede a terminal outcome: after all calls return the request can
still await graph observation. An admitted empty request is immediately closed
but still needs the existing fresh mirror publication for `ObservedNoOp`.
Backend destruction seals execution without fabricating a terminal report.

## Backend implementation

`NativeReporter::claim_execution() -> Option<NativeExecutionLease>` acquires one
non-cloneable lease before callbacks, under the existing dispatch/removal lock.
`seal_execution()` permanently revokes future claims. A seal with outstanding
leases stays pending; the last lease's drop closes it, including during unwind.
Producer implementations must explicitly seal when no queued calls remain;
setting a terminal report alone does not prove execution closure. Existing
synthetic reporters that only set reports therefore conservatively stay pending.

The production worker carries the lease through no-op checks, live validation,
execution and result handling outside its queue lock. Cancellation, timeout,
disconnect and removal seal the work. An unwinding callback cancels only that
batch's remaining queued commands, retaining an explicit unconfirmed result;
unrelated FIFO work survives. Once every command has returned, execution seals
without waiting for graph observation. Frozen outcomes are never upgraded after
late returns or unwind. Process abort/crash does not provide a completion result.

Accounting uses one atomic word per receipt (a seal bit plus at most 32 leases),
with no completion queue, retained graph, worker reference or reaper thread.
The existing 32-command/32-in-flight, FIFO, per-tick and origin-cache limits are
unchanged. Production dispatch remains on its existing single worker. Retained
receipts contain only their bounded report and closure state.

## Synthetic checks

Focused regressions cover receipt clones/ticket reuse, monotonic sealing, lease
capacity and unwind; held calls cancelled by stop/disconnect/timeout; polling
while dispatch is locked; removal and capacity release before a late return;
queued tails across ticks; validation/no-op/executor unwind; preservation of
unrelated work; empty requests, no-ops, rejection, expiry and backend drop. The
existing retained-receipt/origin-eviction probe also requires closed execution.

No PipeWire/WirePlumber session, real device, user audio, physical desktop,
performance or hardware acceptance is exercised or claimed.

Cloud checks: 74 core tests, 196 backend tests (including 21 focused native
command tests), 10 legacy library tests and a native GTK compile check passed.
The retained-receipt probe
checks 2,176 no-op requests plus origin eviction. No existing assertion was
removed or weakened. These are synthetic source/compatibility results, not
independent review or live acceptance.
