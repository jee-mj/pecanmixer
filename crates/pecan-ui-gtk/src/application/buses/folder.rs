//! Read-only folder authority, independent of graph revisions and timing ticks.
use super::*;
use crate::application::transport;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct FolderOrigin(Rc<Context>);
struct Context {
    owner: Rc<()>,
    root: Option<PathBuf>,
    latest: Option<Rc<dyn TakePoll>>,
    signature: Option<(Source, Option<TakeKey>, Option<PathBuf>)>,
    closing: bool,
}
impl FolderOrigin {
    pub(crate) fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl BusOwner {
    pub(crate) fn folder_snapshot(&self, now: Duration) -> transport::Snapshot<FolderOrigin> {
        self.folder_snapshot_at(now, Instant::now())
    }
    pub(super) fn folder_snapshot_at(
        &self,
        now: Duration,
        sampled_at: Instant,
    ) -> transport::Snapshot<FolderOrigin> {
        let closing = self.session.admission_closed();
        let graph = self.session.graph();
        let mut d = self.data.borrow_mut();
        let latest = d.latest_started.as_ref().map(|(r, _)| r.clone());
        let status = d.latest_started.as_ref().and_then(|(_, s)| s.clone());
        let signature = status
            .as_ref()
            .map(|s| (s.source, s.take, s.directory.clone()));
        let same = d.folder_origin.as_ref().is_some_and(|old| {
            old.0.root == d.root
                && old.0.closing == closing
                && old.0.signature == signature
                && match (&old.0.latest, &latest) {
                    (None, None) => true,
                    (Some(a), Some(b)) => same_take(a, b),
                    _ => false,
                }
        });
        if !same {
            d.folder_origin = Some(FolderOrigin(Rc::new(Context {
                owner: self.authority.clone(),
                root: d.root.clone(),
                latest,
                signature,
                closing,
            })));
        }
        let origin = d.folder_origin.clone().unwrap();
        let mut takes: Vec<_> = d.takes.iter().map(|t| t.status.clone()).collect();
        let fresh = d.folder_fresh && !d.protocol_invalid;
        let active: Vec<_> = takes.iter().filter(|t| !t.complete).collect();
        let activity = if !fresh || d.unknown_owned || !d.orphans.is_empty() {
            ui::Activity::Unknown
        } else if active
            .iter()
            .any(|t| t.progress == RecordingProgress::StartedLocally)
            || d.config.as_ref().is_some_and(|c| c.recording)
        {
            ui::Activity::Recording
        } else if active.iter().any(|t| {
            matches!(
                t.progress,
                RecordingProgress::Finalizing
                    | RecordingProgress::Failed
                    | RecordingProgress::Unconfirmed
            )
        }) {
            ui::Activity::Finalizing
        } else if !active.is_empty() {
            ui::Activity::Unknown
        } else if d.config.is_some() {
            ui::Activity::Idle
        } else {
            ui::Activity::Unknown
        };
        let mut latest = status.as_ref().and_then(|s| s.take);
        if let Some(status) = status.filter(|s| {
            s.take
                .is_some_and(|id| !takes.iter().any(|t| t.take == Some(id)))
        }) {
            if takes.len() == 80 {
                if let Some(i) = takes.iter().position(|t| t.complete) {
                    takes.remove(i);
                } else {
                    latest = None;
                }
            }
            if takes.len() < 80 {
                takes.push(status);
            }
        }
        // Normalize each actual measurement to this one clock sample before
        // choosing the oldest take. Cached reports keep their original instant
        // in Data; neither delivery nor another sibling can refresh their age.
        for take in &mut takes {
            let current_source = graph
                .as_ref()
                .is_some_and(|g| g.value.verbonden && g.stamp.epoch == take.source.epoch)
                && d.config.as_ref().is_some_and(|c| {
                    c.connected
                        && c.revision.owner == take.source.owner
                        && c.revision.epoch == take.source.config_epoch
                });
            let age = take
                .elapsed_at
                .filter(|_| {
                    fresh
                        && current_source
                        && !take.complete
                        && !d.all_stopped
                        && !take.take.is_some_and(|id| d.stopped_takes.contains(&id))
                        && take.progress == RecordingProgress::StartedLocally
                        && take.failure.is_none()
                })
                .and_then(|at| sampled_at.checked_duration_since(at));
            for track in &mut take.tracks {
                track.elapsed = (track.phase == ui::TrackPhase::Recording
                    && !d.stopped.contains(&track.identity))
                .then_some(track.elapsed)
                .flatten()
                .zip(age)
                .and_then(|(elapsed, age)| elapsed.checked_add(age));
            }
        }
        transport::Snapshot {
            origin,
            root: d.root.clone(),
            latest,
            takes,
            activity,
            observed_at: now,
            closing,
        }
    }
    /// Check only a previously published folder context; never retarget it.
    pub(crate) fn authorize_folder(
        &self,
        origin: &FolderOrigin,
        paths: &transport::FolderPaths,
    ) -> Result<bool, String> {
        if self.session.admission_closed() {
            return Err("Application is closing; no new folder launch admitted.".into());
        }
        let d = self.data.borrow();
        if !Rc::ptr_eq(&origin.0.owner, &self.authority)
            || !d.folder_origin.as_ref().is_some_and(|o| o.same(origin))
            || d.root != origin.0.root
        {
            return Err("Recording folder changed before launch; start a fresh action.".into());
        }
        let latest_matches = match (&origin.0.latest, d.latest_started.as_ref()) {
            (None, None) => true,
            (Some(old), Some((current, _))) => same_take(old, current),
            _ => false,
        };
        let captured = origin
            .0
            .signature
            .as_ref()
            .map(|(source, take, path)| (*source, *take, path.as_ref()));
        let current = d
            .latest_started
            .as_ref()
            .and_then(|(_, status)| status.as_ref())
            .map(|s| (s.source, s.take, s.directory.as_ref()));
        if !latest_matches || captured != current {
            return Err(
                "Latest started folder changed before launch; start a fresh action.".into(),
            );
        }
        let directory = origin
            .0
            .signature
            .as_ref()
            .and_then(|(_, take, path)| take.and_then(|_| path.as_deref()));
        if origin.0.root.as_deref() != Some(paths.root()) || directory != paths.latest() {
            return Err("Folder paths do not match their captured authority.".into());
        }
        if d.protocol_invalid {
            return Err("Folder source authority is unavailable.".into());
        }
        Ok(d.folder_fresh)
    }
}
