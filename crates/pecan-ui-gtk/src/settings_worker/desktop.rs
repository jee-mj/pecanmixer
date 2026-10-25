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
    pub fn new(
        root: PathBuf,
        command: LaunchCommand,
        refresh: RefreshMode,
    ) -> Result<Self, FileError> {
        Ok(Self {
            file: AutostartFile::new(root)?,
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
