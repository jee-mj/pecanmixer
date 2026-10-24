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
//! deliberate failure policy. Shell adapters own that policy; this API never
//! exits the application or treats timeout as successful finalization.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(5);
const ERROR_LIMIT: usize = 32;

#[derive(Clone)]
pub struct Shutdown(Arc<Mutex<State>>, mpsc::Sender<JoinTask>, Arc<AtomicBool>);

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
    worker_joined: bool,
    pending_writers: usize,
    latest_error: Option<String>,
    report: ShutdownReport,
    tracked_writers: usize,
    tracked_tracks: usize,
}

enum JoinTask {
    Worker(JoinHandle<()>),
    Writer(JoinHandle<Result<(), String>>),
    TrackedWriter(
        JoinHandle<Result<(), String>>,
        crate::writer_status::Reporter,
        WriterLease,
    ),
}

pub(crate) struct WriterLease {
    completion: Shutdown,
    tracks: usize,
    registered: bool,
}
impl Drop for WriterLease {
    fn drop(&mut self) {
        let mut state = self.completion.0.lock().unwrap();
        state.tracked_writers -= 1;
        state.tracked_tracks -= self.tracks;
        if !self.registered {
            state.pending_writers -= 1;
        }
    }
}

impl Shutdown {
    pub(crate) fn stop_tracked(&self) {
        self.2.store(true, Ordering::Release);
    }
    pub(crate) fn tracked_stop_signal(&self) -> Arc<AtomicBool> {
        self.2.clone()
    }
    pub(crate) fn reserve_writer(&self, tracks: usize) -> std::io::Result<WriterLease> {
        let mut state = self
            .0
            .try_lock()
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::WouldBlock))?;
        if state.worker_joined || self.2.load(Ordering::Acquire) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "graph worker already joined",
            ));
        }
        let limit = crate::writer_status::MAX_TRACKED_TRACKS;
        if state.tracked_writers >= limit || tracks > limit.saturating_sub(state.tracked_tracks) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "tracked writer capacity reached",
            ));
        }
        state.tracked_writers += 1;
        state.tracked_tracks += tracks;
        // A reservation already owns in-flight work. Shutdown cannot complete
        // in the interval between spawning and registering its join handle.
        state.pending_writers += 1;
        Ok(WriterLease {
            completion: self.clone(),
            tracks,
            registered: false,
        })
    }
    pub(crate) fn tracked_writer(
        &self,
        writer: JoinHandle<Result<(), String>>,
        reporter: crate::writer_status::Reporter,
        mut lease: WriterLease,
    ) {
        lease.registered = true;
        self.1
            .send(JoinTask::TrackedWriter(writer, reporter, lease))
            .unwrap_or_else(|_| panic!("shutdown coordinator unavailable"));
    }
    pub(crate) fn new() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let worker_state = Arc::clone(&state);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("pecanmixer-reaper".into())
            .spawn(move || reap(rx, worker_state))
            .expect("shutdown coordinator starten");
        Self(state, tx, Arc::new(AtomicBool::new(false)))
    }

    pub(crate) fn worker(&self, worker: JoinHandle<()>) {
        {
            let mut state = self.0.lock().unwrap();
            assert!(
                !state.worker_registered,
                "one graph worker per shutdown handle"
            );
            state.worker_registered = true;
        }
        self.1
            .send(JoinTask::Worker(worker))
            .unwrap_or_else(|_| panic!("shutdown coordinator unavailable"));
    }

    pub(crate) fn writer(&self, writer: JoinHandle<Result<(), String>>) {
        self.0.lock().unwrap().pending_writers += 1;
        self.1
            .send(JoinTask::Writer(writer))
            .unwrap_or_else(|_| panic!("shutdown coordinator unavailable"));
    }

    /// Consume the last already-joined writer error at the transport tick.
    /// Never join, wait for I/O or wait for thread-local teardown here.
    pub(crate) fn reap_writers(&self) -> Option<String> {
        self.0.lock().unwrap().latest_error.take()
    }

    /// Safe to poll on the UI thread: inspect already-joined completion state.
    /// Thread-local teardown can block join even after is_finished(); joins
    /// therefore run exclusively on the background coordinator.
    /// A returned report can contain errors; check `is_success()` before exit.
    pub fn try_complete(&self) -> Option<ShutdownReport> {
        self.0.try_lock().ok()?.poll()
    }

    /// Wait off the UI/graph/process thread, for at most the supplied duration
    /// plus scheduler/polling overhead. Timeout is pending, not successful
    /// finalization. This method never joins; background joining may remain
    /// pending after this wait returns.
    pub fn wait_timeout(&self, timeout: Duration) -> Result<ShutdownReport, ShutdownPending> {
        let started = Instant::now();
        loop {
            let pending = if let Ok(state) = self.0.try_lock() {
                if let Some(report) = state.poll() {
                    return Ok(report);
                }
                ShutdownPending::Running {
                    worker_running: !state.worker_registered || !state.worker_joined,
                    pending_writers: state.pending_writers,
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
    fn poll(&self) -> Option<ShutdownReport> {
        (self.worker_registered && self.worker_joined && self.pending_writers == 0)
            .then(|| self.report.clone())
    }
}

impl JoinTask {
    fn is_finished(&self) -> bool {
        match self {
            Self::Worker(thread) => thread.is_finished(),
            Self::Writer(thread) => thread.is_finished(),
            Self::TrackedWriter(thread, ..) => thread.is_finished(),
        }
    }

    /// Only called by the background coordinator. Never hold the state lock
    /// across join: TLS destructors and the OS may still block indefinitely.
    fn join(self, state: &Mutex<State>) {
        match self {
            Self::Worker(thread) => {
                let panicked = thread.join().is_err();
                let mut state = state.lock().unwrap();
                state.report.worker_panicked = panicked;
                state.worker_joined = true;
            }
            Self::Writer(thread) => {
                let result = thread
                    .join()
                    .unwrap_or_else(|_| Err("writer panicked".into()));
                let mut state = state.lock().unwrap();
                state.pending_writers -= 1;
                match result {
                    Ok(()) => state.report.writers_finalized += 1,
                    Err(error) => {
                        state.report.writers_failed += 1;
                        if state.report.writer_errors.len() < ERROR_LIMIT {
                            state.report.writer_errors.push(error.clone());
                        }
                        state.latest_error = Some(error);
                    }
                }
            }
            Self::TrackedWriter(thread, reporter, lease) => {
                let result = thread
                    .join()
                    .unwrap_or_else(|_| Err("writer panicked".into()));
                {
                    let mut state = state.lock().unwrap();
                    state.pending_writers -= 1;
                    match &result {
                        Ok(()) => state.report.writers_finalized += 1,
                        Err(error) => {
                            state.report.writers_failed += 1;
                            if state.report.writer_errors.len() < ERROR_LIMIT {
                                state.report.writer_errors.push(error.clone());
                            }
                            state.latest_error = Some(error.clone());
                        }
                    }
                }
                drop(lease); // Release capacity only after the actual join.
                reporter.joined(&result);
            }
        }
    }
}

fn reap(rx: mpsc::Receiver<JoinTask>, state: Arc<Mutex<State>>) {
    let mut pending: Vec<JoinTask> = Vec::new();
    loop {
        let mut i = 0;
        while i < pending.len() {
            if pending[i].is_finished() {
                pending.swap_remove(i).join(&state);
            } else {
                i += 1;
            }
        }
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(task) => pending.push(task),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // All completion handles have been dropped. Still finish
                // joining registered work off-thread; no caller is waiting.
                for task in pending {
                    task.join(&state);
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_reserved_writer_prevents_completion_before_join_registration() {
        let completion = Shutdown::new();
        let lease = completion.reserve_writer(1).unwrap();
        completion.worker(std::thread::spawn(|| {}));
        assert!(completion.wait_timeout(Duration::from_millis(10)).is_err());
        assert_eq!(completion.0.lock().unwrap().pending_writers, 1);
        drop(lease);
        assert!(completion
            .wait_timeout(Duration::from_secs(1))
            .unwrap()
            .is_success());
    }

    struct HeldTeardown {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }

    impl Drop for HeldTeardown {
        fn drop(&mut self) {
            let _ = self.entered.send(());
            // Keep regression cleanup bounded even if an assertion fails.
            let _ = self.release.recv_timeout(Duration::from_secs(2));
        }
    }

    thread_local! {
        static TEARDOWN: std::cell::RefCell<Option<HeldTeardown>> = const { std::cell::RefCell::new(None) };
    }

    fn held_thread<T: Send + 'static>(value: T) -> (JoinHandle<T>, mpsc::Sender<()>) {
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            TEARDOWN.with(|slot| {
                *slot.borrow_mut() = Some(HeldTeardown {
                    entered,
                    release: gate,
                })
            });
            value
        });
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !thread.is_finished() {
            assert!(
                Instant::now() < deadline,
                "probe must reach the finished-before-TLS-exit state"
            );
            std::thread::yield_now();
        }
        (thread, release)
    }

    #[test]
    fn gated_thread_local_teardown_cannot_block_polling_or_timed_wait() {
        for block_worker in [false, true] {
            let completion = Shutdown::new();
            let release = if block_worker {
                let (thread, release) = held_thread(());
                completion.worker(thread);
                release
            } else {
                let (thread, release) = held_thread(Ok(()));
                completion.writer(thread);
                completion.worker(std::thread::spawn(|| {}));
                release
            };
            let probe_completion = completion.clone();
            let (sent, received) = mpsc::sync_channel(1);
            completion.writer(std::thread::spawn(move || {
                let poll = probe_completion.try_complete();
                let timed = probe_completion.wait_timeout(Duration::from_millis(1));
                let _ = sent.send((poll, timed));
                Ok(())
            }));
            // Both API calls must return before teardown is released. The
            // old caller-side join blocks here despite is_finished() == true.
            let observed = received.recv_timeout(Duration::from_millis(250));
            release.send(()).unwrap();
            let (poll, timed) = observed.expect("completion inspection blocked on TLS teardown");
            assert!(poll.is_none());
            assert!(timed.is_err());
            let report = completion.wait_timeout(Duration::from_secs(2)).unwrap();
            assert!(report.is_success());
            assert_eq!(report.writers_finalized, if block_worker { 1 } else { 2 });
        }
    }

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
