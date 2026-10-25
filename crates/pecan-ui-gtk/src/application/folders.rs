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
    cancelled: bool,
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
        // Completion is delivered only after the I/O worker has joined on a
        // background thread, including any thread-local teardown.
        let receiver = joined_worker("pecan-folder-prepare", move || io.prepare(paths))?;
        *self.pending.borrow_mut() = Some(Job {
            request,
            phase: Phase::Preparing(receiver),
            cancelled: false,
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
                Phase::Preparing(receiver) => {
                    job.cancelled |= self.buses.authorize_folder(&job.request.origin, &job.request.paths).is_err();
                    match receiver.try_recv() {
                        // Cancellation revokes launch, not worker ownership. The
                        // slot and original token stay pending until it finishes.
                        Ok(_) if job.cancelled => Some(Completion::Cancelled),
                        Ok(Ok(plan)) => { job.phase = Phase::Ready(plan); None }
                        Ok(Err(error)) => Some(Completion::Failed(error)),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) if job.cancelled => Some(Completion::Cancelled),
                        Err(TryRecvError::Disconnected) => Some(Completion::Failed("Folder preparation ended without a result.".into())),
                    }
                }
                Phase::Ready(plan) if plan.requested() != &job.request.paths => Some(Completion::Failed("Prepared folder plan does not match captured request paths; no launch admitted.".into())),
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
                        match joined_worker("pecan-folder-launch", move || io.launch(&worker_plan)) {
                            Ok(receiver) => {
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

// One bounded job owns one I/O worker. Its supervisor does only spawning,
// joining and result delivery; no injected I/O or GUI code runs there.
fn joined_worker<T: Send + 'static>(
    name: &'static str,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<Receiver<Result<T, String>>, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name(format!("{name}-join"))
        .spawn(move || {
            let result = std::thread::Builder::new()
                .name(name.into())
                .spawn(work)
                .map_err(|e| e.to_string())
                .and_then(|worker| {
                    worker
                        .join()
                        .unwrap_or_else(|_| Err("Folder worker panicked.".into()))
                });
            let _ = sender.send(result);
        })
        .map_err(|e| e.to_string())?;
    Ok(receiver)
}
