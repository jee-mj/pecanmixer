# Reviewed routed Add owner

Base: reviewed backend-only PR2 `69909b87cc71d55d387377e21a39f06646bdd757`,
which includes independently cleared AddStereo `550bd099`. Exact owner candidate
`6e0cc8fd4bb03739a6c7b803bc019a9c0dc14af9` independently clears review and is
adopted through an unchanged source cherry-pick. Idle routed Add now uses this
reviewed owner; partial repair and active structural edits remain gated.

## Existing authority and completion path

Idle routed `AddStereo` now goes through the existing migration owner/service,
using the original exact graph Arc, config revision, recording-membership
revision and shared external admission lease/ticket. The proposal must match
`BusConfig::add()` exactly. Existing gains, mute, arming and inactive stable-key
policy therefore stay with the reviewed core behavior. Unrouted Add retains its
existing configuration path. No new backend API, queue, worker, sequencer,
counter, dependency, graph Undo or recording start is introduced.

The captured proof retains the original owned node/input generations, incoming
endpoint pairs, and source endpoint/node generations. New suffix identities
cannot exist at admission. The first actual `Completed(Observed)` report's
`observed_inputs` is retained directly. It is never reconstructed from a newer
numeric-ID snapshot and never obtained by looking up the old observation stamp
in a cache. A later receipt cannot rebind a retained suffix proof to replacement
generations.

Success requires Observed with zero in-progress calls, fresh exact configuration
readback, and an accepted current graph/configuration/operation presentation
before releasing the same lease. The retained input witness must be stamped at
`ports_observed`; its exact complete canonical input map must match current
authenticated generations and preserve the original prefix. The original
source/prefix proof must independently match that same current frame. Endpoint
pairs must remain exact, including no unexpected routes on the new suffix.
Harmless graph revisions and recreated link IDs do not replace those proofs.

Busy reads, graph lag, and rejected presentation keep ownership pending. Missing
or foreign witnesses, changed original/suffix identities, superseding layout,
partial allocation, cancellation and timeout never become successful append by
late publication. Existing retained uncertainty, original receipt, explicit
inspection and common quit/stop paths remain; there is no retry, rollback or
partial-prefix history. This candidate does not add automatic partial repair.
Recording, opening, finalizing and unknown structural edits remain gated; no
segmentation-versus-disable product choice is made.

## Synthetic checks

Seventeen focused migration tests pass, including six new append groups:

- The actual Add widget captures original graph/config/membership authority and
  holds shared admission through in-progress calls, busy readback and rejected
  presentation until the matching current snapshot is accepted.
- An owned-node/input-only completion witness survives 100 ordinary publications
  and verified cache eviction; original route pairs survive recreated link IDs.
- Original node/input/source or suffix replacement, foreign authority, missing
  suffix, stamp mismatch and unexpected suffix routes become uncertainty.
- The first observed suffix generations cannot be rebound by a later receipt.
- AppliedLocal/NoOpLocal/missing proof and partial/cancelled outcomes cannot be
  upgraded by later ports, repeated polling or command replay.
- Stale proposals/origins, known recording and competing shared history retain
  their existing gates. Existing unrouted Add and split/join/remove controls pass.

All 151 bus-owner checks pass in each feature mode; all 46 recording-bus/model,
widget and native XTest checks pass with features. Filters overlap. Native build,
scoped Rust formatting and whitespace checks pass. The full workspace suites are
not repeated for this UI-only candidate; its base separately passes 86 validator,
76 core, 240 backend, 546 feature GTK, 492 default GTK, ten legacy tests and five
examples compiled, with both exact-head hosted runs green.

Two initial fixtures were corrected without production relaxation: a prior
blanket routed-Add rejection now checks an invalid proposal, and a presentation
test uses the existing duplicate-track rejection fixture instead of holding a
read-only inspection borrow across publication. It updates the retained child
status to clear that synthetic error, rather than assuming removal from the
service list discards retained receipts. All final targeted checks pass.

Only GTK owner/view and authored synthetic tests plus these notes change.
Backend, core, service/DTO interfaces, startup, settings schema, dependencies and
versions are unchanged from the reviewed base. Tests use injected services and
generic graph data; no real backend constructor, audio service, recording, user
profile or physical desktop is operated. No hardware, audio alignment, real-time,
performance, packaging or physical-input acceptance is claimed.

## Independent clearance and adoption

Independent review passes 151 owner checks in each mode and four additional
probe groups covering replacement, stale/retired Add controls, reconnect/partial
outcomes, foreign routes and superseded configuration. Exact candidate
[CI40175024143](https://github.com/jee-mj/pecanmixer/actions/runs/40175024143)
passes 86 validator controls, 76 core, 240 backend and 552 feature GTK tests,
native build, fixtures and dependency audits.

The reviewed source applies without conflicts onto `69909b87`. GTK runtime and
test files match `6e0cc8fd` exactly; only these adoption notes differ. Backend,
core, service/DTO interfaces, startup, settings schema, dependencies and versions
are unchanged. Exact combined checks and hosted receipts are recorded separately
after publication; candidate CI is not a substitute for combined-head evidence.

Fresh combined validation passes 86 validator controls, 76 core, 240 backend,
552 all-feature GTK and 498 default GTK tests, all ten legacy library tests and
all five legacy examples compiled (none executed). Native build, formatting,
Cargo/ELF audits, three headless fixtures and three controlled smokes pass.
Host and explicit-target workspace packages are cleaned and rebuilt first;
zero Node-tool attempts are recorded. Source/privacy/whitespace and unchanged
dependencies/versions pass. No final local failure or environment/dependency
blocker remains. Partial append repair, active structural policy and physical
hardware/audio/desktop acceptance remain outside this source adoption.
