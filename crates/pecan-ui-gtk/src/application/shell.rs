//! Explicit primary-window lifetime adapter. Not installed by main startup.
use super::{
    drain::SettingsDrain, ApplicationOwner, Mode, NativeWorkspace, Session, SettingsSummary,
};
use crate::{
    lifetime::{
        Completion, Effect, Event, Lifetime, QuitPhase, RequestId, Resources, TrayAvailability,
    },
    settings_worker::{SettingsOperation, SettingsWorker, SubmissionError, SubmissionFailure},
    tray::{ActionMailbox, TrayAdapter, TraySnapshot},
};
use gtk::{gio, glib, prelude::*};
use pecan_pipewire::shutdown::{Shutdown, ShutdownReport};
use std::{
    cell::RefCell,
    io,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy, Default)]
pub struct ShellOptions {
    pub hidden: bool,
    pub tray: bool,
}
/// Validation failure returns every supplied resource to the caller. A failed
/// installation must not detach an already-running settings worker.
pub struct InstallFailure {
    pub reason: io::Error,
    pub owner: Rc<ApplicationOwner>,
    pub settings: Option<SettingsWorker>,
}
impl std::fmt::Debug for InstallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallFailure")
            .field("reason", &self.reason)
            .field("has_settings", &self.settings.is_some())
            .finish()
    }
}
impl std::fmt::Display for InstallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.reason.fmt(f)
    }
}
impl std::error::Error for InstallFailure {}
#[derive(Clone)]
enum OwnedCompletion {
    Native(Shutdown),
    Fixture(Arc<Mutex<Option<ShutdownReport>>>),
}
impl Completion for OwnedCompletion {
    fn try_complete(&self) -> Option<ShutdownReport> {
        match self {
            Self::Native(handle) => handle.try_complete(),
            Self::Fixture(report) => report.try_lock().ok().and_then(|r| r.clone()),
        }
    }
}
/// Install exactly once in the primary activation. Re-activation presents the
/// retained window; it never starts another backend. Explicit library API only:
/// the native executable is not switched to this shell by this candidate.
pub struct ApplicationShell {
    app: gtk::Application,
    owner: Rc<ApplicationOwner>,
    session: Session,
    workspace: Rc<NativeWorkspace>,
    window: gtk::ApplicationWindow,
    policy: RefCell<Lifetime<OwnedCompletion>>,
    mailbox: Arc<ActionMailbox>,
    wake: async_channel::Receiver<()>,
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    clock: RefCell<Option<glib::SourceId>>,
    keep_alive: RefCell<Option<Rc<Self>>>,
    tray: RefCell<Option<TrayAdapter>>,
    tray_snapshot: RefCell<Option<TraySnapshot>>,
    tray_retry: RefCell<Option<TraySnapshot>>,
    settings: RefCell<Option<SettingsWorker>>,
    summary: RefCell<SettingsSummary>,
    drain: RefCell<Option<(RequestId, SettingsDrain)>>,
    status: gtk::Label,
    retry: gtk::Button,
    error_box: gtk::Box,
    failure_shown: RefCell<Option<RequestId>>,
    exit_code: RefCell<Option<i32>>,
    #[cfg(test)]
    offset: std::cell::Cell<Duration>,
}
impl ApplicationShell {
    pub fn install(
        app: &gtk::Application,
        owner: Rc<ApplicationOwner>,
        options: ShellOptions,
        settings: Option<SettingsWorker>,
    ) -> Result<Rc<Self>, InstallFailure> {
        if !app.is_registered() || app.is_remote() {
            return Err(InstallFailure {
                reason: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Install only in a registered primary application.",
                ),
                owner,
                settings,
            });
        }
        if let Some(existing) = owner.shell.borrow().upgrade() {
            if existing.app != *app || settings.is_some() {
                return Err(InstallFailure {
                    reason: io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Owner already installed; retain its original app/settings resources.",
                    ),
                    owner: owner.clone(),
                    settings,
                });
            }
            existing.activate();
            return Ok(existing);
        }
        if owner
            .active
            .borrow()
            .as_ref()
            .is_some_and(|active| active.session.admission_closed())
        {
            return Err(InstallFailure {
                reason: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "This owner's finalization has started; it cannot be reinstalled.",
                ),
                owner: owner.clone(),
                settings,
            });
        }
        if app
            .windows()
            .iter()
            .any(|w| w.widget_name() == "pecan-native-primary")
        {
            return Err(InstallFailure {
                reason: io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "This application already has its primary native owner/window.",
                ),
                owner,
                settings,
            });
        }
        let session = owner.activate();
        let workspace = NativeWorkspace::new(session.clone());
        let (mailbox, wake) = ActionMailbox::new();
        let (policy, effects) = Lifetime::start(
            options.hidden,
            Resources {
                backend: true,
                settings: settings.is_some(),
            },
            mailbox.now(),
        );
        let status = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let quit = gtk::Button::with_label("Quit");
        let retry = gtk::Button::with_label("Retry finalization");
        let error_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        tools.append(&quit);
        tools.append(&retry);
        tools.append(&error_box);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.append(&status);
        root.append(&tools);
        root.append(workspace.widget());
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Pecan Mixer — native candidate")
            .default_width(1000)
            .default_height(700)
            .child(&root)
            .build();
        window.set_widget_name("pecan-native-primary");
        let shell = Rc::new(Self {
            app: app.clone(),
            owner,
            session,
            workspace,
            window,
            policy: RefCell::new(policy),
            mailbox,
            wake,
            hold: RefCell::new(None),
            clock: RefCell::new(None),
            keep_alive: RefCell::new(None),
            tray: RefCell::new(None),
            tray_snapshot: RefCell::new(None),
            tray_retry: RefCell::new(None),
            settings: RefCell::new(settings),
            summary: RefCell::default(),
            drain: RefCell::new(None),
            status,
            retry,
            error_box,
            failure_shown: RefCell::new(None),
            exit_code: RefCell::new(None),
            #[cfg(test)]
            offset: std::cell::Cell::new(Duration::ZERO),
        });
        *shell.owner.shell.borrow_mut() = Rc::downgrade(&shell);
        *shell.keep_alive.borrow_mut() = Some(shell.clone());
        for (button, event) in [(&quit, Event::Quit), (&shell.retry, Event::RetryQuit)] {
            let weak = Rc::downgrade(&shell);
            button.connect_clicked(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.event(event)
                }
            });
        }
        let weak = Rc::downgrade(&shell);
        shell.window.connect_close_request(move |_| {
            if let Some(s) = weak.upgrade() {
                s.event(Event::Close);
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(&shell);
        app.connect_activate(move |_| {
            if let Some(s) = weak.upgrade() {
                s.activate();
            }
        });
        // Fixture construction never creates the real tray or audio endpoint.
        if options.tray && matches!(shell.owner.mode, Mode::Native) {
            match TrayAdapter::start(shell.mailbox.clone(), TraySnapshot::default()) {
                Ok(tray) => *shell.tray.borrow_mut() = Some(tray),
                Err(_) => {
                    shell.event(Event::Tray(TrayAvailability::Unavailable));
                }
            }
        }
        shell.effects(effects);
        let weak = Rc::downgrade(&shell);
        let clock = glib::timeout_add_local(Duration::from_millis(25), move || {
            if let Some(s) = weak.upgrade() {
                s.tick();
                if s.exit_code().is_none() {
                    return glib::ControlFlow::Continue;
                }
            }
            glib::ControlFlow::Break
        });
        *shell.clock.borrow_mut() = Some(clock);
        shell.render();
        Ok(shell)
    }
    fn now(&self) -> Duration {
        #[cfg(test)]
        {
            return self.mailbox.now().saturating_add(self.offset.get());
        }
        #[cfg(not(test))]
        {
            self.mailbox.now()
        }
    }
    pub fn window(&self) -> &gtk::ApplicationWindow {
        &self.window
    }
    pub fn session(&self) -> Session {
        self.session.clone()
    }
    pub fn mailbox(&self) -> Arc<ActionMailbox> {
        self.mailbox.clone()
    }
    pub fn phase(&self) -> QuitPhase {
        self.policy.borrow().phase().clone()
    }
    pub fn exit_code(&self) -> Option<i32> {
        *self.exit_code.borrow()
    }
    pub fn settings_summary(&self) -> SettingsSummary {
        self.summary.borrow().clone()
    }
    pub fn activate(&self) {
        self.event(Event::Activate)
    }
    pub fn request_quit(&self) {
        self.event(Event::Quit)
    }
    pub fn submit_settings(&self, operation: SettingsOperation) -> Result<u64, SubmissionError> {
        if !self.policy.borrow().mutations_allowed() {
            return Err(SubmissionError {
                reason: SubmissionFailure::Stopped,
                operation,
            });
        }
        match self.settings.borrow_mut().as_mut() {
            Some(worker) => worker.submit(operation),
            None => Err(SubmissionError {
                reason: SubmissionFailure::Stopped,
                operation,
            }),
        }
    }
    fn event(&self, event: Event) {
        let effects = self.policy.borrow_mut().apply(event, self.now());
        self.effects(effects);
        self.render();
    }
    fn effects(&self, effects: Vec<Effect>) {
        for effect in effects {
            if self.exit_code().is_some() {
                break;
            }
            match effect {
                Effect::HoldApplication => {
                    if self.hold.borrow().is_none() {
                        *self.hold.borrow_mut() = Some(self.app.hold());
                    }
                }
                Effect::Present => self.window.present(),
                Effect::Hide => self.window.set_visible(false),
                Effect::DisableMutations => self.session.close_admission(),
                Effect::RequestBackendShutdown { request } => {
                    let completion = if let Some(native) = self.owner.native_handle() {
                        Ok(OwnedCompletion::Native(native.request_shutdown()))
                    } else {
                        self.owner
                            .fixture_driver()
                            .ok_or_else(|| "Backend owner unavailable.".to_owned())
                            .and_then(|f| f.request_shutdown().map(OwnedCompletion::Fixture))
                    };
                    let effects =
                        self.policy
                            .borrow_mut()
                            .backend_requested(request, completion, self.now());
                    self.effects(effects);
                }
                Effect::DrainSettings { request } => {
                    if let Some(worker) = self.settings.borrow_mut().take() {
                        *self.drain.borrow_mut() = Some((
                            request,
                            SettingsDrain::new(worker, self.summary.borrow().clone()),
                        ));
                    } // Missing owned resources must remain pending, never fake completion.
                }
                Effect::Exit { code } => {
                    *self.exit_code.borrow_mut() = Some(code);
                    if let Some(clock) = self.clock.borrow_mut().take() {
                        clock.remove();
                    }
                    // Tray teardown is requested/detached, not an audio gate.
                    self.tray.borrow_mut().take();
                    self.window.destroy();
                    self.hold.borrow_mut().take();
                    self.app.quit();
                    self.keep_alive.borrow_mut().take();
                }
            }
        }
    }
    /// One clock runs irrespective of window visibility and meter frame ticks.
    pub fn tick(&self) {
        if self.exit_code().is_some() {
            return;
        }
        while self.wake.try_recv().is_ok() {}
        let effects = self
            .mailbox
            .drain()
            .apply(&mut self.policy.borrow_mut(), self.now());
        self.effects(effects);
        let effects = self.policy.borrow_mut().apply(Event::Tick, self.now());
        self.effects(effects);
        if self.exit_code().is_some() {
            return;
        }
        self.workspace.poll();
        if let Some(worker) = self.settings.borrow().as_ref() {
            for _ in 0..32 {
                match worker.try_event() {
                    Ok(event) => self.summary.borrow_mut().event(event),
                    Err(_) => break,
                }
            }
        }
        let finished = self
            .drain
            .borrow_mut()
            .as_mut()
            .and_then(|(id, drain)| drain.poll(self.now()).map(|summary| (*id, summary)));
        if let Some((request, summary)) = finished {
            let result = summary.result();
            *self.summary.borrow_mut() = summary;
            self.drain.borrow_mut().take();
            let effects = self
                .policy
                .borrow_mut()
                .settings_drained(request, result, self.now());
            self.effects(effects);
        }
        self.render();
    }
    fn render(&self) {
        if self.exit_code().is_some() {
            return;
        }
        let phase = self.phase();
        let message=match &phase{
            QuitPhase::Idle=>"Native candidate · recording disabled".to_owned(),
            QuitPhase::Waiting=>"Finalizing. Waiting for joined backend/settings completion; keep this window open.".into(),
            QuitPhase::Pending{backend,settings}=>format!("Finalization is still pending (backend={backend}, settings={settings}). Retry checks the retained work; it does not restart it."),
            QuitPhase::Blocked(error)=>format!("Finalization request blocked: {error}. Retry keeps other resource progress."),
            QuitPhase::Failed{errors,..}=>format!("Finalization completed with errors: {}. Review before choosing explicit error exit.",errors.join("; ")),
            QuitPhase::Exiting{..}=>return,
        };
        let warning = self
            .drain
            .borrow()
            .as_ref()
            .and_then(|(_, d)| d.warning.clone());
        self.status
            .set_text(&warning.map_or(message.clone(), |w| format!("{message}\n{w}")));
        self.retry.set_visible(matches!(
            phase,
            QuitPhase::Pending { .. } | QuitPhase::Blocked(_)
        ));
        if let QuitPhase::Failed {
            acknowledgement, ..
        } = phase
        {
            if *self.failure_shown.borrow() != Some(acknowledgement) {
                while let Some(child) = self.error_box.first_child() {
                    self.error_box.remove(&child);
                }
                let button = gtk::Button::with_label("Exit after finalization error");
                let weak = self.owner.shell.borrow().clone();
                button.connect_clicked(move |_| {
                    if let Some(s) = weak.upgrade() {
                        s.event(Event::ExitAfterError { acknowledgement });
                    }
                });
                self.error_box.append(&button);
                *self.failure_shown.borrow_mut() = Some(acknowledgement);
            }
        }
        self.workspace
            .widget()
            .set_sensitive(self.policy.borrow().mutations_allowed());
        let graph = self.session.graph();
        let snapshot = TraySnapshot {
            connected: graph.as_ref().is_some_and(|g| g.value.verbonden),
            rate: graph.as_ref().and_then(|g| g.value.rate),
            quit: self.phase(),
        };
        if self.tray_snapshot.borrow().as_ref() != Some(&snapshot) {
            *self.tray_retry.borrow_mut() = Some(snapshot.clone());
            *self.tray_snapshot.borrow_mut() = Some(snapshot);
        }
        let candidate = self.tray_retry.borrow_mut().take();
        if let Some(tray) = self.tray.borrow().as_ref() {
            if let Some(snapshot) = candidate {
                if let Err(snapshot) = tray.try_update(snapshot) {
                    *self.tray_retry.borrow_mut() = Some(snapshot);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
