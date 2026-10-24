# Backend meter capture identity

The graph worker previously retained each meter by numeric node ID and ordered
channel names alone. A replacement with the same ID/channels could therefore
keep a stream configured with the old `target.object` serial or old sink-capture
direction. Coalescing remove/add events into one graph publication hid the
transition from presentation-only comparisons.

The production worker now retains a capture only while all of these match:

- Node serial used by `target.object`.
- Sink-capture direction used by `stream.capture.sink`.
- Ordered audio channel names used by the capture format.
- Registry generations of the node and selected ports, in channel order.

Generations come from the existing registry lifecycle hooks, not DTO equality.
The lookup is private, read-only backend metadata. Missing node/port generation,
serial or channel information leaves that source ineligible. Ordinary volume,
mute, display-label, rate/quantum and unrelated registry changes preserve the
capture. Channel changes and identical coalesced object replacement recreate it.

Reconciliation runs on structural updates on the graph worker, before harvesting
that tick's peaks. It first drops invalid local capture handles, then attempts
replacement creation. If creation returns `None`, the node remains absent from
the capture map; the previous source cannot continue contributing peaks. A later
structural update can retry. There is no new retry loop, delay or wait for a
server acknowledgement. Registry metadata uses the existing command-state mutex
on the graph worker; it is not read from a process callback.
Session-end cleanup still clears captures before reconnect, and the read-only
observer still creates none. Local handle drop is not a server teardown ack.

`meter::maak`, `Meter`, `Pieken`, stamped graph/peak events and legacy serialized
IPC payloads keep their public contracts. The GTK meter caller contract does not
change: consume graph before matching peaks, explicitly bind changed sources,
and clear unavailable capture samples as required by the owning shell. Missing
capture entries are omitted, not replaced with invented silence. The legacy JS
peak cache behavior is unchanged by this backend-only correction.

Remaining protocol limits: peak DTOs carry no per-capture generation, individual
frame sequence, capture timestamp or detailed availability/error status. A GTK
binding cannot immediately reset hold/clip for an identical replacement hidden
from its graph DTO; registry-backed binding identity would be needed for that.
Equal-stamp frame ordering and capture-unavailability handling remain owner
responsibilities. `Some(Meter)` means local construction/configuration/connect
returned successfully, not that the server accepted the route or delivered an
audio buffer. Later stream negotiation/process failures are outside this slice.

## Headless evidence

Two focused regressions first failed under the original channel-only retention
rule: changed serial and changed sink direction both kept the old configuration.
After the correction, nine focused tests pass through the actual production
selector, registry hooks and reconciliation function. Injected synthetic handles
replace only PipeWire stream creation. Tests cover unchanged captures across 200
ordinary revisions, ordered channel changes, identical node/port replacement,
drop-before-create, missing requirements, failed creation and later recovery,
read-only cleanup and reconnect. No PipeWire session, user devices or runtime
capture data is accessed. These are source/lifecycle tests, not hardware, audio,
server-acceptance or performance validation.

Full cloud validation passes 42 core, 62 backend and 10 legacy tests with
`cargo test -p pecan-core -p pecan-pipewire -p pecanmixer --lib --tests --locked`.
`cargo build -p pecan-ui-gtk --locked` and
`cargo check -p pecanmixer --examples --locked` also pass. GTK widget tests are
outside this backend-only change; no production capture session was started.
