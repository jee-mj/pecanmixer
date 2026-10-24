use super::{entry_format, LaunchCommand, RefreshMode, DESKTOP_FILENAME};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_ENTRY_BYTES: u64 = 64 * 1024;
static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryFormat {
    Native,
    Legacy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryState {
    Missing,
    /// Only an exact supported template; not proof that a desktop will run it.
    Managed(EntryFormat),
    /// Edited, corrupt or unknown contents are preserved for explicit review.
    Conflict,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    path: PathBuf,
    original: Option<Vec<u8>>,
    state: EntryState,
}
impl Snapshot {
    pub fn state(&self) -> EntryState {
        self.state
    }
    pub fn original_bytes(&self) -> Option<&[u8]> {
        self.original.as_deref()
    }
}
#[derive(Debug)]
pub struct Update {
    pub snapshot: Snapshot,
    /// True means this call changed the directory entry. A false result can be
    /// an idempotent request, a debug refresh, or a missing-entry refresh.
    pub changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Read,
    CreateDirectory,
    CreateStage,
    WriteStage,
    SyncStage,
    BeforePublish,
    Publish,
    Remove,
    SyncDirectory,
}
#[derive(Debug)]
pub enum FileError {
    InvalidConfigPath,
    WrongSnapshot,
    ChangedOnDisk,
    Conflict,
    TooLarge,
    /// No symlinks or non-directory ancestors at the explicit config root and
    /// its autostart child; no symlink/non-regular destination is followed.
    UnsupportedFileType(PathBuf),
    Io {
        phase: Phase,
        path: PathBuf,
        /// A rename/unlink already happened. Reload before retrying; an error
        /// here is not a promise that the old file remains or is durable.
        published: bool,
        source: io::Error,
    },
}
impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfigPath => write!(f, "config root must be absolute, without '..'"),
            Self::WrongSnapshot => write!(f, "autostart snapshot belongs to another path"),
            Self::ChangedOnDisk => write!(f, "autostart changed on disk; reload before retrying"),
            Self::Conflict => write!(
                f,
                "preserving unrecognized autostart entry; review required"
            ),
            Self::TooLarge => write!(f, "autostart entry exceeds {MAX_ENTRY_BYTES} bytes"),
            Self::UnsupportedFileType(path) => {
                write!(
                    f,
                    "autostart path is a symlink or unsupported type: {}",
                    path.display()
                )
            }
            Self::Io {
                phase,
                path,
                published,
                source,
            } => write!(
                f,
                "autostart {phase:?} at {}: {source} (published={published})",
                path.display()
            ),
        }
    }
}
impl std::error::Error for FileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AutostartFile {
    config_root: PathBuf,
    directory: PathBuf,
    path: PathBuf,
}
impl AutostartFile {
    /// Explicit XDG_CONFIG_HOME input, never a HOME/environment lookup. The root
    /// must exist before enabling; only its autostart child may be created.
    pub fn new(config_root: impl Into<PathBuf>) -> Result<Self, FileError> {
        let config_root = config_root.into();
        if !config_root.is_absolute() || config_root.components().any(|c| c == Component::ParentDir)
        {
            return Err(FileError::InvalidConfigPath);
        }
        let directory = config_root.join("autostart");
        let path = directory.join(DESKTOP_FILENAME);
        Ok(Self {
            config_root,
            directory,
            path,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load(&self) -> Result<Snapshot, FileError> {
        let original = self.read()?;
        let state = match original.as_deref() {
            None => EntryState::Missing,
            Some(bytes) => entry_format(bytes).map_or(EntryState::Conflict, EntryState::Managed),
        };
        Ok(Snapshot {
            path: self.path.clone(),
            original,
            state,
        })
    }
    pub fn enable(&self, loaded: &Snapshot, command: &LaunchCommand) -> Result<Update, FileError> {
        self.replace_with(loaded, command, |_| Ok(()))
    }
    /// Removes only a recognized personal entry, matching legacy disable. This
    /// does not enumerate/mask a possible system-wide installation.
    pub fn disable(&self, loaded: &Snapshot) -> Result<Update, FileError> {
        self.disable_with(loaded, |_| Ok(()))
    }
    /// Explicit equivalent of legacy release-only relocation refresh. Missing
    /// entries stay missing; debug mode performs no I/O and leaves custom input.
    pub fn refresh(
        &self,
        loaded: &Snapshot,
        command: &LaunchCommand,
        mode: RefreshMode,
    ) -> Result<Update, FileError> {
        if loaded.path != self.path {
            return Err(FileError::WrongSnapshot);
        }
        if mode == RefreshMode::Debug {
            return Ok(Update {
                snapshot: loaded.clone(),
                changed: false,
            });
        }
        self.check(loaded)?;
        if loaded.state == EntryState::Missing {
            return Ok(Update {
                snapshot: loaded.clone(),
                changed: false,
            });
        }
        self.enable(loaded, command)
    }
    fn replace_with(
        &self,
        loaded: &Snapshot,
        command: &LaunchCommand,
        mut checkpoint: impl FnMut(Phase) -> io::Result<()>,
    ) -> Result<Update, FileError> {
        self.check_managed(loaded)?;
        let next = command.autostart_entry().into_bytes();
        if loaded.original.as_deref() == Some(next.as_slice()) {
            return Ok(Update {
                snapshot: loaded.clone(),
                changed: false,
            });
        }
        self.ensure_directory()?;
        let (mut file, stage) = self.create_stage()?;
        checkpoint(Phase::WriteStage)
            .and_then(|()| file.write_all(&next))
            .map_err(|e| self.error(Phase::WriteStage, &stage.0, false, e))?;
        checkpoint(Phase::SyncStage)
            .and_then(|()| file.sync_all())
            .map_err(|e| self.error(Phase::SyncStage, &stage.0, false, e))?;
        drop(file);
        checkpoint(Phase::BeforePublish)
            .map_err(|e| self.error(Phase::BeforePublish, &stage.0, false, e))?;
        // Detect external edits during staging. This is not an OS-level CAS:
        // callers must serialize writes and trust stable parent directories.
        self.check_managed(loaded)?;
        fs::rename(&stage.0, &self.path)
            .map_err(|e| self.error(Phase::Publish, &self.path, false, e))?;
        self.sync_directory(&mut checkpoint)?;
        Ok(Update {
            snapshot: Snapshot {
                path: self.path.clone(),
                original: Some(next),
                state: EntryState::Managed(EntryFormat::Native),
            },
            changed: true,
        })
    }
    fn disable_with(
        &self,
        loaded: &Snapshot,
        mut checkpoint: impl FnMut(Phase) -> io::Result<()>,
    ) -> Result<Update, FileError> {
        self.check_managed(loaded)?;
        if loaded.state == EntryState::Missing {
            return Ok(Update {
                snapshot: loaded.clone(),
                changed: false,
            });
        }
        checkpoint(Phase::Remove).map_err(|e| self.error(Phase::Remove, &self.path, false, e))?;
        self.check_managed(loaded)?;
        fs::remove_file(&self.path).map_err(|e| self.error(Phase::Remove, &self.path, false, e))?;
        self.sync_directory(&mut checkpoint)?;
        Ok(Update {
            snapshot: Snapshot {
                path: self.path.clone(),
                original: None,
                state: EntryState::Missing,
            },
            changed: true,
        })
    }
    fn check(&self, loaded: &Snapshot) -> Result<(), FileError> {
        if loaded.path != self.path {
            return Err(FileError::WrongSnapshot);
        }
        if self.read()? != loaded.original {
            return Err(FileError::ChangedOnDisk);
        }
        Ok(())
    }
    fn check_managed(&self, loaded: &Snapshot) -> Result<(), FileError> {
        self.check(loaded)?;
        if loaded.state == EntryState::Conflict {
            return Err(FileError::Conflict);
        }
        Ok(())
    }
    fn read(&self) -> Result<Option<Vec<u8>>, FileError> {
        if !self.directory_exists(&self.config_root)? || !self.directory_exists(&self.directory)? {
            return Ok(None);
        }
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(self.error(Phase::Read, &self.path, false, e)),
        };
        if !metadata.is_file() {
            return Err(FileError::UnsupportedFileType(self.path.clone()));
        }
        if metadata.len() > MAX_ENTRY_BYTES {
            return Err(FileError::TooLarge);
        }
        let mut bytes = Vec::new();
        File::open(&self.path)
            .and_then(|file| file.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|e| self.error(Phase::Read, &self.path, false, e))?;
        if bytes.len() as u64 > MAX_ENTRY_BYTES {
            return Err(FileError::TooLarge);
        }
        Ok(Some(bytes))
    }
    fn directory_exists(&self, path: &Path) -> Result<bool, FileError> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() => Ok(true),
            Ok(_) => Err(FileError::UnsupportedFileType(path.to_owned())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(self.error(Phase::Read, path, false, e)),
        }
    }
    fn ensure_directory(&self) -> Result<(), FileError> {
        // Validate both even if the config root is missing. create_dir returns
        // the actual NotFound/permission error rather than creating a profile.
        self.directory_exists(&self.config_root)?;
        if !self.directory_exists(&self.directory)? {
            fs::create_dir(&self.directory)
                .map_err(|e| self.error(Phase::CreateDirectory, &self.directory, false, e))?;
            File::open(&self.config_root)
                .and_then(|f| f.sync_all())
                .map_err(|e| self.error(Phase::CreateDirectory, &self.config_root, false, e))?;
        }
        Ok(())
    }
    fn create_stage(&self) -> Result<(File, Stage), FileError> {
        for _ in 0..32 {
            let id = STAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = self.directory.join(format!(
                ".{DESKTOP_FILENAME}.{}.{}.tmp",
                std::process::id(),
                id
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok((file, Stage(path))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(self.error(Phase::CreateStage, &path, false, e)),
            }
        }
        Err(self.error(
            Phase::CreateStage,
            &self.directory,
            false,
            io::Error::new(io::ErrorKind::AlreadyExists, "staging name collisions"),
        ))
    }
    fn sync_directory(
        &self,
        checkpoint: &mut impl FnMut(Phase) -> io::Result<()>,
    ) -> Result<(), FileError> {
        checkpoint(Phase::SyncDirectory)
            .and_then(|()| File::open(&self.directory)?.sync_all())
            .map_err(|e| self.error(Phase::SyncDirectory, &self.directory, true, e))
    }
    fn error(&self, phase: Phase, path: &Path, published: bool, source: io::Error) -> FileError {
        FileError::Io {
            phase,
            path: path.to_owned(),
            published,
            source,
        }
    }
}

// Only this call's create_new sibling is cleaned up. Crash leftovers and all
// other directory entries are deliberately left alone.
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
