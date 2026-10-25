use super::*;
use crate::desktop_launch::{legacy_entry, CommandError};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempXdg(PathBuf);
impl TempXdg {
    fn new() -> Self {
        let id = STAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = Path::new("/tmp").join(format!(
            "pecan-synthetic-xdg-{}-{time}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self) -> AutostartFile {
        AutostartFile::new(&self.0).unwrap()
    }
    fn put(&self, bytes: &[u8]) -> Snapshot {
        let file = self.file();
        fs::create_dir_all(&file.directory).unwrap();
        fs::write(file.path(), bytes).unwrap();
        file.load().unwrap()
    }
    fn entries(&self) -> Vec<PathBuf> {
        let mut paths: Vec<_> = fs::read_dir(self.file().directory)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        paths.sort();
        paths
    }
}
impl Drop for TempXdg {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn command() -> LaunchCommand {
    LaunchCommand::new("/synthetic/Applications/Pecan Mixer.AppImage").unwrap()
}

#[test]
fn native_enable_refresh_disable_never_touch_legacy_filename() {
    let dir = TempXdg::new();
    let file = dir.file();
    fs::create_dir_all(&file.directory).unwrap();
    let legacy = file
        .directory
        .join(crate::desktop_launch::LEGACY_DESKTOP_FILENAME);
    let original = legacy_entry("/synthetic/Legacy Mixer.AppImage");
    fs::write(&legacy, original.as_bytes()).unwrap();
    let paths = (
        Some(PathBuf::from("/synthetic/space prefs/settings%U.json")),
        Some(PathBuf::from("/synthetic/captures")),
    );
    let cmd = command()
        .with_paths(paths.0.clone(), paths.1.clone())
        .unwrap();
    let initial = file.load().unwrap();
    assert_eq!(initial.state(), EntryState::Missing);
    let enabled = file.enable(&initial, &cmd).unwrap();
    assert_eq!(enabled.snapshot.launch_command(), Some(cmd.clone()));
    assert_ne!(file.path(), legacy);
    assert_eq!(
        file.load().unwrap().state(),
        EntryState::Managed(EntryFormat::Native)
    );
    assert!(!file.enable(&enabled.snapshot, &cmd).unwrap().changed);
    let moved = LaunchCommand::resolve(
        Path::new("/synthetic/mounted/app"),
        Some(Path::new("/synthetic/Native Relocated.AppImage")),
    )
    .unwrap()
    .with_paths(paths.0, paths.1)
    .unwrap();
    let refreshed = file
        .refresh(&enabled.snapshot, &moved, RefreshMode::Release)
        .unwrap();
    assert!(refreshed.changed);
    assert_eq!(
        fs::read(file.path()).unwrap(),
        moved.autostart_entry().as_bytes()
    );
    assert_eq!(fs::read(&legacy).unwrap(), original.as_bytes());
    assert_eq!(file.load().unwrap().launch_command(), Some(moved));
    file.disable(&refreshed.snapshot).unwrap();
    assert!(!file.path().exists());
    assert_eq!(fs::read(&legacy).unwrap(), original.as_bytes());
}

#[test]
fn legacy_template_at_native_path_is_protected_from_all_mutations() {
    let dir = TempXdg::new();
    let original = legacy_entry("/synthetic/Legacy Mixer.AppImage");
    let snapshot = dir.put(original.as_bytes());
    let file = dir.file();
    assert_eq!(snapshot.state(), EntryState::Managed(EntryFormat::Legacy));
    assert_eq!(snapshot.launch_command(), None);
    assert!(matches!(
        file.enable(&snapshot, &command()),
        Err(FileError::Conflict)
    ));
    assert!(matches!(file.disable(&snapshot), Err(FileError::Conflict)));
    assert!(matches!(
        file.refresh(&snapshot, &command(), RefreshMode::Release),
        Err(FileError::Conflict)
    ));
    assert!(
        !file
            .refresh(&snapshot, &command(), RefreshMode::Debug)
            .unwrap()
            .changed
    );
    assert_eq!(fs::read(file.path()).unwrap(), original.as_bytes());
    assert_eq!(dir.entries(), vec![file.path.clone()]);
}

#[test]
fn native_relocation_preserves_stored_paths_despite_current_launch_options() {
    let dir = TempXdg::new();
    let file = dir.file();
    let settings = Some(PathBuf::from("/synthetic/original %U/native.json"));
    let root = Some(PathBuf::from("/synthetic/original recordings"));
    let original = command()
        .with_paths(settings.clone(), root.clone())
        .unwrap();
    let loaded = dir.put(original.autostart_entry().as_bytes());
    let executable = PathBuf::from("/synthetic/relocated native.AppImage");
    let current_launch = LaunchCommand::new(executable.clone()).unwrap();
    let expected = current_launch.clone().with_paths(settings, root).unwrap();
    let first = file
        .refresh(&loaded, &current_launch, RefreshMode::Release)
        .unwrap();
    assert!(first.changed);
    assert_eq!(
        fs::read(file.path()).unwrap(),
        expected.autostart_entry().as_bytes()
    );
    let different = current_launch
        .with_paths(Some("/synthetic/other.json".into()), None)
        .unwrap();
    assert!(
        !file
            .refresh(&first.snapshot, &different, RefreshMode::Release)
            .unwrap()
            .changed
    );
    assert_eq!(
        fs::read(file.path()).unwrap(),
        expected.autostart_entry().as_bytes()
    );
    // An explicit enable/update, unlike ordinary relocation, can change paths.
    assert!(file.enable(&first.snapshot, &different).unwrap().changed);
    assert_eq!(
        fs::read(file.path()).unwrap(),
        different.autostart_entry().as_bytes()
    );
}

#[test]
fn native_relocation_cannot_publish_an_oversized_preserved_command() {
    let dir = TempXdg::new();
    let file = dir.file();
    let path = PathBuf::from(format!("/{}", "\\".repeat(7000)));
    let original = command()
        .with_paths(Some(path.clone()), Some(path))
        .unwrap();
    let loaded = dir.put(original.autostart_entry().as_bytes());
    let longer_executable = LaunchCommand::new(format!(
        "/{}",
        "\\".repeat(super::super::MAX_PATH_BYTES - 1)
    ))
    .unwrap();
    assert!(matches!(
        file.refresh(&loaded, &longer_executable, RefreshMode::Release),
        Err(FileError::Command(CommandError::TooLong))
    ));
    assert_eq!(
        fs::read(file.path()).unwrap(),
        original.autostart_entry().as_bytes()
    );
    assert_eq!(dir.entries(), vec![file.path.clone()]);
    assert_eq!(
        file.load().unwrap().state(),
        EntryState::Managed(EntryFormat::Native)
    );
}

#[test]
fn login_context_survives_enable_reload_and_relocation_without_discovery() {
    let dir = TempXdg::new();
    let file = dir.file();
    let original = command()
        .with_paths(
            Some(dir.0.join("settings %U.json")),
            Some(dir.0.join("recordings")),
        )
        .unwrap()
        .with_management_context(dir.0.clone())
        .unwrap();
    let missing = file.load().unwrap();
    assert!(!file.path().exists());
    let enabled = file.enable(&missing, &original).unwrap();
    assert!(enabled.changed);
    let captured = file.load().unwrap().launch_command().unwrap();
    assert_eq!(captured, original);
    let moved = LaunchCommand::new("/synthetic/explicitly trusted moved native.AppImage").unwrap();
    let result = file
        .refresh(&enabled.snapshot, &moved, RefreshMode::Release)
        .unwrap();
    let actual = result.snapshot.launch_command().unwrap();
    assert_eq!(actual.management_root(), Some(dir.0.as_path()));
    assert_eq!(actual.settings(), original.settings());
    assert_eq!(actual.recording_root(), original.recording_root());
    let options = crate::startup::Options::parse(
        actual
            .argv(super::super::LaunchMode::Hidden)
            .into_iter()
            .skip(1)
            .map(Into::into),
    )
    .unwrap()
    .unwrap();
    assert_eq!(options.autostart_config_root, Some(dir.0.clone()));
    assert_eq!(
        options.native_executable.as_deref(),
        Some(moved.executable())
    );
    assert_ne!(
        options.native_executable.as_deref(),
        Some(original.executable())
    );
    file.disable(&result.snapshot).unwrap();
    assert!(!file.path().exists());
}

#[test]
fn foreign_management_root_is_not_adopted_or_written() {
    let dir = TempXdg::new();
    let other = TempXdg::new();
    let file = dir.file();
    let foreign = command().with_management_context(other.0.clone()).unwrap();
    let missing = file.load().unwrap();
    assert!(matches!(
        file.enable(&missing, &foreign),
        Err(FileError::Command(CommandError::ManagementContextMismatch))
    ));
    assert!(!file.directory.exists());
    let snapshot = dir.put(foreign.autostart_entry().as_bytes());
    assert_eq!(snapshot.state(), EntryState::Conflict);
    assert!(snapshot.launch_command().is_none());
    assert!(matches!(
        file.enable(&snapshot, &command()),
        Err(FileError::Conflict)
    ));
    assert!(matches!(file.disable(&snapshot), Err(FileError::Conflict)));
    assert!(matches!(
        file.refresh(&snapshot, &command(), RefreshMode::Release),
        Err(FileError::Conflict)
    ));
    assert_eq!(
        fs::read(file.path()).unwrap(),
        foreign.autostart_entry().as_bytes()
    );
    assert!(!other.file().path().exists());
}
fn failure() -> io::Error {
    io::Error::other("synthetic interrupted desktop write")
}

#[test]
fn enable_disable_roundtrip_is_idempotent_and_preserves_unrelated_files() {
    let dir = TempXdg::new();
    let file = dir.file();
    let missing = file.load().unwrap();
    assert_eq!(missing.state(), EntryState::Missing);
    assert!(
        !file.directory.exists(),
        "read must not create any directory"
    );
    assert!(!file.disable(&missing).unwrap().changed);
    let enabled = file.enable(&missing, &command()).unwrap();
    assert!(enabled.changed);
    let sibling = file.directory.join("synthetic-unrelated.desktop");
    fs::write(&sibling, b"unrelated bytes").unwrap();
    let crash_leftover = file.directory.join(".synthetic-interrupted.tmp");
    fs::write(&crash_leftover, b"incomplete").unwrap();
    assert_eq!(
        enabled.snapshot.state(),
        EntryState::Managed(EntryFormat::Native)
    );
    assert_eq!(
        fs::read(file.path()).unwrap(),
        command().autostart_entry().as_bytes()
    );
    assert!(!file.enable(&enabled.snapshot, &command()).unwrap().changed);
    assert_eq!(dir.entries().len(), 3);
    let disabled = file.disable(&enabled.snapshot).unwrap();
    assert!(disabled.changed);
    assert_eq!(disabled.snapshot.state(), EntryState::Missing);
    assert!(!file.disable(&disabled.snapshot).unwrap().changed);
    assert_eq!(fs::read(&sibling).unwrap(), b"unrelated bytes");
    assert_eq!(fs::read(&crash_leftover).unwrap(), b"incomplete");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let enabled = file.enable(&disabled.snapshot, &command()).unwrap();
        assert!(enabled.changed);
        assert_eq!(
            fs::metadata(file.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn explicit_release_refresh_relocates_existing_appimage_and_debug_does_nothing() {
    let dir = TempXdg::new();
    let file = dir.file();
    let old = LaunchCommand::new("/synthetic/Downloads/Old $Pecan.AppImage")
        .unwrap()
        .autostart_entry();
    let loaded = dir.put(old.as_bytes());
    assert_eq!(loaded.state(), EntryState::Managed(EntryFormat::Native));
    let debug = file
        .refresh(&loaded, &command(), RefreshMode::Debug)
        .unwrap();
    assert!(!debug.changed);
    assert_eq!(fs::read(file.path()).unwrap(), old.as_bytes());
    let refreshed = file
        .refresh(&loaded, &command(), RefreshMode::Release)
        .unwrap();
    assert!(refreshed.changed);
    assert_eq!(
        fs::read(file.path()).unwrap(),
        command().autostart_entry().as_bytes()
    );
    assert!(
        !file
            .refresh(&refreshed.snapshot, &command(), RefreshMode::Release)
            .unwrap()
            .changed
    );
    let missing = file.disable(&refreshed.snapshot).unwrap().snapshot;
    assert!(
        !file
            .refresh(&missing, &command(), RefreshMode::Release)
            .unwrap()
            .changed
    );
    assert!(
        !file.path().exists(),
        "relocation never enables a missing entry"
    );
}

#[test]
fn custom_disabled_corrupt_or_foreign_same_name_files_remain_untouched() {
    let dir = TempXdg::new();
    let file = dir.file();
    for bytes in [
        b"[Desktop Entry]\nName=Other\nHidden=true\n".as_slice(),
        b"\xffbroken",
        b"",
        b"unrelated application",
    ] {
        let snapshot = dir.put(bytes);
        assert_eq!(snapshot.state(), EntryState::Conflict);
        assert!(matches!(
            file.enable(&snapshot, &command()),
            Err(FileError::Conflict)
        ));
        assert!(matches!(file.disable(&snapshot), Err(FileError::Conflict)));
        assert!(matches!(
            file.refresh(&snapshot, &command(), RefreshMode::Release),
            Err(FileError::Conflict)
        ));
        assert!(
            !file
                .refresh(&snapshot, &command(), RefreshMode::Debug)
                .unwrap()
                .changed
        );
        assert_eq!(fs::read(file.path()).unwrap(), bytes);
        assert_eq!(dir.entries(), [file.path.clone()]);
    }
}

#[test]
fn stale_and_wrong_path_snapshots_never_overwrite_or_remove_new_contents() {
    let dir = TempXdg::new();
    let file = dir.file();
    let missing = file.load().unwrap();
    let external = b"synthetic external editor contents";
    dir.put(external);
    assert!(matches!(
        file.enable(&missing, &command()),
        Err(FileError::ChangedOnDisk)
    ));
    let known = dir.put(command().autostart_entry().as_bytes());
    fs::write(file.path(), external).unwrap();
    assert!(matches!(
        file.disable(&known),
        Err(FileError::ChangedOnDisk)
    ));
    assert!(matches!(
        file.refresh(&known, &command(), RefreshMode::Release),
        Err(FileError::ChangedOnDisk)
    ));
    let other = TempXdg::new();
    assert!(matches!(
        other.file().enable(&known, &command()),
        Err(FileError::WrongSnapshot)
    ));
    assert_eq!(fs::read(file.path()).unwrap(), external);
}

#[test]
fn interrupted_stage_never_publishes_partial_entry_or_loses_original() {
    let dir = TempXdg::new();
    let file = dir.file();
    let original = LaunchCommand::new("/synthetic/Old.AppImage")
        .unwrap()
        .autostart_entry();
    let loaded = dir.put(original.as_bytes());
    for phase in [Phase::WriteStage, Phase::SyncStage, Phase::BeforePublish] {
        let result = file.replace_with(&loaded, &command(), |p| {
            if p == phase {
                Err(failure())
            } else {
                Ok(())
            }
        });
        assert!(
            matches!(result, Err(FileError::Io { phase: p, published: false, .. }) if p == phase)
        );
        assert_eq!(fs::read(file.path()).unwrap(), original.as_bytes());
        assert_eq!(
            dir.entries(),
            [file.path.clone()],
            "own stage is cleaned up"
        );
    }
    assert!(
        file.enable(&loaded, &command()).unwrap().changed,
        "fresh retry succeeds"
    );
}

#[test]
fn external_edits_during_staging_and_remove_are_rechecked_and_preserved() {
    for remove in [false, true] {
        let dir = TempXdg::new();
        let file = dir.file();
        let loaded = dir.put(command().autostart_entry().as_bytes());
        let next = LaunchCommand::new("/synthetic/Relocated.AppImage").unwrap();
        let mut edit = |phase| {
            if phase
                == if remove {
                    Phase::Remove
                } else {
                    Phase::BeforePublish
                }
            {
                fs::write(file.path(), b"synthetic new custom entry")?;
            }
            Ok(())
        };
        let result = if remove {
            file.disable_with(&loaded, &mut edit)
        } else {
            file.replace_with(&loaded, &next, &mut edit)
        };
        assert!(matches!(result, Err(FileError::ChangedOnDisk)));
        assert_eq!(
            fs::read(file.path()).unwrap(),
            b"synthetic new custom entry"
        );
        assert_eq!(dir.entries(), [file.path.clone()]);
    }
}

#[test]
fn post_publish_sync_error_reports_changed_target_and_requires_reload() {
    let dir = TempXdg::new();
    let file = dir.file();
    let missing = file.load().unwrap();
    let result = file.replace_with(&missing, &command(), |p| {
        if p == Phase::SyncDirectory {
            Err(failure())
        } else {
            Ok(())
        }
    });
    assert!(matches!(
        result,
        Err(FileError::Io {
            phase: Phase::SyncDirectory,
            published: true,
            ..
        })
    ));
    assert_eq!(
        fs::read(file.path()).unwrap(),
        command().autostart_entry().as_bytes()
    );
    assert!(matches!(
        file.enable(&missing, &command()),
        Err(FileError::ChangedOnDisk)
    ));
    let reloaded = file.load().unwrap();
    assert!(!file.enable(&reloaded, &command()).unwrap().changed);
    let result = file.disable_with(&reloaded, |p| {
        if p == Phase::SyncDirectory {
            Err(failure())
        } else {
            Ok(())
        }
    });
    assert!(matches!(
        result,
        Err(FileError::Io {
            phase: Phase::SyncDirectory,
            published: true,
            ..
        })
    ));
    assert!(!file.path().exists());
    assert_eq!(file.load().unwrap().state(), EntryState::Missing);
}

#[test]
fn failed_remove_keeps_owned_entry_and_allows_retry() {
    let dir = TempXdg::new();
    let file = dir.file();
    let loaded = dir.put(command().autostart_entry().as_bytes());
    let result = file.disable_with(&loaded, |p| {
        if p == Phase::Remove {
            Err(failure())
        } else {
            Ok(())
        }
    });
    assert!(matches!(
        result,
        Err(FileError::Io {
            phase: Phase::Remove,
            published: false,
            ..
        })
    ));
    assert_eq!(
        fs::read(file.path()).unwrap(),
        loaded.original_bytes().unwrap()
    );
    assert!(file.disable(&loaded).unwrap().changed);
}

#[test]
fn explicit_path_validation_and_io_errors_never_fall_back_to_a_profile() {
    assert!(matches!(
        AutostartFile::new("relative"),
        Err(FileError::InvalidConfigPath)
    ));
    assert!(matches!(
        AutostartFile::new("/synthetic/../escape"),
        Err(FileError::InvalidConfigPath)
    ));
    let dir = TempXdg::new();
    let missing_root = AutostartFile::new(dir.0.join("not-created-config")).unwrap();
    let loaded = missing_root.load().unwrap();
    assert!(matches!(
        missing_root.enable(&loaded, &command()),
        Err(FileError::Io {
            phase: Phase::CreateDirectory,
            published: false,
            ..
        })
    ));
    assert!(!missing_root.config_root.exists());
    fs::write(dir.0.join("file-not-directory"), b"unchanged").unwrap();
    let bad_root = AutostartFile::new(dir.0.join("file-not-directory")).unwrap();
    assert!(matches!(
        bad_root.load(),
        Err(FileError::UnsupportedFileType(_))
    ));
    assert_eq!(
        fs::read(dir.0.join("file-not-directory")).unwrap(),
        b"unchanged"
    );
    assert_eq!(
        LaunchCommand::new("not-absolute").unwrap_err(),
        CommandError::NotAbsolute
    );
}

#[test]
fn oversized_entries_are_bounded_errors_and_never_deleted() {
    let dir = TempXdg::new();
    let file = dir.file();
    fs::create_dir(&file.directory).unwrap();
    let bytes = vec![b'x'; MAX_ENTRY_BYTES as usize + 1];
    fs::write(file.path(), &bytes).unwrap();
    assert!(matches!(file.load(), Err(FileError::TooLarge)));
    assert_eq!(fs::read(file.path()).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn symlink_targets_roots_and_autostart_directories_are_not_followed() {
    use std::os::unix::fs::symlink;
    let dir = TempXdg::new();
    let file = dir.file();
    let outside = dir.0.join("synthetic-unrelated-target");
    fs::write(&outside, command().autostart_entry()).unwrap();
    fs::create_dir(&file.directory).unwrap();
    symlink(&outside, file.path()).unwrap();
    assert!(matches!(
        file.load(),
        Err(FileError::UnsupportedFileType(_))
    ));
    assert_eq!(
        fs::read(&outside).unwrap(),
        command().autostart_entry().as_bytes()
    );
    fs::remove_file(file.path()).unwrap();
    fs::remove_dir(&file.directory).unwrap();
    let real_dir = dir.0.join("synthetic-other-autostart");
    fs::create_dir(&real_dir).unwrap();
    symlink(&real_dir, &file.directory).unwrap();
    assert!(matches!(
        file.load(),
        Err(FileError::UnsupportedFileType(_))
    ));
    assert_eq!(fs::read_dir(real_dir).unwrap().count(), 0);
    let root_alias = dir.0.join("synthetic-root-alias");
    symlink(&dir.0, &root_alias).unwrap();
    assert!(matches!(
        AutostartFile::new(root_alias).unwrap().load(),
        Err(FileError::UnsupportedFileType(_))
    ));
}

#[cfg(unix)]
#[test]
fn gio_preflight_accepts_quoted_temp_paths_and_exposes_percent_limit_without_launching() {
    use gtk::glib::{self, translate::ToGlibPtr};
    use std::os::unix::fs::PermissionsExt;

    // gio-unix's Rust wrapper is not a crate dependency. This test-only binding
    // calls the existing GIO parser, which checks the temp file but never runs
    // it or registers anything with a desktop. The returned object is owned.
    #[link(name = "gio-2.0")]
    unsafe extern "C" {
        fn g_desktop_app_info_new_from_keyfile(
            key_file: *mut glib::ffi::GKeyFile,
        ) -> *mut glib::gobject_ffi::GObject;
    }
    fn accepted(entry: &str) -> bool {
        let keyfile = glib::KeyFile::new();
        keyfile
            .load_from_data(entry, glib::KeyFileFlags::NONE)
            .unwrap();
        // SAFETY: the valid owned KeyFile outlives this call. A non-null return
        // owns one GObject reference, released exactly once without casting it
        // to a different object type or invoking any launch API.
        unsafe {
            let info = g_desktop_app_info_new_from_keyfile(keyfile.to_glib_none().0);
            if info.is_null() {
                false
            } else {
                glib::gobject_ffi::g_object_unref(info);
                true
            }
        }
    }
    let dir = TempXdg::new();
    for name in [
        "synthetic tool",
        "synthetic \"quote $cash `tick back\\slash",
        "synthetic café",
    ] {
        let path = dir.0.join(name);
        fs::write(&path, b"synthetic fixture; never executed\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(accepted(
            &LaunchCommand::new(&path).unwrap().autostart_entry()
        ));
    }
    let percent = dir.0.join("synthetic literal%f");
    fs::write(&percent, b"synthetic fixture; never executed\n").unwrap();
    fs::set_permissions(&percent, fs::Permissions::from_mode(0o700)).unwrap();
    // Desktop field-code escaping is required by the spec. GIO nevertheless
    // checks the unexpanded argv[0] for existence, rejecting this valid file.
    let escaped = legacy_entry(percent.to_str().unwrap()).replace('%', "%%");
    assert!(!accepted(&escaped));
    assert_eq!(
        LaunchCommand::new(percent).unwrap_err(),
        CommandError::PercentInExecutable
    );
}
