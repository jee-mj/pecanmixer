# Native gesture review correction candidate

This is a narrow correction after `aae6e3c6`, pending independent re-review.
It addresses three independently reproduced input defects. It does not include
settings-bridge changes or enable main startup, recording, buses or telemetry.
The full functional JS port and its remaining parity/acceptance gates remain.

## Reproduction and correction

All six strengthened regressions fail on `aae6e3c6`: a deterministic and native
Xvfb case for each finding. Duplicate XTest key-down requests alone did not
reliably produce held-key repeats, so the native keyboard probes use the isolated
Xvfb server's actual autorepeat and restore its original rate afterward.

1. **Held mute adopted new semantics.** Holding Space on an unmuted strip,
   observing it muted, then releasing submitted an unintended unmute. Mute
   buttons now retain immutable action semantics. A semantic change retires
   only that button/callback and cancels its held input. The row, fader and
   authoritative registry binding remain intact. Partial mute also guards its
   full gain vector, preserving the captured mean/full-vector behavior.
2. **Unconfirmed key input retried without release.** Clearing the local gesture
   omitted cancellation of the window key ledger. Terminating/invalidating the
   local token now cancels its held ledger entries before clearing local input.
   Autorepeat cannot create another intent until an actual release; fresh input
   after that release remains usable.
3. **Refresh lost accumulated keyboard increments.** Repainting the thumb from
   observation before every repeat collapsed four Right events to one increment.
   One local desired keyboard position now survives ordinary refreshes and
   observations until the group completes or is invalidated. The separate label
   still displays the observed gain. Dispatch/history completion remains owned
   by the existing one-flight Session coordinator.

Mute activation uses the button's immutable desired scalar value rather than the
latest GTK active state. It uses a current exact owner graph only while the
original registry identity and semantic action remain valid. Whole-node mute
ignores unrelated volume observations; partial mute conservatively cancels when
its full vector changes. Retired callbacks cannot act on replacement controls.

The existing history input guard adds a per-control retirement helper; its
existing Undo/Redo behavior is unchanged. A replacement mute preserves focus so
that the window guard can consume the canceled release. Mutation callbacks remain
blocked during that canceled sequence. This avoids leaving no focus receiver and
stranding the next legitimate activation.

## Preserved behavior and evidence

The strengthened six cases pass after correction. Additional controls cover
pointer mute retirement/fresh pointer recovery, Return release during pending
completion, partial mute remembering/restoring the newly observed mean, and a
whole-node mute observation during an unrelated active fader. The latter keeps
that same row/gesture and records its one completed volume step.

Four actual Right events separated by refresh ticks retain four increments. The
pipeline admits only the first request while pending, flushes one latest target,
records one final observed history step, undoes the complete original unequal
vector and redoes the final vector. Existing motion-storm, graph/receipt barrier,
no-op, replacement/reconnect, focus-return, shutdown and late-result tests remain
in the package suite. No coordinator/backend/domain protocol was changed here.

Final cloud gate passes: **181 GTK tests in each default/accessibility mode;
216 with accessibility/XTest/isolated-D-Bus features**, including 31 XTest cases
and four isolated bus cases. Native GTK build, CLI headless fixture, both smoke
views, scoped formatting/whitespace/privacy/scope and native Cargo/ELF audits pass.
No final test failure or environment blocker remains. Core/backend/legacy tests
and examples were not repeated because their source is unchanged. Cloud fixtures are
synthetic and use a marked Xvfb/private session bus. No hardware/audio, physical
Wayland/X11/cross-application/AT-SPI/Orca, real-time/performance or distribution
acceptance is claimed. Passing source tests does not replace independent review.

## Settings descendant reconciliation

The settings-only delta at `d8af0567` is separate and must retain its existing
history. After this correction is independently cleared, append **only this
correction commit** onto a descendant of `d8af0567` with a normal cherry-pick;
no branch rewrite or force push is needed. Keep its `bind_display_edits` and
`set_display_editable` hooks along with this correction's mute input observer,
immutable mute controls, ledger cancellation and desired keyboard position.

The new mute module and retirement helper are additive to that settings delta.
If constructor/context reconciliation is nontrivial, publish the combined
candidate for a separate review gate. Rerun the combined settings quit/resize
cases and gesture/native-input cases before any PR2 publication. This correction
branch intentionally contains none of the settings implementation itself.
