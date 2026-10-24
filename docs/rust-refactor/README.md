# Rust refactor research

This directory collects the research snapshot supporting the proposed native Rust/GTK refactor in [issue #1](https://github.com/jee-mj/pecanmixer/issues/1). The five research artifacts remain historical planning/static-analysis snapshots. The [first implementation status](./native-slice.md) records the new bounded native connectivity slice; it does not establish feature parity or runtime performance. The application version remains `0.1.0`.

## Artifact index

| Artifact | Scope | Inspected source revision | Evidence boundary |
|---|---|---|---|
| [`pecanmixer-issue-1-rd.md`](./pecanmixer-issue-1-rd.md) | Initial no-JavaScript architecture research, alternatives, parity plan, risks, and acceptance gates | `57741f75eba111578b4d02566ff12ac5fa4400cd` | Source and primary-documentation review; no build or runtime validation |
| [`pecanmixer-issue-1-npm-inventory.md`](./pecanmixer-issue-1-npm-inventory.md) | Exact npm v3 lockfile package, class, optional-platform, and declared-license inventory accompanying the initial R&D | `57741f75eba111578b4d02566ff12ac5fa4400cd` | Committed lockfile metadata only; not installed-package or independent license-text verification |
| [`pecanmixer-js-node-inventory.md`](./pecanmixer-js-node-inventory.md) | Human-readable inventory of first-party JavaScript/TypeScript, npm tooling, IPC/events, Tauri/WebView dependencies, and removal obligations | `d8d473818331342ab53a0e1e4d3ced7977293449` | Static repository and lockfile inventory; package records are not proof of installed or shipped bytes |
| [`pecanmixer-js-node-inventory.json`](./pecanmixer-js-node-inventory.json) | Machine-readable companion containing tracked-file hashes, symbols, package records, commands, events, scripts, and webview-related Cargo entries | `d8d473818331342ab53a0e1e4d3ced7977293449` | Structured static evidence for the human-readable inventory |
| [`pecanmixer-issue-1-native-gtk-proposal.md`](./pecanmixer-issue-1-native-gtk-proposal.md) | Focused GTK architecture decision, backend/UI boundary, parity map, first-PR vertical slice, test plan, and staged removal checklist | `d8d473818331342ab53a0e1e4d3ced7977293449` | Design proposal grounded in source inspection and primary GTK/GNOME documentation; proposed APIs are uncompiled |

The implementation now moves the graph/meter/recorder code into shared crates and adds a separate read-only GTK preview. Historical source paths/revisions below describe the research baseline, not the current file layout.

The next [Stage 2 audio domain batch](./stage2-domain.md) adds pure audio projections/plans and headless policy fixtures. It is not full Stage 2 or UI parity.

The [first combined native batch](./native-batch-1.md) tracks integration, review findings and the remaining Stage 2–6 sequence.

## Revision and provenance

The initial R&D report and exact npm inventory inspected `57741f75eba111578b4d02566ff12ac5fa4400cd`. The later inventory records that the only repository change before `d8d473818331342ab53a0e1e4d3ced7977293449` was deletion of `CLAUDE.md`; the broader JavaScript/Node inventory and GTK proposal inspect the latter revision.

Before this collection was created, `origin/main` was fetched again. The remote tip and this branch's base both resolved to `d8d473818331342ab53a0e1e4d3ced7977293449`.

The collected documents preserve the substantive research. Publication-only edits replaced local filesystem provenance with relative links and changed ambiguous test-harness wording to “isolated test harness.” No non-public workflow references, credentials, restricted issue URLs, or unrelated infrastructure are included.

## Recommendation versus observed facts

The recommendation is to use direct GTK 4 through `gtk4-rs`, retain and extract the existing Rust PipeWire backend behind owned commands/events, port frontend policy into toolkit-independent Rust modules, and defer libadwaita, Relm4, and Lua unless a demonstrated need justifies them. The proposed first implementation PR is a read-only GTK connectivity slice while the existing application remains the supported UI.

That recommendation is an architectural judgment, not a benchmark result. Static inspection established, among other facts, 18 frontend TS/TSX/declaration files, 13 Tauri commands, four emitted events, 133 npm lockfile package records excluding the root, a dedicated PipeWire worker, 40 ms graph/meter coalescing, passive meter capture, and separate recording writer threads. The inspected audio path is primarily PipeWire graph/control plus metering and recording; no application-owned real-time DSP/mixing engine was identified.

During the original research, no dependency installation, build, test suite, GUI, audio device, live PipeWire/WirePlumber restart, accessibility session, packaging run, binary-size measurement, latency measurement, CPU/memory benchmark, or feature-parity exercise was performed. The later implementation checks are listed separately in [native-slice.md](./native-slice.md). Runtime behavior, performance, distro availability, desktop integration, accessibility, packaging, and final license closure remain implementation-time verification work.
