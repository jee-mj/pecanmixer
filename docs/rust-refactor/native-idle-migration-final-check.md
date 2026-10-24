# Idle migration final binding correction

This separate corrective descendant of `0c2bed3` changes no public API, serialized
IPC, UI, routing order or resource limit. Cleared arming correction `4524167` is
retained separately and must also be included when the stacked prerequisites are
integrated; this commit does not replace it or rewrite either earlier candidate.

The local migration target runs outside the Commands state lock. Its final
options/readback callbacks could replace a captured Rec node or input port with
the same numeric ID. Epoch and config identity equality alone then allowed an
unchanged-allocation split/join to publish AppliedLocally. Four deterministic
contract probes reproduce that result on the original candidate.

The worker now repeats the captured graph-scope validation after every target
accessor has returned, before publishing logical metadata or local completion.
This covers original registry generations, node serial, port mapping and routing
predicates. It validates only surviving prefix inputs after an actual shrink, so
expected tail removal remains valid. Ordinary unrelated graph revisions remain
allowed. No target accessor is moved under the state lock and no wait is added.

If that final binding fails, the retained receipt reports PartiallyApplied and
the typed failure. It preserves the actual target readback/resize result as
evidence of the attempted call, without attributing it to a replacement object.
Local/configuration/transport metadata becomes unavailable, with a new config
revision requiring explicit reconciliation. There is no rollback or successful
no-op claim after lost authority. Existing epoch/config-change handling and
cancellation fences are retained.

The interleaving is deliberately injected through the ConfigTarget test contract.
The production target is synchronous on the PipeWire worker; real registry
callback reentry at this point has not been established. These tests demonstrate
the contract defect and correction, not a deployed audio race or hardware result.

Eight synthetic regressions cover the four split/join replacements, a nominal
no-op replacement, a retained-prefix replacement during shrink, ordinary graph
revisions in both local operations, and expected tail retirement inside resize.
They pass with the original migration cancellation, observation and partial-readback
regressions. Active-recording layout edits remain gated; segmentation is not
implemented or decided by this correction.

Cloud checks pass: 64 core, 186 backend and ten legacy tests using
`cargo test -p pecan-core -p pecan-pipewire -p pecanmixer --lib --tests --locked`,
plus the GTK build, five legacy example checks, formatting and whitespace checks.
An earlier compatibility run reproduced the existing multitake fixture race
between snapshot capture and a stopped writer joining. A separate test-only
commit uses the existing bounded teardown gate to hold membership stable for
that aggregate-flag test. Production revision checks and stale rejection are
unchanged. No server, user recording, hardware or performance test was run.
