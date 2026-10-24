//! Nonblocking shutdown completion and bounded off-UI waits.
//!
//! A successful report means the graph worker and every recorder writer have
//! returned and been joined. Each successful writer explicitly finalized its
//! WAV headers and buffers, and completed any MIDI write. This is not an
//! `fsync`/power-loss durability guarantee. Requesting cancellation alone is
//! never reported as completion.
//!
//! A timeout leaves the handle usable: the caller may poll again. A filesystem
//! operation cannot safely be forcibly cancelled, so unfinished writers keep
//! running. A shell must not exit on a timeout or failed report without its own
//! deliberate failure policy. The legacy tray still uses its fixed delay;
//! switching that UI to this API and presenting failures is separate work.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(5);
const ERROR_LIMIT: usize = 32;

#[derive(Clone)]
pub struct Shutdown(Arc<Mutex<State>>);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Successful take writers, each of which may own multiple WAV files.
    pub writers_finalized: usize,
    /// Failed or panicked take writers, including errors reaped before shutdown.
    pub writers_failed: usize,
    /// First 32 writer errors; `writers_failed` also counts omitted diagnostics.
    pub writer_errors: Vec<String>,
    pub worker_panicked: bool,
}

impl ShutdownReport {
    pub fn is_success(&self) -> bool {
        self.writers_failed == 0 && !self.worker_panicked
    }
}

/// A timed wait expired; no thread was forcibly stopped or detached by waiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownPending {
    /// Another short registry inspection currently owns the state lock.
    Busy,
    Running {
        worker_running: bool,
        pending_writers: usize,
    },
}

#[derive(Default)]
struct State {
    worker_registered: bool,
    worker: Option<JoinHandle<()>>,
    writers: Vec<JoinHandle<Result<(), String>>>,
    report: ShutdownReport,
}

impl Shutdown {
    pub(crate) fn new() -> Self {
        Self(Arc::default())
    }

    pub(crate) fn worker(&self, worker: JoinHandle<()>) {
        let mut state = self.0.lock().unwrap();
        assert!(
            !state.worker_registered,
            "one graph worker per shutdown handle"
        );
        state.worker_registered = true;
        state.worker = Some(worker);
    }

    pub(crate) fn writer(&self, writer: JoinHandle<Result<(), String>>) {
        self.0.lock().unwrap().writers.push(writer);
    }

    /// Called by the existing low-rate transport tick; never wait for I/O.
    pub(crate) fn reap_writers(&self) -> Option<String> {
        self.0.lock().unwrap().reap_writers()
    }

    /// Safe to poll on the UI thread: join only threads already finished.
    /// A returned report can contain errors; check `is_success()` before exit.
    pub fn try_complete(&self) -> Option<ShutdownReport> {
        self.0.try_lock().ok()?.poll()
    }

    /// Wait off the UI/graph/process thread, for at most the supplied duration
    /// plus scheduler/polling overhead. Timeout is pending, not successful
    /// finalization. No unbounded `join` is used, even for a failed writer.
    pub fn wait_timeout(&self, timeout: Duration) -> Result<ShutdownReport, ShutdownPending> {
        let started = Instant::now();
        loop {
            let pending = if let Ok(mut state) = self.0.try_lock() {
                if let Some(report) = state.poll() {
                    return Ok(report);
                }
                ShutdownPending::Running {
                    worker_running: !state.worker_registered || state.worker.is_some(),
                    pending_writers: state.writers.len(),
                }
            } else {
                ShutdownPending::Busy
            };
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(pending);
            }
            std::thread::sleep(POLL_INTERVAL.min(remaining));
        }
    }
}

impl State {
    fn reap_writers(&mut self) -> Option<String> {
        let (finished, pending): (Vec<_>, Vec<_>) = std::mem::take(&mut self.writers)
            .into_iter()
            .partition(JoinHandle::is_finished);
        self.writers = pending;
        let mut latest = None;
        for writer in finished {
            match writer
                .join()
                .unwrap_or_else(|_| Err("writer panicked".into()))
            {
                Ok(()) => self.report.writers_finalized += 1,
                Err(error) => {
                    self.report.writers_failed += 1;
                    if self.report.writer_errors.len() < ERROR_LIMIT {
                        self.report.writer_errors.push(error.clone());
                    }
                    latest = Some(error);
                }
            }
        }
        latest
    }

    fn poll(&mut self) -> Option<ShutdownReport> {
        self.reap_writers();
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            self.report.worker_panicked = self.worker.take().unwrap().join().is_err();
        }
        (self.worker_registered && self.worker.is_none() && self.writers.is_empty())
            .then(|| self.report.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn unfinished_writer_times_out_and_the_same_handle_can_later_complete() {
        let completion = Shutdown::new();
        let (release, gate) = mpsc::channel();
        completion.writer(std::thread::spawn(move || {
            gate.recv_timeout(Duration::from_secs(2))
                .map_err(|e| e.to_string())
        }));
        completion.worker(std::thread::spawn(|| {}));
        assert!(completion.try_complete().is_none());
        assert!(completion.wait_timeout(Duration::from_millis(1)).is_err());
        release.send(()).unwrap();
        let report = completion.wait_timeout(Duration::from_secs(2)).unwrap();
        assert!(report.is_success());
        assert_eq!(report.writers_finalized, 1);
        assert_eq!(completion.clone().try_complete(), Some(report));
    }

    #[test]
    fn ui_poll_and_timed_wait_do_not_block_on_a_busy_state_lock() {
        let completion = Shutdown::new();
        let _guard = completion.0.lock().unwrap();
        assert!(completion.try_complete().is_none());
        assert_eq!(
            completion.wait_timeout(Duration::ZERO),
            Err(ShutdownPending::Busy)
        );
    }

    #[test]
    fn writer_errors_and_thread_panics_are_completed_failures_not_success() {
        let completion = Shutdown::new();
        completion.writer(std::thread::spawn(|| Err("injected write error".into())));
        completion.writer(std::thread::spawn(|| panic!("synthetic writer panic")));
        completion.worker(std::thread::spawn(|| panic!("synthetic graph panic")));
        let report = completion.wait_timeout(Duration::from_secs(2)).unwrap();
        assert!(!report.is_success());
        assert_eq!(report.writers_finalized, 0);
        assert_eq!(report.writers_failed, 2);
        assert!(report.worker_panicked);
        assert!(report
            .writer_errors
            .contains(&"injected write error".into()));
        assert!(report.writer_errors.contains(&"writer panicked".into()));
        assert_eq!(completion.try_complete(), Some(report));
    }

    #[test]
    fn finished_writers_do_not_complete_before_graph_cleanup_returns() {
        let completion = Shutdown::new();
        let (release, gate) = mpsc::channel();
        completion.worker(std::thread::spawn(move || {
            gate.recv_timeout(Duration::from_secs(2)).unwrap();
        }));
        completion.writer(std::thread::spawn(|| Ok(())));
        assert!(completion.wait_timeout(Duration::ZERO).is_err());
        release.send(()).unwrap();
        assert!(completion
            .wait_timeout(Duration::from_secs(2))
            .unwrap()
            .is_success());
    }
}
