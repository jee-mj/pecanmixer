//! Local dynamic-arming decisions. A decision is not writer/server completion.
use crate::{
    bus_config::{ConfigRevision, ConfigSnapshot},
    opname::TrackId,
    recording::{Context, RecordingError, RecordingReceipt, RecordingRevision, Request},
};
use pecan_core::{
    backend::{Stamp, Stamped},
    graph::Momentopname,
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{atomic::Ordering, Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct RecordingArming {
    pub ticket: u64,
    pub origin: Stamped<Arc<Momentopname>>,
    pub expected_config: ConfigRevision,
    pub expected_recording: RecordingRevision,
    /// Complete desired armed set, validated against the unchanged layout.
    pub armed: Vec<String>,
    pub root: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmingOutcome {
    AppliedLocally,
    NoOpLocally,
    PartiallyApplied,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmingProgress {
    Queued,
    Adding,
    ApplyingRemovals,
    Completed(ArmingOutcome),
    Unconfirmed,
}
#[derive(Clone, Debug)]
pub struct ArmingAddition {
    pub bus: String,
    pub receipt: RecordingReceipt,
    pub new_take: bool,
}
impl ArmingAddition {
    pub(crate) fn is_live(&self) -> bool {
        self.receipt.0.live()
            && self
                .receipt
                .0
                .context
                .tracks
                .iter()
                .enumerate()
                .any(|(i, t)| t.key == self.bus && self.receipt.0.reserves(i))
    }
}
#[derive(Clone, Debug)]
pub struct ArmingReport {
    pub ticket: u64,
    pub source: Stamp,
    pub expected_config: ConfigRevision,
    pub expected_recording: RecordingRevision,
    pub progress: ArmingProgress,
    pub additions: Vec<ArmingAddition>,
    /// Accepted stable-ID stops; inspect take receipts for actual track closure.
    pub removed: Vec<TrackId>,
    pub withheld: Vec<TrackId>,
    pub failure: Option<RecordingError>,
    pub configuration: ConfigSnapshot,
    pub recording: RecordingRevision,
}
impl ArmingReport {
    pub fn is_decided(&self) -> bool {
        matches!(
            self.progress,
            ArmingProgress::Completed(_) | ArmingProgress::Unconfirmed
        )
    }
    pub fn is_success(&self) -> bool {
        matches!(
            self.progress,
            ArmingProgress::Completed(ArmingOutcome::AppliedLocally | ArmingOutcome::NoOpLocally)
        )
    }
}
pub(crate) struct ArmingRequest {
    pub report: Mutex<ArmingReport>,
}
#[derive(Clone)]
pub struct ArmingReceipt(pub(crate) Arc<ArmingRequest>);
impl std::fmt::Debug for ArmingReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ArmingReceipt(..)")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmingPending {
    Busy,
    Running(ArmingProgress),
}
impl ArmingReceipt {
    pub fn try_report(&self) -> Option<ArmingReport> {
        Some(self.0.report.try_lock().ok()?.clone())
    }
    /// Local decision only; never joins, opens files, or turns timeout into success.
    pub fn wait_timeout(&self, timeout: Duration) -> Result<ArmingReport, ArmingPending> {
        let started = Instant::now();
        loop {
            let pending = match self.try_report() {
                Some(r) if r.is_decided() => return Ok(r),
                Some(r) => ArmingPending::Running(r.progress),
                None => ArmingPending::Busy,
            };
            let left = timeout.saturating_sub(started.elapsed());
            if left.is_zero() {
                return Err(pending);
            }
            std::thread::sleep(left.min(Duration::from_millis(5)));
        }
    }
}
pub(crate) struct BoundTrack {
    pub request: Arc<Request>,
    pub id: TrackId,
    pub key: String,
}
pub(crate) struct Addition {
    pub context: Context,
    pub existing: Option<Arc<Request>>,
}
pub(crate) struct Work {
    pub receipt: ArmingReceipt,
    pub context: Context,
    pub before: crate::bus_config::IdleBusConfig,
    pub desired: BTreeSet<String>,
    pub baseline: Vec<BoundTrack>,
    pub removals: Vec<usize>,
    pub additions: Vec<Addition>,
    pub attempted: Vec<ArmingAddition>,
    pub root: PathBuf,
    pub claimed: bool,
    pub claim_revision: RecordingRevision,
    pub finished: bool,
}
impl Work {
    pub fn stop_pending(&self) {
        for addition in &self.attempted {
            if addition.new_take && !addition.receipt.0.started_once.load(Ordering::Acquire) {
                addition.receipt.0.request_stop();
                if addition.receipt.0.stage.load(Ordering::Acquire) == crate::recording::QUEUED {
                    addition
                        .receipt
                        .0
                        .reject(RecordingError::CancelledBeforeStart);
                }
            }
        }
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if !self.finished {
            self.stop_pending();
            self.receipt.0.report.lock().unwrap().progress = ArmingProgress::Unconfirmed;
        }
    }
}
