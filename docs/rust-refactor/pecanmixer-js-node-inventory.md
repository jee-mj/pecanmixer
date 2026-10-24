# Pecan Mixer: JavaScript and Node.js removal inventory

Public fork: https://github.com/jee-mj/pecanmixer · verified HEAD `d8d473818331342ab53a0e1e4d3ced7977293449` · inspection date 2026-10-24 (UTC).

## What JavaScript actually does

The whole visible mixer application is React/TypeScript compiled to JavaScript: layout, controls, routing policy/model calculations, recording configuration, undo/redo, persistence, animated meters, MIDI presentation and browser demo simulation. Rust owns the PipeWire connection, actual links/properties, sample processing, peak acquisition, WAV/MIDI writing, tray and autostart. Removing the npm toolchain alone would leave the webview JavaScript application intact. Keeping Rust Tauri does **not** automatically eliminate JavaScript: its webview UI and IPC infrastructure must also be addressed for a literal zero-JavaScript goal.

Node.js is used by Vite, TypeScript, React transformation and the npm Tauri CLI workflow during development/build/preview. No Node server, Electron runtime, Node audio process, Web Audio API or Web MIDI API is present in the first-party application source. Production application JavaScript executes in the system webview; it is not evidence of a shipped Node runtime. Exact release payloads were not inspected.

## Verification and boundaries

- Fresh isolated clone in `source/`; `git ls-remote` independently returned `d8d473818331342ab53a0e1e4d3ced7977293449`; clone HEAD matches. Initial and final working tree checks are clean. All 66 tracked paths are enumerated with byte counts and SHA-256 in the JSON companion.
- Prior research revision `57741f75eba111578b4d02566ff12ac5fa4400cd` differs only by deletion of `CLAUDE.md` (191 lines). The current tree has no AGENTS.md or CLAUDE.md. No stale deleted instructions were applied.
- Static inspection of all frontend modules, configs, manifests/locks, Rust command/event boundary, Rust module responsibilities and design HTML. No dependencies installed; no build, test, GUI or audio operation run; no source edits, version changes or publishing.
- Scope is this public repository only. Committed source is authoritative; package-lock records are possible dependency resolution, not proof of installation or byte inclusion. Cargo.lock is cross-platform resolution, not an active target feature graph.

## First-party module inventory

18 TS/TSX/declaration files under src, 4114 lines (including comments and blanks), plus Vite TypeScript configuration. All are listed below. Symbols, hooks, imports and hashes are in JSON.

| File | Purpose / behavior | Phase / status | Removal requirement |
|---|---|---|---|
| `src/App.tsx` (430 lines) | Root application orchestration: graph/transport/meter subscriptions, tab/view selection, routing destination selection, hidden strips/rows, confirmation UI, keyboard undo/redo, recording bus and clock synchronization, persistent preferences. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/Meter.tsx` (41 lines) | React meter DOM elements and effect/ref registration with shared animation engine. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/MidiMeter.tsx` (75 lines) | MIDI activity cards/port rows: 16 channels, message categories, latest message, events/sec. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/MidiMixer.tsx` (197 lines) | MIDI graph-to-source/destination model and routing cards, selection/hide and record-arm UI; navigation disabled by MIDI_AAN=false. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/Mixer.tsx` (882 lines) | Audio mixer UI: source/destination/recording cards; route toggles, custom route navigation, fader law and dB formatting, pointer/wheel/key volume changes, mute, bus arm/add/remove/split/join, per-bus/all recording, folder action; menus, hide rows, resize row weights and destination/Rec split. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/Patchbay.tsx` (355 lines) | Interactive audio/MIDI patchbay: module layout and dragging, DOM port hit testing, SVG cables, connect/disconnect/move cables, selection/Delete/Escape, mini meters, rearrangement positions. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/Transport.tsx` (111 lines) | MIDI clock/source/start-stop controls (feature gated); recording elapsed time and error display via Looptijd, 100ms refresh. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/api.ts` (538 lines) | Typed graph/recording protocol, 13 Rust IPC calls and graph/transport listeners; latest graph cache; browser-only mock graph, WirePlumber-style routing simulation, fake recording clock, optional ?graaf= JSON fetch. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/bussen.ts` (203 lines) | Persistent recording bus configuration and legacy migration, stereo/mono definitions, arming, dB-to-gain conversion, channel remapping and rerouting when a bus is removed. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/geschiedenis.ts` (111 lines) | Session-only undo/redo closures (200 steps), serialized 250ms replay queue, useSyncExternalStore history hook. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/handelingen.ts` (114 lines) | Undoable connect/disconnect/move, audio route toggle/reset, volume and mute, MIDI route operations; captures graph state and inverse operations. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/iconen.tsx` (76 lines) | Inline SVG icon paths/color tiles rendered through React. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/indeling.ts` (339 lines) | Graph-to-patchbay presentation model: app/hardware grouping, split audio channels, port labels, hidden nodes, MIDI device identification, icon selection. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/main.tsx` (12 lines) | ReactDOM createRoot / StrictMode boot, app mount, IBM Plex Mono 400/500 CSS and app stylesheet imports. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/meters.ts` (132 lines) | Peak event cache; requestAnimationFrame updates DOM clipPath/peak position/classes without React redraw, 24dB/s fall, 1.5s hold, 2s clip; browser mock peaks every 40ms. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/midi.ts` (127 lines) | MIDI event cache/bitmask decoding, 150ms activity afterglow, 1s event rate, destination aggregation, useTik hook every 66ms, browser simulated MIDI every 40ms. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/mixer.ts` (370 lines) | Graph-to-mixer strips/routes: source/destination and recording bus grouping, automatic/manual/custom classification, channel matching, target.object versus extra links, return-to-auto operations, dB/linear conversion. | webview runtime; Vite build transform; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |
| `src/vite-env.d.ts` (1 lines) | Vite/client ambient TypeScript declarations; compile-time only. | typecheck; first-party | Replace this behavior and data contract in chosen non-JS frontend, or explicitly retire feature; remove original module. |

## State, rendering and audio interaction details

- React hooks used: useState, useEffect, useMemo, useRef, useLayoutEffect and useSyncExternalStore. ReactDOM mounts at index.html #root. HTML, CSS and inline SVG provide rendering; patch cables use SVG and pointer geometry. Meter animation directly mutates DOM styles/classes.
- Persistent localStorage keys: `pecanmixer.verborgen` (hidden strips/rows), `pecanmixer.klok` (clock choices; source ID reset per session), `pecanmixer.bussen` (bus layout/gain/mute/arming plus legacy migration), `pecanmixer.rijen` (row weights), `pecanmixer.deling` (destination split). A replacement needs a migration/retirement decision for each. Graph IDs, selection, dragged module positions, meter caches and undo closures are session state.
- JavaScript computes which ports to connect, when to set target.object instead, route classifications, inverse operations, bus channel/gain configuration and remapping. Actual PipeWire and audio sample work happens in Rust. These policy functions are functional behavior to preserve, not merely replaceable paint.
- MIDI code remains authored and imported; audio app subscribes to MIDI even though MIDI navigation/transport controls are gated by `MIDI_AAN=false` and backend `graaf::MIDI=false`. Static inventory includes dormant code; no claim all of it survives optimized bundling.
- Browser preview uses mock graph and simulated level/MIDI/recording data. Optional `?graaf=` fetch loads a graph snapshot; this is distinct from the real IPC path. Removing JS also removes this preview unless replaced.

## Tauri IPC versus Rust backend

All 13 commands below are invoked from src/api.ts, registered with generate_handler! in src-tauri/src/lib.rs. Replacement must expose equivalent typed operations or retire the relevant feature. Events carry serialized graph/status/levels, not an audio playback engine implemented in JS.

| Commands | Purpose | Phase / ownership |
|---|---|---|
| `haal_graaf` | Initial graph snapshot | JS invoke → Rust state |
| `verbind`, `ontkoppel` | Create/remove PipeWire links | JS policy → Rust command queue |
| `zet_doel`, `zet_volume`, `zet_mute` | Stream routing target and controls | JS UI/model → Rust properties |
| `zet_transport`, `zet_bus` | Armed takes/clock and channel gains | JS configuration → Rust recorder |
| `start_opname`, `stop_opname`, `start_bus`, `stop_bus` | All/per-bus recording lifecycle | JS controls → Rust capture/writer |
| `open_map` | Open recordings folder | JS action → Rust xdg-open |

Events: `graaf` → api.ts/App; `transport` → api.ts/App/recording UI; `pieken` → meters.ts; `midi` → midi.ts. Listeners use @tauri-apps/api/event and cleanup callbacks. Rust lib.rs forwards Graaf/Melding events with Emitter.

Rust responsibility map: `graaf.rs` graph discovery/reconnect/command execution/recording planning; `meter.rs` sample peak measurement; `midimeter.rs` MIDI parsing/activity/capture (feature gated); `opname.rs` recording bus DSP, gain, WAV writer, MIDI timing/file writing and folder launch; `tray.rs` native D-Bus tray/window/recording menu; `autostart.rs` desktop startup file; `main.rs`/`lib.rs` application lifecycle. Five src-tauri/examples/*.rs are Rust probes, including JSON graph snapshot generation, not Node scripts. These are first-party native runtime/dev functions; retain/adapt independently from removing the frontend.

## Web assets and design artifacts

| Files | Use / phase | Removal or replacement |
|---|---|---|
| index.html | Runtime mount point and module entry /src/main.tsx | Replace web shell if eliminating webview |
| src/styles.css | App layout, colors, controls, meter styling | Recreate styling in replacement; CSS is not JS |
| @fontsource/ibm-plex-mono/400.css and 500.css | main.tsx imports self-hosted font CSS/assets through Vite | Fonts can be retained without npm; actual selected font bytes require bundle inspection |
| src/iconen.tsx and inline SVG in UI | Executable React-rendered icons/controls | Extract/redraw native/vector equivalents |
| app-icon.svg; src-tauri/icons/* (PNG/SVG) | Application/tray assets; tray PNGs included by Rust | Assets themselves contain no JS dependency; retain as needed |
| docs/screenshots/*.png (5) | README documentation images | No JS execution; may remain |
| docs/ontwerp/project/{Main,MidiMixer,MidiPatchbay,Patchbay}.dc.html (4) | Design HTML/CSS, Google Fonts Mono/Sans links, ./support.js reference and text/x-dc DCLogic component scripts | Not referenced by app build entry. Archive or replace scripts for repository-wide zero JS. support.js is missing from tracked tree; cannot inventory its implementation |

## Node/npm development, build and packaging

| File / command | Purpose and phase | Replacement requirement |
|---|---|---|
| package.json: dev = vite | Node development server, source transforms and HMR | Replace development workflow |
| build = tsc && vite build | Type checks then bundles web assets to dist | Replace checking/build and dist consumption |
| preview = vite preview | Node local built-site preview server | Retire or replace preview |
| tauri = tauri | npm CLI launcher for native dev/build/bundling | Replace launcher; direct Cargo alone still needs frontend decision |
| vite.config.ts | Node process.env.TAURI_DEV_HOST; React plugin, port 1420, optional HMR websocket 1421, excludes src-tauri watching | Remove Vite/HMR integration |
| tsconfig.json, tsconfig.node.json, src/vite-env.d.ts | Strict ES2020/DOM, ESNext bundler resolution, React JSX, noEmit app checking; Vite config project | Remove TS config with TS consumers |
| src-tauri/tauri.conf.json | beforeDevCommand npm run dev; devUrl localhost:1420; beforeBuildCommand npm run build; frontendDist ../dist; deb/rpm/appimage targets | Replace hooks/asset source and packaging assumptions |
| src-tauri/build.rs; Cargo tauri-build + macros/codegen | Rust build-time app context, config/assets/capability integration | Audit generated/embedded web/IPC code when removing JS |
| src-tauri/capabilities/default.json | Main webview core:default permissions; generated schema reference | Replace IPC/capability integration if frontend changes |
| README.md | Documents npm install and npm run tauri dev, requires Node.js | Update removal completion instructions |
| .gitignore | Excludes node_modules, dist, src-tauri/target and gen | Retire obsolete exclusions after workflow change |

No committed CI workflows, release automation, shell packaging scripts, JS test files, npm test/lint scripts, custom codegen scripts, alternate JS lockfiles or vendored node_modules/dist are in the tracked manifest. Standard Vite/Tauri generation exists; no application-specific generated TS binding step is declared. Hosted releases/actions/settings outside Git were not inventoried.

## Exact npm dependency inventory

Lockfile version 3; SHA-256 `1683c9f1438bf34daebd8ceef1a3251b5b2f44f7b88109a8c9b8563572d3aed4`. 133 package records excluding root; 5 production-closure and 128 development records; 64 optional platform records. Ranges are not locked versions. All per-record purposes, declared consumers, resolved URLs, integrity hashes, edges, peers and engine constraints are preserved in JSON.

Direct production roles: react = component/hooks runtime; react-dom = DOM renderer (with transitive scheduler); @tauri-apps/api = invoke/event bridge; @fontsource/ibm-plex-mono = font assets. Direct development roles: @tauri-apps/cli = native workflow launcher; @types/react and @types/react-dom = type declarations; @vitejs/plugin-react = React JSX development transform/refresh; typescript = checker/compiler; vite = dev server/bundler.

Transitive development roles: @babel/* and @types/babel__* parse/transform JSX and support plugin transforms; @jridgewell/*/source-map-js/convert-source-map handle source maps; react-refresh supports HMR; rollup/@rollup/* bundle and platform executables; esbuild/@esbuild/* transform with native platform executables; postcss transforms CSS; browserslist/caniuse-lite/baseline-browser-mapping/node-releases/electron-to-chromium/update-browserslist-db supply target metadata; fdir/tinyglobby/picomatch/fsevents handle scanning/watching; @tauri-apps/cli-* and @napi-rs/lzma-linux-x64-gnu are native tooling platform packages. Other small packages provide parser/token/string, logging, color, version, cache and utility support. Platform entries are alternatives, not all loaded on one host. Every record below requires removal with its consumer; no one-for-one native replacement for implementation-only build utilities is implied.

| Package (lock path in JSON) | Locked | Direct range | Status | Phase | Optional |
|---|---|---|---|---|---|
| @babel/code-frame | 7.29.7 | — | transitive-development | build/development | False |
| @babel/compat-data | 7.29.7 | — | transitive-development | build/development | False |
| @babel/core | 7.29.7 | — | transitive-development | build/development | False |
| @babel/generator | 7.29.8 | — | transitive-development | build/development | False |
| @babel/helper-compilation-targets | 7.29.7 | — | transitive-development | build/development | False |
| @babel/helper-globals | 7.29.7 | — | transitive-development | build/development | False |
| @babel/helper-module-imports | 7.29.7 | — | transitive-development | build/development | False |
| @babel/helper-module-transforms | 7.29.7 | — | transitive-development | build/development | False |
| @babel/helper-plugin-utils | 7.29.7 | — | transitive-development | build/development | False |
| @babel/helper-string-parser | 7.29.7 | — | transitive-development | build/development | False |
| @babel/helper-validator-identifier | 7.29.7 | — | transitive-development | build/development | False |
| @babel/helper-validator-option | 7.29.7 | — | transitive-development | build/development | False |
| @babel/helpers | 7.29.10 | — | transitive-development | build/development | False |
| @babel/parser | 7.29.9 | — | transitive-development | build/development | False |
| @babel/plugin-transform-react-jsx-self | 7.29.7 | — | transitive-development | build/development | False |
| @babel/plugin-transform-react-jsx-source | 7.29.7 | — | transitive-development | build/development | False |
| @babel/template | 7.29.7 | — | transitive-development | build/development | False |
| @babel/traverse | 7.29.10 | — | transitive-development | build/development | False |
| @babel/types | 7.29.8 | — | transitive-development | build/development | False |
| @esbuild/aix-ppc64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/android-arm | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/android-arm64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/android-x64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/darwin-arm64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/darwin-x64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/freebsd-arm64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/freebsd-x64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-arm | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-arm64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-ia32 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-loong64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-mips64el | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-ppc64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-riscv64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-s390x | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/linux-x64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/netbsd-arm64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/netbsd-x64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/openbsd-arm64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/openbsd-x64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/openharmony-arm64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/sunos-x64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/win32-arm64 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/win32-ia32 | 0.28.2 | — | transitive-development | build/development | True |
| @esbuild/win32-x64 | 0.28.2 | — | transitive-development | build/development | True |
| @fontsource/ibm-plex-mono | 5.3.0 | ^5.3.0 | direct-production | web asset | False |
| @jridgewell/gen-mapping | 0.3.13 | — | transitive-development | build/development | False |
| @jridgewell/remapping | 2.3.5 | — | transitive-development | build/development | False |
| @jridgewell/resolve-uri | 3.1.2 | — | transitive-development | build/development | False |
| @jridgewell/sourcemap-codec | 1.6.0 | — | transitive-development | build/development | False |
| @jridgewell/trace-mapping | 0.3.31 | — | transitive-development | build/development | False |
| @napi-rs/lzma-linux-x64-gnu | 1.5.1 | — | transitive-development | build/development | True |
| @rolldown/pluginutils | 1.0.0-beta.27 | — | transitive-development | build/development | False |
| @rollup/rollup-android-arm-eabi | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-android-arm64 | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-darwin-arm64 | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-darwin-x64 | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-freebsd-arm64 | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-freebsd-x64 | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-arm-gnueabihf | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-arm-musleabihf | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-arm64-gnu | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-arm64-musl | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-loong64-gnu | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-loong64-musl | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-ppc64-gnu | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-ppc64-musl | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-riscv64-gnu | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-riscv64-musl | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-s390x-gnu | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-x64-gnu | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-linux-x64-musl | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-openbsd-x64 | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-openharmony-arm64 | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-win32-arm64-msvc | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-win32-ia32-msvc | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-win32-x64-gnu | 4.64.9 | — | transitive-development | build/development | True |
| @rollup/rollup-win32-x64-msvc | 4.64.9 | — | transitive-development | build/development | True |
| @tauri-apps/api | 2.12.4 | ^2 | direct-production | webview runtime | False |
| @tauri-apps/cli | 2.12.3 | ^2 | direct-development | build/development | False |
| @tauri-apps/cli-darwin-arm64 | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-darwin-x64 | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-linux-arm-gnueabihf | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-linux-arm64-gnu | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-linux-arm64-musl | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-linux-riscv64-gnu | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-linux-x64-gnu | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-linux-x64-musl | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-win32-arm64-msvc | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-win32-ia32-msvc | 2.12.3 | — | transitive-development | build/development | True |
| @tauri-apps/cli-win32-x64-msvc | 2.12.3 | — | transitive-development | build/development | True |
| @types/babel__core | 7.20.5 | — | transitive-development | build/development | False |
| @types/babel__generator | 7.27.0 | — | transitive-development | build/development | False |
| @types/babel__template | 7.4.4 | — | transitive-development | build/development | False |
| @types/babel__traverse | 7.28.0 | — | transitive-development | build/development | False |
| @types/estree | 1.0.9 | — | transitive-development | build/development | False |
| @types/react | 19.3.0 | ^19.1.8 | direct-development | build/development | False |
| @types/react-dom | 19.3.0 | ^19.1.6 | direct-development | build/development | False |
| @vitejs/plugin-react | 4.7.0 | ^4.6.0 | direct-development | build/development | False |
| baseline-browser-mapping | 2.11.29 | — | transitive-development | build/development | False |
| browserslist | 4.29.3 | — | transitive-development | build/development | False |
| caniuse-lite | 1.0.30001816 | — | transitive-development | build/development | False |
| convert-source-map | 2.0.0 | — | transitive-development | build/development | False |
| csstype | 3.2.3 | — | transitive-development | build/development | False |
| debug | 4.4.3 | — | transitive-development | build/development | False |
| electron-to-chromium | 1.5.459 | — | transitive-development | build/development | False |
| esbuild | 0.28.2 | — | transitive-development | build/development | False |
| escalade | 3.2.0 | — | transitive-development | build/development | False |
| fdir | 6.5.0 | — | transitive-development | build/development | False |
| fsevents | 2.3.3 | — | transitive-development | build/development | True |
| gensync | 1.0.0-beta.2 | — | transitive-development | build/development | False |
| js-tokens | 4.0.0 | — | transitive-development | build/development | False |
| jsesc | 3.1.0 | — | transitive-development | build/development | False |
| json5 | 2.2.3 | — | transitive-development | build/development | False |
| lru-cache | 5.1.1 | — | transitive-development | build/development | False |
| ms | 2.1.3 | — | transitive-development | build/development | False |
| nanoid | 3.3.20 | — | transitive-development | build/development | False |
| node-releases | 2.0.59 | — | transitive-development | build/development | False |
| picocolors | 1.1.1 | — | transitive-development | build/development | False |
| picomatch | 4.0.7 | — | transitive-development | build/development | False |
| postcss | 8.5.30 | — | transitive-development | build/development | False |
| react | 19.3.1 | ^19.1.0 | direct-production | webview runtime | False |
| react-dom | 19.3.1 | ^19.1.0 | direct-production | webview runtime | False |
| react-refresh | 0.17.0 | — | transitive-development | build/development | False |
| rollup | 4.64.9 | — | transitive-development | build/development | False |
| scheduler | 0.28.0 | — | transitive-production | webview runtime | False |
| semver | 6.3.1 | — | transitive-development | build/development | False |
| source-map-js | 1.2.2 | — | transitive-development | build/development | False |
| tinyglobby | 0.2.17 | — | transitive-development | build/development | False |
| typescript | 5.8.3 | ~5.8.3 | direct-development | build/development | False |
| update-browserslist-db | 1.3.4 | — | transitive-development | build/development | False |
| vite | 7.3.7 | ^7.0.4 | direct-development | build/development | False |
| yallist | 3.1.1 | — | transitive-development | build/development | False |

## JavaScript entering through Rust dependencies

Cargo.lock identifies the following webview/JavaScript-related chain. All are transitive except tauri, tauri-build (build dependency), and tauri-plugin-single-instance. Lock presence does not establish active features or exact injected JS bytes.

| Crate | Locked version | Role / phase and removal requirement |
|---|---|---|
| javascriptcore-rs | 1.1.2 | Linux WebKitGTK/JavaScriptCore bindings; system webview execution, not Node. Remove webview requirement for engine-free UI. |
| javascriptcore-rs-sys | 1.1.1 | Linux WebKitGTK/JavaScriptCore bindings; system webview execution, not Node. Remove webview requirement for engine-free UI. |
| serialize-to-javascript | 0.1.2 | Serialize Rust data into JavaScript code (build/runtime bridge support). Remove consumer path with IPC replacement. |
| serialize-to-javascript-impl | 0.1.2 | Serialize Rust data into JavaScript code (build/runtime bridge support). Remove consumer path with IPC replacement. |
| tauri | 2.12.4 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| tauri-build | 2.7.3 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| tauri-codegen | 2.7.3 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| tauri-macros | 2.7.3 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| tauri-plugin-single-instance | 2.5.4 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| tauri-runtime | 2.12.2 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| tauri-runtime-wry | 2.12.2 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| tauri-utils | 2.10.2 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| tauri-winres | 0.3.6 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |
| webkit2gtk | 2.0.2 | Linux WebKitGTK/JavaScriptCore bindings; system webview execution, not Node. Remove webview requirement for engine-free UI. |
| webkit2gtk-sys | 2.0.2 | Linux WebKitGTK/JavaScriptCore bindings; system webview execution, not Node. Remove webview requirement for engine-free UI. |
| webview2-com | 0.39.1 | Windows webview bindings in cross-platform lock; not proof shipped on Linux. Remove unused platform closure or replace webview path. |
| webview2-com-macros | 0.8.1 | Windows webview bindings in cross-platform lock; not proof shipped on Linux. Remove unused platform closure or replace webview path. |
| webview2-com-sys | 0.39.1 | Windows webview bindings in cross-platform lock; not proof shipped on Linux. Remove unused platform closure or replace webview path. |
| wry | 0.57.0 | Tauri webview/IPC/assets infrastructure; build and/or runtime. Audit or replace the framework path for literal zero JS. |

Tauri commonly supplies initialization/IPC/event JavaScript through its Rust framework, and its codegen incorporates frontend assets. This source confirms use of those framework paths and dependency names, but dependency source archives and generated outputs were not downloaded; exact framework script bodies, byte counts, plugin injection and target-selected crate features remain unknown. No direct first-party eval/injected JavaScript string was found in Rust. Removing @tauri-apps/api alone does not prove these framework paths disappear.

## Largest removal areas and review checklist

1. Replace the complete React/DOM interface: mixer, patchbay gestures, geometry/SVG, faders, meters, keyboard commands, recording controls and persistent preferences.
2. Preserve or relocate frontend policy/state: graph presentation, route classification and port matching, target.object decisions, bus remapping, gain math, undo/redo and synchronization.
3. Replace the invoke/event contract and decide whether any webview/JavaScriptCore/Tauri injected code remains acceptable. A native Rust UI or Rust/Lua split is an option to assess later, not a chosen architecture here.
4. Remove npm/Vite/TypeScript/Babel/Rollup/esbuild and Tauri npm hooks only after equivalent build/package workflow exists; preserve fonts/icons through a non-npm asset path if desired.
5. Include dormant MIDI and four design prototypes in the removal scope; decide whether browser demo behavior and stored preferences must survive.

## Explicit unknowns / limits

- No executable build/test verification, live PipeWire behavior, GUI rendering, performance measurements or device validation; MIDI is explicitly untested/disabled in source.
- No installed dependency tree, dependency archive source inspection, generated schemas, optimized bundle/module tree shaking, artifact size, or packaged release contents. Locked native platform packages and cross-platform Cargo crates are not runtime inclusion proof.
- No pinned Node/npm version in repository; Vite/esbuild engine ranges are copied from lock to JSON, not independently tested. System webview/JavaScriptCore versions are not pinned by Cargo.lock.
- Missing design support.js behavior and remote font response contents are unverified. No source claim of repository-wide complete JS elimination can be made without addressing them.
- No CI/release service state outside committed tree inspected. Revision is a point-in-time remote HEAD observation, not a guarantee the branch will remain unchanged.

## Design-script detail and final verification

`Main.dc.html` contains a DCLogic interactive audio design: selected destination, simulated route toggles/reset confirmation, recording/clock state and meter/fader presentation. `MidiMixer.dc.html` contains a DCLogic interactive MIDI design: selected destination, route toggles/clear confirmation, simulated recording, channel/type aggregation and fixed demonstration data. `Patchbay.dc.html` and `MidiPatchbay.dc.html` each contain a minimal DCLogic class whose renderVals returns an empty object, alongside static design markup. These are documentation/prototype behavior, separate from the React runtime. All four reference absent support.js.

Final remote HEAD recheck still returned `d8d473818331342ab53a0e1e4d3ced7977293449`; source git status remained empty. JSON parsed successfully, all 18 frontend files have purpose/removal mappings, all 133 npm records have per-record purposes and lock evidence, and the 13 JS command names match Rust command declarations. No remaining access blocker; the unbuilt/generated/dependency-source limits above remain.
