# Stage 2 audio domain batch (WIP)

This batch adds a coherent pure Rust audio-policy library on top of the first
native connectivity slice at `fd514a77822ae5a4b3b186dfd541e11fcb882ed0`. It does
not complete Stage 2 and does not make the GTK preview an interactive mixer.
The existing TypeScript remains the production behavior oracle.

## Implemented scope

New modules live under `pecan_core::domain`, with no dependency additions:

| Module | Source behavior and evidence |
|---|---|
| `audio` | Supported audio patchbay grouping/order, labels/icons, monitor hiding, visible links, meter-channel grouping and duplicate keys from `indeling.ts` |
| `mixer` | Apps/hardware/destination strips, driver ordering, AUX stereo/odd pairing, Rec bus strips, mono fan-in/out, channel-name matching, auto/manual/custom classification, target-vs-link decisions and Back-to-auto plans from `mixer.ts` |
| `buses` | Bus definitions, split/join, inherited fader/mute/arming state, exclusive/additive arming, gains, append and ordered route migration/removal/resize plans from `bussen.ts` |
| `levels` | Fader conversion/dead zone, selected-channel gain helpers, -60dB meter mapping, 24dB/s fall, 1.5s peak hold, 2s clipping and 100ms elapsed cap from `Mixer.tsx`/`meters.ts` |
| `history` | Data-based connect/disconnect/move/route/reset/volume/mute intents, inverse plans, current-link-ID resolution from port pairs, a 200-step undo/redo limit and once-per-gesture volume recording from `handelingen.ts`/`geschiedenis.ts` |

The history reducer serializes command plans using an explicit acknowledgement
and graph-revision contract in place of the frontend's arbitrary 250ms delay.
Nonempty plans remain pending until both a command result and a newer graph at
or after its completion stamp have arrived. Failed results leave stacks
unchanged; disconnect/new epoch clears pending work and history. An already
satisfied intent can produce no commands and still record history, matching the
legacy closure-based behavior. This reducer is exercised headlessly; it is not
wired to production command completion or GTK callbacks in this batch.

The synthetic [fixtures](../../fixtures/domain/README.md) were authored by
tracing the existing functions. Expected output includes intentional edge cases,
not a generated copy of the Rust result. Tests cover custom routes remaining
untouched, restored default streams, Rec route preservation, input/output target
selection, missing serials/ports, recreated link IDs, bus state migration,
malformed configuration, level boundaries, history failure/epoch transitions,
bounds and redo invalidation.

## API and integration boundary

Existing shared graph DTOs, `Opdracht`, `BackendCommand`, events and backend
interfaces are unchanged. No backend dispatch, recorder, tray, GTK, TypeScript or
web-build file is changed by this domain batch.

- `BusConfig::definitions()` supplies `BusDef` values to `Mixer::build`.
- Mixer/bus planners return ordered `Vec<Opdracht>` values without sending them.
- `history::Controller::reduce(Input)` returns an optional `CommandPlan` with
  `ticket`, source `epoch`/`revision`, and commands. A future adapter must stamp
  and validate these at the production backend boundary.
- `Input::CommandResult` requires an actual completion stamp from that future
  adapter; queue receipt alone does not establish command completion. The
  reducer's ticket is local history state, not a new backend request protocol.
- Callers supply monotonic time to `levels::Meter`; there are no clocks, sleeps,
  threads, filesystem writes, settings writes or backend side effects here.

## Validation

In the cloud, with Node/npm absent from `PATH`:

```sh
cargo test -p pecan-core --locked
```

PASS: **24 tests** (18 domain tests plus 6 existing connectivity tests),
0 failed. Scoped `cargo fmt --check`, `git diff --check`, fixture JSON and
relative documentation-link checks pass. The core dependency tree remains free
of GTK, Tauri, PipeWire and JavaScript engines; the Cargo lockfile and all
application/crate versions remain unchanged. A minimum-Rust-version build was
not run. No native system headers, display or audio server are needed for these
core tests.

Independent review of the original domain commit additionally reported a matching
TypeScript fixture, 156 matching route/toggle comparisons and five matching reset
plans. This is separate review evidence, not a committed cross-language test
harness. The original 22 tests passed there; two later characterization tests
cover the following public API limits:

- `Action::Batch` plans all members against one immutable snapshot. It is for
  independent primitives, not dependent sequential edits or an atomic backend
  transaction. The generated reset/undo batches satisfy this restriction.
- Nonempty command plans require actual completion plus a newer graph even when
  the requested state was already satisfied. A future completion adapter must
  provide observation progress for confirmed no-ops; queue admission alone does
  not release the pending history transaction.

## Remaining parity and sequencing

- **Stage 2 continues:** native settings schema/import policy, deterministic
  patch geometry, exact locale-sensitive Unicode collation, partial-strip mute
  restoration state and complete UI-gesture/state parity still need work.
  Invalid/non-finite new API input is rejected or bounded; this is not a claim
  to reproduce malformed JavaScript/localStorage behavior.
- **Stage 3:** consume the domain models in native GTK controls only after the
  backend command boundary and completion adapter are ready. No GTK widget or
  live mutation acceptance is supplied by this batch.
- **Stage 4:** native patch editing and animated meters, pointer/keyboard and
  non-spatial accessibility workflows remain separate UI work.
- **Stage 5:** recording lifecycle/finalization, tray/autostart/folder actions,
  settings persistence and desktop behavior remain unfinished.
- **Stage 6:** parity, performance/accessibility/package acceptance and cutover
  still precede legacy JS/Tauri/WebKit removal and any version/release work.

MIDI remains disabled and its UI policy is not ported here. No live PipeWire,
hardware/recording, real-time performance, desktop/accessibility or packaging
claim follows from these fixture tests.
