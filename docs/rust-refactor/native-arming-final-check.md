# Arming final attachment check correction

This is a separate corrective descendant of `8a33878`, with no migration or UI
changes. The public native arming, recording and legacy IPC types are unchanged.

The initial addition check preceded the final transport options/readback calls.
A writer can fail independently while those local accessors execute. The old
implementation then requested old-track stops and returned AppliedLocally even
though a required addition was already known to be unavailable.

The worker now rechecks every required addition's actual attachment/reservation
immediately after the final local accessors and before requesting any captured
removal TrackId. A known failure withholds all removals, retains the child receipt
and reports a partial outcome. Armed options are reconciled to the original set
union the additions that remain live, including when no addition survived. This
corrects the earlier all-failed case that could retain the desired options while
the old recordings continued. Reconciliation readback failure or another lost
addition during that repair invalidates logical metadata instead of publishing
stale successful-addition flags. Original recordings are not stopped by repair.

This is a local decision boundary, not an atomic transaction across writer
threads. A writer can still fail after the final check or after completion; its
retained recording receipt remains authoritative. Neither applied arming nor
writer readiness proves audio arrival, finalized bytes, joined completion, or
server acknowledgement. No waiting, filesystem work, join, new resource limit or
unbounded loop is added to production callbacks.

Six deterministic synthetic regressions cover real temporary WAV write failure
in final precheck/options/final readback, another successful addition's continuity,
failed options reconciliation, and a second addition failing during reconciliation.
The three commit-point probes all reproduced AppliedLocally before the correction;
all pass afterward and verify old-track WAV sample continuity. Fault gates are
test-only, independently bounded and do not reenter Commands.

Cloud checks: 64 core + 162 backend (six new) + ten legacy tests pass using
`cargo test -p pecan-core -p pecan-pipewire -p pecanmixer --lib --tests --locked`.
GTK build, five legacy example checks, changed-file rustfmt and whitespace checks
pass. Independent review remains required before integration or UI adoption. No
server, user hardware, real recording, desktop or real-time acceptance was run.
