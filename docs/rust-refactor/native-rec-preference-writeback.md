# Saved Rec preference writeback

Reviewed `3b9f6133` starts at `6b1ccc1e` and adds only writeback of accepted user
Rec layout, dB, mute and arming intent. It independently clears the writeback
contract and is now integrated with separately reviewed standard GTK shortcuts. Entrypoint mode selection, backend/core APIs, keyboard files, schema version,
dependencies and legacy application remain unchanged. Loading/reloading settings,
reconnect and passive backend readback issued no automatic Rec restore or start
in that writeback-only checkpoint. The separate [reviewed bounded restore](./native-rec-restore-candidate.md)
now covers matching-capacity idle startup/reconnect; general layouts and reload
policy remain gated.

## Legacy boundary and merge

`src/App.tsx:113` calls `bewaarBussen(busConfig)` when local Rec state changes;
it does not wait for the backend to complete. `src/bussen.ts` persists layout,
per-bus dB/mute/arming and the MIDI flag. Routing, hardware volume, rate/quantum
and ephemeral graph IDs are outside that preference. Backend reapplication on
reconnect is a different effect and is deliberately not implemented here.

The owner emits typed preference edits after its existing validation/shared
reservation and before audio handoff/completion. Stale or invalid actions rejected
before local acceptance are not preference events. Subsequent native admission,
rejection, partial or unconfirmed results do not erase accepted saved intent.
This is not evidence that the engine applied that preference.

The merge uses the latest validated `PreferencesState` document, rather than a
settings copy captured by a widget or a whole effective backend configuration:

- dB/mute changes edit only their explicit field and stable Rec key. An explicit
  native no-op can still replace a different saved value. Neither field edit
  overwrites the saved layout, other channels, mute/dB counterpart or arming.
  Stable-key entries can remain inactive in an un-restored saved layout; this
  writeback does not invent an implicit cross-layout field mapping.
- Arming stores the flags explicitly selected by the user action. Additive arm
  edits its one flag; exclusive arm uses its captured visible bus flags. Gain,
  mute and unrelated saved values are retained.
- Explicit add/split/join/remove edits select the requested live layout. Existing
  pure core layout mapping carries latest saved state by stable Rec key, with
  captured state only as fallback for keys not saved. Removal stores no route
  commands or endpoint identities. A field edit never saves the native startup
  two-channel layout over an un-restored saved layout.
- Saved MIDI, clock and all display values survive every merge. Native MIDI stays
  effectively off. Logical native-default initialization is not a Rec preference
  choice and produces no writeback; neither do receipts or readback polling.

For example, a requested -18 dB can be saved while actual readback is still 0 dB.
If the audio request is rejected, a later Mute action retains the saved -18 dB
rather than copying 0 dB from the engine. The Buses page still reports actual
readback/pending/error separately. No optimistic audio completion or graph Undo
is manufactured by preference serialization.

## Existing worker and lifetime

`BusOwner` has one crate-private preference sink, weakly bound to its owning
shell. Calls run outside mutable owner/model borrows. They share no new queue,
worker or counter. A reentrant preference handoff is rejected; after the callback,
original presentation/identity and admission are checked again before audio
handoff. These local checks do not add a missing atomic full-config generation
contract to the backend.

The initial explicit settings load precedes acceptance of preference-bearing
input. If it is not ready, the operation fails locally, releases its shared
reservation and sends no audio request. Memory-only mode is ready immediately.
After load, corrupt/future/error input can retain valid local edits under the
existing recovery rules, while unsafe disk publication remains blocked.

The existing revision-aware coalesced worker saves the latest unsent document.
Older Saved results advance their disk baseline without overwriting newer Rec
or display edits. Queue-full leaves the latest dirty document owned by the UI.
Post-rename failure preserves that local document and requires explicit reload
and save confirmation. The existing exact-input recovery contract is unchanged.
No external reload-during-recording policy is assumed or changed by this slice.

The existing quit drain admits the final dirty document, drains its results and
joins off UI. Closing audio admission can cancel an unsent audio target while
still saving that target as user preference. The saved file is not an audio
flush acknowledgement. Existing native/storage failures remain available to the
common explicit error-exit flow, even if later preference saving succeeds.

Display application now compares actual display preferences and editability,
so Rec-only edits do not repeatedly reapply unrelated mixer layout. The settings
panel/recovery document still sees current full-document revisions. No other
view/history/shortcut changes are included.

## Original restore dependencies

Writeback alone did not supply restoration. The later bounded restore described
above now adopts original full-config authority and matching-capacity metadata.
The original dependency list, including broader unresolved cases, was:

1. A retained original-generation full-configuration witness/API. The reviewed
   gain-only API has that proof; the generic full-config CAS is not a substitute.
2. Initialization/reconciliation beyond the current explicit two-channel,
   unknown-metadata initializer, including saved mono and other channel layouts.
3. Safe channel shrink and routed growth/layout mapping. Existing staged idle
   split/join/remove does not supply arbitrary restore or routed Add semantics.
4. Explicit idle/recording/finalization and external-reload policy, preserving
   existing takes and exposing partial/unknown outcomes. Active-recording
   structural segmentation versus disabling edits remains a user-decision gate.
5. Separate MIDI/clock effect decisions. Stored values remain preserved and
   masked from native execution; load must not auto-record or discover/import a
   profile. Only an explicit supplied path or memory-only mode exists here.

Saved intent can therefore differ from engine state until a separate accepted
user action or future reviewed restore applies it. This candidate adds no
settings watcher, automatic reconnect action, recording or import operation.

## Synthetic checks

Tests use authored generic graph frames, injected audio services and explicit
unique temporary settings directories. No actual device names/IDs, topology,
captures, screenshots, private integration context or user profile is used.

The focused cases cover preserving masked MIDI/clock/display fields, explicit
native no-op versus a different saved value, latest-state layout carryover,
exclusive/additive arming, invalid values, delayed older saves, queue-full,
interleaved display edits, passive load/default initialization, initial-load
fencing, rejected audio followed by Mute, dynamic arming/routed removal before
receipts, a 1,000-update final quit save, post-rename failure/reload/confirmation,
stale input and graph replacement/reentry during the sink. Existing owner,
settings recovery and off-UI drain checks remain part of final validation.

A first test expected automatic successful quit after an archived audio rejection;
it was corrected to verify the existing explicit error exit. A collapsed-panel
Save test was corrected to address the actual expander child. Production failure
semantics and assertions were not weakened. Final combined results follow below. Source/synthetic clearance is separate from
real PipeWire/audio, physical desktop/input and performance acceptance.

## Candidate validation

Fresh source-matched checks pass 86 validator controls, 76 core, 219 backend,
457 all-feature GTK and 412 default GTK tests. The 15 new cases cover writeback
policy, worker interaction and the real shell/audio-adapter boundary; the focused
`rec_` filter passes 19 cases including four existing controls. Native build,
formatting, Cargo/ELF dependency audits, three headless fixtures and three GTK
smokes pass. All ten legacy library tests and compilation of five legacy examples
pass. Workspace host/target packages were rebuilt before the final check. The
Node-tool attempt inventory is empty.

Source/privacy, whitespace, unchanged versions/manifests/lockfile and untouched
core/backend/keyboard-file checks pass. No final failed check or environment
blocker remains. The exact candidate hosted run 40163554352 succeeds. Independent review passes
19 Rec checks, 11 preference checks and three additional probes for delayed-save
coalescing, rejected gain through layout/unrelated settings, and protected input
with quit. No automatic restore, hardware/audio, physical desktop or performance
acceptance is claimed. Combined adoption receipts are recorded below/in the PR.

The combined reviewed adoption with standard GTK shortcuts passes 86 validator
controls, 76 core, 219 backend, 468 feature GTK and 414 default GTK tests, native
build/formatting/dependency audits, three headless fixtures, three GTK smokes,
ten legacy library tests and compilation of five legacy examples. No Node-tool
attempt is recorded. Runtime writeback modules match `3b9f6133` exactly; automatic
restore and the original-generation full-config API were absent at that checkpoint.
Both have subsequent separately reviewed bounded follow-ups. Privacy/scope,
whitespace and unchanged versions/lockfile checks pass. Hosted exact-head evidence
is recorded separately in the PR description.
