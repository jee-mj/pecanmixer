# Native local idle bus configuration candidate

This backend-only slice is based on PR2 `dec5dbfa`, independently of the tracked
writer candidate. It adds no UI, recording start/stop, writer integration,
settings persistence, MIDI enablement or serialized IPC fields.

## Native API

`Graaf::config_snapshot() -> Option<ConfigSnapshot>` inspects bounded owned local
state. `None` means the short command registry lock is busy, not disconnected.
The snapshot has `connected`, `recording`, optional `local: BusReadback`, optional
`configuration: IdleBusConfig`, optional derived `transport: Opties`, and a
`ConfigRevision { owner, epoch, revision }`. `local` reports the filter's actual
channel count, gains and optional node ID. It is not observed port publication.

`Graaf::configure_idle(IdleConfigIntent { ticket, expected, configuration })`
returns `Result<ConfigReceipt, ConfigError>`. The caller must retain the revision
that produced the edited UI state; stamping an older configuration with the
latest revision at admission defeats compare-and-apply intent. `owner` binds the
request to one process-local backend instance, `epoch` to one connection session,
and `revision` to local configuration changes. None is a graph revision or
persistent setting. Counters never wrap into a reusable authority; exhausted
configuration revisions reject further native requests until the next epoch.

`IdleBusConfig { buses: BusConfig, clock: Option<u32>, start_stop: bool }` reuses
core's bus definitions, effective gain/mute calculation and armed bus selection.
There is no second bus planner. A native request replaces the complete desired
idle configuration. The caller's local pending model must resolve its receipt
before treating that value as applied.

`ConfigReceipt::try_report()` never waits for its status lock, worker or server;
`None` means busy. Its report retains the caller ticket and expected revision:

| Progress | Meaning |
| --- | --- |
| `Queued` | Admitted to the existing bounded command FIFO. No application has completed. |
| `Applying` | The worker claimed this request. Cancellation cannot roll it back. |
| `Completed` | A typed local result and readback are retained. This does not acknowledge server observation. |
| `Unconfirmed` | The request was dropped abnormally without a completion report. Effects may be unknown. |

`ConfigCompletion` contains `LocalConfigOutcome::{Applied, NoOp,
Rejected(ConfigError), PartiallyApplied(ConfigError)}`, the resulting snapshot,
an optional `ChannelResizeResult`, and `cancellation_requested`. A claimed request
can finish after shutdown is requested; its actual local result remains explicit.
A rejection or failure is not automatically retried or disguised as success.
Receipts remain independently readable after later requests and reconnects.

Local revision increments on successful configuration changes, partial physical
resize changes, legacy configuration commands, recording-state transitions and
changes to filter readback. Exact native no-ops do not advance it. Admission and
execution both validate the expected revision, so competing requests admitted
against one revision cannot both silently replace it. A retry uses the returned
current snapshot, with deliberate reconciliation by the caller.

On disconnect the epoch changes, pending requests are rejected and their capacity
is cleared atomically with ordinary commands. Shutdown also rejects queued work.
Neither transition replays old desired configuration into a new session.

## Validation and resize boundary

Native input permits 1–64 total channels in mono/stereo bus definitions, at most
64 widths and 128 bounded state entries, keys of at most 256 bytes without NUL,
finite dB values, and finite effective gains in `[0, 4]`. Invalid values fail
without silently applying the legacy clamp. Inactive state entries may remain,
matching core/settings retention. `buses.midi`, a clock source, and MIDI start/stop
are explicitly unsupported in this native slice. Native callers must use MIDI
off; legacy defaults and serialized transport options retain their behavior.

The worker checks actual recording activity before changing gains, mute, arming
or transport options. Dynamic updates while recording return `RecordingActive`.
Arming is derived into the legacy `Opties.bussen` representation and read back
alongside the logical bus state. Both gains and stored/effective native transport
options are checked after application. A mismatch is partial/unconfirmed local
application, never a false successful result.

`Bus::try_set_channels(requested) -> ChannelResizeResult` reports `before`,
`requested`, `actual`, and optional `ResizeFailure::{InvalidCount, AddFailed,
RemoveFailed}`. It uses the same allocator loop as injected tests. Failed removal
keeps the failed port in local ownership for a future retry. Gain/peak storage
follows actual capacity, including partial growth. A partial resize does not
apply the requested gains/options and clears invented logical configuration
metadata; callers see actual capacity and a new revision if it changed.

The native contract permits initial metadata adoption for the current channel
count and unrouted idle growth. Native channel shrink is rejected, even with no
currently observed routes: an asynchronous mirror is not proof that removing
ports is safe. Layout changes with existing bus routes are also rejected. It
does not connect/disconnect links or implement staged migration. Split/join/remove
with route and active-track migration ordering remains explicit unfinished
parity work, not a permanent restriction on the final application. Local port
allocation alone proves neither graph-observed ports nor recording readiness.

## Queue, ownership and legacy compatibility

Configuration requests share the existing 256-slot FIFO, normal capacity 255,
stop-all reserved final slot, and 32-command per-tick budget. At most 16 native
configuration requests may wait in that FIFO; the single worker can additionally
have one claimed request. Admission uses a nonblocking lock attempt and returns
`Busy` or `QueueFull` explicitly. No separate priority queue changes command order.
Payloads and retained reports are bounded; consumer-owned receipt clones remain
the caller's responsibility. No new filesystem operation, join or wait runs on
the UI or audio callback.

The legacy void/clamping `Bus::zet_kanalen`, gain setter, command payloads and
recording behavior remain supported. Legacy setters invalidate native logical
metadata rather than inventing mute/grouping/arming state from raw gains. The
snapshot still reports actual local channel/gain readback. Bitwise float readback
comparison permits stable invalid legacy values to be repaired by valid native
input without endless revision churn. No invalid native values are admitted.

`NativeIntent` continues to reject `ZetBus`, `ZetTransport`, `Start`, `StartBus`,
`Stop`, and `StopBus`; this distinct API cannot become a graph/history success
receipt. No UI has been wired to it. The native owner quit gate, tracked writer
readiness/finalization, recording commands, dynamic arming, staged routing,
observed-port coordination and folder actions need separate integration/review.

## Synthetic validation

Tests use generic in-memory filter ports and public synthetic graphs only. They
exercise admission versus local completion, exact gain/mute/arming/options
readback, invalid formats/gains/MIDI, stale and competing revisions, backend-owner
replacement, no-ops, partial allocation/retry, typed removal errors, native shrink
and routed-layout rejection, active recording rejection, queue saturation and
stop reservation, per-tick budgets/FIFO, legacy invalidation, disconnect/reconnect,
busy polling, revision exhaustion, readback mismatch, cancellation during a
claimed apply and repair of invalid legacy readback.

These tests are source/headless evidence. No PipeWire/WirePlumber service, audio
device, actual port allocation, recording, latency/performance benchmark or
desktop acceptance session was used. Observed/server completion remains distinct
from every local result above. Versions remain `0.1.0`.

| Cloud check | Result |
| --- | --- |
| `cargo test -p pecan-core -p pecan-pipewire -p pecanmixer --lib --tests --locked` | Passed: 64 core, 82 backend, 10 legacy; zero failures. |
| New local configuration tests | All 15 passed. |
| `cargo build -p pecan-ui-gtk --locked` | Passed; no GTK source changed. |
| `cargo check -p pecanmixer --examples --locked` | Passed. |
| Changed Rust formatting and `git diff --check` | Passed. |

The isolated cloud toolchain used Rust 1.99.1, GTK4 4.18.6, PipeWire 1.4.2,
GTK3 3.24.49 and WebKitGTK 2.52.6. No runtime dependency was added.
