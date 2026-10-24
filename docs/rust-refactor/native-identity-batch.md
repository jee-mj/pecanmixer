# Native history and capture identity batch (draft)

This increment follows the reviewed UI source batch at
`8add3a711f9497c4bb13ecaf8eb2ec8ef493959c`. It adds independently reviewed retained
history identities/selector correction and backend capture identity retention.
**No production native owner, controls, undo, recording or capture startup is
enabled.** Legacy JS/history/IPC remains unchanged; full functional parity and
cutover remain unfinished.

## Reviewed scope

History combines the complete candidate at
`afc1d1f2d2fa68385b75ca7a1e70e3805e63a3e0`, including its correction to the reproduced
generated-selector bug. Registry witnesses bind retained steps to the native
owner, epoch and original node/port generations. Explicit, generated and potential
route target strings retain their observed match sets and matching generations.
A batch cannot adopt a different target merely because that object was already
another member's dependency. Normal link/target planner transitions, recreated
link IDs, hardware-only routes and unaffected history remain usable.

`Graaf::history_graph(&origin)` authenticates the exact stamp/graph Arc against
the existing 64-origin cache. Feed successful witnesses through
`history::Input::NativeGraph`; plain graphs cannot authorize replay. Steps retain
only original dependencies, not old graph Arcs/cache entries. Missing/replaced
dependencies prune affected undo/redo/pending state. History remains bounded to
200 steps. Reconnect, owner changes and witnessed/unwitnessed transitions clear
history. Recreate the result bridge with the controller when replacing an owner.

Capture source is the reviewed
`280c669ee8af408b259a19b9712060fa6ad66ea6`. It retains a handle only while node serial,
sink direction, ordered channels and selected node/port generations match.
Invalid handles drop before replacement construction. A failed constructor leaves
the source absent from harvesting; another structural update can retry. Ordinary
source values, unrelated ports/registry changes and equivalent capture
configuration retain the handle. Session cleanup drops captures before reconnect.
This is local handle ownership, not a server teardown acknowledgement.

The backend overlaps merged automatically. Core/history source matches the
reviewed history candidate; shared backend files differ from it by exactly the
reviewed capture delta. No hand-written reconciliation or novel backend logic
was added. All UI, main startup, legacy, dependency/lockfile and version source
remains unchanged by this increment.

## Evidence

Independent history review reran the original retargeting probe: the old candidate
retained a step whose generated selector could resolve to synthetic node 15
instead of node 20; the correction rejects it. Twelve independent probes pass,
including ordinary input/output link-target transitions, hardware-only controls,
selective replacement pruning, recreated links, no-op/delayed receipts, bounded
retention and reconnect. The isolated candidate's 64 core, 58 backend and 10
legacy tests pass.

Independent capture review passed nine additional probes using the unchanged
selector/reconciler with synthetic handles: serial/direction/generation changes,
equal-named channel reordering, unselected ports, drop-before-create, failed
creation/absence, ordinary retention and cleanup/reconnect. The isolated capture
candidate passed 42 core, 62 backend and 10 legacy tests. Neither review accessed
an audio server, user device/profile or runtime capture.

Final combined checks:

| Check | Result |
|---|---|
| Core/backend Rust tests | PASS: 64 core + 67 backend |
| Legacy library tests | PASS: 10 (141 total non-GTK tests) |
| GTK-package tests under Xvfb/session bus, fatal criticals | PASS: 59 each default/accessibility mode; 64 with all five XTest cases |
| Native GTK build / five existing examples | PASS |
| Reviewed-source, privacy/scope and whitespace audit | PASS; synthetic data only; no dependency/lockfile/version or startup change |

An initial display run failed before GTK initialization because of an incorrect
Xvfb executable path. Corrected display runs pass in all modes. This was a test
launcher error, not a source regression.

## Integration and acceptance limits

A future native shell must deliver exact owner-produced frames/witnesses, bind
initial UI intent and applied-gesture before/after values to their original
objects, and wait for actual observed receipt plus graph progress. A missing
witness must pause/downgrade history, not synthesize generations. The trusted
producer constructor does not authenticate arbitrary caller-supplied metadata.
Initially ambiguous addresses are not made unique; their match sets are retained.
Future planner target capabilities need corresponding potential-selector coverage.

Invalidating history does not cancel or roll back already dispatched effects.
Native batches remain ordered/non-atomic, and one claimed command may finish after
cancellation. Observed is a server-fed postcondition, not a causal transaction
acknowledgement. Unconfirmed outcomes may have effects. Legacy IPC retains its
pre-admission stale-intent limitation.

Peak DTOs still lack per-capture generations, frame sequence, timestamps and
detailed availability/errors. Identical replacements hidden by the graph DTO
cannot immediately reset a GTK binding's hold/clip identity. Missing captures
are omitted rather than invented silence; no peak event is emitted when the
entire harvest is empty. A local constructed/connected handle does not prove
server negotiation or audio delivery. The owning shell must retain the documented
meter ordering/binding/clear/deactivation responsibilities; this batch does not
magically provide availability or live audio acceptance.

Not run: live PipeWire/WirePlumber/hardware/restart/recording acceptance, physical
desktop/Wayland/cross-app/AT-SPI/Orca workflows, real-time/performance benchmarks,
crash/power-loss tests, release packaging or final license audit. No merge,
release, version bump or premature JS removal is included.
