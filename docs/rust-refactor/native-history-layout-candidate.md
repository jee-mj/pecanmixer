# Same-frame history guard reconciliation candidate

Base: published PR2 `5ad62b1cd18e1ed4217275da74638b5f83e47b6a`.
This is a separate candidate for independent review, not adopted into PR2.

## Existing behavior and bounded change

Legacy `geschiedenis.ts` stores up to 200 action pairs and queues Undo/Redo with
250 ms spacing. `handelingen.ts` resolves current graph endpoints and rebuilds
the mixer for route replay. `mixer.ts` retains current bus definitions explicitly
so that replay can see them. Legacy does not perform the native registry-guard
pruning introduced earlier in this migration. This candidate changes neither
legacy history nor the remaining native queued-input parity gap.

Native history already validates both forward/inverse dependencies, route
selectors and registry generations when accepting a newer graph. A changed
logical bus configuration can be accepted without a graph revision, but the
history reducer deliberately ignores equal/older graph observations. Replaying
the same NativeGraph is therefore not a reconciliation operation. On the base,
the new tests retain a stale Rec step in Undo `(2, 0)` or Redo `(1, 1)` instead
of preserving only the unrelated mute `(1, 0)`.

The existing guard-retention block is factored into one private helper. New
pure `Controller::reconcile_buses(&HistoryGraph, BusConfig)` requires idle core
history, valid definitions and the same already accepted native owner/stamp/Arc.
It updates the logical configuration and applies exactly those existing guard
rules. It does not replace the retained witness, allocate a ticket, call command
planning, complete a pending step or publish/invent a graph revision.

`Session::adopt_buses` keeps its existing equal-configuration no-op and shared
pending/uncertain/volume/external gates. For a changed accepted configuration it
uses its retained current witness, calls the pure reconciliation method, then
rotates the existing history activation context. Unaffected steps retain order;
changed Rec selectors disappear immediately. No backend/cache lookup, inbox
consumer, timer, audio command or UI widget is added.

This scope is ordinary configuration adoption only. Explicit recovery flow is
unchanged. In-flight uncertainty retains its existing hold; a quiescent failed
report is not reinterpreted as a new hold or success. No retry, rollback, queued
action, graph Undo for local settings or active structural-edit policy is added.

## Focused synthetic evidence

Both same-stamp Undo/Redo regressions fail unchanged on the base and pass after
the change. Six Session/widget cases in both feature modes cover:

- Affected Undo and Redo are reconciled without a graph event, changed stamp/Arc
  or outgoing command; forgotten endpoint caches do not invalidate the retained
  accepted witness.
- Unrelated history remains usable; unchanged definitions, append and gain/mute/
  arming metadata preserve still-valid steps.
- Pending commands, actual retained in-progress uncertainty, volume gestures and
  external leases keep their existing gates and actual receipts. Invalid
  configuration and copied frames fail without mutating history.
- A retired Undo button cannot act on the newly exposed older entry; a fresh
  action can still undo that preserved mute on the same frame.

Three additional core checks verify authority rejection, no mutation on invalid
input, pending-work preservation, graph identity and unchanged ticket allocation.
All 79 core and 240 backend tests pass. Existing focused checks also pass 151
bus-owner tests and 41 history/native-input tests with all features. Filters
overlap. Native build, scoped Rust formatting and whitespace checks pass.
The in-progress fixture uses nonzero in-progress evidence to exercise the actual
existing Session hold; it does not assume every quiescent Unconfirmed status
holds admission.

Full GTK/legacy suites are not repeated for this candidate. Its automatic hosted
CI and independent review remain separate receipts. No backend/service/DTO,
startup, settings schema, dependencies or version changes are included. All
fixtures are authored public-project data and synthetic registry publications;
no PipeWire service, real recording, user profile or physical desktop is operated.
Hardware/audio/performance acceptance, partial repair, general saved-layout
restoration, queued input and active structural policy remain separate gates.
