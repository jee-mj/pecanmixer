# Native entrypoint adoption candidate

This separate candidate follows independently cleared source `417f49c3`.
It changes the native executable's entrypoint to the existing owner/shell,
functional mixer and patchbay, request-aware display settings and mounted meter
adapter. **This new application wiring requires independent review before PR2
publication.** No live backend, audio server, recorder, real tray or user machine
was started to validate it. The legacy React/Tauri application remains intact;
this is another functional migration slice, not completed parity or cutover.

## One owner and explicit launch modes

`startup::Options` parses paths before constructing GTK. Normal GUI activation
uses one retained `ApplicationOwner::native` / `ApplicationShell` and the native
application ID. Later activations present its existing window; they do not
recreate a worker, inbox, history controller, clock or settings worker. The
primary launch's settings/path/view choices remain in force on reactivation;
secondary launches do not reconfigure the existing process.

Display preferences default to memory only. `--settings /absolute/file.json`
explicitly starts the existing serialized off-UI worker; `--memory-defaults`
explicitly keeps memory defaults. There is no profile search, directory creation,
WebKit database access, automatic migration or implicit overwrite. The shell
retains initial-load/edit/request/disk revisions, recovery choices, save triggers
and final unsent-document drain. Saved bus/clock preferences are retained but
are not applied as audio configuration by this slice.

`--tray` is explicit opt-in. `--hidden` retains the reviewed no-host/host-loss
fallback and application hold. The shell's one clock continues receipt polling,
settings and finalization work while hidden. Close/Quit uses the retained common
shutdown handle, explicit pending/error decisions and off-UI settings join; it
does not invoke an independent delay or direct process exit. A completed failure
returns a nonzero executable status after the explicit error-exit decision.
Process signals, desktop session-manager integration and autostart remain separate.

`--fixture graph.json` selects a separate **NON_UNIQUE fixture application ID**,
so it cannot activate an already-running native owner. It rejects `--settings`
and `--tray`; the branch constructs only `ApplicationOwner::fixture`, never Graaf.
All synthetic generations are explicitly fixture-only. Fixture input captures
synthetic requests; it does not simulate or automatically acknowledge audio
effects. A fixture has no native backend/writer to join. Tests inject delayed
synthetic completion to exercise the real shell's lifetime gates.

`--headless` requires a fixture and projects JSON before GTK construction.
`--smoke` requires a fixture and exits through the controlled shell Quit path
after population, retaining the existing population-count output. `--patchbay`
selects that retained page. Fixture, headless and smoke modes start no PipeWire,
real tray or application-settings/profile I/O. The supplied fixture file itself
is read explicitly; smoke is not audio, desktop or performance acceptance.

## Single accepted graph/peak feed

`NativeWorkspace::poll` now retains the batch from its one `Session::poll` call.
It refreshes controls and supplies that same accepted batch to the reviewed meter
adapter. No second drain, graph observer, backend or telemetry clock is added.
Receipt-only/peak-only ticks do not rescan structural metadata or reset samples.

The factory reads the exact `HistoryGraph` witness already accepted by Session,
checks its stamp and owned graph Arc, and binds each node plus **all relevant
non-monitor audio ports**. `RegistryBinding` carries owner/epoch/object generations;
the retained token also contains ordered port IDs, direction, index and channel
mapping. Missing generations leave the source unbound. Numeric IDs, serials,
names or observer equality never manufacture production authority.

The adapter's existing DTO serial requirement remains conservative. Replacement,
missing witness/serial, disconnect and channel-mapping changes retire/reset old
readings. Graph precedes peaks; no-op/history progress still uses the reviewed
receipt/graph barrier. Peak DTOs still cannot prove capture freshness, generation,
complete selected channels or detailed availability. No timestamp, timeout-derived
silence or invented capture health is added.

The same bounded channel widgets mount beside the corresponding mixer strip;
mini widgets mount in existing patchbay headers without changing module/port hit
geometry. Structural rebuilds reparent retained wrappers. Tab/window mapping and
actual ancestor/viewport clipping control active surfaces/visible keys. Hidden
time uses the latest slot on resume, without an animation catch-up queue. No
physical display throughput or performance claim follows from these fixtures.

Bus definitions still use the validated, MIDI-disabled native default policy,
matching the Session model. Adopting saved/live local bus configuration requires
the separate reviewed owner/configuration adapter; meters do not apply it.

## Capabilities and remaining gates

The adopted workspace exposes reviewed gain/mute/partial mute, continuous gesture
completion/history, Undo/Redo and keyboard/pointer patchbay actions through one
Session. It does not mount bus configuration/recording controls or call their
mutation APIs. Recording, bus gain/mute/arming, dynamic arming and structural
migration remain unavailable in the native UI. The legacy application remains
available until full replacement parity and acceptance.

Native graph observation is a server-fed postcondition, not a causal transaction
acknowledgement. Admission/local execution is not completion, and cancellation
can allow one claimed operation to finish. No native audio action was submitted
to a real server by these checks. Live PipeWire/WirePlumber/restart/audio/recording,
physical KDE/GNOME/Wayland/cross-app/AT-SPI/Orca, real-time/performance, packaging,
license closure, fsync/crash durability and final cutover remain untested.

The [parity matrix](./native-parity-status.md) retains remaining route selectors,
bus/transport adapters, queued input/complete shortcuts, folder actions, persistent
module positions, autostart and other functional gates. No dependency, version,
release, merge or JavaScript-removal change is included.

## Synthetic checks

New registered-application tests inject only the synthetic backend and temporary
settings directories. They cover one factory/window/session across repeated
activation; hidden fallback; retained delayed shutdown; factory failure; request
and graph barriers; initial settings load; unsent-final-edit save and joined exit.
Workspace tests cover exact stamp/Arc/witness binding, port-generation replacement,
missing generations/serial, disconnect, projection error, channel remapping,
10,000 coalesced samples, widget retention, page/window visibility and real GTK
viewport clipping. Existing input, lifecycle and settings regressions remain.

Final cloud checks pass:

| Check | Result |
|---|---|
| GTK package, default / accessibility | PASS: 249 each |
| GTK accessibility + XTest + isolated tray D-Bus | PASS: 288; retains thirty-five XTest and four isolated D-Bus cases |
| New injected activation / workspace telemetry cases | PASS: six activation/CLI and five telemetry/viewport cases |
| Native GTK build | PASS |
| Headless connected/disconnected/reconnected | PASS: JSON matches reviewed-base results, with no display/session bus |
| Mixer/patchbay/hidden-disconnected smoke | PASS: fixture-only owner, controlled Quit |
| Invalid live/fixture CLI combinations and help | PASS: reject before resource construction; help requires no display/bus |
| Scoped formatting, whitespace, privacy/scope, native Cargo tree / ELF | PASS; no added dependency or legacy web runtime/GTK3 |
| Core/backend/legacy tests and five examples | NOT REPEATED: unchanged source; reviewed base has 67 core + 134 backend + 10 legacy and five example compilations |
| Independent entrypoint review | PENDING; no startup candidate is published to PR2 yet |

Early test-development failures were incomplete synthetic JSON fields and a
channel-remapping assertion that incorrectly expected an obsolete strip key to
remain bound. The complete fixture and current-projection assertions pass; old
wrappers remain retired. No final check failure or environment blocker remains.

All fixtures are authored synthetic/public project data. No actual device names,
identifiers/topology, audio captures, private runtime logs, user profile data or
unrelated integration context is included.
