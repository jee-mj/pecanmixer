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

## Focus-return recovery correction

Independent synthetic input probes reproduced three failures in the previous
candidate: Tab release on mute stranded a fader, a released-off-widget Shift
stranded a fresh arrow after refresh, and release/refocus after an observed held
Right request prevented a second request. All three were reproduced here using
XTest events delivered through Xvfb/GDK/GTK before the correction (0/1, 0/1 and
1/2 submissions respectively).

The correction captures only a control's value/activation keys. Tab and modifier
keys never become fader gestures. A containing-window capture observer delivers
key releases to the original control even after focus moves or pending controls
are disabled. Focus loss still cancels rather than rearming an active gesture.
Held-key ownership cannot migrate to another/replacement control; a retired or
dropped origin remains blocked until release. Only a fresh press can then bind
the latest presentation. Original graph/acknowledgement/reconnect cancellation
and retired-callback guards remain in place.

## Focused cloud checks

The UI-only correction passes 42 GTK-package tests in each default/`a11y-tests`
mode and 45 with `a11y-tests,x11-input-tests`, under Xvfb/session bus with fatal
criticals. The opt-in XTest feature adds three real synthetic keyboard regressions;
the observed-held-Right test also verifies that repeat before release/refocus
cannot resume against a new origin. Three display-free keyboard-lifetime tests
cover release routing, held/replacement ownership and excluded modifier/navigation
keys. Native GTK builds and scoped format/whitespace checks pass.

XTest uses already-installed X11/XTest libraries only in the opt-in test binary.
There is no production dependency, lockfile, version or main-startup change. To
run against a deliberately started Xvfb fixture, select `GDK_BACKEND=x11` and set
`PECAN_XTEST_FIXTURE_DISPLAY` to that fixture's `DISPLAY`, then run:

```sh
cargo test -p pecan-ui-gtk --lib --locked --features a11y-tests,x11-input-tests -- --test-threads=1
```

The earlier candidate passed 40 core, 45 backend and 10 legacy tests; those suites
were not repeated for this UI-only correction. All graphs/settings are synthetic.
No audio server, device, user profile or physical desktop was exercised. These
window-contained synthetic keyboard checks do not establish physical input,
Wayland, cross-application focus, assistive-technology or performance acceptance.

The display-settings descendant `4e3115544a62b8b3756d83f9edcb421983242b10` remains
immutable and has not received this correction. After independent clearance,
cherry-pick the correction onto a descendant and push normally. Preserve both
`PreferenceLayout` and the shared keyboard release observer if their constructor
edits overlap; do not rebase/force-push either published review candidate.

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
