# Reviewed Folder owner adoption

The separate candidate starts at PR2 `a5875178`, includes the independently
cleared transport helper `84717ec4` plus retirement correction `53f76e2d`, and
receives independent owner correction clearance at `07e6edaf`. Integration starts
from PR2 `7fb0b934`, preserving the separately reviewed idle migration owner.
The only source conflict resolution retains adjacent Folder and migration module
declarations; no reviewed runtime logic is changed.

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
updates do not replace Folder bindings. At this Folder checkpoint native track
elapsed remained `None`; the separately reviewed elapsed follow-up below now
adopts real producer observations. No receipt-arrival stopwatch or audio alignment
assertion is made.

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

## Worker-capacity and prepared-plan review correction

Independent review blocked `f7fb8199` on two injected-contract failures. Cancelling
preparation cleared its pending slot before the worker finished, so repeated root
changes could admit more outstanding workers. A substituted prepared plan could
also reach the injected launcher before the presentation rejected its paths.
NativeIo currently prepares matching plans; no deployed wrong-folder launch is
claimed. Both review cases are reproduced before correction.

Cancellation now revokes launch while retaining the original request token and
execution slot until preparation finishes. A small private background join
supervisor delivers results only after the I/O worker has joined, including
thread-local teardown. UI polling never joins; only one I/O worker is admitted
per pending job. The same joined-result handoff is used for launcher workers.
No pool, new runtime, timer or general cancellation framework is introduced.
The existing cancellation test now waits for actual worker completion before
expecting the action to become available again.

Before launch admission, `plan.requested()` must equal the original captured
paths. Mismatch is an explicit failure and never reaches the launcher. Current
authority revalidation and the original token remain required; no retargeting,
retry or substituted-path completion is allowed.

Three new synthetic regressions cover repeated root changes behind one gated
worker with fresh recovery afterward, substituted prepared plans, and responsive
UI polling while a cancelled worker's thread-local teardown is gated. All 67
focused owner tests pass. Review and final source-matched aggregate evidence are
separate from these focused checks. No actual launcher or user profile is used.

Corrected-source checks pass: 67 focused owner tests, 353 default GTK tests, and
native aggregate with 86 validator controls, 74 core, 197 backend and 396 GTK
feature tests, formatting/build/audits, three headless fixtures and three GTK
smokes. The explicit-target native workspace packages were rebuilt to keep this
candidate's evidence source-matched; zero Node-tool attempts were recorded.
Backend/legacy execution source is unchanged from `f7fb8199`; legacy suites were
not separately repeated for this UI-only correction. No final failed local check
or environment blocker remains. Hosted checks and independent re-review remain
separate pending gates before PR2 inclusion.

## Independent correction clearance and next parity gate

Independent re-review clears `07e6edaf`: 67 targeted owner tests and all three
unchanged review probes pass, including both original failures. Cancelled
preparation retains its execution capacity through background join/teardown, and
substituted plans fail before launch while releasing the original request token.
The integration retains that exact logic and the existing migration source.

The next transport prerequisite identified here was real native elapsed
production. The subsequent [reviewed elapsed follow-up](./native-elapsed-adoption-candidate.md)
now retains producer measurement time and eligible stable tracks, compensates
cached observation age, and reuses the existing clock/oldest-take formatter.
Opening, failure, stop, disconnect and finalization remain unavailable as required.
This source wall time does not prove sample arrival/alignment or file duration.

## Combined integration validation

The reviewed Folder Rust files match `07e6edaf` byte for byte, except the three
shared bus registration/service/test files, which also retain the independently
reviewed migration additions. The migration implementation/tests, manifests,
lockfile and versions remain unchanged. Only adjacent module registrations and
coverage documentation needed conflict resolution; no new dispatch logic is
introduced by integration.

Fresh host and explicit-target workspace rebuilds pass: 86 validator controls,
74 core, 197 backend and 407 all-feature GTK tests, native build/formatting,
Cargo/ELF audits, three headless fixtures and three GTK smokes. Default GTK passes
364 tests. All ten retained legacy library tests and compilation of the five
legacy examples pass. Zero Node-tool attempts are recorded. Synthetic fixtures
and public source are the only input; there is no actual launcher/profile/audio
operation. No final local failure or environment blocker remains. Exact published
PR-head hosted results are recorded separately in the PR description.
