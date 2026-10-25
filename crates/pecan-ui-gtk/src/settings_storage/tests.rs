use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pecan-settings-{}-{unique}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> SettingsFile {
        SettingsFile::new(self.0.join("settings.json")).unwrap()
    }
    fn staged_files(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().contains(".stage-"))
            .collect()
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn edited() -> SettingsV1 {
    let mut preferences = SettingsV1::default().into_preferences();
    preferences.destination_width_px = Some(360.0);
    preferences.clock.start_stop = true;
    SettingsV1::new(preferences).unwrap()
}
fn fail() -> io::Error {
    io::Error::other("injected storage interruption")
}

#[test]
fn explicit_path_missing_defaults_atomic_roundtrip_and_owner_only_files() {
    assert!(matches!(
        SettingsFile::new("relative.json"),
        Err(StorageError::InvalidPath)
    ));
    assert!(matches!(
        SettingsFile::new("/"),
        Err(StorageError::InvalidPath)
    ));
    let dir = TestDir::new();
    let store = dir.store();
    let missing = store.load().unwrap();
    assert_eq!(missing.status(), &NativeLoadStatus::Missing);
    assert_eq!(missing.settings(), &SettingsV1::default());
    assert!(missing.original_bytes().is_none());
    let saved = store.save(&missing, &edited()).unwrap();
    assert_eq!(saved.status(), &NativeLoadStatus::Loaded);
    assert_eq!(saved.settings(), &edited());
    let loaded = store.load().unwrap();
    assert_eq!(loaded.settings(), &edited());
    assert_eq!(
        loaded.original_bytes(),
        Some(fs::read(store.path()).unwrap().as_slice())
    );
    assert!(dir.staged_files().is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn corrupt_future_and_invalid_utf8_require_explicit_recovery_with_verbatim_backup() {
    let dir = TestDir::new();
    let store = dir.store();
    for raw in [
        b"{truncated".to_vec(),
        br#"{"schema_version":2,"preferences":{"future":[1,2,3]}}"#.to_vec(),
        vec![0xff, 0xfe, 0x00],
    ] {
        fs::write(store.path(), &raw).unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.recovery_required());
        assert_eq!(loaded.original_bytes(), Some(raw.as_slice()));
        assert!(matches!(
            store.save(&loaded, &edited()),
            Err(StorageError::RecoveryRequired(_))
        ));
        assert_eq!(fs::read(store.path()).unwrap(), raw);
        let recovered = store.recover(&loaded, &edited()).unwrap();
        assert_ne!(recovered.backup, store.path());
        assert_eq!(fs::read(&recovered.backup).unwrap(), raw);
        assert_eq!(store.load().unwrap().settings(), &edited());
        assert!(!recovered.loaded.recovery_required());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&recovered.backup)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn future_version_status_survives_load_without_interpreting_its_preferences() {
    let dir = TestDir::new();
    let store = dir.store();
    let raw = br#"{"schema_version":900,"preferences":{"not_v1":"preserve exactly"}}"#;
    fs::write(store.path(), raw).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(
        loaded.status(),
        &NativeLoadStatus::PreserveUnsupportedVersion(900)
    );
    assert_eq!(loaded.settings(), &SettingsV1::default());
    assert_eq!(fs::read(store.path()).unwrap(), raw);
}

#[test]
fn wrong_snapshot_and_external_edits_or_deletion_are_not_overwritten() {
    let dir = TestDir::new();
    let store = dir.store();
    let initial = store.load().unwrap();
    let saved = store.save(&initial, &SettingsV1::default()).unwrap();
    let other = SettingsFile::new(dir.0.join("other.json")).unwrap();
    assert!(matches!(
        other.save(&saved, &edited()),
        Err(StorageError::WrongSnapshot)
    ));
    assert!(!other.path().exists());
    assert!(matches!(
        store.save(&initial, &edited()),
        Err(StorageError::ChangedOnDisk)
    ));
    fs::write(store.path(), b"external edit").unwrap();
    assert!(matches!(
        store.save(&saved, &edited()),
        Err(StorageError::ChangedOnDisk)
    ));
    assert_eq!(fs::read(store.path()).unwrap(), b"external edit");
    fs::remove_file(store.path()).unwrap();
    assert!(matches!(
        store.save(&saved, &edited()),
        Err(StorageError::ChangedOnDisk)
    ));
    assert!(!store.path().exists());
}

#[test]
fn interruption_before_replace_keeps_original_and_cleans_only_owned_stage() {
    for failure in [
        WritePhase::CreateStage,
        WritePhase::WriteStage,
        WritePhase::SyncStage,
        WritePhase::BeforeReplace,
    ] {
        let dir = TestDir::new();
        let store = dir.store();
        let loaded = store.load().unwrap();
        let baseline = store.save(&loaded, &SettingsV1::default()).unwrap();
        let before = fs::read(store.path()).unwrap();
        let error = store
            .save_with(&baseline, &edited(), |phase| {
                if phase == failure {
                    Err(fail())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert!(!error.published(), "{error}");
        assert_eq!(fs::read(store.path()).unwrap(), before);
        assert!(dir.staged_files().is_empty());
        store.save(&baseline, &edited()).unwrap();
        assert_eq!(store.load().unwrap().settings(), &edited());
    }
}

#[test]
fn directory_sync_or_post_replace_failure_reports_published_and_requires_reload() {
    for failure in [WritePhase::Replace, WritePhase::SyncDirectory] {
        let dir = TestDir::new();
        let store = dir.store();
        let missing = store.load().unwrap();
        let error = store
            .save_with(&missing, &edited(), |phase| {
                if phase == failure {
                    Err(fail())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert!(
            error.published(),
            "must expose successful replacement: {error}"
        );
        assert_eq!(store.load().unwrap().settings(), &edited());
        assert!(dir.staged_files().is_empty());
        assert!(matches!(
            store.save(&missing, &SettingsV1::default()),
            Err(StorageError::ChangedOnDisk)
        ));
        let current = store.load().unwrap();
        store.save(&current, &SettingsV1::default()).unwrap();
    }
}

#[test]
fn crash_leftover_stage_is_never_loaded_or_automatically_deleted() {
    let dir = TestDir::new();
    let store = dir.store();
    let missing = store.load().unwrap();
    store.save(&missing, &SettingsV1::default()).unwrap();
    let orphan = dir.0.join(".settings.json.stage-crashed");
    fs::write(&orphan, b"{unfinished").unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.settings(), &SettingsV1::default());
    store.save(&loaded, &edited()).unwrap();
    assert_eq!(fs::read(&orphan).unwrap(), b"{unfinished");
    assert_eq!(store.load().unwrap().settings(), &edited());
}

#[test]
fn recovery_failure_retains_original_and_reports_complete_backup_for_retry() {
    for failure in [WritePhase::BackupSynced, WritePhase::BeforeReplace] {
        let dir = TestDir::new();
        let store = dir.store();
        let raw = b"{bad settings";
        fs::write(store.path(), raw).unwrap();
        let loaded = store.load().unwrap();
        let error = store
            .recover_with(&loaded, &edited(), |phase| {
                if phase == failure {
                    Err(fail())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert!(!error.published());
        let backup = error.recovery_backup().unwrap();
        assert_eq!(fs::read(backup).unwrap(), raw);
        assert_eq!(fs::read(store.path()).unwrap(), raw);
        assert!(dir.staged_files().is_empty());
        let retry = store.recover(&loaded, &edited()).unwrap();
        assert_ne!(retry.backup, backup);
        assert_eq!(fs::read(&retry.backup).unwrap(), raw);
        assert_eq!(store.load().unwrap().settings(), &edited());
    }
}

#[test]
fn detectable_change_during_staging_is_rejected_even_after_recovery_backup() {
    for recovery in [false, true] {
        let dir = TestDir::new();
        let store = dir.store();
        fs::write(
            store.path(),
            if recovery {
                b"{bad".as_slice()
            } else {
                br#"{"schema_version":1}"#.as_slice()
            },
        )
        .unwrap();
        let loaded = store.load().unwrap();
        let checkpoint = |phase| {
            if phase == WritePhase::BeforeReplace {
                fs::write(store.path(), b"new external content")?;
            }
            Ok(())
        };
        let error = if recovery {
            store
                .recover_with(&loaded, &edited(), checkpoint)
                .unwrap_err()
        } else {
            store.save_with(&loaded, &edited(), checkpoint).unwrap_err()
        };
        assert!(!error.published());
        assert_eq!(fs::read(store.path()).unwrap(), b"new external content");
        assert!(dir.staged_files().is_empty());
        if recovery {
            assert_eq!(fs::read(error.recovery_backup().unwrap()).unwrap(), b"{bad");
        } else {
            assert!(matches!(error, StorageError::ChangedOnDisk));
        }
    }
}

#[test]
fn nonregular_target_and_missing_parent_are_errors_without_implicit_directory_creation() {
    let dir = TestDir::new();
    let store = dir.store();
    fs::create_dir(store.path()).unwrap();
    assert!(matches!(
        store.load(),
        Err(StorageError::Io {
            phase: WritePhase::Read,
            ..
        })
    ));
    let nested = SettingsFile::new(dir.0.join("missing/settings.json")).unwrap();
    let missing = nested.load().unwrap();
    assert!(matches!(
        nested.save(&missing, &edited()),
        Err(StorageError::Io {
            phase: WritePhase::CreateStage,
            published: false,
            ..
        })
    ));
    assert!(!dir.0.join("missing").exists());
    #[cfg(unix)]
    {
        let target = dir.0.join("target.json");
        fs::write(&target, b"untouched").unwrap();
        let linked = SettingsFile::new(dir.0.join("linked.json")).unwrap();
        std::os::unix::fs::symlink(&target, linked.path()).unwrap();
        assert!(linked.load().is_err());
        assert_eq!(fs::read(target).unwrap(), b"untouched");
    }
}

#[test]
fn legacy_export_is_used_only_as_explicit_migration_data() {
    let dir = TestDir::new();
    let store = dir.store();
    let exported = include_str!("../../../../fixtures/settings/web-current.json");
    fs::write(dir.0.join("web-export.json"), exported).unwrap();
    let native = store.load().unwrap();
    assert_eq!(native.status(), &NativeLoadStatus::Missing);
    assert_eq!(native.settings(), &SettingsV1::default());
    let entries = serde_json::from_str(exported).unwrap();
    let migrated = settings::migrate_web(&entries, &mut JsonDecoder);
    assert!(!migrated.issues.is_empty());
    store.save(&native, &migrated.settings).unwrap();
    assert_eq!(store.load().unwrap().settings(), &migrated.settings);
    assert_eq!(
        fs::read_to_string(dir.0.join("web-export.json")).unwrap(),
        exported
    );
}
