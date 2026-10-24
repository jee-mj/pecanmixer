# Bounded native per-bus takes

This candidate extends independently cleared recording `0e24fd9371c34b27d0d8b5f266549efd7fcab18a`.
It adds independent, immutable per-bus takes and the ownership/resource foundation
for dynamic arming. It does not implement arming orchestration, structural layout
migration, native UI adoption, or new legacy serialized commands/events.

## Rust API and owner integration

`Graaf::recording_snapshot() -> Option<RecordingSnapshot>` uses the short command
registry try-lock. `None` is a busy inspection, not disconnected/default/empty
state. The snapshot includes current `ConfigSnapshot`, `RecordingRevision`, and
at most 64 owned take receipts. Retain receipts independently of later snapshots.
Completed requests leave this list; their absence is not completion evidence.

`RecordingRevision { owner, revision }` changes for request admission, writer
installation/capture attachment, stop/close, exit and registry retirement. Sample
counters and ordinary graph revisions do not change it. The owner is the backend
configuration authority. Exhaustion rejects new starts; stops still work.
Receipts are live evidence, not an atomic frozen report of every writer phase.

`Graaf::start_recording_bus(RecordingBusStart { start: RecordingStart,
expected_recording: RecordingRevision, bus: String })` returns
`Result<RecordingBusStartResult, RecordingError>`:

- `Admitted(receipt)` queues a new take for exactly that configured audio bus.
  The bus need not be armed; this explicit operation does not change arming.
- `NoOp(existing_receipt)` creates no writer or FIFO entry. An exact matching
  bus binding is already reserved. The receipt may still say Queued or Opening:
  this result is not proof of local capture, audio arrival or server completion.

Capture the graph origin and both revisions from the state actually edited.
Admission compares both revisions and validates the original node/port generations,
serial, direction, channel names and rate. Execution/attachment revalidate that
binding. A replacement sharing numeric IDs never becomes an existing-capture
no-op. Invalid/evicted origin, stale revisions, bounded admission contention and
capacity errors are explicit, without modifying existing recordings.

The original `start_recording(RecordingStart)` stays idle-only and starts all
armed buses in one take. There is one unfinished start transition at a time;
already attached siblings continue while another take opens. Retry Busy only as
a deliberate fresh request. Every newly admitted per-bus take gets its own writer,
directory and stable IDs. No existing take's mapping, start time or files change.

The public config CAS revision still advances for the aggregate recording flag.
A private configuration identity distinguishes those flag-only transitions from
actual configuration/readback changes. Thus a start admitted just after the last
capture stops can survive the timer's recording-only revision update. Its report
retains the originally captured ConfigRevision. Logical/legacy configuration
changes, even change-and-change-back, retire that private identity and reject a
pending start. No stale UI request is restamped or admitted against a newer revision.

## Stops and completion

`RecordingStop::{Track,Take,All}` remains usable after disconnect. Track/take stops
still return `Retained(receipt)` for the exact stable identity. Stop-all now returns
`RetainedMany(Vec<RecordingReceipt>)` when multiple requests are affected, including
queued/opening/finalizing takes. `RecordingStopResult::receipts()` exposes all
receipts uniformly, with an empty slice for NoOp. Exhaustive Rust matches need
the new variant; legacy IPC payloads are unchanged.

Stop-all signals every managed native request without ordinary FIFO capacity,
removes queued starts and preserves unrelated queued command order. Duplicate
stops reuse receipts. Recent duplicate-ID lookup remains bounded at 16 completed
requests; caller-retained receipts survive lookup eviction. Native stop-all is
not legacy-recorder or application-shutdown acknowledgement.

Poll every returned receipt. Each take retains the baseline meanings: readiness
is not capture attachment; stop requested is not track finalization; finalized
WAV bytes/header are not actual thread join. A one-track stop does not join a take
whose siblings continue. Timed waits retain pending handles; if aggregating waits
off-UI, use one overall deadline rather than multiplying a timeout per take.

Writer/ingress failure is scoped to its take. A failed addition does not stop an
older healthy take. A writer can join successfully while its recording receipt
retains an ingress/disconnect failure. Likewise common backend shutdown can join
all writers successfully despite a failed recording receipt. Keep both results;
neither is permission to discard the other. Quit uses the common lifecycle handle.

## Capacity and callback boundaries

The backend retains at most 64 queued/opening/active/finalizing native requests,
plus 16 recent completions. Each immutable mapping has at most 64 track entries;
retained descriptors are distinct from live capture and open-file reservations.
Only one live/pending capture reserves a configured bus at a time. Stopped old
tracks and a new take can retain the same bus key with different stable IDs.

The common coordinator still caps tracked writer slots and reserved/open track
slots at 64 each. Writer slots remain occupied until actual join. A track slot
now releases after writer-side explicit finalization consumed/dropped its file
(including a reported close error), or explicit evidence it was not opened.
Unconfirmed/panicked exits retain unreleased slots until actual join. Release is
idempotent; retained receipts/descriptors do not keep a closed-file slot occupied.
This permits ordinary rearming beside a long-running sibling in an older take.
Slow teardown can still exhaust writer/take slots, producing explicit backpressure.

The existing 255 ordinary FIFO slots, reserved stop-all slot and 32-command tick
budget remain. Writer ingress stays at 32 packets per writer, each retaining at
most 65,536 float samples. With 64 writers the sample-queue allocation ceiling is
512 MiB, plus the existing at-most-16-MiB shared channel-block matrix, in-flight
packets and bounded metadata. These are limits, not measured memory/performance
acceptance. The post-gain channel blocks are collected once and fed to each live
take. One take's format/full/busy failure does not skip another take's ingress.
Existing captures survive a temporarily busy writer-status read. No I/O or joins
move into UI or process callbacks; filesystem opening and joins remain off-thread.

## Unfinished parity and tests

Dynamic arming still needs a separate additions-before-removals operation with
explicit partial results, retained previous recordings on failed additions and
authenticated config/membership readback. Structural add/split/join/remove routing
migration, MIDI, gain/arming edits while recording and native UI recording remain
separate unfinished parity work. Configuration mutations stay locked while native
writers are owned. Legacy Stop/StopBus bridge to owned native takes while preserving
the earlier-legacy-stop/later-queued-start FIFO rule and legacy serialized contracts.

Tests use only synthetic registry objects, temporary roots, decoded float WAV
bytes and injected opening/write/TLS gates. Coverage includes separate take maps,
queued/opening/active no-ops, stale config/membership/origins, replacement, repeated
stable stops/rearming, failure isolation, command/sample saturation, disconnect,
64 retained takes, all-receipt stop-all, open-track capacity and close errors.
No server, device, user audio, desktop, hardware alignment, real-time or performance
acceptance is claimed. No release, version bump, PR2 update or UI enablement is included.
