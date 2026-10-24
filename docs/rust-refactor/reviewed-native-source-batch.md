# Reviewed native source integration batch

This draft increment follows PR2 `dec5dbfa0526f7709f6114a0865cc2a2ed43adf1`.
It combines independently reviewed library slices while retaining the existing
legacy application and observational native executable. The goal remains the
full functional JavaScript-to-Rust/GTK port. **No executable startup cutover or
real native recording is enabled.** Source/fixture clearance is distinct from
hardware, audio, desktop and performance acceptance.

## Included reviewed source

| Source candidate | Included scope | Integration/acceptance still missing |
|---|---|---|
| Owner/corrected history `3c1f75b9` | One session/window/hold/clock, exact graph/witness receipts/history, finalization-aware shell, admission gate, off-UI settings drain and captured/retired Undo/Redo contexts | Main/GApplication/CLI adoption, process arbitration, physical desktop acceptance |
| Keyboard/pointer `973ba290` / `b0ec1dae` | Port create/remove/move, selection/delete/cancel through the shared native history handoff | Production shell adoption and physical input acceptance |
| Lifetime/tray `d3ad825` / `17ca0dcb` | Reserved quit, leased host evidence/fallback, tagged shutdown, pending/retry/completed-error handling | Real desktop/host/session-manager acceptance; autostart |
| Writer `86fdd510` | Bounded tracked audio FIFO, readiness and per-track finalization, retained take/track identities, function-finished versus real joined completion | Capture attachment, native recording commands, timestamps/alignment and recording UI |
| Idle configuration `5cadf0ba` | Owner/epoch/local-revision compare-and-apply; local gain/mute/arming/options/readback; typed partial resize | Native bus editor/adoption, observed ports, recording-time edits, shrink/routed migration |
| Session layout `a8d472af` | Retained module movement/rearrange, clamping/aliases, cancel/reconnect behavior on the same PatchbayOverview | Durable position persistence, production tab/desktop acceptance |
| Fader `14119b96` | Pure legacy drag/wheel/reset/grouping proposals and synthetic JS oracle fixtures | Controller identity/arbitration, coalesced dispatch, final observed completion and one history entry |
| Partial mute `52f15fee` | Pure selected-channel gain-memory proposals and commit/undo vectors | Stable caller identity, serialized proposals, receipt/graph completion, invalidation and widget/history wiring |

Earlier pure policy, settings storage/worker, isolated meters, graph command
identity and legacy finalization work remains intact. Each current workspace
retains its single PatchbayOverview across page switches. Main still instantiates
its earlier observational presentation, not the functional owner workspace.

## Reconciliation

The owner lineage remains intact at `3c1f75b9`. Writer and idle-config changes to
shared backend files merged automatically; the combined differences are exactly
the reviewed additive deltas. The only writer/config conflict was a test's
`Vec::new()` versus `vec![]`, resolved to the former with unchanged test behavior.
Layout and pure fader/mute source is retained exactly, with both module exports.
No new command protocol or dispatch/reducer logic was written to reconcile them.

A separate small integration adjustment constructs explicit native defaults via
`application::default_idle_configuration()`: MIDI off, no clock source and no MIDI
start/stop. The native history/mixer projections use those defaults. This merely
composes the existing validated defaults with the native API's required disabled
MIDI flags; it starts/applies nothing. The core/legacy BusConfig default and saved
settings remain unchanged. Neither new pure gesture policy is automatically
called by widgets in this batch.

## Evidence and runtime limits

Constituent independent reviews exercised real synthetic GTK/XTest delivery,
source/oracle parity, generation-safe history, actual synthetic WAV contents and
fault injection, local configuration partial failures/competing revisions and
isolated D-Bus host failure/recovery. The held-history regression suite failed on
the old candidate and passed on its correction; the settings/shutdown source
remained unchanged by that correction. Reconciliation audits compare exact source
and normalized additive backend patches against those reviewed candidates.

Final combined cloud checks:

| Check | Result |
|---|---|
| Core/backend Rust tests | PASS: 64 core + 96 backend |
| Legacy library tests | PASS: 10; 170 total core/backend/legacy |
| GTK package default/accessibility modes | PASS: 159 each |
| GTK accessibility + XTest + isolated tray D-Bus | PASS: 185; includes twenty-two XTest and four isolated D-Bus cases |
| Native GTK build / all five existing examples | PASS |
| Exact-source/additive-overlap, whitespace/privacy/scope audits | PASS; no novel protocol reconciliation; only native-default composition and module/test spelling integration |
| Native normal Cargo tree / ELF | PASS: no Tauri/Wry/WebKit/JavaScriptCore, GTK3 or extra GUI framework; Node absent from command PATH |

GTK runs use Xvfb/private session buses with fatal criticals. All final checks
pass, with no dependency/environment blocker. No runtime logs, captured audio or
user device information are published as test evidence.

Admission is not completion. Native graph observation is a server-fed
postcondition, not a causal transaction acknowledgement. Batches can have partial
effects; one claimed command may finish after cancellation. Local configuration
completion is not graph-observed ports. Queued config and local configuration
revisions must never stand in for native graph/history acknowledgements. Caller
intent must retain its edited origin/revision, rather than stamping old edits with
the latest authority at admission.

Writer readiness only proves initialized WAVs, not attached producers or captured
audio. Per-track finalization and function-finished status do not establish joined
take completion. Rejected audio vectors are consumed; overload is explicit, and
there is no real-time/lossless guarantee. Legacy writer queues retain their
existing behavior and are not described by the new tracked-path bounds.

Pure fader End and mute proposals are not applied-write evidence. Future
integration must coalesce/arbitrate input, validate stable identities separately
from graph origin, await receipts/observations and commit history exactly once.
The current controller still supports the earlier bounded gain/whole-node mute
path, rather than full continuous gesture/partial-pair behavior. MIDI remains
disabled. Recording, dynamic arming, full bus migration, telemetry availability,
settings editor/startup/save adoption, complete shortcuts, folder actions,
autostart and cutover remain unfinished.

All fixture provenance is public/synthetic. No actual user device names,
identifiers/topology, captures, private logs, user profile data or unrelated
integration context belongs in this batch. JS oracle scripts are optional test
provenance tools; the native binary has no Node/JS runtime dependency.

Not run: live PipeWire/WirePlumber/hardware/audio/restart/recording acceptance,
physical KDE/GNOME/Wayland/cross-app/AT-SPI/Orca workflows, real-time/performance
benchmarks, fsync/crash/power-loss durability, release packaging or final license
closure. No merge, release, version bump or premature JavaScript deletion.
