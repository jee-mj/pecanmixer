//! Owned writer evidence. Polling never joins or waits for filesystem work.
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub const MAX_TRACKED_TRACKS: usize = 64;
pub const MAX_WRITER_ERROR_BYTES: usize = 4096;
static NEXT_TAKE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TakeId(u64);
impl TakeId {
    pub fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrackId {
    take: TakeId,
    index: u16,
}
impl TrackId {
    pub fn take(self) -> TakeId {
        self.take
    }
    pub fn index(self) -> usize {
        usize::from(self.index)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriterPhase {
    Opening,
    Ready,
    Finalizing,
    Finished,
    Joined,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotOpened {
    Stopped,
    InitializationFailed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackPhase {
    Opening,
    Open,
    Closing,
    /// An explicit WAV finalize call returned this result, including any
    /// preceding write error or accepted-but-unwritten data. It is not merely
    /// a close request, and an error does not imply that no bytes were written.
    Finalized(Result<(), String>),
    NotOpened(NotOpened),
    /// A panic/abnormal exit prevented proof of explicit finalization.
    Unconfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackReport {
    pub id: TrackId,
    pub name: String,
    pub channels: u16,
    pub phase: TrackPhase,
    pub samples_accepted: u64,
    pub samples_written: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriterOutcome {
    Finalized,
    StoppedBeforeReady,
    Failed(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriterReport {
    pub take: TakeId,
    pub directory: Option<PathBuf>,
    pub phase: WriterPhase,
    /// All requested WAVs were opened; this does not prove audio capture.
    pub was_ready: bool,
    pub stop_requested: bool,
    pub tracks: Vec<TrackReport>,
    pub outcome: Option<WriterOutcome>,
}
impl WriterReport {
    /// Joined and explicitly finalized, not an acknowledgement of start.
    pub fn is_success(&self) -> bool {
        self.phase == WriterPhase::Joined && self.outcome == Some(WriterOutcome::Finalized)
    }
}

pub(crate) struct Signals {
    pub ready: AtomicBool,
    pub stop: AtomicBool,
    pub closed: AtomicBool,
    pub closing: AtomicU64,
    pub accepted: [AtomicU64; MAX_TRACKED_TRACKS],
    pub owner_stop: Arc<AtomicBool>,
}
impl Default for Signals {
    fn default() -> Self {
        Self {
            ready: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            closing: AtomicU64::new(0),
            accepted: std::array::from_fn(|_| AtomicU64::new(0)),
            owner_stop: Arc::new(AtomicBool::new(false)),
        }
    }
}
impl Signals {
    pub(crate) fn stopping(&self) -> bool {
        self.stop.load(Ordering::Acquire) || self.owner_stop.load(Ordering::Acquire)
    }
}
#[derive(Clone)]
pub struct WriterReceipt {
    take: TakeId,
    count: usize,
    state: Arc<Mutex<WriterReport>>,
    pub(crate) signals: Arc<Signals>,
}
#[derive(Clone)]
pub(crate) struct Reporter(WriterReceipt);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriterPending {
    Busy,
    Running(WriterPhase),
}

impl WriterReceipt {
    pub fn take_id(&self) -> TakeId {
        self.take
    }
    pub fn track_id(&self, index: usize) -> Option<TrackId> {
        (index < self.count).then_some(TrackId {
            take: self.take,
            index: index as u16,
        })
    }
    pub fn try_report(&self) -> Option<WriterReport> {
        let mut report = self.state.try_lock().ok()?.clone();
        report.was_ready = self.signals.ready.load(Ordering::Acquire);
        report.stop_requested = self.signals.stopping();
        if !matches!(report.phase, WriterPhase::Finished | WriterPhase::Joined) {
            if report.stop_requested {
                report.phase = WriterPhase::Finalizing;
            } else if report.was_ready {
                report.phase = WriterPhase::Ready;
            }
        }
        let closing = self.signals.closing.load(Ordering::Acquire);
        for track in &mut report.tracks {
            track.samples_accepted =
                self.signals.accepted[track.id.index()].load(Ordering::Acquire);
            if track.phase == TrackPhase::Open && closing & (1 << track.id.index()) != 0 {
                track.phase = TrackPhase::Closing;
            }
        }
        Some(report)
    }
    pub fn try_complete(&self) -> Option<WriterReport> {
        self.try_report().filter(|r| r.phase == WriterPhase::Joined)
    }
    /// Off-UI bounded wait; a timeout retains this same completion handle.
    pub fn wait_timeout(&self, timeout: Duration) -> Result<WriterReport, WriterPending> {
        let start = Instant::now();
        loop {
            let pending = match self.try_report() {
                Some(report) if report.phase == WriterPhase::Joined => return Ok(report),
                Some(report) => WriterPending::Running(report.phase),
                None => WriterPending::Busy,
            };
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(pending);
            }
            std::thread::sleep(remaining.min(Duration::from_millis(5)));
        }
    }
}
pub(crate) fn status(
    tracks: &[crate::opname::Spoor],
    owner_stop: Arc<AtomicBool>,
) -> std::io::Result<(WriterReceipt, Reporter)> {
    // Retain the workspace's pre-1.99 Rust floor; the renamed try_update API
    // is newer. Exhaustion never wraps/reuses a take identity.
    #[allow(deprecated)]
    let id = NEXT_TAKE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| std::io::Error::other("take identity exhausted"))?;
    let take = TakeId(id);
    let receipt = WriterReceipt {
        take,
        count: tracks.len(),
        signals: Arc::new(Signals {
            owner_stop,
            ..Default::default()
        }),
        state: Arc::new(Mutex::new(WriterReport {
            take,
            directory: None,
            phase: WriterPhase::Opening,
            was_ready: false,
            stop_requested: false,
            outcome: None,
            tracks: tracks
                .iter()
                .enumerate()
                .map(|(i, t)| TrackReport {
                    id: TrackId {
                        take,
                        index: i as u16,
                    },
                    name: t.naam.clone(),
                    channels: t.kanalen,
                    phase: TrackPhase::Opening,
                    samples_accepted: 0,
                    samples_written: 0,
                })
                .collect(),
        })),
    };
    Ok((receipt.clone(), Reporter(receipt)))
}
pub(crate) fn error_text(error: &str) -> String {
    let mut end = error.len().min(MAX_WRITER_ERROR_BYTES);
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].to_owned()
}
impl Reporter {
    pub(crate) fn written(&self, index: usize, samples: u64) {
        if let Some(track) = self.0.state.lock().unwrap().tracks.get_mut(index) {
            track.samples_written = track.samples_written.saturating_add(samples);
        }
    }
    pub(crate) fn finalized(&self, index: usize, result: Result<(), String>) -> Option<String> {
        let mut state = self.0.state.lock().unwrap();
        if let Some(track) = state.tracks.get_mut(index) {
            let accepted = self.0.signals.accepted[index].load(Ordering::Acquire);
            let lost = (result.is_ok() && track.samples_written < accepted).then(|| {
                format!(
                    "track {index}: {} accepted samples were not written after take failure",
                    accepted - track.samples_written
                )
            });
            let result = if let Some(error) = &lost {
                Err(error.clone())
            } else {
                result
            };
            track.phase = TrackPhase::Finalized(result.map_err(|e| error_text(&e)));
            return lost;
        }
        None
    }
    pub(crate) fn directory(&self, directory: PathBuf) {
        self.0.state.lock().unwrap().directory = Some(directory);
    }
    pub(crate) fn track(&self, index: usize, phase: TrackPhase) {
        if let Some(track) = self.0.state.lock().unwrap().tracks.get_mut(index) {
            track.phase = phase;
        }
    }
    pub(crate) fn ready(&self) {
        self.0.signals.ready.store(true, Ordering::Release);
    }
    pub(crate) fn stopping(&self) -> bool {
        self.0.signals.stopping()
    }
    pub(crate) fn finalizing(&self) {
        self.0.state.lock().unwrap().phase = WriterPhase::Finalizing;
    }
    pub(crate) fn finished(&self, result: &Result<(), String>) {
        self.0.signals.closed.store(true, Ordering::Release);
        let mut state = self.0.state.lock().unwrap();
        for track in &mut state.tracks {
            if track.phase == TrackPhase::Opening {
                track.phase = TrackPhase::NotOpened(if result.is_ok() && self.stopping() {
                    NotOpened::Stopped
                } else {
                    NotOpened::InitializationFailed
                });
            } else if matches!(track.phase, TrackPhase::Open | TrackPhase::Closing) {
                track.phase = TrackPhase::Unconfirmed;
            }
        }
        state.phase = WriterPhase::Finished;
        state.outcome = Some(match result {
            Err(error) => WriterOutcome::Failed(error_text(error)),
            Ok(()) if self.0.signals.ready.load(Ordering::Acquire) => WriterOutcome::Finalized,
            Ok(()) => WriterOutcome::StoppedBeforeReady,
        });
    }
    pub(crate) fn joined(&self, result: &Result<(), String>) {
        if self.0.state.lock().unwrap().phase != WriterPhase::Finished {
            // No normal finish report means an abnormal exit. An opener can
            // panic after creating a file, so do not invent NotOpened evidence.
            self.0.signals.closed.store(true, Ordering::Release);
            let mut state = self.0.state.lock().unwrap();
            for track in &mut state.tracks {
                if matches!(
                    track.phase,
                    TrackPhase::Opening | TrackPhase::Open | TrackPhase::Closing
                ) {
                    track.phase = TrackPhase::Unconfirmed;
                }
            }
            state.outcome = Some(WriterOutcome::Failed(error_text(
                result
                    .as_ref()
                    .err()
                    .map(String::as_str)
                    .unwrap_or("writer exited without finalization evidence"),
            )));
        }
        let mut state = self.0.state.lock().unwrap();
        if let Err(error) = result {
            state.outcome = Some(WriterOutcome::Failed(error_text(error)));
        }
        state.phase = WriterPhase::Joined;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    #[test]
    fn polling_and_timed_wait_remain_nonblocking_while_status_is_busy() {
        let (receipt, _) = status(
            &[crate::opname::Spoor {
                naam: "Synthetic".into(),
                kanalen: 1,
            }],
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let held = receipt.clone();
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _lock = held.state.lock().unwrap();
            entered.send(()).unwrap();
            let _ = gate.recv_timeout(Duration::from_secs(2));
        });
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        let start = Instant::now();
        assert!(receipt.try_report().is_none());
        assert!(receipt.try_complete().is_none());
        assert_eq!(
            receipt.wait_timeout(Duration::from_millis(1)),
            Err(WriterPending::Busy)
        );
        let elapsed = start.elapsed();
        release.send(()).unwrap();
        holder.join().unwrap();
        assert!(elapsed < Duration::from_millis(100));
        assert_eq!(receipt.try_report().unwrap().phase, WriterPhase::Opening);
    }
}
