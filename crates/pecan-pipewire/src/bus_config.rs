//! Native idle configuration. Completion here is local, never graph observation.
use pecan_core::{
    domain::buses::BusConfig,
    graph::{BusOpname, Opdracht, Opties},
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

pub const MAX_CONFIG_PENDING: usize = 16;
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfigRevision {
    pub owner: u64,
    pub epoch: u64,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IdleBusConfig {
    pub buses: BusConfig,
    pub clock: Option<u32>,
    pub start_stop: bool,
}

#[derive(Clone, Debug)]
pub struct BusReadback {
    /// Local filter identity; its ports need not have appeared in the graph.
    pub node: Option<u32>,
    pub channels: usize,
    pub gains: Vec<f32>,
}
impl PartialEq for BusReadback {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
            && self.channels == other.channels
            && self.gains.len() == other.gains.len()
            && self
                .gains
                .iter()
                .zip(&other.gains)
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigSnapshot {
    pub revision: ConfigRevision,
    pub connected: bool,
    pub local: Option<BusReadback>,
    /// None after legacy configuration or a partial resize: do not invent
    /// bus grouping/mute/arming metadata from a raw channel/gain vector.
    pub configuration: Option<IdleBusConfig>,
    pub transport: Option<Opties>,
    pub recording: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IdleConfigIntent {
    pub ticket: u64,
    pub expected: ConfigRevision,
    pub configuration: IdleBusConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResizeFailure {
    InvalidCount,
    AddFailed { channel: usize },
    RemoveFailed { channel: usize, code: i32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelResizeResult {
    pub before: usize,
    pub requested: usize,
    pub actual: usize,
    pub failure: Option<ResizeFailure>,
}
impl ChannelResizeResult {
    pub fn is_complete(&self) -> bool {
        self.failure.is_none() && self.actual == self.requested
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    Busy,
    QueueFull,
    Stopped,
    Disconnected,
    StaleRevision,
    Unavailable,
    InvalidInput,
    UnsupportedMidi,
    RecordingActive,
    LayoutMigrationRequired,
    RevisionExhausted,
    ResizeFailed,
    ReadbackMismatch,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalConfigOutcome {
    Applied,
    NoOp,
    Rejected(ConfigError),
    PartiallyApplied(ConfigError),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigCompletion {
    pub outcome: LocalConfigOutcome,
    pub snapshot: ConfigSnapshot,
    pub resize: Option<ChannelResizeResult>,
    /// A claimed operation can finish after cancellation. It is not rollback.
    pub cancellation_requested: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigProgress {
    Queued,
    Applying,
    Completed(ConfigCompletion),
    Unconfirmed,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigReport {
    pub ticket: u64,
    pub expected: ConfigRevision,
    pub progress: ConfigProgress,
}
#[derive(Clone, Debug)]
pub struct ConfigReceipt(Arc<Mutex<ConfigReport>>);
impl ConfigReceipt {
    /// Never waits on the status lock, worker, filesystem or server.
    pub fn try_report(&self) -> Option<ConfigReport> {
        self.0.try_lock().ok().map(|r| r.clone())
    }
}
pub(crate) struct Prepared {
    pub intent: IdleConfigIntent,
    pub channels: usize,
    pub gains: Vec<f32>,
    pub options: Opties,
    receipt: ConfigReceipt,
    completed: bool,
}
impl Prepared {
    pub(crate) fn new(intent: IdleConfigIntent) -> Result<(Self, ConfigReceipt), ConfigError> {
        let config = &intent.configuration;
        if config.buses.widths.len() > 64
            || config.buses.states.len() > 128
            || config
                .buses
                .states
                .iter()
                .any(|(key, state)| key.len() > 256 || key.contains('\0') || !state.db.is_finite())
        {
            return Err(ConfigError::InvalidInput);
        }
        let defs = config
            .buses
            .definitions()
            .map_err(|_| ConfigError::InvalidInput)?;
        let Opdracht::ZetBus { kanalen, gain } = config
            .buses
            .backend_config()
            .map_err(|_| ConfigError::InvalidInput)?
        else {
            unreachable!()
        };
        if kanalen > 64
            || gain
                .iter()
                .any(|g| !g.is_finite() || !(0.0..=4.0).contains(g))
        {
            return Err(ConfigError::InvalidInput);
        }
        if config.buses.midi || config.clock.is_some() || config.start_stop {
            return Err(ConfigError::UnsupportedMidi);
        }
        let options = Opties {
            bussen: defs
                .into_iter()
                .filter(|d| config.buses.state(&d.key).armed)
                .map(|d| BusOpname {
                    naam: d.file,
                    kanalen: d.channels,
                })
                .collect(),
            midi: false,
            klok: None,
            start_stop: false,
        };
        // Retain bounded owned metadata, not the caller's spare Vec capacity.
        let intent = intent.clone();
        let receipt = ConfigReceipt(Arc::new(Mutex::new(ConfigReport {
            ticket: intent.ticket,
            expected: intent.expected,
            progress: ConfigProgress::Queued,
        })));
        Ok((
            Self {
                intent,
                channels: kanalen,
                gains: gain,
                options,
                receipt: receipt.clone(),
                completed: false,
            },
            receipt,
        ))
    }
    pub(crate) fn applying(&self) {
        self.receipt.0.lock().unwrap().progress = ConfigProgress::Applying;
    }
    pub(crate) fn finish(mut self, completion: ConfigCompletion) {
        self.receipt.0.lock().unwrap().progress = ConfigProgress::Completed(completion);
        self.completed = true;
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if !self.completed {
            self.receipt.0.lock().unwrap().progress = ConfigProgress::Unconfirmed;
        }
    }
}

pub(crate) struct Registry {
    pub snapshot: ConfigSnapshot,
    pub exhausted: bool,
    /// Changes for actual configuration/readback transitions, but not for the
    /// aggregate recording flag. Kept internal; public CAS revisions still
    /// advance for both. Pending starts must survive recording-only changes.
    pub identity: Arc<()>,
}
impl Default for Registry {
    fn default() -> Self {
        // Never reuse a backend authority, including after identity exhaustion.
        #[allow(deprecated)]
        let owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .unwrap_or(0);
        Self {
            snapshot: ConfigSnapshot {
                revision: ConfigRevision {
                    owner,
                    ..Default::default()
                },
                ..Default::default()
            },
            exhausted: owner == 0,
            identity: Arc::new(()),
        }
    }
}
impl Registry {
    pub fn reset(&mut self, epoch: u64) {
        self.identity = Arc::new(());
        let owner = self.snapshot.revision.owner;
        self.snapshot = ConfigSnapshot {
            revision: ConfigRevision {
                owner,
                epoch,
                revision: 0,
            },
            ..Default::default()
        };
        self.exhausted = owner == 0;
    }
    pub fn bump(&mut self) {
        self.identity = Arc::new(());
        self.bump_recording();
    }
    fn bump_recording(&mut self) {
        match self.snapshot.revision.revision.checked_add(1) {
            Some(revision) => self.snapshot.revision.revision = revision,
            None => self.exhausted = true,
        }
    }
    pub fn refresh(&mut self, local: Option<BusReadback>, recording: bool, legacy: bool) {
        let configuration_changed = legacy || self.snapshot.local != local;
        if configuration_changed || self.snapshot.recording != recording {
            if configuration_changed {
                self.snapshot.configuration = None;
                self.snapshot.transport = None;
            }
            self.snapshot.local = local;
            self.snapshot.recording = recording;
            if configuration_changed {
                self.bump();
            } else {
                self.bump_recording();
            }
        }
    }
}

pub(crate) trait ConfigTarget {
    fn readback(&self) -> Option<BusReadback>;
    fn recording(&self) -> bool;
    fn config_locked(&self) -> bool {
        self.recording()
    }
    fn routed(&self) -> bool;
    fn resize(&mut self, channels: usize) -> ChannelResizeResult;
    fn gains(&mut self, gains: &[f32]);
    fn options(&mut self, options: Opties);
    fn options_match(&self, options: &Opties) -> bool;
}

/// Share this allocator loop with the actual filter and fault-injected tests.
pub(crate) fn resize_ports<P>(
    ports: &mut Vec<P>,
    requested: usize,
    mut add: impl FnMut(usize) -> Option<P>,
    mut remove: impl FnMut(&P) -> i32,
) -> ChannelResizeResult {
    let mut result = ChannelResizeResult {
        before: ports.len(),
        requested,
        actual: ports.len(),
        failure: None,
    };
    if !(1..=64).contains(&requested) {
        result.failure = Some(ResizeFailure::InvalidCount);
        return result;
    }
    while ports.len() > requested {
        let code = remove(ports.last().unwrap());
        if code < 0 {
            result.failure = Some(ResizeFailure::RemoveFailed {
                channel: ports.len() - 1,
                code,
            });
            break;
        }
        ports.pop();
    }
    while ports.len() < requested && result.failure.is_none() {
        match add(ports.len()) {
            Some(port) => ports.push(port),
            None => {
                result.failure = Some(ResizeFailure::AddFailed {
                    channel: ports.len(),
                });
                break;
            }
        }
    }
    result.actual = ports.len();
    result
}
