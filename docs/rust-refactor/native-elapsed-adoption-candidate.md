# Reviewed native elapsed adoption

This separate review branch descends from producer `38db4706` on PR2 `940afbd1`.
Independent producer review passes its 203 backend and ten legacy tests plus four
additional probe groups: cached measurement age, serial/node/port generation and
epoch replacement, unconfirmed ownership, and 1,000 real measurement reads that
preserve membership/latest identity. No backend defect was found. The UI adoption
below independently clears at `bcdd4553`; integration retains its exact Rust
source and producer `38db4706`, with documentation-only follow-up.

The existing native receipt adapter maps duration only to the producer's eligible
stable TrackIds. It retains the exact `observed_at` as the additive
`TakeStatus.elapsed_at`; it never stamps delivery as a new measurement. Missing
producer timing remains missing even if an older phase says StartedLocally.
Accepted per-track/take stops project Finalizing before the writer catches up.
No core graph DTO or legacy serialized payload changes.

The Folder/transport projection samples one monotonic instant alongside the
existing shell clock. Each retained measurement is aged to that sample before
the existing model chooses the oldest running duration. Different takes' report
ages are accounted for independently. Cached raw durations/instants remain
unchanged in the owner. Repeated deliveries cannot compound age; takes that
started before the shell clock origin remain valid. Future instants and duration
overflow cannot authorize a running clock.

Projection additionally requires current connected graph epoch and configuration
owner/epoch, fresh known owner inspection, running phase, no failure and no local
accepted stop. Busy, unknown, unconfirmed, disconnected and finalizing state stays
unavailable rather than restarting a clock. Stopped tracks cannot supply another
track's duration; surviving grouped tracks retain their take origin. The original
source reports are observations rather than atomic graph/writer transactions.

Timing-only values are removed from the bus edit publication comparison. Actual
receipt state remains retained, while periodic clock observations do not recreate
bus edit origins, Folder bindings, requests or widgets. The existing transport
formatter, hidden-view gating and application clock are reused. No timer,
consumer, command, dispatch queue, worker or I/O action is added.

Four synthetic GTK cases cover differently aged observations and cached delivery,
partial stops and missing sibling timing, twelve unavailable-authority cases,
and a 100-publication timing storm with stable edit/Folder bindings and zero new
audio handoffs. Fixed measurement instants avoid timing sleeps as a source of
duration; the two rendered-clock fixtures mount their widgets because the
reviewed helper deliberately does not update unmapped labels. Native IDs and
producer lifecycle behavior retain the separately reviewed backend tests.

This is local wall time since validated attachment. It is not sample duration,
sample arrival/alignment, WAV length, lossless recording, server convergence or
finalization proof. No actual recorder, device, file-manager, profile, private
topology or user audio is used by the new UI fixtures. Hardware, desktop,
performance and audio acceptance remain unrun. Active-recording structural edits
remain gated pending the separate product decision.

Final cloud checks pass: 368 default GTK tests and the aggregate's 86 validator
controls, 74 core, 203 backend and 411 all-feature GTK tests, native build,
formatting, Cargo/ELF dependency checks, three headless fixtures and three GTK
smokes. The Node-tool attempt log is empty. The producer review also ran all ten
legacy library tests successfully; legacy examples were not repeated for the
UI-only adoption delta. Native workspace packages were rebuilt for the explicit
target to avoid stale cross-candidate artifacts. No final local failing check or
environment blocker remains. The source, fixtures and scope audit finds only
synthetic/public project data, with no dependency, version or lockfile change.
Independent adapter review passes 82 owner tests, eight transport tests and
three additional cache/stop/pending-token probes. [Exact candidate hosted run
40156462471](https://github.com/jee-mj/pecanmixer/actions/runs/40156462471) passes.
Final integrated PR-head checks are recorded in the PR description separately.

## Integration checkpoint

Integration fast-forwards PR2 `940afbd1` through the independently reviewed
producer and adapter. All Rust source matches `bcdd4553`; follow-up changes only
documentation. Fresh host and explicit-target workspace rebuilds pass the native
aggregate with 86 validator controls, 74 core, 203 backend and 411 GTK tests,
formatting/build/dependency audits, three headless fixtures and three smokes.
Default GTK passes 368 tests. All ten legacy library tests and compilation of the
five legacy examples pass again. Zero Node-tool attempts are recorded. No final
local failure or environment blocker remains. The separate Rec-level UI and
active-gain backend candidates are not included in this checkpoint.
