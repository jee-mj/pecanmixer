//! Explicit-path, serialized off-UI persistence. No startup/profile discovery or
//! audio effects. Submission is admission only; consume the matching result.
//! Both queues are bounded. Drain results before joining off-UI on shutdown;
//! dropping this handle detaches and does not promise queued saves completed.
mod desktop;
use crate::settings_storage::{LoadedSettings, RecoveredSettings, SettingsFile, StorageError};
pub use desktop::{DesktopContext, DesktopOperation, DesktopReply, DesktopResult};
use pecan_core::settings::SettingsV1;
use std::{
    io,
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
};

pub const SETTINGS_QUEUE_CAPACITY: usize = 16;
#[derive(Debug)]
pub enum SettingsOperation {
    Desktop(DesktopOperation),
    Reload,
    Save(SettingsV1),
    /// Recover only the exact input snapshot the user explicitly reviewed.
    Recover {
        from: LoadedSettings,
        settings: SettingsV1,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionFailure {
    QueueFull,
    Stopped,
    IdExhausted,
    /// The application has not completed its initial preferences load.
    NotReady,
}
#[derive(Debug)]
pub struct SubmissionError {
    pub reason: SubmissionFailure,
    /// Retained ownership lets the caller retry its latest desired document.
    pub operation: SettingsOperation,
}
#[derive(Debug)]
pub enum SettingsEvent {
    Desktop {
        request: u64,
        result: DesktopResult,
    },
    /// The initial explicit load has request 0; later reloads have their own ID.
    Loaded {
        request: u64,
        snapshot: LoadedSettings,
    },
    Saved {
        request: u64,
        snapshot: LoadedSettings,
    },
    Recovered {
        request: u64,
        snapshot: LoadedSettings,
        backup: PathBuf,
    },
    Failed {
        request: u64,
        error: Arc<StorageError>,
    },
    /// No I/O occurred. An earlier error blocks saves until an explicit reload
    /// or successful explicit recovery. The original error remains available.
    Blocked {
        request: u64,
        error: Arc<StorageError>,
    },
}
impl SettingsEvent {
    pub fn request(&self) -> u64 {
        match self {
            Self::Desktop { request, .. }
            | Self::Loaded { request, .. }
            | Self::Saved { request, .. }
            | Self::Recovered { request, .. }
            | Self::Failed { request, .. }
            | Self::Blocked { request, .. } => *request,
        }
    }
}
struct Work {
    request: u64,
    operation: SettingsOperation,
}
pub struct SettingsWorker {
    sender: SyncSender<Work>,
    events: Receiver<SettingsEvent>,
    thread: JoinHandle<()>,
    next: u64,
    preferences: bool,
    desktop: bool,
}
impl SettingsWorker {
    /// Starting this explicit worker performs its initial load on that worker,
    /// not the caller. SettingsFile validates paths but chooses no directory.
    pub fn start(file: SettingsFile) -> io::Result<Self> {
        Self::start_store(file)
    }
    pub fn start_with_desktop(
        file: Option<SettingsFile>,
        context: DesktopContext,
    ) -> io::Result<Self> {
        if file
            .as_ref()
            .is_some_and(|f| f.path() == context.file.path())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Settings and desktop entry must use separate files.",
            ));
        }
        Self::start_parts(file.map(|f| Box::new(f) as Box<dyn Store>), Some(context))
    }
    pub fn supports_preferences(&self) -> bool {
        self.preferences
    }
    pub fn supports_desktop(&self) -> bool {
        self.desktop
    }
    fn start_store(store: impl Store) -> io::Result<Self> {
        Self::start_parts(Some(Box::new(store)), None)
    }
    fn start_parts(
        store: Option<Box<dyn Store>>,
        desktop: Option<DesktopContext>,
    ) -> io::Result<Self> {
        let preferences = store.is_some();
        let has_desktop = desktop.is_some();
        let (sender, requests) = mpsc::sync_channel(SETTINGS_QUEUE_CAPACITY);
        let (events_tx, events) = mpsc::sync_channel(SETTINGS_QUEUE_CAPACITY);
        let thread = thread::Builder::new()
            .name("pecanmixer-settings".into())
            .spawn(move || run(store, desktop, requests, events_tx))?;
        Ok(Self {
            sender,
            events,
            thread,
            next: 0,
            preferences,
            desktop: has_desktop,
        })
    }
    pub fn submit(&mut self, operation: SettingsOperation) -> Result<u64, SubmissionError> {
        let supported = match &operation {
            SettingsOperation::Desktop(_) => self.desktop,
            _ => self.preferences,
        };
        if !supported {
            return Err(SubmissionError {
                reason: SubmissionFailure::NotReady,
                operation,
            });
        }
        let Some(request) = self.next.checked_add(1) else {
            return Err(SubmissionError {
                reason: SubmissionFailure::IdExhausted,
                operation,
            });
        };
        match self.sender.try_send(Work { request, operation }) {
            Ok(()) => {
                self.next = request;
                Ok(request)
            }
            Err(TrySendError::Full(work)) => Err(SubmissionError {
                reason: SubmissionFailure::QueueFull,
                operation: work.operation,
            }),
            Err(TrySendError::Disconnected(work)) => Err(SubmissionError {
                reason: SubmissionFailure::Stopped,
                operation: work.operation,
            }),
        }
    }
    pub fn try_event(&self) -> Result<SettingsEvent, TryRecvError> {
        self.events.try_recv()
    }
    /// Stops accepting work. Continue draining these bounded results before
    /// joining the returned thread off-UI; an undrained result queue can be full.
    pub fn close(self) -> (Receiver<SettingsEvent>, JoinHandle<()>) {
        let Self {
            sender,
            events,
            thread,
            ..
        } = self;
        drop(sender);
        (events, thread)
    }
}
trait Store: Send + 'static {
    fn load(&self) -> Result<LoadedSettings, StorageError>;
    fn save(
        &self,
        from: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<LoadedSettings, StorageError>;
    fn recover(
        &self,
        from: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<RecoveredSettings, StorageError>;
}
impl Store for SettingsFile {
    fn load(&self) -> Result<LoadedSettings, StorageError> {
        SettingsFile::load(self)
    }
    fn save(
        &self,
        from: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<LoadedSettings, StorageError> {
        SettingsFile::save(self, from, next)
    }
    fn recover(
        &self,
        from: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<RecoveredSettings, StorageError> {
        SettingsFile::recover(self, from, next)
    }
}
fn run(
    store: Option<Box<dyn Store>>,
    desktop: Option<DesktopContext>,
    requests: Receiver<Work>,
    events: SyncSender<SettingsEvent>,
) {
    let mut current = store.as_ref().map(|s| s.load().map_err(Arc::new));
    if let Some(initial) = &current {
        let initial = match initial {
            Ok(snapshot) => SettingsEvent::Loaded {
                request: 0,
                snapshot: snapshot.clone(),
            },
            Err(error) => SettingsEvent::Failed {
                request: 0,
                error: error.clone(),
            },
        };
        if events.send(initial).is_err() {
            return;
        }
    }
    for Work { request, operation } in requests {
        let event = match operation {
            SettingsOperation::Desktop(operation) => SettingsEvent::Desktop {
                request,
                result: desktop
                    .as_ref()
                    .expect("desktop admission checked")
                    .execute(operation),
            },
            operation => {
                let store = store.as_ref().expect("preference admission checked");
                let current = current.as_mut().expect("preference baseline");
                match operation {
                    SettingsOperation::Reload => match store.load() {
                        Ok(snapshot) => {
                            *current = Ok(snapshot.clone());
                            SettingsEvent::Loaded { request, snapshot }
                        }
                        Err(error) => failed(request, error, current),
                    },
                    SettingsOperation::Save(next) => match current {
                        Err(error) => SettingsEvent::Blocked {
                            request,
                            error: error.clone(),
                        },
                        Ok(from) => match store.save(from, &next) {
                            Ok(snapshot) => {
                                *current = Ok(snapshot.clone());
                                SettingsEvent::Saved { request, snapshot }
                            }
                            Err(error) => failed(request, error, current),
                        },
                    },
                    SettingsOperation::Recover { from, settings } => {
                        match store.recover(&from, &settings) {
                            Ok(recovered) => {
                                *current = Ok(recovered.loaded.clone());
                                SettingsEvent::Recovered {
                                    request,
                                    snapshot: recovered.loaded,
                                    backup: recovered.backup,
                                }
                            }
                            Err(error) => failed(request, error, current),
                        }
                    }
                    SettingsOperation::Desktop(_) => unreachable!(),
                }
            }
        };
        // Only this dedicated I/O worker can block on the bounded result queue.
        if events.send(event).is_err() {
            break;
        }
    }
}

fn failed(
    request: u64,
    error: StorageError,
    current: &mut Result<LoadedSettings, Arc<StorageError>>,
) -> SettingsEvent {
    let error = Arc::new(error);
    *current = Err(error.clone());
    SettingsEvent::Failed { request, error }
}

#[cfg(test)]
mod tests;

// Fault injection stays in synthetic unit builds; production callers receive
// only the ordinary explicit-path worker constructor.
#[cfg(test)]
impl SettingsWorker {
    pub(crate) fn fixture(
        file: SettingsFile,
        load_gate: Option<(SyncSender<()>, Receiver<()>)>,
        save_gate: Option<(SyncSender<()>, Receiver<()>)>,
        fail_after_publish: bool,
    ) -> io::Result<Self> {
        struct Fixture {
            file: SettingsFile,
            load: std::sync::Mutex<Option<(SyncSender<()>, Receiver<()>)>>,
            save: std::sync::Mutex<Option<(SyncSender<()>, Receiver<()>)>>,
            fail: std::sync::atomic::AtomicBool,
        }
        impl Store for Fixture {
            fn load(&self) -> Result<LoadedSettings, StorageError> {
                if let Some((entered, release)) = self.load.lock().unwrap().take() {
                    let _ = entered.send(());
                    let _ = release.recv();
                }
                self.file.load()
            }
            fn save(
                &self,
                from: &LoadedSettings,
                next: &SettingsV1,
            ) -> Result<LoadedSettings, StorageError> {
                if let Some((entered, release)) = self.save.lock().unwrap().take() {
                    let _ = entered.send(());
                    let _ = release.recv();
                }
                let saved = self.file.save(from, next)?;
                if self.fail.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    return Err(StorageError::Io {
                        phase: crate::settings_storage::WritePhase::SyncDirectory,
                        path: self.file.path().to_path_buf(),
                        published: true,
                        recovery_backup: None,
                        source: io::Error::other("Synthetic post-rename failure"),
                    });
                }
                Ok(saved)
            }
            fn recover(
                &self,
                from: &LoadedSettings,
                next: &SettingsV1,
            ) -> Result<RecoveredSettings, StorageError> {
                self.file.recover(from, next)
            }
        }
        Self::start_store(Fixture {
            file,
            load: std::sync::Mutex::new(load_gate),
            save: std::sync::Mutex::new(save_gate),
            fail: std::sync::atomic::AtomicBool::new(fail_after_publish),
        })
    }
}
