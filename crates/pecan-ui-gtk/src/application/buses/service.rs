//! UI-side projections of the existing native protocols. No backend queue,
//! writer, identity allocation or completion implementation is duplicated here.
use super::*;
use pecan_pipewire::{
    bus_config::{ConfigError, ConfigReceipt, ConfigReport, ConfigSnapshot, IdleConfigIntent},
    opname::{TakeId, TrackId, TrackPhase as NativeTrackPhase, WriterOutcome, WriterPhase},
    recording::{
        RecordingBusStart, RecordingBusStartResult, RecordingError, RecordingProgress,
        RecordingReceipt, RecordingRevision, RecordingStart, RecordingStop,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration(ConfigError),
    Recording(RecordingError),
    Owner(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Source {
    pub ticket: u64,
    pub epoch: u64,
    pub revision: u64,
    pub owner: u64,
    pub config_epoch: u64,
    pub config_revision: u64,
}
impl Source {
    pub fn new(
        ticket: u64,
        graph: &Graph,
        config: pecan_pipewire::bus_config::ConfigRevision,
    ) -> Self {
        Self {
            ticket,
            epoch: graph.stamp.epoch,
            revision: graph.stamp.revision,
            owner: config.owner,
            config_epoch: config.epoch,
            config_revision: config.revision,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrackKey {
    Native(TrackId),
    #[cfg(test)]
    Synthetic(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TakeKey {
    Native(TakeId),
    #[cfg(test)]
    Synthetic(u64),
}
#[derive(Clone, Debug, PartialEq)]
pub struct TakeStatus {
    pub source: Source,
    pub progress: RecordingProgress,
    pub writer: Option<WriterPhase>,
    pub writer_outcome: Option<WriterOutcome>,
    pub failure: Option<RecordingError>,
    pub complete: bool,
    pub take: Option<TakeKey>,
    pub directory: Option<std::path::PathBuf>,
    pub tracks: Vec<ui::Track<TrackKey>>,
}
pub trait ConfigPoll {
    fn report(&self) -> Option<ConfigReport>;
}
pub trait TakePoll {
    fn report(&self) -> Option<TakeStatus>;
}
pub struct Readback {
    pub revision: RecordingRevision,
    pub config: ConfigSnapshot,
    pub takes: Vec<Rc<dyn TakePoll>>,
}
pub struct Started {
    pub receipt: Rc<dyn TakePoll>,
    pub no_op: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    All,
    Track(TrackKey),
    Take(TakeKey),
}
pub trait Service {
    fn read(&self) -> Option<Readback>;
    fn configure(&self, intent: IdleConfigIntent) -> Result<Rc<dyn ConfigPoll>, Error>;
    fn start_all(&self, intent: RecordingStart) -> Result<Started, Error>;
    fn start_bus(&self, intent: RecordingBusStart) -> Result<Started, Error>;
    fn stop(&self, target: Stop) -> Result<Vec<Rc<dyn TakePoll>>, Error>;
}
struct NativeConfig(ConfigReceipt);
impl ConfigPoll for NativeConfig {
    fn report(&self) -> Option<ConfigReport> {
        self.0.try_report()
    }
}
struct NativeTake(RecordingReceipt);
impl TakePoll for NativeTake {
    fn report(&self) -> Option<TakeStatus> {
        let r = self.0.try_report()?;
        let source = Source {
            ticket: r.ticket,
            epoch: r.source.epoch,
            revision: r.source.revision,
            owner: r.config.owner,
            config_epoch: r.config.epoch,
            config_revision: r.config.revision,
        };
        let writer = r.writer.as_ref();
        let tracks = r
            .tracks
            .iter()
            .map(|track| {
                let phase = writer
                    .and_then(|w| w.tracks.iter().find(|t| t.id == track.id))
                    .map_or(ui::TrackPhase::Unconfirmed, |t| match &t.phase {
                        NativeTrackPhase::Opening => ui::TrackPhase::Starting,
                        NativeTrackPhase::Open => {
                            if r.started_once {
                                ui::TrackPhase::Recording
                            } else {
                                ui::TrackPhase::WriterReady
                            }
                        }
                        NativeTrackPhase::Closing => ui::TrackPhase::Finalizing,
                        NativeTrackPhase::Finalized(result) => {
                            if writer.is_some_and(|w| w.phase == WriterPhase::Joined) {
                                ui::TrackPhase::Joined(result.clone())
                            } else {
                                ui::TrackPhase::Finalized(result.clone())
                            }
                        }
                        NativeTrackPhase::NotOpened(reason) => {
                            ui::TrackPhase::NotOpened(format!("{reason:?}"))
                        }
                        NativeTrackPhase::Unconfirmed => ui::TrackPhase::Unconfirmed,
                    });
                ui::Track {
                    identity: TrackKey::Native(track.id),
                    bus_key: track.key.clone(),
                    phase,
                    elapsed: None,
                }
            })
            .collect();
        Some(TakeStatus {
            source,
            progress: r.progress,
            writer: writer.map(|w| w.phase),
            writer_outcome: writer.and_then(|w| w.outcome.clone()),
            failure: r.failure.clone(),
            complete: r.is_complete(),
            take: writer.map(|w| TakeKey::Native(w.take)),
            directory: writer.and_then(|w| w.directory.clone()),
            tracks,
        })
    }
}
pub(crate) struct Native(pub Rc<pecan_pipewire::graaf::Graaf>);
impl Service for Native {
    fn read(&self) -> Option<Readback> {
        let r = self.0.recording_snapshot()?;
        Some(Readback {
            revision: r.revision,
            config: r.configuration,
            takes: r
                .takes
                .into_iter()
                .map(|r| Rc::new(NativeTake(r)) as Rc<dyn TakePoll>)
                .collect(),
        })
    }
    fn configure(&self, intent: IdleConfigIntent) -> Result<Rc<dyn ConfigPoll>, Error> {
        self.0
            .configure_idle(intent)
            .map(|r| Rc::new(NativeConfig(r)) as Rc<dyn ConfigPoll>)
            .map_err(Error::Configuration)
    }
    fn start_all(&self, intent: RecordingStart) -> Result<Started, Error> {
        self.0
            .start_recording(intent)
            .map(|r| Started {
                receipt: Rc::new(NativeTake(r)),
                no_op: false,
            })
            .map_err(Error::Recording)
    }
    fn start_bus(&self, intent: RecordingBusStart) -> Result<Started, Error> {
        self.0
            .start_recording_bus(intent)
            .map(|r| match r {
                RecordingBusStartResult::Admitted(r) => Started {
                    receipt: Rc::new(NativeTake(r)),
                    no_op: false,
                },
                RecordingBusStartResult::NoOp(r) => Started {
                    receipt: Rc::new(NativeTake(r)),
                    no_op: true,
                },
            })
            .map_err(Error::Recording)
    }
    fn stop(&self, target: Stop) -> Result<Vec<Rc<dyn TakePoll>>, Error> {
        let target = match target {
            Stop::All => RecordingStop::All,
            Stop::Track(TrackKey::Native(id)) => RecordingStop::Track(id),
            Stop::Take(TakeKey::Native(id)) => RecordingStop::Take(id),
            #[cfg(test)]
            _ => return Err(Error::Recording(RecordingError::InvalidInput)),
        };
        self.0
            .stop_recording(target)
            .map(|r| {
                r.receipts()
                    .iter()
                    .cloned()
                    .map(|r| Rc::new(NativeTake(r)) as Rc<dyn TakePoll>)
                    .collect()
            })
            .map_err(Error::Recording)
    }
}
