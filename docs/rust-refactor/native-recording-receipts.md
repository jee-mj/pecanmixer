# Native recording start/stop candidate

This is a bounded backend integration of reviewed writer `86fdd510` and idle
configuration `5cadf0ba`. PR2 was `dec5dbfa` at initial verification and advanced
to `fc67e2617345c652ad7f8ee158e1461aef381621` with both prerequisites integrated.
Their combined backend/core source matches this slice's prerequisite base. The
final candidate carries only the recording delta onto `fc67e261`.

The initial prerequisite combination had one overlap: the identical six-opcode
native rejection regression used `Vec::new()` versus `vec![]`; one copy was
retained. Original source commits remain immutable. No shared core DTO, legacy
serialized command/event payload, GTK widget, settings or tray file changes here.

## Callable contract

`Graaf::start_recording(RecordingStart { ticket, origin, expected_config, root })`
returns `Result<RecordingReceipt, RecordingError>`. `origin` is the exact owned
native graph frame, and `expected_config` is the local `ConfigRevision` observed
when planning the request. Copies/reconstructed graphs and stale configuration
revisions fail closed. The caller supplies a recording root of at most 4096 bytes.
Admission performs no filesystem operation, wait or join.

This first native start records all configured armed audio buses in one take,
with one WAV per bus. It requires known native idle configuration, observed owned
Rec-bus ports, a nonzero graph rate and no existing native/legacy recording.
One native request may be queued, opening, active or finalizing per backend.
No armed buses is an explicit error. MIDI and dynamic start/arming remain absent.

Start shares the existing 256-slot command FIFO, 255 normal slots, stop-all
reservation and 32-command tick budget. The worker revalidates configuration,
captured object generations, node serial, exact Rec channel ports and nominal
rate before writer admission and capture attachment. Ordinary graph revisions
and origin-cache eviction do not substitute or invalidate unchanged objects.
The take retains bounded identity metadata, not an old full graph.

`Graaf::stop_recording(RecordingStop::{All, Take(TakeId), Track(TrackId)})` returns
`Result<RecordingStopResult::{NoOp, Retained(RecordingReceipt)}, RecordingError>`.
Stops address this native recorder namespace, not legacy take names. `NoOp`
means no managed native request; it is not legacy-writer or backend-shutdown
completion. Quit must use the common backend shutdown completion handle.

Stops do not depend on a connected graph or an epoch match. Whole-take/stop-all
requests signal the writer atomically, including when its sample FIFO is full.
Stop-all also cancels and removes a queued native start without waiting for a
worker/session. No file was opened for that rejected queued request. `Busy` from
the short command-registry admission lock means the request was not admitted and
needs an explicit caller retry; it is never silently accepted.

Take/track IDs become available after writer admission, during Opening. Stop-all
can cancel Queued before those IDs exist. A track stop records one bounded bit
in `track_stops`, including during Opening. Subsequent native callbacks skip
that track; the worker retries its reserved FIFO close until accepted. This
requested bit does not acknowledge file closure. Stopping the last mapped track
also requests whole-take finalization. Already-running callbacks may finish a
bounded block; stop is not rollback of admitted samples.

Duplicate stops reuse the same retained receipt. The backend retains at most
16 completed requests for duplicate-ID lookup; older/foreign IDs report
`UnknownTake` rather than targeting a replacement. Caller-owned receipts remain
readable after backend retention eviction. Track mapping is installed once and
remains immutable through partial track stops, disconnect and completion.

## Receipt meanings

`RecordingReceipt::try_report()` is nonblocking and returns `None` if a short
status inspection is busy. `wait_timeout(Duration)` is an off-UI polling helper;
it never opens files, joins, forcibly cancels I/O or turns timeout into success.
Timeout retains the same usable receipt.

| Progress | Evidence |
| --- | --- |
| `Queued` | Admitted to the bounded command FIFO. |
| `Opening` | Writer admission began; files may still be opening. Even WAV readiness alone does not mean capture attached. |
| `StartedLocally` | All WAVs initialized and the worker attached the captured immutable channel mapping. This proves neither audio arrival nor graph/server acknowledgement. |
| `Finalizing` | Stop/failure requested or writer returned; actual join may still be pending. |
| `JoinedSuccess` | Capture had attached, no native failure was retained, and the writer actually joined after successful explicit WAV finalization. Zero samples is possible and remains visible in writer counters. |
| `Failed` | A typed failure is retained. Inspect `writer`: failure before writer admission has no WAV/join claim; failure after admission completes only when that writer joins. |
| `Unconfirmed` | Abnormal worker/start exit prevented a trustworthy result. This is not rollback, finalization success or permission to exit. |

`RecordingReport` includes source stamp/config revision, `started_once`, stop
flags, immutable `RecordingTrack` descriptors, detailed `WriterReport` and a typed
failure. `is_complete()` requires actual writer join or a known rejected start
with no admitted writer. `is_success()` requires `JoinedSuccess`.

For an individual track stop, inspect that track's `WriterReport` phase/result.
It can be `Finalized` while sibling tracks continue and the take remains
`StartedLocally`. `wait_timeout` waits for the entire take, not an individual
track. Writer `Finished` is never upgraded to joined by polling; slow thread-local
teardown stays pending. Finalization means finalized WAV headers/buffered bytes,
without an `fsync` or power-loss durability guarantee.

Disconnect fails an active/opening take explicitly, stops ingress and finalizes
accepted bytes off-thread. A later reconnect never replays the start or rebinds
its tracks. A take already intentionally stopped is not retroactively failed by
later disconnect. Shutdown stops queued/native/legacy work through the existing
common lifecycle path; all writer joins remain on the background coordinator.

## Sample ingress and ownership

The existing Rec filter/DSP path feeds the native capture through its post-gain
channel blocks. Legacy capture behavior remains separate and unchanged. Native
ingress validates the actual callback clock fraction against the immutable WAV
rate and caps each track packet at the writer's 65,536-sample allocation limit.
At most 64 channels/tracks are configured, so the native channel-block matrix
is bounded (at most 16 MiB for mono tracks), followed by at most one packet per
track per callback. The writer retains its 32-audio-packet limit and reserved
close/stop capacity. Native capture is on the existing graph loop, without
enabling `RT_PROCESS` or moving joins/I/O into a process callback.

Contention, saturation, invalid samples, format change or unavailable channel
blocks fail the take explicitly and request stop. A rejected ingress packet is
not silently counted as written. All earlier admitted packets drain/finalize.
A writer can therefore finalize valid files successfully while the native take
reports an ingress/disconnect failure. This is not a lossless-delivery,
latency, CPU, memory-performance or hardware acceptance claim. Existing null DSP
buffers follow legacy silence substitution; no audio-arrival proof is implied.

Opening/active/finalizing native ownership rejects configuration mutation and
another start. A preceding legacy config/start still wins FIFO precedence and
can reject the queued native request. An earlier legacy Stop does not cancel a
later queued start; out-of-band native stop-all deliberately cancels queued starts.
Legacy Stop/StopBus can stop an already-owned native take/track. Standalone legacy
IPC, setters, recorder start/stop and serialized transport events retain their
supported behavior. Existing transport event payloads describe the legacy path;
native owner integration must consume these recording receipts explicitly.

Two narrow prerequisite integrations are intentional: `WriterSender::request_stop`
adds an atomic FIFO-draining stop signal; recording-only transitions advance local
config revision while retaining unchanged bus metadata. A separate config-locked
check protects Opening/Finalizing without pretending Opening is active recording.
The writer fault-injection constructor is crate-visible for headless tests.

## Remaining parity and validation

Real UI recording remains disabled until independent review and native owner
quit-gate integration. Per-bus start/dynamic arming, multiple simultaneous native
takes, structural split/join/remove migration, settings/folder actions, MIDI,
receipt wake/render wiring and full transport adoption are separate unfinished
parity work, not permanent product restrictions. Original `NativeIntent` recording
opcodes remain unsupported; this is a distinct typed Rust API.

Tests use synthetic registry graphs, injected opening/write/teardown gates and
temporary roots. They inspect actual WAV samples/headers and distinguish admission,
readiness, attachment, per-track finalization, join, cancellation and failure.
No PipeWire/WirePlumber service, user audio, device data or desktop session was used.
No hardware, real-time, audio alignment/performance or release acceptance is claimed.
