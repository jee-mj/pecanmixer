# Native connectivity fixtures

These sanitized, hand-reviewed inputs use the existing `src/api.ts` / Rust
`Momentopname` wire contract and the browser fixture's Spotify, speaker and Rec
bus pattern. They are authored test data, not recordings of hardware or proof of
feature parity. Device names and object IDs are synthetic. Arrays are deliberately
out of display order. `reconnect-after.json` retains names but replaces every
session object ID. `transport.json` exercises the complete existing transport
payload; no file is opened and no recording is started by these fixtures.

The expected overview records only the initial native preview's read-only node
and port projection. Mixer grouping, routing policy and recording parity are
outside this slice.
