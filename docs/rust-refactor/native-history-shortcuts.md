# Native history shortcuts

Reviewed source `0fc653f7`, directly based on `6b1ccc1e`, adds window-scoped
standard GTK ShortcutController/KeyvalTrigger/CallbackAction bindings. The
integration preserves that runtime and test source exactly alongside separately
reviewed Rec preference writeback. No new physical-key tracker, worker, timer,
queue, backend API, dependency or version is introduced.

## Activation and identity

The original `src/App.tsx` window keydown accepts Ctrl or Meta with Z for Undo,
Shift+Z or Y for Redo, outside INPUT/SELECT/TEXTAREA. Extra modifiers do not
change that dispatch; Shift+Y also means Redo. GTK aliases include Linux Super,
combined Ctrl/Meta/Super and extra Alt/Hyper modifiers; GTK handles lock modifiers.
The fixed table contains 112 triggers sharing two callback actions.

Each delivered activation synchronously selects the current history button.
That button retains its immutable graph/context and existing Session admission,
registry-generation and receipt/history checks. A stale button cannot recapture
newer authority during the same activation. Its rejection can refresh the view;
only a subsequent delivered activation can select the refreshed button.

Standard GTK activations follow the legacy keydown/repeat semantics. After a
request is acknowledged, a newly delivered autorepeat may Undo/Redo the next
current entry, including after focus returns. A physical release is not a new
application-level requirement. Existing pointer/Space/Enter history-button
capture/cancellation guards remain unchanged, and a held history button cannot
be bypassed by a shortcut.

Busy, stale, identity-unavailable and finalizing activations are dropped; they
are not queued or retried after acknowledgement. This is a stated remaining
parity gap: the original JS history queue is not implemented by these bindings.
No server completion or history progress is inferred from shortcut delivery.

## Native focus and lifetime

The window capture controller returns Proceed for native editable fields,
TextView, DropDown/ComboBox and input-style Range/CheckButton/Switch controls,
including their internal children. Native text Undo/Redo remains available.
Outside those fields, matching history chords are consumed like the original
window handler, including when there is no admitted history action.

The workspace retains one adapter. Root mapping attaches its local controller
to the owning window. Remapping the same window adds no duplicate; reparenting
removes the old window controller. Hidden/unmapped roots cannot activate it.
Weak references and explicit adapter teardown remove the controller without
retaining a workspace/window cycle. Other windows have no workspace binding.
No production X11 or global input hook is used; XTest remains opt-in fixture code.

## Independent evidence

The exact candidate hosted run 40163959165 succeeds. Independent source review
and 33 targeted history checks pass, including the existing pointer/Space/Enter
cases, real Entry/TextView Undo/Redo, stale/busy/finalization, autorepeat and
release/held input across another window. Three additional synthetic native probe
groups pass: extra modifier combinations/Caps Lock, composite input focus, and
controller reparent/drop lifetime with one activation per event. An empty
synthetic ComboBox initially could not take focus; populating/delegating focus
corrected that probe fixture. No production defect was found by that probe.

Only the standard GTK candidate is included. Combined integration checks and
exact published-head hosted results are recorded separately below/in the PR.
All fixtures are authored public/generic data on a private marked Xvfb bus/display.
No actual device names, topology, captures, private platform context or user
profile is used. Physical Wayland/X11/compositor interception, cross-application,
AT-SPI/Orca and real audio/performance acceptance remain unperformed.

Fresh combined source checks pass 86 validator controls, 76 core, 219 backend,
468 feature GTK and 414 default GTK tests, native build/formatting, Cargo/ELF
audits, three headless fixtures, three GTK smokes, ten legacy library tests and
compilation of five legacy examples. Host and explicit-target workspace packages
were rebuilt. The Node-tool attempt inventory is empty. Runtime modules match
the independently reviewed candidates exactly, with no reconciliation or old
latch inclusion. Privacy/scope, whitespace and unchanged versions/lockfile checks
pass. No final local failed check or environment blocker remains. Exact published
head and hosted outcomes are recorded in the PR description.
