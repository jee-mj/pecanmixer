//! Legacy quit policy, independent of Tauri/GTK/D-Bus for headless tests.
//!
//! One attempt at a time. Timeout retains the same completion handle; retry
//! checks it again rather than starting another backend or inventing success.
//! Only a successful completed report permits exit(0). A completed failure
//! stays open until the user explicitly chooses exit-after-error (exit(1)).
//! This policy is wired to explicit tray Quit. Existing window-close/background
//! and external termination paths are unchanged. Tray status is refreshed on
//! AboutToShow, without waiting for a D-Bus acknowledgement from this adapter.

use std::sync::Mutex;
use std::time::Duration;

use pecan_pipewire::shutdown::{Shutdown, ShutdownPending, ShutdownReport};

pub(crate) const WAIT_LIMIT: Duration = Duration::from_secs(5);
pub(crate) type QuitAdapter = Quit<Shutdown>;

pub(crate) trait Completion: Clone {
    fn wait(&self, limit: Duration) -> Result<ShutdownReport, ShutdownPending>;
}

impl Completion for Shutdown {
    fn wait(&self, limit: Duration) -> Result<ShutdownReport, ShutdownPending> {
        self.wait_timeout(limit)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
    #[default]
    Idle,
    Waiting,
    Pending(ShutdownPending),
    Failed(ShutdownReport),
    Blocked(String),
    Exiting,
}

impl Phase {
    pub(crate) fn status(&self) -> Option<String> {
        match self {
            Self::Idle => None,
            Self::Waiting => Some("Quit: stopping audio and finalizing recordings…".into()),
            Self::Pending(_) => {
                Some("Quit pending: finalization is unconfirmed. Choose Retry Quit.".into())
            }
            Self::Failed(report) => {
                let detail = if report.worker_panicked {
                    "Audio worker stopped unexpectedly; recordings may be incomplete.".into()
                } else {
                    report
                        .writer_errors
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Recording finalization failed.".into())
                };
                Some(format!("Quit failed: {detail}"))
            }
            Self::Blocked(message) => Some(format!("Quit blocked: {message}")),
            Self::Exiting => Some("Exiting Pecan Mixer…".into()),
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Pending(_) | Self::Blocked(_) => "Retry Quit",
            Self::Waiting => "Finalizing recordings…",
            Self::Failed(_) => "Exit after finalization error",
            Self::Exiting => "Exiting…",
            Self::Idle => "Quit",
        }
    }
}

pub(crate) struct Quit<C> {
    state: Mutex<State<C>>,
}

struct State<C> {
    phase: Phase,
    completion: Option<C>,
}

pub(crate) struct Attempt<C> {
    completion: Option<C>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    StayOpen,
    Exit(i32),
}

enum Outcome {
    Finished(ShutdownReport),
    Pending(ShutdownPending),
    Blocked(String),
}

impl<C: Clone> Default for Quit<C> {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                phase: Phase::Idle,
                completion: None,
            }),
        }
    }
}

impl<C: Clone> Quit<C> {
    pub(crate) fn phase(&self) -> Phase {
        self.state.lock().unwrap().phase.clone()
    }

    /// Claim before spawning, so duplicate clicks cannot create extra workers.
    pub(crate) fn begin(&self) -> Option<Attempt<C>> {
        let mut state = self.state.lock().unwrap();
        match state.phase {
            Phase::Waiting | Phase::Failed(_) | Phase::Exiting => None,
            _ => {
                state.phase = Phase::Waiting;
                Some(Attempt {
                    completion: state.completion.clone(),
                })
            }
        }
    }

    pub(crate) fn could_not_spawn(&self, message: String) {
        self.finish(Outcome::Blocked(message));
    }

    fn finish(&self, outcome: Outcome) -> Action {
        let mut state = self.state.lock().unwrap();
        if state.phase != Phase::Waiting {
            return Action::StayOpen;
        }
        state.phase = match outcome {
            Outcome::Finished(report) if report.is_success() => {
                state.phase = Phase::Exiting;
                return Action::Exit(0);
            }
            Outcome::Finished(report) => Phase::Failed(report),
            Outcome::Pending(pending) => Phase::Pending(pending),
            Outcome::Blocked(message) => Phase::Blocked(message),
        };
        Action::StayOpen
    }

    /// An explicit error exit is permitted only after all threads completed.
    /// Pending/timeouts and missing-backend/spawn errors never permit it.
    pub(crate) fn exit_after_error(&self) -> Action {
        let mut state = self.state.lock().unwrap();
        if matches!(state.phase, Phase::Failed(_)) {
            state.phase = Phase::Exiting;
            Action::Exit(1)
        } else {
            Action::StayOpen
        }
    }
}

impl<C: Completion> Quit<C> {
    /// Run only off the UI/tray thread. Factory is called once; pending retries
    /// reuse the original handle. No tray update/acknowledgement is awaited.
    pub(crate) fn run(
        &self,
        attempt: Attempt<C>,
        request_shutdown: impl FnOnce() -> Result<C, String>,
    ) -> Action {
        let completion = match attempt.completion {
            Some(completion) => completion,
            None => match request_shutdown() {
                Ok(completion) => {
                    self.state.lock().unwrap().completion = Some(completion.clone());
                    completion
                }
                Err(message) => return self.finish(Outcome::Blocked(message)),
            },
        };
        self.finish(match completion.wait(WAIT_LIMIT) {
            Ok(report) => Outcome::Finished(report),
            Err(pending) => Outcome::Pending(pending),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[derive(Clone)]
    struct Probe {
        outcomes: Arc<Mutex<VecDeque<Result<ShutdownReport, ShutdownPending>>>>,
        waits: Arc<AtomicUsize>,
    }
    impl Probe {
        fn new(
            outcomes: impl IntoIterator<Item = Result<ShutdownReport, ShutdownPending>>,
        ) -> Self {
            Self {
                outcomes: Arc::new(Mutex::new(outcomes.into_iter().collect())),
                waits: Arc::default(),
            }
        }
    }
    impl Completion for Probe {
        fn wait(&self, limit: Duration) -> Result<ShutdownReport, ShutdownPending> {
            assert_eq!(limit, WAIT_LIMIT, "adapter must use its explicit bound");
            self.waits.fetch_add(1, Ordering::Relaxed);
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected wait")
        }
    }
    fn success() -> ShutdownReport {
        ShutdownReport {
            writers_finalized: 2,
            ..Default::default()
        }
    }
    fn pending() -> ShutdownPending {
        ShutdownPending::Running {
            worker_running: false,
            pending_writers: 1,
        }
    }

    #[test]
    fn successful_completion_is_the_only_automatic_success_exit() {
        let quit = Quit::default();
        let completion = Probe::new([Ok(success())]);
        let attempt = quit.begin().unwrap();
        assert_eq!(quit.phase(), Phase::Waiting);
        assert_eq!(
            quit.run(attempt, || Ok(completion.clone())),
            Action::Exit(0)
        );
        assert_eq!(quit.phase(), Phase::Exiting);
        assert!(quit.begin().is_none());
        assert_eq!(quit.exit_after_error(), Action::StayOpen);
        assert_eq!(completion.waits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn timeout_stays_open_and_retry_reuses_the_original_completion() {
        let quit = Quit::default();
        let completion = Probe::new([Err(pending()), Ok(success())]);
        assert_eq!(
            quit.run(quit.begin().unwrap(), || Ok(completion.clone())),
            Action::StayOpen
        );
        assert_eq!(quit.phase(), Phase::Pending(pending()));
        assert_eq!(quit.exit_after_error(), Action::StayOpen);
        assert_eq!(quit.phase().label(), "Retry Quit");
        assert_eq!(
            quit.run(quit.begin().unwrap(), || panic!(
                "must not request shutdown again"
            )),
            Action::Exit(0)
        );
        assert_eq!(completion.waits.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn writer_or_worker_failure_stays_open_until_explicit_nonzero_exit() {
        for report in [
            ShutdownReport {
                writers_failed: 1,
                writer_errors: vec!["synthetic flush error".into()],
                ..Default::default()
            },
            ShutdownReport {
                worker_panicked: true,
                ..Default::default()
            },
        ] {
            let quit = Quit::default();
            let completion = Probe::new([Ok(report.clone())]);
            assert_eq!(
                quit.run(quit.begin().unwrap(), || Ok(completion)),
                Action::StayOpen
            );
            assert_eq!(quit.phase(), Phase::Failed(report));
            assert!(
                quit.begin().is_none(),
                "repeated normal Quit is not force exit"
            );
            assert_eq!(quit.phase().label(), "Exit after finalization error");
            assert_eq!(quit.exit_after_error(), Action::Exit(1));
            assert_eq!(quit.exit_after_error(), Action::StayOpen);
        }
    }

    #[test]
    fn repeated_quit_claims_cannot_duplicate_waiters_or_shutdown_requests() {
        let quit = Quit::default();
        let completion = Probe::new([Err(pending()), Ok(success())]);
        let attempts = AtomicUsize::new(0);
        let first = quit.begin().unwrap();
        for _ in 0..10_000 {
            assert!(quit.begin().is_none());
        }
        assert_eq!(
            quit.run(first, || {
                attempts.fetch_add(1, Ordering::Relaxed);
                Ok(completion.clone())
            }),
            Action::StayOpen
        );
        let retry = quit.begin().unwrap();
        assert!(quit.begin().is_none());
        assert_eq!(
            quit.run(retry, || panic!("completion must be retained")),
            Action::Exit(0)
        );
        assert!(quit.begin().is_none());
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
        assert_eq!(completion.waits.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn unavailable_backend_never_exits_and_allows_a_later_retry() {
        let quit = Quit::<Probe>::default();
        assert_eq!(
            quit.run(quit.begin().unwrap(), || Err("Backend is not ready".into())),
            Action::StayOpen
        );
        assert!(matches!(quit.phase(), Phase::Blocked(_)));
        assert_eq!(quit.exit_after_error(), Action::StayOpen);
        assert_eq!(
            quit.run(quit.begin().unwrap(), || Ok(Probe::new([Ok(success())]))),
            Action::Exit(0)
        );
    }

    #[test]
    fn spawn_failure_stays_open_and_does_not_discard_a_pending_completion() {
        let quit = Quit::default();
        let completion = Probe::new([Err(pending()), Ok(success())]);
        assert_eq!(
            quit.run(quit.begin().unwrap(), || Ok(completion.clone())),
            Action::StayOpen
        );
        let _unspawned = quit.begin().unwrap();
        quit.could_not_spawn("Synthetic thread spawn failure".into());
        assert!(matches!(quit.phase(), Phase::Blocked(_)));
        assert_eq!(quit.exit_after_error(), Action::StayOpen);
        assert_eq!(
            quit.run(quit.begin().unwrap(), || panic!("keep original completion")),
            Action::Exit(0)
        );
        assert_eq!(completion.waits.load(Ordering::Relaxed), 2);
    }
}
