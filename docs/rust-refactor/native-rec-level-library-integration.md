# Reviewed Rec-level libraries, before owner adoption

This source integration starts at elapsed checkpoint `eee59393` and combines
backend `04d0c890` with helper/UI `676e2301` and correction `dd411c66`. The only
merge conflict retains both adjacent elapsed/gain test-module declarations.
Reviewed runtime logic, existing elapsed producer/adapter and native startup are
unchanged. No native level callback is bound and its capability stays pending.
The rendered Rec gain/mute widgets therefore do not enable live edits yet.

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

The backend level-only API validates same-layout configuration CAS and captures
the current Rec node/port registry binding at admission. It changes only local
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
This is a concrete owner-adoption gate, not original-identity proof. The narrowly
corrected captured-origin backend interface must independently clear before any
live owner wiring; the present native application cannot call this library path.

The next owner slice will use that one reviewed authority seam, shared Session
admission/tickets, one in-flight receipt plus one latest desired value, final
flush, actual readback and retained failures. It must not duplicate the backend
API, add another ticket counter or manufacture graph history. No placeholder
owner implementation or new dispatcher is added here while the final interface
is pending.

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
