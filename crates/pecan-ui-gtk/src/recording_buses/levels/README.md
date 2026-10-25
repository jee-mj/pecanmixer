# Rec level UI contract

This candidate starts at PR2 `940afbd1`. It adds the pure `BusConfig::with_db`
and `with_mute` operations and retained controls in existing recording-bus rows.
BusOwner, Session, service, backend, startup, settings effects and graph history
are unchanged. Production has no level callback/capability yet; no mutation is
enabled by this source alone. No new dependency or framework is added.

## Original behavior

`Mixer.tsx::Kaart` uses the logical bus state for dB/mute. `bus.zet({db})` keeps
mute/arming; `bus.zet({mute})` keeps dB/arming. Moving a muted fader changes the
later unmute level. The `-90 dB` off value produces zero without marking M on.
`bussen.ts::stuurBussen` applies the selected bus's gain to all its channels,
leaving unrelated buses unchanged. Gain is applied before peaks/capture samples
in `opname.rs`; it affects the recording, not only a monitor control.

The original bus branch returns before graph volume/mute Undo functions. These
UI events must not become RecordVolume or graph mute/history entries. The pure
setters validate real bus keys; dB input uses the existing -90 through +6 UI
range, rejects nonfinite values, and never changes source configuration.

Pointer movement, Shift sensitivity, wheel grouping/latest observed dB, off
sentinel, reset and pointercancel finalization use `controls::fader` unchanged.
A native zero-displacement press submits nothing. The initial pointer capture
stays exact until its first change, even across ordinary frame updates. Each
owner interaction is immutable after admission; updates carry its request,
rather than taking a fresh origin or changing the target.

The vertical pointer fader is paired with a named native dB proposal spin button
and Set gain action for keyboard/accessibility. This explicit commit path is an
accessibility addition. The pointer surface is presentation-only to AT; the
spin/action expose the adjustable value and commit. Full desktop/screen-reader
acceptance is separate. Observed row text remains actual published metadata;
a proposed thumb/value or callback acceptance is never labeled audio completion.

## Adoption by the existing owner

Set `Capabilities.idle_levels = Available` only when the actual idle level
adapter is present; call `BusControls::bind_levels`. Routing/structural/Add
capabilities cannot enable it. Active/opening/finalizing/unknown operation is
still unavailable: the current backend idle configuration path rejects recording
ownership. Active gain needs a reviewed gains-only backend extension that keeps
layout, arming, options and track identities unchanged. This is a temporary
capability gap, not a permanent restriction on legacy behavior.

The owner must also project shared admission/uncertainty: another graph operation
without this level interaction makes idle levels unavailable. The retained level
interaction itself keeps its capability while its pending operation gates fresh
starts. Use the same owner and inbox; no second subscription is needed.

The callback receives `levels::Event<O,R>`. O is the owner's exact existing
origin; R is its opaque interaction request, never a new widget/backend ticket:

Callbacks run on the GTK thread and must return promptly; never wait on native
receipts, writer joins or filesystem work in a callback.

- Begin(capture, kind) and Set(intent) must return Tracked(R). Err means no new
  work was admitted. Recompute/validate Set.proposed_configuration using core.
- Update(R, db), End(R, db, reason) and Abandon(R) return Accepted. This means
  handoff, not submission, observed audio, cancellation, rollback or completion.
- The owner retains all real receipts. One R spans the whole continuous gesture,
  its coalesced desired values and all sample receipts; an intermediate sample's
  terminal result must not complete the UI interaction.
- Publish matching `Snapshot.operation` as pending until End/Abandon and all
  admitted work is resolved/reconciled. The same existing publication gate used
  for structural work applies; zero in-progress/fresh readback is owner evidence.
- Keep one native receipt in flight and one latest desired value, flush the final
  End value, and allocate real tickets only through the shared owner sequence.
  A's current external-lease API needs a narrow way to allocate subsequent sample
  tickets under the retained lease. No second queue/counter is provided here.
- Bind the captured bus/channel layout and native Rec object/port generations in
  the owner. This generic UI cannot authenticate generations from DTO equality.
  Revalidate at every native handoff. Disconnect, invalid/missing metadata,
  changed bus definitions, rebind and unmap abandon local input; admitted R remains
  retained until a matching terminal status. Stop/quit stay owner operations.
- Busy, unrelated and Queued/Applying/AwaitingObservation frames never release R.
  Matching partial/unconfirmed results additionally latch level reconciliation;
  after fresh actual owner reconciliation and terminal receipt closure, publish
  actual status and explicitly call `levels_reconciled()`. An ordinary frame or
  callback rebind cannot clear this latch. Both uncertainty latches also gate the
  existing `configuration_pending()` seam used by companion routing/configuration
  controls. Protocol-violating untracked replies
  fail closed; reconstruct only after the owner resolves all retained work.

Call `tick_levels(monotonic_duration)` from the existing owner clock (also while
waiting on publication) to close wheel groups after 600 ms. No timer/consumer is
created. Coalesced snapshots use the existing one-slot `offer_snapshot` path.
Widgets are retained by bus key; fresh action callbacks capture exact context and
delegate. Terminal callback returns cannot affect a newly admitted request.
Dropping an open input sends Abandon to its original delegate; it does not drop
or complete the owner's native receipt.

Saved bus preferences, reconnect application of settings, live backend adoption
and active gain support remain later integration work. No filesystem/profile,
recorder or live folder effects are part of this candidate.

## Checks

Targeted synthetic checks cover core gain/mute restoration, unrelated channels,
input bounds; default/active gates, immutable origin/request flow, busy/mismatched
reports, reentrant publication, partial reconciliation, motion/wheel storms,
reset, end failure, disconnect, rebind, view drop and stale callback returns.
The existing isolated XTest harness additionally checks actual native pointer
capture, reentrant refresh, double-click reset and held Space mute retirement.
Fixtures are authored generic data; no device names/topology/profile/log captures
are taken from a user machine. No hardware/audio/performance/Wayland/Orca claim
follows from these tests.

Cloud checks on 2026-10-25 (Rust 1.99.1, GTK 4.18.6): all 76 core tests and
all 415 all-feature GTK tests passed, including the additional isolated XTest
case. Seven focused default-feature level-widget tests, the native build,
package formatting and whitespace checks passed. Backend and legacy source and
dependencies are unchanged; their full suites were not rerun for this candidate.
