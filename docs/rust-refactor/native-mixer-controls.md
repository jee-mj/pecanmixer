# Native mixer controls candidate

The executable remains read-only. This candidate adds library-level strip gain
and whole-node mute controls using the exact `NativeIntent` / `NativeReceipt`
contract. Independent UI review and production ownership/history wiring are
required before enabling live controls. The goal remains the complete functional
JS-to-Rust/GTK port; this candidate does not complete mixer parity.

## Dispatch and observed state

One cloneable `native_backend::NativeClient` retains a supplied native owner and
shares ticket allocation across mixer/patchbay clients. It forwards the exact
owned `Stamped<Arc<Momentopname>>` presentation origin to `dispatch_native`, never
a reconstructed graph or the separate read-only observer's frame. It does not
start a worker, choose settings paths or change main startup. Application-owned
shutdown/finalization remains an integration responsibility.

The controller permits one pending request, displays observed values and reads
native queued/local, observed/no-op, rejected and unconfirmed states separately.
An observed result must be followed by a graph at least as new as its reported
stamp; admission/local execution cannot release the gate. A subsequent external
change remains visible without leaving an already-proven observation pending.
Observation is a server-fed postcondition witness, not a causal transaction ack.
Failures may have partial effects and are not reported as rollback.

## Gesture origin correction

Each scale and mute button has its own origin capture, independent of other
controls and of the current row-key map. Capture-phase input observation records
mouse/touch, keyboard and scroll beginnings without claiming GTK's gestures.
Key repeat and continuous scroll retain their original input lifetime. Focus
after a captured button press preserves that press's origin.

When a newer graph changes the presented binding, a captured gesture is cancelled
rather than rebound. Release-time callbacks retain the cancelled state; only a
fresh input beginning can rearm against the current presentation. An
acknowledgement cannot resume a held fader against a newer origin. Overlapping
input sources cancel the interaction. Focus loss/unmap also cancels it.

Rows with changed node ID/serial, channel selection or port IDs receive a new
widget lifetime. Removed/replaced rows retire both origin captures permanently.
Retained callbacks close over that retired control, so they cannot look up a
new same-key row or target it. Programmatic graph refresh suppresses value/toggle
signals and never dispatches an audio intent.

This deliberately cancels a held gesture even on a same-target newer graph. It
is a conservative safety boundary, not full drag/wheel coalescing or one-step
history parity. Physical pointer/touch/keyboard, accessibility action and
screen-reader acceptance are still untested. Synthetic input-lifetime and GTK
signal regressions establish source behavior, not those desktop outcomes.

## Focused cloud checks

The resumed candidate passes 40 core, 45 backend and 10 legacy tests, native GTK
build and all five example checks. The GTK package passes 39 tests in each
default/`a11y-tests` mode under Xvfb/session bus with fatal criticals. This includes
17 pure control/origin/adapter tests, five control widget tests, earlier
presentation/patchbay tests and 11 display-free storage tests. The earlier parked
no-display GTK failures and ambiguous compatibility-wrapper status were resolved
by these properly configured reruns. All input graphs/settings are synthetic;
no audio server, device, user profile or physical desktop was exercised.

## Unfinished parity and review gates

See the [functional parity matrix](./native-parity-status.md). Partial-strip mute
gain restoration, Rec-bus controls, continuous gestures/rounding/reset behavior,
independent output/Rec selection, routing, complete undo/redo, settings runtime
effects and native recording/tray/quit integration remain unfinished. Nodes
without serials are unavailable to this first controller. Backend timeouts can
report unconfirmed effects; this controller does not invent completion evidence.

The candidate reconciles the published lifecycle/storage code with the bounded
native contract, without changing its shared core DTOs or signatures. Further
backend receipt-dedup compatibility work must be reconciled before integration.
No dependency/version change, JS deletion, merge, release, hardware acceptance or
performance claim is included. It is published on a temporary review branch,
not directly to PR2 before independent clearance.
