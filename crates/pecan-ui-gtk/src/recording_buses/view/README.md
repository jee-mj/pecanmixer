# Isolated native bus/recording view

This GTK-only descendant of the pure model at `2647263` exports
`recording_buses::BusControls<O,R,T>`. No owner, Session, backend, main startup,
settings, autostart or tray code is changed. Constructing the view starts no
audio worker, recorder, writer, poll timer or simulated backend. Without an
attached callback its controls are observational. All tests use the existing
documented public synthetic domain fixtures.

## Surface

The view renders separate Output and Rec edit-target dropdowns, configured bus
rows, observed gain/mute labels, exclusive Arm only/Disarm buttons, additive
Armed checkboxes, split/join and remove controls, Add stereo bus, a disabled
recording control, and Refresh status. These explicit native arming actions are
keyboard/focusable equivalents; the legacy Ctrl/Meta-click shortcut is not
implemented by this slice. Gain/mute are read-only here.

Model availability controls sensitivity and explanations. Unimplemented idle,
routed, shrinking, active-layout and dynamic-arming paths remain explicit pending
capabilities. Declaring those capabilities available still requires the reviewed
owner executor; the view implements no migration and issues no start/stop opcode.
Recording receipts remain pending. This is a functional presentation prerequisite,
not completed bus/recording parity or runtime adoption.

Per-track labels use opaque stable identity `T: Ord + Clone`, not a bus filename
or label. Ordinary phase/time updates reuse those widgets, including when the
adapter moves an identity to its validated current bus association. Multiple
tracks may appear under one bus. Tracks without a current bus appear separately;
missing reports never turn into a claimed Joined result. Writer readiness,
recording, finalization result and joined result have distinct labels. Duplicate
track identities reject the publication and disable configuration controls.

Rows and track identities are retained independently of replaceable input
controls. Dropdown labels ellipsize while keeping full text in their model and
tooltip. The bus/track area scrolls and resizes; unavailable graph ports remain
separate from local configuration readback. Native accessible names identify
the two selectors and each bus action/status. Labels and test-backend properties
are not proof of AT-SPI/Orca usability.

## One typed callback

```rust,ignore
view.bind_intents(move |event| match event {
    Event::Configuration(intent) => {
        // Use the exact captured origin/expected configuration revision and
        // existing reviewed owner path. Retain the real report outside the UI.
        // Return that operation's unique owner-owned correlation identity R.
        todo!("return Ok(Handoff::Tracked(real_request_identity))")
    }
    Event::Selection { origin, kind, key } => {
        // Local edit-target choice only; send no audio/configuration mutation.
        // The view keeps its staged local choice after this UI acceptance.
        Ok(Handoff::UiOnly)
    }
    Event::Refresh { origin } => {
        // Read/reconcile current status. Never replay a failed configuration.
        Ok(Handoff::UiOnly)
    }
});
```

`R: Clone + Eq` must uniquely identify each actual tracked operation, including
its original source/owner where applicable. A reused ticket alone is insufficient.
`Tracked(R)` is handoff ownership, not observed configuration or recording success.
The view waits for Snapshot.operation with that exact identity; unrelated status
or elapsed-time publications cannot release the gate. Queued/Applying remain
pending through the pure model. Only later authoritative snapshots replace bus
configuration/arming. A checkbox returns to its observed value while pending.

The future adapter maps the existing idle API's true ConfigProgress and
LocalConfigOutcome and retains the captured ConfigRevision inside `O`. It must
correlate real reports, filter source order and preserve current readback; no
widget-generated ticket, graph identity or completion contract is introduced.
Active arming/migration and recording need their separate reviewed implementations.
Wrong callback reply kinds are unconfirmed integration errors, not success.

Errors returned synchronously keep current state. Rejected/partial/unconfirmed
owner results stay visible, with model restrictions. Refresh is read-only and
uses a last accepted origin when an invalid publication removed the model; it
does not authorize a mutation against that old origin. A fresh valid publication
permits deliberate fresh input. Retained old controls cannot replay on its new
origin. No automatic retry of writes or optimistic history is implemented.

Callbacks run without model/callback RefCell borrows held across application
integration. A selection stages only local UI choice and restores its exact
previous fallback state on rejection. Reentrant status/selection publication is
supported and correlated. The owner
retains all admitted work and shutdown/finalization obligations outside the view.
Replacing a widget, dropping it or losing a report is not completion/cancellation.

## Publication and input boundaries

`observe` synchronously consumes an ordered snapshot and returns ViewError.
`offer_snapshot` instead keeps one latest snapshot and schedules one weak idle
wake; it creates no polling clock or unbounded publication queue. Offered newer
state immediately retires old callbacks. A direct observe supersedes any queued
offer. `publication_error` exposes asynchronous rejection. Bus/track input budgets
are checked before duplicate-identity allocation. A malformed publication disables
configuration and exposes explicit recovery status.

Coalescing state must not lose a pending request's only terminal report. The
owner keeps that real report readable and republishes its typed progress with
current authenticated readback until `awaited_request()` clears. Never stamp its
old resulting configuration with a new origin. Recording receipts/finalization
are retained independently of these replaceable presentation snapshots.

Controls capture immutable action/key/origin and a view lifetime. Publication,
error transitions and input cancellation retire callbacks instead of rebinding
their tails. A scoped helper mechanically reuses the reviewed activation observer
from `application/history_input.rs` at `3c1f75b9`, generalized from Button to
Widget capture for checkbox/dropdown use. No owner source is edited. A shared
utility can replace the scoped copy after combined review; this is not another
owner/history implementation. It observes releases on the containing window,
blocks cancelled repeats/releases and permits fresh input after release. Focus
is restored to an equivalent enabled control only without held input. Missing
releases outside the window remain the same conservative recovery limitation.
Input ownership in this candidate is scoped to bus controls. Coordinating held
keys and focus across the full mixer/history/bus workspace requires combined
owner integration and mixed-view native tests before live adoption.

## Validation and remaining parity

Thirteen synthetic widget tests cover independent selection; observed arming;
handoff correlation and no optimistic configuration; pending/error/refresh and
fresh retry; stale callbacks/reconnect; multiple, missing and unmatched track IDs;
malformed/duplicate publications; reentrant status and drop cleanup; a 1,000-frame
coalescing storm; long labels, resize/focus and track reordering. Three optional
XTest fixtures cover held pointer split-to-join replacement, held Space repeats,
and focus/Escape/release/fresh activation. The 13 existing pure model tests remain.

Remaining work includes the live owner/idle-config adapter, actual recording
receipts and transport, bus gain/mute controls, route/active-track migration,
dynamic arming execution, source R routing integration, folder actions,
Ctrl/Meta-click gesture parity and final cutover. No live backend, audio server,
profile, hardware capture or user desktop is used by these checks. Physical
touch/Wayland/cross-application/AT workflows, recording/audio quality, performance
and full functional acceptance remain untested. No version bump, PR2 write,
merge, release or legacy deletion is included.

Cloud checks pass: 26 focused pure/widget tests in default mode; 29 with the
accessibility test backend and three XTest cases; native GTK build; formatting
and whitespace checks. The complete default GTK package also passes 85 tests.
GTK runs use a deliberately started Xvfb/private bus and
fatal criticals. Exact published accessible labels are checked through GTK's
test backend; these are fixture checks, not physical desktop or AT acceptance.
