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

## Read-only patchbay increment

Open **Patchbay** with the native page switcher, or start directly on that page:

```sh
cargo run -p pecan-ui-gtk --locked -- --fixture fixtures/graph/audio-basic.json --patchbay
```

This is an observation surface. It displays each node's explicit output ports
on the left and input ports on the right, with native module/port labels and
Cairo cables behind them. It consumes the existing ordered `Overview` and the
existing snapshot's `Kabel` endpoint data; it does not port or duplicate app,
hardware, stereo, monitor-hiding or route policy. Nodes without ports remain
available in Mixer overview. There is no drag, rearrange, connect, disconnect,
route, meter capture or command-dispatch path.

Every observed link also appears as a named native row in **Connections**. Use
Tab to reach the list and arrow keys to inspect rows. Long visual labels are
ellipsized; full labels and IDs remain in accessible names/tooltips. Missing or
inconsistent endpoints suppress only their decorative line: the connection row
and a visible count remain. Disconnection clears both graph and connections.
The native split pane and both scroll surfaces handle window resizing.

The binding delegates stamp acceptance to `OverviewState` and updates the
patchbay on every accepted graph stamp, including link-only snapshots that do
not change the overview. Equal structural models retain their widgets. The
existing capacity-one wake/latest-value inbox remains unchanged. One current
model/geometry is retained, with widgets proportional to the current graph;
these collections are not virtualized or performance-benchmarked.

The six crate tests now include three display-free deterministic geometry tests,
a binding fixture test, and two GTK widget suites (the existing mixer suite plus
the patchbay suite), serialized onto GTK's test thread. Patchbay coverage includes
exact port-row endpoints, stable layout under reordered input, missing endpoints,
font-metric expansion, 10,000 coalesced snapshots, link-only updates, stale replay,
connection selection/focus recovery, long labels, 240 modules/160 links, scrolling,
and 1400×900, 900×600 and 420×500 allocations. The optional `a11y-tests` feature
checks published native accessible labels. `PECAN_GTK_TEST_SNAPSHOTS=<existing
writable directory>` optionally saves GTK fixture snapshots for inspection.

These remain synthetic population/allocation checks. No live server, hardware,
Wayland/physical desktop, Orca/AT-SPI workflow, theme/high-DPI acceptance, rendering
throughput, memory/latency benchmark or interactive-patchbay parity is established.
The package and default GTK API feature floor remain unchanged.

## Explicit settings storage boundary

`settings_storage::SettingsFile` is a synchronous, app-side adapter for the
validated core `SettingsV1` contract. It is not called by startup or the preview
and does not apply preferences. A future shell must call it off GTK and audio
process threads and serialize writers. The caller supplies an absolute file
path in an existing directory; there is no HOME/XDG discovery or implicit
directory creation.

`load()` returns settings, the core load status, exact original bytes and an
opaque snapshot token. `save(&loaded, &next)` requires a missing or valid native
document and checks that the disk content still matches that snapshot before
writing and before replacement. It writes a unique owner-only sibling, syncs
the file, renames it over the target and syncs the directory. On Linux the
rename is the atomic visibility boundary. These content checks are not an
interprocess lock or a guarantee against changes racing the final check;
multi-instance coordination remains a shell integration dependency.

Corrupt JSON, invalid UTF-8 and unsupported schema versions stay untouched by
ordinary saves. Only an explicit `recover(&loaded, &next)` can replace them:
it first creates and syncs a unique, byte-exact owner-only recovery backup and
syncs its directory. Complete backups remain available if replacement later
fails. Errors expose the phase, whether replacement already happened and any
completed backup path. After a post-rename error callers must reload; a
directory-sync failure leaves power-loss durability uncertain. Unfinished
staging files are ignored and are never adopted or automatically deleted on
load. Normal error cleanup removes only this operation's own staging file.
Existing non-regular targets, including symlinks, are rejected at the read checks.

Legacy import remains an explicit data operation: a caller may pass an exported
key/value map to core `migrate_web` using `JsonDecoder`, review its diagnostics,
then supply the resulting validated settings to `save` or `recover`. This
adapter never searches profiles, opens WebKit databases or imports adjacent files.

```sh
# Display-free tests; all storage is confined to fresh test temporary directories:
cargo test -p pecan-ui-gtk --locked --lib settings_storage
```

The tests cover native schema roundtrip, absent/valid/corrupt/future input,
invalid UTF-8, byte-exact recovery, detected concurrent edits, pre/post-rename
faults, staging leftovers, retry semantics, missing directories, non-regular
targets and explicit legacy export input. Fault injection is deterministic;
these tests do not simulate a kernel crash, power loss or a physical filesystem
failure. No GUI, autostart, tray or runtime settings integration is provided.
