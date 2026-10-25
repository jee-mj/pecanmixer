//! Desktop-entry generation and opt-in, synchronous autostart file operations.
//!
//! No environment/profile discovery, process execution or startup hook lives here.
//! Call I/O off the UI thread; serialize writers and retain the loaded snapshot.
//! See README.md for the legacy compatibility and filesystem trust boundaries.
mod storage;
pub use storage::{AutostartFile, EntryFormat, EntryState, FileError, Phase, Snapshot, Update};

use std::path::{Path, PathBuf};

/// Distinct from the preserved legacy application and its personal autostart.
pub const DESKTOP_FILENAME: &str = "nl.jetse.pecanmixer.Native.desktop";
pub const LEGACY_DESKTOP_FILENAME: &str = "nl.jetse.pecanmixer.desktop";
const HEADER: &str = "[Desktop Entry]\nType=Application\nName=Pecan Mixer (Native)\nComment=Patchbay and mixer for PipeWire\nExec=";
const LEGACY_HEADER: &str = "[Desktop Entry]\nType=Application\nName=Pecan Mixer\nComment=Patchbay and mixer for PipeWire\nExec=";
const LEGACY_FOOTER: &str = "\nIcon=audio-card\nTerminal=false\nCategories=AudioVideo;Audio;Mixer;\nX-GNOME-Autostart-enabled=true\nX-KDE-autostart-after=panel\n";
const FOOTER: &str = "\nIcon=audio-card\nTerminal=false\nCategories=AudioVideo;Audio;Mixer;\nX-GNOME-Autostart-enabled=true\nX-KDE-autostart-after=panel\nX-PecanMixer-Native-Autostart=1\n";
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
        write!(f, "cannot represent desktop launch command: {self:?}")
    }
}
impl std::error::Error for CommandError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchCommand {
    executable: PathBuf,
    settings: Option<PathBuf>,
    recording_root: Option<PathBuf>,
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
        Ok(Self {
            executable,
            settings: None,
            recording_root: None,
        })
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    /// Only the native entrypoint's two explicit path arguments. No environment,
    /// profile/default path, existence check, or arbitrary command is inferred.
    /// Omission preserves native memory-only settings / unselected recording root.
    pub fn with_paths(
        mut self,
        settings: Option<PathBuf>,
        recording_root: Option<PathBuf>,
    ) -> Result<Self, CommandError> {
        for (path, file) in [(settings.as_ref(), true), (recording_root.as_ref(), false)] {
            if let Some(path) = path {
                if !path.is_absolute() {
                    return Err(CommandError::NotAbsolute);
                }
                if file && path.file_name().is_none() {
                    return Err(CommandError::NoFileName);
                }
                let text = path.to_str().ok_or(CommandError::NotUtf8)?;
                if text.chars().any(char::is_control) {
                    return Err(CommandError::ControlCharacter);
                }
                if text.len() > MAX_PATH_BYTES {
                    return Err(CommandError::TooLong);
                }
            }
        }
        self.settings = settings;
        self.recording_root = recording_root;
        // Escaped arguments can expand to four bytes per input byte. Never
        // publish a command that our bounded reader cannot subsequently load.
        if self.autostart_entry().len() > storage::MAX_ENTRY_BYTES as usize {
            return Err(CommandError::TooLong);
        }
        Ok(self)
    }
    pub fn settings(&self) -> Option<&Path> {
        self.settings.as_deref()
    }
    pub fn recording_root(&self) -> Option<&Path> {
        self.recording_root.as_deref()
    }
    /// Argument vector for inspection/adaptation, not a shell command to execute.
    pub fn argv(&self, mode: LaunchMode) -> Vec<String> {
        let mut args = vec![self.text().to_owned()];
        if let Some(settings) = &self.settings {
            args.extend([
                "--settings".into(),
                settings.to_str().expect("validated path").into(),
            ]);
        }
        if let Some(root) = &self.recording_root {
            args.extend([
                "--recording-root".into(),
                root.to_str().expect("validated path").into(),
            ]);
        }
        if mode == LaunchMode::Hidden {
            args.push("--hidden".into());
            args.push("--tray".into());
        }
        args
    }
    /// Raw Exec= value for a desktop FILE. Both escaping layers are applied:
    /// quoted Exec argument first, then the desktop string's backslash escaping.
    /// Executable paths containing '%' are rejected at construction because of
    /// GIO's preflight behavior. Percent characters in path arguments become
    /// literal %% escapes, never launcher field-code requests.
    pub fn exec_value(&self, mode: LaunchMode) -> String {
        let quoted = self
            .argv(mode)
            .iter()
            .map(|arg| {
                // All supplied paths are absolute. Only fixed flags stay unquoted.
                if arg.starts_with("--") {
                    arg.clone()
                } else {
                    quote_argument(arg)
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        quoted.replace('\\', "\\\\")
    }
    /// Native-only identity/template. Hidden login launch explicitly requests
    /// the native tray service; its existing no-host visibility policy applies.
    pub fn autostart_entry(&self) -> String {
        format!("{HEADER}{}{FOOTER}", self.exec_value(LaunchMode::Hidden))
    }
    fn text(&self) -> &str {
        self.executable
            .to_str()
            .expect("validated UTF-8 executable")
    }
}

fn quote_argument(text: &str) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

// Exact current native templates alone are mutable. Historical legacy bytes
// may be recognized for explanation, but are protected even at our filename.
fn entry_format(bytes: &[u8]) -> Option<EntryFormat> {
    if bytes.len() > storage::MAX_ENTRY_BYTES as usize {
        return None;
    }
    if native_command(bytes).is_some() {
        return Some(EntryFormat::Native);
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let raw = text
        .strip_prefix(LEGACY_HEADER)?
        .strip_suffix(LEGACY_FOOTER)?;
    let path = unquote_executable(raw, false)?;
    let command = LaunchCommand::new(path).ok()?;
    (legacy_entry(command.text()) == text).then_some(EntryFormat::Legacy)
}

fn native_command(bytes: &[u8]) -> Option<LaunchCommand> {
    if bytes.len() > storage::MAX_ENTRY_BYTES as usize {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let raw = text.strip_prefix(HEADER)?.strip_suffix(FOOTER)?;
    let command = parse_native(raw)?;
    (command.autostart_entry() == text).then_some(command)
}

fn parse_native(raw: &str) -> Option<LaunchCommand> {
    let decoded = unescape_value(raw)?;
    let argv = gtk::glib::shell_parse_argv(&decoded).ok()?;
    let argv = argv
        .iter()
        .map(|s| s.to_str())
        .collect::<Option<Vec<_>>>()?;
    if !(3..=7).contains(&argv.len()) || argv[argv.len() - 2..] != ["--hidden", "--tray"] {
        return None;
    }
    let command = LaunchCommand::new(unescape_percent(argv[0])?).ok()?;
    let (mut settings, mut recording_root) = (None, None);
    let mut i = 1;
    while i < argv.len() - 2 {
        let value = PathBuf::from(unescape_percent(*argv.get(i + 1)?)?);
        match argv[i] {
            "--settings" if settings.is_none() && recording_root.is_none() => {
                settings = Some(value)
            }
            "--recording-root" if recording_root.is_none() => recording_root = Some(value),
            _ => return None,
        }
        i += 2;
    }
    command.with_paths(settings, recording_root).ok()
}
fn unescape_percent(value: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '%' && chars.next()? != '%' {
            return None;
        }
        out.push(c);
    }
    Some(out)
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
    format!("{LEGACY_HEADER}{quoted}{LEGACY_FOOTER}")
}

#[cfg(test)]
mod tests;
