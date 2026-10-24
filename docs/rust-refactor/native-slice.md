# First native connectivity slice (WIP)

This implements the foundation/read-only portion of the [GTK proposal](./pecanmixer-issue-1-native-gtk-proposal.md), on top of the research snapshot `923c7392850dcd335eac8cdde308dd08b77a63fa`. It is related to issue #1; it does not complete the refactor. All application/crate versions remain `0.1.0`.

## Implemented boundary

- A root Cargo workspace and lockfile contain `pecan-core`, `pecan-pipewire`, `pecan-ui-gtk`, and the existing `src-tauri` package. Every pre-existing locked package version is retained.
- `pecan-core` owns the existing graph/transport/recording-option/MIDI DTOs, with the same serialized Dutch field names. Node status is now an owned `String`. The crate has no GTK, Tauri or PipeWire dependency.
- The existing graph, meter, MIDI meter and recorder implementations move into `pecan-pipewire`, without a second backend copy. Legacy module re-exports preserve imports used by the tray and existing examples.
- A temporary Tauri adapter preserves all 13 commands, all four event names, raw JSON payloads, volume clamping and tray updates. Existing JS/TS, web preferences, npm tooling and the production shell remain in place.
- A distinct, non-installed `nl.jetse.pecanmixer.GtkPreview` application uses direct gtk4-rs. GTK objects stay on its main thread. It receives owned events through replaceable graph/peak/MIDI/transport slots, a bounded ordered notice queue with explicit overflow, and one async wake token. There is no Tauri, WebKit, JavaScriptCore, libadwaita, Relm4 or Lua dependency in this binary's Cargo tree.
- The preview displays connection state, rate, quantum and a stable ordered node/port list. The same pure projection/reducer powers synthetic fixture mode and display-free JSON output. Disconnects clear the view; epochs reject stale graph/frame replay. Structural widgets change only when the overview changes.
- Live preview startup is observational: it skips the legacy Rec bus, meter streams, MIDI capture and command execution. Dropping its observer requests cancellation; connected sessions check at the existing 40 ms tick and reconnect waits can be interrupted. An off-UI `stop_and_join` method is available. The legacy app's recording/startup behavior is preserved.

## Build and run

Use a current stable Rust toolchain, a C compiler/linker, `pkg-config`, libclang with its C headers, and GTK4/PipeWire development headers and libraries. On Debian-family systems the native packages include `libgtk-4-dev`, `libpipewire-0.3-dev`, `libclang-dev` and `clang`. GTK4's base 4.0 API is selected, with higher GTK version features disabled. This is an API floor, not proof of compatibility on every distribution. The locked gtk4-rs 0.11.5 / GLib 0.22 generation declares Rust 1.92; a minimum-toolchain build has not been run. No libadwaita is required. Live operation still targets Linux/PipeWire; WirePlumber remains policy owner.

```sh
# Core-only, no display or native GTK/PipeWire development dependencies:
cargo test -p pecan-core --locked

# Shared backend unit tests and GTK build:
cargo test -p pecan-pipewire --locked
cargo build -p pecan-ui-gtk --locked

# Native fixture window, display-free projection, and live read-only observer:
cargo run -p pecan-ui-gtk --locked -- --fixture fixtures/graph/audio-basic.json
cargo run -p pecan-ui-gtk --locked -- --fixture fixtures/graph/audio-basic.json --headless
cargo run -p pecan-ui-gtk --locked

# Xvfb fixture widget/binding smoke; requires Xvfb and a session bus:
xvfb-run -a dbus-run-session -- target/debug/pecan-ui-gtk --fixture fixtures/graph/audio-basic.json --smoke

# Temporary legacy compatibility checks (also need GTK3/WebKitGTK development packages):
cargo test -p pecanmixer --lib --locked
cargo check -p pecanmixer --examples --locked
```

`--smoke` quits after populating the GTK widget tree; it does not establish completed frame rendering, physical desktop acceptance or accessibility. Fixtures are synthetic, documented in [fixtures/README.md](../../fixtures/README.md), and are not recordings of hardware. The existing npm/Tauri build remains the production workflow. The root lockfile replaces `src-tauri/Cargo.lock`; Cargo output now uses the root `target/` directory.

## Cloud validation (2026-10-24)

The cloud started without Rust and native development pkg-config files. Standard Rust tooling and Debian GTK/PipeWire/legacy build dependencies were downloaded into isolated temporary directories, without modifying system packages or a user machine. Tested: Rust 1.99.1, GTK 4.18.6, PipeWire headers/library 1.4.2, GTK3 3.24.49, WebKitGTK 2.52.6. These versions describe this build environment; no supported-distro matrix is established.

| Check | Result |
|---|---|
| `cargo test -p pecan-core -p pecan-pipewire --locked` | PASS: 6 core + 6 backend tests; Node/npm absent from `PATH` |
| `cargo test -p pecanmixer --lib --locked` | PASS: 4 tests (3 IPC payload contracts + existing autostart test) |
| `cargo check -p pecanmixer --examples --locked` | PASS: all 5 existing examples compile |
| `cargo build -p pecan-ui-gtk --locked` | PASS; also checked with Node/npm absent from `PATH` |
| Native `--fixture audio-basic.json --headless` | PASS: JSON equals committed expected overview |
| Xvfb + session-bus `--smoke`, audio-basic/disconnected/reconnect-after | PASS: GTK widgets populated for 3 nodes/6 ports, 0/0, 3/6 respectively; no frame/physical-desktop claim |
| Native Cargo tree and `ldd` | PASS: GTK4 present; no Tauri/Wry/WebKit/JavaScriptCore/serialize-to-JavaScript tree path or web-engine linked library |
| Legacy API/static dependency/version checks | PASS: all 13 command bodies/registrations unchanged; all pre-existing locked package versions retained; all application/crate versions `0.1.0` |
| Formatting of new crates and changed legacy Rust files, `git diff --check`, fixture JSON and touched documentation links | PASS |
| Independent scoped source reviews | PASS after correcting the smoke message from “rendered” to “widgets populated”; no outstanding concrete finding |

No final check failed. An initial backend build failed because the cloud's partial libclang setup lacked discoverable `stdbool.h`; adding the standard compiler include directories resolved it. Rustfmt was initially absent and formatting differences were corrected after installing the standard component. Xvfb runs emitted non-fatal font/shader-cache warnings because the sandbox home cache was read-only; these checks do not validate fonts, GPU rendering or desktop appearance. No dependency/environment blocker remains for the checks above. The unrun acceptance work is listed below.

## Deliberately unfinished

This slice is narrower than the proposal's complete first-PR wish list. In particular, `Backend`/stamped-command validation is a core contract exercised with a fake backend; production legacy mutation dispatch is still unstamped. The native preview exposes no mutation API, so it cannot replay old object IDs into a new connection. Routing/volume/recording commands must acquire worker-side epoch validation before exposing them in GTK.

Full recorder shutdown/finalization acknowledgement, writer joining on app quit, domain-policy ports, mixer/patchbay/faders/meters, settings migration, tray/autostart/folder actions and application parity are unfinished. MIDI remains disabled. The legacy fixed-delay tray quit behavior is unchanged. A synthetic cancellation test is not evidence of recorder finalization or live server recovery.

No real PipeWire/WirePlumber session/restart, hardware/audio recording, real-time/latency/CPU/memory benchmark, Wayland session, keyboard/screen-reader acceptance, physical desktop regression, release build, packaging/distribution or final license audit has been performed. The full workspace still contains JavaScript/Node/Tauri/WebKit intentionally. Remove that stack only after parity and cutover acceptance; no merge, version bump or release is part of this slice.
