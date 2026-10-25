# Reviewed explicit routing owner recovery

Reviewed candidate `0a8cd86b071c49cfcc5a8ea20b2e99d9c6c250af`, descended from
closure checkpoint `480c87d`, adds a narrow explicit owner recovery action.
Independent review clears 19 targeted Session/XTest cases plus three extra probes
for retired controls, current-origin recovery and repeated failed inspections.
The probes preserve the original diagnostic and recording state. Integration
retains the exact reviewed program source without reconciliation changes.
Large reset batching, immediate configuration-driven history reconciliation,
arming/migration adapters and active-recording structural policy are unchanged.

## Original receipt and fresh inspection

`Routes` retains its original token/request for unconfirmed and potentially
partial rejected outcomes, including terminal reports with zero historical
in-progress counts. It installs one request-bound inspection callback, rather
than replacing it on every tick. A per-owner guard rejects nested recovery, and
the original request is checked again after inspection. A retired callback cannot recover another
request. `Request` has a weak Session identity; it does not retain an owner,
worker or backend lifetime through a cycle.

The explicit action checks the original receipt's atomic `Closed` evidence. It
requires a terminal rejected/unconfirmed outcome, a connected exact accepted
graph, no newer/other pending history, gesture or configuration ownership, and
the same matching uncertainty hold if one exists. Native reports and frozen
counts are never rewritten. Closure or ordinary polling does not invoke recovery.

BusOwner performs read-only configuration inspection using its existing
service/current presentation. Unknown metadata, a busy read, a changed owner or
revision, separate configuration uncertainty, finalization and reentrant
presentation changes reject recovery. It does not use the idle-only structural
edit gate, mutate configuration, choose a destination or stop a recording.

The Session also inspects the backend's current owned graph and its registry
witness. Native inspection compares the exact current Arc/stamp before and after
`history_graph`; the fixture checks its latest published frame. A merely retained
origin, evicted witness, copied frame or foreign Session request is insufficient.
An unsupported endpoint fails closed by default. This adds no inbox drain,
publication factory, native ticket or command. The current frame may be unchanged:
fresh inspection does not require or manufacture a graph revision.

These are current published/local readbacks, not a server round-trip or an atomic
graph/configuration transaction. Further server messages can still arrive.
Existing native admission/execution identity validation remains authoritative.
Local execution closure does not prove server convergence, rollback, observation,
audio arrival or recorder finalization.

## Local recovery and input

After inspection, the existing routing `reconciled` boundary refreshes its
projection. Session rechecks ownership and adopts the inspected definitions while
releasing only the matching uncertainty hold. No history result is acknowledged,
failed-prefix undo is invented or old valid history intentionally cleared. The
original receipt/outcome stays retained as the recent completed diagnostic; the
Session status still describes the original routing outcome as unconfirmed or
potentially partial. Failed final inspection keeps owner routing controls gated,
even if a provisional presentation refresh already occurred.

The optional owner-bound button is inside the existing routing surface and uses
its capture-or-cancel input ledger, presentation/epoch checks and focus-retirement
logic. Held Return, autorepeat and release cannot transfer to replacement route
controls. A fresh activation after actual native release is allowed. The callback
does not add a second selection store, controller, consumer, periodic source,
generic command framework or dependency.

## Synthetic verification and limits

Seven owner scenarios cover claimed-call closure, unchanged-frame inspection,
retained original reports, prior valid history, busy/configuration replacement,
reentrant history ownership, undrained/evicted frames, unknown metadata,
finalization, reconnect and nested recovery. The nested callback probe originally
reproduced loss of the retained diagnostic and now passes with the owner guard.
A pure Session case rejects copied frames and foreign
requests. The native XTest case keeps Return held across successful recovery,
checks autorepeat suppression and then checks fresh route activation. It uses
the existing bounded GTK activation interval, without resending an input action.

Final local checks pass **370 all-feature and 327 default GTK tests**, including
the nine new controller/Session/native-input cases, plus native build, changed
file formatting, whitespace and scope/privacy checks. The earlier nested
inspection probe failed with a lost diagnostic before the guard and now passes
unchanged. The native Return checkpoint uses the same 300 ms GTK activation
interval as the pre-existing Return test; no production input retry was added.
Core/backend/legacy suites and the aggregate validator were not repeated for
this GTK-only candidate; the unchanged base checkpoint separately passes its
74 core, 196 backend and 361 GTK aggregate and hosted checks. The owner logic has
now separately cleared its independent review gate; hardware acceptance remains
unperformed.

The reviewed combined integration passes the locked native aggregate: **86
validator controls, 74 core, 196 backend and 370 all-feature GTK tests**,
formatting/build, Cargo/ELF closure checks, three headless fixtures and three
controlled smokes. Zero Node-tool attempts are recorded. Program source matches
the independently reviewed `0a8cd86b` exactly; only integration/status documentation
changes. That source separately passes 327 default GTK tests and hosted run
[40147995158](https://github.com/jee-mj/pecanmixer/actions/runs/40147995158).
Legacy suites/examples were not repeated for this GTK-only follow-up; the
preceding closure checkpoint retains their passing receipts. No final local
test failure or environment blocker remains.

No hardware, real audio/server, user profile, physical desktop, performance,
server-convergence or failed-prefix rollback acceptance is claimed. All fixtures
are synthetic/public project data. No recording, legacy IPC, core DTO, version,
dependency, installed-profile, merge, release or JavaScript removal change.
