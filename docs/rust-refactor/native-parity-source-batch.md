# Native functional source candidate

This candidate follows draft PR2 `fc67e2617345c652ad7f8ee158e1461aef381621`.
It combines independently cleared source; the combined-tree gate is still
pending. The full functional JavaScript-to-Rust/GTK port remains the goal.
**Main startup is unchanged and observational. No real native recording,
telemetry mounting, profile discovery or settings migration is enabled.**
The legacy application and serialized IPC remain available during migration.

## Reviewed constituents

| Original candidate | Included source | Remaining adoption gate |
|---|---|---|
| Recording `0e24fd93`, multitake `b5c33745` | Exact-origin/config/membership-bound starts, immutable per-bus takes, stable stop identities and retained opening/attachment/finalization/join evidence | Native owner/transport adapter, dynamic arming and live audio acceptance |
| Bus model `fff6eba` / `2647263`, corrected widgets `de142062` | Independent Output/Rec choices, typed configuration handoff, correlated pending/terminal readback and retained track status | Owner/config/recording adapters, editable gain/mute and staged structural migration |
| Gesture `aae6e3c6`, correction `5608d2f4` | One in-flight sample/latest desired value, observed final full-vector history, receipt-committed partial mute memory and capture-or-cancel input | Combined reconciliation gate and production/physical input acceptance |
| Settings bridge `d8af0567` | Loaded/disk/edit/request revisions, display-only filters/resize/reset, explicit recovery and final unsent-document drain before worker close | Combined reconciliation gate, explicit startup path policy and audio settings effects |
| Telemetry `2354e31c` | Already-consumed batch adapter, exact graph before peaks, retained channel widgets and explicit identity/visibility resets | Authoritative binding factory, mounting/viewport updates and capture freshness/availability protocol |

Dynamic-arming work is not included. Source existence and queue admission do
not make a native capability available in the executable. Bus/recording and
telemetry libraries remain isolated rather than starting another owner or inbox
consumer. MIDI stays disabled in native defaults.

## Reconciliation and independent gate

The recording/backend files exactly match `b5c33745`; core gesture additions
exactly match `5608d2f4`. Bus files exactly match `de142062`; meter files exactly
match `2354e31c`. The settings delta is unchanged except its mixer file also
retains the reviewed gesture correction. Relative to that correction, the mixer
adds only the settings delta's existing nine-line `bind_display_edits` /
`set_display_editable` hooks. Its mute observer, immutable mute actions, keyboard
release cancellation and latest desired keyboard position remain intact.

Manual cherry-pick conflicts were limited to retaining both library module
exports and both documentation entries. No new backend/command/receipt policy,
controller behavior or production wiring was written for reconciliation.
The startup file, workspace/manifest/lock files and versions are unchanged.
Nevertheless the corrected gestures and settings share the application surface;
the combined candidate must pass a separate independent gate before PR2 adoption.

The combined suite retains the strengthened before/after regressions for held
mute semantic substitution, held autorepeat after unconfirmed completion and
keyboard increments separated by refreshes. Settings resize/filter/save/quit
tests run alongside them. A matching terminal handoff report is retained outside
the bus view; replacing a publication or widget is never cancellation evidence.

## Evidence

Final combined cloud checks:

| Check | Result |
|---|---|
| Rust core/backend | PASS: 67 core + 134 backend |
| Legacy library | PASS: 10; 211 total core/backend/legacy |
| GTK default/accessibility | PASS: 238 in each mode |
| GTK accessibility + XTest + isolated tray D-Bus | PASS: 277; thirty-five XTest and four isolated D-Bus cases |
| Native GTK build / five existing Rust examples | PASS |
| Connected/disconnected/reconnected CLI fixtures; mixer/patchbay smoke fixtures | PASS |
| Reviewed-source equivalence, scope/privacy/whitespace | PASS; no new reconciliation behavior |
| Native normal Cargo tree / ELF | PASS: no legacy web runtime, GTK3 or extra GUI framework |
| Independent combined-tree review | PENDING; constituents are cleared separately |

There is no final check failure or dependency/environment blocker. All GTK
checks use a deliberately started Xvfb/private bus and fatal criticals, with
Node absent from the test command path. Synthetic WAV/fault tests do not open a
PipeWire session. Fixtures and temporary directories are authored/generic only;
no actual user devices, topology, captures, private logs or profile data are used.

## Functional and acceptance limits

Native graph completion is a server-fed postcondition, not a causal server
transaction acknowledgement. No-op history still needs fresh progress; local
execution, gesture End and queue admission are not applied-write proof. Ordered
commands can partially apply and cancellation can allow one claimed operation
to finish. Legacy IPC retains its pre-admission stale-intent limitation.

Recording local attachment, initialized WAVs, per-track finalized bytes,
function-finished status and actual background join are distinct evidence.
Retain all affected receipts and use the common shutdown handle for quit.
One take's ingress failure must remain visible even if its writer joins
successfully. These source checks establish neither captured audio nor alignment,
real-time/lossless behavior, fsync or crash durability.

Telemetry requires the exact native graph Arc/stamp plus an authoritative
owner/epoch/node and relevant audio-port generation/mapping token. A node-only
token is insufficient. Missing serial stays unbound. Existing peak DTOs have no
capture timestamp, intra-stamp sequence, generation or detailed availability;
equal-stamp replay and stopped-producer detection remain unavailable. Node-frame
presence does not establish complete selected channels or capture health.

Settings remain explicit-path, one serialized worker per path. Older results
advance only the disk baseline while newer local edits survive. Errors after
replacement require actual reload; corrupt/future input requires explicit
reviewed recovery. Final unsaved data is retained on failure, with all joining
off the UI thread. No automatic profile import or audio/config effect is added.

Still unfinished: executable/GApplication/CLI adoption, live meters and viewport
integration, native route selectors/reset, bus/transport/recording adapters,
dynamic arming, staged routed shrink/migration, complete shortcuts/queued input,
folder actions, durable module positions, autostart and final cutover. The
[parity matrix](./native-parity-status.md) separates these from implemented source.

Not run: live PipeWire/WirePlumber/hardware/audio/restart/recording acceptance,
physical KDE/GNOME/Wayland/cross-app/AT-SPI/Orca workflows, real-time/performance
benchmarks, power-loss/crash durability, release packaging or final license
closure. No merge, release, version bump or premature JS deletion.
