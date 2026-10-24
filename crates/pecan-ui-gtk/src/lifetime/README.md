# Pure native lifetime policy

This module is an isolated application policy, not startup integration. It creates
no GTK objects, backend, tray, settings worker, directory or recording. The current
native executable remains observational. Recording must stay disabled until the
owning application implements and verifies the finalization effects below.

`Lifetime::start(hidden, Resources { backend, settings }, now)` constructs exactly
one primary owner. Resources must describe what that owner actually retains;
never omit an owned worker because its cached transport says it is idle. Handle
`HoldApplication` once and retain that hold until an `Exit` effect. Activating or
hiding the window must not create another owner, backend or writer.

## Visibility

Visible startup requests presentation immediately. Hidden startup waits at most
one second for usable tray evidence, then presents a reachable window. Explicit
activation also presents the primary, including a secondary launch that supplied
`--hidden`. A usable tray permits toggle/close-to-hide. No usable tray makes Close
request coordinated quit. During quit, Close keeps the progress/error window
reachable and cannot force exit. Actual GApplication registration, process-level
single-instance arbitration, application ID and GTK close handling remain the
application integrator's responsibility.

Loss or expiry of tray evidence restores a hidden window. Evidence lasts five
seconds and must be renewed by actual protocol checks. Recovery does not
unexpectedly hide a window already shown as fallback. Use one monotonic elapsed
clock for all policy times. `apply_tray_evidence` preserves the producer timestamp;
delivery of an old queued observation cannot renew its lease. Future timestamps
are rejected. A regular `Tick` must continue while hidden and during finalization;
this is separate from mapped/visible meter ticks.

## Quit and resource evidence

The first quit disables mutations, presents progress and issues at most one
`RequestBackendShutdown` and one `DrainSettings` for resources actually owned.
The owner must call the existing backend `request_shutdown()` and return its
`Shutdown` handle with the exact request ID. `Completion` defaults to that real
handle's nonblocking `try_complete`; no wait or join runs here. Stop admission,
command submission, transport-idle and dropping a backend are not completion.

For settings, close admission, drain/handle every accepted result and join the
settings worker **off the GTK thread**. Return `settings_drained` only after all
that work finished, including any published-write/reload diagnostics. An empty
result queue or an operation still running is not completion. A completed error
also represents a completed drain/join attempt; report a running operation as
pending instead. The policy itself performs no settings I/O or migration.

After five seconds, unresolved work remains Pending and the original completion
handle is retained. Retry resumes inspection without starting another shutdown
or settings drain. A failed request to obtain the backend handle is Blocked and
can retry that request while retaining other resource progress. Request IDs
reject late callbacks from an earlier attempt. Repeated Quit while waiting does
not spawn another attempt.

Exit zero requires successful completion of every owned resource. The backend
report represents joined graph/writer threads and explicit WAV/MIDI finalization,
not fsync or power-loss durability. Completed resource failure stays open until
an explicit `ExitAfterError` acknowledges the exact failure token, then permits
exit one. Pending work never permits that failure-exit route. A cached ordinary
Quit action cannot acknowledge a later error. Handle `Exit` by recording its code,
releasing the application hold and quitting the main loop; never call process
exit merely because a timer expired. Forced termination/session teardown is not
made durable or intercepted by this policy.

Deterministic tests use synthetic clocks and completion probes. They cover
startup, host absence/loss/lease expiry, activation, Close versus Quit, duplicate
requests, late handles, pending retries, blocked factories, independent settings
drain, failure acknowledgement and delayed/future tray evidence. They do not
establish actual GTK/GApplication, desktop tray, hardware/audio, filesystem or
performance acceptance. Core/backend and legacy quit source remain unchanged.
