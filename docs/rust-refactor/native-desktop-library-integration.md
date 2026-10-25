# Reviewed native desktop helper — library only

Reviewed `79d2c9ebec0854e1775331a1cd8d0c3b1ef11aa3` applies directly onto PR2
`d0d8f23f`. This checkpoint adds only `desktop_launch`, its authored tests/README
and the library export. It does not mount startup, a worker, tray or autostart UI,
and it does not access any actual user profile or install/launch anything.

Independent review clears 26 focused checks and three additional groups covering
exact argv/escaping, relocation and omitted options, stale snapshots and preserved
legacy/other-root entries. Native identity and marker are distinct from legacy;
only exact current generated native entries can be changed. Executable percent
characters remain explicitly unsupported. Settings and recording arguments retain
literal paths. Root/executable/bundle authority remains an explicit caller input;
no HOME/XDG/APPIMAGE lookup or packaging proof is inferred.

Fresh combined cloud checks pass 86 validator controls, 76 core, 230 backend,
509 feature GTK and 455 default GTK tests, native build/formatting, Cargo/ELF
audits, three headless fixtures and three controlled smokes. The 26 helper checks,
ten legacy library tests and compilation of five legacy examples pass. Workspace
host and target packages were rebuilt, and the Node-tool attempt inventory is
empty. Runtime sources match `79d2c9eb` exactly. Whitespace/privacy/scope and
unchanged versions/manifests/lockfile checks pass. No final failed check or
dependency/environment blocker remains; exact published-head hosted evidence is
recorded separately in the PR description.

Worker/tray/runtime adoption remains a separate review candidate. It must reuse
one serialized settings worker and its quit drain, with desktop capability separate
from preference persistence/readiness. Any future executable/config-root/options
must be explicit; ordinary launch must never create an enabled entry. Retain real
request/results and reload after stale/post-publish errors. No legacy takeover,
real profile mutation, login/desktop acceptance, cross-process arbitration or
release/install/license closure is established here. Legacy functionality and
versions remain unchanged.
