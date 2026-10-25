use super::*;
use crate::{
    application::{buses::Source, BusTrackKey},
    recording_buses::Track,
};
use pecan_pipewire::opname::WriterPhase;
use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

pub(super) struct Temp(pub PathBuf);
impl Temp {
    pub(super) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = PathBuf::from(format!(
            "/tmp/pecan-transport-synthetic-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn take(root: &Path, id: u64, elapsed: Option<Duration>) -> BusRecordingStatus {
    BusRecordingStatus {
        source: Source {
            ticket: id,
            epoch: 1,
            revision: 1,
            owner: 77,
            config_epoch: 1,
            config_revision: 1,
        },
        progress: RecordingProgress::StartedLocally,
        writer: Some(WriterPhase::Ready),
        writer_outcome: None,
        failure: None,
        complete: false,
        take: Some(BusTakeKey::Synthetic(id)),
        directory: Some(root.join(format!("Take {id}"))),
        tracks: vec![Track {
            identity: BusTrackKey::Synthetic(id),
            bus_key: format!("rec-{id}"),
            phase: TrackPhase::Recording,
            elapsed,
        }],
    }
}
pub(super) fn snapshot(root: &Path, at: Duration) -> Snapshot<Rc<()>> {
    Snapshot {
        origin: Rc::new(()),
        root: Some(root.into()),
        latest: Some(BusTakeKey::Synthetic(1)),
        takes: vec![take(root, 1, Some(Duration::from_secs(5)))],
        activity: Activity::Recording,
        observed_at: at,
        closing: false,
    }
}

#[test]
fn looptijd_source_format_boundaries_preserve_js_rounding_and_hour_transition() {
    for (seconds, expected) in [
        (0., "0:00.0"),
        (0.15, "0:00.1"),
        (1.25, "0:01.3"),
        (59.95, "0:60.0"),
        (60., "1:00.0"),
        (3599.96, "59:60.0"),
        (3600., "1:00:00.0"),
        (3661.25, "1:01:01.3"),
    ] {
        assert_eq!(
            elapsed_text(Duration::from_secs_f64(seconds)),
            expected,
            "{seconds}"
        );
    }
}

#[test]
fn oldest_reported_time_and_latest_folder_are_independent_and_missing_time_is_unavailable() {
    let temp = Temp::new();
    let mut m = Model::default();
    let mut s = snapshot(&temp.0, Duration::from_secs(10));
    s.takes
        .push(take(&temp.0, 2, Some(Duration::from_secs(12))));
    m.observe(s).unwrap();
    assert_eq!(
        m.elapsed(Duration::from_millis(11200)),
        Some(Duration::from_millis(13200))
    );
    let request = m.request_folder(&m.bind().unwrap()).unwrap();
    assert_eq!(
        request.paths.latest(),
        Some(temp.0.join("Take 1").as_path())
    );
    assert!(m.complete(&request.token, Completion::Cancelled));
    let mut s = snapshot(&temp.0, Duration::from_secs(20));
    s.takes[0].tracks[0].elapsed = None;
    m.observe(s).unwrap();
    assert_eq!(m.elapsed(Duration::from_secs(1000)), None);
    let mut s = snapshot(&temp.0, Duration::from_secs(21));
    s.activity = Activity::Finalizing;
    m.observe(s).unwrap();
    assert_eq!(m.elapsed(Duration::from_secs(1000)), None);
    let mut s = snapshot(&temp.0, Duration::from_secs(22));
    s.takes = vec![take(&temp.0, 2, Some(Duration::from_secs(3)))];
    s.latest = Some(BusTakeKey::Synthetic(2));
    m.observe(s).unwrap();
    assert_eq!(
        m.elapsed(Duration::from_secs(23)),
        Some(Duration::from_secs(4))
    );
    let mut s = snapshot(&temp.0, Duration::from_secs(24));
    s.activity = Activity::Idle;
    s.takes[0].complete = true;
    s.takes[0].progress = RecordingProgress::JoinedSuccess;
    s.takes[0].writer = Some(WriterPhase::Joined);
    s.takes[0].tracks[0].phase = TrackPhase::Joined(Ok(()));
    m.observe(s).unwrap();
    assert_eq!(m.elapsed(Duration::from_secs(1000)), Some(Duration::ZERO));
}

#[test]
fn folder_preparation_uses_explicit_root_canonical_fallback_and_one_literal_argument() {
    let temp = Temp::new();
    let root = temp.0.join("Recording $(synthetic) ' spaces");
    let latest = root.join("Take 1");
    std::fs::create_dir_all(&latest).unwrap();
    let paths = FolderPaths::new(root.clone(), Some(latest.clone())).unwrap();
    let plan = paths.prepare().unwrap();
    assert_eq!(plan.program(), "xdg-open");
    assert_eq!(plan.argument(), latest.canonicalize().unwrap().as_os_str());
    assert_eq!(plan.requested(), &paths);
    let outside = temp.0.join("Outside synthetic");
    std::fs::create_dir(&outside).unwrap();
    for candidate in [outside.clone(), root.join("Missing take")] {
        assert_eq!(
            FolderPaths::new(root.clone(), Some(candidate))
                .unwrap()
                .prepare()
                .unwrap()
                .target(),
            root.canonicalize().unwrap()
        );
    }
    #[cfg(unix)]
    {
        let link = root.join("Escaping take");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert_eq!(
            FolderPaths::new(root.clone(), Some(link))
                .unwrap()
                .prepare()
                .unwrap()
                .target(),
            root.canonicalize().unwrap()
        );
    }
    assert!(FolderPaths::new(PathBuf::from("relative"), None).is_err());
    let file = temp.0.join("Not a directory");
    std::fs::write(&file, b"synthetic").unwrap();
    assert!(FolderPaths::new(file, None).unwrap().prepare().is_err());
    let new_root = temp.0.join("New explicit root");
    assert!(FolderPaths::new(new_root.clone(), None)
        .unwrap()
        .prepare()
        .is_ok());
    assert!(new_root.is_dir());
}

#[test]
fn pending_correlation_stale_bindings_and_snapshot_budgets_do_not_retarget_folder_requests() {
    let temp = Temp::new();
    let mut m = Model::default();
    m.observe(snapshot(&temp.0, Duration::from_secs(1)))
        .unwrap();
    let stale = m.bind().unwrap();
    let mut timing = snapshot(&temp.0, Duration::from_millis(1500));
    timing.takes[0].tracks[0].elapsed = Some(Duration::from_millis(5500));
    m.update_elapsed(timing.activity, &timing.takes, timing.observed_at)
        .unwrap();
    assert!(Rc::ptr_eq(&stale.0, &m.bind().unwrap().0));
    assert_eq!(
        m.observe(snapshot(&temp.0, Duration::from_millis(1250))),
        Err(Error::OldSnapshot)
    );
    let request = m.request_folder(&stale).unwrap();
    let second_root = temp.0.join("Second root");
    m.observe(snapshot(&second_root, Duration::from_secs(2)))
        .unwrap();
    assert!(matches!(m.request_folder(&stale), Err(Error::StaleBinding)));
    assert!(matches!(
        m.request_folder(&m.bind().unwrap()),
        Err(Error::Pending)
    ));
    let unrelated = FolderPaths::new(second_root, None)
        .unwrap()
        .prepare()
        .unwrap();
    assert!(!m.complete(&request.token, Completion::Submitted(unrelated)));
    assert_eq!(m.pending(), Some(request.token.clone()));
    let actual = request.paths.prepare().unwrap();
    assert!(m.complete(&request.token, Completion::Submitted(actual)));
    assert!(!m.complete(
        &request.token,
        Completion::Failed("old synthetic error".into())
    ));
    let next = m.request_folder(&m.bind().unwrap()).unwrap();
    assert!(!m.complete(&request.token, Completion::Cancelled));
    assert_eq!(m.pending(), Some(next.token.clone()));
    assert!(m.complete(&next.token, Completion::Cancelled));
    assert_eq!(
        m.observe(snapshot(&temp.0, Duration::from_secs(1))),
        Err(Error::OldSnapshot)
    );
    let mut duplicate = snapshot(&temp.0, Duration::from_secs(3));
    duplicate.takes.push(duplicate.takes[0].clone());
    assert_eq!(m.observe(duplicate), Err(Error::DuplicateIdentity));
    let mut huge = snapshot(&temp.0, Duration::from_secs(3));
    huge.takes.resize(MAX_TAKES + 1, huge.takes[0].clone());
    assert_eq!(m.observe(huge), Err(Error::Budget));
}
