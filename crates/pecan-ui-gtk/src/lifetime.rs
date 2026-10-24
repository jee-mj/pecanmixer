//! Pure native shell policy. Effects are requests to the owning application;
//! this module starts no backend, GTK object, filesystem operation or tray.
use pecan_pipewire::shutdown::{Shutdown, ShutdownReport};
use std::time::Duration;

pub const HIDDEN_STARTUP_LIMIT: Duration = Duration::from_secs(1);
/// Usability is leased: a stalled tray worker cannot leave a hidden process.
pub const TRAY_EVIDENCE_LIMIT: Duration = Duration::from_secs(5);
pub const FINALIZATION_LIMIT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAvailability {
    Unknown,
    Usable,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    AwaitingTray,
    Visible,
    Hidden,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    /// True whenever the application owns a native graph worker, even if the
    /// cached transport says no recording is active.
    pub backend: bool,
    pub settings: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId(u64);
impl RequestId {
    pub fn get(self) -> u64 {
        self.0
    }
    pub(crate) fn from_token(value: u64) -> Option<Self> {
        (value != 0).then_some(Self(value))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuitPhase {
    Idle,
    Waiting,
    Pending {
        backend: bool,
        settings: bool,
    },
    Blocked(String),
    /// Every owned resource is complete/joined, but finalization had errors.
    Failed {
        acknowledgement: RequestId,
        errors: Vec<String>,
    },
    Exiting {
        code: i32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Retain a GApplication hold for this owner until an Exit effect is handled.
    HoldApplication,
    Present,
    Hide,
    DisableMutations,
    RequestBackendShutdown {
        request: RequestId,
    },
    /// Close admission, drain every accepted result and join off-UI. Report only
    /// after that work has finished; an empty queue is not completion evidence.
    DrainSettings {
        request: RequestId,
    },
    /// Record this exit status, then release the hold and quit the main loop.
    Exit {
        code: i32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A secondary launch presents the primary even if that launch used --hidden.
    Activate,
    Toggle,
    Hide,
    Close,
    Quit,
    RetryQuit,
    ExitAfterError {
        acknowledgement: RequestId,
    },
    Tray(TrayAvailability),
    Tick,
}
/// Only nonblocking inspection is permitted. Native Shutdown already reaps all
/// joins on its coordinator thread, including potentially gated TLS teardown.
pub trait Completion: Clone {
    fn try_complete(&self) -> Option<ShutdownReport>;
    /// Additional owner evidence, kept separate from the writer's join report.
    /// None retains the same handle while its diagnostic status is busy.
    fn additional_errors(&self, _joined: &ShutdownReport) -> Option<Vec<String>> {
        Some(vec![])
    }
}
impl Completion for Shutdown {
    fn try_complete(&self) -> Option<ShutdownReport> {
        Shutdown::try_complete(self)
    }
}
enum Backend<C> {
    NotOwned,
    Needed,
    Requested(RequestId),
    Waiting(C),
    Complete(ShutdownReport, Vec<String>),
    Blocked(String),
}
enum Settings {
    NotOwned,
    Needed,
    Draining(RequestId),
    Complete(Result<(), String>),
}
pub struct Lifetime<C: Completion = Shutdown> {
    visibility: Visibility,
    tray: TrayAvailability,
    last_tray_evidence: Option<Duration>,
    hidden_deadline: Duration,
    quit_deadline: Duration,
    phase: QuitPhase,
    backend: Backend<C>,
    settings: Settings,
    next: u64,
    quit_id: Option<RequestId>,
}
impl<C: Completion> Lifetime<C> {
    /// Construct exactly once in the primary application. Hiding or activating
    /// the window must never reconstruct this owner or restart its resources.
    pub fn start(hidden: bool, resources: Resources, now: Duration) -> (Self, Vec<Effect>) {
        let mut effects = vec![Effect::HoldApplication];
        if !hidden {
            effects.push(Effect::Present);
        }
        (
            Self {
                visibility: if hidden {
                    Visibility::AwaitingTray
                } else {
                    Visibility::Visible
                },
                tray: TrayAvailability::Unknown,
                last_tray_evidence: None,
                hidden_deadline: now.saturating_add(HIDDEN_STARTUP_LIMIT),
                quit_deadline: now,
                phase: QuitPhase::Idle,
                backend: if resources.backend {
                    Backend::Needed
                } else {
                    Backend::NotOwned
                },
                settings: if resources.settings {
                    Settings::Needed
                } else {
                    Settings::NotOwned
                },
                next: 0,
                quit_id: None,
            },
            effects,
        )
    }
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }
    pub fn tray(&self) -> TrayAvailability {
        self.tray
    }
    pub fn phase(&self) -> &QuitPhase {
        &self.phase
    }
    pub fn mutations_allowed(&self) -> bool {
        self.phase == QuitPhase::Idle
    }
    /// Real adapter evidence retains its production time. Delivery delay cannot
    /// renew an expired tray lease. Use the mailbox's monotonic clock for now.
    pub fn apply_tray_evidence(
        &mut self,
        availability: TrayAvailability,
        observed_at: Duration,
        now: Duration,
    ) -> Vec<Effect> {
        if availability == TrayAvailability::Usable
            && (observed_at > now || now.saturating_sub(observed_at) >= TRAY_EVIDENCE_LIMIT)
        {
            return self.apply(Event::Tray(TrayAvailability::Unknown), now);
        }
        let effects = self.apply(Event::Tray(availability), now);
        if availability == TrayAvailability::Usable {
            self.last_tray_evidence = Some(observed_at);
        }
        effects
    }
    pub fn apply(&mut self, event: Event, now: Duration) -> Vec<Effect> {
        let mut effects = vec![];
        if matches!(self.phase, QuitPhase::Exiting { .. }) {
            return effects;
        }
        self.expire_tray(now, &mut effects);
        match event {
            Event::Activate => self.present(&mut effects),
            Event::Toggle => {
                if self.phase == QuitPhase::Idle
                    && self.visibility == Visibility::Visible
                    && self.tray == TrayAvailability::Usable
                {
                    self.visibility = Visibility::Hidden;
                    effects.push(Effect::Hide);
                } else {
                    self.present(&mut effects);
                }
            }
            Event::Hide => {
                if self.phase == QuitPhase::Idle && self.tray == TrayAvailability::Usable {
                    self.visibility = Visibility::Hidden;
                    effects.push(Effect::Hide);
                } else {
                    self.present(&mut effects);
                }
            }
            Event::Close
                if self.phase == QuitPhase::Idle && self.tray == TrayAvailability::Usable =>
            {
                self.visibility = Visibility::Hidden;
                effects.push(Effect::Hide);
            }
            Event::Close if self.phase != QuitPhase::Idle => self.present(&mut effects),
            Event::Close | Event::Quit | Event::RetryQuit => self.begin_quit(now, &mut effects),
            Event::ExitAfterError { acknowledgement } => {
                if matches!(&self.phase, QuitPhase::Failed { acknowledgement: shown, .. } if *shown == acknowledgement)
                {
                    self.phase = QuitPhase::Exiting { code: 1 };
                    effects.push(Effect::Exit { code: 1 });
                }
            }
            Event::Tray(availability) => {
                self.tray = availability;
                self.last_tray_evidence = (availability == TrayAvailability::Usable).then_some(now);
                if availability == TrayAvailability::Usable
                    && self.visibility == Visibility::AwaitingTray
                    && self.phase == QuitPhase::Idle
                {
                    self.visibility = Visibility::Hidden;
                    effects.push(Effect::Hide);
                } else if availability != TrayAvailability::Usable
                    && self.visibility == Visibility::Hidden
                    || availability == TrayAvailability::Unavailable
                        && self.visibility == Visibility::AwaitingTray
                {
                    self.present(&mut effects);
                }
            }
            Event::Tick => {
                if self.visibility == Visibility::AwaitingTray && now >= self.hidden_deadline {
                    self.present(&mut effects);
                }
                self.progress(now, &mut effects);
            }
        }
        effects
    }
    /// The owner must return the handle from this exact request, not report
    /// command admission/Stop/transport-idle as completed finalization.
    pub fn backend_requested(
        &mut self,
        request: RequestId,
        result: Result<C, String>,
        now: Duration,
    ) -> Vec<Effect> {
        if !matches!(self.backend, Backend::Requested(expected) if expected == request) {
            return vec![];
        }
        self.backend = match result {
            Ok(handle) => Backend::Waiting(handle),
            Err(error) => Backend::Blocked(error),
        };
        let mut effects = vec![];
        self.progress(now, &mut effects);
        effects
    }
    /// Success means results were drained/handled and the settings thread was
    /// joined off-UI. An error also means the drain/join attempt has completed;
    /// a still-running operation must remain pending rather than send this event.
    pub fn settings_drained(
        &mut self,
        request: RequestId,
        result: Result<(), String>,
        now: Duration,
    ) -> Vec<Effect> {
        if !matches!(self.settings, Settings::Draining(expected) if expected == request) {
            return vec![];
        }
        self.settings = Settings::Complete(result);
        let mut effects = vec![];
        self.progress(now, &mut effects);
        effects
    }
    fn expire_tray(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        if self.tray == TrayAvailability::Usable
            && self
                .last_tray_evidence
                .is_some_and(|last| now.saturating_sub(last) >= TRAY_EVIDENCE_LIMIT)
        {
            self.tray = TrayAvailability::Unknown;
            self.last_tray_evidence = None;
            if self.visibility == Visibility::Hidden {
                self.present(effects);
            }
        }
    }
    fn present(&mut self, effects: &mut Vec<Effect>) {
        self.visibility = Visibility::Visible;
        effects.push(Effect::Present);
    }
    fn id(&mut self) -> Option<RequestId> {
        self.next = self.next.checked_add(1)?;
        Some(RequestId(self.next))
    }
    fn begin_quit(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        if matches!(
            self.phase,
            QuitPhase::Waiting | QuitPhase::Failed { .. } | QuitPhase::Exiting { .. }
        ) {
            return;
        }
        if self.quit_id.is_none() {
            let Some(id) = self.id() else {
                self.phase = QuitPhase::Blocked("Lifetime request IDs exhausted".into());
                return;
            };
            self.quit_id = Some(id);
            effects.push(Effect::DisableMutations);
        }
        self.phase = QuitPhase::Waiting;
        self.quit_deadline = now.saturating_add(FINALIZATION_LIMIT);
        self.present(effects);
        if matches!(self.backend, Backend::Needed | Backend::Blocked(_)) {
            let Some(request) = self.id() else {
                self.phase = QuitPhase::Blocked("Lifetime request IDs exhausted".into());
                return;
            };
            self.backend = Backend::Requested(request);
            effects.push(Effect::RequestBackendShutdown { request });
        }
        if matches!(self.settings, Settings::Needed) {
            let Some(request) = self.id() else {
                self.phase = QuitPhase::Blocked("Lifetime request IDs exhausted".into());
                return;
            };
            self.settings = Settings::Draining(request);
            effects.push(Effect::DrainSettings { request });
        }
        self.progress(now, effects);
    }
    fn progress(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        // Timeout stays open, retaining handles. Retry deliberately resumes
        // inspection, matching the legacy policy rather than inventing success.
        if self.phase != QuitPhase::Waiting {
            return;
        }
        if let Backend::Waiting(completion) = &self.backend {
            if let Some(report) = completion.try_complete() {
                if let Some(errors) = completion.additional_errors(&report) {
                    self.backend = Backend::Complete(report, errors);
                }
            }
        }
        if let Backend::Blocked(error) = &self.backend {
            self.phase = QuitPhase::Blocked(error.clone());
            return;
        }
        let backend_done = matches!(self.backend, Backend::NotOwned | Backend::Complete(..));
        let settings_done = matches!(self.settings, Settings::NotOwned | Settings::Complete(_));
        if backend_done && settings_done {
            let mut errors = vec![];
            if let Backend::Complete(report, additional) = &self.backend {
                errors.extend(additional.iter().take(32).cloned());
                if report.worker_panicked {
                    errors.push("Audio worker panicked".into());
                }
                errors.extend(report.writer_errors.iter().take(32).cloned());
                if report.writers_failed > 0 && report.writer_errors.is_empty() {
                    errors.push("Recording writer finalization failed".into());
                }
            }
            if let Settings::Complete(Err(error)) = &self.settings {
                errors.push(format!("Settings drain failed: {error}"));
            }
            if errors.is_empty() {
                self.phase = QuitPhase::Exiting { code: 0 };
                effects.push(Effect::Exit { code: 0 });
            } else {
                let Some(acknowledgement) = self.quit_id else {
                    self.phase = QuitPhase::Blocked("Missing quit attempt".into());
                    return;
                };
                self.phase = QuitPhase::Failed {
                    acknowledgement,
                    errors,
                };
            }
        } else if now >= self.quit_deadline {
            self.phase = QuitPhase::Pending {
                backend: !backend_done,
                settings: !settings_done,
            };
        }
    }
}

#[cfg(test)]
mod tests;
