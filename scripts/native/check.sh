#!/usr/bin/env bash
# Native Linux validation only. Never launch the application without --fixture.
set -euo pipefail

fail() { printf 'native-check: %s\n' "$*" >&2; return 1; }

forbidden_package() {
    case "$1" in
        tauri|tauri-*|wry|tao|webkit*|javascriptcore*|serialize-to-javascript*|gtk|gtk-sys|\
        node|nodejs|node-*|nodejs-*|napi|napi-*|v8|rusty_v8|quick-js|quickjs*|\
        rquickjs*|boa_*|mozjs*|duktape*|mujs*|deno_*|deno-*|cef|cef-*|cef_*|electron*|neon|neon-*|js-sys|web-sys|wasm-bindgen*) return 0 ;;
        *) return 1 ;;
    esac
}

# Input is Cargo's --prefix none --format '{p}' output, not arbitrary metadata.
check_packages() {
    local line name count=0
    while IFS= read -r line; do
        [[ -n "$line" ]] || continue
        [[ "$line" =~ ^[a-zA-Z0-9_-]+[[:space:]]v[0-9] ]] || {
            fail "unrecognized Cargo package row: $line"; return 1;
        }
        name=${line%% *}
        if forbidden_package "$name"; then
            fail "forbidden native dependency: $name"; return 1
        fi
        count=$((count + 1))
    done < "$1"
    (( count > 0 )) || fail 'empty Cargo dependency evidence'
}

forbidden_library() {
    case "${1,,}" in
        libwebkit*|libjavascriptcore*|libnode.*|libv8*|libmozjs*|libquickjs*|libduktape*|libmujs*|libgjs*|libcef*|libgtk-3.*|libgdk-3.*) return 0 ;;
        *) return 1 ;;
    esac
}

check_linked_libraries() {
    local line library count=0 gtk4=false
    while IFS= read -r line; do
        [[ "$line" != *'not found'* ]] || { fail "unresolved runtime library: $line"; return 1; }
        read -r library _ <<< "$line"
        [[ -n "$library" ]] || continue
        if forbidden_library "$library"; then
            fail "forbidden native ELF dependency: $library"; return 1
        fi
        [[ "$library" != libgtk-4.so* ]] || gtk4=true
        count=$((count + 1))
    done < "$1"
    (( count > 0 )) && "$gtk4" || fail 'ELF closure must include the native GTK4 library'
}

# Exposed separately so regression tests can prove failures are detected without
# installing/running any JavaScript runtime. Record names only, never arguments.
make_node_guards() {
    local directory=$1 command
    mkdir -p "$directory"
    cat > "$directory/blocked-js-runtime" <<'GUARD'
#!/usr/bin/env bash
printf '%s\n' "${0##*/}" >> "${NATIVE_NODE_ATTEMPTS:?}"
printf 'native-check: JavaScript tooling is disabled: %s\n' "${0##*/}" >&2
exit 126
GUARD
    chmod +x "$directory/blocked-js-runtime"
    for command in node nodejs npm npx pnpm yarn corepack bun deno; do
        ln -s blocked-js-runtime "$directory/$command"
    done
}

inventory_source_file() {
    [[ -f "$1" && ! -L "$1" ]] || { fail "source asset must be a regular file: $1"; return 1; }
    sha256sum -- "$1"
}

normalize_license_rows() {
    local root=$1 line
    while IFS= read -r line; do
        # Cargo appends this display marker after the license field too.
        line=${line%' (*)'}
        printf '%s\n' "${line//"$root"/.}"
    done | sort -u
}

main() {
    local mode=${1:-all}
    [[ $# -le 1 ]] || { fail 'usage: scripts/native/check.sh [all|audit]'; return 1; }
    case "$mode" in all|audit) ;; *) fail 'usage: scripts/native/check.sh [all|audit]'; return 1 ;; esac
    local repo_root target report_dir line binary fixture build_root command
    repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
    cd -- "$repo_root"
    target=$(rustc -vV | sed -n 's/^host: //p')
    [[ "$target" == *-linux-* ]] || { fail 'this bounded validator supports its Linux host only'; return 1; }
    for command in cargo git timeout sort sha256sum; do
        command -v "$command" >/dev/null || { fail "missing tool: $command"; return 1; }
    done
    report_dir=$(mktemp -d "${TMPDIR:-/tmp}/pecan-native-check.XXXXXX")
    export NATIVE_NODE_ATTEMPTS="$report_dir/node-attempts.txt"
    : > "$NATIVE_NODE_ATTEMPTS"
    make_node_guards "$report_dir/no-js"
    export PATH="$report_dir/no-js:$PATH"
    export LC_ALL=C
    export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
    printf 'Native validation output: %s\n' "$report_dir"
    {
        printf 'commit: '; git rev-parse HEAD
        rustc --version
        cargo --version
        printf 'host: %s\n' "$target"
        printf 'scope: native Linux target; no legacy build or audio examples\n'
    } > "$report_dir/context.txt"

    timeout 20m cargo tree -p pecan-ui-gtk --target "$target" --edges normal,build \
        --prefix none --format '{p}' --locked > "$report_dir/native-packages.txt"
    check_packages "$report_dir/native-packages.txt"
    timeout 20m cargo tree -p pecan-core -p pecan-pipewire -p pecan-ui-gtk --target "$target" \
        --features pecan-ui-gtk/a11y-tests,pecan-ui-gtk/x11-input-tests,pecan-ui-gtk/tray-bus-tests \
        --edges normal,build,dev --prefix none --format '{p}' --locked > "$report_dir/test-packages.txt"
    check_packages "$report_dir/test-packages.txt"
    {
        printf '# Cargo license metadata only; manual license/NOTICE review remains required.\n'
        timeout 20m cargo tree -p pecan-ui-gtk --target "$target" --edges normal,build \
            --prefix none --format '{p}|{l}' --locked | normalize_license_rows "$repo_root"
    } > "$report_dir/native-licenses.txt"
    {
        printf '# Retained source assets and synthetic fixtures; NOT an installed package manifest.\n'
        git ls-files -z -- LICENSE app-icon.svg 'src-tauri/icons/*' 'docs/screenshots/*' \
            'fixtures/*' 'crates/pecan-ui-gtk/src/controls/*/oracle.json' |
            while IFS= read -r -d '' fixture; do inventory_source_file "$fixture"; done
    } > "$report_dir/source-assets.sha256"
    cat "$report_dir/context.txt" "$report_dir/native-licenses.txt" "$report_dir/source-assets.sha256"
    if [[ "$mode" == audit ]]; then
        [[ ! -s "$NATIVE_NODE_ATTEMPTS" ]] || { fail 'JavaScript tooling was attempted'; return 1; }
        return
    fi

    for command in readelf ldd xvfb-run dbus-run-session cmp; do
        command -v "$command" >/dev/null || { fail "missing tool: $command"; return 1; }
    done
    # All GUI resources are private fixtures, independent of an ambient desktop,
    # settings profile or PipeWire socket. Never set up an actual audio service.
    mkdir -p "$report_dir/profile"/{config,data,cache,runtime}
    chmod 700 "$report_dir/profile/runtime"
    export XDG_CONFIG_HOME="$report_dir/profile/config"
    export XDG_DATA_HOME="$report_dir/profile/data"
    export XDG_CACHE_HOME="$report_dir/profile/cache"
    export XDG_RUNTIME_DIR="$report_dir/profile/runtime"
    export PIPEWIRE_REMOTE=pecan-native-ci-unavailable
    export GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=test
    unset PECAN_GTK_TEST_SNAPSHOTS
    build_root=${CARGO_TARGET_DIR:-$repo_root/target}
    mkdir -p "$build_root"
    export CARGO_TARGET_DIR
    CARGO_TARGET_DIR=$(cd -- "$build_root" && pwd -P)
    bash scripts/native/test-check.sh
    timeout 20m cargo fmt -p pecan-core -p pecan-pipewire -p pecan-ui-gtk -- --check
    env -u DISPLAY -u DBUS_SESSION_BUS_ADDRESS timeout 20m \
        cargo test -p pecan-core -p pecan-pipewire --target "$target" --locked
    # Explicit host and target-dir bind the inspected graph to the executable;
    # inherited cross-compilation configuration cannot select another artifact.
    timeout 20m cargo build -p pecan-ui-gtk --target "$target" --locked
    binary="$CARGO_TARGET_DIR/$target/debug/pecan-ui-gtk"
    [[ -x "$binary" ]] || { fail "missing native executable: $binary"; return 1; }
    readelf -d "$binary" > "$report_dir/native-needed.txt"
    ldd "$binary" > "$report_dir/native-linked.txt"
    check_linked_libraries "$report_dir/native-linked.txt"
    # Display-free paths must work with no session bus/display. The Rust core
    # tests provide semantic JSON comparison; this executable check verifies the
    # exact reviewed pretty-output contract as well.
    for fixture in audio-basic disconnected reconnect-after; do
        env -u DISPLAY -u DBUS_SESSION_BUS_ADDRESS timeout 15s "$binary" \
            --fixture "fixtures/graph/$fixture.json" --headless > "$report_dir/$fixture.json"
    done
    cmp fixtures/expected/audio-basic-overview.json "$report_dir/audio-basic.json"
    env -u DISPLAY -u DBUS_SESSION_BUS_ADDRESS timeout 15s "$binary" --help > "$report_dir/help.txt"
    timeout 20m xvfb-run -a -s '-screen 0 1600x1000x24 -nolisten tcp' dbus-run-session -- bash -c '
        set -euo pipefail
        export PECAN_XTEST_FIXTURE_DISPLAY="$DISPLAY"
        export PECAN_TRAY_TEST_BUS="$DBUS_SESSION_BUS_ADDRESS"
        # The tray fixtures share one private watcher name and explicitly
        # require serial libtest execution (tray/README.md). GTK serialization
        # alone does not serialize these ordinary #[test] D-Bus fixtures.
        cargo test -p pecan-ui-gtk --target "$2" --locked --features a11y-tests,x11-input-tests,tray-bus-tests -- --test-threads=1
        timeout 15s "$1" --fixture fixtures/graph/audio-basic.json --smoke
        timeout 15s "$1" --fixture fixtures/graph/audio-basic.json --smoke --patchbay
        timeout 15s "$1" --fixture fixtures/graph/disconnected.json --smoke --hidden
    ' native-fixtures "$binary" "$target"
    [[ ! -s "$NATIVE_NODE_ATTEMPTS" ]] || { fail 'JavaScript tooling was attempted'; return 1; }
    printf 'PASS: native build, synthetic tests, fixture CLI, Cargo/ELF closure checks.\n'
    printf 'Inventory output remains at %s\n' "$report_dir"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then main "$@"; fi
