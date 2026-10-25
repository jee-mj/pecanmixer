# Sequential Back-to-auto candidate

Based on `261fd352d1cd4055afd47a721cb4e88be456182c`. This candidate requires
independent review before PR2 adoption. It addresses resets exceeding the existing
32-command native intent limit, including their Undo/Redo, without changing that
backend limit, command DTOs, dependencies, versions, configuration or settings.

## Legacy behavior

`src/handelingen.ts:79` captures one Back-to-auto step and its inverse: remove the
auto-restored pairs, restore original manual pairs, then restore original target
values. `src/mixer.ts:342` orders the forward operation as manual unlinks, target
clears, then default-route restores. The existing Rust core remains the planner
and inverse oracle. Rec links remain outside the reset. Link replay has pair
semantics, including when the original graph has duplicate links.

## One owner operation

The existing Session retains one pending reducer step and its original identity
guard. A reset plan over 32 commands becomes a typed `SequenceReport`, distinct
from `NativeReport`; each real child receipt remains unchanged and retained.
One child runs at a time. Children reserve correlation from the existing history
ticket allocator and are never delivered to the history-result bridge. Only the
parent receives one final acknowledgment, so successful reset, Undo and Redo each
remain one history operation.

Before each next child, the existing clock validates the retained original
semantic/node/port generations against the exact current authenticated frame.
Completed prefix effects must still be present. The owner resolves original
disconnect pairs against current matching links, preserving command order and
preferring an unchanged original link ID. A reused ID for a different pair is
never selected. Ordinary revisions, publication-cache eviction and recreated
same-pair links do not themselves cancel unchanged authority. A replaced
dependency stops future work rather than rebinding the original intent.

A child must have a real matching `Observed`/`ObservedNoOp` receipt, a fresh
same-epoch observation, zero in-progress calls, closed local dispatch and an
accepted Session graph covering that observation. Zero-command progress caused
by already-absent pairs remains bounded and creates no fabricated child receipt.
Final completion also checks the whole captured pair/target postcondition on
the current witnessed graph. This is server-fed postcondition evidence, not
causal transaction acknowledgment or WirePlumber policy convergence. In
particular, clearing a target does not prove a policy manager has finished
relinking it.

Bounds are 16,384 original planned actions, 512 real child receipts, at most 32
native commands and 32 logical progress units per drive, one in-flight child and
30 seconds total. No new timer, worker, inbox consumer or backend sequencer is
added. Other oversized actions remain unsupported.

## Partial outcomes and recovery

Admission failure, child rejection/unconfirmed status, lost authority, reverted
earlier effects, disconnect, total timeout or application finalization stops
future children. A claimed call can finish later; the parent keeps its failed
outcome and retains all real receipts. Existing explicit routing recovery
requires every child's actual local execution closure, released history
ownership and fresh authenticated graph/config inspection. It does not convert
the failed prefix into history, retry, roll back or claim audio convergence.
Stop/quit authority is unchanged.

The routing UI retains its parent request throughout child progress, including
while recording. It displays observed planned actions and retained child count,
not admission as completion. The final outcome uses the same routing graph
barrier and explicit recovery surface as ordinary requests.

## Synthetic checks

All graphs, names, identities and reports are authored generic fixtures. The
tests neither start PipeWire nor access a user's files, hardware or recording.
Fourteen focused checks cover:

- More than two children, exact forward ordering, one-step Undo/Redo, original
  full inverse pair set and unchanged Rec links.
- Duplicate pairs across chunk boundaries, recreated link IDs, more than the
  native publication-cache budget of ordinary revisions, and no-op observation.
- No next child/history acknowledgment before real observation; changed
  generation, reverted prefix and skipped-unit prefix checks.
- Second admission rejection, partial/unconfirmed late execution, deadline,
  disconnect and quit, with real reports retained and no future batches.
- Mounted routing confirmation during synthetic recording, parent pending
  state, one history entry and explicit no-prefix-Undo recovery.

Focused command (serial, isolated synthetic Xvfb/D-Bus for GTK cases):

```sh
cargo test -p pecan-ui-gtk --locked large_reset -- --test-threads=1
```

Local combined validation passed: 86 validator controls, 79 core tests, 253
backend tests, 595 all-feature GTK/input/D-Bus checks, native build, three
headless fixture paths, three GTK smoke paths and Cargo/ELF dependency closure
audits. A separate default-feature GTK run passed 541 checks; legacy headless
unit checks passed 10. All five existing audio examples compile; they are not
run. The guarded native script recorded zero JavaScript-tooling attempts.
These aggregate checks include the 14 focused checks above; they do not add
independent review or hardware evidence.

Physical desktop, audio hardware, policy-manager behavior under actual load,
recording continuity and performance remain untested. The user decision for
structural layout changes during recording is still gated. This candidate
does not remove JS or complete the full migration.
