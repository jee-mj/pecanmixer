# Native running elapsed observations

This backend-only candidate starts at
`940afbd188dcce86e532141bda5e53a6d46746f7`. It adds no UI/Session wiring, timer,
worker, dependency, serialized legacy payload or version change. Existing native
transport presentation continues to show active timing unavailable until its
owner adapter deliberately adopts this evidence.

## Read-only API

`RecordingReport.elapsed: Option<RecordingElapsed>` accompanies the actual
retained receipt's existing source, take and immutable track mapping:

```rust
pub struct RecordingElapsed {
    pub observed_at: std::time::Instant,
    pub duration: std::time::Duration,
    pub running_tracks: Vec<TrackId>,
}
```

`duration` is local monotonic wall time since this take's first validated capture
attachment, measured at `observed_at`. `running_tracks` identifies only open,
unstopped tracks eligible for that observation. It is bounded by the existing
64-track limit. Grouped tracks share their take's start, as in the legacy active
take model; separately started takes have separate starts. Coincident timestamps
never establish sample alignment.

The original `graaf::begin` assigns `Actief.t0` when installing an active legacy
take; its transport publication uses the maximum active age and each bus's take
age. `src/Transport.tsx::Looptijd` interpolates that wall time. Native recording
already distinguishes opening from `StartedLocally`, so its corresponding origin
is first capture attachment after writer readiness and identity/format validation.
Queue admission, WAV opening, readiness alone, UI receipt arrival and sample/byte
counters cannot start this clock. Legacy behavior remains unchanged.

## Lifecycle semantics

| State/event | Running elapsed evidence |
|---|---|
| Queued, opening, partial initialization or ready but not attached | `None` |
| First validated local attachment | Store one immutable monotonic origin |
| Repeated pump, busy writer-status read, duplicate/no-op start | Preserve that origin |
| Stop one grouped track | Exclude its stable ID immediately; surviving tracks keep the take origin |
| Dynamic arming addition | Its independent take gets a new origin at attachment; unchanged tracks keep their original origin |
| Stop take/all, ingress fault, known writer failure, lost capture or disconnect | No running observation |
| Finalizing, finished/joined, failed or unconfirmed ownership | `None`; no historical file duration is invented |
| Reconnect | Old receipts remain stopped; any newly admitted take needs its own validated attachment |
| Common tracked-writer shutdown | Its stop signal suppresses running elapsed before the next capture pump |

This measures an attached take's local wall time. It does not measure captured
samples, WAV duration, audio arrival, successful ingress, server convergence or
file finalization. An observation can have positive elapsed while accepted and
written sample counters are zero. There is no pause accumulator or retrospective
stop/finalization duration in this slice. The reviewed transport helper already
uses unavailable timing for unknown/finalizing activity and zero only for known
idle state with no unfinished receipts.

The origin is one `OnceLock<Instant>` per existing request, initialized on the
graph worker. Reports use nonblocking existing status reads; busy status still
returns `None` from `try_report()`. No lock, time read or allocation is added to
the audio ingress callback. The recording queue, membership revisions, writer
ownership, finalization and admission policy are unchanged.

## Owner adoption contract

Retain `observed_at` with `duration`, including when caching a report. Cloning a
cached observation does not make it fresh. Map elapsed only to the listed stable
track IDs from that actual receipt, and continue to honor recording/track phases,
owner/epoch authority and inspection coverage. Do not copy one sibling's elapsed
to another independently started take.

Before passing values to the existing transport helper, translate the measurement
instant to its owning clock's monotonic origin or compensate for measurement age
coherently. Never stamp cached duration with the current UI delivery time. Multiple
take observations can have different measurement times; account for those ages
before choosing the oldest running age. This API does not claim an atomic graph,
writer-status and lifecycle transaction, and later failures/stops can supersede
an observation. No owner adoption is included here.

Respect `elapsed: None` even if another status field still reflects an earlier
started phase while a writer exits. Never infer a replacement clock from that
phase or a prior positive observation.

## Synthetic verification

Six focused cases use existing temporary-WAV/lifecycle fixtures and fixed
observation instants. They cover gated opening/readiness before attachment,
attachment-time bounds, a pre-start observation, zero samples, cached delivery,
10,000 revision-neutral reads, partial track stops, separate/no-op takes, dynamic
arming continuity, stop/fault/disconnect and common shutdown. A
fault-injected WAV path also verifies that a known track write error removes
running evidence while another track's finalization is still gated. The existing
unconfirmed-start probe also requires no elapsed evidence. No timing sleep is
used to manufacture recording age; existing bounded fixture readiness waits only
synchronize actual writer lifecycle.

Final cloud checks pass: 203 backend tests (including these six cases), ten
legacy library compatibility tests, the native GTK compile check, changed-source
formatting and whitespace checks. The existing application adapter compiles
without modification and continues to leave timing unavailable. No assertion or
legacy payload was removed or weakened.

No real service, device, user audio, profile, physical desktop, sample alignment,
latency or performance acceptance is exercised. Native UI timing adoption remains
a separately reviewed integration step.
