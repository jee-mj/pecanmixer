# Recording directory, folder and elapsed presentation

This isolated application component uses existing retained `BusRecordingStatus`,
`BusTakeKey` and track phases/timing. The isolated helper review did not mount it in the native owner.
At its isolated review, BusOwner, dynamic arming, shared Session/history, startup
and backend code were unchanged. The separate owner adoption candidate is
documented in `docs/rust-refactor/native-folder-owner.md`; its new wiring and
latest-start projection require their own independent review. No recording command, receipt consumer, timer or file-manager process
is created by the controls.

## Source parity

`src/Mixer.tsx` calls `openMap(transport.map)` from Folder. The original JavaScript
has no recording-directory picker. `opname::open_map` creates the fixed recording
root, selects a canonical last-take path within it, otherwise falls back to root,
and launches `xdg-open` with one argument. This component keeps the native app's
existing explicit-root policy: it displays that root and does not discover HOME,
expand `~`, install a picker or silently choose a profile directory.

`src/Transport.tsx::Looptijd` displays the oldest running take's reported elapsed
time, interpolated between publications, in `m:ss.t` / `h:mm:ss.t` form. The pure
formatter preserves JS decimal rounding and the original rounded `60.0` boundary
without carrying into the next minute. Running duration comes only from existing
Recording tracks with supplied elapsed values. Missing or unconfirmed timing,
unknown activity and finalizing activity are unavailable. Explicit idle with no
unfinished receipts is zero. Neither receipt arrival nor writer byte counts start
an invented stopwatch; elapsed time establishes no audio arrival/alignment.

## Required owner seam

Supply `Snapshot<O>` with the owner's retained explicit root, original UI context,
actual latest-started take identity, retained statuses, authoritative Activity and
the monotonic observation time for those timing values. Never select latest by
sorting caller tickets or take IDs. Preserve the latest take's folder evidence
after stop/disconnect and recent-status eviction. Missing supplied folder evidence
falls back to the recording root; it is not relabelled as a known latest folder.
Do not restamp a cached timing report as newly measured.

At isolated helper review, BusOwner had no read-only root/latest-start projection.
The separate Folder owner candidate adds that projection and one retained actual
latest-start receipt. Native `Track.elapsed` still remains `None`; real duration
production is not part of that adoption. Activity stays Unknown when inspection
is uncertain. Active elapsed is unavailable, without an arrival stopwatch.

Use structural `observe`/`offer` when folder authority, root, latest evidence or
closing state changes. Keep its opaque origin stable independently of periodic
timing reports. Supply fresh timing via `update_elapsed(activity, takes, at)`;
the caller authenticates this input against the same retained owner. This leaves
folder bindings and pending requests intact. Do not feed a second backend
consumer or rebuild the folder controls for every timing publication.

Call `tick(now)` from the existing owning clock (approximately 100 ms matches
legacy display cadence). Tick updates only the time label, performs no status
scan/structural rebuild, and leaves hidden views untouched. Timing updates also
retain the widgets. Snapshot offers keep one bounded projection/weak idle wake,
with equal-time delivery order preserved.
There are at most 80 takes, 64 tracks per take and 4096 encoded bytes per retained
path. Receipt ownership/finalization stays with BusOwner, not this projection.

The Folder callback receives the exact captured origin/paths and a UI correlation
token. Validate origin/path authority in the owner; it is not a native audio
ticket. An Err means no launch was admitted. After admission retain the token
until the real result, including busy/error states across UI publications.
`complete(Submitted(plan))` describes launch admission, not a visible desktop
window; mismatched/late results cannot replace another pending request.
Structural rendering, unavailable state and handler rebinding revoke every old
button callback, including idle buttons retained outside the widget tree. Each
button captures its own handler. Timing-only updates preserve its valid binding;
pending requests keep their original token across handler replacement.

Only explicit `FolderPaths` go off the UI thread. `prepare()` creates that root,
canonicalizes, and falls back on missing, escaping or non-directory take paths.
It returns a plan with a fixed `xdg-open` program and one literal OS argument;
it never launches anything. The owner must perform launch/reaping and report its
actual outcome. It must revalidate a stale request rather than retarget it to a
new root/latest take. No Folder launch or profile write is exercised here.

## Checks and limits

Eight focused synthetic cases cover legacy formatting boundaries, oldest-active
versus latest-folder selection, missing timing, explicit-root canonical fallback,
symlink escape, preparation errors, pending/result correlation, stale callbacks,
reentrant delivery, coalesced publications, hidden resume and tick storms without
button rebuilds, including fresh timing updates and stale-time rejection. Both
retired-button regressions (unavailable bypass and rebound-handler adoption)
failed against `84717ec4` before the correction. Recovery and completion cannot
revive a retired callback, and fresh controls still admit the captured request.
Filesystem writes are confined to unique test directories under
`/tmp`; labels/IDs/paths are authored generic fixtures. The widget tests use
isolated Xvfb/private D-Bus and do not launch a file manager or recording worker.

Cloud checks on 2026-10-25 (Rust 1.99.1, GTK 4.18.6): focused default-feature
tests 8/8; complete all-feature GTK suite 378/378, including its existing isolated
XTest/private-bus cases; native `cargo build -p pecan-ui-gtk --locked`, package
formatting and whitespace checks passed. Core/backend/legacy sources are unchanged
and their test suites were not rerun for this UI/helper-only candidate.

Owner mounting, real timing production, folder launch/reaping and actual desktop
acceptance remain pending. No hardware, private profile/topology, recording/audio,
Wayland/screen-reader, performance or finalization acceptance is claimed.
