//! Desktop-entry generation and opt-in, synchronous autostart file operations.
//!
//! No environment/profile discovery, process execution or startup hook lives here.
//! Call I/O off the UI thread; serialize writers and retain the loaded snapshot.
//! See README.md for the legacy compatibility and filesystem trust boundaries.
mod storage;
pub use storage::{AutostartFile, EntryFormat, EntryState, FileError, Phase, Snapshot, Update};

use std::path::{Path, PathBuf};

/// Existing application identity, unchanged pending native packaging adoption.
pub const DESKTOP_FILENAME: &str = "nl.jetse.pecanmixer.desktop";
const HEADER: &str = "[Desktop Entry]\nType=Application\nName=Pecan Mixer\nComment=Patchbay and mixer for PipeWire\nExec=";
const FOOTER: &str = "\nIcon=audio-card\nTerminal=false\nCategories=AudioVideo;Audio;Mixer;\nX-GNOME-Autostart-enabled=true\nX-KDE-autostart-after=panel\n";
const MAX_PATH_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    Visible,
    Hidden,
}

/// The caller supplies its build mode; this module never infers it from the
/// library's compilation profile or performs an automatic startup refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshMode {
    Debug,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    NotAbsolute,
    NoFileName,
    NotUtf8,
    ControlCharacter,
    EqualSign,
    PercentInExecutable,
    TooLong,
}
impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot represent desktop executable: {self:?}")
    }
}
impl std::error::Error for CommandError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchCommand {
    executable: PathBuf,
}
impl LaunchCommand {
    /// APPIMAGE takes precedence over the temporary mounted current executable.
    /// An invalid supplied APPIMAGE is an error, never a fallback to the mount.
    /// No existence, executable-bit or canonicalization checks are implicit.
    pub fn resolve(current_exe: &Path, appimage: Option<&Path>) -> Result<Self, CommandError> {
        Self::new(appimage.unwrap_or(current_exe))
    }
    pub fn new(executable: impl Into<PathBuf>) -> Result<Self, CommandError> {
        let executable = executable.into();
        if !executable.is_absolute() {
            return Err(CommandError::NotAbsolute);
        }
        if executable.file_name().is_none() {
            return Err(CommandError::NoFileName);
        }
        let text = executable.to_str().ok_or(CommandError::NotUtf8)?;
        if text.chars().any(char::is_control) {
            return Err(CommandError::ControlCharacter);
        }
        // The Desktop Entry specification disallows '=' in executable paths.
        if text.contains('=') {
            return Err(CommandError::EqualSign);
        }
        // Desktop syntax requires %% for a literal %. GIO preflights argv[0]
        // before expansion, and therefore rejects that escaped executable even
        // when the real file exists. Do not silently write a nonworking entry
        // or introduce an implicit /usr/bin/env wrapper dependency.
        if text.contains('%') {
            return Err(CommandError::PercentInExecutable);
        }
        if text.len() > MAX_PATH_BYTES {
            return Err(CommandError::TooLong);
        }
        Ok(Self { executable })
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    /// Argument vector for inspection/adaptation, not a shell command to execute.
    pub fn argv(&self, mode: LaunchMode) -> Vec<String> {
        let mut args = vec![self.text().to_owned()];
        if mode == LaunchMode::Hidden {
            args.push("--hidden".into());
        }
        args
    }
    /// Raw Exec= value for a desktop FILE. Both escaping layers are applied:
    /// quoted Exec argument first, then the desktop string's backslash escaping.
    /// Executable paths containing '%' are rejected at construction because of
    /// GIO's preflight behavior; no launcher field codes can be introduced.
    pub fn exec_value(&self, mode: LaunchMode) -> String {
        let mut quoted = String::from("\"");
        for c in self.text().chars() {
            match c {
                '"' | '`' | '$' | '\\' => {
                    quoted.push('\\');
                    quoted.push(c);
                }
                _ => quoted.push(c),
            }
        }
        quoted.push('"');
        if mode == LaunchMode::Hidden {
            quoted.push_str(" --hidden");
        }
        quoted.replace('\\', "\\\\")
    }
    /// Retains the original Name/Icon/categories/vendor keys and --hidden.
    pub fn autostart_entry(&self) -> String {
        format!("{HEADER}{}{FOOTER}", self.exec_value(LaunchMode::Hidden))
    }
    fn text(&self) -> &str {
        self.executable
            .to_str()
            .expect("validated UTF-8 executable")
    }
}

// Recognize complete, exact generator templates, including the legacy writer.
// Unknown/edited desktop entries (even at our basename) require caller review.
fn entry_format(bytes: &[u8]) -> Option<EntryFormat> {
    let text = std::str::from_utf8(bytes).ok()?;
    let raw = text.strip_prefix(HEADER)?.strip_suffix(FOOTER)?;
    for format in [EntryFormat::Native, EntryFormat::Legacy] {
        let decoded = match format {
            EntryFormat::Native => match unescape_value(raw) {
                Some(value) => value,
                None => continue,
            },
            EntryFormat::Legacy => raw.to_owned(),
        };
        if let Some(path) = unquote_executable(&decoded, format == EntryFormat::Native) {
            if let Ok(command) = LaunchCommand::new(path) {
                let expected = match format {
                    EntryFormat::Native => command.autostart_entry(),
                    EntryFormat::Legacy => legacy_entry(command.text()),
                };
                if expected == text {
                    return Some(format);
                }
            }
        }
    }
    None
}

fn unescape_value(raw: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '\\' {
            match chars.next()? {
                '\\' => '\\',
                's' => ' ',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                _ => return None,
            }
        } else {
            c
        });
    }
    Some(out)
}

fn unquote_executable(raw: &str, percent_escaped: bool) -> Option<String> {
    let raw = raw.strip_prefix('"')?.strip_suffix("\" --hidden")?;
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        out.push(match c {
            '\\' => match chars.next()? {
                c @ ('"' | '`' | '$' | '\\') => c,
                _ => return None,
            },
            '"' | '`' | '$' => return None,
            '%' if percent_escaped => {
                if chars.next()? != '%' {
                    return None;
                }
                '%'
            }
            _ => c,
        });
    }
    Some(out)
}

fn legacy_entry(path: &str) -> String {
    let mut quoted = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push_str("\" --hidden");
    format!("{HEADER}{quoted}{FOOTER}")
}

#[cfg(test)]
mod tests;
