# Reviewed Rec-level libraries, before owner adoption

This source integration starts at elapsed checkpoint `eee59393` and combines
backend `04d0c890` with helper/UI `676e2301` and correction `dd411c66`. The only
merge conflict retains both adjacent elapsed/gain test-module declarations.
Reviewed runtime logic, existing elapsed producer/adapter and native startup are
unchanged. At that library checkpoint no native level callback was bound and its capability
stayed pending. The later reviewed owner adoption is recorded below.

The pure setters preserve selected-bus stored dB, mute and arming, other buses,
channel layout and MIDI state. Stereo channels share gain; -90 dB silence does
not imply mute. Changing the fader while muted changes the later unmute level.
Bus levels do not enter graph undo. The retained widgets emit typed captures and
requests for a future owner, with end/abandon evidence distinct from completion.

Independent review reproduced a pointer-drag plus wheel overlap that discarded
both input finalizers while leaving the owner request pending. Correction
`dd411c66` ignores wheel while pointer capture is staged/active and explicitly
abandons admitted work if another Begin cannot obtain capture. Independent
deterministic/native regressions fail on the original and pass on correction;
eight focused cases and three additional wheel/final-value/reconciliation probes
pass. The original End/Abandon still retains owner evidence until its matched
terminal report. The correction is retained unchanged.

The initial backend level-only API validates same-layout configuration CAS and
captures the current Rec node/port registry binding at admission. It changes only local
gain, never resize/options or take/track identities. Known logical dB/mute state
and transport readback remain required. Native opening starts, arming, migration
and legacy ownership keep their existing gates. Local Applied/NoOp/partial
results are not server observation, audio delivery, rollback or graph undo.

Independent backend review passes 74 core, 209 backend and ten legacy tests plus
four probe groups for port ordering, callback-time replacement, newer same-epoch
configuration preservation and partial-stop continuity. A fifth boundary probe
confirms the advertised admission scope: an idle same-ID replacement before
admission can preserve configuration revision and accept the old configuration
intent. `IdleConfigIntent` alone carries no original widget graph witness.
This identified a concrete owner-adoption gate, not original-identity proof.
The subsequent independently cleared `bfad5259` closes that backend gap by
requiring the original retained `HistoryGraph` in `configure_bus_gains`, alongside
the configuration CAS. It checks backend authority, epoch, generations, node
serial and input mapping atomically at admission; ordinary revisions and cache
eviction do not discard unchanged identity. There is no weak overload. Original
controls must retain this witness through every sample, never recapture or fall
back to idle/legacy admission after rejection. Independent review passes 16
focused tests and five additional authority/mapping/replacement groups.
That library-only native application bound no level callback; owner wiring
was a separate independent gate, since cleared in the adoption below.

The next owner slice will use that one reviewed authority seam, shared Session
admission/tickets, one in-flight receipt plus one latest desired value, final
flush, actual readback and retained failures. It must not duplicate the backend
API, add another ticket counter or manufacture graph history. No placeholder
owner implementation or new dispatcher is added by this library checkpoint.

All new fixtures are authored synthetic public-project data. No device names,
private topology, user recordings/profile, actual launcher, hardware session or
audio acceptance is used. Active-recording structural edits remain separately
gated pending the user's product choice. Native MIDI and the legacy application
remain unchanged; there is no version, dependency, release or cutover change.

## Combined cloud checks

Fresh host and explicit-target workspace rebuilds pass 86 validator controls,
76 core, 215 backend and 421 all-feature GTK tests, native build/formatting,
Cargo/ELF dependency checks, three headless fixtures and three GTK smokes.
Default GTK passes 376 tests. All ten retained legacy library tests and
compilation of five legacy examples pass. The Node-tool attempt log is empty.
Source/privacy, whitespace and unchanged versions/manifests/lockfile checks pass.
No final local failed check or environment blocker remains. The exact PR-head
hosted outcome is recorded separately in the PR description. No physical
desktop, hardware/audio, real-time or final package/license acceptance follows.

## Original-origin and fixture follow-up

The follow-up starts at library checkpoint `37a0b607`, retaining elapsed source
and the reviewed UI correction exactly. It adopts independently cleared
`bfad5259` with no runtime reconciliation, then fixture-only `953c4078`.
The original CI failed at an arming fixture's immediate report unwrap during
writer finalization, after 214/215 backend tests; its branch run passed. The same
failure reproduced locally on repetition 17. The corrected assertion waits only
for the first readable report, with the existing two-second bound and no pump or
success predicate, then preserves the stop assertion. Independent review passes
215 backend tests, 50 repetitions and failure-preservation coverage; an
omitted-stop negative control still fails. The author additionally passed 100
repetitions, and exact candidate run 40158281137 succeeds. Production lifecycle
ordering, API and other assertions are unchanged by this fixture correction.

Fresh combined checks pass 86 validator controls, 76 core, 219 backend and 421
feature GTK tests, 376 default GTK tests, native build/formatting/dependency
audits, three headless fixtures and three smokes. All ten legacy library tests
and five example compilation checks pass. The Node-tool attempt log is empty;
source/privacy, whitespace and unchanged versions/lockfile checks pass. Exact
published-head hosted results remain separately recorded in the PR description.
At that follow-up checkpoint the authority API was integrated without a native
level callback. Shared one-flight owner adoption remained a separate candidate.

## Gain-fixture report readability follow-up

A later owner-candidate aggregate reproduced a separate immediate report unwrap
at `recording_gain_tests.rs:335` after 218/219 backend cases. The runtime-identical
published checkpoint and other runs passed. Independently reviewed fixture-only
`87613e3d` replaces that unwrap with the existing two-second first-readable
helper and an always-true predicate. It neither pumps lifecycle work nor filters
for a successful phase. The original StartedLocally, track/take/directory
identity, ordered WAV samples and finalization assertions remain unchanged.
Independent review passes 219 backend tests and 50 exact fixture repetitions;
unexpected-stop and ingress-failure controls both reach and fail the unchanged
StartedLocally assertion. The integration preserves the reviewed fixture bytes
and changes no production behavior, owner callback, dependency or version.

The combined native aggregate passes 86 validator controls, 76 core, 219 backend
and 421 all-feature GTK tests, formatting/build, Cargo/ELF checks, three headless
fixtures and three GTK smokes. Its Node-tool attempt inventory is empty.
Previously passed 376 default GTK, ten legacy library and five example
compilation checks belong to the unchanged runtime checkpoint; they were not
rerun for this fixture-only delta. The separate Rec owner was outside that
fixture integration and retained its own independent gate, since cleared for
the adoption below. Exact published
head and hosted results are recorded in the PR description.

## Reviewed Rec owner adoption

The subsequent integration applies owner `11b3046` and correction `594e46eb` on
top of fixture checkpoint `4c25232`, preserving `87613e3d` exactly. Runtime UI
source matches the independently cleared corrected owner without reconciliation.
Native gain/mute callbacks now use the original Session witness, one shared
lease/ticket allocator, one in-flight receipt, latest desired target, final flush
and actual local readback. Stored muted dB, arming and unselected channels remain
preserved; these Rec edits add no graph Undo. Known native recording gains do
not resize, replace options or restart takes. Opening, unknown ownership and
finalizing fresh input remain gated.

Independent correction review passes 21 focused owner cases and six recovery
probes; exact candidate hosted run 40160930801 succeeds. Literal old probes
expected the operation to remain present after a successful Refresh; that
expectation is obsolete. Phase-specific checks confirm uncertainty before
Refresh and archived original evidence afterward, with no replay, correct
replacement/reentry handling and fresh-input recovery. No claim that every
literal old assertion passes is made.

The [owner contract and original/corrected evidence](./native-rec-level-owner-candidate.md)
distinguish local completion from server/audio proof and retain partial/failure
outcomes. Combined integration checks and exact published-head hosted results
are recorded separately. Active-recording structural policy, saved audio/clock
effects, MIDI, hardware acceptance and cutover remain separate.

Fresh combined adoption checks pass 86 validator controls, 76 core, 219 backend,
442 feature GTK and 397 default GTK tests, native build/formatting, dependency
audits, three headless fixtures, three GTK smokes, ten legacy library tests and
compilation of five legacy examples. The Node-tool attempt inventory is empty;
source/privacy, whitespace and unchanged versions/lockfile checks pass. No final
local failed check remains. Hosted exact-head evidence is recorded separately.
