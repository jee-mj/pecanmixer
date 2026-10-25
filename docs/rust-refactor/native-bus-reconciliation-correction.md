# Bus owner Refresh reconciliation correction

This candidate is a bounded descendant of original bus owner
`30553a53a9de37c7fd113a7294a6debf5caed0b1`. It changes only the GTK owner,
its presentation fence and synthetic tests; no backend protocol, dispatch,
settings behavior, dependency or legacy IPC is changed. Independent re-review
is required before bus integration is promoted to draft PR2.

## Reproduced defect

Refresh previously discarded a terminal configuration operation before attempting
inspection. A busy `Service::read()` then left the cached earlier configuration
with no operation/reconciliation fence. The original completion snapshot and
partial/unconfirmed evidence were lost, and controls could reenable from cache.
Native recording preflight and backend revisions still reject stale requests;
no unsafe real audio mutation was demonstrated.

Four focused synthetic cases reproduce the failure before the correction:
Applied, Unconfirmed and Partial with either stale known or unknown metadata.
All four fail on the original owner: three reenable Record all armed; the fourth
loses its original operation even though unknown metadata still disables start.
No actual device, topology, audio file, profile or user machine is involved.

## Retained barrier and fresh inspection

Refresh now requests reconciliation on the same retained operation. Busy or
older reads preserve its receipt, completed snapshot and status. Presentation
publishes the pending barrier without granting edit/start authority or submitting
an audio command. Transport sensitivity also checks the configuration coherence
fence; the backend preflight remains unchanged.

The owner captures the requested operation identity before reading. It can retire
only that still-current terminal operation, after the widget has consumed its
terminal handoff and a successful inspection matches the native owner, connected
current graph epoch and required configuration revision. The required revision
is the retained completion's revision, or the original expected revision for an
Unconfirmed report with no completion snapshot. A reentrant Refresh during
receipt polling cannot consume an inspection that preceded the request.

Ordinary graph revisions remain valid. An Unconfirmed operation can reconcile to
a freshly inspected unchanged configuration; no artificial revision increment
or inferred successful mutation is required. Fresh partial readback with unknown
logical metadata retires the operation but does not enable recording: the owner
still requires explicit, validated native-default initialization where supported.
Wrong owners, disconnected epochs and older snapshots retain the fence. Stops,
retained recording receipts and common finalization authority remain available.

The fresh read is a versioned local observation, not a graph/server transaction
acknowledgement or an atomic guarantee against subsequent changes by other clients.
All-armed membership remains preflight only, not atomic membership CAS. Dynamic
arming and idle migration are separate backend/owner slices. Active-recording
structural edits remain gated; no segmentation/disabling product choice is made.

## Synthetic evidence

The correction adds twelve cases: the four original failures plus fresh recovery,
ordinary graph revisions, older inspection, unknown-metadata initialization,
unchanged-revision reconciliation, wrong epoch/owner, reentrant request ordering
and disconnect/new-epoch recovery. All 29 focused bus-owner tests pass, including
the original seventeen receipt, stop, correlation and common-quit cases.

Final cloud checks pass: 278 GTK tests in each default/accessibility mode, 317
with all fixture features (including isolated XTest/private D-Bus), native GTK
build, changed-file formatting, whitespace and source/privacy scope checks. A
display-free connected headless result matches the committed expected JSON and
a connected fixture smoke exits through controlled Quit. The four before-fix
failures are retained as explicit regression evidence; no final failure or
environment/dependency blocker remains. Independent re-review is still pending.
Core/backend/legacy source is unchanged and those suites are not repeated for
this correction. Source and synthetic widget tests establish no live PipeWire,
recording/audio/alignment, physical desktop/input/accessibility, real-time,
performance or crash/fsync acceptance. No merge, release, version bump or legacy
removal is included.


## Independent re-review clearance

Independent re-review of `cb24a2ba5bbd03753fd635e6cae7ae8a987d58aa` passes
all nine original probes unchanged and three extra recovery/authentication probes,
along with 29 focused and 317 all-feature cases. Fresh successful readback restores
controls, while unknown metadata still requires explicit initialization before
recording. This closes the original candidate finding; combined publication and
actual audio/desktop acceptance are recorded separately.
