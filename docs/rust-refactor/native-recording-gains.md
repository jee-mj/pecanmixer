# Native recording level configuration candidate

Based on `940afbd188dcce86e532141bda5e53a6d46746f7`. This backend-only
candidate adds `Graaf::configure_bus_gains(&HistoryGraph, IdleConfigIntent) ->
Result<ConfigReceipt, ConfigError>`. It reuses the existing exact backend-owner,
epoch and configuration-revision authority, bounded FIFO, receipt and local
readback types. The ordinary `configure_idle` and serialized legacy IPC are
unchanged. There is no UI or Session adoption in this commit.

## Accepted operation

Only the dB and mute state of current known Rec buses may change. Layout,
effective arming, MIDI, transport/clock settings and unknown saved bus states
must remain unchanged. Existing validated logical metadata, transport options
and matching local gain readback are required; raw gains cannot reconstruct
muted dB or arming. Missing default states for known buses are supported.

Like the existing JS Rec setter, changing mute preserves dB, and changing dB
preserves mute. The caller merges only its selected bus states into the
captured configuration. Recomputed unselected gains are bitwise unchanged.
The actual Rec filter applies gain before either writer's sample ingress.
This operation calls only the gain setter and readback, never the channel
allocator or recording-options setter. Existing take/track IDs, immutable
channel/port mappings, capture attachments and recording membership remain
unchanged. Level edits do not enter graph undo history.

## Authority and completion

Admission and execution validate the exact configuration revision. Admission
also requires the caller's original authenticated `HistoryGraph`, obtained via
the existing `Graaf::history_graph` accessor for the exact observed publication
when the control's origin is captured. Retain that witness through the gesture;
do not obtain a new witness from the latest graph during admission. The original
one-argument gain API at `04d0c890` had only an admission-time binding and is
replaced, with no weaker gain overload exposed to a retained native UI.

Under the same admission lock, the backend validates this witness's original
owned Rec serial, input channel assignment, node/port generations, backend
authority and epoch against current registry state. A configuration CAS alone
cannot detect an object replacement that leaves the local gain vector and
numeric node ID unchanged. Such stale origins now return `StaleRevision` before
consuming queue capacity, both before and after replacement graph publication;
an authentic current origin can deliberately target the replacement.

The existing opaque registry binding allows retained witnesses to outlive
ordinary graph revisions and publication-cache eviction. Foreign/forged
authority, reconnect, changed serial or reassigned `rec_N` inputs fail closed.
The admitted work retains only its bounded local binding; execution revalidates
it before the setter and after target callbacks. A replacement object with
the same numeric ID cannot inherit retained UI intent, admitted work or a
running take's identity. No new core types or serialized contracts are added.
The complete owned audio input set must currently be observable, so graph-lag
or missing/ambiguous ports produce an explicit unavailable result rather than
binding a different Rec instance.

Pending/opening native starts, arming and structural migration are busy.
Previously started native takes can coexist while running or finalizing.
Legacy recorder ownership remains locked. This does not change MIDI's existing
disabled status or add recording layout migration.

`Queued` and `Applying` describe pending local execution. A completed `Applied`
requires exact local gain readback and unchanged recording options, with
`resize: None`; it advances the configuration revision and identity. Existing
running takes deliberately validate their original mapping without requiring
their old configuration identity. Exact retries return `NoOp` without a new
revision. A changed dB while muted still commits the logical dB even though
the effective gain remains zero.

No-effect failures are rejected. Known partial effects retain actual local
readback and invalidate uncertain logical metadata. Authority or binding loss
after a claimed setter reports partial effects and does not overwrite a newer
configuration/epoch with old readback. Cancellation drops queued work; a setter
already executing may return an applied local result with
`cancellation_requested: true`. None of these outcomes claim server graph
convergence, audio arrival, rollback or a sample-aligned automation boundary.
Unknown configuration after a partial result requires owner reconciliation;
the caller must not treat it as a successful edit or guess missing metadata.

## Bounds and validation

The operation shares the existing 255 ordinary FIFO entries, 16 pending config
entries, 32-command dispatch tick and reserved stop-all slot. Per-intent input
and metadata limits are unchanged (at most 64 channels); captured bindings are
bounded by that channel count. No new worker, timer, join, file operation or
lock is added to the audio callback. The only process-loop refactor extracts
its existing sample multiplication for the synthetic WAV regression.

Focused tests use generated graph objects and temporary synthetic float WAVs.
They cover gain/mute/unmute including dB changes while muted, exact sample
bytes/headers, unchanged track mapping/options/membership, worker polling after
each edit, no-op/stale-owner/stale-revision rejection, opening/legacy gates,
invalid structural/arming/transport proposals, same-ID replacements before and
after the setter, readback failures, disconnect/cancellation and queue bounds.
They do not use a server, audio device, user's recordings or profile. Hardware,
audio quality, latency, real-time performance and desktop acceptance remain
unrun. Native owner/UI gain admission remains a separate integration task.

The initial candidate passed all 209 backend tests (12 new focused tests),
74 core integration tests, ten legacy tests and a native GTK compile check.
The origin follow-up adds four regression groups for pre-admission replacement,
original serial/channel mapping, foreign/copied witnesses, cache eviction and
reconnect; the original lifecycle/WAV cases now supply authentic origins too.
Current follow-up checks pass all 213 backend tests (16 focused), ten legacy
tests and `cargo check -p pecan-ui-gtk --locked`. Core source is unchanged; its
prior 74-test receipt is retained. Scoped Rust formatting and `git diff --check`
pass. No application or external launcher was started.
