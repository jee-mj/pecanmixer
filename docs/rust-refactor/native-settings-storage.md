# Explicit native settings storage boundary (draft)

This increment follows [batch 3](./native-batch-3.md). It adds app-side synchronous
storage for the validated settings contract. It is **not called by startup or the
GTK preview**, selects no profile directory, and applies no runtime preferences.
The future owning shell must supply an explicit absolute path and serialize all
calls off GTK and audio process threads.

Ordinary saves preserve corrupt JSON, invalid UTF-8 and future-schema input.
Explicit recovery first retains a synced, byte-exact owner-only backup. Saves
write a unique owner-only sibling, sync it, atomically rename it on Linux, then
sync the directory. Errors distinguish pre-replacement failure from an already
published replacement; callers reload after a post-rename error. Detected content
changes abort saves. These checks are not an interprocess lock or protection
against a change racing the final content check. Crash/power-loss and hostile
filesystem behavior are not established by synthetic tests.

Independent review passed all 11 focused storage tests plus two additional probes:
an already-open reader retains the old inode across replacement, and failed
post-rename recovery exposes published state while retaining the original backup.
The 11 tests also pass after integration with batch 3; native GTK builds. All I/O
used fresh temporary directories with synthetic settings. No user profile or
browser database was accessed. Shared core APIs/storage source are unchanged by
integration; documentation and export conflicts retained both earlier features.

GTK adds a direct reference to the workspace's already-locked `serde` dependency;
no new package/version, JavaScript runtime or product-version change is included.
Mixer/patchbay production controls, settings UI/runtime effects, multi-instance
coordination, native tray/autostart and complete parity remain unfinished.
GTK display tests were not repeated for this display-free storage increment.
No live hardware, desktop, performance, power-loss or packaging checks were run.

See [storage API and failure semantics](../../crates/pecan-ui-gtk/README.md#explicit-settings-storage-boundary).
