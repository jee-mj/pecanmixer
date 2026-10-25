# Native history identity candidate

This library/backend candidate protects retained undo/redo steps from registry
object replacement. It does not enable GTK history controls or replace the
legacy JavaScript history. All examples and tests use synthetic public fixtures.

## Existing behavior and the gap

`src/geschiedenis.ts` retains at most 200 forward/inverse closures. Replay reads
the latest graph, with a 250 ms queue delay. `src/handelingen.ts` captures port
pairs rather than link IDs: recreated links receive new IDs. External removals
leave replay doing whatever remains possible. Volume gestures record their
before/after values once after application; route actions re-resolve strip keys.

The Rust controller already replaced the timer with command results and observed
graph stamps, but retained numeric node/port IDs and strip keys. Binding a replay
to the latest graph did not prove those IDs still denoted the original objects.
Node serial comparison alone cannot close this gap: port DTOs have no serial,
and a remove/add cycle can be coalesced into two identical published graphs.

## Integration contract

- `Graaf::history_graph(&Stamped<Arc<Momentopname>>)` returns
  `Result<pecan_core::history_identity::HistoryGraph, AdmissionError>`. It looks
  up the exact stamp and graph Arc in the existing 64-publication native origin
  cache, then copies that publication's node/port generations. Copies, another
  owner's graphs and evicted publications return `OriginUnavailable`.
- Feed each accepted native frame as `history::Input::NativeGraph(witness)`.
  `HistoryGraph::graph()` returns the exact owned graph for presentation/planning.
  `HistoryGraph::from_registry` is a trusted backend/test-producer constructor;
  generation maps must come from registry lifecycle events, never DTO equality.
- `Input::Execute` and `Input::RecordVolume` capture dependencies from the
  controller's accepted witness. Initial UI intents, including an already-applied
  volume gesture, must already be bound to that original observed object by the
  UI adapter. Supplying stale before/after values under a replacement's current
  witness is not an authorized way to record a gesture.
- Dispatch returned plans with the unchanged `bind_history_plan` and
  `dispatch_native` APIs, using the controller's exact source graph. Admission
  and local execution remain progress, not completion. `HistoryResults` and the
  existing graph-at-observation gate remain unchanged.
- `Input::Graph` still supports observation/forward planning compatibility. It
  cannot authorize undo/redo; such replay returns `IdentityUnavailable`. Switching
  between witnessed/unwitnessed frames clears retained history and pending state.
  If a witness is unavailable, pause history or supply the accepted plain graph
  to invalidate it; never synthesize generations or stamp old actions anew.
- Use one native owner per controller; recreate the controller/result bridge
  when replacing that owner. Normal stale/out-of-order frames remain ignored.

Each step retains only its original generation dependencies and owner identity,
not an old graph or origin-cache slot. Ordinary revisions and unrelated object
replacement preserve the step. Missing/replaced dependencies prune affected
steps from both stacks and invalidate affected pending work. An affected batch
is discarded as one step; unrelated earlier steps remain available. A replay
cannot expand onto new objects introduced by a route/reset planner. This is
deliberately stricter than the JavaScript behavior after partial target removal.
Target strings and route keys also retain their original resolutions. A batch
cannot silently redirect one selector to a different object merely because that
object was already a dependency of another member. Existing first-match route
key resolution is preserved; this does not make colliding strip keys unique.

Selector witnesses include explicit `Target` actions, currently generated
`ZetDoel` commands and potential serial selectors from both target-capable
branches of a `Route`. A link-only or no-op plan can later become a target plan
without changing the witness. Potential selectors are limited to the planner's
stream/targetable-strip capabilities; hardware-only routes and non-targetable
channel pairs do not acquire unrelated serial dependencies. Every initially
matching node's registry generation is retained, including potential matches
absent from the initial command list.

This closes the batch case where a route to serial `2020` and a mute on node 15
were captured together, then node 15 acquired the name `2020` without any
generation change. The old generation union alone allowed undo/redo to retain
the selector after its resolution changed. The corrected witness invalidates
that step. Stable link/target transitions and switching between a route's two
target-capable branches remain usable. This does not change resolver priority
or make an initially ambiguous address unique; it retains the observed match
set and rejects changes. Future planner target capabilities must remain covered
by the potential-selector witness and its transition regressions.

Link generations are excluded: disconnect replay resolves the latest link ID
from the original, still-identical endpoint pair. Connect no-ops still progress
locally when the accepted graph already proves the pair exists; native commands
reported as observed no-ops still require their newer published observation.

Invalidating history does not cancel or roll back effects already dispatched.
A late result for invalidated pending work returns `UnknownTicket` without
altering a new pending operation; adapters should ignore that stale result.
The existing delivery watermark continues suppressing older repeated receipts.
Rejected or unconfirmed native results never invent successful undo/redo.

## Validation and limits

Focused core tests cover node/port/owner replacement, selective pruning, ordinary
revisions, undo/redo, recreated links, routes/batches, target name/serial reuse,
missing witnesses, reconnect, no-op progress and delayed receipts. Backend tests
use the actual production registry-generation hooks and admission queue, without
starting a server. They verify identical coalesced port replacement, replacement
after undo planning, origin authentication/eviction, and retained usable history
after 200 publications with the old graph released. The existing 200-step limit,
64-origin cache and native queue/receipt bounds remain in force.

Live history remains disabled until the native owner, UI gesture binding,
witness delivery and receipt handling are independently reviewed and wired.
No GTK controls, serialized payloads, legacy JS behavior, recorder lifecycle,
settings, dependency versions or application versions change here. These tests
make no hardware, audio, desktop-input or performance acceptance claim.

Cloud checks for this candidate: `cargo test -p pecan-core -p pecan-pipewire
-p pecanmixer --lib --tests --locked` passes 64 core, 58 backend and 10 legacy
tests. `cargo build -p pecan-ui-gtk --locked` and
`cargo check -p pecanmixer --examples --locked` pass. There are 22 identity-specific
core tests (including eight selector regressions) and five backend regressions.
Two selector regressions first reproduced the stale generated/potential-selector
failure before the correction. GTK widget/input tests were not rerun for this
backend/core-only change.
