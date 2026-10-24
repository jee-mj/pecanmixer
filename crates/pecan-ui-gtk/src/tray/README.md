# Isolated native tray adapter

`TrayAdapter` is GTK-free source with an explicit constructor. The preview never
starts it, and main/application ownership is unchanged. This candidate provides
Show, toggle and finalization-aware Quit presentation only. It installs no
autostart entry and offers no recording, folder, settings or desktop action.
Recording must remain disabled until the owning shell adopts and verifies the
[pure lifetime policy](../lifetime/README.md) and real finalization effects.

## Application handoff

Create one `ActionMailbox` and retain its bounded wake receiver. Use
`mailbox.now()` as the monotonic time for `Lifetime::start`, all incoming events
and regular `Event::Tick`. On each async wake, drain the mailbox and apply the
batch on GTK's main context. Handle resulting effects there; the adapter never
owns a GTK object. Keep ticks running while hidden and finalizing, independently
of meter/frame ticks. Actual GApplication registration, one primary owner,
window close interception, backend ownership and process exit remain unwired.

The mailbox has fixed atomic slots and a capacity-one wake token. Quit has a
reserved flag, independent of status/visibility traffic. Visibility commands
compose Show/Toggle order and toggle parity; accepted clicks do not accumulate
an unbounded queue. Availability precedes visibility when draining; Quit precedes
visibility changes, so a burst cannot hide the finalization surface. A closed
consumer is reported explicitly. Menu actions carry the action the user saw:
a cached normal Quit cannot become a later error acknowledgement. The policy
checks an explicit failure token and completed resources before permitting a
nonzero exit.

`TrayAdapter::start` acknowledges creation of the worker, not a visible icon.
Consume availability evidence to decide whether hiding is allowed. `try_update`
retains one latest `TraySnapshot`; busy/closed returns the unconsumed snapshot
for deliberate retry. Success is presentation admission, not a D-Bus/display
acknowledgement. Repeated updates coalesce while the single worker is busy.

## Protocol evidence and failure behavior

The worker uses the already-locked `ksni` StatusNotifierItem implementation.
Initial usable evidence requires successful registration and a true
`IsStatusNotifierHostRegistered` check. It performs an uncached D-Bus Get every
second, with a one-second method timeout, to detect a host disappearing while
the watcher itself remains present. Watcher loss revokes availability. ksni's
watcher-online callback precedes re-registration, so it revokes usability and
requests serialized teardown/re-registration instead of announcing success.
A fresh service generation rejects late callbacks from older services.

Evidence timestamps are captured before registration/checks, not on delayed
main-thread delivery. The policy rejects expired/future evidence and expires a
five-second lease if the adapter stalls. A hidden window then becomes reachable;
subsequent host recovery does not unexpectedly hide it again. This conservative
fallback depends on the application actually handling effects and running its
lifetime tick with the mailbox clock. A successful protocol check does not prove
that a desktop rendered the icon or made its menu usable.

Spawn, update, health checking and shutdown waits run on one dedicated worker.
The adapter retains one service, one pending snapshot and bounded wake state;
re-registration is serialized. No worker calls GTK or an audio mutation API.
Dropping/requesting stop never joins. `close` returns the thread for an optional
**off-UI** join. D-Bus operations/shutdown may still delay that worker; no bounded
join is promised. Tray acknowledgement or tray-thread termination must never be
used as recorder completion evidence or a mandatory prerequisite for safe audio
quit. Request tray stop when exiting, while the actual backend/settings resource
completion follows the lifetime contract. Unexpected worker failure revokes
availability and rejects later presentation updates.

## Checks and remaining acceptance

Display-free tests cover visibility/quit policy, retained completion and settings
drain, reserved Quit under saturated/coalesced/concurrent traffic, stale service
callbacks, cached menu actions, busy update retry, blocked spawn/update/health
calls, worker failure and immediate stop/close admission. They use only synthetic
state and injected services. They do not claim real-time bounds.

The optional `tray-bus-tests` feature must run serially inside a fresh
`dbus-run-session`, with `PECAN_TRAY_TEST_BUS` explicitly set to that session's
`DBUS_SESSION_BUS_ADDRESS`. Its synthetic watcher checks absent/rejecting hosts,
registration, watcher loss/recovery and host loss without watcher restart. The
tests reject an unmarked ambient bus and store no device data or runtime logs.
The adapter itself is not started by ordinary fixture tests or preview startup.

All versions remain 0.1.0. Direct references to ksni, zbus and async-channel reuse
existing locked packages; no existing package versions change. There is no new
Tauri/GTK3/WebKit dependency or second backend protocol. Real KDE/GNOME host and
Wayland/X11 behavior, activation tokens/focus, physical input, accessibility,
session-manager/forced termination, live recording, filesystem durability,
performance and packaging acceptance remain unperformed. Production integration
and independent review are still required; no cutover or parity completion is
claimed.
