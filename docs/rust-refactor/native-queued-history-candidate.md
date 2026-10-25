# Queued Undo/Redo candidate

Based on `602aaf075cb1e5e5d1bded1dd76d744a75a4e405`. This is a separate
review candidate, not an assertion of PR2 adoption or full parity.

## Original behavior and bounded port

`src/geschiedenis.ts:44` queues direction closures in FIFO order. The first
attempt runs immediately; the next is scheduled 250 ms after that closure
returns. Each closure chooses the then-current Undo/Redo stack top and looks up
current graph IDs. An empty direction is a no-op but still consumes its turn.
`src/App.tsx:92` accepts repeated Ctrl/Meta+Z, Shift+Z and Y outside input fields.
The original direction queue is unbounded; the history itself is capped at 200.

One queue now belongs to the retained native workspace. It stores at most 200
waiting directions and one active real Session request. Overflow rejects the
new input visibly without dropping/reordering older accepted inputs. It uses
the existing workspace poll clock, keeps the 250 ms minimum after each attempt,
and additionally waits for actual local dispatch closure, truthful completion
and accepted graph progress. It creates no timer, backend queue, ticket counter,
history controller or synthetic acknowledgment.

Only an input's direction is queued, not a frozen command list, numeric IDs or
a history entry. Execution selects the current valid stack top through the same
Session/core guards and native admission path. Core pruning can remove an
invalid old entry before that turn; no stale action is sent to a replacement
object. Original registry guards on surviving entries remain required. Current
graph Arcs are authenticated; copied/stale input snapshots reject admission.
The additive `HistoryGraph::same_connection` checks only owner/epoch continuity;
it is not a substitute for an entry's semantic/generation guards.

Unrelated user edits can occur between turns, as with the legacy direction
queue. Already-admitted directions run against the resulting valid history.
Configuration/volume/recovery/finalization gates still reject new history
admission. An existing queued direction waits behind a subsequently acquired
shared configuration lease; it never bypasses that lease. Bus structural edits
do not become history actions or gain new recording permissions.

The window's GTK text-input exclusions and immutable button-press/context guards
remain. Keyboard directions may queue even when their current stack is empty,
matching JS keydown semantics. Fresh buttons can queue while a history operation
is busy; held or retired controls cannot admit new input after context changes.
Focusing an editor does not retroactively cancel already accepted input; new
editor shortcuts remain native editor actions.

## Failure, cancellation and recovery

Lost native connection/owner authority, disconnect/reconnect or finalization
cancels unsent directions. Admitted work keeps its real request and ordinary
Session/lifetime handling. A rejection/unconfirmed result cancels the remainder
without retry. A closed zero-effect rejection permits a fresh user input;
partial/unconfirmed effects retain the original request under the existing
explicit recovery gate. There is no rollback or failed-prefix Undo.

The mounted history inspection control uses the existing current authenticated
bus/config inspector and Session recovery method. Actual receipt closure is
required, including when frozen counts still show an old in-progress call.
Busy/unknown inspection does not release recovery. Retired inspection callbacks
cannot recover a different request. Metadata loss on reconnect retains the
already-reviewed clock-driven initialization ordering. Original failures remain
retained after inspection; canceled inputs never revive automatically.

The queue treats a large sequential reset or its inverse as one parent barrier,
not independent native children. Undo/Redo ordering and one final parent history
acknowledgment remain the existing Session/core responsibility.

## Synthetic evidence

The candidate uses only existing authored graphs, injected receipt/registry
metadata, isolated GTK/XTest windows and synthetic configuration services.
No PipeWire, hardware, profile, real folder launcher or recording is operated.

Focused cases cover mixed Undo/Undo/Redo order, delayed report versus graph and
local closure, minimum spacing, empty-stack no-op, bounded overflow, quit and
reconnect cancellation, replacement pruning, copied origins/shared leases,
large reset/Undo/Redo parent barriers and mounted failure inspection.
The existing held button, stale callback, focus-return and native editor checks
remain active. The former XTest assertion that busy Redo is dropped is replaced
by a fixed-input case requiring that exact queued Redo, with no second keydown.
The completed synthetic native report helper now explicitly seals actual local
execution, rather than allowing the queue to infer it from status alone.

Local checks pass 10 focused queue/widget cases, 607 final all-feature GTK/input/
D-Bus cases, 553 default-feature GTK cases, 10 legacy unit cases and compilation
of all five existing audio examples without running them. The existing native
validation script passes 86 validator controls, 79 core and 253 backend cases,
native build, fixtures and dependency audits (606 GTK cases before the last
large-parent queue case and end-of-attempt spacing clarification). The final
607-case run includes those changes. The final native executable build,
Cargo/ELF audits, three headless fixtures, exact expected overview comparison,
three GTK smokes and formatting/diff checks pass again on that final source.
No new dependency or JavaScript runtime was introduced; the guarded native
script recorded zero JavaScript-tool attempts. Independent review remains a
required gate before PR2 integration.

```sh
cargo test -p pecan-ui-gtk --locked queued_history -- --test-threads=1
cargo test -p pecan-ui-gtk --locked --features a11y-tests,x11-input-tests,tray-bus-tests -- --test-threads=1
```

These are source/controller/widget checks, not physical keyboard, cross-app,
Wayland/desktop, audio, policy-manager, recording or performance acceptance.
Full native parity/cutover remains unfinished. Active-recording structural
policy, saved-layout shrink/repair, desktop packaging/defaults and JS removal
are not changed by this candidate. Versions/dependencies remain unchanged.
