# Native routing selection and reset boundary

This controller and GTK surface delegate to the application-owned routing bridge
and shared Session history. The callback is absent by default; attaching a fixture
callback does not establish audio execution or a completed history step.

## Legacy behavior and source oracle

`src/mixer.ts` supplies route classification and planning; `src/handelingen.ts`
supplies undoable Route on/off and Back to auto actions; `src/App.tsx` supplies
the independent Output/Rec choices and confirmation count; `src/Mixer.tsx`
supplies custom/manual/other-destination presentation.

The implementation calls the existing Rust `Mixer::route`, history `Action::Route`
and `Action::BackToAuto`, and `history::plan`. It does not reproduce channel
pairing, auto/manual classification, routing policy or reset rules in widgets.
Off/on requests carry a fixed desired state captured at activation. Custom
patches open the patchbay and do not submit an audio/history action. Ordinary
and recording destinations remain independent. Other active destinations and
other Rec destinations are visible; a Rec route means a signal-routing choice,
not evidence of an armed/running recorder.

Reset counts are the core's manual links + target overrides + disconnected
streams, matching the legacy total. Confirm sends one `Input::Execute` with
`Action::BackToAuto`; cancel sends nothing. Core reset excludes Rec links and
restores appropriate default routes. Cancellation, new publications, unmap and
replacement controls invalidate the old confirmation. Each reopened confirmation
has a distinct token even when the graph is unchanged. Escape cancellation is a
native keyboard accessibility addition to the inline legacy confirmation.

`oracle.mjs` reads the original public TS modules using Node's built-in type
stripping, removes imports, and supplies in-memory API stubs. No Tauri/native API
module is evaluated, profile/localStorage is read, or command executed. It records
92 route cases, 94 selection cases and six resets from synthetic scenarios.
`oracle.json` is consumed by Cargo without Node/npm; hashes identify its original
source inputs. Optional regeneration, tested with Node 24.19.0:

```sh
node crates/pecan-ui-gtk/src/routing/oracle.mjs
```

All fixture names, IDs, routes and status reports are synthetic. No fixture came
from a user machine, audio server, capture or runtime log.

## Exact owner adapter contract

1. Keep the existing `recording_buses::Model` as the sole selection store. Use
   `Capture::from_bus_controls(exact_graph, &bus_controls, budget)` to project the
   **same instance** already owned by `BusControls`. The lower-level
   `from_bus_model` is available for an owner-managed model and headless tests.
   A capture holds selection keys only as immutable projection, never as another
   choice store. The bus view's new `inspect_model` is guarded against queued or
   invalid publications; its closure must not reenter the view.
2. The bus snapshot's opaque `O` must authenticate the exact graph **and bus
   configuration**. Retain the native owner snapshot/witness; never reconstruct
   one from IDs, an observer graph, a filtered display projection, or copied DTOs.
   Captures preserve the exact `Stamped<Arc<Momentopname>>`. Projection comparison
   detects inconsistent input; it does not create registry-generation authority.
3. On `Event::Selection`, call the same bus view's
   `select_destination(&binding, &key)`, then synchronously publish a fresh
   capture to routing. This local bridge uses the existing model binding, renders
   both choices, sends no configuration/recording event and never releases its
   awaited request. Rejected/stale choices remain unchanged. It also exposes
   `configuration_pending()`, including reentrant handoff before a request ID
   exists. Routing defers mutations during that gate.
4. On `Event::History`, validate the opaque origin, exact graph and configuration
   against the current owner. Forward `intent.history` (`Input::Execute`) through
   the **shared** Session/history ticket path. Do not dispatch a widget-built
   command list or instantiate another history controller. `OpenPatchbay` is
   navigation only. Keep graph/domain inputs complete regardless of hidden strips.
5. `RequestToken` is an opaque, presentation-only correlation, created before
   invoking the callback. Associate it with the owner's real request/receipt;
   never pass it to the backend as a native origin or ticket. An `Err` before any
   progress means no admission. Reentrant progress is safe; an error after progress
   retains the request. Destroying this view never releases backend/history ownership.
6. Deliver the real `NativeReport` separately through `report(&token, report)`.
   Graph offers are replaceable; reports must not be coalesced away. The source
   stamp, planned command count and first reported native ticket must match.
   Queued/local execution retains pending state across graph updates, selection,
   disconnect, reconnect and unrelated/mismatched reports. Admission/local
   submission is never displayed as observed completion. A large reset instead
   uses `report_sequence`: explicitly typed owner evidence retaining the real
   child reports, never a synthesized parent `NativeReport`. A child's report
   cannot satisfy the parent request's command count or complete its history.
7. An observed result needs a same-epoch observation stamp and zero in-progress
   calls. Controls still wait for an accepted displayed graph at/after that stamp.
   Rejected work with submitted/in-progress effects, Unconfirmed results, and
   partial/unconfirmed bus configuration latch a reconciliation restriction.
   Unrelated publications cannot clear it. A zero-effect rejection permits a
   fresh activation only when no reconciliation restriction was already latched;
   it cannot resolve an earlier partial/unconfirmed configuration result. Held
   input never becomes an automatic retry.
8. Only the owner may call `reconciled(capture)` after actual receipt quiescence,
   current graph/config reconciliation and shared history release. It rejects
   unresolved routing/configuration work. A graph update, cancel button or retry
   does not attest rollback. If capture authority is temporarily unavailable,
   call `unavailable(message)`; it retains display/pending handles and disables
   old callbacks. A busy read is not a disconnected/default publication.

The exact public effect remains an audio history intent, not another backend
dispatch/completion protocol. No native ticket, epoch, generation, successful
receipt, timer acknowledgement or recorder state is synthesized by the widget.

## Bounds, input and remaining parity

Default capture limits are 512 source strips, 256 destinations and 16,384 graph
objects. Graph/selection updates use one replaceable slot and one weak idle wake;
mixed direct/coalesced updates reject older or conflicting same-stamp graphs.
The surface retains at most one pending audio request and one confirmation. It
has no backend/event consumer or periodic poller. Rendering is bounded by these
inputs, but rows/buttons are rebound on accepted publications; there is no frame
time, allocation-rate, CPU or memory benchmark claim.

The two destination lists and source rows scroll independently. Labels wrap and
frame titles ellipsize to avoid forcing huge window sizes. Native focusable
buttons have source/destination/state accessible names, a status surface and a
named reset confirmation. The reviewed bus activation observer is reused with
crate-only visibility changes: window releases, focus loss and retirement prevent
held Space/Return/pointer input from activating replacement controls. Tab remains
navigation, Escape cancels a draft, and stale detached callbacks are inert.

The native intent limit remains 32 commands. The reviewed large-reset follow-up adds a
Session-owned sequential workflow for Back to auto and its captured inverse:
at most 16,384 planned actions, 512 retained real child receipts, one child in
flight and 30 seconds total. It preserves unlink/target/restore ordering, one
inverse and one final history acknowledgment. Each next child needs the original
semantic/generation guard, a current authenticated native frame and the earlier
effects still present. Original endpoint pairs resolve recreated link IDs.
Ordinary non-reset actions retain their existing single-intent limit.

Only observed real child receipts, local execution closure and the witnessed
combined captured postcondition can complete the parent. This does not establish
WirePlumber policy convergence or an atomic transaction. Failure, disconnection,
replacement, timeout or finalization stops future children; existing explicit
recovery waits for actual closure and fresh graph/config inspection. No rollback,
automatic retry or failed-prefix Undo is created. See
[the candidate contract](../../../../docs/rust-refactor/native-large-route-reset-candidate.md)
for focused synthetic evidence and independent review. After reconnect, a closed
old-epoch hold releases on the next normal clock tick after graph acceptance;
metadata initialization is not immediately eligible inside that acceptance call.
The failed parent remains retained and routing still needs explicit recovery.

Rec routing here does not start/stop recording,
arm buses, migrate layouts or change settings. Active-recording structural policy
and lifecycle effects remain separate gates. No JS deletion, merge, release,
version bump, real desktop/AT-SPI acceptance, hardware/audio test or performance
acceptance is part of this candidate.

## Synthetic checks

```sh
cargo test -p pecan-ui-gtk --locked --lib routing::tests::
GTK_A11Y=test xvfb-run -a dbus-run-session -- \
  cargo test -p pecan-ui-gtk --locked --features a11y-tests --lib routing::
```

Opt-in XTest checks also require `GDK_BACKEND=x11`, `x11-input-tests`, and
`PECAN_XTEST_FIXTURE_DISPLAY` equal to the isolated Xvfb `DISPLAY`. They refuse an
unmarked desktop. Tests cover held semantic replacements, real Return autorepeat
after rejection, fresh activation, reset Escape/Tab/pointer cancellation, native
accessible labels, reentrant pending status, shared bus-model selection, graph
barriers, partial reconciliation, mixed publication order, 1,000 coalesced offers,
missing/disconnected/unknown inputs and narrow/wide large-list allocation.
These are source/widget fixtures, not evidence of physical input/audio acceptance.

The reconciliation correction has four model/widget regressions that fail on
`cdc418fb`: partial or unconfirmed configuration arrives while routing is queued,
then a zero-effect routing rejection and ordinary publication must retain the
restriction until explicit owner reconciliation. The fresh-rejection control
continues to allow retry. Corrected candidate checks pass 274 default and 274
accessibility tests, 315 all-feature tests on isolated Xvfb/D-Bus, and native build.

## Focus retirement on GTK 4.14

The combined hosted run at `88fa38b9` exposed a presentation defect, rather than
a focus-readiness timeout that needed a longer wait. The unchanged routing XTest
reproduces with Ubuntu GTK 4.14.5 loaded in an isolated cloud directory: its second
focus checkpoint stalls after Space release following a graph replacement. The
same source passes with GTK 4.18.6. Both local runtime checks use cloud GLib 2.84.4;
they do not reproduce the hosted Ubuntu image in full.

GTK 4.14 retains the old focused button in the detached row subtree. Its root is
absent, and the window observer receives no Space release. GTK's
[key-event target selection](https://github.com/GNOME/gtk/blob/4.14.5/gtk/gtkmain.c#L1430)
uses the current focus widget, explaining why that detached target strands the
held-input ledger. The strengthened regression fails before the correction on
the detached-focus assertion as well as reproducing the original timeout.

Before unparenting controls, routing now unsets window focus only when it belongs
to one of the replaced subtrees. It retains the existing focus key for ordinary
restoration and leaves external focus alone. It does not clear the held ledger:
the real release must reach the window before a fresh activation is enabled.
No additional key event, activation retry, longer wait, domain action or dispatch
path is introduced. Existing stale-release, fixed route intent, Return autorepeat
and fresh-activation assertions remain; explicit release-ledger and external-focus
checks strengthen them. Focus timeout reports now identify their call site.

Correction checks pass 345 all-feature tests with each GTK runtime on isolated
Xvfb/private D-Bus, native build, formatting and whitespace checks. This is source
and synthetic input evidence; independent review, hosted validation and physical
desktop/accessibility acceptance remain separate. No backend/core/startup,
dependency, version or legacy-source change is included.

Cloud validation on 2026-10-24 passed 269 default GTK library tests and 310 with
all features under isolated Xvfb/D-Bus, including 20 routing controller/widget
tests and two routing XTest groups. Source-oracle regeneration, native binary
build, crate formatting, whitespace and synthetic-provenance audits passed.
No real PipeWire server, desktop session, user profile, audio device or capture
was accessed, and no production routing/recording command was executed.
