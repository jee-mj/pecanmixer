//! Idle structural bus migration evidence; no UI or active-take remapping.
use crate::{
    bus_config::{
        BusReadback, ChannelResizeResult, ConfigError, ConfigRevision, ConfigSnapshot,
        IdleBusConfig,
    },
    recording::RecordingRevision,
};
use pecan_core::{
    backend::{Stamp, Stamped},
    graph::{Momentopname, Opdracht, Opties, Soort},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// One plan retains at most this many initial links, independent of chunking.
pub const MAX_MIGRATION_LINKS: usize = 256;
pub const MAX_MIGRATION_CALLS: usize = MAX_MIGRATION_LINKS * 2;
pub const OBSERVATION_LIMIT: Duration = Duration::from_secs(2);
pub const TOTAL_LIMIT: Duration = Duration::from_secs(30);
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutAction {
    AddStereo,
    Split { bus: String },
    Join { bus: String },
    Remove { bus: String },
}
#[derive(Clone, Debug)]
pub struct LayoutIntent {
    pub ticket: u64,
    pub origin: Stamped<Arc<Momentopname>>,
    pub expected_config: ConfigRevision,
    pub expected_recording: RecordingRevision,
    pub action: LayoutAction,
}
/// One validated saved layout, including initialization when logical metadata
/// is unknown. Retain the original authenticated witness. Only equal capacity
/// or growth is supported; existing channel indices and routes stay unchanged.
#[derive(Clone, Debug)]
pub struct SavedLayoutIntent {
    pub ticket: u64,
    pub origin: pecan_core::history_identity::HistoryGraph,
    pub expected_config: ConfigRevision,
    pub expected_recording: RecordingRevision,
    pub configuration: IdleBusConfig,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationError {
    Busy,
    QueueFull,
    Stopped,
    Disconnected,
    StaleConfig,
    StaleRecording,
    OriginUnavailable,
    InvalidObject,
    InvalidInput,
    RecordingActive,
    TooManyRoutes,
    CallBudget,
    ExternalConflict,
    ObservationTimedOut,
    Cancelled,
    Local(String),
    Configuration(ConfigError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    AppliedLocally,
    Observed,
    NoOpLocally,
    Rejected,
    PartiallyApplied,
    Unconfirmed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationProgress {
    Queued,
    Connecting,
    AwaitingNew,
    Disconnecting,
    AwaitingOld,
    ApplyingLocal,
    AwaitingPorts,
    Cancelling,
    Completed(MigrationOutcome),
    Unconfirmed,
}
#[derive(Clone, Debug)]
pub struct MigrationReport {
    pub ticket: u64,
    pub source: Stamp,
    pub expected_config: ConfigRevision,
    pub expected_recording: RecordingRevision,
    pub progress: MigrationProgress,
    pub configuration: ConfigSnapshot,
    pub proposed: IdleBusConfig,
    pub attempted: usize,
    pub submitted: usize,
    pub no_ops: usize,
    pub in_progress: usize,
    pub new_observed: Option<Stamp>,
    pub old_observed: Option<Stamp>,
    pub ports_observed: Option<Stamp>,
    /// Authenticated owned node/final input generations for appended ports.
    /// Present only after a successful growth observation; local resize alone
    /// cannot supply this proof. Retain across ordinary graph/cache revisions.
    pub observed_inputs: Option<pecan_core::history_identity::HistoryGraph>,
    pub resize: Option<ChannelResizeResult>,
    pub local_readback: Option<BusReadback>,
    pub cancellation_requested: bool,
    pub failure: Option<MigrationError>,
}
impl MigrationReport {
    pub fn is_decided(&self) -> bool {
        matches!(
            self.progress,
            MigrationProgress::Completed(_) | MigrationProgress::Unconfirmed
        )
    }
    pub fn is_success(&self) -> bool {
        matches!(
            self.progress,
            MigrationProgress::Completed(
                MigrationOutcome::AppliedLocally
                    | MigrationOutcome::Observed
                    | MigrationOutcome::NoOpLocally
            )
        )
    }
}
#[derive(Clone, Debug)]
pub struct MigrationReceipt(pub(crate) Arc<Mutex<MigrationReport>>);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationPending {
    Busy,
    Running(MigrationProgress),
}
impl MigrationReceipt {
    pub fn try_report(&self) -> Option<MigrationReport> {
        Some(self.0.try_lock().ok()?.clone())
    }
    pub fn wait_timeout(&self, timeout: Duration) -> Result<MigrationReport, MigrationPending> {
        let start = Instant::now();
        loop {
            let pending = match self.try_report() {
                Some(r) if r.is_decided() => return Ok(r),
                Some(r) => MigrationPending::Running(r.progress),
                None => MigrationPending::Busy,
            };
            let left = timeout.saturating_sub(start.elapsed());
            if left.is_zero() {
                return Err(pending);
            }
            std::thread::sleep(left.min(Duration::from_millis(5)));
        }
    }
}
pub(crate) type Pair = (u32, u32);
#[derive(Clone)]
pub(crate) struct Port {
    pub id: u32,
    pub node: u32,
    pub name: String,
    pub input: bool,
    pub kind: Soort,
    pub generation: u64,
}
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Begin,
    Connect(Pair),
    Disconnect {
        pair: Pair,
        id: u32,
        generation: u64,
    },
    Apply,
}
#[derive(Clone)]
pub(crate) struct Tag {
    pub receipt: MigrationReceipt,
    pub kind: Kind,
}
pub(crate) struct Work {
    pub receipt: MigrationReceipt,
    pub source: Stamp,
    pub node: u32,
    pub serial: Option<u64>,
    pub node_generation: u64,
    pub ports: Vec<Port>,
    /// Bounded by the validated 64-channel target. Latched at first coherent
    /// publication, never rebound to a later generation.
    pub appended: Vec<Port>,
    /// Saved initialization may replace unknown transport metadata deliberately.
    /// Ordinary edits retain their existing known-options preflight.
    pub saved_layout: bool,
    pub source_ports: Vec<Port>,
    pub source_nodes: BTreeMap<u32, (u64, Option<u64>)>,
    pub affected: BTreeSet<u32>,
    pub initial: BTreeSet<Pair>,
    pub desired: BTreeSet<Pair>,
    pub obsolete: BTreeSet<Pair>,
    pub connected: BTreeSet<Pair>,
    pub destroyed: BTreeSet<(u32, u64)>,
    pub proposed: IdleBusConfig,
    pub gains: Vec<f32>,
    pub options: Opties,
    pub channels: usize,
    pub expected_identity: Arc<()>,
    pub expected_recording: RecordingRevision,
    pub phase: MigrationProgress,
    pub claimed: bool,
    pub queued: bool,
    pub in_progress: bool,
    pub cancelled: Option<MigrationError>,
    pub local_applied: bool,
    pub start: Instant,
    pub deadline: Instant,
    pub last_submission: Option<Stamp>,
    pub finished: bool,
}
impl Work {
    pub fn tag(&self, kind: Kind) -> Tag {
        Tag {
            receipt: self.receipt.clone(),
            kind,
        }
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if !self.finished {
            let mut report = self.receipt.0.lock().unwrap();
            report.progress = MigrationProgress::Unconfirmed;
        }
    }
}
pub(crate) enum Claim {
    Skip,
    Control,
    Execute(Opdracht),
}
