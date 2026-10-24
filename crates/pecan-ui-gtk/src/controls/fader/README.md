# Pure fader interaction candidate

`controls::fader::Fader<T>` ports `src/Mixer.tsx::Fader` only. It reuses core
`levels` for the existing fader curve. It does not replace any GTK callbacks,
change Session/owner/backend/core, dispatch commands, or record history. Native
startup remains observational; version stays 0.1.0. All test values are synthetic.

`pointer_down` emits Begin without changing gain; `pointer_move` emits relative
Update values using height minus the 24 px thumb (160 px fallback), Shift ×0.2,
the existing nonlinear curve, clamping and JS 0.1 dB rounding. `observe` updates
the observed value but does not move the local position during a pointer drag.
`pointer_up` and `pointer_cancel` each emit one End, not another Update. The
legacy pointercancel finalizes; it does not restore old values. A click without
motion can therefore End at rounded dB without ever proposing an Update.

Wheel uses the current observed dB, steps by 1 dB or Shift 0.1 dB, maps off to
the -61 sentinel, maps values below -60 back to off, and caps at +6. The first
token stays attached across a burst. An explicit `advance` delivers the 600 ms
idle End. Calls follow actual event-loop order: delivering another wheel first
can replace an already-due timer before its callback runs. Repeated wheel input
without new observation repeats the same proposed value, matching JS. Shift
wheel up from off still proposes off in the original source.

Double-click emits Begin/Update(0)/End(0). Like JS, it does not implicitly cancel
an existing wheel timer. Pointer and wheel refs are independent, so overlap is
represented explicitly; the future owner must serialize/reject overlapping
groups rather than copy the legacy card's shared mutable before-value ref.

There is no internal event queue or timer. State holds at most one pointer token
and one wheel token; each call returns at most three outputs. Motion events emit
proposals immediately, not a claim that dispatch has been coalesced. The caller
must consume outputs and coalesce accepted desired values. This module alone
does not establish latency, rendering or performance acceptance.

## Adapter contract still required

`T` is opaque caller-owned interaction identity, not a backend ticket/receipt.
Begin requests an identity-bound group; Update proposes a desired gain; End
marks input completion. None authorizes history success. The owning application
must supply the reviewed Session/owner adapter: authoritative registry validation,
exact native origin Arcs, one in-flight receipt plus one latest desired value,
fresh observed graph barriers and a single before/after history group after
the final confirmed sample. An End with no Update is not evidence of a write.
An unconfirmed intermediate/final sample must not become a claimed successful
history entry. Do not feed optimistic `RecordVolume` from these outputs.

For disconnect, replacement or unmount the owner calls `invalidate`, which emits
Abandon for retained tokens and cancels their local timers. It cannot cancel or
roll back already-admitted work. `observe` alone does not validate identity or
rearm invalidated inputs. Fresh input requires a freshly authorized token.

Finite-number, height (>24 px), duplicate pointer-down and monotonic-clock checks
are explicit native validation beyond the JS oracle's malformed-input handling.
They reject invalid input without altering a valid gesture. No keyboard or
accessibility additions are implemented here: original Fader has neither a key
handler nor a focusable range role. Future native keyboard/AT actions need named
dB values, reset/fine-step equivalents and their own identity/completion tests.
Partial-strip mute restoration is outside this candidate.

## Reproducible source-oracle tests

`oracle.mjs` is optional test tooling, not a native runtime dependency. It reads
the retained public JS source, extracts its unchanged interaction callback
bodies and fader-law functions, removes the nested PointerEvent type annotation,
and runs them with synthetic event listeners, props and timers. Opaque tokens
annotate input groups for comparison; JS has no such completion authority.
The committed JSON records the Mixer.tsx SHA-256 and 83 traces covering drag,
Shift, height changes, delayed observations, cancellation, wheel grouping/off/
clamps, event ordering, reset and overlap. JS rounding fixtures include ties,
the float below positive half and negative zero. Those boundary inputs retain
their exact IEEE bits so JSON parser rounding cannot change the tested float.

From repository root:

```sh
node crates/pecan-ui-gtk/src/controls/fader/oracle.mjs
cargo test -p pecan-ui-gtk --locked controls::fader
```

Cargo tests consume committed JSON and require neither Node nor a display/audio
server. Additional Rust tests cover 100,000-event pointer and wheel storms,
constant token retention, explicit invalidation and malformed input. Existing
GTK and owner integration remain unchanged. Actual widget, physical desktop,
assistive-technology and live audio acceptance are not established by this slice.

Cloud validation: eight focused Rust tests pass in default and `a11y-tests`
feature modes, without a display or audio server. Oracle regeneration is
byte-identical. These checks validate the pure policy and its source fixtures;
the accessibility feature build does not test assistive-technology interaction.
