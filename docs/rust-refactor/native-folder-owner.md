# Folder owner adoption candidate

This separate candidate starts at PR2 `a5875178`, includes the independently
cleared transport helper `84717ec4` plus retirement correction `53f76e2d`, and
awaits independent owner review. The idle migration candidate remains separate.
No unreviewed owner source is published to PR2 by this candidate.

The existing Buses and recording page now contains the retained Folder/directory
presentation in a bounded scroller. No picker is added: original JS has a Folder
action. Native construction supplies the process adapter; fixture construction
supplies no I/O endpoint. Labels and explicit-root updates perform no filesystem
access. Headless/fixture/smoke paths cannot prepare or launch a real folder job.

## Latest-start authority, without timing

`RecordingSnapshot::latest_started` retains one original receipt, assigned only
when a take first passes capture-attachment validation. Existing active takes do
not reassert themselves on subsequent worker iterations. Stop, disconnect and
bounded recent-history eviction do not erase this folder evidence. Writer
readiness, queued requests, ticket/ID sorting and UI arrival order are not used
as start chronology. No clock, timestamp, elapsed field, command, FIFO or timing
subsystem is added. Local attachment still does not establish sample arrival or
audio correctness.

BusOwner projects this actual receipt with the explicit root into a stable,
opaque folder origin, independent of graph/timing revisions. Busy reports on the
same latest receipt preserve known folder evidence; a newly designated receipt
cannot reuse the previous take's path. A complete latest status can be included
for folder projection after recent UI-history eviction, without removing live
receipt ownership. The full graph, Session/history and backend snapshot consumer
are unchanged. Foreign origin or substituted paths cannot authorize a launch.

## Explicit action lifecycle

One Folder request retains its original token and captured paths. Preparation
runs on a worker with explicit paths only. The existing owner clock consumes its
result, checks current root/latest/closing authority again, and waits across busy
inspection before admitting a launcher worker. A stale preparation is cancelled
before launch and cannot be retargeted by changing root or latest take. Already
admitted preparation may finish creating its explicit root; cancellation guarantees
no folder launch from that retired preparation.

The native worker uses a fixed `xdg-open` program and one literal OS argument,
then waits/reaps off the GTK thread. Success is reported only after the helper
returns successfully, as submission evidence rather than proof that a desktop
window opened. Spawn/wait/exit errors remain correlated to the original request.
Once launch is admitted, root/closing changes do not relabel or cancel its actual
outcome. There is one pending job and bounded result channel; no automatic retry,
UI join, new timer, profile discovery or shell command is introduced. External
folder helpers are not recorder/settings finalization obligations. A pending
preparation cannot launch after closing; admitted external helpers retain their
normal independent process lifetime.

The owning shell supplies its existing monotonic clock to the workspace. Timing
updates do not replace Folder bindings. Native track elapsed remains `None`, so
active recording time is visibly unavailable; only authoritative idle can show
zero. No receipt-arrival stopwatch or audio alignment assertion is made.

## Synthetic validation and limits

Six owner cases use injected preparation/launch implementations and unique
synthetic temporary roots. They cover fixture isolation, actual latest receipt,
off-UI work, missing active timing, retained widgets/tick storms, root/latest
cancellation, busy prelaunch inspection, admitted failure through closing, path
substitution and stopped/disconnected/evicted folder evidence. The new backend
regression checks first attachment, stable latest evidence after stopping the
newer take, repeated pumping and recent-history eviction; existing readiness
coverage additionally checks that queued/opening alone never sets latest.

No native `xdg-open`, user profile, private device/topology, actual recording or
hardware operation is executed in these tests. Filesystem writes are only the
injected synthetic roots and existing backend temporary-WAV fixtures. Physical
desktop/file-manager, audio, elapsed production, performance and packaging
acceptance remain unrun. Structural edits during recording remain gated; no
segmentation policy is selected.

Final local validation passes: 64 focused owner tests (six new Folder cases),
197 backend tests (including the new latest-start regression), 350 default GTK
tests, and the native aggregate's 86 validator controls, 74 core, 197 backend and
393 all-feature GTK tests. Formatting, native build, Cargo/ELF audits, three
headless fixtures and three GTK smokes pass with zero Node-tool attempts. The ten
legacy library tests and compilation of five retained examples also pass.

A targeted authority probe reproduced accepting an old folder origin after the
owner's latest receipt changed but before widget republication. Authorization
now checks current receipt identity/signature directly, in addition to the
published context and captured paths. The regression passes in the final gate.
No final failed check or environment blocker remains; independent review and
hosted candidate results remain separate gates. The helper's reviewed Rust source
is unchanged; only its adoption-status notes are updated.
