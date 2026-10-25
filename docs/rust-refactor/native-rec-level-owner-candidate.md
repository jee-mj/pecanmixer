# Rec level owner candidate

This candidate starts at `990a1844`, after the reviewed original-witness gain
API and arming fixture correction. It binds the existing Rec row callbacks to
`BusOwner`, using the same application `Session`, backend, polling loop and
workspace clock. It is a separate candidate for independent review; inclusion
of the earlier helper/backend libraries on PR2 did not enable these callbacks.

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

## Candidate check results

The final source passes 394 default GTK tests and 439 feature GTK tests
(`a11y-tests,x11-input-tests,tray-bus-tests`), including all 18 new owner cases.
The synchronous reentrant rejection regression fails before preserving the
already established diagnostic and passes afterward. Native build, formatting,
three headless fixtures, three GTK smokes and ELF closure checks pass. All ten
legacy library tests and compilation of its five examples pass. Source/privacy,
whitespace and unchanged manifests/versions/lockfile checks pass. No Node-tool
attempt was recorded in the validation inventories.

The final aggregate check passes 86 validator controls and 76 core tests, but
fails at an unchanged backend WAV fixture after 218/219 backend cases:
`active_gain_mute_and_muted_db_edits_change_wav_samples_without_restarting`
unwraps an unavailable `recording.try_report()` at `recording_gain_tests.rs:335`.
No backend/core source differs from `990a1844`. An earlier combined run passed
all 219 backend cases and all then-current 438 GTK cases before the final narrow
owner diagnostic regression was added; that is not a green final aggregate.
The final GTK/build/fixture checks above were run independently after the
aggregate stopped. The backend fixture failure is retained for the backend
owner rather than hidden, weakened or repaired in this UI candidate.

An initial aggregate invocation also stopped at tool discovery because its
wrapper omitted the existing `xvfb-run` directory from PATH. Correcting the
invocation used the already available tool; no installation was needed. No
current environment blocker remains. Independent owner review and the backend
fixture failure remain integration gates. The published PR2 base has its own
successful exact-head hosted run; candidate hosted results are separate.
