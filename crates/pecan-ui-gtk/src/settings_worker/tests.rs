//! Fresh synthetic temporary files only; no profile/browser or audio access.
use super::*;
use crate::settings_storage::WritePhase;
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pecan-display-store-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self) -> SettingsFile {
        SettingsFile::new(self.0.join("synthetic.json")).unwrap()
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn settings(width: f64) -> SettingsV1 {
    let mut p = SettingsV1::default().into_preferences();
    p.destination_width_px = Some(width);
    SettingsV1::new(p).unwrap()
}
fn event(worker: &SettingsWorker) -> SettingsEvent {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match worker.try_event() {
            Ok(event) => return event,
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(error) => panic!("settings worker result unavailable: {error:?}"),
        }
    }
}
fn close(worker: SettingsWorker) {
    let (events, thread) = worker.close();
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(3)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    thread.join().unwrap();
}
struct GatedStore {
    file: SettingsFile,
    entered: SyncSender<()>,
    release: Receiver<()>,
    first: AtomicBool,
    calls: Arc<Mutex<Vec<thread::ThreadId>>>,
}
impl Store for GatedStore {
    fn load(&self) -> Result<LoadedSettings, StorageError> {
        self.calls.lock().unwrap().push(thread::current().id());
        if self.first.swap(false, Ordering::SeqCst) {
            self.entered.send(()).unwrap();
            self.release.recv().unwrap();
        }
        self.file.load()
    }
    fn save(
        &self,
        from: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<LoadedSettings, StorageError> {
        self.calls.lock().unwrap().push(thread::current().id());
        self.file.save(from, next)
    }
    fn recover(
        &self,
        from: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<RecoveredSettings, StorageError> {
        self.calls.lock().unwrap().push(thread::current().id());
        self.file.recover(from, next)
    }
}
#[test]
fn rapid_changes_are_serial_off_ui_bounded_and_retry_preserves_latest_document() {
    let dir = TestDir::new();
    let file = dir.file();
    let (entered_tx, entered) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    let calls = Arc::new(Mutex::new(vec![]));
    let caller = thread::current().id();
    let mut worker = SettingsWorker::start_store(GatedStore {
        file: file.clone(),
        entered: entered_tx,
        release: release_rx,
        first: AtomicBool::new(true),
        calls: calls.clone(),
    })
    .unwrap();
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    for i in 1..=SETTINGS_QUEUE_CAPACITY {
        assert_eq!(
            worker
                .submit(SettingsOperation::Save(settings(200.0 + i as f64)))
                .unwrap(),
            i as u64
        );
    }
    let retry = worker
        .submit(SettingsOperation::Save(settings(299.0)))
        .unwrap_err();
    assert_eq!(retry.reason, SubmissionFailure::QueueFull);
    release.send(()).unwrap();
    assert!(matches!(
        event(&worker),
        SettingsEvent::Loaded { request: 0, .. }
    ));
    for i in 1..=SETTINGS_QUEUE_CAPACITY {
        let SettingsEvent::Saved { request, snapshot } = event(&worker) else {
            panic!("save")
        };
        assert_eq!(request, i as u64);
        assert_eq!(
            snapshot.settings().preferences().destination_width_px,
            Some(200.0 + i as f64)
        );
    }
    assert_eq!(worker.submit(retry.operation).unwrap(), 17);
    // Close admission with a save still queued; draining results is required
    // before an off-UI join can establish that accepted writes finished.
    let (events, thread) = worker.close();
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(3)).unwrap(),
        SettingsEvent::Saved { request: 17, .. }
    ));
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(3)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    thread.join().unwrap();
    assert_eq!(file.load().unwrap().settings(), &settings(299.0));
    let threads = calls.lock().unwrap();
    assert!(threads.iter().all(|id| *id != caller && *id == threads[0]));
}
#[test]
fn external_edits_block_queued_saves_until_explicit_reload() {
    let dir = TestDir::new();
    let file = dir.file();
    let mut worker = SettingsWorker::start(file.clone()).unwrap();
    assert!(matches!(
        event(&worker),
        SettingsEvent::Loaded { request: 0, .. }
    ));
    fs::write(file.path(), serde_json::to_vec(&settings(333.0)).unwrap()).unwrap();
    worker
        .submit(SettingsOperation::Save(settings(201.0)))
        .unwrap();
    let SettingsEvent::Failed { error: first, .. } = event(&worker) else {
        panic!("changed content must fail")
    };
    assert!(matches!(*first, StorageError::ChangedOnDisk));
    worker
        .submit(SettingsOperation::Save(settings(202.0)))
        .unwrap();
    let SettingsEvent::Blocked { error, .. } = event(&worker) else {
        panic!("no blind retry")
    };
    assert!(Arc::ptr_eq(&error, &first));
    assert_eq!(file.load().unwrap().settings(), &settings(333.0));
    worker.submit(SettingsOperation::Reload).unwrap();
    let SettingsEvent::Loaded { snapshot, .. } = event(&worker) else {
        panic!("reload")
    };
    assert_eq!(snapshot.settings(), &settings(333.0));
    worker
        .submit(SettingsOperation::Save(settings(204.0)))
        .unwrap();
    assert!(matches!(event(&worker), SettingsEvent::Saved { .. }));
    assert_eq!(file.load().unwrap().settings(), &settings(204.0));
    close(worker);
}
#[test]
fn corrupt_input_requires_explicit_recovery_of_the_reviewed_bytes() {
    let dir = TestDir::new();
    let file = dir.file();
    let raw = vec![0xff, 0x00, 0xfe];
    fs::write(file.path(), &raw).unwrap();
    let mut worker = SettingsWorker::start(file.clone()).unwrap();
    let SettingsEvent::Loaded {
        snapshot: original, ..
    } = event(&worker)
    else {
        panic!("load")
    };
    assert!(original.recovery_required());
    worker
        .submit(SettingsOperation::Save(settings(201.0)))
        .unwrap();
    assert!(
        matches!(event(&worker),SettingsEvent::Failed{error,..} if matches!(*error,StorageError::RecoveryRequired(_)))
    );
    assert_eq!(fs::read(file.path()).unwrap(), raw);
    fs::write(file.path(), b"{new synthetic invalid input").unwrap();
    worker
        .submit(SettingsOperation::Recover {
            from: original,
            settings: settings(202.0),
        })
        .unwrap();
    assert!(
        matches!(event(&worker),SettingsEvent::Failed{error,..} if matches!(*error,StorageError::ChangedOnDisk))
    );
    worker.submit(SettingsOperation::Reload).unwrap();
    let SettingsEvent::Loaded {
        snapshot: reviewed, ..
    } = event(&worker)
    else {
        panic!("reload")
    };
    worker
        .submit(SettingsOperation::Recover {
            from: reviewed,
            settings: settings(203.0),
        })
        .unwrap();
    let SettingsEvent::Recovered {
        backup, snapshot, ..
    } = event(&worker)
    else {
        panic!("recover")
    };
    assert_eq!(fs::read(backup).unwrap(), b"{new synthetic invalid input");
    assert_eq!(snapshot.settings(), &settings(203.0));
    close(worker);
}
struct PublishedOnce {
    file: SettingsFile,
    fail: AtomicBool,
    calls: Arc<AtomicUsize>,
}
impl Store for PublishedOnce {
    fn load(&self) -> Result<LoadedSettings, StorageError> {
        self.file.load()
    }
    fn save(
        &self,
        from: &LoadedSettings,
        next: &SettingsV1,
    ) -> Result<LoadedSettings, StorageError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let saved = self.file.save(from, next)?;
        if self.fail.swap(false, Ordering::SeqCst) {
            return Err(StorageError::Io {
                phase: WritePhase::SyncDirectory,
                path: self.file.path().to_owned(),
                published: true,
                recovery_backup: None,
                source: io::Error::other("synthetic post-replace error"),
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
#[test]
fn post_rename_error_requires_reload_before_another_save() {
    let dir = TestDir::new();
    let file = dir.file();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = SettingsWorker::start_store(PublishedOnce {
        file: file.clone(),
        fail: AtomicBool::new(true),
        calls: calls.clone(),
    })
    .unwrap();
    event(&worker);
    worker
        .submit(SettingsOperation::Save(settings(201.0)))
        .unwrap();
    assert!(matches!(event(&worker),SettingsEvent::Failed{error,..} if error.published()));
    assert_eq!(file.load().unwrap().settings(), &settings(201.0));
    worker
        .submit(SettingsOperation::Save(settings(202.0)))
        .unwrap();
    assert!(matches!(event(&worker), SettingsEvent::Blocked { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    worker.submit(SettingsOperation::Reload).unwrap();
    let SettingsEvent::Loaded { snapshot, .. } = event(&worker) else {
        panic!("reload")
    };
    assert_eq!(snapshot.settings(), &settings(201.0));
    worker
        .submit(SettingsOperation::Save(settings(203.0)))
        .unwrap();
    assert!(matches!(event(&worker), SettingsEvent::Saved { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(file.load().unwrap().settings(), &settings(203.0));
    close(worker);
}
