# Bounded idle Rec restore candidate

This owner candidate starts at reviewed public head `f9fc8e7c`. It consumes the
independently reviewed full-config origin API `9e803bba`; the owner integration
itself still needs independent review before PR2 adoption. The goal remains full
legacy functional parity. This is a deliberately bounded restore prerequisite,
not general saved-layout migration or a recording/reload policy decision.

## Supported path and source

The existing shell tick processes the single workspace/inbox and then settings
results. Only while lifetime admission remains open does it offer the latest
validated preferences to the existing BusOwner. No backend, window, timer,
settings worker, ticket counter or history controller is added.

Memory defaults and a valid or missing initial explicit settings file can supply
Rec layout/dB/mute/arming. Initial load must complete first. Corrupt, unsupported,
failed or protected input never supplies fallback audio configuration. Explicit
Reload/recovery disables automatic restoration for the remainder of that shell;
no external reload-during-recording policy is inferred. Accepted Rec edits still
use existing writeback; restore never invokes that preference sink. Stored MIDI,
clock and display values remain untouched while effective MIDI/clock/start-stop
are masked off. It neither starts recording nor chooses/imports a profile.

Native startup already creates a two-channel local Rec filter. Restore waits for
its original authenticated node and complete published `rec_N` input prefix.
This candidate only supports a desired total channel count equal to actual local
capacity. A different mono/stereo grouping can initialize unknown metadata or
regroup an unrouted matching-capacity filter. Existing routed layouts must keep
their grouping. Channel growth, shrink and routed regrouping are explicitly
reported as requiring a separate migration; no commands are sent for them. It
does not silently initialize default stereo metadata over saved mono intent.

## Admission, completion and recovery

A bounded epoch latch permits one automatic admission attempt per configuration
owner/epoch. An admitted request keeps its original Session HistoryGraph and CAS
through `Service::configure_from` to `Graaf::configure_idle_from`; the UI binding
retains original registry identities for subsequent readback validation. There
is no newer-witness recapture or weak configure/legacy fallback.

The owner reserves the existing shared Session lease and ticket. Its actual
ConfigReceipt is polled outside mutable UI borrows, correlated to the original
request/ticket/revision and retained. An admission error remains an admission
error; no synthetic completion receipt is manufactured. There is no retry after
admission failure, partial or unconfirmed completion in the same epoch.

Applied/NoOp are local evidence. After terminal delivery, a subsequent readable
configuration inspection must cover the completion and match the original
published input identities and desired logical/local values. NoOp/rejection
also need an inspection after their report, even when revision is unchanged.
Genuinely older readback waits. Busy inspection retains the request and shared
lease. A newer incompatible snapshot, replacement or lost epoch becomes retained
uncertainty, never an impossible old-success wait or automatic replay. Terminal
presentation must be accepted before the shared lease is released. Actual reports
are archived in a bounded 16-entry history; partial/unconfirmed diagnostics also
remain in the existing common-quit error collection.

A previously accepted, released automatic restore can be retired on a new epoch,
with its report archived before fresh authority is published. The next tick may
restore once for that new epoch. Uncertain outcomes and retained prior user
operations still require existing explicit Refresh/reconciliation. A user bus
operation or known recording already owning the epoch causes automatic restore
to be skipped rather than applied after that activity ends. Opening, finalizing,
unknown ownership and active recording are never overridden. Stop all and common
Quit retain their existing authority; admission closing prevents another restore
while actual receipts survive through finalization.

Same-capacity original-input presentation is not server convergence, causal
acknowledgement, audio correctness or proof of meter freshness. Restore creates
no graph Undo step and never invents rollback or successful-prefix history.

## Synthetic verification

The initial twelve authored restore tests cover original exact Arc/witness admission,
MIDI/clock masking without writeback, 1,000 repeated ticks, NoOp plus busy reads,
delayed old readback versus superseding configuration, numeric-ID generation
replacement, ordinary revisions retaining original authority, new-epoch restore,
unknown-metadata mono grouping, incomplete prefix and unsupported count, active
recording/prior user ownership, service/receipt reentry, valid/missing/corrupt/
future initial files and explicit reload gating. A real shell/settings-worker test
uses one explicit synthetic temporary directory and injected graph/config receipts
for initial load, repeated activation, pending work during Quit and unchanged
settings bytes. Fixture mode constructs no native audio service or launcher.

An initial test expected Arm to unconditionally enable an already armed bus;
it was corrected to assert the original toggle's saved value. An initial shutdown
fixture announced backend join before completing its claimed synthetic operation;
it was corrected to complete that operation before the joined handle, preserving
the existing explicit failure semantics. Test-only constructor/visibility errors
were corrected before final validation. No production safety assertion was
removed. Final source-matched check counts and exact candidate head are returned
for independent review; hardware/audio/physical desktop/performance are unrun.

## Remaining dependencies

- Independent owner review and combined PR2 integration; helper inclusion alone
  does not enable this candidate in the published application.
- General saved layouts need separately reviewed initialization/growth/shrink and
  routed migration planning through existing backend barriers. No new backend
  sequencer or routed Add is supplied here.
- Restore after retained prior user work and explicit reload/recovery remain gated.
  Active-recording structural segmentation versus disabling edits remains an
  explicit user-decision gate. No automatic recording or profile import exists.
- Clock effects, larger route resets, queued input, history reconciliation,
  autostart/distribution and real desktop/audio acceptance remain separate parity
  work. Legacy JS/Tauri, versions and dependencies are preserved.

Initial `c6301335` local checks pass 86 validator controls, 76 core, 230 backend, 480 feature
GTK and 426 default GTK tests, native build/formatting, Cargo/ELF audits, three
headless fixtures and three controlled GTK smokes. The focused restore filter
passes 16 checks (12 new cases and four existing controls). All ten legacy
library tests and compilation of all five legacy examples pass. Workspace host
and target packages were rebuilt. Zero Node attempts, whitespace/scope/privacy
and unchanged core/backend/manifests/lockfile/version checks pass. No final
failed check or dependency/environment blocker remains. Candidate hosted and
independent-owner review remain separate pending gates.

## Owned-filter route correction

Independent review blocks `c6301335`: the general `has_routes` helper includes
foreign Rec instances, so a foreign cable incorrectly consumes the owned filter's
automatic restore attempt. Both known- and unknown-metadata cases reproduce with
zero admissions before correction. The narrow fix checks only cables touching
the captured `ConfigSnapshot.local.node` in the original witnessed graph. Backend
route guards and owned routed-layout gates remain unchanged; no layout capability
is added.

Three new synthetic GTK cases pass afterward: foreign routing admits the matching
owned capacity with known/unknown metadata; owned routed regrouping still admits
nothing in either case; known unchanged grouping still permits gain restoration.
The restore filter passes 19 checks in both default and feature modes, and all 11
preferences checks, native build, formatting, whitespace and scope/privacy checks
pass. Core/backend, legacy source, dependencies, versions and other owner paths
are unchanged. The full initial suites are not rerun for this narrow correction;
exact corrected-candidate hosted results and independent re-review remain pending.
No PR2 publication or hardware/audio acceptance is implied.
