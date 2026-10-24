# Native bus/configuration and recording owner candidate

This separate descendant of reviewed startup `20e9f90c` uses only the already
cleared native idle configuration, initial recording/multitake and corrected bus
widget APIs. It does not include dynamic arming, idle migration, desktop launch
or validation-scaffolding follow-ups. New owner/UI/lifetime wiring requires
independent review before PR2 publication. No live backend, capture, real writer,
user profile or hardware was run for this slice.

## One owner and the existing protocols

`ApplicationShell::bus_owner()` exposes its retained `BusOwner`. The shell passes
the same native Graaf already used by Session; there is no new backend, FIFO,
inbox consumer, polling clock or recording implementation. The existing shell
clock polls the application adapter, including while hidden/finalizing. Fixture
startup has no real bus endpoint. Controlled tests inject a separate synthetic
service through a test-only hook, using the same owner/controller/view logic.

The Buses and recording page mounts the reviewed `BusControls` plus bounded
application-owned transport/status controls. The isolated widget's disabled
recording placeholder is suppressed only when an application recording owner is
attached. Default standalone widget behavior and its typed handoff stay intact.
Output and Rec selections remain independent local choices; selecting either
sends no audio command.

An opaque `BusOrigin` retains the exact graph Arc/stamp, atomic recording snapshot's
configuration and membership revisions, owner context, selected directory and
original stable stop targets. Ordinary polling does not rebind callbacks or
overwrite pending results. A changed publication retires old activation callbacks;
recording directory/configuration edits cannot be stamped with newer authority.
Cached/busy inspection preserves the last accepted state, not fabricated defaults.
Even the first busy configuration inspection preserves the connected graph and
known Output choices; local UI choice requires no invented recording authority.

Configuration and start tickets reserve from Session's existing history sequence.
An additive private lease serializes local configuration with graph/history/volume
changes. Recording uses the lease only for admission, so an active take does not
disable unrelated gain/patch actions. Local configuration results never feed
graph-history completion or masquerade as server-observed port publication.

## Local configuration

Bus actions recompute their captured proposal using existing core policy and
retain the exact `ConfigRevision`. Declared Split/Join direction must match the
original definition. Native `configure_idle` remains responsible for admission,
execution validation, gains/options/readback and typed partial results. No migration
planner or backend dispatch branch is copied into this UI adapter.

Each owner request has a unique retained correlation identity and native receipt.
Queued/Applying remain pending; terminal local results release the shared lease.
Reentrant handoff/report reads hold no owner/model borrow across native callbacks.
A delayed old request's result cannot be delivered into a newer request slot.
The real report remains owned until its matching terminal publication is consumed.

Completion snapshots do not replace newer readback or get stamped onto another
epoch. If the last readback precedes a completed revision, controls wait for fresh
authenticated inspection. Partial/unconfirmed outcomes require deliberate Refresh
and valid metadata before another configuration/start. Inspection does not repair
unknown logical metadata. Initial adoption is explicit, using MIDI-disabled native
defaults only for a locally known two-channel filter; no startup audio setting is
automatically applied. Other unknown layouts remain pending.

Unrouted idle growth and supported grouping/arming use the cleared idle API.
Channel shrink/remove, routed migration, active arming and every structural edit
while recording/finalizing remain unavailable. Their capability explanations are
preserved. No active-layout user decision is assumed.

## Recording and stops

`--recording-root /absolute/directory` explicitly supplies the destination. There
is no discovery, implicit directory choice, automatic start or directory-picker
operation. Fixture mode rejects recording roots alongside settings/tray options.
The GUI exposes all-armed and per-bus starts only after known native configuration
and an explicit directory; per-track/per-take/all stops retain actual backend IDs.

Per-bus start passes the original graph, exact configuration revision and exact
`RecordingRevision` to the cleared backend API. The adapter also compares both
current revisions before handoff, without restamping the edited intent. The
baseline all-armed API accepts configuration/origin and enforces idle-only start;
it has no membership-CAS field. Its membership preflight is not an invented atomic
backend guarantee. It still creates one immutable all-armed take, not a simulated
sequence of independently acknowledged per-bus starts.

No-op start retains the existing receipt; it proves no new writer and does not
prove capture attachment. Queued/Opening, local Started, Finalizing, writer
Finished and actual Joined remain distinct. Stable IDs may not exist before the
writer is installed: Stop all is available during that interval; a per-bus stop
must not guess a track from a filename or latest bus membership.

Once IDs are published, the original stop-target set is captured. Disconnect,
replacement/new takes and later same-key buses do not redirect it. Stops use
`RecordingStop::{Track,Take,All}` directly, not the legacy name-based bridge.
Repeated stops coalesce; stop-all bypasses graph/configuration admission gates.
The backend remains the authority for reserved-stop capacity and actual closure.
Stopping is a request, not file finalization or joined completion.

Receipts survive omission from later replaceable snapshots. Busy status retains
the original handle and state. The owner keeps at most 64 unfinished plus 16
recent completed takes, bounded opaque diagnostic aliases, and bounded completed
failure summaries. Original admitted receipts remain owned through completion;
stop-returned aliases create no new take ownership. Completed-source tombstones
prevent snapshot reimports from growing history indefinitely. No completion is
inferred from absence. Malformed receipt identity fails closed, retaining evidence
for common shutdown rather than accepting another request's status.

## Common quit and separate evidence

Quit still invokes exactly the existing backend `request_shutdown()` handle and
settings drain. The application adapter creates no writer stop/join coordinator
or independent finalization deadline. Source admission closes before finalization;
the same handles and hidden clock remain retained through timeout/retry.

The UI `Completion` trait adds a default nonblocking `additional_errors` hook.
Existing implementers preserve their behavior. The owning shell checks retained
configuration/recording diagnostics after the common backend really joins; busy
diagnostics keep the same completion pending. An already failed, actually joined
backend still permits explicit error exit when diagnostics are unavailable; no
individual take is thereby marked Joined. Recording ingress failure and writer
joined success remain separate fields/results. The hook never rewrites writer
counters, invents a worker panic or labels ingress failure as a join failure.
Known failures/unconfirmed outcomes use the existing explicit nonzero error-exit
decision. Expected cancellation before start is not presented as a shutdown I/O
failure. Individual status is never changed to Joined merely because quit joined
the owner. No fsync/crash durability is claimed.

## Remaining parity and evidence

This is not completed transport/bus parity. Editable bus gain/mute, shared main
mixer Rec-model/configuration adoption, source routing/reset, bus edit history,
saved audio configuration application, directory picking/open-folder, elapsed/
alignment evidence, dynamic arming and staged migration remain unfinished.
Session-only patch module layout is already implemented; durable positions are
not an original-JS parity requirement.

Synthetic tests cover captured versions and shared tickets, pending/reentrant/
delayed results, stale directory/membership/action handoffs, partial/unconfirmed
readback, queued starts/no-ops, omitted/busy receipts, disconnect-safe original
stop IDs, bounded histories, common quit and separate ingress/join evidence.
They use controlled reports, not fake assertions about actual captured WAVs.
The full existing native-input, settings, history, meters and bus-widget suites
remain relevant integration checks. Two old tests initially counted all workspace
dropdowns; they now scope the same port-choice/draft assertions to the patchbay,
while the new page keeps its separate Output/Rec selectors.

Final cloud checks:

| Check | Result |
|---|---|
| GTK package default/accessibility | PASS: 266 each |
| Accessibility + XTest + isolated tray D-Bus | PASS: 305; retains thirty-five XTest and four isolated D-Bus cases |
| New owner/controller/quit cases | PASS: 17; controlled synthetic backend only |
| Native GTK build | PASS |
| Connected/disconnected/reconnected headless | PASS: unchanged reviewed-base JSON, without display/session bus |
| Mixer/patchbay/hidden-disconnected smoke | PASS: fixture only, controlled Quit |
| Recording-root/fixture rejection and help | PASS: invalid modes reject before resources; no normal native launch run |
| Formatting, whitespace, scope/privacy and native Cargo-tree/ELF | PASS; no new dependency, legacy web runtime or GTK3 |
| Independent owner/lifetime integration review | PENDING; no adapter source promoted to PR2 |

No final check failure or dependency/environment blocker remains. Backend/core/
legacy source and dependencies are unchanged; their cleared baseline evidence is
67 core, 134 backend, 10 legacy tests and five example compilations.
No hardware/audio/live PipeWire/restart, physical desktop/Wayland/AT-SPI/Orca,
real-time/performance, distribution, crash/power-loss or final cutover acceptance
is claimed. All fixtures/paths are synthetic/public project data only. No actual
device names/identifiers/topology, user captures/private logs/profile content or
unrelated integration context belongs in this source. No merge, release, version
bump or legacy JS removal is included.
