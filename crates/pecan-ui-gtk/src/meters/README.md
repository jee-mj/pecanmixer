# Native meter widget adapter

This module supplies native per-channel and mini-meter presentation. It does not
start a backend, create audio links, change startup, or make the preview a
functional mixer. Inputs in its tests are authored synthetic data.

The owning GTK shell retains one `MeterFeed`, feeds each accepted inbox batch,
and retains a `MeterWidget` wrapper for each displayed bar:

```rust,ignore
use pecan_ui_gtk::meters::{MeterFeed, MeterMode, MeterWidget};

let feed = MeterFeed::default();
feed.apply(&batch); // graph first, then peaks; before moving the batch elsewhere
let channel = MeterWidget::new(&feed, "Channel 1", MeterMode::Channel);
channel.bind(node_id, &[channel_index]);
container.append(channel.widget());

// A mini bar displays the maximum of the supplied channels, like the legacy UI.
let mini = MeterWidget::new(&feed, "Source mini meter", MeterMode::Mini);
mini.bind(node_id, &module_meter_channels);
```

Indices/groups come from `domain::mixer::Strip::channels` or
`domain::audio::AudioModule::meter_channels`. This module does not infer device,
stereo, input/output or route policy. Reuse widgets across model updates; call
`bind` again with current domain indices and `set_label` when their labels change.
Rebinding the same known identity and channel selection preserves hold/decay.

`MeterFeed` and widgets belong to the GTK thread. Backend threads continue to
publish owned frames into `LatestInbox`; no GTK object crosses that boundary.
The current preview uses its observer and does not produce active meter capture.
Connecting a production owner and mounting these widgets remains shell work.

The feed retains one graph and one replaceable peak map. Peaks must match the
current graph's exact epoch/revision; old/future frames are rejected. Multiple
frames normally share one structural stamp, so equal-stamp frames replace in
delivery order. The DTO has no finer frame sequence or capture timestamp; this
adapter cannot detect reordered replay within a stamp or infer sample age.
`clear_peaks()` explicitly removes displayed samples when capture is unavailable.
No arbitrary capture timeout or silence event is invented.

Bindings require a known node serial and retain its epoch, name, class and audio
port layout. Removal, serial/name/class/layout change, disconnect or epoch change
clears values/clip state immediately and invalidates the binding. A newly observed
replacement does not silently reattach; the shell must explicitly bind its new
model. Numeric-ID reuse is tested with serial changes, removal and epoch changes.
The graph DTO cannot reveal a replacement reusing both its ID and serial with
identical metadata and no observed removal. A stronger backend generation token
would be needed to distinguish that case. Every accepted graph revision discards
old samples, while an unchanged known source retains its animation state.

Each mapped, active widget registers its own GTK frame-clock tick. Unmapping or
`set_active(false)` unregisters the tick and resets animation state. Remapping
uses only the latest delivered frame, with no catch-up loop. The shell can use
`set_active(false)` for off-screen rows: this module does not implement viewport
occlusion detection or virtualization. Frames never create/rebuild GTK children
or queue a redraw by themselves. Visible ticks call the existing pure
`levels::Meter` and render only after accumulated changes or state transitions.

Bars are explicitly inverted to grow upward from silence; GTK's vertical
continuous allocation otherwise anchors the fill at the top ([GTK source](https://github.com/GNOME/gtk/blob/4.18.6/gtk/gtklevelbar.c#L395)).
A widget allocation test checks the filled block's bottom edge in both modes.
Normal bars expose level, held-peak line, clipping text and a dBFS readout. Mini
bars omit the peak line, clipping text and numeric readout, matching the legacy
mini presentation. Both reuse GTK LevelBar semantics and accessible names. The
normalized accessible value is the existing -60..0 dBFS scale; the normal text
readout supplies dBFS. Native role/value behavior still needs desktop/AT-SPI/Orca
acceptance. No new GTK feature floor, dependencies or global theme are introduced.

Focused display-free frame-slot test and headless GTK fixtures:

```sh
cargo test -p pecan-ui-gtk --locked frame_slot
G_DEBUG=fatal-criticals xvfb-run -a dbus-run-session -- \
  cargo test -p pecan-ui-gtk --locked meters
GTK_A11Y=test G_DEBUG=fatal-criticals xvfb-run -a dbus-run-session -- \
  cargo test -p pecan-ui-gtk --locked --features a11y-tests meters
```

Fixtures cover per-channel selection/mini maxima, core decay/hold/clipping and
elapsed cap, missing/non-finite channels, old/future stamps, source replacement,
disconnect, hidden/inactive lifecycle, latest-frame resumption, callback ownership,
accumulated redraw thresholds and 10,000 coalesced frames without structural
rebuilds. They do not establish real audio capture, physical rendering throughput,
CPU/memory/latency budgets, a distro matrix or hardware/desktop acceptance.
