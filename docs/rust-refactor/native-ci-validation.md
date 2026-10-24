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
  dependencies with the same enabled fixture features reject known
  Tauri/Wry/WebKit/JavaScript engines/bindings and GTK3.
  This does not reject Rust JSON serialization or frozen JSON reference data.
- PATH guards make attempted Node/npm/npx/pnpm/yarn/corepack/bun/deno use fail and
  record command names only. An ignored failure still fails the final check.
  No npm manifest, frontend build or optional Node oracle generator is executed.
- Native ELF direct dependencies are recorded with `readelf`; the trusted built
  executable's transitive loader closure is inspected with `ldd`. Known web/JS
  libraries, GTK3, missing libraries or missing GTK4 evidence fail validation.
  Both requested library names and resolved file paths are checked by basename;
  standalone absolute paths, relative paths, spaces and a missing final newline
  cannot hide a known forbidden filename. A directory component alone is not
  treated as a library filename or as GTK4 evidence.
- The core/backend suite runs without a display/session bus. GTK accessibility,
  XTest and tray fixtures run on a newly allocated Xvfb and private D-Bus session,
  with the explicit safety markers required by the existing tests. The GTK
  command uses `--test-threads=1`: tray fixtures share a well-known watcher name
  and their existing README requires serial execution. GTK's own test-thread
  serialization does not serialize ordinary D-Bus tests.
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

## Cloud verification of the scaffold

The validator exited zero from a separate clean Git checkout: 67 core tests,
134 backend tests, 288 GTK feature tests, three headless fixtures, three controlled
smoke launches, native build, formatting and Cargo/ELF checks passed. Its 30
negative/positive controls include rejecting the known legacy graph/runtime
families, missing ELF libraries, symlink assets, duplicate license display rows
and a tool that swallows a blocked Node failure. The actual retained legacy Cargo
tree was also independently rejected without building or running that shell.

Final output has 142 deduplicated native license records, 37 tracked source
assets/fixtures and no recorded Node-tooling attempts. Local checks used the
existing cloud Rust 1.99.1 / GTK4 4.18.6 / PipeWire 1.4.2 development environment;
the source checkout was clean and contained no frontend installation/build output.
Cargo registry/build dependencies can be cached; the explicit host artifact was
compiled afresh on the initial run. A GitHub-hosted Ubuntu workflow execution
has not been observed by this local verification, and is a separate integration
check. No real audio/server, user profile, login or package installation was used.

The initial hosted run
[`40131961545`](https://github.com/jee-mj/pecanmixer/actions/runs/40131961545)
failed the existing aggregate-recording-flag test: a stopped writer joined
between snapshot capture and admission, correctly producing StaleRecording.
The other 133 backend tests passed; checkout and native dependency installation
succeeded. This was the previously reported test-ordering race, not a weakened
production admission rule or a CI dependency/scaffolding failure.

The exact test-only correction `3e48684` is now included as a cherry-pick from
the separate migration review series. It uses the existing bounded teardown
gate to hold membership stable for the intended aggregate-flag scenario. The
original assertions remain, production revision checks are unchanged, and the
full workflow continues without test retries, exclusions or suppression.
Independent review and a terminal hosted follow-up result remain required.

Hosted follow-up
[`40132277213`](https://github.com/jee-mj/pecanmixer/actions/runs/40132277213)
passed all 134 backend tests, then failed one synthetic tray availability case
with 287 GTK tests passing. The CI driver had omitted the documented serial
execution requirement for the shared private D-Bus watcher fixtures. The next
driver revision adds `--test-threads=1`; it retains all 288 tests, timeouts,
availability assertions and runtime checks. No production tray code is changed.

Hosted run
[`40132730528`](https://github.com/jee-mj/pecanmixer/actions/runs/40132730528)
completed successfully at `9699e85`: 67 core, 134 backend and 288 GTK tests,
30 validator controls, three headless fixtures and three controlled smoke
launches passed. This predates the ELF correction below and does not validate it.

## ELF filename correction after independent review

The reviewer demonstrated that the original first-token check accepted
`/synthetic/libnode.so.1` and `/synthetic/libgtk-3.so.0` alongside a valid GTK4
row. Both bypasses were reproduced before changing the validator. The correction
normalizes library basenames, inspects both sides of an `ldd` mapping and retains
whole path fields rather than splitting filenames on whitespace. It also reads
an unterminated final line. It does not load any fixture library or execute a
JavaScript runtime.

The original 30 controls remain. The expanded 86 controls cover all twelve
forbidden ELF families as absolute entries, relative requested paths, resolved
paths and tab-separated mappings; mixed case, spaces, unterminated input, valid
absolute GTK4 evidence and harmless directory names are included. Ambiguous
multiple mapping delimiters fail closed, while `=>` inside an otherwise
unambiguous path component remains valid. The expanded
suite fails against the preserved old validator and passes with the correction.
The corrected full cloud validator also exited zero: all 86 controls, 67 core,
134 backend and 288 GTK tests, native formatting/build, Cargo/ELF inspection,
three headless fixtures and three controlled smoke launches passed, with no
recorded Node-tooling attempt. The correction changes only the two validation
scripts and this report. Its independent review and exact hosted-run result
remain separate from these local cloud checks.
Unknown/renamed engines, `dlopen` plugins and future installed payloads still
require separate review; this remains a known-filename regression gate, not a
general binary attestation. No hardware/audio, real desktop, performance,
packaging or crash-durability acceptance is established.


## Independent correction clearance and PR2 integration

The complete corrected candidate is
`ed2472c338e0d091ef4ca1cf0d7d87467beb4e8a`, including both `c577452` and
`ed2472c` after the earlier `9699e85` driver. Independent review reproduced the
original two absolute-path bypasses on the old validator and confirmed rejection
with the corrected source. All 86 validator controls plus 39 independent
before/after, negative and benign-path probes pass. The real built native
loader closure is also accepted. The independent metadata audit verifies 142
unique declared-license rows, 37 selected source hashes and zero Node-tool attempts.

Exact hosted run
[`40137874890`](https://github.com/jee-mj/pecanmixer/actions/runs/40137874890)
completed successfully at `ed2472c338e0d091ef4ca1cf0d7d87467beb4e8a`.
Its logs verify 86 validator controls, 67 core, 134 backend and 288 GTK tests,
format/build/closure checks, three headless fixtures and three controlled smokes.
These are hosted results, distinct from the independent focused checks above.
The workflow and stopped-writer test fixture match the prior reviewed driver;
no test retry, exclusion, assertion removal or production API change was added.

PR2 integration takes this existing descendant of reviewed startup `20e9f90c`
without rebasing or copying unrelated candidates. Bus owner, dynamic arming,
idle migration and routing follow-ups remain excluded from this CI publication.
The known-filename and packaging/license limitations above remain unchanged.
The native aggregate validator is rerun on the integration checkout before push;
its final receipt is recorded below.

The integration-checkout aggregate `bash scripts/native/check.sh` exits zero:
86 validator controls, 67 core, 134 backend and 288 GTK tests pass, together with
native formatting/build, Cargo/ELF checks, all three headless fixtures and three
controlled smoke launches. Source asset hashes match and Node-attempt evidence is
empty. No final failure or environment/dependency blocker remains. The separate
legacy Tauri suite, default-only GTK test modes and examples were not repeated
for this CI-only follow-up; their prior results remain baseline evidence. No live
audio/hardware, physical desktop, performance or packaging acceptance was run.
