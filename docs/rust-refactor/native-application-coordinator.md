# Native application coordinator candidate

This candidate starts from PR2 `dec5dbfa0526f7709f6114a0865cc2a2ed43adf1`.
It includes independently reviewed keyboard patchbay `973ba290` and pointer
`b0ec1dae`; their only cherry-pick conflict was the existing shared X11 test
feature comment. Both mixer keyboard and patchbay pointer test paths remain.
The new application coordinator and its widget reconciliation require independent
review. **Main startup remains observational. This is not runtime cutover or
full functional parity.** No legacy source, dependencies, lockfile, version or
production startup is changed. Existing CLI fixture mode is also unchanged;
this candidate's functional workspace is exercised through the library fixtures.

## Ownership and integration contract

`application::ApplicationOwner` is retained once for the GApplication lifetime.
Repeated `activate()` calls return the same `Session`, backend endpoint, inbox,
history controller and result bridge. Hide/re-present should retain both this
owner and the window/workspace. Separate owner instances deliberately represent
separate applications; the shell must not create one for every activation.

`ApplicationOwner::native()` is explicit library construction. Its first
activation uses one `Graaf::start_native`, and `native_handle()` returns that
same owner for the future reviewed lifetime adapter. It is never called by
these tests or by main startup. `ApplicationOwner::fixture()` takes a separate
construction branch: no Graaf, observer, PipeWire connection or audio worker is
created. Its `FixtureDriver` supplies synthetic publications, explicit registry
generations and controlled reports; it is not a server simulator.

`Session::poll()` is the sole inbox drain and receipt poll, on the GTK main
context. Clones share all state. It performs no completion waits, joins, filesystem I/O or
nested event loops. Receipt/witness access still uses the backend's mutexes;
no latency or real-time guarantee is implied. Receipt progress must be polled even without graph wakes.
It returns the drained batch for later telemetry adapters; it accepts the graph
before any future meter delivery. Telemetry/recording is not mounted here.

Only exact owner-produced graph Arcs are accepted as action origins. The session
obtains `Graaf::history_graph` for accepted publications and feeds the exact
`HistoryGraph` into the one core history controller. Missing identity downgrades
history and pauses mutations; neither widget nor coordinator invents registry
generations or stamps a copied/observer graph as native authority. Reconnect and
replaced dependencies invalidate pending/history through the reviewed reducer.

`Session::execute/undo/redo` form one submission path. `SessionAdapter` converts
mixer volume/mute commands into domain actions. Patchbay widgets hand off actions
through that same history path. The domain ticket binds the actual NativeIntent;
there is no second independent client ticket allocator. At most one intent is
pending across mixer, patchbay and undo/redo. No-op *backend commands* still need
observed/no-op receipts plus a fresh graph revision. An Execute action that
plans no commands sends nothing and records no spurious inverse step.

Receipt ticket/source/count metadata is checked at admission. Queue/local
submission is progress only. Observed evidence reaches mixer controllers, which
retain their graph barrier; patchbay remains pending until the shared gate also
resolves. Rejection and unconfirmed/invalidated outcomes remain distinct. Old
receipt objects are no longer polled after invalidation, and UI request handles
also use pointer identity. One HistoryResults bridge is retained with this one
history controller. A replacement owner needs a replacement session/controller/
bridge together; callers cannot inject graph, result or RecordVolume inputs
through the patchbay handoff.

`NativeWorkspace` connects the existing mixer and keyboard/pointer patchbay
libraries plus Undo/Redo buttons. Undo/Redo callbacks retain the presented origin
rather than adopting a newer unseen graph. A missing witness disables the view
without inventing a pending receipt. The workspace creates no worker, timer,
tray or filesystem adapter. Its `poll()` must be driven by exactly one owning
main-context clock and refreshed while hidden/finalizing where lifetime policy
requires it. The lifetime adapter must retain native shutdown completion and
settings drain/join obligations before exit; dropping a window is not completion.

## Functional parity position

| Original functionality | This candidate | Remaining gate |
|---|---|---|
| Mixer gain and whole-node mute | Shared domain/receipt/history path and observed values | Production owner/lifetime adoption; complete gestures and partial-strip restore |
| Connect/disconnect/move, selection/delete/cancel | Reviewed keyboard/pointer library attached to the same coordinator | Independent coordinator review and production shell adoption |
| Undo/redo | One generation-aware history, real receipt and graph barriers, labels/buttons | Full gesture coalescing and production adoption |
| Repeated activation/reconnect | Shared session; identity-based invalidation and delayed result isolation | Reviewed window/quit/tray lifetime integration |
| Meters | Existing separately reviewed library; batch available for a future adapter | Availability/generation protocol and graph-before-peaks adoption |
| Recording/buses/transport | No new support | Native command contract and backend completion/recording integration |
| Settings/tray/autostart | Existing reviewed pieces unchanged; no automatic adoption | Separate settings/editor, lifetime/tray and autostart integration |

The public functional parity matrix remains in `native-parity-status.md`; this
candidate adds a reviewable owning-shell prerequisite rather than claiming all
remaining cells complete. The first runtime adoption must wait for independent
review of this coordinator and the separate lifetime/quit candidate.

## Synthetic evidence

Tests exercise one session across 1,000 activations; exact Arc forwarding; shared
mixer/patch tickets; admission versus local versus observed states; receipt-before-
graph and graph-before-receipt ordering; no-op progress; replaced numeric IDs with
identical DTOs; unaffected history retention; recreated link IDs; reconnect;
missing witnesses; copied/stale origins; malformed receipt metadata; invalid
acknowledgement recovery; and timeout/unconfirmed outcomes. GTK integration
exercises mute, patch and undo through actual widgets, stale presented Undo
origins, and unavailable-identity gating. These use existing public synthetic
fixtures with generic application labels and temporary cloud display resources.

Final cloud checks:

| Check | Result |
|---|---|
| Core/backend tests | PASS: 64 core + 67 backend |
| Legacy library tests | PASS: 10; total 141 non-GTK tests |
| GTK package default/accessibility modes | PASS: 93 each |
| GTK package accessibility + XTest | PASS: 104, including five mixer-keyboard and six patchbay-pointer XTest cases |
| New application tests | PASS: 18 synthetic tests, including four widget integration cases |
| Native GTK build / all five existing Rust examples | PASS |
| Source/privacy/scope/whitespace | PASS; reviewed patchbay source unchanged, existing stronger mixer keyboard regressions retained, synthetic/public fixture provenance only |

GTK runs use Xvfb and a session bus with fatal criticals. No final check failed
or required an unavailable dependency. These are source/fixture checks; XTest
regressions do not establish physical desktop or live audio acceptance.

The legacy executable is preserved. No production backend was activated during
these checks. A successful observation remains a server-fed postcondition, not
a causal transaction acknowledgement; batches can have partial effects and one
claimed command can finish after cancellation. Invalidating a request/history
step is not cancellation or rollback of its backend effects.

Not tested: real PipeWire/WirePlumber/hardware/audio/recording, physical desktop,
Wayland/cross-application/assistive-technology interaction, real-time/performance,
crash/power-loss durability, packaging or release acceptance. Full fader gesture
coalescing, partial-strip mute, independent output/Rec selections, recording/bus
and settings/tray/autostart parity remain unfinished. No merge, release, version
bump or JavaScript removal is included.
