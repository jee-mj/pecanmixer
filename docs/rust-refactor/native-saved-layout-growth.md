# Backend saved-layout initialization and growth candidate

Base: PR2 `5ad62b1cd18e1ed4217275da74638b5f83e47b6a`. This is a backend
prerequisite for general saved Rec configuration. No owner/startup/settings
adoption is included and existing generic configuration gates are unchanged.

## API and admission

`Graaf::restore_idle_layout(SavedLayoutIntent) -> Result<MigrationReceipt,
MigrationError>` accepts one complete `IdleBusConfig`, original authenticated
`HistoryGraph`, shared caller ticket, configuration CAS and recording-membership
revision. Capture those together for the original intent; never substitute a
newer witness or retry a rejected admission against a newer revision.

The complete saved proposal is validated by the existing native configuration
validator: mono/stereo widths, 1–64 channels, bounded state keys/entries, finite
gains and MIDI/clock/start-stop disabled. dB, mute, arming and inactive state
entries remain exactly as supplied. No intermediate default stereo configuration
or synthetic Add/Split/Join plan is derived. The caller supplies validated saved
preferences explicitly; the backend reads no settings file or profile.

Unlike ordinary edits, saved initialization can start with unknown current
logical/transport metadata. Actual local node/capacity/readback and the complete
published canonical `rec_N` prefix must still exist. Known transport options
retain their exact preflight; unknown options can be initialized deliberately.
Desired capacity must equal or exceed actual capacity. Shrink is rejected before
effects with `Configuration(LayoutMigrationRequired)`.

Admission authenticates original owned node/input and incoming source node/port
generations under the existing backend lock. The original channel mapping,
serials and route pairs must still match. Its retained witness survives ordinary
graph revisions and publication-cache eviction. Foreign/copied authorities,
missing original inputs, stale revisions and known unpublished replacements
cannot authorize a new request. Normal and saved admissions share one migration
slot, existing FIFO, worker and dispatch fences; no second sequencer is added.

## Effects and completion

Restoration matches the legacy `stuurBussen` behavior: keep existing channel
indices and apply the supplied total count/gains and derived armed transport
options. Existing incoming endpoint pairs remain exact, independent of recreated
link IDs. Same-name foreign Rec nodes are excluded. Unexpected routes on either
the old prefix or appended suffix are conflicts, never removed to recover.
This operation submits zero connect/disconnect calls and starts no recording.

Equal-capacity initialization/regrouping returns existing
`AppliedLocally`/`NoOpLocally` outcomes after actual local readback. Original
input/source identities and route pairs are checked after the final accessor.
There are no appended identities or fabricated observed-port stamp in this path.

Growth uses the existing `ApplyingLocal -> AwaitingPorts -> Observed` path.
Local resize success alone leaves logical metadata unknown. Each suffix input is
latched at its first coherent authenticated publication; out-of-order/immediate
arrival is supported, replacement/removal/rename after latching is rejected.
Only the complete exact extended input map in a fresh publication commits the
saved logical configuration. Successful growth retains the existing
`observed_inputs` witness stamped exactly at `ports_observed`. Its generations
cover the owned node and complete final input prefix, at most 65 objects. The
first completed Observed witness must be retained, not reconstructed later.

Capacity stays at 64 channels; original routes stay capped at 256 records. Saved
growth retains at most 63 new port records in the existing Work. Existing
two-second observation/30-second total limits, one queued migration action,
32-command drain, normal queue bound and reserved Stop capacity remain unchanged.
Recording/opening/finalizing ownership and arming keep the idle path gated.

Partial allocation, post-call authority loss, cancellation and timeout preserve
the full original proposal, typed actual resize/readback and uncertainty. A
claimed local apply may finish after cancellation, with the receipt pending
until that call returns. There is no rollback, automatic replay or late upgrade
of a frozen failed receipt. This explicit primitive adds no automatic repair
policy for a previously failed prefix; that still requires separate owner
reconciliation and is not adopted here.

## Required owner adoption

Use the existing shared Session lease/ticket and once-per-epoch restoration
policy; do not relax current user-operation, recording, protected-input, reload
or quit guards. Retain original full configuration, witness and source/prefix
proof along with the actual receipt. Correlate ticket, source stamp, both
revisions and exact proposal, and require zero in-progress calls.

For equal capacity, require local success followed by fresh compatible actual
configuration/options readback and the original authenticated input/route map.
For growth, require completed Observed plus the retained final-input witness,
matching original prefix/source proof, exact final canonical input map and
preserved endpoint pairs in a current authenticated frame. Do not require an
already-coalesced observation frame to reappear or look it up in a bounded cache.
Accept current graph/configuration/terminal presentation before releasing the
same lease. Busy reads, partial outcomes, replacement and supersession must keep
the existing pending/reconciliation behavior. No preference writeback or graph
Undo is implied by restoring preferences.

## Focused synthetic checks and limits

Tests extend the existing migration fixture, not a new framework. They cover
unknown/known routed metadata, equal-capacity regroup/no-op, complete saved state
and odd/64-channel growth, delayed/out-of-order/immediate suffix publication,
recreated and foreign routes, retained/cache-evicted and foreign witnesses,
prefix/source/suffix replacement, malformed/shrinking/stale proposals, actual
readback/options conflicts, partial allocation, timeout, cancellation,
disconnect/shutdown, recording, route/queue bounds and unchanged generic gates.

Local checks pass: 52 migration tests (including 12 saved-layout groups), all 252
backend library tests, 17 existing GTK owner migration tests with all features
under isolated Xvfb/D-Bus, and the native GTK executable build. Scoped formatting
and whitespace checks pass. These are authored source/fixture checks awaiting
independent review; hosted CI and physical acceptance are not claimed.

No PipeWire server, hardware, audio capture, user profile, physical desktop,
performance, server convergence or causal transaction acceptance is exercised.
Owner integration, arbitrary shrink, topology replay, active-recording structure,
automatic failed-prefix repair and MIDI/clock effects remain separate work.
Legacy behavior, dependencies and version `0.1.0` are preserved.
