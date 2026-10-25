//! Explicit desktop capability on the one existing serialized I/O worker.
use crate::desktop_launch::{AutostartFile, FileError, LaunchCommand, RefreshMode, Snapshot};
use std::{path::PathBuf, sync::Arc};
#[derive(Clone, Debug)]
pub struct DesktopContext {
    pub(super) file: AutostartFile,
    command: LaunchCommand,
    refresh: RefreshMode,
}
impl DesktopContext {
    /// Pure path validation only. The caller explicitly selects a trusted native
    /// executable/bundle and config root; no environment or artifact guessing.
    /// Generated login commands retain that same paired management authority.
    pub fn new(
        root: PathBuf,
        command: LaunchCommand,
        refresh: RefreshMode,
    ) -> Result<Self, FileError> {
        let file = AutostartFile::new(root.clone())?;
        let command = command
            .with_management_context(root)
            .map_err(FileError::Command)?;
        Ok(Self {
            file,
            command,
            refresh,
        })
    }
    pub(super) fn execute(&self, operation: DesktopOperation) -> DesktopResult {
        match operation {
            DesktopOperation::Inspect => self.file.load().map(|snapshot| DesktopReply {
                snapshot,
                changed: false,
            }),
            DesktopOperation::Enable(from) => {
                self.file
                    .enable(&from, &self.command)
                    .map(|r| DesktopReply {
                        snapshot: r.snapshot,
                        changed: r.changed,
                    })
            }
            DesktopOperation::Disable(from) => self.file.disable(&from).map(|r| DesktopReply {
                snapshot: r.snapshot,
                changed: r.changed,
            }),
            DesktopOperation::Refresh(from) => self
                .file
                .refresh(&from, &self.command, self.refresh)
                .map(|r| DesktopReply {
                    snapshot: r.snapshot,
                    changed: r.changed,
                }),
        }
        .map_err(Arc::new)
    }
}
#[derive(Debug)]
pub enum DesktopOperation {
    Inspect,
    Enable(Snapshot),
    Disable(Snapshot),
    Refresh(Snapshot),
}
#[derive(Clone, Debug)]
pub struct DesktopReply {
    pub snapshot: Snapshot,
    pub changed: bool,
}
pub type DesktopResult = Result<DesktopReply, Arc<FileError>>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_context_login_roundtrip_retains_exact_explicit_controls_and_paths() {
        let root = PathBuf::from("/synthetic/authorized config %U");
        let executable = PathBuf::from("/synthetic/trusted native.AppImage");
        let settings = Some(PathBuf::from("/synthetic/settings.json"));
        let recordings = Some(PathBuf::from("/synthetic/recordings"));
        let context = DesktopContext::new(
            root.clone(),
            LaunchCommand::new(executable.clone())
                .unwrap()
                .with_paths(settings.clone(), recordings.clone())
                .unwrap(),
            RefreshMode::Release,
        )
        .unwrap();
        let options = crate::startup::Options::parse(
            context
                .command
                .argv(crate::desktop_launch::LaunchMode::Hidden)
                .into_iter()
                .skip(1)
                .map(Into::into),
        )
        .unwrap()
        .unwrap();
        assert_eq!(options.autostart_config_root, Some(root.clone()));
        assert_eq!(options.native_executable, Some(executable));
        assert_eq!(options.settings, settings);
        assert_eq!(options.recording_root, recordings);
        assert!(options.hidden && options.tray);
        assert_eq!(context.command.management_root(), Some(root.as_path()));
        let wrong = LaunchCommand::new("/synthetic/native")
            .unwrap()
            .with_management_context("/synthetic/other".into())
            .unwrap();
        assert!(matches!(
            DesktopContext::new(root, wrong, RefreshMode::Release),
            Err(FileError::Command(
                crate::desktop_launch::CommandError::ManagementContextMismatch
            ))
        ));
    }
}
