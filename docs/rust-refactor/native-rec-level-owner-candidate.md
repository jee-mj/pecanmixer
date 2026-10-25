# Reviewed Rec level owner

This candidate starts at `990a1844`, after the reviewed original-witness gain
API and arming fixture correction. It binds the existing Rec row callbacks to
`BusOwner`, using the same application `Session`, backend, polling loop and
workspace clock. Its original source was a separate review candidate; inclusion of the earlier
helper/backend libraries did not enable these callbacks. Independently cleared
`594e46eb` is now integrated on top of fixture checkpoint `4c25232`. Runtime
source matches the reviewed corrected owner exactly; the later gain-report
fixture correction is preserved. Historical check receipts below retain their
original candidate scope.

## Authority and admission

The bus control's original context now retains exactly the Session's
`HistoryGraph` paired with its graph Arc. Each interaction retains that witness
and the Rec node/input generation binding through every sample. Ordinary graph
observations can update presentation without replacing the witness. Replacement,
reconnect, missing authority or changed logical configuration ends the original
input without admitting a write to a replacement. There is no legacy or weaker
idle API fallback.

The native adapter calls only
`configure_bus_gains(&original_witness, IdleConfigIntent)`. That reviewed backend
performs its own atomic identity/configuration check; the owner checks are not a
replacement for it. A foreign owner capture, mismatched graph/witness or changed
captured bus/layout is rejected before handoff. There is one shared Session lease
for the complete interaction. Each sample uses the existing history ticket
allocator; the added crate-private sample method requires the same live lease
and open admission. Rec levels never create graph Undo entries.

## Continuous input and completion

An interaction retains one desired dB/mute value and at most one admitted config
receipt. Motion while a receipt is pending replaces the desired value rather
than creating a backlog. Once the actual local completion has a later matching
inspection, the latest unsent value can be submitted using the original witness.
End/Abandon ends input but retains the final desired value and existing receipt;
it is neither native completion nor rollback. A no-motion interaction submits
nothing. Known unadmitted Busy/QueueFull responses retry from the existing owner
clock, reusing that sample's ticket; input storms do not repeatedly retry.

The owner publishes the same interaction request as pending through intermediate
sample reports. Even same-revision NoOp/rejection needs an inspection after the
report. A genuinely older readback waits; a newer incompatible configuration or
new epoch becomes uncertainty instead of waiting forever for superseded state.
The shared lease is released only after the matching terminal publication is
accepted by the control. Raw native evidence and an already established
identity-loss diagnostic survive a late receipt; a partial prefix is not success
or failed-prefix Undo.

The reviewed fader policy remains the input oracle: relative drag, fine motion,
reset and the existing 600 ms wheel clock. Wheel steps read the latest observed
dB, so events arriving before readback can propose the same target. No separate
prediction or stopwatch is introduced. The rendered input remains retained
through ordinary publications. Existing pointer/wheel overlap and held-activation
cancellation rules remain in place.

## Visible behavior and limits

| Capability | Candidate behavior |
| --- | --- |
| Idle Rec gain/mute | Existing Set gain/Mute actions and relative fader submit through original authority |
| Known native recording | Same gain-only endpoint, no resize/options/restart or arming mutation |
| Opening, unknown ownership, finalizing fresh input | Disabled; an already owned interaction retains its receipt through busy status |
| Stored dB while muted | Changes future unmute level; mute/arming and unselected channel values are preserved |
| -90 dB | Silence independently of the mute flag |
| Partial/unconfirmed | Retained diagnostics and explicit Refresh reconciliation; unknown logical metadata additionally requires explicit initialization |
| Quit/Stop all | Common authority remains available; closing admission prevents further sample dispatch and retains admitted receipts for finalization audit |
| Undo | No graph Undo for local Rec level changes, matching the legacy scope |

Callback/readback polling occurs outside mutable UI-state borrows. Reentrant
returns are checked against the original request, ticket and receipt before
application. Closing admission during preflight prevents handoff. A receipt
already admitted before shutdown can finish locally; that does not imply the
unsent final target was written. Partial and unconfirmed native results retain
the common shutdown error path. Recovery never upgrades the historical report,
replays a failed prefix or substitutes a new witness into an old interaction.

Fresh recording gains require known native attachment evidence. The UI keeps
structural changes during recording gated pending the separate product decision;
MIDI, clock and transport configuration are unchanged. This slice adds no backend
sequencer, dependency, runtime, profile discovery, automatic migration, version
bump or JavaScript removal.

## Synthetic evidence

The new owner cases exercise real GTK Set gain/Mute actions and the scroll
controller, plus injected service/receipt boundaries. Their Rec graph is an
authored fixture with the public canonical owned-node name and explicit synthetic
registry generations. No actual device names, topology, audio capture, launch,
user files or hardware sessions are used. Native XTest coverage inherited from
the reviewed controls remains part of the combined validation; a signal/controller
fixture alone is not physical pointer/Wayland/assistive-technology acceptance.

The targeted cases cover a 1,000-update storm, one-flight final flush, shared
tickets and no Undo, ordinary revision versus same-ID replacement, foreign/stale
capture, Busy/QueueFull retries, older versus superseding readback, same-revision
NoOp/rejection, partial metadata initialization, reentrant read/report callbacks,
retired button callbacks, unselected mono values, active recording, wheel idle
completion, applied-prefix rejection, and closing admission while a real receipt
is retained. Final combined counts are recorded after the checks complete.

Local Applied/NoOp readback is not server/audio delivery, sample alignment or
physical desktop/performance acceptance. Backend origin review does not itself
clear the new owner logic: independent review of this candidate is required
before publishing it to the draft PR2 branch.

## Original candidate check results

The original `11b3046` source passes 394 default GTK tests and 439 feature GTK tests
(`a11y-tests,x11-input-tests,tray-bus-tests`), including all 18 new owner cases.
The synchronous reentrant rejection regression fails before preserving the
already established diagnostic and passes afterward. Native build, formatting,
three headless fixtures, three GTK smokes and ELF closure checks pass. All ten
legacy library tests and compilation of its five examples pass. Source/privacy,
whitespace and unchanged manifests/versions/lockfile checks pass. No Node-tool
attempt was recorded in the validation inventories.

The original candidate's final aggregate passed 86 validator controls and 76
core tests, but failed at an unchanged backend WAV fixture after 218/219 cases:
`active_gain_mute_and_muted_db_edits_change_wav_samples_without_restarting`
unwraps an unavailable `recording.try_report()` at `recording_gain_tests.rs:335`.
That owner branch's backend/core source matched `990a1844`. An earlier combined run passed
all 219 backend cases and all then-current 438 GTK cases before the final narrow
owner diagnostic regression was added; that is not a green final aggregate.
The final GTK/build/fixture checks above were run independently after the
aggregate stopped. The backend fixture failure was retained for its separate correction rather
than hidden, weakened or repaired in the UI candidate.

An initial aggregate invocation also stopped at tool discovery because its
wrapper omitted the existing `xvfb-run` directory from PATH. Correcting the
invocation used the already available tool; no installation was needed. No
environment blocker remained. Independent owner review and the backend fixture
were separate gates at that checkpoint; both have since cleared below. The published PR2 base has its own
successful exact-head hosted run; candidate hosted results are separate.

## Superseded unsent-target correction

Independent review found two recovery deadlocks in `11b3046`: after a -6 dB
sample completed with a final -18 dB target still unsent, a newer known config
or invalidated logical metadata could leave Applying and its shared lease held
indefinitely. The existing next-sample guard correctly refused the mismatched
readback, but the final-target guard returned before classifying that superseding
state as uncertainty. Repeated Refresh could not make the old target valid.

The runtime correction only moves the existing superseded-readback classification
before the unsent-target guard. A newer incompatible state becomes current
uncertainty; the original native report remains historical and unchanged, the
final target is not replayed and no prefix Undo is manufactured. Its accepted
terminal publication releases the lease. Explicit Refresh uses fresh inspection;
unknown metadata still requires explicit initialization. A genuinely older
readback continues to wait, then admits the final target with the original
witness once its actual matching inspection arrives.

Both new known/unknown-state regressions fail on the original source and pass
on the correction. All 21 focused owner cases pass in default mode, including
the unchanged original 18 and the older-readback final-flush control. All 442
feature GTK tests and the rebuilt native executable pass; formatting and scope
checks pass. The full default suite, backend, legacy and CLI checks were not
repeated for this six-line ordering delta; their original receipts remain above.
The backend readability fixture has a separately reviewed follow-up outside this
owner branch, to preserve during eventual integration. That correction was held for independent re-review rather than included in the
fixture checkpoint. The independent gate is now cleared, as recorded below.

## Independent clearance and reviewed integration

Corrected owner `594e46eb` independently clears 21 focused cases and six recovery
probes; [exact candidate run 40160930801](https://github.com/jee-mj/pecanmixer/actions/runs/40160930801)
succeeds. Review confirms uncertainty before Refresh, archived original outcome
afterward, known/unknown-metadata recovery, replacement, reentry and fresh input.
Literal old probes retained an obsolete post-Refresh operation-exists expectation;
phase-specific checks replace that expectation. This is not an all-literal-old-
assertions-pass claim.

Integration applies the two cleared owner commits onto `4c25232` without runtime
conflicts, retaining the separately reviewed gain-report readability fixture.
Native Rec gain/mute is now wired for idle and known native recording through the
reviewed authority/admission path. Independent review is source/synthetic
clearance; physical input, audio correctness, sample alignment and performance
acceptance remain unperformed. Combined integration check receipts follow.

Fresh combined integration checks pass 86 validator controls, 76 core, 219
backend, 442 feature GTK and 397 default GTK tests, including all 21 owner cases.
Native build/formatting, Cargo/ELF audits, three headless fixtures and three GTK
smokes pass. All ten retained legacy library tests and compilation of all five
legacy examples pass. Workspace packages were rebuilt for host and explicit
target to avoid cross-candidate reuse. The Node-tool attempt inventory is empty.
Privacy/scope, whitespace and unchanged versions/manifests/lockfile checks pass.
No final failed check or dependency/environment blocker remains. These are
synthetic/source checks; exact published-head CI is recorded in the PR body.
