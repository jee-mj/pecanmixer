# Saved Rec layout owner candidate

Base: reviewed PR2 `e989371ac671a25171fb087a63281a9b8a44d971`.
This candidate awaits independent owner review; it is not an additional PR or
a claim of full parity or physical audio acceptance.

## Legacy behavior and bounded adoption

`src/App.tsx` calls `stuurBussen(busConfig)` when configuration/connection
changes. `src/bussen.ts` sends total channels and the per-channel gain vector,
preserving existing indices. The existing native settings/restore owner already
provides initial-load protection, once-per-epoch admission, recorded user-work/
recording skips and explicit reload/recovery gates. Those rules are retained.

The native Service now explicitly supports the reviewed
`restore_idle_layout(SavedLayoutIntent)` primitive for equal-capacity
initialization/regrouping and nondecreasing capacity, including unknown logical
and transport metadata. It supplies the original accepted Session HistoryGraph,
complete saved proposal, shared ticket and captured configuration/membership
revisions. A native rejection never falls back to configure/legacy or recaptures
new authority. Endpoints without this capability retain their prior bounded
configuration path; unrelated configuration and user migration gates are unchanged.

Effective MIDI/clock/start-stop stay disabled. Stored preferences, inactive bus
states and display/clock/MIDI fields stay in the original document. Restore
emits no preference edit/writeback or graph Undo, starts no recording and uses
no new owner, inbox consumer, timer, settings schema or backend sequencer.

## Retained authority and completion

The existing migration Work retains the original HistoryGraph, owned node and
complete canonical input-prefix generations, original source generations and
incoming endpoint pairs. Admission requires a fresh matching configuration/
membership inspection and idle ownership. Injected admission/readers may reenter;
mutable UI state is not borrowed across those calls. A recording observation in
preflight retains the same epoch skip even if nested polling joins before return.

Actual MigrationReceipt reports are correlated to ticket, source stamp, both
revisions and exact proposal, and remain retained. Growth requires completed
Observed, zero in-progress calls, the backend's first actual `observed_inputs`
witness/stamp, unchanged original prefix/sources and the exact final canonical
map/pairs in a current authenticated frame. A coalesced or evicted observation
is not reconstructed by cache lookup. Local success cannot substitute for growth
observation. Equal-capacity AppliedLocally/NoOpLocally remain local evidence and
also check original route pairs, actual node/channel/gains and transport options.

For saved work, a fresh inspection must follow terminal delivery, including
same-revision NoOp. Busy or genuinely older inspections retain the same shared
lease. Supersession, replacement, lost connection, partial and unconfirmed
outcomes preserve uncertainty and the original report. An admission error stays
an error, without a manufactured completion report. The exact graph/configuration
and terminal presentation must be accepted by the existing bus model before
lease release. Existing stop/common Quit and bounded original-report archives
are reused. Closed successful work may retire on reconnect before a separate
next tick captures current preferences and current-epoch authority.

There is no automatic same-epoch replay, rollback or failed-prefix Undo. Explicit
Refresh remains the existing user reconciliation boundary; this slice adds no
new repair policy. Shrink, topology replay, active-recording structure and
recording/reload policy remain excluded. Local/observed evidence is not causal
server acknowledgement, convergence, sample correctness or meter freshness.

## Synthetic verification

Fifteen new cases use existing injected services, authenticated fixture frames,
GTK model validation and explicit synthetic temporary settings only. They cover
known/unknown routed initialization, odd and 64-channel growth, original exact
authority/masking, repeated ticks, pending/busy/NoOp/zero-in-flight barriers,
retained/cache-evicted suffix witnesses, original source/prefix/suffix replacement,
endpoint pairs versus link IDs, incompatible actual readback, partial/admission
errors with no fallback/replay, disconnect/supersession, successful reconnect,
recording/user/quit/shrink/missing-prefix gates, nested callbacks and rejected
terminal presentation. The real shell/settings-worker case waits initial load,
repeatedly activates, retains pending growth through Quit and asserts unchanged
settings bytes and no graph commands.

All 47 restore-filter checks pass in default and all existing test-feature modes.
One initial compilation visibility error was corrected. One early test also
invoked explicit Refresh after reconnect while expecting only automatic-tick
behavior; it now separates unchanged automatic no-replay assertions from the
existing explicit archival/reconciliation boundary, without changing that runtime
policy. An initial feature invocation used nonexistent `accessibility-tests`;
it ran no tests and was corrected to existing `a11y-tests,x11-input-tests,tray-bus-tests`.
Final aggregate check receipts are returned with the candidate handoff.

Initial `05841309` local `scripts/native/check.sh` passes 86 validator controls, 79 core,
253 backend and 573 all-feature GTK tests, formatting, native executable build,
three headless fixtures, three controlled smokes and Cargo/ELF dependency audits.
All 519 default GTK tests pass separately. Host and explicit-target workspace
packages were cleaned and rebuilt, and the tooling guard records zero Node
attempts. Focused restore checks cover 47 cases in each feature mode. No final
failed check or environment/dependency blocker remains. Legacy tests/examples
are not repeated in this slice because their source/interfaces are unchanged;
their earlier baseline receipts are separate. Hosted candidate CI and independent
review remain pending at publication; PR2 is not updated by this candidate.

Core/backend/legacy source, manifests/lockfile, dependencies, versions and settings
schema remain unchanged. Public fixtures are authored and generic. No real
PipeWire session, device data, profile, launcher, recording, physical desktop,
accessibility hardware or performance acceptance is exercised.

## Queued recording epoch-skip correction

Independent review blocks `05841309` on a scoped ownership contract gap. A
queued/opening take can reject saved-layout preflight while the filter's
`recording` flag is still false. If it joins before the next owner poll, the
old code forgets to skip automatic restoration in that epoch. Successful owner
admission followed by stop/join before a tick, including supported reader
reentry, has the same gap. Native UI reachability is unproven; these are injected
service/owner reproductions, not an observed deployed failure.

The correction latches the original captured epoch immediately after successful
recording admission, before reading its first receipt. Saved-layout preflight
also latches correctly scoped unfinished/opaque ownership, with matching
configuration owner/epoch and membership owner, rather than relying only on the
filter flag. Each injected take reader is polled once, outside mutable UI borrows.
It adds no new retry, timer, recording effects or policy; the existing epoch latch
still permits a later idle reconnect and is never restamped to a newer epoch.

Five new synthetic regressions run unchanged before/after (source checksum
verified). On `05841309`, all three queued/admission/reentry reproductions fail;
the earlier-active-poll and later-epoch controls pass. After correction, all five
pass in default and all existing test-feature modes. All 171 bus-owner checks in
each mode, native executable build, scoped formatting/whitespace and unchanged
core/backend/legacy/manifests/lockfile checks pass. No final failed check or
environment blocker remains. The complete aggregate, legacy examples and physical
acceptance are not repeated for this narrow correction; the initial receipts
above remain historical. Corrected candidate hosted CI and unchanged independent
review probes remain separate gates before PR2 integration.
