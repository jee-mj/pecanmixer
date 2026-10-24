# Session module placement candidate

This extends the isolated pointer patchbay library. It changes presentation only:
module movement and Rearrange never send audio commands or history inputs, change
routing, write settings or start a backend. Main/application ownership is untouched.
The existing history adapter remains necessary for actual cable edits.

## Legacy source comparison

`src/App.tsx` keeps a session-only position map per audio/MIDI tab. `Patchbay.tsx`
uses a module's key, retains overrides even while modules are absent, clamps moved
top-left coordinates to zero and raises the touched module. Untouched defaults
recompute their right column from the visible viewport. Rearrange clears every
position override for the selected tab. It does not undo routing or clear cable
selection. The native audio view now provides those placement behaviors.

The native view continues to use its existing overview node/side modules and
font-aware metrics. This increment does not introduce grouping/domain rules or
claim the JS grouping, MIDI or pixel geometry is fully reproduced. Full graph
updates reuse the existing core audio module keys, including duplicate suffixes.
Overview-only projections use their existing display name/direction keys. Those
keys are presentation aliases, not authority for an audio operation.

## Ownership and integration

Retain the same `PatchbayOverview` while switching or hiding views. Its position
map lasts for that instance's session; creating a new view starts a new map.
The current presentation stack already retains its views. A future multi-tab
owner must retain a separate view/session per supported tab. Positions have no
serialization or settings representation, and no profile is read or written.

Drag a module header with the primary pointer to move it. Port dragging keeps
its existing cable behavior. Starting a module move cancels any cable draft;
starting a cable gesture cancels a module move. Frames are moved in place and
raised without rebuilding them. Port hit regions, cable endpoints, Bezier
controls and native stacking share the resulting geometry, including modules
crossing columns or covering each other.

`rearrange()` and the native Rearrange button clear all overrides, including
currently absent modules, and recompute defaults for the current viewport. Cable
selection and already-submitted audio requests remain intact. As a conservative
safety guard, Rearrange retires held module/cable gestures; a late release cannot
resume against the reset layout. Escape/focus loss/unmap/native cancellation stop
a module move at its last session position, without issuing an undo/audio action.

Every accepted newer full graph cancels the captured module move, even if its
read-only projection is unchanged. Removal, serial replacement, ID reuse and
disconnect cannot substitute a new target into a held sequence. Position aliases
may deliberately restore a returning named module's last position, matching the
legacy session map; that restoration never authorizes a stale input. A fresh
press is required. Scrolling/resizing retain the captured module identity and
world-coordinate origin; moved overrides remain fixed while untouched defaults
follow viewport width.

Motion replaces one pending position and marks geometry dirty. The mapped canvas
tick updates native frames and cable/hit geometry together. Final release/reset
flushes immediately. There is no accumulating frame queue or movement history;
position-map size follows the number of distinct explicitly moved session keys,
including absent ones, until Rearrange. This mirrors legacy retention and is not
a constant-byte-memory or performance claim.

## Evidence and limits

Synthetic geometry/GTK tests cover clamping, stacking/occlusion, crossed cable
curves, viewport defaults, frame allocation deltas, 3,000 coalesced movements,
retained widget identity, selection-preserving reset, absent-key reset, reconnect
positions, serial/removal/disconnect cancellation, core key reuse and callback
lifetime. Opt-in marked-Xvfb tests exercise native module movement followed by a
real synthetic port gesture, reset during held module/cable input, replacement,
disconnect and scroll/resize during capture. All graphs are hand-authored public
fixtures; no actual device data, desktop screenshots or runtime logs are stored.

Native GTK coordinate comparisons allow small float rounding; frame-allocation
deltas account for toolkit-owned frame borders. These are source/widget and
synthetic input checks, not physical desktop, exact CSS/pixel, touch/Wayland,
keyboard module-movement, assistive-technology, audio, server-restart, CPU/memory
or latency acceptance. Keyboard cable controls and the native keyboard-accessible
Rearrange action remain available. Production integration, independent review and
remaining functional parity are still required. Versions remain 0.1.0.
