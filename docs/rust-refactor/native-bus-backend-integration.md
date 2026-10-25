# Corrected bus, backend and routing source integration

This follow-up starts at reviewed CI/startup head
`0b2b72b69c4e29dd4758a24d1b2d97f9c3485e1d`. It combines independently cleared
source without adding the separate production routing/arming/migration bridge.
Full functional Rust/GTK parity remains the goal; legacy React/Tauri is retained.

This is the earlier bus/backend/library batch record. The subsequent
[reviewed routing-owner follow-up](./native-routing-owner-candidate.md) now
mounts ordinary routing, shared selections, custom-patch navigation and bounded
reset confirmation through the same Session, retaining actual reports and
uncertainty barriers. It also adopts authenticated bus definitions in Session.
The historical batch checks and then-unmounted routing status below are preserved
as provenance. Arming/migration adapters, configuration-driven meter/history
reconciliation and resets above 32 commands remain unfinished.

## Reviewed dependencies and reconciliation

| Slice | Original source and correction | Independent gate |
|---|---|---|
| Native bus/configuration/recording owner | `30553a53a9de37c7fd113a7294a6debf5caed0b1`, then `cb24a2ba5bbd03753fd635e6cae7ae8a987d58aa` | Corrected owner clears all nine original probes unchanged, three additional probes, 29 focused and 317 all-feature tests |
| Dynamic arming backend | `8a33878ed228712673d72f4399c6e12b7c49d19d`, then `4524167c930f0bc42af8703a25f862fd89b1657f` | Cleared attachment/readback correction and retained child-receipt contract |
| Staged idle migration backend | `0c2bed3241cdc3d4c418e9e70b8954cb66564e6a`, then `8edafb21490d2d93a5e854f76416d664e9899b23` | Cleared final node/port-generation revalidation and observed graph barriers |
| Isolated routing controller/widgets | `cdc418fb902f8854fcb5f94ef74787cc0c362f36`, then `8240f71be1fff89f0e2866cf7d6e4f92b69af6ae` | 27 targeted cases and seven independent probes pass, including former uncertainty/rejection failures |

The original migration branch predates the arming correction. Integration takes
both corrections; its arming file retains the two reviewed migration additions
plus all final attachment/repair checks. Other backend files match the corrected
migration tree, with the corrective arming test file retained separately. The
stopped-writer deterministic fixture `3e486842` is already present through the CI
cherry-pick `e7c05a0`; the identical patch is not applied a second time.

The only textual source conflict is adjacent independent `BusControls` methods:
`recording_owner_attached` and `configuration_pending`. Both reviewed bodies are
retained unchanged. Removing the routing-only additions reconstructs the exact
corrected bus-view file. Routing module/fixtures match `8240f71b` byte-for-byte;
application bus-owner code matches `cb24a2ba`. No new shared-store behavior,
backend dispatch protocol, dependency, version or production routing caller is
introduced by this reconciliation.

## Newly exposed native behavior

The executable's Buses and recording page shares the existing Session, native
owner/inbox, ticket sequence, clock and common shutdown authority. It provides
supported idle bus actions/readback and independent Output/Rec choices. With an
explicit `--recording-root /absolute/directory`, known configuration and valid
native authority, it exposes all-armed/per-bus start and stable per-bus/per-take/
all stops. No directory discovery or automatic recording is added. Fixture mode
rejects real recording roots and starts no PipeWire, real tray or settings worker.

Configuration results and recording receipts are retained through terminal
states, including omitted snapshots, disconnect and delayed reports. Busy Refresh
retains the original configuration barrier until successful authenticated
inspection; unknown logical metadata still requires explicit initialization.
Unrelated mixer/history/patchbay operations share admission ownership, while an
active take alone does not disable ordinary gain/patch controls. Common Quit
retains actual finalization/join and explicit failure obligations.

**All-armed membership is preflight only, not atomic membership CAS.** The
baseline all-armed API has no membership-CAS argument. Per-bus start retains its
exact configuration/membership revisions. Queue admission, local attachment,
track finalization, function-finished and actual join remain distinct evidence;
these synthetic checks do not establish real recorded audio correctness.

## Integrated library capabilities and unfinished wiring

Corrected dynamic arming and staged idle split/join/remove migration are backend
libraries. Their native owner/UI adapters are not enabled by this batch. The
routing surface is also a reviewed isolated library: route classification,
selection, custom-patch navigation and reset confirmation/report handling are not
yet mounted in production. It supports at most 32 planned commands and rejects
larger resets without truncation.

The next bounded owner bridge must use BusOwner's sole configuration/selection
model, adopt authenticated bus definitions in Session, retain actual NativeReport
through request invalidation/terminal delivery, and forward reports via the same
Session/history path even while hidden/disconnected. Captures refresh on accepted
publications/choices, not each clock tick. A terminal report with nonzero
in-progress calls does not prove quiescence; recovery without such evidence stays
gated. This source integration does not invent that evidence.

Active-recording structural edits remain disabled pending the user's product
choice; no segmentation/disabling decision is assumed. Editable Rec gains/mute,
main mixer bus/configuration adoption, saved audio settings, complete transport/
folder actions, larger resets, shortcuts/session integration and distribution
remain functional gates. Module positions preserve the original session-only
behavior; durable position persistence is not an original-JS requirement.

## Combined cloud verification

The locked native aggregate validator exits zero with 86 controls, 67 core,
192 backend and 344 all-feature GTK tests, native formatting/build, Cargo/ELF
closure checks, three headless fixture projections and three controlled smokes.
All original Refresh regressions, routing uncertainty probes in the committed
suite, arming writer-failure cases and migration binding barriers coexist.
The validator records zero Node-tool attempts. It consumes committed oracle JSON;
the optional routing Node generator is not executed.

Additional checks pass: ten legacy tests, all five existing Rust example
compilations and 303 default-mode GTK tests. Accessibility-only mode was not
separately repeated; all-feature execution includes its assertions. No final
regression failure or environment/dependency blocker remains. New fixture names, IDs, graphs and status reports are synthetic
public-project data, not user hardware/runtime evidence. Source hashes, whitespace,
version/lockfile and dependency scope are checked before push.

Not run: live PipeWire/WirePlumber/hardware/audio/restart/recording acceptance,
physical desktop/Wayland/cross-app/AT-SPI/Orca workflows, real-time/performance,
crash/power-loss, installed payload or final license closure. Graph observations
are server-fed postconditions, not causal transaction acknowledgements. Ordered
operations may partially apply and cancellation may allow one claimed operation
to finish. No merge, release, version bump, installation or legacy JS removal.
