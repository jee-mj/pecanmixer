#!/usr/bin/env bash
# Negative controls for the validation boundary; no installed runtime is used.
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/check.sh"
work=$(mktemp -d "${TMPDIR:-/tmp}/pecan-native-check-tests.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
count=0
check() { "$@"; count=$((count + 1)); }
reject_packages() { ! check_packages "$1" >/dev/null 2>&1; }
reject_libraries() { ! check_linked_libraries "$1" >/dev/null 2>&1; }
printf '%s\n' 'pecan-ui-gtk v0.1.0 (/synthetic/project)' 'serde_json v1.0.0' 'gtk4 v0.11.5' > "$work/cargo"
check check_packages "$work/cargo"
for package in tauri-runtime-wry webkit2gtk javascriptcore-rs-sys serialize-to-javascript nodejs-sys napi rquickjs boa_engine wasm-bindgen gtk; do
    printf '%s v1.0.0\n' "$package" >> "$work/cargo"
    check reject_packages "$work/cargo"
    sed -i '$d' "$work/cargo"
done
: > "$work/empty"
check reject_packages "$work/empty"
printf 'not Cargo evidence\n' > "$work/malformed"
check reject_packages "$work/malformed"
printf '%s\n' 'libgtk-4.so.1 => /synthetic/libgtk-4.so.1 (0x1)' 'libgio-2.0.so.0 => /synthetic/libgio-2.0.so.0 (0x2)' > "$work/elf"
check check_linked_libraries "$work/elf"
for library in libwebkit2gtk-4.1.so libjavascriptcoregtk-4.1.so libnode.so.1 libv8.so libgtk-3.so.0; do
    printf '%s => /synthetic/%s (0x3)\n' "$library" "$library" >> "$work/elf"
    check reject_libraries "$work/elf"
    sed -i '$d' "$work/elf"
done
printf 'libgtk-4.so.1 => not found\n' > "$work/missing"
check reject_libraries "$work/missing"
check reject_libraries "$work/empty"
export NATIVE_NODE_ATTEMPTS="$work/attempts"
make_node_guards "$work/guards"
for command in node npm npx; do
    if "$work/guards/$command" ignored-argument > /dev/null 2>&1; then
        fail 'JavaScript guard unexpectedly succeeded'; exit 1
    fi
    count=$((count + 1))
done
[[ $(wc -l < "$NATIVE_NODE_ATTEMPTS") -eq 3 ]]
! grep -q ignored-argument "$NATIVE_NODE_ATTEMPTS"
printf 'synthetic asset\n' > "$work/asset"
ln -s asset "$work/asset-link"
if inventory_source_file "$work/asset-link" > /dev/null 2>&1; then
    fail 'asset inventory followed a symlink'; exit 1
fi
count=$((count + 1))
printf 'PASS: %s native validation negative/positive controls.\n' "$count"
