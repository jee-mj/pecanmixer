# Native settings result and display bridge candidate

This candidate descends from `aae6e3c6` and remains independently unreviewed.
Its gesture prerequisite is **blocked** by independent synthetic findings in held
mute activation, continued held-key input after unconfirmed completion, and
keyboard accumulation across refreshes. This settings delta does not fix those
three findings or authorize production adoption.

The explicit `ApplicationShell` now retains loaded native settings, current local
preferences and edit/request revisions. Main startup remains observational.
No profile discovery, CLI change, automatic import/migration, telemetry mounting,
bus/clock application or recording behavior is added. The legacy app remains.

## Result and persistence contract

`PreferencesState` owns one desired document, a loaded disk snapshot and bounded
request metadata. An injected `SettingsWorker` still owns all filesystem calls.
No worker/path means memory-only display preferences; it does not create a file.
Initial load precedes editing. Valid/missing input supplies the initial document;
corrupt/future input supplies in-memory defaults while retaining original bytes.

Display edits retain the full validated document, including untouched bus/clock
preferences. They replace one unsent desired document and advance a local edit
revision. Normally only one automatic save is pending. Saved results acknowledge
only their matching edit revision/document; older results advance the disk
baseline without overwriting newer local edits. Explicit accepted operations keep
FIFO semantics. Queue-full returns ownership of the unadmitted operation; an
explicit rejected Save does not silently become a new desired document. Already
authorized unsent UI edits remain coalesced for retry.

After an error, autosave pauses. This includes an error reported after atomic
replacement: reload reads actual disk state rather than assuming rollback or
retrying an old snapshot. A dirty reload retains the local document and requires
explicit Save local edits confirmation; a clean reload adopts the disk document.
There is no automatic cross-document conflict merge or interprocess lock.

Recovery is explicit and separate from ordinary Save. The panel previews the
complete replacement document before confirmation. Any local edit or newly
loaded baseline invalidates that review. The existing storage adapter checks the
exact reviewed snapshot, preserves a byte-exact backup and rejects changed input.
No WebKit database is read and no migration is started by this bridge.

`ApplicationShell::preferences` exposes retained state; `edit_display(DisplayEdit)`
provides typed display-only editing. `submit_settings` uses the same request
ledger, retaining its existing operation/result contract. The added
`SubmissionFailure::NotReady` reports initial-load admission rejection. Graph and
backend DTOs/protocols are unchanged.

## UI and resize behavior

The panel shows/hides durable strip and row keys, resets either adjacent row pair
without changing the third, and resets output width to fit. The mixer still
retains its full graph, exact origin and completed history. Filtering never
prunes backend state or sends audio/configuration commands.

Resize capture checks public Paned child geometry and uses one window observer,
not private GTK handle widgets. An ancestor GestureDrag was denied by GTK's
internal descendant gesture in an actual Xvfb probe; window capture resolves that
conflict and observes release outside the handle. Unmapped pages do not intercept
other pages. Disabled editing emits no updates; unmapping/grab loss retires input. Cancellation
keeps the last proposed display value.

The row policy matches the existing JS formula: the adjacent pair's weight sum is
preserved, with a 110 px minimum (half the pair height when smaller). Split width
rounds to pixels and clamps to a 120 px minimum and the remaining available width
minus separator/other-side minimum. Double-click equalizes only that row pair;
split double-click restores null/fit width. Allocation clamping and applying
loaded/programmatic preferences do not generate saves. Wide native handles make
the gesture target visible; keyboard reset buttons provide explicit alternatives.

## Finalization

Quit closes external mutation admission and transfers the document/request state
with the worker to the retained off-UI drain. The helper drains results while
waiting for pending requests and admitting the latest dirty document. It does not
close the worker before that final document is acknowledged, unless an explicit
error/recovery condition prevents saving. It then drains all accepted results
and joins the actual worker, including thread-local teardown, off the GTK thread.

Spawn failure retains the entire payload for retry. Pending/timeout keeps the
window/hold alive. A final unsavable document remains retained and is reported by
`SettingsSummary::unsaved_preferences`; joined completion with that error requires
the existing explicit nonzero error-exit decision. Admission is not saved proof,
and a blocked/corrupt input is never silently overwritten to make quit succeed.

## Synthetic evidence and limits

Fixtures use fresh generic temporary directories and authored documents only.
Coverage includes initial load, delayed/stale results, 100,000 pure edits, rapid
UI edits behind a full request queue, unsent-final-document quit, actual file
replacement followed by a reported fault, explicit reload/save confirmation,
future-input review invalidation/backup and dirty-corrupt quit. Existing shutdown,
spawn-failure, hidden-window and joined-worker tests remain in the package suite.

Actual GTK/XTest resize input covers row drag, split minimum, pair/fit double-click
resets, child-content exclusion and hidden-page isolation. Pure resize tests cover
small allocations, both pair minima, pair sums and rounding. These are cloud
fixtures, not physical touchscreen, Wayland, cross-application or accessibility
acceptance. No actual device names, topology, runtime logs or user profile data
are published.

Final cloud checks: **190 default tests, 190 accessibility tests, and 221 tests
with accessibility/XTest/isolated-D-Bus fixtures pass**. This includes 27 XTest
cases and four isolated bus cases. Native GTK build, CLI headless fixture, both
mixer/patchbay smoke fixtures, scoped format/whitespace and privacy/scope checks
pass. Normal native Cargo tree/ELF contains no legacy web runtime, GTK3 or added
GUI framework. No final settings-suite failure or environment blocker remains.
The three independent gesture-prerequisite failures above remain unresolved;
passing package tests does not clear that separate gate. Core/backend,
legacy library tests and examples need not be repeated for this UI-only delta;
their source is unchanged. Physical audio/desktop/performance, crash/power-loss
acceptance, filesystem/profile startup policy, automatic migration and release
packaging remain outside this slice. Native startup, recording and bus adapter
integration remain separate gates.
