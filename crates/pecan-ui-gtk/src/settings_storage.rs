//! Explicit-path, synchronous settings storage for a future off-UI shell adapter.
//!
//! Nothing here chooses a profile directory, reads WebKit, runs at startup, or
//! applies preferences. Ordinary saves require a missing/valid native snapshot;
//! replacing corrupt/future input requires explicit recovery with a byte-exact
//! backup. Calls perform filesystem I/O and must stay off GTK/process threads.
//!
//! The owning shell must serialize writers. Content checks detect disk edits at
//! load/save boundaries, but are not a cross-process compare-and-swap or a guard
//! against hostile directory changes. Same-directory rename gives Linux atomic
//! visibility; file/directory sync errors remain explicit. Crash leftovers are
//! ignored, never automatically promoted or deleted.
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use pecan_core::settings::{self, Decoder, NativeLoadStatus, SettingsV1};

/// JSON is an app-side choice, not a new dependency in the validated core.
#[derive(Default)]
pub struct JsonDecoder;
impl Decoder for JsonDecoder {
    type Error = serde_json::Error;
    fn decode<T: serde::de::DeserializeOwned>(&mut self, input: &str) -> Result<T, Self::Error> {
        serde_json::from_str(input)
    }
}

/// Owned snapshot/token. Keep it to detect changed input before saving. The raw
/// bytes also permit explicit recovery of invalid UTF-8 without a lossy decode.
#[derive(Clone, Debug)]
pub struct LoadedSettings {
    settings: SettingsV1,
    status: NativeLoadStatus,
    path: PathBuf,
    original: Option<Vec<u8>>,
}
impl LoadedSettings {
    pub fn settings(&self) -> &SettingsV1 {
        &self.settings
    }
    pub fn status(&self) -> &NativeLoadStatus {
        &self.status
    }
    pub fn original_bytes(&self) -> Option<&[u8]> {
        self.original.as_deref()
    }
    pub fn recovery_required(&self) -> bool {
        matches!(
            self.status,
            NativeLoadStatus::PreserveCorruptInput
                | NativeLoadStatus::PreserveUnsupportedVersion(_)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WritePhase {
    Read,
    Backup,
    BackupSynced,
    CreateStage,
    WriteStage,
    SyncStage,
    BeforeReplace,
    Replace,
    SyncDirectory,
}
#[derive(Debug)]
pub enum StorageError {
    InvalidPath,
    WrongSnapshot,
    ChangedOnDisk,
    RecoveryRequired(NativeLoadStatus),
    NoRecoveryNeeded,
    Encode(serde_json::Error),
    RecoveryIncomplete {
        backup: PathBuf,
        source: Box<StorageError>,
    },
    Io {
        phase: WritePhase,
        path: PathBuf,
        /// True means replacement already happened. Reload before retrying;
        /// directory-sync failure does not mean the old document remains.
        published: bool,
        /// An explicit recovery may already have preserved this complete backup.
        recovery_backup: Option<PathBuf>,
        source: io::Error,
    },
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath => write!(f,"settings path must be absolute and name a file"),
            Self::WrongSnapshot => write!(f,"snapshot belongs to a different settings file"),
            Self::ChangedOnDisk => write!(f,"settings changed on disk; reload before saving"),
            Self::RecoveryRequired(status) => write!(f,"explicit recovery required: {status:?}"),
            Self::NoRecoveryNeeded => write!(f,"snapshot does not require recovery"),
            Self::Encode(error) => write!(f,"could not encode settings: {error}"),
            Self::RecoveryIncomplete{backup,source}=>write!(f,"original preserved at {}: {source}",backup.display()),
            Self::Io{phase,path,published,recovery_backup,source} => write!(f,"settings {phase:?} at {}: {source} (published={published}, recovery_backup={recovery_backup:?})",path.display()),
        }
    }
}
impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encode(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::RecoveryIncomplete { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SettingsFile {
    path: PathBuf,
}
#[derive(Debug)]
pub struct RecoveredSettings {
    pub loaded: LoadedSettings,
    pub backup: PathBuf,
}
impl SettingsFile {
    /// The parent directory must already exist. No ambient HOME/XDG lookup or
    /// automatic directory creation is hidden in this API.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let path = path.into();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(StorageError::InvalidPath);
        }
        Ok(Self { path })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load(&self) -> Result<LoadedSettings, StorageError> {
        let original = self.read()?;
        let result = match original.as_deref() {
            Some(bytes) => match std::str::from_utf8(bytes) {
                Ok(text) => settings::load_native(Some(text), &mut JsonDecoder),
                Err(_) => settings::NativeLoad {
                    settings: SettingsV1::default(),
                    status: NativeLoadStatus::PreserveCorruptInput,
                },
            },
            None => settings::load_native(None, &mut JsonDecoder),
        };
        Ok(LoadedSettings {
            settings: result.settings,
            status: result.status,
            path: self.path.clone(),
            original,
        })
    }
    /// Write through a synced sibling and atomically replace the native file.
    /// A stale snapshot or a corrupt/unsupported document is never knowingly
    /// overwritten. Inspect `StorageError::published` on failure: a successful
    /// rename followed by a sync error requires reload, not a blind retry.
    pub fn save(
        &self,
        loaded: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<LoadedSettings, StorageError> {
        self.save_with(loaded, next, |_| Ok(()))
    }
    /// Explicit reset/recovery: preserve the exact original bytes in a unique
    /// owner-only sibling backup before replacing the active native document.
    /// No legacy import is performed. A caller may supply explicitly migrated
    /// SettingsV1 data as `next`, after reviewing the core migration diagnostics.
    pub fn recover(
        &self,
        loaded: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<RecoveredSettings, StorageError> {
        self.recover_with(loaded, next, |_| Ok(()))
    }
    fn save_with(
        &self,
        loaded: &LoadedSettings,
        next: &SettingsV1,
        checkpoint: impl FnMut(WritePhase) -> io::Result<()>,
    ) -> Result<LoadedSettings, StorageError> {
        self.check(loaded)?;
        if loaded.recovery_required() {
            return Err(StorageError::RecoveryRequired(loaded.status.clone()));
        }
        self.replace(loaded, next, None, checkpoint)
    }
    fn recover_with(
        &self,
        loaded: &LoadedSettings,
        next: &SettingsV1,
        mut checkpoint: impl FnMut(WritePhase) -> io::Result<()>,
    ) -> Result<RecoveredSettings, StorageError> {
        self.check(loaded)?;
        if !loaded.recovery_required() {
            return Err(StorageError::NoRecoveryNeeded);
        }
        let (mut file, mut backup) = self.unique_file("recovery", WritePhase::Backup)?;
        let bytes = loaded
            .original
            .as_deref()
            .ok_or(StorageError::NoRecoveryNeeded)?;
        file.write_all(bytes)
            .map_err(|error| self.io_error(WritePhase::Backup, &backup.path, false, None, error))?;
        file.sync_all()
            .map_err(|error| self.io_error(WritePhase::Backup, &backup.path, false, None, error))?;
        let backup_path = backup.path.clone();
        // After a complete synced backup exists, retain it even if a later
        // directory sync or replacement fails. Partial backups are cleaned up.
        backup.keep = true;
        drop(file);
        self.sync_parent().map_err(|error| {
            self.io_error(
                WritePhase::BackupSynced,
                &backup_path,
                false,
                Some(&backup_path),
                error,
            )
        })?;
        checkpoint(WritePhase::BackupSynced).map_err(|error| {
            self.io_error(
                WritePhase::BackupSynced,
                &backup_path,
                false,
                Some(&backup_path),
                error,
            )
        })?;
        let result = self
            .replace(loaded, next, Some(&backup_path), checkpoint)
            .map_err(|error| error.with_backup(Some(&backup_path)))?;
        Ok(RecoveredSettings {
            loaded: result,
            backup: backup_path,
        })
    }
    fn replace(
        &self,
        loaded: &LoadedSettings,
        next: &SettingsV1,
        backup: Option<&Path>,
        mut checkpoint: impl FnMut(WritePhase) -> io::Result<()>,
    ) -> Result<LoadedSettings, StorageError> {
        let mut bytes = serde_json::to_vec_pretty(next).map_err(StorageError::Encode)?;
        bytes.push(b'\n');
        let (mut file, mut stage) = self
            .unique_file("stage", WritePhase::CreateStage)
            .map_err(|error| error.with_backup(backup))?;
        checkpoint(WritePhase::CreateStage).map_err(|error| {
            self.io_error(WritePhase::CreateStage, &stage.path, false, backup, error)
        })?;
        file.write_all(&bytes).map_err(|error| {
            self.io_error(WritePhase::WriteStage, &stage.path, false, backup, error)
        })?;
        checkpoint(WritePhase::WriteStage).map_err(|error| {
            self.io_error(WritePhase::WriteStage, &stage.path, false, backup, error)
        })?;
        file.sync_all().map_err(|error| {
            self.io_error(WritePhase::SyncStage, &stage.path, false, backup, error)
        })?;
        checkpoint(WritePhase::SyncStage).map_err(|error| {
            self.io_error(WritePhase::SyncStage, &stage.path, false, backup, error)
        })?;
        drop(file);
        checkpoint(WritePhase::BeforeReplace).map_err(|error| {
            self.io_error(WritePhase::BeforeReplace, &self.path, false, backup, error)
        })?;
        self.check(loaded)?;
        // The staged file is in this exact directory: Linux rename provides an
        // atomic visibility boundary, never an in-place partial JSON rewrite.
        fs::rename(&stage.path, &self.path).map_err(|error| {
            self.io_error(WritePhase::Replace, &self.path, false, backup, error)
        })?;
        stage.keep = true;
        checkpoint(WritePhase::Replace)
            .map_err(|error| self.io_error(WritePhase::Replace, &self.path, true, backup, error))?;
        checkpoint(WritePhase::SyncDirectory).map_err(|error| {
            self.io_error(WritePhase::SyncDirectory, &self.path, true, backup, error)
        })?;
        self.sync_parent().map_err(|error| {
            self.io_error(WritePhase::SyncDirectory, &self.path, true, backup, error)
        })?;
        Ok(LoadedSettings {
            settings: next.clone(),
            status: NativeLoadStatus::Loaded,
            path: self.path.clone(),
            original: Some(bytes),
        })
    }
    fn check(&self, loaded: &LoadedSettings) -> Result<(), StorageError> {
        if loaded.path != self.path {
            return Err(StorageError::WrongSnapshot);
        }
        if self.read()? != loaded.original {
            return Err(StorageError::ChangedOnDisk);
        }
        Ok(())
    }
    fn read(&self) -> Result<Option<Vec<u8>>, StorageError> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(self.io_error(
                    WritePhase::Read,
                    &self.path,
                    false,
                    None,
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "settings target is not a regular file",
                    ),
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(self.io_error(WritePhase::Read, &self.path, false, None, error))
            }
        }
        fs::read(&self.path)
            .map(Some)
            .map_err(|error| self.io_error(WritePhase::Read, &self.path, false, None, error))
    }
    fn unique_file(
        &self,
        kind: &str,
        phase: WritePhase,
    ) -> Result<(File, OwnedTemporary), StorageError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let mut name = OsString::from(".");
            name.push(self.path.file_name().ok_or(StorageError::InvalidPath)?);
            name.push(format!(
                ".{kind}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let path = self.path.with_file_name(name);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok((file, OwnedTemporary { path, keep: false })),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(self.io_error(phase, &path, false, None, error)),
            }
        }
        Err(self.io_error(
            phase,
            &self.path,
            false,
            None,
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not reserve a unique settings sibling",
            ),
        ))
    }
    fn sync_parent(&self) -> io::Result<()> {
        File::open(self.path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "missing settings parent")
        })?)?
        .sync_all()
    }
    fn io_error(
        &self,
        phase: WritePhase,
        path: &Path,
        published: bool,
        backup: Option<&Path>,
        source: io::Error,
    ) -> StorageError {
        StorageError::Io {
            phase,
            path: path.to_owned(),
            published,
            recovery_backup: backup.map(Path::to_owned),
            source,
        }
    }
}
impl StorageError {
    /// Whether this operation renamed the replacement into the active path.
    /// This describes visibility, not proof of persistence after power loss.
    pub fn published(&self) -> bool {
        match self {
            Self::Io { published, .. } => *published,
            Self::RecoveryIncomplete { source, .. } => source.published(),
            _ => false,
        }
    }
    /// A complete, file-synced original retained by explicit recovery. An error
    /// during directory sync can still leave its crash durability uncertain.
    pub fn recovery_backup(&self) -> Option<&Path> {
        match self {
            Self::Io {
                recovery_backup, ..
            } => recovery_backup.as_deref(),
            Self::RecoveryIncomplete { backup, .. } => Some(backup),
            _ => None,
        }
    }
    fn with_backup(mut self, backup: Option<&Path>) -> Self {
        if let Self::Io {
            recovery_backup, ..
        } = &mut self
        {
            *recovery_backup = backup.map(Path::to_owned);
            self
        } else if let Some(backup) = backup {
            Self::RecoveryIncomplete {
                backup: backup.to_owned(),
                source: Box::new(self),
            }
        } else {
            self
        }
    }
}
struct OwnedTemporary {
    path: PathBuf,
    keep: bool,
}
impl Drop for OwnedTemporary {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests;
