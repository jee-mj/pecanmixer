# Combined native UI source batch (draft)

This reviewed source batch follows backend head
`82d2f7d2546607cbbb44c07947218b6065100f0d`. It combines independently reviewed mixer
control/gesture source, display preferences/serialized settings persistence and
isolated meter widgets. **Main startup remains unchanged and read-only.** This
is a prerequisite toward full functional JS parity, not an alternate product or
activation of production controls, undo, recording or meter capture.

## Composition and review gate

- Mixer source comes from `7a12de4d755f9db667060f9c10d8533707489a97`, including
  original-origin cancellation, retired callbacks and window-level value-key
  release recovery. The shared native client retains the reviewed exact intent/
  receipt contract. Application ownership/history wiring remains separate.
- Display/persistence delta comes from `4e3115544a62b8b3756d83f9edcb421983242b10`.
  Hidden strip/row keys affect presentation only; full graph/model/origin state
  remains available. Row weights and destination width are applied one-way.
  One worker per explicit path serializes I/O, reports admission separately from
  saved results, and blocks saves after errors until explicit reload/recovery.
- Meter source comes from `7a4805d20911519cb36ea0ea60f3136284633617`. It retains
  one latest frame and stable widget wrappers, with per-channel/mini presentation,
  decay/hold/clipping and active/mapped tick lifetime checks. It starts no capture
  and is not mounted by the existing preview.

The display merge preserves both `PreferenceLayout` and the shared
`KeyboardRelease` observer in the constructor, then passes that observer and the
correct fader/mute role to each control's input capture. The library export merge
retains all modules. Meter, display, persistence and origin/keyboard source files
otherwise match their reviewed candidates exactly. No backend/core/legacy or
main-startup implementation changes are included.

Independent review cleared the constructor reconciliation at
`856ba2b7fffdfdc7540f16cb10bd78def6b2cb4b`, including hide/show and pending-input
probes. The stronger regression delta
`cd93e2ee53778620ecc71974736c856171f64341` is included: releases on an unrelated
entry before/after acknowledgement cannot strand a fresh input, while held repeat
remains cancelled. All five opt-in XTest cases pass in the combined tree.
Constituent review, reconciliation review and final synthetic checks clear this
source publication gate; they do not clear live application/desktop acceptance.

## Integration contract and remaining functional gates

A production shell must own one native backend/inbox, deliver exact accepted
owned origins to controls, and reconcile receipt/history results before controls
are enabled. Generation-aware old history replay remains a separate unresolved
integration gate. Pointer patchbay work is excluded.

A settings shell must consume the initial load before editing, retain one worker
per path, match request results without rolling newer in-memory state backward,
and drain bounded results before joining off-UI. No profile paths are discovered
or accessed here. Bus/clock/transport settings do not become audio commands.
Interactive preference editors and save triggers remain separate.

Meter integration must feed graph before peaks; match exact epoch/revision; keep
equal-stamp delivery order and wrapper lifetimes; use the domain's channel indices;
explicitly rebind replacement sources; call `clear_peaks` when capture is absent;
and deactivate off-screen widgets. The existing backend capture-retention identity
correction is independently reviewed but remains outside this batch. **Do not
enable live capture based only on this widget-source review.** No causal server acknowledgement or performance
acceptance is inferred from synthetic meter frames.

The [parity matrix](./native-parity-status.md) remains authoritative for unfinished
mixer/routes/selection/gesture/history, recording/buses, settings/tray/desktop and
cutover work. The legacy application is retained. No merge, release, dependency
addition, version bump or JS removal is included.

## Checks and evidence boundary

Final combined checks pass: 42 core + 53 backend + 10 legacy tests,
59 GTK-package tests in each default/accessibility-test mode and 64 with XTest,
native GTK build and all five example checks. GTK runs use Xvfb/session bus with
fatal criticals. Reviewed-source identity, scope/privacy and whitespace checks
pass. Native dependency/linked-library inspection finds no Tauri/Wry/WebKit/
JavaScriptCore path. Tests run without Node/npm on PATH. Checks use
only hand-authored synthetic graphs/settings and temporary files. No actual audio
device names/IDs/topology, audio capture, user profile or private runtime logs are
used. Real audio/hardware/restart, physical desktop/Wayland/AT-SPI/Orca, cross-app
input, CPU/memory/latency/real-time, crash/power-loss, distribution and license
acceptance remain unperformed gates.

The later [identity batch](./native-identity-batch.md) integrates the separately reviewed history and capture corrections described above; production mounting and live startup remain disabled.
