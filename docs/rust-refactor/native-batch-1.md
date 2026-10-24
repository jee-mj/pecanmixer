# Native migration batch 1 (draft)

This batch advances the [remaining Stage 2–6 sequence](./pecanmixer-issue-1-native-gtk-proposal.md#staged-coexistence-and-cutover) after the first native connectivity slice. It combines initial pure audio policy, production backend admission/epoch hardening, and a read-only GTK presentation. It does not complete any remaining stage or activate native mixer mutations. Application/crate versions remain `0.1.0`; the legacy application remains supported.

This records the first published batch. See [batch 2](./native-batch-2.md) for the later read-only patchbay and pure settings-contract increment.

## Scope and integration boundary

- **Pure policy:** [the domain batch](./stage2-domain.md) supplies audio grouping/strips, channel pairing, route classification and plans, bus layout edits, level math and serialized undo/redo. It has no dispatch or toolkit side effects and does not change shared DTOs.
- **Backend hardening:** `Graaf` implements the existing `Backend` trait. Commands enter a bounded FIFO stamped with their session epoch, and the worker revalidates epoch, connection and live object references before execution. Drop requests cancellation and discards pending work. Native observation remains a separate handle without a mutation API.
- **GTK presentation:** [native strips](../../crates/pecan-ui-gtk/README.md) consume the existing read-only overview. They show grouped input/output ports, connection/empty state and notices, retain disclosure/focus state, reflow and scroll large collections, and publish accessible names/status roles. This display does not infer domain routing/stereo rules or dispatch commands.

The core history reducer's command-result ticket/completion-stamp contract is not connected to backend queue admission. A successful production `dispatch` means accepted into the queue, not applied or acknowledged by PipeWire. The native UI must remain read-only until intent-origin epochs and actual command completion are integrated and tested. Legacy IPC has no intent epoch; admission stamping cannot detect an already-obsolete frontend ID reused before admission.

## Review status

The GTK presentation review found no blocking source issue. Its small GTK accessible-name binding matches the native header signature and preserves the selected default base API; actual runtime validation is against GTK 4.18, not GTK 4.0.

Three backend boundary findings were corrected and independently re-reviewed:

1. The 256-entry FIFO now reserves its final slot for stop-all (255 ordinary entries). Adjacent duplicate stops coalesce, while intervening work retains FIFO order.
2. Session epoch invalidation atomically clears stale pending capacity before fresh admission, allowing the connected-transition configuration replay to be admitted.
3. Commands bind the published object generations at admission and compare them against the live registry immediately before execution, including referenced node owners, ports, links and clock sources. Same-session replacement IDs no longer pass solely because the numeric ID exists.

The corrected boundary passed 21 backend tests and the independent original ID-reuse/capacity regression probes. Cancellation clears queued work; one command already dequeued can still execute or finish. Success remains admission, not PipeWire acknowledgement. Pre-admission stale intent from an already-superseded caller snapshot still requires richer intent identity; legacy IPC is unchanged, and the native UI stays read-only.

No recorder finalization acknowledgement is introduced by this batch. Queue admission, cancellation requests and display tests must not be described as proof of graceful recording shutdown.

## Validation

| Combined cloud check | Result |
|---|---|
| `cargo test -p pecan-core -p pecan-pipewire --locked` | PASS: 24 core + 21 backend tests |
| `cargo test -p pecanmixer --lib --locked` | PASS: 4 legacy tests |
| `cargo check -p pecanmixer --examples --locked` | PASS: all five existing examples compile |
| `cargo build -p pecan-ui-gtk --locked` | PASS |
| Xvfb + session-bus GTK library fixture contract, default and `a11y-tests` features | PASS in both modes with fatal criticals enabled; optional properties checked using GTK's test backend |
| Three native headless fixture outputs and three GTK fixture smokes | PASS; audio-basic JSON equals committed expected output |
| Native Cargo tree / linked-library inspection | PASS: GTK4 present, no Tauri/Wry/WebKit/JavaScriptCore path or web-engine linked library |
| Scoped formatting, diff/fixture/documentation/version checks | PASS; lockfile, legacy JS/IPC and version `0.1.0` retained |

Build/test commands ran with Node/npm absent from `PATH`. The GTK contract checks 120 strips, 300 ports, long names, scrolling, focus/disclosure preservation, reconnect/stale graph handling and allocations at 1400×900, 900×600 and 420×500. These are synthetic assertions, not performance measurements or physical desktop/screen-reader acceptance. The three backend review findings above have been corrected and independently cleared. Full parity, command-completion integration and the unrun acceptance gates remain unfinished.

## Remaining sequence

| Stage | Current boundary | Remaining acceptance |
|---|---|---|
| 2 — pure policy | Initial supported-audio slice and fixtures | Settings schema/import policy, patch geometry, exact locale collation, partial-strip mute/gesture/state parity and completion integration |
| 3 — GTK mixer | Read-only native observation strips | Bind domain strips/routes and safe intents; faders/mute/destination/history; keyboard/accessibility and behavior parity |
| 4 — patchbay and meters | Pure channel/level math only | Native patch editing/geometry, animations, stress and non-spatial/keyboard workflows |
| 5 — lifecycle/integration | Existing recorder/tray behavior retained | Writer finalization acknowledgement, settings, tray/autostart/folder actions, desktop acceptance |
| 6 — cutover | Legacy JS/Node/Tauri/WebKit retained | Proven parity/performance/accessibility/package/license gates, then removal and merge-ready version work |

Not run: real PipeWire/WirePlumber session/restarts or devices, audio recording acceptance, real-time/latency/CPU/memory measurements, physical Wayland/X11 desktop regression, AT-SPI/Orca acceptance, packaging/distribution or final license audit. Synthetic widget allocations/focus/accessible-property assertions do not establish those outcomes. No merge, release, version bump or premature JS deletion is part of this batch.
