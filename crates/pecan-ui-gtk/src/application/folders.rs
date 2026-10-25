//! One explicitly requested folder job. Preparation, launch and reaping are off
//! the GTK thread. Fixture construction supplies no filesystem/process endpoint.
use super::{buses::folder::FolderOrigin, transport::*, BusOwner};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::{
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    time::Duration,
};

pub(super) trait Io: Send + Sync {
    fn prepare(&self, paths: FolderPaths) -> Result<FolderPlan, String>;
    fn launch(&self, plan: &FolderPlan) -> Result<(), String>;
}
pub(super) struct NativeIo;
impl Io for NativeIo {
    fn prepare(&self, paths: FolderPaths) -> Result<FolderPlan, String> {
        paths.prepare().map_err(|e| e.to_string())
    }
    fn launch(&self, plan: &FolderPlan) -> Result<(), String> {
        let mut child = std::process::Command::new(plan.program())
            .arg(plan.argument())
            .spawn()
            .map_err(|e| e.to_string())?;
        let status = child.wait().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("Folder helper exited unsuccessfully: {status}"))
        }
    }
}
enum Phase {
    Preparing(Receiver<Result<FolderPlan, String>>),
    Ready(FolderPlan),
    Launching(FolderPlan, Receiver<Result<(), String>>),
}
struct Job {
    request: FolderRequest<FolderOrigin>,
    phase: Phase,
}
pub(super) struct Folders {
    buses: Rc<BusOwner>,
    io: Option<Arc<dyn Io>>,
    controls: TransportControls<FolderOrigin>,
    widget: gtk::ScrolledWindow,
    current: RefCell<Option<FolderOrigin>>,
    pending: RefCell<Option<Job>>,
    valid: Cell<bool>,
    polling: Cell<bool>,
    now: Cell<Duration>,
    weak: Weak<Self>,
}
impl Folders {
    pub(super) fn new(buses: Rc<BusOwner>, io: Option<Arc<dyn Io>>) -> Rc<Self> {
        let controls = TransportControls::new();
        let widget = gtk::ScrolledWindow::builder()
            .child(controls.widget())
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(65)
            .max_content_height(180)
            .propagate_natural_height(true)
            .build();
        let this = Rc::new_cyclic(|weak| Self {
            buses,
            io,
            controls,
            widget,
            current: RefCell::default(),
            pending: RefCell::default(),
            valid: Cell::new(false),
            polling: Cell::new(false),
            now: Cell::new(Duration::ZERO),
            weak: weak.clone(),
        });
        if this.io.is_some() {
            let weak = this.weak.clone();
            this.controls.bind_folder(move |r| {
                weak.upgrade()
                    .ok_or("Folder owner is no longer available.")?
                    .request(r)
            });
        }
        this.poll(Duration::ZERO);
        this
    }
    pub(super) fn widget(&self) -> &gtk::ScrolledWindow {
        &self.widget
    }
    fn request(&self, request: FolderRequest<FolderOrigin>) -> Result<(), String> {
        if self.pending.borrow().is_some() {
            return Err("A folder request is already pending.".into());
        }
        if !self.valid.get() {
            return Err("Folder presentation is unavailable.".into());
        }
        self.buses
            .authorize_folder(&request.origin, &request.paths)?;
        let io = self
            .io
            .clone()
            .ok_or("Folder I/O is disabled in fixture mode.")?;
        let paths = request.paths.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        // The worker receives explicit paths only, never GTK/owner state. Its
        // result is not launch admission. Joining is never done on the UI thread.
        std::thread::Builder::new()
            .name("pecan-folder-prepare".into())
            .spawn(move || {
                let _ = sender.send(io.prepare(paths));
            })
            .map_err(|e| e.to_string())?;
        *self.pending.borrow_mut() = Some(Job {
            request,
            phase: Phase::Preparing(receiver),
        });
        Ok(())
    }
    pub(super) fn poll(&self, now: Duration) {
        if self.polling.replace(true) {
            return;
        }
        let now = self.now.get().max(now);
        self.now.set(now);
        let snapshot = self.buses.folder_snapshot(now);
        let structural = !self.valid.get()
            || !self
                .current
                .borrow()
                .as_ref()
                .is_some_and(|o| o.same(&snapshot.origin));
        if structural {
            let origin = snapshot.origin.clone();
            self.valid.set(self.controls.observe(snapshot).is_ok());
            if self.valid.get() {
                *self.current.borrow_mut() = Some(origin);
            }
        } else {
            let _ = self
                .controls
                .update_elapsed(snapshot.activity, &snapshot.takes, now);
        }
        let job = self.pending.borrow_mut().take();
        if let Some(mut job) = job {
            let completion = match &mut job.phase {
                Phase::Preparing(receiver) => match self
                    .buses
                    .authorize_folder(&job.request.origin, &job.request.paths)
                {
                    Err(_) => Some(Completion::Cancelled),
                    Ok(_) => match receiver.try_recv() {
                        Ok(Ok(plan)) => {
                            job.phase = Phase::Ready(plan);
                            None
                        }
                        Ok(Err(error)) => Some(Completion::Failed(error)),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) => Some(Completion::Failed(
                            "Folder preparation ended without a result.".into(),
                        )),
                    },
                },
                Phase::Ready(plan) => match self
                    .buses
                    .authorize_folder(&job.request.origin, &job.request.paths)
                {
                    Err(_) => Some(Completion::Cancelled),
                    Ok(false) => None, // Busy inspection retains the prepared plan/token.
                    Ok(true) if !self.valid.get() => Some(Completion::Cancelled),
                    Ok(true) => {
                        let io = self.io.clone().unwrap();
                        let worker_plan = plan.clone();
                        let original = plan.clone();
                        let (sender, receiver) = mpsc::sync_channel(1);
                        match std::thread::Builder::new()
                            .name("pecan-folder-launch".into())
                            .spawn(move || {
                                let _ = sender.send(io.launch(&worker_plan));
                            }) {
                            Ok(_) => {
                                job.phase = Phase::Launching(original, receiver);
                                None
                            }
                            Err(error) => Some(Completion::Failed(error.to_string())),
                        }
                    }
                },
                // An admitted launcher retains its original result across root,
                // latest-take and closing changes. It is never retargeted.
                Phase::Launching(plan, receiver) => match receiver.try_recv() {
                    Ok(Ok(())) => Some(Completion::Submitted(plan.clone())),
                    Ok(Err(error)) => Some(Completion::Failed(error)),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => Some(Completion::Failed(
                        "Folder launch ended without a result.".into(),
                    )),
                },
            };
            if let Some(completion) = completion {
                self.controls.complete(&job.request.token, completion);
            } else {
                *self.pending.borrow_mut() = Some(job);
            }
        }
        self.controls.tick(now);
        self.polling.set(false);
    }
}
