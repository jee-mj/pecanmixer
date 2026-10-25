# Explicit native desktop worker/tray candidate

This candidate starts at reviewed library-only PR2 head `068f90e1`. It adds one
explicit desktop capability to the existing serialized settings worker, native
shell/tray controls and paired CLI context. Corrected `bc438072` independently
clears re-review; the [combined integration record](./native-desktop-owner-integration.md)
tracks its adoption with the existing reconnect source. No actual user profile,
autostart entry, process or login session is operated by the implementation tests.

## Explicit context and remaining session boundary

Native controls require both `--autostart-config-root /absolute/config` and
`--native-executable /absolute/native-artifact`. They are a trusted caller assertion
of the native executable/bundle and personal root, not artifact/packaging proof.
The existing helper validates representations; executable percent stays unsupported.
There is no HOME/XDG/APPIMAGE lookup, executable guessing or legacy takeover.
Current `--settings` and `--recording-root` are passed literally to the reviewed
command helper; omitted settings stays memory-only and omitted recording stays
unselected. Ordinary launch, including explicit-context launch, only inspects.
Enable/update, disable and executable refresh require a separately accepted user
control action. Debug refresh retains the helper's no-I/O behavior.

The reviewed generated login command currently carries only settings/recording
paths and hidden/tray flags. It does **not** carry the desktop-management context.
A login-started process therefore has no desktop controls until it is quit and
relaunched with that explicit context. This candidate does not infer a root from
stored paths or forward context from a secondary process. Persisted management
context, process/session arbitration, AppImage/native packaging identity and real
login acceptance remain separate work. This is not full autostart parity.

Fixture/headless/smoke options reject desktop context before any worker/backend
construction. Native resources are constructed only by the retained primary
activation factory; secondary/repeated activation does not open another root or
create another worker/window. An injected startup fixture verifies that behavior
without a native backend or real tray.

## One worker and independent capabilities

`SettingsWorker::start` keeps the existing preference-only contract.
`start_with_desktop(optional_settings_file, DesktopContext)` uses that same bounded
request/result queue and global ticket allocator. Preference and desktop capabilities
are explicit; memory-only preferences remain ready even when a desktop worker
exists. The same file cannot be used directly for both targets.

Desktop Inspect/Enable/Disable/Refresh run off UI using the reviewed path-bound
snapshot API. Preference-load errors block preference publication but not desktop
operations, and desktop events cannot alter the preference baseline/document or
recovery/readiness. No new schema field, backend command, thread-per-click or
request counter is added. Queue-full is no admission and produces no deferred
mutation. Only the harmless initial Inspect is retried while it is unadmitted.

One desktop action stays pending until its matching actual result. Successful
results replace the snapshot; stale/foreign/conflicting and post-publication errors
retain the cause and require explicit inspection before further mutations. A late
result or stale control token cannot overwrite or authorize a newer request. Old
GTK buttons and cached tray callbacks retain their original worker request token.
Pending/error/protected state uses an unavailable/unknown presentation rather than
a false unchecked success. A checkmark is shown only for known personal native
entry presence/absence; no system-wide or legacy disabled state is claimed.

Tray callbacks post one fixed-size atomic mailbox action and perform no I/O. The
shell applies reserved Quit/lifetime effects first, then validates the captured
desktop token and admission. The main-window controls remain available without a
tray host. Public settings submission cannot bypass the separate desktop owner.

## Quit and errors

The existing off-UI settings drain admits the final preference document, closes
admission, drains all already accepted desktop/preference results and joins the
same worker. Its summary retains the actual latest desktop result and every
failure. The shell applies that result after drain completion, including the
explicit quit-error flow. A successful later inspection cannot erase earlier
publication/durability errors. No GTK or ksni callback joins a thread or performs
filesystem/process work. Desktop entry publication does not establish login launch,
crash durability, filesystem CAS across processes or successful desktop acceptance.

## Synthetic checks

Thirteen new checks cover desktop-only memory preferences, preference-load error
independence, pending/stale controls and delayed results, external edits and wrong
path snapshots, state-level post-publication error/reload, actual shell/tray handoff,
retired GTK buttons, mixed preference/desktop quit drain, queue-full ownership,
explicit error exit, immutable tray callback identity/unknown state, bounded mailbox
traffic with reserved Quit, CLI validation and one-factory/no-host reactivation.
The focused filter includes the 26 already reviewed helper tests. Paths/executables
and legacy sentinels are authored synthetic values; all writes use unique temporary
directories. No generated program is launched.

An initial mailbox fixture dropped its receiver and correctly received
ConsumerStopped; it now retains that receiver. A tri-state assertion was corrected
to expect known absence after a successful disable. The startup fixture now waits
for the existing bounded no-host grace period rather than expecting immediate
visibility. Production policy/assertions were not weakened. The post-publication
owner test injects its result at the state boundary; actual storage fault behavior
remains covered by the unchanged reviewed helper tests.

Initial `b01614ce` local checks pass 39 focused desktop/helper cases (13 new plus 26 reviewed
helper cases), four worker checks and 11 preference checks. Fresh combined native
validation passes 86 validator controls, 76 core, 230 backend, 522 feature GTK and
468 default GTK tests, native build/formatting, Cargo/ELF audits, three headless
fixtures and three controlled smokes. Ten legacy library tests and compilation
of five legacy examples pass. Workspace host/target packages were rebuilt; zero
Node-tool attempts are recorded. Source/privacy/whitespace and unchanged core,
backend, manifests, lockfile, schema and versions pass. That exact candidate's
hosted run `40170157397` passed. Independent review then found the mailbox ordering
defect below; those original suites did not cover it. Reconnect candidate
`91afa807` is separate and is not a hidden dependency of this source tree.

## Mailbox callback ordering correction

Independent review reproduced a current Disable followed by a retired Enable
callback replacing the one mailbox slot. The owner correctly rejected the old
token, but the current Disable had been lost. A new deterministic regression
fails on `b01614ce` with queued source 7 instead of source 9. The correction uses
the existing atomic compare-and-exchange helper to retain the newer queued
presentation source. Equal-source posts still retain the last choice. Older posts
return false without overwriting the slot. Storage remains one atomic action;
reserved Quit admission and first application are unchanged.

The new mailbox regression checks both callback orders, same-source choices and
reserved Quit. A synthetic GTK owner regression checks both orders through actual
explicit-path worker results and verifies that the temporary native entry is
removed. Fresh corrected checks pass 41 desktop/helper cases in each feature
mode, 18 tray checks, four worker checks and 11 preference checks, plus native
build, formatting and whitespace checks. The tray/desktop filters overlap; these
are not disjoint totals. Full native/core/backend/legacy suites were not repeated
for this narrow correction. No dependency blocker or final local failure remains.
Independent re-review passes 41 desktop checks, 14 default tray checks, four worker
checks, 11 preference checks and three additional callback/conflict/quit probes.
Its harness acknowledges ignored stale posts as false; action and file assertions
are unchanged. Exact candidate hosted run `40171438619` succeeds with 86 validator
controls, 76 core, 230 backend and 524 feature GTK tests plus native build, fixture
CLI and dependency checks. No actual user profile, login session, program launch
or audio acceptance is exercised.
