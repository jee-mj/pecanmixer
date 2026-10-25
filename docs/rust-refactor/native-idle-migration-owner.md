# Idle migration owner candidate

This separate candidate starts at reviewed PR2 `a5875178` and awaits independent
review before publication to PR2. It mounts the existing staged backend migration
through the current BusOwner/Session admission path. No backend sequencer,
recording policy, transport controller, dependency or startup path is added.

Supported actions are idle routed split/join and idle remove/shrink. Unrouted
split/join and unrouted Add retain their existing idle-configuration path. Routed
Add has its own pending capability: enabling routed split/join never enables Add.
Opening, active, finalizing, unconfirmed or unreadable recording ownership cannot
admit structural work. Stop all and the existing common quit authority remain
available. Active-recording segmentation versus disabling edits is not decided.

`Service::migrate_idle(LayoutIntent)` is a narrow wrapper around
`Graaf::migrate_idle_bus`. The owner retains the real `MigrationReceipt`, shared
Session ticket/lease and exact edited graph Arc, configuration and membership
revisions. It recomputes the proposed configuration with the existing pure core
policy, scopes removal to the authenticated Rec node, and repeats current-source
checks after fresh preflight. It installs pending ownership before handoff and
polls injected/native receipt methods outside UI-state borrows, then rechecks the
original request before applying results. No command queue or route sequencer is
duplicated in the UI.

Progress retains distinct local Applied/NoOp, awaiting graph publication,
Observed, rejected, partial and unconfirmed states. The routing projection also
gates the new waiting phase. A terminal report needs zero local in-progress calls
and fresh owner readback before release. Busy/older inspection cannot substitute
a cached pre-completion configuration for the actual result. Proposed metadata is
never written into the owner in place of actual backend configuration.

Observed additionally requires the backend's observed stamp to be consumed (or
covered by a later accepted frame), the original registry generations of the Rec
node/surviving inputs/source endpoints, the surviving input mapping, and the
intended incoming endpoint pairs. Ordinary unrelated/coalesced revisions and
recreated link IDs are allowed. Replacement or a changed current result is shown
as current uncertainty while the original backend outcome remains unchanged.
This is a postcondition check over the core plan, not another staged executor.

The owner publishes the corresponding graph/configuration and tracked result to
the existing bus view before releasing the lease. A rejected view publication
cannot release ownership. Completed/released evidence stays historical across
later busy reads. Partial/unconfirmed work keeps the explicit inspection barrier;
reconciliation does not replay commands, synthesize rollback or invent successful
prefix history. Original reports, including readback, counters, resize and failure,
are retained in a bounded 16-report archive; existing failure retention remains
bounded. A new pending request never returns an older archived report as its own
current evidence. Common shutdown audits the real receipt rather than assuming
that a UI phase or missing report means completion.

Synthetic owner cases cover real widget handoff, split/join/remove dispatch,
original captures and proposals, routed Add exclusion, delayed/coalesced graph
and config delivery, busy/older inspection, rejected display publication,
replacement generations, partial/in-flight evidence, retained reconciliation,
reentrant polling/admission, stale preflight and active/quit gates. Existing
routing fixtures that intentionally supply an Add-capable adapter now explicitly
declare that separate capability; the native owner leaves it pending. Generic
terminal-request tests include Observed, while pending tests include the new
waiting phase.

No actual audio, user device identifiers/topology, profile, screenshots or runtime
captures are used. Backend protocol tests use their existing synthetic/fault/WAV
fixtures. Real PipeWire/recording/desktop/performance acceptance, causal server
acknowledgement, immediate configuration-driven history reconciliation and the
remaining transport/settings parity are not established by this candidate.

Final cloud checks pass: 67 focused bus/routing owner tests (nine new migration
cases); 345 default GTK tests; and `scripts/native/check.sh` with 86 validator
controls, 74 core, 196 backend and 388 all-feature GTK tests, formatting, native
build, Cargo/ELF audits, three headless fixtures and three GTK smokes. The Node
attempt log is empty. No final failing check or environment blocker remains.

The first broader run exposed two old routing fixtures that implicitly enabled
routed Add through the shared layout capability. Their explicit adapter support
is now declared separately, with their pending/reentry assertions unchanged. The
native owner's routed Add gate is independently asserted. Legacy unit/example
suites were not rerun for this GTK-only slice; their sources and dependencies are
unchanged. Hosted candidate checks and independent review are separate pending
gates. No physical/hardware acceptance follows from these synthetic checks.
