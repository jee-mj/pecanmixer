# Pure settings contract fixtures

This optional follow-up is separate from the first published native batch. It
adds only core types/functions and synthetic tests; it reads no actual user's
settings and writes no files, browser storage or desktop preferences.

The inspected legacy sources are [App.tsx](../../src/App.tsx),
[Mixer.tsx](../../src/Mixer.tsx), [bussen.ts](../../src/bussen.ts),
[mixer.ts](../../src/mixer.ts) and the existing recorder's
[64-channel limit](../../crates/pecan-pipewire/src/opname.rs), unchanged in the
first batch at `a0fa112946f7d9bb12d67b88b39be700e916667b`.

## Actual persisted source fields

| Web localStorage entry | Source default / conversion |
|---|---|
| `pecanmixer.bussen` | `bussen: [2]`, `stand[rec-1-2] = {db: 0, mute: false, gewapend: true}`, `midi: true`; native names are widths/states/armed |
| Historical `paren` inside the same bus entry | Coupled becomes `[2]`, uncoupled `[1,1]`; `stand ?? {}` and `midi ?? true`, matching the current legacy migration branch |
| `pecanmixer.verborgen` | Empty set; row/strip/bus/MIDI keys are based on names, not raw PipeWire object IDs; duplicates are removed |
| `pecanmixer.klok` | `start_stop: false`; the source explicitly resets `klok` to null on load, so native storage has no clock source field |
| `pecanmixer.rijen` | Apps/hardware/destinations weights default to 1 each; source `bestemmingen` maps to native `destinations` |
| `pecanmixer.deling` | Null means automatic width; otherwise a pixel number. The authored drag control has a 120px minimum |

MIDI configuration is preserved as a preference, not enabled. The runtime MIDI
feature remains disabled. Patch positions, selected destination/bus, active tab,
view history, undo stack, routing, graph objects, meters and transport state are
not imported: the source does not persist them in these five entries.

## Contract and API

`pecan_core::settings::SettingsV1` serializes as `{schema_version: 1,
preferences: ...}`. Its fields are private; construct with `SettingsV1::new` after
editing a cloned `Preferences`. Serialization/deserialization uses existing
serde. `Decoder` injects JSON decoding, keeping serde_json test-only in the core.
The existing GUI's JSON dependency can implement it later; no runtime adapter
is added here.

- `load_native` distinguishes missing, loaded, corrupt and unsupported-version
  input. Corrupt/future input produces in-memory defaults with an explicit
  preserve-input status. A future filesystem adapter must honor that status;
  this pure function never overwrites or deletes anything.
- `migrate_web` accepts an explicitly supplied map of storage keys to JSON
  strings, reads only the five known keys, and does not mutate it. Bad components
  fall back independently and return bounded diagnostics. Unknown legacy fields
  are not carried into native storage.
- `HiddenKey` accepts the existing name-based key forms, not bare object IDs.
  Native documents reject unknown fields at every storage layer, including any
  clock port, node, link or route field. Names are opaque source keys; this does
  not promise that every application's node name remains stable forever.
- Native validation requires mono/stereo bus widths within the actual 64-channel
  backend limit, finite bus dB in the authored -90..+6 range, positive finite row
  weights with a finite sum, and null or finite width >=120px. It does not clamp
  or invent compatibility for malformed permissive-JavaScript/localStorage
  values. Valid inactive `rec-*` state keys are retained, as the source keeps
  them rather than automatically pruning them.

## Evidence

- `native-default-v1.json`: hand-authored source defaults and native round trip.
- `web-current.json` / `native-current-expected-v1.json`: complete five-entry
  migration, duplicate hidden key, optional arming, row mapping and clock ID drop.
- `web-pairs.json`: the existing historical coupled/uncoupled migration, including
  missing arming and explicit-null MIDI default behavior.
- `web-corrupt.json`: independent invalid components, filtered non-string/raw-ID
  hidden values, and an ignored clock selection.
- `native-future.json` / `native-truncated.txt`: preserve unknown version and
  damaged input, with no automatic rewrite.

Focused command: `cargo test -p pecan-core --test settings_contract --locked`.
Twelve settings tests cover these fixtures plus validation boundaries, closed
native fields, default merge rules, inactive bus state and transient exclusion.
The whole core suite has 36 tests (24 existing plus these 12). Tests run without
Node/npm, a display, PipeWire or native development libraries.

This does not establish a WebKit export mechanism, on-disk schema migration,
atomic persistence/backup/recovery, settings UI or native effect wiring. Those
remain separate runtime work and this commit need not block earlier batches.
