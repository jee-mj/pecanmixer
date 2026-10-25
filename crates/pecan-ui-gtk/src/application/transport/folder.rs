//! Explicit-path I/O boundary. Call off the GUI thread after owner validation.
//! This module never discovers a profile or starts a file-manager process.
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderPaths {
    root: PathBuf,
    latest: Option<PathBuf>,
}
#[derive(Clone, Debug)]
pub struct FolderPlan {
    requested: FolderPaths,
    target: PathBuf,
}
impl FolderPaths {
    pub fn new(root: PathBuf, latest: Option<PathBuf>) -> io::Result<Self> {
        if !valid(&root) || latest.as_ref().is_some_and(|p| !valid(p)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "explicit absolute recording paths required",
            ));
        }
        Ok(Self { root, latest })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn latest(&self) -> Option<&Path> {
        self.latest.as_deref()
    }
    /// Match the legacy folder fallback within the caller's explicit root.
    /// Only that root may be created. A disappeared/escaping/non-directory take
    /// falls back to root; root preparation errors remain errors.
    pub fn prepare(&self) -> io::Result<FolderPlan> {
        std::fs::create_dir_all(&self.root)?;
        let root = self.root.canonicalize()?;
        let target = self
            .latest
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .filter(|p| p.is_dir() && p.starts_with(&root))
            .unwrap_or(root);
        Ok(FolderPlan {
            requested: self.clone(),
            target,
        })
    }
}
impl FolderPlan {
    pub fn requested(&self) -> &FolderPaths {
        &self.requested
    }
    pub fn target(&self) -> &Path {
        &self.target
    }
    pub fn program(&self) -> &'static str {
        "xdg-open"
    }
    /// Pass as one OS argument, never shell text. Launch/reaping is owner work.
    pub fn argument(&self) -> &std::ffi::OsStr {
        self.target.as_os_str()
    }
}
pub(super) fn valid(path: &Path) -> bool {
    let bytes = path.as_os_str().as_encoded_bytes();
    path.is_absolute() && bytes.len() <= super::MAX_PATH && !bytes.contains(&0)
}
