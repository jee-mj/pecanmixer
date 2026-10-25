# Native patchbay input candidate

The native executable remains observational. This module offers isolated audio
patch intents through the existing keyboard/history adapter; no production owner
or backend sender is attached here. No core/backend DTO, startup, dependency
version or legacy JS behavior is changed by this increment.

## Input and history boundary

Bind `PatchbayOverview::bind_history_actions` to the application's shared history
controller. Feed it only exact full snapshots from the retained native owner via
`update_graph`; the callback receives that same `Stamped<Arc<Momentopname>>` and
one `history::Input::Execute`. Match its Arc to the backend's retained opaque
origin. Never reconstruct identity from IDs or the read-only observer.

The callback's `Ok(())` acknowledges handoff only. The application must drive
`set_actions_pending` from admission failures, real completion receipts and graph
observation. A graph update alone does not clear pending work; Escape cannot
cancel an already-submitted backend request. Core history owns command plans and
undo boundaries; this widget introduces no request ticket or undo/redo dispatcher.
Backend failure may have partial effects and is not presented as rollback.

`update(Overview, links)` is the read-only projection path. Calling it suspends
both pointer and keyboard editing, even if the projection is unchanged. Resume
with a strictly newer accepted full `update_graph` snapshot. This prevents mixed
observer/full-graph feeds from retaining an editable stale presentation origin.

## Pointer behavior

- Primary-button drag connects output to input or input to output. Only existing
  core audio-layout endpoints are editable. Green feedback marks a compatible,
  nonduplicate target; unsupported, missing and same-side ports cannot connect.
- Clicking a cable selects it; clicking it again clears sole selection. Shift
  toggles membership. Selection is shared with the non-spatial connection list;
  Delete/Backspace submits one disconnect history action for the selection.
- Starting on a connected input picks its selected incoming cable, or its sole
  incoming cable. Multiple unselected incoming cables start a reverse connection.
- Moving to a new compatible input submits one `Action::Move`. Releasing a picked
  cable without a compatible target submits `Action::Disconnect`. This preserves
  the legacy behavior, including release over its original input, a duplicate or
  wrong-side target. The status explicitly describes this consequence.
- Escape, native gesture cancellation, focus loss and unmap preserve the original
  cable. Drafts never alter graph DTOs or insert an observed link optimistically.
- Every newer accepted graph cancels an active draft, including same-target
  revisions, disconnect/reconnect and endpoint replacement. Cancelled held input
  cannot adopt the new origin; release only retires it. A fresh press is required.

The canvas uses GTK gestures and deterministic canvas-coordinate hit regions.
Scrolling and resizing do not change routing state or reconstruct origin data.
No persistent positions, auto-scroll, MIDI editing,
production mutation, recorder/tray/settings effects or full patchbay parity is
claimed.

## Bounds and checks

A single active draft retains one exact graph Arc and replaces its latest point.
Motion queues a GTK redraw and does not rebuild structural widgets or accumulate
frames. Hit testing uses fixed subdivisions per observed cable; this is bounded
source work, not a real-time performance guarantee. The existing graph feed and
structural equality checks retain their coalesced update behavior.

Synthetic fixture tests cover both directions, exact origin, duplicate/wrong-side
rejection, selected/ambiguous input pickup, move/drop/cancel, one history handoff,
late release after graph changes, pending/rejection, missing endpoints, read-only
feed suspension, deterministic geometry, larger font metrics, and 3,000 motion
updates without structural replacement. Existing large/long-label and resize/focus
checks remain in place.

The opt-in `x11-input-tests` feature adds native XTest delivery through an explicitly
marked Xvfb/GDK/GTK fixture: connection in both directions, cable selection and
Delete, move/drop/Escape, graph replacement, scroll/resize during drag, and unmap.
It uses installed X11/XTest libraries only in the test binary. Start a fresh Xvfb
fixture, select `GDK_BACKEND=x11`, and set `PECAN_XTEST_FIXTURE_DISPLAY` to that
fixture's `DISPLAY` before running the feature's tests serially. Never point these
tests at a user desktop. No hardware or profile information is used or recorded.

These are source/widget/native synthetic input checks. Physical pointer/touch,
Wayland, cross-application capture/focus, assistive-technology actions, actual audio,
server recovery, CPU/memory/latency measurements and packaging acceptance remain
unperformed. Full application integration and independent review are still needed.

Session-only module dragging and Rearrange are described in the
[placement contract](./session/README.md); they send no audio/history command.
