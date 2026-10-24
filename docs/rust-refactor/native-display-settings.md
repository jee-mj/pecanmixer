# Display settings and explicit persistence worker candidate

This isolated increment is based on mixer candidate
`d42c9ef9ae18f17d112ea85c1a1b75c56e121c11`. It is not wired to main startup or a
production native owner. **That base has an independently reproduced focus-return
input-lifetime defect and is not cleared for live controls or PR2 integration.**
This settings increment does not fix or supersede that defect.

## Presentation contract

`DisplayPreferences::from(&SettingsV1)` selects hidden strip/row keys, row weights
and output-destination width only. Bus gain/layout/arming, MIDI clock and transport
preferences are not applied to audio. `MixerControls::set_display_preferences`
filters rendered widgets while retaining its complete domain mixer, graph Arc,
origin stamp and command/history data. Hidden/replaced controls retire their
callbacks; showing a strip creates a new control without pruning backend state.

`PreferenceLayout` supplies three independently scrolled sections, saved row
weight proportions and the output/Rec presentation split. Hidden rows redistribute
visible proportions; empty rows remain unless explicitly hidden, matching the
legacy row policy. A null destination width uses the output's natural width;
large saved widths clamp to the current allocation without rewriting settings.
Application is one-way: user resize/hide menus and save-trigger wiring remain
separate work. This does not claim full CSS/desktop behavior parity.

## Persistence contract

`SettingsWorker::start(SettingsFile::new(explicit_absolute_path)?)` explicitly
starts one serialized I/O worker and emits initial load result 0. It discovers
no HOME/XDG/profile/browser path. `submit` accepts `Reload`, `Save(SettingsV1)` or
`Recover { from: reviewed_loaded_snapshot, settings }` and returns a worker-local
request ID. Submission is admission, not saved state. Both queues have capacity
16; a full request queue returns the operation/document for deliberate retry.
Rapid updates retain FIFO semantics for accepted operations, not silent dropping
or implicit coalescing.

Consume `Loaded`, `Saved`, `Recovered`, `Failed` and `Blocked` events by request
ID. A failed operation blocks further saves until explicit reload or successful
explicit recovery. In particular, an already-published replacement error does
not reuse the old token or blindly retry. Recovery uses exactly the original
bytes the caller reviewed; changed input is rejected and successful recovery
retains its backup. Storage source/checks and core DTOs are unchanged.

The caller owns display state: an older save result must not overwrite newer
in-memory preferences. Wait for and handle initial load before enabling editing
in the future shell. One worker per path must be owned across that shell's
lifetime; this is not an interprocess lock. `close()` stops admission and returns
the bounded result receiver plus thread handle. Drain results before joining
**off-UI**. Dropping the worker detaches and does not establish queued-save
completion. Native application quit integration is not included.

## Verification and limits

- Four display-free worker tests pass: gated rapid requests/queue-full retry,
  same worker-thread identity for file operations, draining a pending save on
  close, detected external edits, explicit reviewed-input recovery, and reload
  after a simulated post-rename error. All use fresh synthetic temporary files.
- Pure display tests retain the full graph/model/origin and allow history planning
  for hidden objects; unrelated audio preferences do not change display settings.
- Synthetic GTK tests exercise weight/width allocations, hide/show filtering,
  retained callbacks and zero submitted audio commands across rapid display edits.
- All 48 GTK-package tests pass in each default/`a11y-tests` mode under Xvfb/session
  bus with fatal criticals; native GTK build and scoped format/whitespace checks
  pass. Core/backend/legacy suites were not repeated: their source is unchanged
  by this increment. Dependency manifests, lockfile and versions are unchanged.

The passing suite does not clear the separately reproduced base focus defect.
No main startup, recorder/tray/clock/bus action, meter widget, user profile or
actual audio device was touched. No hardware/audio, physical desktop/AT-SPI/Orca,
performance, crash/power-loss or packaging acceptance was run. Filesystem faults
are deterministic injected conditions, not kernel/power-loss simulations. Full
functional parity and independent review remain required before cutover.
