//! Presentation over retained recording status. No recorder, path discovery,
//! file-manager launch, receipt polling or clock source is started here.
mod folder;
mod view;
pub use folder::{FolderPaths, FolderPlan};
pub use view::TransportControls;

use super::{BusRecordingStatus, BusTakeKey};
use crate::recording_buses::{Activity, TrackPhase};
use pecan_pipewire::recording::RecordingProgress;
use std::{cell::Cell, collections::BTreeSet, path::PathBuf, rc::Rc, time::Duration};

const MAX_TAKES: usize = 80;
const MAX_TRACKS: usize = 64;
const MAX_PATH: usize = 4096;

/// Owner-authenticated projection. `latest` is designated by actual start
/// chronology, never sorted from ticket/ID values. `observed_at` uses the same
/// monotonic clock as tick(); do not restamp cached elapsed values.
pub struct Snapshot<O> {
    pub origin: O,
    pub root: Option<PathBuf>,
    pub latest: Option<BusTakeKey>,
    pub takes: Vec<BusRecordingStatus>,
    pub activity: Activity,
    pub observed_at: Duration,
    pub closing: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    OldSnapshot,
    Budget,
    DuplicateIdentity,
    InvalidPath,
    Unavailable,
    StaleBinding,
    Pending,
    Closing,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Recording transport: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct RequestToken(Rc<()>);
impl PartialEq for RequestToken {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for RequestToken {}
/// UI correlation, not a native ticket or filesystem permission. The owner
/// validates origin before preparing/launching these exact captured paths.
#[derive(Clone)]
pub struct FolderRequest<O> {
    pub origin: O,
    pub paths: FolderPaths,
    pub token: RequestToken,
}
pub enum Completion {
    /// Launch admission only; this does not prove a file-manager window opened.
    Submitted(FolderPlan),
    /// No launch was admitted. This is not cancellation of an admitted launch.
    Cancelled,
    Failed(String),
}
#[derive(Clone, Copy)]
enum Clock {
    Idle,
    Running(Duration),
    Unavailable,
}
struct Projection<O> {
    origin: O,
    root: Option<PathBuf>,
    latest: Option<PathBuf>,
    paths: Option<FolderPaths>,
    clock: Cell<Clock>,
    timing_at: Cell<Duration>,
    observed_at: Duration,
    closing: bool,
}
#[derive(Clone)]
pub struct Binding<O>(Rc<Projection<O>>);
pub struct Model<O> {
    current: Option<Rc<Projection<O>>>,
    pending: Option<FolderRequest<O>>,
    message: Option<String>,
}
impl<O> Default for Model<O> {
    fn default() -> Self {
        Self {
            current: None,
            pending: None,
            message: None,
        }
    }
}
impl<O: Clone> Model<O> {
    pub fn observe(&mut self, snapshot: Snapshot<O>) -> Result<(), Error> {
        self.adopt(Self::project(snapshot)?)
    }
    fn adopt(&mut self, projection: Rc<Projection<O>>) -> Result<(), Error> {
        if self
            .current
            .as_ref()
            .is_some_and(|p| projection.observed_at < p.observed_at.max(p.timing_at.get()))
        {
            return Err(Error::OldSnapshot);
        }
        self.current = Some(projection);
        Ok(())
    }
    fn project(snapshot: Snapshot<O>) -> Result<Rc<Projection<O>>, Error> {
        validate_takes(&snapshot.takes)?;
        let latest = snapshot.latest.and_then(|id| {
            snapshot
                .takes
                .iter()
                .find(|t| t.take == Some(id))
                .and_then(|t| t.directory.clone())
        });
        if latest.as_ref().is_some_and(|p| !folder::valid(p)) {
            return Err(Error::InvalidPath);
        }
        let paths = snapshot
            .root
            .as_ref()
            .map(|root| FolderPaths::new(root.clone(), latest.clone()))
            .transpose()
            .map_err(|_| Error::InvalidPath)?;
        let clock = clock(&snapshot.activity, &snapshot.takes);
        Ok(Rc::new(Projection {
            origin: snapshot.origin,
            root: snapshot.root,
            latest,
            paths,
            clock: Cell::new(clock),
            timing_at: Cell::new(snapshot.observed_at),
            observed_at: snapshot.observed_at,
            closing: snapshot.closing,
        }))
    }
    pub fn bind(&self) -> Result<Binding<O>, Error> {
        self.current.clone().map(Binding).ok_or(Error::Unavailable)
    }
    pub fn request_folder(&mut self, binding: &Binding<O>) -> Result<FolderRequest<O>, Error> {
        if !self
            .current
            .as_ref()
            .is_some_and(|p| Rc::ptr_eq(p, &binding.0))
        {
            return Err(Error::StaleBinding);
        }
        if self.pending.is_some() {
            return Err(Error::Pending);
        }
        if binding.0.closing {
            return Err(Error::Closing);
        }
        let request = FolderRequest {
            origin: binding.0.origin.clone(),
            paths: binding.0.paths.clone().ok_or(Error::Unavailable)?,
            token: RequestToken(Rc::new(())),
        };
        self.pending = Some(request.clone());
        self.message = Some("Folder request pending".into());
        Ok(request)
    }
    pub fn complete(&mut self, token: &RequestToken, completion: Completion) -> bool {
        let Some(request) = self.pending.as_ref().filter(|r| &r.token == token) else {
            return false;
        };
        self.message = Some(match completion {
            Completion::Submitted(plan) => {
                if plan.requested() != &request.paths {
                    return false;
                }
                format!("Folder request submitted: {}", plan.target().display())
            }
            Completion::Cancelled => "Folder request cancelled before launch".into(),
            Completion::Failed(error) => format!(
                "Folder request failed for {}: {}",
                request.paths.root().display(),
                error.chars().take(1024).collect::<String>()
            ),
        });
        self.pending = None;
        true
    }
    /// Fresh timing only: preserve captured folder bindings and pending work.
    pub fn update_elapsed(
        &self,
        activity: Activity,
        takes: &[BusRecordingStatus],
        at: Duration,
    ) -> Result<(), Error> {
        let p = self.current.as_ref().ok_or(Error::Unavailable)?;
        if at < p.observed_at.max(p.timing_at.get()) {
            return Err(Error::OldSnapshot);
        }
        validate_takes(takes)?;
        p.clock.set(clock(&activity, takes));
        p.timing_at.set(at);
        Ok(())
    }
    pub fn elapsed(&self, now: Duration) -> Option<Duration> {
        let p = self.current.as_ref()?;
        match p.clock.get() {
            Clock::Idle => Some(Duration::ZERO),
            Clock::Running(base) => base.checked_add(now.saturating_sub(p.timing_at.get())),
            Clock::Unavailable => None,
        }
    }
    pub fn pending(&self) -> Option<RequestToken> {
        self.pending.as_ref().map(|r| r.token.clone())
    }
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}
fn validate_takes(takes: &[BusRecordingStatus]) -> Result<(), Error> {
    if takes.len() > MAX_TAKES {
        return Err(Error::Budget);
    }
    let mut take_ids = BTreeSet::new();
    let mut tracks = BTreeSet::new();
    for take in takes {
        if take.tracks.len() > MAX_TRACKS {
            return Err(Error::Budget);
        }
        if take.take.is_some_and(|id| !take_ids.insert(id)) {
            return Err(Error::DuplicateIdentity);
        }
        for track in &take.tracks {
            if !tracks.insert(track.identity) {
                return Err(Error::DuplicateIdentity);
            }
        }
    }
    Ok(())
}
fn clock(activity: &Activity, takes: &[BusRecordingStatus]) -> Clock {
    let active: Vec<_> = takes.iter().filter(|t| !t.complete).collect();
    if *activity == Activity::Idle && active.is_empty() {
        return Clock::Idle;
    }
    if *activity != Activity::Recording
        || active.iter().any(|t| {
            t.progress == RecordingProgress::Unconfirmed
                || t.tracks.iter().any(|t| t.phase == TrackPhase::Unconfirmed)
        })
    {
        return Clock::Unavailable;
    }
    let mut oldest = None;
    for track in active
        .iter()
        .flat_map(|t| &t.tracks)
        .filter(|t| t.phase == TrackPhase::Recording)
    {
        let Some(elapsed) = track.elapsed else {
            return Clock::Unavailable;
        };
        oldest = Some(oldest.map_or(elapsed, |old: Duration| old.max(elapsed)));
    }
    oldest.map_or(Clock::Unavailable, Clock::Running)
}
/// The original Looptijd format, including rounded 60.0 without carrying into
/// the next minute. Idle/active authority is separate from this formatter.
pub fn elapsed_text(elapsed: Duration) -> String {
    let s = elapsed.as_secs_f64();
    let hours = (s / 3600.).floor() as u64;
    let minutes = ((s % 3600.) / 60.).floor() as u64;
    let tenths = decimal_tenths(s % 60.);
    let seconds = format!("{:02}.{}", tenths / 10, tenths % 10);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds}")
    } else {
        format!("{minutes}:{seconds}")
    }
}
fn decimal_tenths(value: f64) -> u64 {
    // JS toFixed(1) rounds the exact positive binary float, with ties up.
    // Multiplying as f64 first would change cases such as 0.15.
    if value == 0. {
        return 0;
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1_u64 << 52 };
    let shift = (if exponent == 0 { 1074 } else { 1075 - exponent }) as u32;
    if shift >= 128 {
        return 0;
    }
    let numerator = u128::from(mantissa) * 10;
    let whole = numerator >> shift;
    let remainder = numerator & ((1_u128 << shift) - 1);
    (whole + u128::from(remainder >= (1_u128 << (shift - 1)))) as u64
}

#[cfg(test)]
mod tests;
