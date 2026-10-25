# Reconnect after completed Rec edits — owner candidate

This bounded candidate starts at reviewed PR2 head `d0d8f23f`. It extends the
matching-capacity idle restore owner; independent review is still required before
PR2 adoption. It does not add channel growth/shrink, routed regrouping, recording
reload policy or profile discovery. Backend/core contracts and legacy behavior
remain unchanged.

## Completed historical work

The previous owner could retire only its own successful automatic restore. A
completed user configuration or gain edit retained an old-epoch completion
snapshot, so current-epoch coherence could never authorize the next restore.
Both cases reproduce with zero restore admissions on `d0d8f23f`.

The candidate accepts only a strictly older epoch with the same configuration
owner, successful terminal Applied/NoOp/Observed evidence, released shared
ownership and completed specialized publication barriers. It keeps the existing
protocol distinctions:

- Generic configuration inspects the actual original ConfigReceipt outside any
  UI state borrow. Its ticket/revision, successful local outcome, connected
  logical readback and complete resize must agree; cancellation remains gated.
- Rec levels require an ended/released interaction with no in-flight dispatch,
  pending inspection or failed sample, plus its retained successful receipt.
- Migration retains its decided report and requires released publication, zero
  executing calls, matching source/configuration/membership and no cancellation.
- Arming retains its completed local decision and exact correlation. Independently
  retained child takes must already be joined/complete; their receipts are not
  removed by retiring the parent configuration operation.
- Prior successful automatic restores retain their existing receipt/publication
  proofs. Failed, rejected, cancelled, partial or unconfirmed outcomes are not
  silently upgraded to successful history.

The sole Session must be quiet, and the current accepted graph Arc/witness must
provide complete authenticated Rec inputs. The existing read-only service must
also return a fresh configuration/membership snapshot matching that presentation.
Busy inspection waits. Receipt and service callbacks run outside mutable UI
borrows; original presentation/request and all idle/ownership conditions are
checked again after callbacks. A replacement callback cannot authorize retirement
through an obsolete Origin. This adds no inbox consumer, timer or backend queue.

Retirement archives the complete original operation and receipt in a bounded
16-entry collection and uses existing protocol-specific report archives. No
receipt is rewritten or stamped into the new epoch. Retirement never releases
another lease, submits audio or replays an old command. It publishes fresh owner
authority and returns; the next existing tick captures the latest validated
preferences and obtains a new shared ticket through the existing restore path.

Waiting for an old pending operation does not consume a restore attempt for the
new epoch. It may become eligible only after actual successful closure. Seeing
recording/opening/finalizing/unknown ownership still skips automatic effects for
that epoch before retirement, so ending a take cannot turn that skip into a
deferred reload. Same-epoch user work is not automatically retired. Stop all,
retained failures and common Quit keep their existing authority.

Local completion and fresh inspection are not server convergence, causal audio
acknowledgement or recorder correctness. The new restore still uses the strong
original-generation backend API; these separate preflight reads are not atomic
transactions. Uncertain/failed work still requires explicit recovery. MIDI/clock
remain effectively off and saved values remain preserved.

## Synthetic evidence

Thirteen new GTK/controller cases cover completed config and gain regressions,
NoOp receipt identity, five pending/failure phases, shared-lease and busy-read
barriers, incomplete authoritative inputs, reentrant receipt publication, joined
arming children, late successful closure without epoch starvation, active-take
skip without deferred effects, completed migration retention, executing migration
calls and a 20-epoch bounded-history/ticket check. Both initial config/gain
regressions fail before correction and pass afterward. The restore filter now
passes 32 tests including existing controls.

A migration test initially assumed a particular first split button/key; it was
corrected to use the actual resulting logical key. A test-helper visibility error
was corrected without changing helper semantics. No production assertion or
existing regression was removed. All data is authored generic frames and
injected receipts; no user device, profile, capture, launcher or hardware service
is accessed. Source/build check counts and the exact candidate are returned for
independent review. Physical/audio/performance acceptance remains unrun.

Final source-matched local checks pass 86 validator controls, 76 core, 230 backend,
496 feature GTK and 442 default GTK tests, native build/formatting, Cargo/ELF
checks, three headless fixtures and three controlled smokes. The 32-test restore
filter includes the 13 new cases. All ten legacy library tests and compilation
of all five legacy examples pass. Workspace host/target packages were rebuilt;
zero Node-tool attempts are recorded. Whitespace/privacy/scope and unchanged
core/backend/manifests/lockfile/version checks pass. No final failed check or
dependency/environment blocker remains. Hosted and independent owner review are
separate pending gates; no PR2 adoption is claimed by this candidate publication.
