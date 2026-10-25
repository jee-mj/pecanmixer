# Finalization-aware native shell candidate

This is a separate, unreviewed application wiring increment on top of the owner/
coordinator candidate `d5e3855d5d1599ddc5ac696a4d698021d26697aa`. The exact reviewed
lifetime/tray sources `d3ad825` / `17ca0dcb` are incorporated first. Cherry-pick
reconciliation only preserves module declarations and both X11/tray-bus test
features. New shell, admission and drain logic needs independent review.

**Main startup and its existing CLI fixture path remain unchanged. Real native
startup and recording are not enabled.** These APIs are exercised through
synthetic registered GTK applications, not a user's desktop or audio server.
The full functional port remains unfinished.

## Owning application boundary

`application::ApplicationShell::install` requires a registered primary
`gtk::Application`, one retained `Rc<ApplicationOwner>`, explicit `ShellOptions`
and an optional already-created `SettingsWorker`. It returns one retained window,
workspace, application hold and 25 ms main-context source. Reinstalling the same
owner presents the same window; another owner cannot replace that application's
primary. An exited owner cannot restart its closed session. Validation errors
return the supplied owner and settings worker in `InstallFailure`, so rejected
installation does not detach accepted settings work.

The shell retains itself and the GApplication hold until the lifetime policy's
Exit effect. Hiding never destroys its window/backend. Activation, close and quit
callbacks are attached once. A hidden startup without usable host evidence
presents a window after the policy grace period; host loss or lease expiry also
restores it. Recovery does not unexpectedly hide the fallback window. Close hides
only with usable tray evidence; otherwise it requests coordinated finalization.
Close during finalization keeps the progress window reachable.

One `ActionMailbox` clock supplies production times for tray evidence, shell
actions and regular lifetime ticks. Its reserved Quit survives coalesced
visibility traffic. This clock continues while hidden or finalizing, and it also
drains the session inbox/polls receipts independently of mapped meter ticks.
There is no separate timer per activation or view. The same PatchbayOverview is
retained across workspace switches; no spatial-layout candidate is incorporated.

Fixture owners follow their separate endpoint construction path. Even when
`ShellOptions.tray` is true, a fixture shell starts neither Graaf/PipeWire nor a
real tray adapter. Synthetic completion/factory-failure controls are explicit
FixtureDriver methods. Tests alone use a virtual offset on the mailbox clock;
production uses its unmodified monotonic time. Native constructors remain explicit
library APIs and are never called by the fixture checks or main startup.

## Finalization and failure behavior

The first Quit irreversibly calls `Session::close_admission` before requesting
backend shutdown. Mixer, patchbay, undo/redo and retained session clones all share
that gate. Previously admitted receipts can still be observed; closing admission
is not cancellation, rollback or completion.

The shell delivers the exact backend owner's real `request_shutdown` handle to
Lifetime with the supplied request ID. It retains that completion in the policy,
never substitutes queue admission or transport-idle. Waiting, timeout/Pending,
blocked request creation and completed failure have separate UI states. Retry
inspects the retained work, and only a failed attempt to obtain a handle can issue
a new handle request. Separate error-exit buttons capture their failure token;
ordinary Quit never silently acknowledges a later failure. Effects after a nested
Exit are ignored, preventing a coalesced Show from reopening a destroyed window.

Exit records `exit_code()`, removes the single source, destroys the window,
releases the hold and calls GApplication quit. A future main entry point must
return that recorded code after its application loop. No `process::exit` call or
fixed finalization delay is added. The main executable has not adopted this path.

Settings are an explicit optional resource; this shell chooses no profile path,
starts no settings worker and applies no settings/audio preferences. The caller
remains responsible for initial-load-before-edit and request-aware preference
state; `submit_settings` is only an explicit operation handoff. It rejects new
operations after finalization begins. Accepted results are counted and bounded
error diagnostics retained, including published-write/reload information carried
by storage errors; these results never overwrite an editor's in-memory values.

On DrainSettings the shell transfers the sole worker to an off-UI drain helper.
That helper closes admission, consumes all bounded results, then joins the actual
settings worker including teardown before reporting completion with the original
request ID. It reports joined errors rather than assuming success from an empty
queue. The helper itself is not synchronously joined on GTK. Failed helper spawn
retains the worker and join handle in an owned payload for a later attempt. An
unexpectedly closed completion channel stays pending with an explicit diagnostic;
it cannot authorize error exit or invent joined completion. No write is retried
automatically, and no corrupt/future input is overwritten by finalization.

The tray adapter remains a presentation service. Updates retain at most one
latest snapshot for busy admission retry. Exit requests tray stop without joining
it on GTK and without making tray teardown an audio/settings completion gate.
Real recording remains unsupported by this native command/UI path.

## Synthetic checks and remaining gates

New shell tests cover repeated activation/install; second-owner rejection; actual
hidden main-context ticks; no-host/host-loss/expiry reopening; hide versus close/
quit; exact admission shutdown for retained clients; delayed completion; repeated
quit and timeout retry; blocked factory recovery; explicit failure exit; reserved
quit plus queued Show; off-UI settings drain with bounded result saturation;
helper-spawn failure; joined storage failure; retained resources after rejected
installation; and prevention of exited-owner resurrection. Existing coordinator,
keyboard/pointer, meter, persistence, lifetime and isolated D-Bus tests remain.

Final cloud checks pass:

| Check | Result |
|---|---|
| Core/backend tests | 64 core + 67 backend |
| Legacy library tests | 10; total 141 non-GTK tests |
| GTK package default / accessibility modes | 131 each |
| GTK accessibility + XTest + isolated tray D-Bus | 146; includes eleven XTest and four isolated D-Bus cases |
| New shell cases | 16; includes accepted settings result saturation, failed helper start/retry and actual hidden main-context clock |
| Native build / five existing Rust examples | PASS |
| Reviewed-source/privacy/scope/whitespace audit | PASS; reviewed lifetime/tray implementation unchanged; core/backend/legacy/main/patchbay/meter sources unchanged |

GTK checks run on synthetic Xvfb/private session buses with fatal criticals. All
final checks pass; there is no remaining dependency/build blocker. Temporary
settings fixtures use explicit synthetic directories, not user profiles. These
checks establish source/fixture behavior rather than physical input or audio
acceptance.

No physical desktop, secondary-process/single-instance arbitration, real
KDE/GNOME tray/Wayland/assistive-technology workflow, audio hardware, live
recording, real-time/performance, crash/power-loss durability or packaging
acceptance is established. Unexpected forced process termination is outside this
policy. Native session/settings editors, complete gestures and channel-pair mute,
independent output/Rec selection, telemetry adoption, buses/recording/transport,
autostart and migration cutover remain unfinished. No merge, release, version
bump or JavaScript deletion is included.

The [held history activation correction](./native-held-history-activation.md) addresses the subsequently reproduced Undo/Redo retargeting defect. The finalization implementation is unchanged; the complete corrected owner candidate requires independent re-review.
