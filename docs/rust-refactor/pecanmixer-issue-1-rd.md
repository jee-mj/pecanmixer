# Pecan Mixer issue 1 — no-JavaScript migration R&D

Date: 2026-10-24

Scope: research and design only for [jee-mj/pecanmixer issue 1](https://github.com/jee-mj/pecanmixer/issues/1)

## Recommendation

Replace the Tauri/React shell with a native **GTK 4 application written in Rust**, keep the existing PipeWire graph, metering, recorder, MIDI-parser, and StatusNotifierItem code behind typed Rust interfaces, and port the TypeScript mixer/patchbay/bus/undo models into testable Rust modules before changing the UI.

This is a genuine no-JavaScript architecture: the shipped process has no HTML renderer, WebView, JavaScript engine, npm assets, or Node-based build step. A Rust/WASM frontend inside Tauri, Yew, or Leptos does **not** meet that bar: it still runs in a WebView and leaves the WebKit/JavaScriptCore runtime in the binary's dependency graph.

Lua is not needed for feature parity and should not be in the 0.1.1 critical path. If user scripting is wanted later, make it an optional, non-real-time policy/macro layer over immutable snapshots; keep PipeWire objects, recording, link mutation, validation, persistence, and all real-time work in Rust.

The migration should not be merged on visual similarity alone. The current repository has only six Rust unit tests, no frontend tests, no graph/routing integration tests, and no CI. “Parity” therefore has to be demonstrated by new fixtures, model tests, isolated PipeWire integration tests, recorder output tests, accessibility checks, and a clean build whose toolchain and runtime are independently inspected.

## Audited baseline

The fork and upstream were fetched into an isolated workspace and compared directly.

| Item | Audited value |
|---|---|
| Fork | `jee-mj/pecanmixer`, `main` |
| Upstream | `BadgerSoft-NL/pecanmixer`, `main` |
| Fork HEAD | `57741f75eba111578b4d02566ff12ac5fa4400cd` |
| Upstream HEAD | `57741f75eba111578b4d02566ff12ac5fa4400cd` |
| Ahead / behind | 0 / 0; merge base is the same commit |
| Commit | 2026-10-24 10:37:43 +0200, “README: screenshots bij mixer, patchbay en opnemen” |
| Existing release tag | annotated `v0.1.0` object `068a184…`, peeling to commit `d2ac747…` |
| Current versions | `0.1.0` in `package.json:4`, `src-tauri/Cargo.toml:3`, and `src-tauri/tauri.conf.json:4` |
| Project license | GPL-3.0-or-later in both manifests; repository `LICENSE` is GPLv3 text |
| Lock fingerprints | `package-lock.json` SHA-256 `1683c9f1…d3aed4`; `src-tauri/Cargo.lock` SHA-256 `e861e9b2…fec6` |

The requested 0.1.1 version is a release gate only. No version or release file should change until the refactor has passed the acceptance gates and is actually being prepared for merge.

## What exists today

The public README describes the supported audio baseline at `README.md:15-54`:

- TotalMix-style audio mixer with destination editing, `target.object` stream movement, additional manual links, detection/preservation of custom routes, “Back to auto,” per-stream faders and mute, meters, hidden/resizable rows, and split destination/record-bus panes.
- Audio patchbay with ordered source/destination modules, drag-to-connect, cable click-to-disconnect, module positioning, rearrange, and mini meters.
- Multi-bus recording: stereo buses can split to mono, later buses move up with routes retained, source-to-rec routing, exclusive or Ctrl-additive arming, simultaneous sample-aligned recording, dynamic bus arm/disarm, one 32-bit-float WAV per bus, one directory per take, and folder opening.
- StatusNotifierItem tray behavior, window hide-on-close, optional XDG autostart, continued routing/metering/recording while hidden, and live graph reconnect after PipeWire restarts.
- MIDI mixer, patchbay, recording, and external clock exist in source but are deliberately disabled and untested with hardware. They are **not** part of current supported parity.

These are source claims, not a verified baseline. The migration's first phase must turn them into executable acceptance tests.

### React/Tauri boundary

The frontend contains 18 TS/TSX files (4,114 lines) plus one 1,495-line stylesheet. `src/api.ts` duplicates the Rust graph/transport schema, wraps 13 Tauri commands, listens for four events, and supplies an in-browser mock:

- Commands: `haal_graaf`, `verbind`, `ontkoppel`, `zet_doel`, `zet_volume`, `zet_mute`, `zet_transport`, `zet_bus`, `start_opname`, `stop_opname`, `start_bus`, `stop_bus`, and `open_map` (`src-tauri/src/lib.rs:12-80,130-144`).
- Events: `graaf`, `pieken`, `midi`, and `transport` (`src-tauri/src/lib.rs:105-127`).
- Most frontend invokes are fire-and-forget, so command failures are not presented except indirectly through recorder transport state.
- Closing hides the Tauri window; single-instance activation presents it; startup initializes the tray/autostart and starts the graph worker (`src-tauri/src/lib.rs:82-146`).

This API boundary should become in-process, owned Rust enums rather than a serialized string-command protocol:

```text
GTK main loop
  ↕ bounded/coalescing UI channel
App controller + pure domain model
  ↕ BackendCommand / BackendEvent
existing dedicated PipeWire graph thread
  ↕
PipeWire / WirePlumber
```

Graph snapshots may be `Arc`-owned; meter events must be latest-value/coalesced so a busy UI cannot back-pressure the PipeWire worker. The current borrowed `Melding<'_>` callback should not leak through the new boundary.

### PipeWire backend and behavior worth retaining

- `src-tauri/src/graaf.rs:1-19,240-293`: PipeWire runs on a separate thread, publishes full snapshots no faster than every 40 ms, and retries connection every two seconds. Core/session errors return it to the reconnect loop.
- `graaf.rs:740-880,1047-1114`: commands resize/configure the record bus, update transport, start/stop all or one bus, link/unlink ports, set `target.object`, and set volume/mute. WirePlumber stays policy owner.
- `src-tauri/src/meter.rs:1-140`: per-node passive capture and peak calculation. This low-level code is UI-independent.
- `src-tauri/src/opname.rs:1-193,381-596`: recording bus/filter, 32-bit-float WAV writing, take directories, constrained folder opening, and clock/MIDI timing. Armed buses share the same process callback and are sample-aligned; a bus started during recording gets a new take, and stopping one finalizes only that bus.
- `src-tauri/src/tray.rs:1-235`: direct D-Bus StatusNotifierItem via `ksni`. It is mostly reusable, but window/exit actions currently accept Tauri handles and quit waits a fixed 600 ms after asking the recorder to stop. Replace that delay with an explicit recorder shutdown/finalization acknowledgement and join.
- `src-tauri/src/autostart.rs:1-86`: XDG autostart and AppImage-aware executable resolution are independent of React. Rework path handling and retain its unit test.
- `src-tauri/src/graaf.rs:166` and `src/api.ts:82-84`: MIDI is disabled on both sides. Keep it disabled until a separate hardware validation plan passes.

### Domain behavior currently stranded in TypeScript

The following is application logic, not view code, and should move into pure Rust first:

- `src/indeling.ts`: graph-to-module classification, audio/MIDI grouping, channel interpretation.
- `src/mixer.ts`: auto/manual/custom route interpretation, destination matching, `target.object` versus links, cleanup, and “Back to auto.”
- `src/bussen.ts`: default/persisted bus configuration, channel/gain mapping, and ordered route migration when a bus is removed.
- `src/geschiedenis.ts` and `src/handelingen.ts`: serialized command execution and undo/redo (limit 200).
- `src/meters.ts`: peak smoothing/hold/decay (24 dB/s).
- `src/Patchbay.tsx` and `src/Mixer.tsx`: geometry, interaction state, fader law, channel split, and persistence are currently mixed with rendering; extract deterministic portions.

Settings currently live in WebKit localStorage under `pecanmixer.bussen`, `pecanmixer.verborgen`, `pecanmixer.klok`, `pecanmixer.rijen`, and `pecanmixer.deling` (`src/App.tsx`, `src/Mixer.tsx`, `src/bussen.ts`). Clock port IDs are intentionally cleared on startup; patchbay positions are session-only; routes are live PipeWire state and are not persisted. The native app should store a schema-versioned JSON file under `$XDG_CONFIG_HOME/pecanmixer/settings.json`, written atomically, and continue excluding transient PipeWire IDs and routes.

## Dependency and license inventory

### JavaScript/npm

The root manifest has four direct production and six direct development dependencies. The lockfile contains 133 package records excluding the root: 5 in the production closure, 128 development, 64 optional platform records, and 69 non-optional records.

| Class | Locked dependencies |
|---|---|
| Direct production | `@fontsource/ibm-plex-mono 5.3.0` (OFL-1.1); `@tauri-apps/api 2.12.4` (Apache-2.0 OR MIT); `react 19.3.1` and `react-dom 19.3.1` (MIT) |
| Transitive production | `scheduler 0.28.0` (MIT) |
| Direct development | `@tauri-apps/cli 2.12.3` (Apache-2.0 OR MIT); `@types/react 19.3.0`, `@types/react-dom 19.3.0`, `@vitejs/plugin-react 4.7.0` (MIT); `typescript 5.8.3` (Apache-2.0); `vite 7.3.7` (MIT) |
| Major transitive build families | Babel, esbuild and 25 platform packages, Rollup and 24 platform packages, Tauri CLI and 11 platform packages, browserslist/caniuse data, PostCSS, source maps, globbing, React Refresh, types |
| Declared-license totals | MIT 110; Apache-2.0 OR MIT 13; ISC 5; Apache-2.0 2; BSD-3-Clause 1; CC-BY-4.0 1; OFL-1.1 1 |

Every locked name, version, direct/transitive classification, optional flag, and declared license is in [the exact npm inventory](pecanmixer-issue-1-npm-inventory.md). Lock metadata is evidence, not a substitute for reviewing license texts/notices.

The source removal scope is wider than `src/`: `index.html`, Vite/TypeScript configs, package manifests/lock, and four `docs/ontwerp/project/*.dc.html` files contain script tags and executable `text/x-dc` design-code blocks. Those design artifacts must be converted to non-executable screenshots/notes or removed. Otherwise a repository-wide “no JavaScript” check will correctly fail.

### Rust/native

The Cargo lock contains 478 package records (446 unique names). Current direct locked versions are: `tauri 2.12.4`, `tauri-build 2.7.3`, `tauri-plugin-single-instance 2.5.4`, `ksni 0.3.7`, `pipewire 0.10.1`, `serde 1.0.229`, `serde_json 1.0.154`, `hound 3.5.1`, `midly 0.5.3`, and `libc 0.2.191`.

Removing npm is insufficient. Tauri's locked native closure includes `tauri-runtime-wry 2.12.2`, `wry 0.57.0`, `webkit2gtk 2.0.2`, `javascriptcore-rs 1.1.2`, `javascriptcore-rs-sys 1.1.1`, GTK 3 bindings, and `serialize-to-javascript 0.1.2`. All Tauri/Wry/WebKit/JavaScriptCore/serialization glue must disappear from the post-migration Cargo graph.

License work before merge:

1. Keep GPL-3.0-or-later for the combined work unless maintainers intentionally choose otherwise.
2. Run `cargo deny check licenses` against the final lock and generate distributable acknowledgements (for example with `cargo-about`); manually inspect native libraries, Apache NOTICE obligations, fonts, and bundled assets.
3. If retaining IBM Plex Mono, vendor only the required font files plus the OFL text and attribution. A system monospace fallback removes that asset dependency.
4. GTK 4 is LGPL-2.1-or-later and GPL-compatible, but binary bundles still need the applicable license/source-compliance material. This report is not legal advice.

## UI architecture choice

| Candidate | Strict no-JS? | Fit | Main risks | Decision |
|---|---:|---|---|---|
| **GTK 4 + gtk4-rs** | Yes | Linux/PipeWire scope; native scales, toggles, paned/scrolled layouts, CSS theming, DrawingArea; built-in accessibility; GApplication single-instance | System GTK version; custom patchbay accessibility; GTK main-thread discipline; distro/AppImage matrix | **Recommended** |
| egui/eframe | Yes | Excellent immediate-mode canvas for meters and patch cables; Rust-only | Native/custom controls need careful AccessKit semantics; GPU/winit and Wayland/X11 testing; desktop fidelity | Prototype fallback if GTK patchbay performance is inadequate |
| Slint | Yes | Retained declarative UI without Node; accessibility and Linux system-tray support; GPL option | Adds a UI DSL/compiler and rendering backend; licensing configuration; less direct reuse of existing `ksni`; accessibility still needs testing | Feasible alternative if another UI language is acceptable |
| Iced | Yes | Rust subscriptions and canvas suit event streams | Accessibility is still experimental/open; API churn; custom widget work | Not first choice for this release |
| Tauri + Yew/Leptos/Rust-WASM | **No** | Reduces hand-written TS | Retains HTML/WebView/WebKit/JavaScriptCore and often JS/WASM loader glue | Reject for issue 1 |
| Qt/QML | No for this criterion | Mature desktop toolkit | QML's JavaScript engine/expression environment conflicts with strict removal | Reject |

GTK implementation outline:

- Use `gtk::Application` with application ID `nl.jetse.pecanmixer`; GApplication's session-bus uniqueness replaces the Tauri single-instance plugin. Activation presents the existing window.
- Use `ApplicationWindow::set_hide_on_close(true)`; retain `ksni`, but have tray actions send typed controller commands rather than hold a Tauri `AppHandle`.
- Build mixer strips from native `Scale`, `ToggleButton`, labels, `ScrolledWindow`, and `Paned`. Use a CSS provider for the current dark visual language.
- Implement the patchbay with a positioned widget layer plus `DrawingArea` cables. Each endpoint and connection must also be reachable from a keyboard-accessible connection list/action surface; never make SVG/canvas geometry the only control path.
- Do not update a widget tree for every raw meter sample. Keep latest peaks in a bounded shared model and render at a capped frame rate.

The current UI has partial labels and ARIA, but important paths are pointer-only: the fader is a handled `div` without slider semantics or keyboard behavior (`src/Mixer.tsx:791-880`), patch ports/cables are non-focusable span/SVG targets (`src/Patchbay.tsx:218-255,344-349`), and splitters are pointer-driven. The native rewrite must add, not merely preserve, keyboard adjustment, focus indication/order, names/states, screen-reader announcements, and non-visual connect/disconnect operations.

## Optional Lua boundary

Default decision: **omit Lua from 0.1.1**. The existing application has no user scripting contract, and moving core logic to Lua would weaken type safety, determinism, real-time isolation, error handling, and license/dependency simplicity without preserving a current feature.

A later feature-gated `scripting-lua` crate could be justified for user-defined, non-real-time macros:

- Input: an immutable, sanitized graph/domain snapshot plus a small declarative API.
- Output: proposed high-level actions such as a route plan, bus template, naming rule, or UI macro.
- Enforcement: Rust validates IDs, bounds, conflicts, and policy; potentially destructive route changes require user confirmation.
- Isolation: a dedicated non-real-time worker with instruction/time and memory limits. Do not expose raw PipeWire pointers, callbacks, filesystem, processes, network, `os`, `io`, `package`, `debug`, FFI, or Rust userdata with ambient authority.
- Packaging: `mlua` with a pinned Lua 5.4 and `vendored` feature can avoid a system Lua dependency, but `Lua::sandbox` is a Luau-only convenience; Lua 5.4 globals must be constructed explicitly. Keep this feature off by default and outside the parity gate.

## Staged migration

1. **Freeze and specify the baseline.** Record the audited SHAs/locks; create graph, layout, mixer, bus, undo, and recording fixtures from source behavior; document which README claims are actually testable. Add CI before architecture changes.
2. **Decouple the backend without changing behavior.** Define owned `BackendCommand`/`BackendEvent` and a `Backend` trait; move graph, meter, recorder, MIDI parser, and autostart into Tauri-free Rust modules; introduce graceful recorder shutdown acknowledgement. Keep the old UI temporarily as an oracle.
3. **Port domain logic.** Translate `indeling.ts`, `mixer.ts`, `bussen.ts`, `geschiedenis.ts`, deterministic patchbay geometry, settings schema, and meter smoothing to pure Rust. Run old/new golden fixtures against the same graphs and expected command sequences.
4. **Build a small native vertical slice.** GTK application/single instance, status/reconnect banner, typed channel, settings, hide/show/quit tray behavior, and one read-only graph list. Confirm it builds and launches without Node before investing in full styling.
5. **Port features slice by slice.** Mixer route semantics and faders; patchbay editing; meters; record buses and transport; folder opening; autostart. Each slice lands with keyboard/accessibility and behavior tests. MIDI remains disabled.
6. **Remove the web stack only after measured parity.** Delete the React sources, package manifests/lock, Vite/TypeScript configs, `index.html`, Tauri config/build script/dependencies, and executable design-code HTML. Update source/build documentation. Check both repository contents and Cargo/runtime dependency graphs.
7. **Release gate.** Run clean, locked, Node-free builds and all parity/accessibility/package tests. Only when merging the completed refactor, set all surviving version sources to **0.1.1**, add release notes, and build distribution artifacts. Do not publish automatically.

A useful Rust layout is `pecan-core` (pure graph/view model, routing, buses, undo, config), `pecan-pipewire` (existing worker, meter, recorder, optional disabled MIDI code), and `pecan-ui-gtk` (controller, widgets, tray, settings). It may begin as modules in one package and split into workspace crates only when the dependency boundary is stable.

## Parity and acceptance tests

### Pure model

- Node/port classification and mono/stereo channel pairing, including incomplete/unknown metadata.
- WirePlumber-owned auto routes versus explicit `target.object`, manual links, custom routes that must not be touched, and exact “Back to auto” commands.
- Bus add, stereo split/merge, gain mapping, removal, renumbering, and ordered migration of every affected route.
- Undo/redo ordering, 200-entry limit, command serialization, failure behavior, and stale-ID handling.
- Settings defaults, round-trip, schema migration, corrupt input, atomic replacement, and proof that ephemeral port/node IDs and live routes are not persisted.
- Meter hold/decay/fader-law boundary values.

### Backend and PipeWire integration

Use fake backends for UI/controller tests, plus an isolated PipeWire/WirePlumber integration harness with its own runtime directory and socket:

- Add/remove nodes and ports; link/unlink; set and clear `target.object` by serial; volume/mute.
- Restart the isolated server; show disconnected state, reject/queue commands deliberately, reconnect on the two-second policy, and avoid stale object IDs.
- Verify full graph events are limited/coalesced at 25 Hz, transport at the current cadence, and UI stalls do not grow an unbounded queue.
- Verify passive meters do not change routing and the record node does not auto-connect.

### Recorder

Feed deterministic synthetic samples:

- WAV format is IEEE float, 32-bit, with correct sample rate, channel count, frame count, and finalized headers.
- All initially armed buses begin on the same callback/frame; dynamic arm creates its documented separate take; dynamic disarm closes only that bus; stop/quit finalizes all files before process exit.
- Stereo/mono channel selection and per-channel gains match the bus model.
- Take naming/collision handling, safe path confinement, failed I/O propagation, and folder-open fallback are explicit.

### UI and accessibility

- Screenshot/layout checks at the documented 1400×900 and minimum 900×600 sizes, high DPI, light/dark desktop settings, Wayland and X11.
- Keyboard-only traversal and actions for every fader, mute, edit, route, record-arm, patch connection, splitter, tray-equivalent command, and confirmation.
- AT-SPI/Orca inspection for names, roles, values, state, focus order, changing recording status, and error announcements.
- Large graph/rapid peak stress test with frame-time and memory/backlog limits.
- Manual tray behavior on KDE and GNOME with AppIndicator; clearly degrade when no StatusNotifier host exists.

MIDI acceptance for 0.1.1 is simply that both gates remain off, controls remain unavailable with an explanation, and no MIDI filter/port/clock is activated. Unit tests for parser/clock math do not establish hardware support. Enabling MIDI belongs to a separate issue with real devices and clock/start-stop matrices.

### Mechanical no-JS gate

A clean checkout must prove all of the following:

- No `package.json`, package lock, npm/yarn/pnpm/bun/deno script, Node invocation, JS/JSX/TS/TSX/MJS/CJS file, executable HTML script, generated web asset, or `node_modules`.
- `cargo tree`/metadata contains no `tauri*`, `wry`, `webkit2gtk*`, `javascriptcore*`, or `serialize-to-javascript`.
- `readelf -d`/distribution dependency inspection shows no WebKit or JavaScriptCore shared library.
- `cargo test --workspace --locked` and `cargo build --workspace --locked --release` succeed in an environment whose `PATH` contains no Node/npm. `--frozen` plus checked-in or prepared `cargo vendor` sources can be a reproducible/offline CI lane, not a hidden packaging prerequisite.
- The built application can run from the Cargo target directory with its documented system libraries; AppImage/deb/rpm creation is a separate job.

## Build, runtime, and portability

Packaging-independent source requirements should be explicit: pinned stable Rust/MSRV, Cargo, a C toolchain, `pkg-config`, clang/libclang while `pipewire-sys` uses bindgen, PipeWire development files, GTK 4 development files, and D-Bus development/runtime support. Runtime requirements remain Linux, PipeWire, WirePlumber 0.5+, GTK 4 shared libraries, and a session D-Bus. StatusNotifierItem support is optional for visibility but required for the advertised tray UX; `xdg-open` or a desktop portal is needed for folder opening.

Primary portability risks:

- GTK 4 minimum version versus supported distro matrix; bundled AppImage plugins/themes versus system integration.
- Wayland/X11 pointer capture and custom DrawingArea behavior.
- PipeWire/WirePlumber API and metadata differences; the current app relies on `target.object` and policy-owned links.
- Tray-host variance; hiding the only window is dangerous when no usable tray exists.
- Font metrics if the npm font is replaced.
- The current backend contains unsafe FFI/filter work but lacks graph/recorder integration coverage.
- Current fixed-delay quit may lose finalization on slow storage.
- `xdg-open`, `HOME` fallback, and autostart paths behave differently in sandboxed packages.

## Open decisions before implementation

1. Supported distributions/architectures and the minimum GTK/PipeWire/WirePlumber versions.
2. GTK 4 as final choice after a time-boxed patchbay + 25 Hz meter + AT-SPI prototype; egui is the fallback only if the prototype fails measured criteria.
3. Whether to attempt one-time import of WebKit localStorage. Its on-disk location and portability were not audited; a documented reset may be safer.
4. Vendor IBM Plex Mono with OFL notice, or use the desktop monospace font.
5. Exact accessible representation for patch connections and custom mixer layouts.
6. Whether folder opening should use `xdg-open`, a portal, or both.
7. Whether Lua is omitted entirely (recommended) or added later as an off-by-default feature.
8. Whether exact pixel parity matters more than native control/accessibility improvements.
9. Packaging targets and whether each target bundles GTK; keep that decision out of the Cargo build contract.
10. MIDI remains disabled until a separately scoped hardware test plan is approved.

## Primary references

Repository evidence is pinned to commit `57741f75eba111578b4d02566ff12ac5fa4400cd`; paths above are relative to the cloned `pecanmixer/` source.

- Issue and acceptance intent: [jee-mj/pecanmixer issue 1](https://github.com/jee-mj/pecanmixer/issues/1)
- Tauri's documented web frontend and webview process model: [Tauri start](https://v2.tauri.app/start/), [architecture](https://v2.tauri.app/concept/architecture/)
- GTK Rust bindings, installation, and accessibility: [gtk4-rs API](https://gtk-rs.org/gtk4-rs/stable/0.10/docs/gtk4/index.html), [installation](https://gtk-rs.org/gtk4-rs/git/book/installation.html), [accessibility](https://gtk-rs.org/gtk4-rs/git/book/accessibility.html), [GTK license/API](https://docs.gtk.org/gtk4/)
- Single-instance native application semantics: [Gio.Application](https://docs.gtk.org/gio/class.Application.html)
- Alternatives: [egui](https://docs.rs/egui/latest/egui/), [egui-winit features](https://docs.rs/egui-winit/), [Slint accessibility](https://docs.slint.dev/latest/docs/slint/guide/development/best-practices/accessibility/), [Slint Rust features](https://docs.slint.dev/latest/docs/rust/slint/docs/cargo_features/), [Iced docs](https://docs.iced.rs/), [Iced accessibility issue](https://github.com/iced-rs/iced/issues/552)
- PipeWire/WirePlumber policy: [WirePlumber linking policy](https://pipewire.pages.freedesktop.org/wireplumber/policies/linking.html), [settings and `target.object`](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/settings.html), [pipewire-rs keys](https://pipewire.pages.freedesktop.org/pipewire-rs/pipewire/keys/index.html)
- Desktop contracts: [XDG base directories](https://specifications.freedesktop.org/basedir-spec/latest/), [XDG autostart](https://specifications.freedesktop.org/autostart-spec/latest/), [StatusNotifierItem](https://specifications.freedesktop.org/status-notifier-item/latest-single/)
- Reproducible Rust builds: [Cargo build flags](https://doc.rust-lang.org/cargo/commands/cargo-build.html), [Cargo vendor](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html), [Cargo native build scripts](https://doc.rust-lang.org/cargo/reference/build-script-examples.html)
- Optional Lua research: [Lua 5.4 manual](https://www.lua.org/manual/5.4/manual.html), [mlua README](https://github.com/mlua-rs/mlua/blob/main/README.md), [mlua `Lua` API](https://docs.rs/mlua/latest/mlua/struct.Lua.html)
- License automation: [cargo-deny](https://github.com/EmbarkStudios/cargo-deny), [cargo-about](https://github.com/EmbarkStudios/cargo-about)

## Research constraints and confidence

No dependencies were installed, no build/test/GUI/audio command was run, no host setting changed, and no code/version/release file in the cloned repository was modified. Findings about current behavior come from source and documentation; they are intentionally labeled as unverified where runtime evidence is absent. There were no access blockers.
