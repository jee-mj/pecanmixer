# Additive native CI and executable validation

This candidate adds `.github/workflows/native.yml` and
`scripts/native/{check,test-check}.sh` on top of `20e9f90c`. It preserves the
legacy frontend, Tauri member, lockfile, versions, identities and supported
legacy commands. No package, release, tag, installer or autostart entry is created.

## Run the same check locally and in CI

On a Linux checkout with current stable Rust (locked gtk4-rs declares Rust 1.92),
`rustfmt`, a C compiler, `pkg-config`, clang/libclang and GTK4/PipeWire/SPA
headers, install Xvfb/xauth, D-Bus and XTest libraries for the synthetic GUI suite:

```sh
# Dependency/license metadata and source-asset hashes, with no build or GUI:
bash scripts/native/check.sh audit

# Core/backend tests, native build, isolated GTK tests and fixture CLI checks:
bash scripts/native/check.sh

# Small validator negative/positive controls, requiring no compiler or display:
bash scripts/native/test-check.sh
```

The workflow uses one Ubuntu 24.04 Linux host, two Cargo build jobs, the public
repository's exact event SHA and `--locked` Cargo commands. It installs only the
native development/test packages named in the workflow. Anonymous Git checkout
and rustup shell commands avoid Node-based checkout/setup/upload actions; no
new secret is required. This public-repository checkout is not a private-repo
credential mechanism. Push/PR/manual triggers perform validation only, with
read-only contents permission and a 30-minute job limit. There is no publishing
step or release trigger. Rust commands have independent 20-minute limits and
fixture executable invocations have 15-second limits.

## What is checked

- The Linux host is explicit for Cargo graph inspection, build and tests. The
  inspected native executable is that host's artifact, even if a caller has a
  separate Cargo target directory or inherited cross-target configuration.
- Both native normal/build dependencies and the three native crates' test
  dependencies reject known Tauri/Wry/WebKit/JavaScript engines/bindings and GTK3.
  This does not reject Rust JSON serialization or frozen JSON reference data.
- PATH guards make attempted Node/npm/npx/pnpm/yarn/corepack/bun/deno use fail and
  record command names only. An ignored failure still fails the final check.
  No npm manifest, frontend build or optional Node oracle generator is executed.
- Native ELF direct dependencies are recorded with `readelf`; the trusted built
  executable's transitive loader closure is inspected with `ldd`. Known web/JS
  libraries, GTK3, missing libraries or missing GTK4 evidence fail validation.
- The core/backend suite runs without a display/session bus. GTK accessibility,
  XTest and tray fixtures run on a newly allocated Xvfb and private D-Bus session,
  with the explicit safety markers required by the existing tests.
- Application config/data/cache/runtime directories are temporary. The PipeWire
  remote name deliberately has no service. No PipeWire server is started.
- Executable checks use only `--help` and explicit committed fixture paths:
  `fixtures/graph/{audio-basic,disconnected,reconnect-after}.json` in headless
  mode; connected mixer/patchbay and hidden disconnected `--smoke` modes. Smoke
  uses controlled Quit. No bare native startup, settings import, real tray,
  recording, autostart installation or audio example is launched.
- Core tests compare semantic fixture JSON. The CLI also retains the reviewed
  pretty-output comparison against `fixtures/expected/audio-basic-overview.json`.
  Fader/partial-mute tests consume committed `oracle.json` files directly.

Cargo documents `normal,build` as an approximation of the compiled graph, not
formal equivalence; see [cargo-tree](https://doc.rust-lang.org/cargo/commands/cargo-tree.html).
ELF inspection corroborates the built host executable. These are concrete known
runtime regression gates, not proof against an unknown engine, absolute-path
subprocess invocation, `dlopen` plugin, or future bundled script/resource. New
native dependencies and the eventual installed payload still require review.
The PATH guard does not uninstall tools from the CI runner or isolate arbitrary
untrusted programs. Fixture-only selection and the private runtime directory
remain essential caller constraints.

## Inventory output and package boundary

Each run prints its fresh `/tmp/pecan-native-check.*` report directory. CI logs
retain the context, native license metadata and source-asset hashes. Files include:

- `context.txt`: commit, Rust/Cargo versions, host and stated scope.
- `native-packages.txt`, `test-packages.txt`: scoped Cargo graph evidence.
- `native-licenses.txt`: deduplicated declared package license expressions, with
  the checkout root normalized. Missing/legacy expressions remain review items;
  metadata is not license approval, source/NOTICE verification or legal closure.
- `source-assets.sha256`: tracked project LICENSE/icon assets, existing public
  screenshots, synthetic fixtures and frozen oracle JSON. This includes retained
  legacy source assets for later migration; it is not a claim they are bundled.
- Full runs also retain `native-needed.txt`, `native-linked.txt`, headless JSON,
  help output and an empty `node-attempts.txt` on success.

No artifact upload action is introduced. The report directory is not installed,
committed or treated as an application package. Running checks from a clean
checkout does not require a preexisting `dist`, `node_modules`, generated Tauri
schema, Node binary or user profile. Cargo dependencies may use the normal Cargo
cache; the workflow creates a fresh checkout/build without a cache action.

## Final cutover still needs

Finish/review route selectors, bus/recording/transport adapters, dynamic arming,
structural migration semantics and remaining input/settings/desktop behavior.
Preserve the explicit disabled MIDI status. Complete the controlled owner/quit
integration before exposing recording. Move retained Rust examples/assets before
removing the web/Tauri member; prune the lock and retire executable oracle/design
JavaScript while retaining frozen data and pinned historical evidence.

Then choose the final executable/application identity, add the actual installed
payload/desktop/AppStream and supported Linux package recipes, and inspect their
complete runtime/build/asset/license closure. Package version consistency and
0.1.1 migration notes belong to the final merge-ready gate. This candidate does
not change versions, identity, legacy build policy or deployment permissions.
Real audio/server restart/recording, desktop login/Wayland/accessibility workflows,
release packaging, crash durability and performance acceptance remain separate.
