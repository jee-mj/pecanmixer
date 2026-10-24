# Native mixer controls: isolated functional prerequisite

This candidate adds a volume/whole-node mute controller and actual GTK scale and
mute callbacks. The executable remains read-only until the backend's reviewed
origin-bound intent and completion API is connected. This is a prerequisite for
the full functional JS port, not a claim of mixer or application parity.

The UI-local `controls::Adapter` retains adapter-owned snapshot and request types.
It forwards the original presented snapshot with an existing `Opdracht`. It does
not construct backend identity tokens, introduce a competing backend protocol,
or equate a successful submit with command completion. The production adapter
must use the exact reviewed backend API, rather than reconstructing the latest
origin at dispatch time. Patchbay controls should share that production adapter.

The controller allows one pending request. It displays observed values and waits
for both explicit actual completion evidence and a newer matching observation,
including for no-op changes. Rejection leaves observed values intact. Disconnect,
epoch change, target removal or serial change invalidates pending state. Bindings
from an older graph revision are conservatively rejected. GTK refreshes suppress
signals to avoid dispatch on refresh, rejection or reconnect. Callbacks hold weak
controller references, so closing the view releases it.

The first supported intents are strip gain (preserving untouched channels) and
whole-node mute. Rec-bus gain/arming, partial-strip mute gain restoration, gesture
coalescing and one-step history, routing, output/Rec selection, transport, settings
runtime effects and shutdown integration remain unfinished. Nodes without serials
are unavailable to these controls. Waiting is truthful but currently has no UI
timeout; the future production adapter must report terminal failures and progress
for no-op commands. No server acknowledgement is claimed by the candidate fake.

Validation uses only the repository's hand-authored synthetic graph and a fake
adapter. Nine controller tests cover origin forwarding, admission/completion/
observation separation, no-op progress, rejection, stale revisions, reconnects,
ID reuse, channel preservation, unsupported intents and default read-only state.
Two GTK tests exercise real scale/toggle signals, pending/failed sensitivity,
observed-value restoration, disconnect and release of the controller. There are
no audio-server, user-profile, hardware, physical desktop or performance tests.

## Parked adapter candidate

The local candidate now forwards the exact `Stamped<Arc<Momentopname>>` origin
through `NativeIntent`/`NativeReceipt` using one cloneable `NativeClient`. It does
not start a native owner or enable live controls in the executable. Mixer and
patchbay callers can share its ticket allocator and exact backend API. Native
receipts distinguish queued/local execution, observed/no-op, rejected and
unconfirmed states; the UI waits for the reported graph revision before clearing
pending state. Observed state is a postcondition witness, not a causal server
transaction acknowledgement, and subsequent external changes remain visible.

Compatibility source integration retains the reviewed recorder lifecycle and
settings storage. Core/backend output records 40 core and 45 backend tests
passing; its shell wrapper reported a nonzero status despite no test failures in
the captured output. Fourteen pure controller/adapter tests pass. An over-broad
`controls::` test filter also ran two GTK tests without a display; both failed
initializing GTK. Earlier GTK results apply to the prior isolated candidate,
not this updated adapter. A proper Xvfb run, final native build, independent
adapter review and application ownership/control integration remain unperformed.
Work was parked before those checks. No hardware or user-profile action ran.

### Unresolved live-gesture origin boundary

A subsequent independent signal probe reproduced row-binding substitution: a
refresh can replace a retained scale callback's binding with a replacement
object's newer origin during an active gesture. This is a signal-level result,
not physical-pointer acceptance. Before live gestures are enabled, capture the
origin at gesture start and retain it, or cancel the gesture when its target or
origin changes. This remains unresolved; the receipt adapter's stale-binding
checks alone do not establish gesture safety. Controls remain disabled.

The bounded native backend contract at
`e7223aa12de021952dd25392dee83b4140b653cb` subsequently received independent review
clearance. That clearance does not clear this UI gesture boundary or establish
live application parity. The separate, unwired patchbay actions candidate is
`973ba2900a7268b90f0a09643804923e5d218806`; it has not been integrated here.
No further fix, review, build or integration round was started for this handoff.
