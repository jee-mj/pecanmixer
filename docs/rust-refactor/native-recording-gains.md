# Reviewed native recording level library

Based on `940afbd188dcce86e532141bda5e53a6d46746f7`. This backend-only
candidate adds `Graaf::configure_bus_gains(IdleConfigIntent) ->
Result<ConfigReceipt, ConfigError>`. It reuses the existing exact backend-owner,
epoch and configuration-revision authority, bounded FIFO, receipt and local
readback types. The ordinary `configure_idle` and serialized legacy IPC are
unchanged. There is no UI or Session adoption in this commit. The subsequent
[library integration record](./native-rec-level-library-integration.md) preserves
its bounded clearance and the separate captured-origin gate before live adoption.

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
also captures the locally owned Rec node serial and node/input-port registry
generations; execution revalidates these before the setter and after target
callbacks. An unrelated graph revision is allowed; a replacement object with
the same numeric ID cannot inherit admitted work or a running take's identity.
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

Cloud checks: all 209 backend tests pass (including 12 new focused tests),
all 74 core integration tests and 10 legacy library tests pass, and the native
GTK target passes `cargo check --locked`. Scoped Rust formatting and
`git diff --check` pass. No application or external launcher was started.
