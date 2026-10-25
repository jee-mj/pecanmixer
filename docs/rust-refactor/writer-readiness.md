# Tracked WAV writer preparation

This backend prerequisite adds an audio-only tracked writer alongside the legacy
recorder, based on PR2 `dec5dbfa`. It does not enable native recording commands,
attach capture producers, change serialized IPC, or implement bus controls.

## Callable contract

`pecan_pipewire::opname::tracked_writer(root, Vec<Spoor>, rate, &Shutdown)` returns
`io::Result<TrackedWriter>`. The returned value owns a clonable `WriterSender` and
`WriterReceipt`. Format/budget validation and thread creation happen at admission;
directory reservation and WAV initialization happen on the writer thread. An
admitted request can therefore fail during initialization. The caller supplies
the root and must validate command origin/configuration before calling this API.
No filesystem operation or join runs in the admission method.
This is preparation for backend-internal dispatch: the live owner coordinator is
not exposed to GTK. `request_shutdown()` must not be used to obtain a coordinator
for starting work, because it also stops admission.

`receipt.take_id()` and `receipt.track_id(index)` produce opaque `TakeId` and
`TrackId` values. Take identities are process-unique, never wrap, and do not depend
on a timestamp, bus name, or reusable vector index. A track identity includes its
take. A sender rejects identities belonging to any other take. Separate takes
reserve separate child directories atomically, including across process restarts
that reuse an integer ID. IDs are not persisted recording-library identities.

`receipt.try_report()` returns an owned snapshot, or `None` if the short status
lock is busy. `try_complete()` returns only a joined report. Both methods inspect
status without joining. `wait_timeout(Duration)` is an off-UI polling convenience;
it returns the same retained joined report or `WriterPending::{Busy, Running}`.
Its deadline bounds waiting apart from scheduler and bounded snapshot overhead;
it cannot cancel a filesystem call or thread teardown. A timeout keeps the
receipt usable.

| Evidence | Meaning |
| --- | --- |
| `WriterPhase::Opening` | An admitted writer is reserving its directory/opening files. |
| `was_ready` / `WriterPhase::Ready` | Every requested WAV was initialized. This does not prove a producer was attached or audio captured. |
| `TrackPhase::Closing` | A close request was accepted; completion is still pending. |
| `TrackPhase::Finalized(result)` | An explicit WAV finalization call returned. The result includes preceding write errors or accepted samples that could not be written. Successful finalization covers the WAV header and buffered bytes, without `fsync`. |
| `WriterPhase::Finalizing` | Stop or final cleanup is in progress. |
| `WriterPhase::Finished` | The writer function returned and retained its outcome. Thread-local teardown/join may still be pending. |
| `WriterPhase::Joined` | The background coordinator actually joined the writer and released its capacity reservation. |

`WriterReport::is_success()` requires `Joined` and `WriterOutcome::Finalized`.
`StoppedBeforeReady` is explicit successful cleanup after cancellation observed
before readiness, not a successful start. Initialization or I/O failure is
`Failed(String)`. Per-track `NotOpened` means no successfully initialized writer
was obtained; initialization failure can still leave a partial file. `Unconfirmed`
means an abnormal exit prevented proof of explicit finalization. No phase equates
queue admission, cancellation requested, or local file creation with PipeWire
server acknowledgement.

`TrackReport` retains its ID, original name/channel count, phase, accepted sample
count, and written sample count. Accepted samples that are discarded after
another track fails cause an error on the affected track, even if its empty or
partial WAV header finalizes correctly. A write/close error stops the take;
cleanup attempts finalization of every other open WAV and retains errors.

## Sending and limits

`sender.try_audio(TrackId, Vec<f32>)`, `try_close(TrackId)`, and `try_stop()` use a
bounded FIFO with nonblocking lock admission. They return `WriterSendError`:
`Busy`, `Full`, `NotReady`, `Closed`, `WrongTrack`, or `InvalidSamples` as applicable.
An error means the message was not accepted; `try_audio` consumes its supplied
vector even on failure. A future producer adapter must handle overload explicitly
and size/chunk packets appropriately. There is no automatic retry or promise of
lossless real-time delivery. Finite samples are required.

Audio is rejected until readiness and after an accepted track close/stop.
Close/stop acceptance is idempotent. One close slot per track and one stop slot
are reserved independently of audio saturation, and remain ordered after earlier
accepted packets. `Busy` still requires a caller retry; this is not a completion
acknowledgement. Closing the last track alone leaves the take waiting for stop or
sender EOF. Dropping the last sender drains accepted packets and finalizes.

The tracked path accepts 1–64 tracks per take, 1–64 channels per track, nonzero
sample rate with checked WAV byte-rate arithmetic, names at most 256 bytes with
distinct nonempty sanitized filenames, and a root path at most 4096 bytes. Each
take permits 32 queued audio packets of at most 65,536 samples each, including
the vector's allocated capacity (up to 8 MiB
of queued sample storage plus the currently processed packet), at most one close
per track, and one explicit stop. Names/status/error text are bounded; individual
error strings retain at most 4096 UTF-8 bytes. Directory reservation tries at most
128 unique names. One backend owner permits at most 64 tracked writers and 64
total tracked tracks, counting opening, running, and not-yet-joined writers.
Reservations are released only after a real join, or a failed spawn.

`Graaf::request_shutdown()` also signals tracked writers. That flag rejects new
audio and makes each writer drain earlier accepted messages before finalization,
even if a producer retains a sender. The writer checks the owner flag while idle
at a five-millisecond polling interval. A reservation counts as pending shutdown
work before the thread is spawned/registered, preventing an early successful
shutdown report. All joins remain on the existing background coordinator. A
blocked teardown can delay other joins; UI polling remains pending and does not
join. Global `ShutdownReport` retains writer failures under its existing limits;
the per-take receipt retains detailed evidence independently.

## Compatibility and remaining work

Legacy `schrijver`, its `(mpsc::Sender<Bericht>, JoinHandle<...>)` result,
`Actief`, Dutch IPC payloads, and existing recording commands keep their contracts.
The legacy path still uses its existing unbounded sample channel and synchronous
directory reservation; these new tracked-path bounds do not describe that path.
Both paths share WAV processing/finalization code. Existing MIDI behavior remains
on the legacy path; this new sender has no MIDI input.

Native `ZetBus`, `ZetTransport`, `Start`, `StartBus`, `Stop`, and `StopBus` remain
unsupported. Capture attachment, observed routing/configuration completion,
dynamic arming, simultaneous starts, track-to-bus bookkeeping, recording UI,
folder actions, and the native owner quit gate must be deliberately integrated
and reviewed before enabling native recording. Structural active-layout migration
(split/join/remove with route and active-track migration ordering) remains an
unfinished parity item, not a new permanent bus-edit restriction.

Tests use temporary synthetic roots, deterministic sample blocks, and injected
open/write/seek/flush failures and teardown gates. They verify actual WAV headers
and samples, partial initialization, opening cancellation, duplicate requests,
stable identities, bounded saturation, retained loss/error status, sender EOF,
owner shutdown, registration races, and pending timed waits. They provide source
and headless compatibility evidence only. Hardware, server routing/recovery,
audio timing/alignment, performance, power-loss durability, and desktop acceptance
remain untested.

## Cloud checks for this candidate

| Command | Result |
| --- | --- |
| `cargo test -p pecan-core -p pecan-pipewire -p pecanmixer --lib --tests --locked` | Passed: 64 core, 81 backend, 10 legacy tests. |
| `cargo test -p pecan-pipewire --locked` after the retained-allocation bound regression | Passed: 81 backend tests, 0 doc tests. |
| `cargo build -p pecan-ui-gtk --locked` | Passed. No GTK source changed. |
| `cargo check -p pecanmixer --examples --locked` | Passed. |
| Changed Rust formatting and `git diff --check` | Passed. |

Fourteen new backend tests cover the tracked lifecycle. The existing native
rejection regression additionally covers all six recording/configuration opcodes.
The environment used Rust 1.99.1, GTK4 4.18.6, PipeWire 1.4.2, GTK3 3.24.49 and
WebKitGTK 2.52.6 from the isolated cloud toolchain; no service was started.
