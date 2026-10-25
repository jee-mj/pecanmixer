# Native migration batch 3: recorder shutdown completion (draft)

This increment follows [batch 2](./native-batch-2.md) at
`02b9c489cc538ab6b344d7dc408cb11f8bd49827`. It adds reusable backend shutdown
completion and adopts it in the **legacy Tauri tray quit path**. The GTK executable
still uses its read-only observer; native production ownership, mixer controls,
routing, recording controls and native quit integration remain separate work.
The goal remains the complete functional JS-to-Rust/GTK port, with legacy removal
after parity and cutover gates. Product versions remain `0.1.0`.

## Implemented and independently reviewed

- `Graaf::request_shutdown` cancels admission/queued work and returns a cloneable
  completion handle. Success requires the graph worker and all registered writer
  threads to be joined, with explicit writer finalization results. Unfinished
  writer handles/results survive session reconnects.
- Only a background coordinator joins threads. UI polling and timed waiting do
  not join a supposedly finished thread, which can still be running thread-local
  destructors. A reproduced gated-destructor regression now covers this boundary.
- WAV/MIDI writers explicitly finalize remaining tracks after write/open/close
  failures and sender disconnect. Real synthetic WAV tests inspect finalized
  headers, samples and frame counts. Errors/panics are completed failures rather
  than successful shutdown; error retention is bounded.
- Legacy tray quit replaces the fixed 600ms delay with an off-UI wait bounded to
  five seconds. Timeout keeps the application open and retains the same completion
  handle for retry. Failure keeps it open until an explicitly offered error-exit
  action, which uses a nonzero exit code. Duplicate clicks cannot create duplicate
  waiters or shutdown requests. Normal successful exit requires actual completion.

Cancellation can still allow one already-dequeued command to finish. Queue
admission is not command completion or a server acknowledgement. Finalized bytes
and joined writers do not establish `fsync` durability, crash recovery, protection
against forced termination, or hardware audio correctness. Window-close and
external-termination behavior were not broadened by this tray-only adapter.

## Focused cloud verification

| Check | Result |
|---|---|
| `cargo test -p pecan-core --locked` | PASS: 36 tests |
| Backend library tests | PASS: 35 tests, including lifecycle/writer regressions and gated TLS teardown |
| Legacy application library tests | PASS: 10 tests, including six quit-policy tests |
| Native GTK build and existing Rust examples | PASS |
| Source/privacy/scope and whitespace checks | PASS: synthetic fixtures only; no dependency, lockfile or product-version change |

The backend and legacy checks ran together via
`cargo test -p pecan-core -p pecan-pipewire -p pecanmixer --lib --locked`;
the separate core command includes its integration tests (81 total non-GTK tests).
Independent review also reproduced the original blocking teardown probe and
checked the corrected polling/waiting path and quit state machine. GTK widget
coverage is recorded in earlier batches; it was not repeated for this backend/
legacy-quit increment. The separately prepared mixer candidate is not included.

Not run: live PipeWire/WirePlumber/hardware/restart/audio acceptance, live recorder
or real tray/desktop acceptance, latency/CPU/memory/real-time benchmarks, physical
Wayland/X11 or AT-SPI/Orca workflows, release packaging or final license audit.
Filesystem settings persistence is still a separate, unintegrated adapter.
No merge, release, version bump, JavaScript removal or full-parity claim is made.
