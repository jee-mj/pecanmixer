//! Fixed monotonic observations and injected receipts only. No recorder or I/O.
use super::*;
use crate::application::{folders::Folders, transport::TransportControls};

struct Window(gtk::Window);
impl Window {
    fn show(widget: &impl IsA<gtk::Widget>) -> Self {
        let window = Self(gtk::Window::builder().child(widget).build());
        window.0.present();
        let context = gtk::glib::MainContext::default();
        let end = Instant::now() + Duration::from_millis(40);
        while Instant::now() < end {
            while context.pending() && Instant::now() < end {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(widget.is_mapped());
        window
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

fn active() -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver) {
    let (owner, fake, driver) = setup();
    owner
        .set_recording_root(Some("/tmp/pecan-elapsed-synthetic-no-io".into()))
        .unwrap();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 41);
    fake.config.borrow_mut().recording = true;
    owner.poll();
    (owner, fake, driver)
}
fn timing(fake: &Fake, index: usize, at: Instant, duration: Duration) {
    let take = fake.takes.borrow()[index].clone();
    let mut take = take.0.borrow_mut();
    let status = take.as_mut().unwrap();
    status.elapsed_at = Some(at);
    for track in &mut status.tracks {
        track.elapsed = Some(duration);
    }
}
fn labels(root: &impl IsA<gtk::Widget>) -> Vec<String> {
    let mut todo = vec![root.as_ref().clone()];
    let mut labels = vec![];
    while let Some(widget) = todo.pop() {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            labels.push(label.text().to_string());
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            todo.push(widget);
        }
    }
    labels
}
#[gtk::test]
fn elapsed_adoption_compares_measurement_ages_and_preserves_cached_observation() {
    let (owner, fake, _) = active();
    let origin = owner.origin().unwrap();
    fake.admit(
        RecordingStart {
            ticket: 99,
            origin: origin.0.graph.clone().unwrap(),
            expected_config: fake.config.borrow().revision,
            root: "/tmp/pecan-elapsed-synthetic-no-io".into(),
        },
        None,
        None,
    )
    .unwrap();
    fake.started(1, 42);
    let wall = Instant::now();
    timing(
        &fake,
        0,
        wall - Duration::from_secs(20),
        Duration::from_secs(10),
    );
    timing(
        &fake,
        1,
        wall - Duration::from_secs(1),
        Duration::from_secs(25),
    );
    owner.poll();
    let snapshot = owner.folder_snapshot_at(Duration::from_secs(2), wall);
    assert_eq!(
        snapshot.takes[0].tracks[0].elapsed,
        Some(Duration::from_secs(30))
    );
    assert_eq!(
        snapshot.takes[1].tracks[0].elapsed,
        Some(Duration::from_secs(26))
    );
    let controls = TransportControls::new();
    controls.observe(snapshot).unwrap();
    let _window = Window::show(controls.widget());
    controls.tick(Duration::from_secs(5));
    assert!(labels(controls.widget()).iter().any(|s| s == "0:33.0"));
    // Re-delivering the same cached report adds its actual age once, not the
    // duration of every intervening delivery or the sibling's measurement age.
    owner.poll();
    let later = owner.folder_snapshot_at(Duration::from_secs(7), wall + Duration::from_secs(5));
    controls
        .update_elapsed(later.activity, &later.takes, later.observed_at)
        .unwrap();
    assert!(labels(controls.widget()).iter().any(|s| s == "0:35.0"));
    let retained = owner.recording_status();
    assert_eq!(retained[0].elapsed_at, Some(wall - Duration::from_secs(20)));
    assert_eq!(retained[0].tracks[0].elapsed, Some(Duration::from_secs(10)));
}

#[gtk::test]
fn elapsed_adoption_rejects_missing_future_failed_busy_and_foreign_authority() {
    for mode in 0..12 {
        let (owner, fake, driver) = active();
        let wall = Instant::now();
        timing(
            &fake,
            0,
            wall - Duration::from_secs(2),
            Duration::from_secs(4),
        );
        match mode {
            0 => {
                fake.takes.borrow()[0]
                    .0
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .elapsed_at = None
            }
            1 => {
                fake.takes.borrow()[0]
                    .0
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .elapsed_at = Some(wall + Duration::from_secs(1))
            }
            2 => {
                fake.takes.borrow()[0]
                    .0
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .failure = Some(RecordingError::CaptureUnavailable)
            }
            3 => fake.busy.set(true),
            4 => fake.config.borrow_mut().revision.owner += 1,
            5 => {
                let mut graph = graph();
                graph.verbonden = false;
                driver.publish(graph.clone(), generations(&graph));
                owner.session.poll();
            }
            6 => {
                owner
                    .data
                    .borrow_mut()
                    .stopped
                    .insert(TrackKey::Synthetic(41));
            }
            7 => owner.data.borrow_mut().all_stopped = true,
            8 => {
                fake.takes.borrow()[0]
                    .0
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .progress = RecordingProgress::Unconfirmed
            }
            9 => {
                fake.takes.borrow()[0]
                    .0
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .tracks[0]
                    .elapsed = Some(Duration::MAX)
            }
            10 => fake.config.borrow_mut().revision.epoch += 1,
            _ => {
                owner
                    .data
                    .borrow_mut()
                    .stopped_takes
                    .insert(TakeKey::Synthetic(41));
            }
        }
        owner.poll();
        let snapshot = owner.folder_snapshot_at(Duration::ZERO, wall);
        assert!(
            snapshot
                .takes
                .iter()
                .flat_map(|t| &t.tracks)
                .all(|t| t.elapsed.is_none()),
            "mode {mode}"
        );
        let controls = TransportControls::new();
        controls.observe(snapshot).unwrap();
        assert!(
            labels(controls.widget())
                .iter()
                .any(|s| s == "Recording time unavailable"),
            "mode {mode}"
        );
    }
}

#[gtk::test]
fn elapsed_adoption_partial_stop_preserves_survivor_and_missing_sibling_is_unavailable() {
    let (owner, fake, _) = active();
    let wall = Instant::now();
    timing(
        &fake,
        0,
        wall - Duration::from_secs(2),
        Duration::from_secs(4),
    );
    {
        let take = fake.takes.borrow()[0].clone();
        let mut status = take.0.borrow_mut();
        let status = status.as_mut().unwrap();
        let mut survivor = status.tracks[0].clone();
        survivor.identity = TrackKey::Synthetic(42);
        survivor.bus_key = "rec-2".into();
        status.tracks[0].phase = ui::TrackPhase::Finalizing;
        status.tracks[0].elapsed = None;
        status.tracks.push(survivor);
    }
    owner.poll();
    let snapshot = owner.folder_snapshot_at(Duration::ZERO, wall);
    assert!(snapshot.takes[0].tracks[0].elapsed.is_none());
    assert_eq!(
        snapshot.takes[0].tracks[1].elapsed,
        Some(Duration::from_secs(6))
    );
    let controls = TransportControls::new();
    controls.observe(snapshot).unwrap();
    let _window = Window::show(controls.widget());
    assert!(labels(controls.widget()).iter().any(|s| s == "0:06.0"));
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .tracks[1]
        .elapsed = None;
    owner.poll();
    let snapshot = owner.folder_snapshot_at(Duration::from_secs(1), wall + Duration::from_secs(1));
    controls
        .update_elapsed(snapshot.activity, &snapshot.takes, snapshot.observed_at)
        .unwrap();
    assert!(labels(controls.widget())
        .iter()
        .any(|s| s == "Recording time unavailable"));
}

#[gtk::test]
fn elapsed_adoption_timing_storm_preserves_edit_and_folder_bindings_without_audio() {
    let (owner, fake, _) = active();
    let wall = Instant::now();
    timing(
        &fake,
        0,
        wall - Duration::from_secs(10),
        Duration::from_secs(5),
    );
    owner.poll();
    let edit = owner.origin().unwrap();
    let folders = Folders::new(owner.clone(), None);
    let folder = button(folders.widget(), "Folder");
    for n in 0..100 {
        timing(
            &fake,
            0,
            wall - Duration::from_secs(10) + Duration::from_millis(n),
            Duration::from_secs(5) + Duration::from_millis(n),
        );
        owner.poll();
        folders.poll(Duration::from_millis(n));
        assert!(Rc::ptr_eq(&edit.0, &owner.origin().unwrap().0));
        assert_eq!(button(folders.widget(), "Folder"), folder);
    }
    assert_eq!(fake.starts.borrow().len(), 1);
    assert!(fake.stops.borrow().is_empty());
    assert!(fake.configs.borrow().is_empty());
    assert!(labels(folders.widget())
        .iter()
        .any(|s| s.starts_with("0:15.")));
}
