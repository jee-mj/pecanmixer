# Partial-strip mute memory candidate

`controls::partial_mute::Memory<T>` ports the selected-channel gain memory in
`src/Mixer.tsx::Kaart`. It uses existing core `strip_db`/`with_db` helpers and
already-projected `Strip` values. No routing/grouping rules or graph DTOs are
duplicated. Whole-node mute and Rec-bus gain/mute are rejected by this API.
Nothing is mounted in live GTK, Session, owner, backend or startup. Version,
dependencies and common status documents are unchanged.

## Source behavior

Mute remembers the selected channels' arithmetic mean as dB, sets those channels
to zero and preserves every other channel. Restore applies the remembered mean
gain equally to the selected channels, preserving other channels from the current
snapshot. It does not restore their original imbalance. Every proposal retains
the complete original and proposed vectors, so the owning history path can undo
to the original unequal gains exactly. This module never writes that history.

For example, selected `[0.25, 0.75]` restores as `[0.5, 0.5]`; the mute proposal's
original full vector still contains `[0.25, 0.75]` for undo. Initially zero gain
can toggle the local marker while both full vectors remain identical; it must
not invent a successful audio change or history step. Original below-off mean
gain is floored at -90 dB, so it also restores as zero, matching JS.

The marker is active only with compatible memory and selected gain at off.
Positive external gain does not silently delete memory; the next mute stores
that current mean. A partial fader proposal forgets only its exact strip key,
including movement to off. Other strips' memory remains. Keys use exact string
equality, including case and prefixes, matching the original Map. Hiding alone
does not require forgetting session memory.

## Explicit owner contract

`propose_toggle` / `propose_fader` return immutable full-vector proposals and
stage a memory transition. Preparation or dropping a proposal changes no memory.
The owner supplies `T`, an opaque identity with equality that represents the
actual backend owner/epoch/registry generation across ordinary graph revisions.
Do not substitute stable keys, numeric IDs, serials or visible DTO equality for
this authority. Node/port/channel/vector shape checks are additional guards.

The future adapter must retain the exact native origin, dispatch through the
shared reviewed ticket/receipt path, validate observed completion and current
identity, then call `apply_memory`. This method applies only local memory; it
does not authenticate supplied identity, inspect a receipt or assert that the
provided strip is truly observed. It deliberately does not copy backend success
or postcondition rules. No optimistic `RecordVolume` or new dispatch protocol is
introduced. Accepted no-op proposals still need the existing completion contract;
the owner/history reducer decides whether unchanged vectors warrant a step.

Rejected proposals may be dropped. After unconfirmed effects, the owner must
reconcile/invalidate rather than guess whether gain or memory succeeded. Undo
and redo use the retained full vectors through the shared history contract;
their interaction with this session cache needs explicit integration coverage.

`invalidate_identity`, `invalidate_key` and `clear` retire outstanding proposals
and forget applicable memory. A different identity cannot use an old entry even
if its visible DTO/key is identical. These hooks do not cancel admitted writes.
Proposal application also checks its originating Memory instance and revision,
preventing delayed/repeated/cross-cache application. A memory change retires
other prepared proposals, consistent with a serial owner; parallel operations
are not an advertised contract.

Capacity is an explicit constructor parameter. Exhaustion reports `Capacity`
before a new remembered entry is proposed, rather than evicting a gain or falling
back to whole-node mute. Existing-key restoration/forgetting remains available.
This bounded capacity, identity/revision invalidation and malformed-input checks
are native safeguards beyond the original global JS Map. No UI policy for these
errors is implemented. The owner must budget capacity and invalidate removed
bindings. Non-finite/negative vectors, missing/duplicate/out-of-range channels,
whole-node/bus input and revision exhaustion fail without mutating memory.

## Synthetic source-oracle checks

The optional `oracle.mjs` generator executes retained public `Kaart` callback
bodies, `stripDb`/`metDb` and `volumeVastleggen`, with synthetic strips, memory,
writes and history callbacks. The fixture contains source SHA-256 values and
13 traces covering unequal and zero gain, repeated toggles, mean restoration,
external edits, partial movement, sibling keys, case/prefix boundaries and
original full undo vectors. Simulated observations round to the existing f32
DTO representation. These success traces apply memory after each simulated
observation; they do not establish backend execution or acknowledgement.

```sh
node crates/pecan-ui-gtk/src/controls/partial_mute/oracle.mjs
cargo test -p pecan-ui-gtk --locked controls::partial_mute
```

Cargo reads committed JSON; Node is optional fixture tooling, not a native
runtime dependency. Additional Rust tests cover proposal rejection, identical
visible replacement, binding changes, invalidation, cross-cache/delayed results,
capacity and invalid inputs. No physical input, screen reader, hardware, audio,
performance or production integration acceptance is claimed.

Cloud validation: ten focused tests pass in default and `a11y-tests` feature
modes, without a display/audio server. Thirteen oracle traces exercise 42 gain
proposals, with bitwise preservation of unselected channels. Regeneration is
byte-identical; the accessibility feature build is not an AT interaction test.
