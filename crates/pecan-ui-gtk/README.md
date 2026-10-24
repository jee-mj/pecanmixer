# GTK observation preview

This non-installed `0.1.0` preview presents the core's ordered node/port overview
as native strips. It observes connection state, rate and quantum. Port disclosure
controls support keyboard focus; the window reflows strips and scrolls large
collections. Long port lists have a bounded native scroller inside each strip.
No fader, mute, destination, route, meter, recorder, tray or settings control is
active. The legacy application remains the supported mixer.

The GTK binding consumes `pecan_core::overview::Overview`. It only partitions
ports by their explicit input/output flag for display. Stereo/device grouping,
route status, volume, mute and destination rules must come from a future core
presentation model before they can be bound; no domain policy is inferred here.
A live control also requires worker-side epoch validation. Adding either future
interface is outside this presentation-only change.

```sh
cargo build -p pecan-ui-gtk --locked
cargo run -p pecan-ui-gtk --locked -- --fixture fixtures/graph/audio-basic.json
# Display-free projection (unchanged core JSON contract):
cargo run -p pecan-ui-gtk --locked -- --fixture fixtures/graph/audio-basic.json --headless
# Widget tests need a display/session bus, but never need a PipeWire server:
xvfb-run -a -s "-screen 0 1600x1000x24" dbus-run-session -- cargo test -p pecan-ui-gtk --locked
# Optional GTK >= 4.10 accessibility test API, without raising the default API floor:
GTK_A11Y=test xvfb-run -a -s "-screen 0 1600x1000x24" dbus-run-session -- cargo test -p pecan-ui-gtk --locked --features a11y-tests
```

The widget test covers connected/disconnected/empty/reconnected models, an old
snapshot replay, 120 strips, 300 ports in one strip, long names, bounded notice
replacement, preserved disclosure/focus and removed-focus recovery, native scroll
ranges, keyboard focus traversal, and allocations at 1400×900, 900×600 and 420×500.
These are synthetic widget population/allocation assertions. They do not prove
completed desktop frame presentation, performance, Wayland/physical desktop,
AT-SPI/Orca announcements, theme contrast or full keyboard/screen-reader acceptance.
The optional test API only checks that accessible labels are published.

A small local binding publishes accessible labels with GTK's stable
[`gtk_accessible_update_property_value`](https://docs.gtk.org/gtk4/method.Accessible.update_property_value.html)
API: gtk4-rs 0.11 gates its high-level Accessible wrapper behind 4.10. Status
roles are assigned when constructing labels. This preserves the selected base
API in the default build; it is not a runtime test on GTK 4.0. The port scroller
is explicitly hidden when its expander collapses to keep keyboard traversal out
of the unrooted content on GTK 4.18.
