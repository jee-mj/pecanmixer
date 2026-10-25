# Reviewed reconnect after completed Rec edits

This bounded candidate starts at reviewed PR2 head `d0d8f23f`. It extends the
matching-capacity idle restore owner; corrected `91afa807` has independently
cleared and is adopted in this follow-up. It does not add channel growth/shrink, routed regrouping, recording
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

Initial `7f913a95` source-matched local checks pass 86 validator controls, 76 core, 230 backend,
496 feature GTK and 442 default GTK tests, native build/formatting, Cargo/ELF
checks, three headless fixtures and three controlled smokes. The 32-test restore
filter includes the 13 new cases. All ten legacy library tests and compilation
of all five legacy examples pass. Workspace host/target packages were rebuilt;
zero Node-tool attempts are recorded. Whitespace/privacy/scope and unchanged
core/backend/manifests/lockfile/version checks pass. No final failed check or
dependency/environment blocker remains. Hosted and independent owner review are
separate pending gates; no PR2 adoption is claimed by this candidate publication.

## Callback-observed recording skip correction

Independent review blocks `7f913a95`: a retirement reader callback can publish
recording ownership, causing retirement to return without latching the epoch
skip. Without another restore tick while active, joining that take then permits
a deferred restore. The service-callback case reproduces locally; the same gap
also reproduces through a receipt callback. Both incorrectly admit one restore
after join before correction, while the extra-active-tick control passes. This
is an injected callback-contract failure; native nonreentrant reader reachability
and a deployed recording failure are not demonstrated.

The narrow correction rechecks current recording ownership immediately after
retirement callbacks and records the same existing epoch skip. A fresh reader's
recording flag also records that skip before returning, without requiring an
extra cached owner tick. Namespace checks use current owner/epoch presentation,
and skip updates cannot move the epoch backward. No active reload policy,
retirement eligibility, backend API, layout support or recording action changes.
Original work/receipt and joined-child diagnostics remain retained.

Four new synthetic checks cover the service and receipt callback regressions,
the unchanged extra-active-tick control and a recording flag seen only in fresh
readback. Corrected restore filters pass 36 tests in each feature mode; all 145
bus-owner tests and 11 preference tests, native build, formatting, whitespace
and source/privacy checks pass. Full initial suites are not rerun for this
correction. Core/backend/legacy, dependencies, lockfile and versions are unchanged.
No final failed check or environment blocker remains. Exact corrected hosted and
independent re-review are pending; no PR2 integration is claimed.

## Nested observation correction

Independent re-review blocks `8611149a`: a callback can publish active recording,
then publish its joined state before returning. A post-callback occupancy check
therefore misses an ownership observation already made by the owner. This nested
variant reproduces one unintended restore after join on `8611149a`; the simple
service/receipt cases remain fixed. Native reader reachability is still unproven.

The narrow correction records the existing epoch skip at the normal owner poll's
recording-observation/publication point, only for a restore-capable endpoint. The
latch survives nested active-to-joined observations and cannot affect a newer
epoch. Repeated observations do not replace an already decided restore status.
There is no generic callback framework, new active-recording policy or layout
capability. Pending/uncertain operations and their receipts remain retained.

The existing service-callback regression now also exercises the nested join and
successful recovery in a later epoch; no test functions or infrastructure were
added. Restore filters still pass 36 checks in each feature mode, plus all 145
bus-owner tests, 11 preference tests, native build, formatting and whitespace/
privacy/scope checks. Core/backend/legacy/dependencies/versions are unchanged.
No final failed check or environment blocker remains. Full initial suites are not
repeated; exact corrected hosted and independent re-review remain pending.

## Independent clearance and combined adoption

Independent re-review clears `91afa807`: all five unchanged probes pass,
including nested/service/receipt observations and later-epoch recovery, along
with 37 targeted review checks in each mode. Exact candidate run 40169219633
succeeds. The earlier callback correction did not cover the nested observation;
the final owner-observation latch does, without adding test functions/framework.

The three reviewed commits apply onto pure desktop-library head `068f90e1` with
one append-only documentation conflict. Both coverage records are preserved.
Runtime bus sources match `91afa807` exactly and desktop helper/export sources
match reviewed `79d2c9eb`; no runtime reconciliation or unreviewed desktop
worker/tray/CLI adoption is included.

Fresh combined checks pass 86 validator controls, 76 core, 230 backend, 526 feature
GTK and 472 default GTK tests, native build/formatting, Cargo/ELF audits, three
headless fixtures and three controlled smokes. Ten legacy library tests and all
five legacy examples compiled pass. Workspace host/target packages were rebuilt;
zero Node-tool attempts are recorded. Privacy/scope/whitespace and unchanged
versions/manifests/lockfile checks pass. No final failure or environment blocker
remains. Exact published-head hosted evidence is recorded separately in PR2.

Successful closed historical edits no longer permanently suppress legitimate
idle reconnect restore. Pending/failed/uncertain work remains owned, recorded
activity skips only its observed epoch, and later-epoch recovery still uses new
authority/current preferences. General layout restore and recording/reload policy,
desktop management-context persistence and physical/audio acceptance remain open.
