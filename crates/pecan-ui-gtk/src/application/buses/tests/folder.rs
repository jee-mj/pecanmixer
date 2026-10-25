//! Injected preparation/launcher; all filesystem work uses synthetic temp roots.
use super::*;
use crate::application::{
    folders::{Folders, Io},
    transport::{FolderPaths, FolderPlan},
    NativeWorkspace,
};
use std::sync::{mpsc, Arc, Mutex};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "pecan-folder-owner-synthetic-{}-{}",
            std::process::id(),
            APP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[derive(Default)]
struct FakeIo {
    prepared: AtomicUsize,
    launched: Mutex<Vec<FolderPlan>>,
    threads: Mutex<Vec<std::thread::ThreadId>>,
    prepare_gate: Mutex<Option<mpsc::Receiver<()>>>,
    launch_gate: Mutex<Option<mpsc::Receiver<()>>>,
    failure: Mutex<Option<String>>,
}
impl Io for FakeIo {
    fn prepare(&self, paths: FolderPaths) -> Result<FolderPlan, String> {
        self.threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        self.prepared.fetch_add(1, Ordering::Release);
        if let Some(gate) = self.prepare_gate.lock().unwrap().take() {
            gate.recv_timeout(Duration::from_secs(3))
                .map_err(|e| e.to_string())?;
        }
        paths.prepare().map_err(|e| e.to_string())
    }
    fn launch(&self, plan: &FolderPlan) -> Result<(), String> {
        self.threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        self.launched.lock().unwrap().push(plan.clone());
        if let Some(gate) = self.launch_gate.lock().unwrap().take() {
            gate.recv_timeout(Duration::from_secs(3))
                .map_err(|e| e.to_string())?;
        }
        self.failure.lock().unwrap().clone().map_or(Ok(()), Err)
    }
}
fn labels(root: &impl IsA<gtk::Widget>) -> Vec<String> {
    let mut todo = vec![root.as_ref().clone()];
    let mut out = vec![];
    while let Some(w) = todo.pop() {
        if let Some(l) = w.downcast_ref::<gtk::Label>() {
            out.push(l.text().to_string());
        }
        let mut c = w.first_child();
        while let Some(w) = c {
            c = w.next_sibling();
            todo.push(w);
        }
    }
    out
}
fn drive(owner: &BusOwner, folders: &Folders, condition: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    let mut tick = 0;
    while !condition() {
        assert!(
            Instant::now() < end,
            "synthetic folder job did not reach expected phase: {:?}",
            labels(folders.widget())
        );
        owner.poll();
        folders.poll(Duration::from_millis(tick));
        tick += 1;
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn active(temp: &Temp) -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver, PathBuf) {
    let (owner, fake, driver) = setup();
    let root = temp.0.join("Recording root");
    owner.set_recording_root(Some(root.clone())).unwrap();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 41);
    let path = root.join("Actual started take");
    std::fs::create_dir_all(&path).unwrap();
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .directory = Some(path.clone());
    fake.config.borrow_mut().recording = true;
    owner.poll();
    (owner, fake, driver, path)
}
#[gtk::test]
fn folder_is_mounted_but_fixture_mode_never_prepares_or_launches() {
    let temp = Temp::new();
    let (owner, _, _) = setup();
    let root = temp.0.join("Must not be created");
    owner.set_recording_root(Some(root.clone())).unwrap();
    let workspace = NativeWorkspace::new(owner.session.clone());
    workspace.attach_test_buses(owner);
    for n in 0..20 {
        workspace.poll_at(Duration::from_millis(n));
    }
    let folder = button(workspace.widget(), "Folder");
    assert!(!folder.is_sensitive());
    folder.emit_clicked();
    assert!(!root.exists());
}
#[gtk::test]
fn folder_launch_uses_actual_latest_receipt_and_off_ui_workers_without_an_arrival_stopwatch() {
    let temp = Temp::new();
    let (owner, _, _, path) = active(&temp);
    let io = Arc::new(FakeIo::default());
    let folders = Folders::new(owner.clone(), Some(io.clone()));
    assert!(labels(folders.widget())
        .iter()
        .any(|s| s == "Recording time unavailable"));
    button(folders.widget(), "Folder").emit_clicked();
    drive(&owner, &folders, || {
        labels(folders.widget())
            .iter()
            .any(|s| s.contains("Folder request submitted"))
    });
    assert_eq!(io.launched.lock().unwrap().len(), 1);
    assert_eq!(
        io.launched.lock().unwrap()[0].target(),
        path.canonicalize().unwrap()
    );
    assert!(io
        .threads
        .lock()
        .unwrap()
        .iter()
        .all(|id| *id != std::thread::current().id()));
    let button_before = button(folders.widget(), "Folder");
    for n in 0..1000 {
        owner.poll();
        folders.poll(Duration::from_millis(n));
    }
    assert_eq!(button(folders.widget(), "Folder"), button_before);
    assert_eq!(io.launched.lock().unwrap().len(), 1);
    assert!(labels(folders.widget())
        .iter()
        .any(|s| s == "Recording time unavailable"));
}
#[gtk::test]
fn root_or_latest_change_cancels_preparation_before_any_launch_and_late_result_stays_retired() {
    for root_change in [false, true] {
        let temp = Temp::new();
        let (owner, fake, _, _) = active(&temp);
        let io = Arc::new(FakeIo::default());
        let (release, gate) = mpsc::channel();
        *io.prepare_gate.lock().unwrap() = Some(gate);
        let folders = Folders::new(owner.clone(), Some(io.clone()));
        button(folders.widget(), "Folder").emit_clicked();
        drive(&owner, &folders, || {
            io.prepared.load(Ordering::Acquire) == 1
        });
        if root_change {
            owner
                .set_recording_root(Some(temp.0.join("New root")))
                .unwrap();
        } else {
            let mut value = fake.takes.borrow()[0].0.borrow().clone().unwrap();
            value.take = Some(TakeKey::Synthetic(88));
            for track in &mut value.tracks {
                track.identity = TrackKey::Synthetic(88);
            }
            value.source.ticket = 88;
            value.directory = Some(temp.0.join("Other actual take"));
            *fake.latest_started.borrow_mut() = Some(Rc::new(Take(RefCell::new(Some(value)))));
        }
        owner.poll();
        folders.poll(Duration::from_secs(1));
        assert!(labels(folders.widget())
            .iter()
            .any(|s| s.contains("cancelled before launch")));
        release.send(()).unwrap();
        for n in 0..20 {
            owner.poll();
            folders.poll(Duration::from_millis(1100 + n));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(io.launched.lock().unwrap().is_empty());
        assert!(button(folders.widget(), "Folder").is_sensitive());
    }
}
#[gtk::test]
fn busy_authority_retains_prepared_plan_until_a_fresh_same_source_inspection() {
    let temp = Temp::new();
    let (owner, fake, _, _) = active(&temp);
    let io = Arc::new(FakeIo::default());
    let folders = Folders::new(owner.clone(), Some(io.clone()));
    button(folders.widget(), "Folder").emit_clicked();
    drive(&owner, &folders, || {
        io.prepared.load(Ordering::Acquire) == 1
    });
    fake.busy.set(true);
    for n in 0..20 {
        owner.poll();
        folders.poll(Duration::from_millis(n));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(io.launched.lock().unwrap().is_empty());
    assert!(!button(folders.widget(), "Folder").is_sensitive());
    assert_eq!(io.prepared.load(Ordering::Acquire), 1);
    fake.busy.set(false);
    drive(&owner, &folders, || {
        labels(folders.widget())
            .iter()
            .any(|s| s.contains("Folder request submitted"))
    });
    assert_eq!(io.launched.lock().unwrap().len(), 1);
}
#[gtk::test]
fn admitted_launch_retains_original_error_through_root_change_and_closing() {
    let temp = Temp::new();
    let (owner, _, _, _) = active(&temp);
    let old_root = owner.data.borrow().root.clone().unwrap();
    let io = Arc::new(FakeIo::default());
    let (release, gate) = mpsc::channel();
    *io.launch_gate.lock().unwrap() = Some(gate);
    *io.failure.lock().unwrap() = Some("Synthetic helper failed after admission".into());
    let folders = Folders::new(owner.clone(), Some(io.clone()));
    button(folders.widget(), "Folder").emit_clicked();
    drive(&owner, &folders, || io.launched.lock().unwrap().len() == 1);
    let new_root = temp.0.join("Changed explicit root");
    owner.set_recording_root(Some(new_root.clone())).unwrap();
    owner.session.close_admission();
    owner.poll();
    folders.poll(Duration::from_secs(2));
    assert!(!button(folders.widget(), "Folder").is_sensitive());
    release.send(()).unwrap();
    drive(&owner, &folders, || {
        labels(folders.widget())
            .iter()
            .any(|s| s.contains("Synthetic helper failed after admission"))
    });
    let text = labels(folders.widget()).join("\n");
    assert!(text.contains(old_root.to_str().unwrap()));
    assert!(text.contains(new_root.to_str().unwrap()));
    assert_eq!(io.launched.lock().unwrap().len(), 1);
    assert!(!button(folders.widget(), "Folder").is_sensitive());
}
#[gtk::test]
fn folder_path_substitution_is_rejected_and_latest_evidence_survives_stop_disconnect_and_eviction()
{
    let temp = Temp::new();
    let (owner, fake, driver, path) = active(&temp);
    let snapshot = owner.folder_snapshot(Duration::ZERO);
    assert!(owner
        .authorize_folder(
            &snapshot.origin,
            &FolderPaths::new(temp.0.join("Foreign root"), None).unwrap()
        )
        .is_err());
    let original = fake.latest_started.borrow().clone();
    let mut changed = original.as_ref().unwrap().0.borrow().clone().unwrap();
    changed.take = Some(TakeKey::Synthetic(99));
    changed.source.ticket = 99;
    *fake.latest_started.borrow_mut() = Some(Rc::new(Take(RefCell::new(Some(changed)))));
    owner.poll(); // Snapshot source changed before folder widgets were republished.
    let old_paths = FolderPaths::new(snapshot.root.clone().unwrap(), Some(path.clone())).unwrap();
    assert!(owner
        .authorize_folder(&snapshot.origin, &old_paths)
        .is_err());
    *fake.latest_started.borrow_mut() = original;
    owner.poll();
    fake.joined(0, None);
    fake.config.borrow_mut().recording = false;
    owner.poll();
    for n in 0..20 {
        let origin = owner.origin().unwrap();
        fake.admit(
            RecordingStart {
                ticket: 100 + n,
                origin: origin.0.graph.clone().unwrap(),
                expected_config: fake.config.borrow().revision,
                root: temp.0.clone(),
            },
            None,
            None,
        )
        .unwrap();
        let index = fake.takes.borrow().len() - 1;
        fake.joined(index, Some(RecordingError::CancelledBeforeStart));
        owner.poll();
    }
    assert!(!owner
        .recording_status()
        .iter()
        .any(|s| s.take == Some(TakeKey::Synthetic(41))));
    let mut g = graph();
    g.verbonden = false;
    driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    owner.poll();
    let io = Arc::new(FakeIo::default());
    let folders = Folders::new(owner.clone(), Some(io.clone()));
    button(folders.widget(), "Folder").emit_clicked();
    drive(&owner, &folders, || {
        labels(folders.widget())
            .iter()
            .any(|s| s.contains("Folder request submitted"))
    });
    assert_eq!(
        io.launched.lock().unwrap()[0].target(),
        path.canonicalize().unwrap()
    );
}
