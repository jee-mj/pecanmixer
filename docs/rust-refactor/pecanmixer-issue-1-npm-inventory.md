# Pecan Mixer issue 1 — exact npm lock inventory

Research snapshot: fork `jee-mj/pecanmixer` and upstream `BadgerSoft-NL/pecanmixer`, both at `57741f75eba111578b4d02566ff12ac5fa4400cd` (2026-10-24).

Source: committed npm lockfile v3, `pecanmixer/package-lock.json`, SHA-256 `1683c9f1438bf34daebd8ceef1a3251b5b2f44f7b88109a8c9b8563572d3aed4`. The root record is excluded. Classification uses the root `package.json` direct dependency sets and each lock record's `dev` and `optional` flags.

- 133 package records: 5 production-closure records and 128 development records.
- 64 optional platform records and 69 non-optional records. Optional records are lockfile possibilities, not packages loaded together on one host.
- License metadata: MIT 110; Apache-2.0 OR MIT 13; ISC 5; Apache-2.0 2; BSD-3-Clause 1; CC-BY-4.0 1; OFL-1.1 1.
- Direct production: `@fontsource/ibm-plex-mono`, `@tauri-apps/api`, `react`, `react-dom`.
- The only transitive production record is `scheduler`.
- Direct development: `@tauri-apps/cli`, `@types/react`, `@types/react-dom`, `@vitejs/plugin-react`, `typescript`, `vite`.

This is an inventory of lockfile metadata, not a legal conclusion or independent verification of every upstream license text.

```tsv
package	locked version	class	optional	declared license
@babel/code-frame	7.29.7	transitive-dev	no	MIT
@babel/compat-data	7.29.7	transitive-dev	no	MIT
@babel/core	7.29.7	transitive-dev	no	MIT
@babel/generator	7.29.8	transitive-dev	no	MIT
@babel/helper-compilation-targets	7.29.7	transitive-dev	no	MIT
@babel/helper-globals	7.29.7	transitive-dev	no	MIT
@babel/helper-module-imports	7.29.7	transitive-dev	no	MIT
@babel/helper-module-transforms	7.29.7	transitive-dev	no	MIT
@babel/helper-plugin-utils	7.29.7	transitive-dev	no	MIT
@babel/helper-string-parser	7.29.7	transitive-dev	no	MIT
@babel/helper-validator-identifier	7.29.7	transitive-dev	no	MIT
@babel/helper-validator-option	7.29.7	transitive-dev	no	MIT
@babel/helpers	7.29.10	transitive-dev	no	MIT
@babel/parser	7.29.9	transitive-dev	no	MIT
@babel/plugin-transform-react-jsx-self	7.29.7	transitive-dev	no	MIT
@babel/plugin-transform-react-jsx-source	7.29.7	transitive-dev	no	MIT
@babel/template	7.29.7	transitive-dev	no	MIT
@babel/traverse	7.29.10	transitive-dev	no	MIT
@babel/types	7.29.8	transitive-dev	no	MIT
@esbuild/aix-ppc64	0.28.2	transitive-dev	yes	MIT
@esbuild/android-arm	0.28.2	transitive-dev	yes	MIT
@esbuild/android-arm64	0.28.2	transitive-dev	yes	MIT
@esbuild/android-x64	0.28.2	transitive-dev	yes	MIT
@esbuild/darwin-arm64	0.28.2	transitive-dev	yes	MIT
@esbuild/darwin-x64	0.28.2	transitive-dev	yes	MIT
@esbuild/freebsd-arm64	0.28.2	transitive-dev	yes	MIT
@esbuild/freebsd-x64	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-arm	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-arm64	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-ia32	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-loong64	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-mips64el	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-ppc64	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-riscv64	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-s390x	0.28.2	transitive-dev	yes	MIT
@esbuild/linux-x64	0.28.2	transitive-dev	yes	MIT
@esbuild/netbsd-arm64	0.28.2	transitive-dev	yes	MIT
@esbuild/netbsd-x64	0.28.2	transitive-dev	yes	MIT
@esbuild/openbsd-arm64	0.28.2	transitive-dev	yes	MIT
@esbuild/openbsd-x64	0.28.2	transitive-dev	yes	MIT
@esbuild/openharmony-arm64	0.28.2	transitive-dev	yes	MIT
@esbuild/sunos-x64	0.28.2	transitive-dev	yes	MIT
@esbuild/win32-arm64	0.28.2	transitive-dev	yes	MIT
@esbuild/win32-ia32	0.28.2	transitive-dev	yes	MIT
@esbuild/win32-x64	0.28.2	transitive-dev	yes	MIT
@fontsource/ibm-plex-mono	5.3.0	direct-prod	no	OFL-1.1
@jridgewell/gen-mapping	0.3.13	transitive-dev	no	MIT
@jridgewell/remapping	2.3.5	transitive-dev	no	MIT
@jridgewell/resolve-uri	3.1.2	transitive-dev	no	MIT
@jridgewell/sourcemap-codec	1.6.0	transitive-dev	no	MIT
@jridgewell/trace-mapping	0.3.31	transitive-dev	no	MIT
@napi-rs/lzma-linux-x64-gnu	1.5.1	transitive-dev	yes	MIT
@rolldown/pluginutils	1.0.0-beta.27	transitive-dev	no	MIT
@rollup/rollup-android-arm-eabi	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-android-arm64	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-darwin-arm64	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-darwin-x64	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-freebsd-arm64	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-freebsd-x64	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-arm-gnueabihf	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-arm-musleabihf	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-arm64-gnu	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-arm64-musl	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-loong64-gnu	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-loong64-musl	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-ppc64-gnu	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-ppc64-musl	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-riscv64-gnu	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-riscv64-musl	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-s390x-gnu	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-x64-gnu	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-linux-x64-musl	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-openbsd-x64	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-openharmony-arm64	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-win32-arm64-msvc	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-win32-ia32-msvc	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-win32-x64-gnu	4.64.9	transitive-dev	yes	MIT
@rollup/rollup-win32-x64-msvc	4.64.9	transitive-dev	yes	MIT
@tauri-apps/api	2.12.4	direct-prod	no	Apache-2.0 OR MIT
@tauri-apps/cli	2.12.3	direct-dev	no	Apache-2.0 OR MIT
@tauri-apps/cli-darwin-arm64	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-darwin-x64	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-linux-arm-gnueabihf	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-linux-arm64-gnu	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-linux-arm64-musl	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-linux-riscv64-gnu	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-linux-x64-gnu	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-linux-x64-musl	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-win32-arm64-msvc	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-win32-ia32-msvc	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@tauri-apps/cli-win32-x64-msvc	2.12.3	transitive-dev	yes	Apache-2.0 OR MIT
@types/babel__core	7.20.5	transitive-dev	no	MIT
@types/babel__generator	7.27.0	transitive-dev	no	MIT
@types/babel__template	7.4.4	transitive-dev	no	MIT
@types/babel__traverse	7.28.0	transitive-dev	no	MIT
@types/estree	1.0.9	transitive-dev	no	MIT
@types/react	19.3.0	direct-dev	no	MIT
@types/react-dom	19.3.0	direct-dev	no	MIT
@vitejs/plugin-react	4.7.0	direct-dev	no	MIT
baseline-browser-mapping	2.11.29	transitive-dev	no	Apache-2.0
browserslist	4.29.3	transitive-dev	no	MIT
caniuse-lite	1.0.30001816	transitive-dev	no	CC-BY-4.0
convert-source-map	2.0.0	transitive-dev	no	MIT
csstype	3.2.3	transitive-dev	no	MIT
debug	4.4.3	transitive-dev	no	MIT
electron-to-chromium	1.5.459	transitive-dev	no	ISC
esbuild	0.28.2	transitive-dev	no	MIT
escalade	3.2.0	transitive-dev	no	MIT
fdir	6.5.0	transitive-dev	no	MIT
fsevents	2.3.3	transitive-dev	yes	MIT
gensync	1.0.0-beta.2	transitive-dev	no	MIT
js-tokens	4.0.0	transitive-dev	no	MIT
jsesc	3.1.0	transitive-dev	no	MIT
json5	2.2.3	transitive-dev	no	MIT
lru-cache	5.1.1	transitive-dev	no	ISC
ms	2.1.3	transitive-dev	no	MIT
nanoid	3.3.20	transitive-dev	no	MIT
node-releases	2.0.59	transitive-dev	no	MIT
picocolors	1.1.1	transitive-dev	no	ISC
picomatch	4.0.7	transitive-dev	no	MIT
postcss	8.5.30	transitive-dev	no	MIT
react	19.3.1	direct-prod	no	MIT
react-dom	19.3.1	direct-prod	no	MIT
react-refresh	0.17.0	transitive-dev	no	MIT
rollup	4.64.9	transitive-dev	no	MIT
scheduler	0.28.0	transitive-prod	no	MIT
semver	6.3.1	transitive-dev	no	ISC
source-map-js	1.2.2	transitive-dev	no	BSD-3-Clause
tinyglobby	0.2.17	transitive-dev	no	MIT
typescript	5.8.3	direct-dev	no	Apache-2.0
update-browserslist-db	1.3.4	transitive-dev	no	MIT
vite	7.3.7	direct-dev	no	MIT
yallist	3.1.1	transitive-dev	no	ISC
```
