# Held history activation correction candidate

The owner candidate at `b32af909c17b28c532bad2ce9eecc4cbefd62f22` reused its Undo/
Redo buttons and read the latest presentation when `clicked` arrived. Holding
pointer/Space on an entry, then replacing its object, could prune that entry and
make release undo/redo a different newly exposed entry. Focus/Escape cancellation
and delayed activation also needed a real presentation lifetime.

This narrow correction changes application history-button handling only. The
finalization shell, settings drain, reviewed lifetime/tray implementation,
backend/core, mixer controls, patchbay, main startup and dependencies stay
unchanged. It does not integrate the separate layout, writer, fader or bus
candidates. Main startup and real native recording remain disabled/unadopted.
The corrected owner candidate still requires independent re-review before PR2
integration or production startup.

## Binding and cancellation

The shared Session provides a private, opaque UI history-context token. Planning,
accepted graph/witness changes, terminal receipt/history progress, invalidation
and admission shutdown retire that context. This token is UI presentation
authority, not a replacement for the backend's original graph Arc or registry
witness. It never serializes or supplies invented object generations.

Each button instance closes over an immutable context/origin. A context change
retires/removes its old callbacks and constructs replacement controls. Late
`clicked`/`activate` signals cannot adopt those replacements. The callback also
checks the current Session context, covering a no-command history transition
that changes the stack without publishing a new graph. Button labels, duplicate
labels and stack lengths do not authorize an entry.

One main-window observer tracks activation keys/pointer/touch started on the
history buttons. Retiring a held input blocks replacement controls, suppresses
its repeat/release and resumes availability after an observed release. A release
on another widget in the same window still ends the old input. Focus loss,
Escape, grab/touch cancellation and unmapping cancel the applicable sequence.
Fresh native presses and explicit activation of current controls work after
release. No queued cancellation is reinterpreted as an action on another entry.
Ordinary graph updates conservatively cancel held history activation too; the
user can start again against the new presentation. Idle refreshes retain the
buttons and their focus. The PatchbayOverview and cleared shutdown wiring are
retained.

## Evidence

The same six opt-in GTK/XTest groups were run against an unchanged production
source copy of `b32af90` and the correction. All six fail before and pass after:

- Pointer and Space holds for both Undo and Redo while the pressed entry is
  pruned, including repeated subsequent graph revisions.
- Focus/release on another widget, Escape cancellation and fresh Return.
- Cancelled held repeats, retired activation signals and fresh current activation.

A separate GTK regression also fails before and passes after: another caller
consumes an already-satisfied undo step without changing the graph, then a
retained old button attempts activation. It must not target the next entry.
Existing direct-callback tests now explicitly verify retired callbacks remain
retired and fetch the current control for fresh activation.

Final combined checks:

| Check | Result |
|---|---|
| Core/backend tests | PASS: 64 core + 67 backend |
| Legacy library tests | PASS: 10 (141 total non-GTK tests) |
| GTK default / accessibility modes | PASS: 132 each |
| GTK accessibility + XTest + isolated tray D-Bus | PASS: 153, including seventeen XTest and four isolated D-Bus cases |
| Original finalization shell cases | PASS: all sixteen; shell/drain/lifetime/tray implementation unchanged |
| Native build / five Rust examples | PASS |
| Scope/privacy/whitespace | PASS; all inputs synthetic/public fixtures |

Native input is delivered on deliberately created Xvfb displays/private session
buses. No user desktop, audio server, profile or actual device data was used.
Physical touch/Wayland/cross-application/accessibility acceptance and real-time/
performance remain untested. The release observer covers the containing window;
missing releases outside it remain a conservative input-recovery/desktop
acceptance limitation, rather than permission to rebind a held input. This
correction does not establish full shortcut/queued-input or functional parity.
No merge, release, version bump or JavaScript deletion is included.
