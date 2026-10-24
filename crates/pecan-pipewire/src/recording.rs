//! Native audio-only take receipts. No UI adoption or server success contract.
use crate::{
    bus_config::ConfigRevision,
    opname::{
        TakeId, TrackId, TrackedWriter, WriterPhase, WriterReport, WriterSendError,
        MAX_WRITER_SAMPLES,
    },
};
use pecan_core::{
    backend::{Stamp, Stamped},
    graph::Momentopname,
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

pub const RECENT_TAKES: usize = 16;
#[derive(Clone, Debug)]
pub struct RecordingStart {
    pub ticket: u64,
    pub origin: Stamped<Arc<Momentopname>>,
    pub expected_config: ConfigRevision,
    pub root: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingStop {
    All,
    Take(TakeId),
    Track(TrackId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingError {
    Busy,
    QueueFull,
    Stopped,
    Disconnected,
    StaleConfig,
    OriginUnavailable,
    InvalidOrigin,
    ConfigurationUnknown,
    Unavailable,
    NoArmedTracks,
    AlreadyRecording,
    InvalidInput,
    UnknownTake,
    UnknownTrack,
    CancelledBeforeStart,
    Writer(String),
    Ingress(WriterSendError),
    FormatChanged,
    QuantumTooLarge,
    CaptureUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingProgress {
    Queued,
    Opening,
    StartedLocally,
    Finalizing,
    JoinedSuccess,
    Failed,
    Unconfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingTrack {
    pub id: TrackId,
    pub key: String,
    pub name: String,
    pub channels: Vec<usize>,
    pub ports: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecordingReport {
    pub ticket: u64,
    pub source: Stamp,
    pub config: ConfigRevision,
    pub progress: RecordingProgress,
    pub started_once: bool,
    pub stop_requested: bool,
    /// Accepted per-track stop requests; actual file close is in writer.tracks.
    pub track_stops: u64,
    pub tracks: Arc<Vec<RecordingTrack>>,
    pub writer: Option<WriterReport>,
    pub failure: Option<RecordingError>,
}
impl RecordingReport {
    pub fn is_success(&self) -> bool {
        self.progress == RecordingProgress::JoinedSuccess
    }
    pub fn is_complete(&self) -> bool {
        self.writer
            .as_ref()
            .is_some_and(|w| w.phase == WriterPhase::Joined)
            || (self.writer.is_none() && self.progress == RecordingProgress::Failed)
    }
}
#[derive(Clone)]
pub struct RecordingReceipt(pub(crate) Arc<Request>);
impl std::fmt::Debug for RecordingReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecordingReceipt(..)")
    }
}
#[derive(Debug)]
pub enum RecordingStopResult {
    NoOp,
    Retained(RecordingReceipt),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingPending {
    Busy,
    Running(RecordingProgress),
}
impl RecordingReceipt {
    pub fn try_report(&self) -> Option<RecordingReport> {
        self.0.report()
    }
    /// Off-UI bounded polling; never opens files or joins a thread.
    pub fn wait_timeout(&self, timeout: Duration) -> Result<RecordingReport, RecordingPending> {
        let begin = Instant::now();
        loop {
            let pending = match self.try_report() {
                Some(report) if report.is_complete() => return Ok(report),
                Some(report) => RecordingPending::Running(report.progress),
                None => RecordingPending::Busy,
            };
            let remaining = timeout.saturating_sub(begin.elapsed());
            if remaining.is_zero() {
                return Err(pending);
            }
            std::thread::sleep(remaining.min(Duration::from_millis(5)));
        }
    }
}
pub(crate) const QUEUED: u8 = 0;
pub(crate) const OPENING: u8 = 1;
const REJECTED: u8 = 2;
const UNCONFIRMED: u8 = 3;
#[derive(Clone)]
pub(crate) struct TrackSpec {
    pub key: String,
    pub name: String,
    pub channels: Vec<usize>,
    pub ports: Vec<u32>,
}
pub(crate) struct Context {
    pub source: Stamp,
    pub config: ConfigRevision,
    pub ticket: u64,
    pub node: u32,
    pub serial: Option<u64>,
    pub rate: u32,
    pub objects: Vec<(u32, Option<u64>)>,
    pub tracks: Vec<TrackSpec>,
}
pub(crate) struct Installed {
    pub writer: TrackedWriter,
    pub tracks: Arc<Vec<RecordingTrack>>,
}
pub(crate) struct Request {
    pub context: Context,
    pub stage: AtomicU8,
    pub installed: OnceLock<Installed>,
    pub stop: AtomicBool,
    pub track_stops: AtomicU64,
    pub attached: AtomicBool,
    pub started_once: AtomicBool,
    failure: Mutex<Option<RecordingError>>,
    fault: AtomicU8,
}
impl Request {
    pub fn new(context: Context) -> Arc<Self> {
        Arc::new(Self {
            context,
            stage: AtomicU8::new(QUEUED),
            installed: OnceLock::new(),
            stop: AtomicBool::new(false),
            track_stops: AtomicU64::new(0),
            attached: AtomicBool::new(false),
            started_once: AtomicBool::new(false),
            failure: Mutex::new(None),
            fault: AtomicU8::new(0),
        })
    }
    pub fn reject(&self, error: RecordingError) {
        self.fail(error);
        self.stage.store(REJECTED, Ordering::Release);
    }
    pub fn fail(&self, error: RecordingError) {
        let mut failure = self.failure.lock().unwrap();
        if failure.is_none() {
            *failure = Some(error);
        }
    }
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.attached.store(false, Ordering::Release);
        if let Some(take) = self.installed.get() {
            take.writer.sender.request_stop();
        }
    }
    pub fn request_track_stop(&self, index: usize) {
        let Some(installed) = self.installed.get() else {
            return;
        };
        let mask = self.track_stops.fetch_or(1_u64 << index, Ordering::AcqRel) | (1_u64 << index);
        let all = if installed.tracks.len() == 64 {
            u64::MAX
        } else {
            (1_u64 << installed.tracks.len()) - 1
        };
        if mask & all == all {
            self.request_stop();
        }
    }
    pub fn owned(&self) -> bool {
        self.stage.load(Ordering::Acquire) != QUEUED
            && !self.report().is_some_and(|r| r.is_complete())
    }
    pub fn live(&self) -> bool {
        self.attached.load(Ordering::Acquire)
            && !self.stop.load(Ordering::Acquire)
            && self.installed.get().is_some_and(|i| {
                !i.writer.receipt.signals.stopping()
                    && !i.writer.receipt.signals.closed.load(Ordering::Acquire)
            })
    }
    pub fn ingress_fault(&self, fault: u8) {
        let _ = self
            .fault
            .compare_exchange(0, fault, Ordering::AcqRel, Ordering::Acquire);
        self.request_stop();
    }
    fn report(&self) -> Option<RecordingReport> {
        let mut failure = self.failure.try_lock().ok()?.clone();
        if failure.is_none() {
            failure = match self.fault.load(Ordering::Acquire) {
                1 => Some(RecordingError::Ingress(WriterSendError::Full)),
                2 => Some(RecordingError::Ingress(WriterSendError::Busy)),
                3 => Some(RecordingError::QuantumTooLarge),
                4 => Some(RecordingError::FormatChanged),
                5 => Some(RecordingError::CaptureUnavailable),
                6 => Some(RecordingError::Ingress(WriterSendError::InvalidSamples)),
                7 => Some(RecordingError::Ingress(WriterSendError::NotReady)),
                8 => Some(RecordingError::Ingress(WriterSendError::WrongTrack)),
                _ => None,
            };
        }
        let installed = self.installed.get();
        let writer = match installed {
            Some(i) => Some(i.writer.receipt.try_report()?),
            None => None,
        };
        let started_once = self.started_once.load(Ordering::Acquire);
        let stop_requested =
            self.stop.load(Ordering::Acquire) || writer.as_ref().is_some_and(|w| w.stop_requested);
        if let Some(w) = &writer {
            if failure.is_none() {
                if let Some(crate::opname::WriterOutcome::Failed(error)) = &w.outcome {
                    failure = Some(RecordingError::Writer(error.clone()));
                }
            }
            if failure.is_none() && w.phase == WriterPhase::Joined && !started_once {
                failure = Some(RecordingError::CancelledBeforeStart);
            }
        }
        let stage = self.stage.load(Ordering::Acquire);
        let progress = if stage == UNCONFIRMED {
            RecordingProgress::Unconfirmed
        } else if writer
            .as_ref()
            .is_some_and(|w| w.phase == WriterPhase::Joined)
        {
            if failure.is_none() && started_once && writer.as_ref().unwrap().is_success() {
                RecordingProgress::JoinedSuccess
            } else {
                RecordingProgress::Failed
            }
        } else if writer.is_none() && stage == REJECTED {
            RecordingProgress::Failed
        } else if stop_requested
            || failure.is_some()
            || writer
                .as_ref()
                .is_some_and(|w| matches!(w.phase, WriterPhase::Finalizing | WriterPhase::Finished))
        {
            RecordingProgress::Finalizing
        } else if started_once {
            RecordingProgress::StartedLocally
        } else if stage == QUEUED {
            RecordingProgress::Queued
        } else {
            RecordingProgress::Opening
        };
        Some(RecordingReport {
            ticket: self.context.ticket,
            source: self.context.source,
            config: self.context.config,
            progress,
            started_once,
            stop_requested,
            track_stops: self.track_stops.load(Ordering::Acquire),
            tracks: installed.map(|i| i.tracks.clone()).unwrap_or_default(),
            writer,
            failure,
        })
    }
}
pub(crate) struct PreparedStart {
    pub request: Arc<Request>,
    pub root: PathBuf,
    armed: bool,
}
impl PreparedStart {
    pub fn new(request: Arc<Request>, root: PathBuf) -> Self {
        Self {
            request,
            root,
            armed: true,
        }
    }
    pub fn disarm(&mut self) {
        self.armed = false;
    }
}
impl Drop for PreparedStart {
    fn drop(&mut self) {
        if self.armed && self.request.stage.load(Ordering::Acquire) != REJECTED {
            self.request.stage.store(UNCONFIRMED, Ordering::Release);
            self.request.request_stop();
        }
    }
}
#[derive(Default)]
pub(crate) struct Registry {
    pub active: Option<Arc<Request>>,
    pub recent: VecDeque<Arc<Request>>,
}
impl Registry {
    pub fn reap(&mut self) {
        if self
            .active
            .as_ref()
            .is_some_and(|r| r.report().is_some_and(|r| r.is_complete()))
        {
            self.recent.push_back(self.active.take().unwrap());
            while self.recent.len() > RECENT_TAKES {
                self.recent.pop_front();
            }
        }
    }
    pub fn stop_all(
        &mut self,
        failure: Option<RecordingError>,
        queued: bool,
    ) -> RecordingStopResult {
        let Some(request) = &self.active else {
            return RecordingStopResult::NoOp;
        };
        if !queued && request.stage.load(Ordering::Acquire) == QUEUED {
            return RecordingStopResult::NoOp;
        }
        if !request.stop.load(Ordering::Acquire) {
            if let Some(failure) = failure {
                request.fail(failure);
            }
        }
        request.request_stop();
        if request.stage.load(Ordering::Acquire) == QUEUED {
            let failure = request
                .failure
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(RecordingError::CancelledBeforeStart);
            request.reject(failure);
        }
        RecordingStopResult::Retained(RecordingReceipt(request.clone()))
    }
}

/// Worker-loop capture only. It retains the installed immutable take mapping.
#[derive(Clone)]
pub(crate) struct Capture(pub Arc<Request>);
pub(crate) type NativeOpnemer = std::rc::Rc<std::cell::RefCell<Option<Capture>>>;
impl Capture {
    pub fn accepts(&self, frames: usize, rate_num: u32, rate_denom: u32) -> bool {
        if !self.0.live() {
            return false;
        }
        if rate_num == 0
            || u64::from(rate_denom) != u64::from(self.0.context.rate) * u64::from(rate_num)
        {
            self.0.ingress_fault(4);
            return false;
        }
        let width = self
            .0
            .context
            .tracks
            .iter()
            .map(|t| t.channels.len())
            .max()
            .unwrap_or(1);
        if frames > MAX_WRITER_SAMPLES / width {
            self.0.ingress_fault(3);
            return false;
        }
        true
    }
    pub fn ingest(&self, blocks: &[Vec<f32>], frames: usize) {
        if frames == 0 {
            return;
        }
        let Some(installed) = self.0.installed.get() else {
            return;
        };
        if !self.0.live() {
            return;
        }
        let closing = self.0.track_stops.load(Ordering::Acquire)
            | installed
                .writer
                .receipt
                .signals
                .closing
                .load(Ordering::Acquire);
        for track in installed.tracks.iter() {
            if closing & (1 << track.id.index()) != 0 {
                continue;
            }
            if frames > MAX_WRITER_SAMPLES / track.channels.len() {
                self.0.ingress_fault(3);
                return;
            }
            if track
                .channels
                .iter()
                .any(|c| blocks.get(*c).is_none_or(|b| b.len() != frames))
            {
                self.0.ingress_fault(5);
                return;
            }
            let mut samples = Vec::with_capacity(frames * track.channels.len());
            for frame in 0..frames {
                for channel in &track.channels {
                    samples.push(blocks[*channel][frame]);
                }
            }
            match installed.writer.sender.try_audio(track.id, samples) {
                Ok(()) | Err(WriterSendError::Closed) => {}
                Err(WriterSendError::Full) => {
                    self.0.ingress_fault(1);
                    return;
                }
                Err(WriterSendError::Busy) => {
                    self.0.ingress_fault(2);
                    return;
                }
                Err(error) => {
                    self.0.ingress_fault(match error {
                        WriterSendError::NotReady => 7,
                        WriterSendError::WrongTrack => 8,
                        _ => 6,
                    });
                    return;
                }
            }
        }
    }
}
