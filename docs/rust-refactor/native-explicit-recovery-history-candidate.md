# Explicit recovery history reconciliation candidate

Base: reviewed PR2 `6d1703acdd9b20b8c0f43fa1016e922177ea1457`.
This is a separate candidate pending independent review, not a PR2 adoption or
claim that the full JavaScript/native port is complete.

## Concrete missing behavior

Legacy `src/geschiedenis.ts` retains action pairs and queues Undo/Redo.
`src/handelingen.ts` resolves current endpoints and rebuilds the mixer with
current bus definitions for route replay. Legacy does not implement the native
registry-guard pruning added earlier in this migration. That existing native
contract already rechecks both forward/inverse dependencies on newer graph
events and ordinary same-frame configuration adoption; this changes no policy
and does not port the remaining JavaScript queued-input behavior.

`Session::recover_request` previously assigned `history.buses` directly after
authenticated inspection. It bypassed that reconciliation. Normal adoption then
saw equal definitions and returned, leaving a stale Rec entry visible in Undo or
Redo until another graph event. Its existing guard rejected replay, so the gap
is stale/unusable history presentation, not demonstrated unsafe routing.

Two exact same-frame regressions fail on the base: changed Rec Undo remains
`(2, 0)` and changed Rec Redo remains `(1, 1)` instead of preserving only the
unrelated mute `(1, 0)`. Unchanged/append/level-metadata controls and existing
pending/authority/input-gate controls pass on that same base.

## Minimal correction

The direct assignment is replaced with the existing
`Controller::reconcile_buses(&HistoryGraph, BusConfig)` call. It receives the
already inspected current native witness and reuses exactly the existing guard
rules. Both existing recovery-readiness checks, current-frame owner/stamp/Arc
authority, local execution closure, shared-work gates and activation-context
rotation remain unchanged. Reconciliation must succeed before uncertainty is
cleared; rejection does not discard the original hold or receipt.

Affected Rec selectors disappear immediately. Valid Undo/Redo order survives.
There is no graph publication/invented revision, new ticket, audio command,
failed-prefix history, success upgrade, automatic rollback/retry, new observer,
timer, backend API or settings change. The original frozen failure report and
Unconfirmed status remain historical after explicit recovery.

## Focused synthetic evidence

Four authored cases reuse the existing history/fixture helpers. Their source
checksum is unchanged before/after: two regressions fail and two controls pass
on the base; all four pass after correction in both feature modes. They verify:

- Immediate pruning of affected Undo and Redo on the same stamp/Arc, including
  an ordinary equal-configuration adoption afterward, without a graph event.
- Preserved unrelated mute replay, valid unchanged/append/level-metadata Undo/
  Redo ordering, exact next ticket and unchanged actual failed receipt.
- Open local execution, external lease, active gesture, pending command,
  invalid configuration and unavailable fresh witness still reject recovery
  without mutating history or the original failure/hold.

All 79 core tests and 30 existing/new Session checks in each feature mode pass.
All-feature filters additionally pass 54 history/native-input checks and 20
routing-owner checks. Native build, scoped formatting/whitespace, unchanged test
checksum and source/privacy/scope checks pass. Filters overlap; their counts must
not be added as distinct tests. There are no final failed checks or environment/
dependency blockers. Full GTK/backend/legacy suites are not repeated for this
minimal Session-only change; exact candidate hosted CI and independent review
remain separate pending gates.

Only one existing Session method, test registration/new fixture tests and this
note change. Core/backend/legacy source, manifests/lockfile, dependencies,
settings schema and versions remain unchanged. No real PipeWire service, audio,
user profile, physical desktop, performance run or private device data is used.
General repair, large-reset batching, queued input and active-recording structural
policy remain separate work. PR2 is left unchanged during candidate preparation.
