# Dynamic arming owner/UI candidate

This candidate starts at reviewed PR2 source `0ac53189`. It awaits independent
review before PR2 integration. Native audio, physical input, real-time performance
and desktop acceptance have not been tested.

The existing bus-row exclusive Arm/Disarm action and additive Armed checkbox use
`Graaf::arm_recordings` while an authenticated recording is active. The owner
recomputes the complete armed set with the existing pure bus policy, captures the
original graph Arc and exact configuration/membership revisions, and reserves the
same Session ticket/admission lease used by other configuration operations. Idle
arming keeps its existing idle-configuration path. Structural edits during
recording remain unavailable; no segmentation-versus-disable decision is made.

The owner installs its pending request before handoff, polls the real arming
receipt on its existing clock, correlates original request/revision identities,
and retains actual configuration, membership, removed/withheld IDs and child
receipts. Queued/Adding/ApplyingRemovals do not acknowledge completion. A decided
Applied/NoOp result is local arming completion only. Partial and Unconfirmed
outcomes retain the explicit Refresh/readback barrier, including across busy or
older inspections. A current authenticated inspection can reconcile the local
configuration without rewriting the original outcome or completing child writers.

Arming children can share the parent's ticket/source and have no writer/take IDs
at admission. `RecordingReceipt::same_request` compares original receipt identity
without polling, allocation or locking. UI retention, retirement and finalization
audit now use that identity, including when fresh readback wrappers refer to the
same native receipt. The existing bounded take/history retention is preserved.
Children survive omitted snapshots, busy reports, parent completion, Stop all
and disconnect; their later failures remain separate from parent success. Common
shutdown remains the sole finalization authority. Late child discovery cannot
clear an already requested Stop all. Parent partial/error diagnostics are retained
through explicit reconciliation and subsequent operations for the quit audit.

The existing opening/unknown-ownership start gates and application-finalization
admission gate remain in place. Structural edits still require an idle owner.
Live arming requires a captured explicit absolute recording root;
missing/busy/stale metadata does not synthesize a request. Stable-ID Stop/Stop all
and common quit remain available. The existing all-armed start is still only
membership-preflight checked, not an atomic membership compare-and-swap. No new
recorder, transport timer, selection authority, shutdown handle, command queue,
settings adapter or production entrypoint is introduced.

Validation uses generic synthetic graphs and injected UI receipts, plus the
existing backend temporary-WAV tests. The seven owner cases cover a real widget
handoff, exact source/revisions, pending/reentrant admission, stale captures,
same-source sibling retention before writer IDs, omitted/busy child reports,
late failures, partial-readback recovery, no-op versus file completion, reserved
stops/disconnect, invalid receipt identity, and the existing opening/structural/
finalization gates. Existing backend multi-addition coverage additionally checks
that same-source sibling native receipts remain distinct while clones compare
equal. The candidate changes no production backend execution policy.

Candidate cloud validation:

- 56 focused bus/routing owner tests, including seven new arming tests, passed.
- `scripts/native/check.sh` passed: 86 validator controls, 74 core tests across
  unit/fixture suites, 196 backend tests, 377 GTK feature tests, formatting,
  native build, Cargo/ELF closure audit, three headless fixtures and three GTK
  smokes. The runtime-guard attempt log is empty.
- 334 default-feature GTK tests passed.
- The retained legacy's 10 library tests and compilation of its five examples
  passed. No example or audio session was executed.
- Final local checks have no failing tests. Independent review and hosted
  candidate/PR-head evidence are separate gates; this record does not claim them.

Only synthetic injected UI receipts, existing generic graph fixtures and backend
fault/temporary-WAV fixtures were used. No user device names, topology, runtime
logs, profile, audio input or workstation data were accessed or published.

## Child-polling review correction

Independent review blocked original candidate `9c1c86dd`: its arming poll held a
mutable owner-state borrow while inspecting a child receipt. An injected child
that read `recording_status()` deterministically panicked with an already-borrowed
RefCell. This was an injected-contract defect, not an observed native/deployed
crash. Passing hosted validation on that original source did not cover reentry.

The correction obtains child reports without holding an owner-state borrow, then
checks the original request identity again before retaining results or releasing
its lease. Busy children keep their original handles. Two narrow regressions
cover reading the owner during child polling (available and busy child reports),
and reentry that completes the original operation and admits a new one. The old
poll cannot apply its result to the new operation or release the new lease. The
first regression fails before the correction with the reviewed borrow panic;
both pass afterward, together with all 58 focused bus/routing owner cases.

Correction validation: 58 focused owner tests, 336 default-feature GTK tests,
379 all-feature GTK tests, formatting and native build pass. Core/backend execution, legacy tests/examples,
closure audits and headless/smoke fixtures are unchanged and were not separately
repeated for this UI-only correction; their passing evidence above belongs to
`9c1c86dd`. Its hosted run `40149936652` also succeeded. Independent re-review and
corrected-head hosted checks remain required separately before PR2 integration.
