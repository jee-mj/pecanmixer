# Retained origin for full idle configuration

This backend prerequisite starts at PR2 `6b1ccc1e4ec18d1862997458db075b4dd826e5a9`.
It adds no automatic settings restore, owner/UI changes, serialized payloads,
dependencies, version changes or active-recording structural policy.

## Caller contract

```rust,ignore
let origin = graph.history_graph(&original_publication)?;
// Retain origin with the user's intent. Use the independently read config CAS.
let receipt = graph.configure_idle_from(&origin, idle_config_intent)?;
// Retain and poll receipt.try_report(); admission is only queue admission.
```

`HistoryGraph` is the existing authenticated witness used by gain-only admission.
Do not obtain a newer witness when admitting a retained request. The new method
checks the supplied configuration revision and original owned Rec node serial,
registry generation, and exact `rec_N` input mapping under the same admission
lock. Foreign or invented authorities, reconnects, known unpublished replacements,
and same-ID node/input replacements are rejected. Unrelated graph revisions and
publication-cache eviction do not invalidate a retained authenticated witness.

The owned node and its current input prefix must already have been published.
Unknown/incomplete topology is unavailable rather than inferred from a name or
numeric ID. Internal queued metadata is at most one node plus 64 input identities;
it shares the existing 16-configuration/255-ordinary FIFO limits and reserved stop
slot. There is no new worker, queue or shared core type.

The old `configure_idle(intent)` remains a CAS-only compatibility/bootstrap path.
Its supported initialization behavior is retained. New retained native controls
and any future saved restore must adopt `configure_idle_from` deliberately; this
candidate does not change existing owner call sites or claim that they adopted it.
The gain-only API and legacy IPC remain compatible.

## Execution and completion

Execution validates retained authority before target queries and again afterward,
before a no-op or first mutation. It checks between resize, gain and option calls,
and after final readback/query callbacks. The original node/input prefix cannot
be rebound to replacement objects. Unrouted growth can finish before new ports
appear in the graph; any newly observed inputs must extend the permitted channel
prefix without replacing the captured inputs. Growth does not fabricate a graph
publication or server acknowledgement.

- Rejection before a mutating call has no adapter-induced local effects.
- `NoOp` means validated local configuration/options already match, with the
  original authority still valid.
- `Applied` means matching local filter, gains and options readback. It does not
  mean graph-observed ports, audio arrival, routing convergence or captured bytes.
- Loss of identity/revision after a local call is attempted is conservatively
  `PartiallyApplied`. Remaining setters are skipped; effects are not rolled back.
  A newer config/epoch snapshot is preserved. If the request still owns the CAS,
  stale local/configuration/transport metadata is invalidated instead of assigning
  replacement readback to the old request.
- Resize failure retains typed actual capacity and existing partial/retry rules.
  A retry needs a new config revision and witness of that actual input prefix.
- Queued disconnect/shutdown cancellation returns the existing terminal rejection
  and clears capacity. A call already begun can finish; subsequent calls are
  fenced and cancellation/partial status remains visible.

Callbacks and PipeWire calls run outside the command lock. These are local
before/after guards, not an atomic transaction with the server or an interruption
of a call already executing. Production callbacks are synchronous; synthetic
reentry proves the contract without establishing a hardware race.

## Safety gates and remaining restore work

Shrink always requires staged migration. Routed layout changes/growth still
require migration. The retained path also fences routes newly observed by the
command mirror after an older target query, without claiming server convergence.
Native-owned/opening/finalizing recordings and arming exclude
this idle full-config path; legacy recording remains checked at execution. MIDI,
clock and start/stop options remain disabled by existing validation.

The migration API already captures authority for its specific split/join/remove
actions, but cannot apply an arbitrary saved bus configuration. This addition is
therefore necessary for a future full restore; it does not replace migration's
connect/observe/disconnect/observe/shrink barriers. Arbitrary shrink/routed growth,
saved-layout initialization, external reload policy and active structural behavior
remain separate adoption gates. No profile is discovered or loaded here.

## Synthetic evidence

The pre-change probe reproduced acceptance after a same-ID registry replacement
at unchanged config revision. Eleven focused regressions cover original node/port
identity, serial/channel mapping, foreign authority/cache eviction, queued and
callback replacement, no-op authority, newer config/reconnect preservation,
claimed-call stop fencing, immediate/delayed growth publication, typed partial
allocation/retry, newly observed route fencing, existing safety gates and bounded
admission/cancellation. The route callback probe failed with `Applied` before the
additional mirror guard; no production/server race is claimed from that probe.

Fixtures are authored generic graphs and in-memory local targets. No user device
names, hardware topology, profile, audio capture or real services are involved.
Final check counts and the candidate commit are returned separately for independent
review; hardware, audio, physical desktop and performance acceptance are unrun.

Cloud checks passed: 76 core tests, 230 backend tests (including the 11 focused
regressions), all 10 legacy library tests, all-feature GTK compilation, scoped
Rust formatting and whitespace checks. No owner/UI, core, IPC, manifest, lockfile
or version files changed. Publication and independent review remain separate.
