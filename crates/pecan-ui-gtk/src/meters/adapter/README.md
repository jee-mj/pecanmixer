# Accepted-batch telemetry presentation

`meters::TelemetryAdapter<I>` is an isolated GTK adapter over the existing
`MeterFeed`/`MeterWidget` and pure mixer/audio projections. It starts no backend,
capture, worker, inbox consumer or polling timer. Main, Session, application owner,
settings, audio commands and backend/core interfaces are unchanged.

The caller passes the batch already returned by its sole consumer, such as the
existing `Session::poll()` call. Do not drain an inbox or call Session::poll a
second time just to feed these widgets. The adapter applies an accepted graph
before that batch's peaks. Peaks require the exact current epoch/revision;
equal-stamp peak frames replace in delivery order. There is one graph/peak feed
and no extra frame queue or idle publication queue.

## Caller authority

For a connected graph publication, supply `MeterSources` with the exact graph
stamp **and owned Arc**, validated `BusConfig::definitions()` and an opaque binding
per node. `I: Clone + Eq` must identify the backend owner/epoch, node generation
and relevant audio-port generations/mapping. Use all audio ports conservatively;
a node-only binding does not detect a replaced port. Bindings must come from the
authoritative witness for that exact publication, not the observer, names,
numeric IDs or serial comparisons. Missing node bindings leave those meters
unbound. Changed tokens force unbind/reset/rebind even for identical public DTOs.

The `fc67e261` base does not export a per-object registry binding type. This
generic boundary allows the later core `RegistryBinding` to be supplied after
its prerequisite is reviewed/integrated, without adding a second backend
identity API. Production wiring must construct/validate that full binding from
the exact native witness; this slice does not manufacture it. The existing
MeterWidget also conservatively requires a DTO serial. Even a supplied identity
does not make a missing-serial source available in this slice.

Missing or mismatched source publications deactivate/unbind existing widgets
and report a projection error. They do not guess a new authority. Explicit
`rebind_current(sources)` requires the retained exact graph, clears old samples
first, and can restore authority or update bus presentation without a new graph
revision. `reset_sources()` removes authority while the caller obtains a witness.

## Projection, mounting and visibility

The pure `Mixer::build` supplies each strip's channel indices: one Channel widget
is retained per selected channel. The pure `audio::layout` supplies each module's
`meter_channels`: one Mini widget is retained per nonempty group. No stereo,
grouping, routing or capture policy is copied into the adapter. `MeterKey`
combines surface, numeric node address, domain item key and bar index only for
presentation lookup; it is never command or registry authority.

`meters()` exposes the current specifications/widgets; `meter(key)` finds one.
The caller mounts them next to the corresponding strip/module and keeps the
same widget when its key persists. Tokens/channels are rebound as needed; labels
are updated from the new domain projection. Removing/disconnecting a source or
dropping the adapter retires externally retained widgets as well.

New widgets and both surfaces start inactive. The shell must set
`set_surface_active(Mixer/Patchbay, visible)` for its window/tab and
`set_visible_keys(surface, keys)` from actual layout/viewport visibility. Only
bound, on-screen meters in active surfaces register GTK frame ticks. Hidden
surfaces retain the last viewport selection; new keys need a new visibility
publication. Unmapping also stops ticks through the existing widget. Scroll
occlusion detection/virtualization remains caller work, not an inferred property
of mapped GTK children. Hidden time has no animation catch-up loop; resume uses
the one latest peak frame.

`new(widget_limit)` bounds the combined retained channel/mini inventory; Default
uses 512. Projection validates the entire desired inventory before adding GTK
widgets. An over-budget projection retires old bindings and reports the limit;
it does not partially mount new widgets. Pure domain projection work is still
proportional to the supplied structural graph; this is not a performance budget
for arbitrary graphs. Peak-only batches do not reproject or rebuild widgets.

## Samples and reset

`clear_samples()` explicitly clears levels/hold/clipping while retaining source
bindings. The caller can invoke it when its authoritative capture state requires
a reset. It must not infer failure from a batch without peaks. The DTO has no
capture timestamp, sequence within a stamp, sample age or unavailable reason.
This adapter adds none: it cannot reject reordered replay at the same stamp or
distinguish a stopped producer from unchanged/absent samples. A node peak vector
can also omit selected channels. The existing pure meter math handles their
values; `MeterReading::has_frame` describes node-frame presence, not capture
health, complete channels, audio arrival or hardware acceptance.

## Synthetic checks and remaining integration

Tests cover domain-derived indices, graph/peak ordering, stale/future stamps,
equal-stamp delivery, unchanged widgets, byte-identical registry replacement,
missing/copied authority, explicit reset/rebind, missing serial/channel data,
disconnect/reconnect/drop, hidden/offscreen resume, a 10,000-frame one-slot storm,
and atomic projection limits on a generic 100-node graph. All graphs/tokens and
labels are authored public synthetic fixtures. There is no server, hardware,
user profile, capture, runtime log or desktop-session dependency.

Cloud checks pass: ten new adapter cases in both default and accessibility modes,
18 focused meter cases with accessibility assertions, the complete 169-test
default GTK suite, native GTK build, scoped formatting and whitespace checks.
GTK checks use a deliberately started Xvfb/private bus with fatal criticals.

Owner wiring, validated production witness generation, mounting/viewport updates
and authoritative capture reset signals still need independent integration.
The executable remains observational and does not mount this adapter. Fixture
population/counters do not establish audio correctness, real-time/CPU/latency
budgets, physical display throughput, Wayland or desktop accessibility acceptance.
