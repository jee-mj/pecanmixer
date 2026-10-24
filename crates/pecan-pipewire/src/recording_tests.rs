//! Synthetic registry, injected writers and temporary WAV roots only.
#[path = "recording_arming_tests.rs"]
mod arming;
#[path = "recording_multitake_tests.rs"]
mod multiple;
use super::*;
use crate::{
    bus_config::{self, ConfigTarget},
    test_support::{assert_wav, TestDir},
};
use pecan_core::{
    domain::buses::{BusConfig, BusState},
    graph::{Knoop, Opties, Poort, Soort},
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

struct Owner {
    completion: Shutdown,
    release: Option<mpsc::Sender<()>>,
}
impl Owner {
    fn new() -> Self {
        let completion = Shutdown::new();
        let (release, gate) = mpsc::channel();
        completion.worker(std::thread::spawn(move || {
            let _ = gate.recv_timeout(Duration::from_secs(10));
        }));
        Self {
            completion,
            release: Some(release),
        }
    }
    fn finish(&mut self) -> crate::shutdown::ShutdownReport {
        self.completion.stop_tracked();
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        self.completion
            .wait_timeout(Duration::from_secs(2))
            .unwrap()
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
struct Target {
    gains: Vec<f32>,
    options: Option<Opties>,
}
impl ConfigTarget for Target {
    fn readback(&self) -> Option<bus_config::BusReadback> {
        Some(bus_config::BusReadback {
            node: Some(900),
            channels: self.gains.len(),
            gains: self.gains.clone(),
        })
    }
    fn recording(&self) -> bool {
        false
    }
    fn routed(&self) -> bool {
        false
    }
    fn resize(&mut self, requested: usize) -> bus_config::ChannelResizeResult {
        bus_config::ChannelResizeResult {
            before: self.gains.len(),
            requested,
            actual: self.gains.len(),
            failure: None,
        }
    }
    fn gains(&mut self, gains: &[f32]) {
        self.gains = gains.to_vec();
    }
    fn options(&mut self, options: Opties) {
        self.options = Some(options);
    }
    fn options_match(&self, options: &Opties) -> bool {
        self.options.as_ref() == Some(options)
    }
}
struct Fixture {
    commands: Commands,
    owner: Owner,
    capture: RefCell<Vec<Capture>>,
    root: TestDir,
    target: Target,
}
impl Fixture {
    fn new(split: bool) -> Self {
        Self::new_layout(if split { vec![1, 1] } else { vec![2] })
    }
    fn new_layout(widths: Vec<usize>) -> Self {
        let channels: usize = widths.iter().sum();
        let commands = Commands::default();
        let node: Knoop = serde_json::from_value(
            serde_json::json!({"id":900,"serial":9900,"naam":opname::BUS_NAAM,"volume":vec![1.0; channels]}),
        )
        .unwrap();
        let ports = (0..channels as u32)
            .map(|c| Poort {
                id: 901 + c,
                knoop: 900,
                volgnummer: Some(c),
                naam: format!("rec_{}", c + 1),
                alias: None,
                ingang: true,
                soort: Soort::Audio,
                kanaal: Some(format!("AUX{c}")),
                monitor: false,
                fysiek: false,
                groep: None,
            })
            .collect();
        for id in 900..=900 + channels as u32 {
            commands.object_added(id);
        }
        commands.publish(Momentopname {
            verbonden: true,
            rate: Some(48000),
            knopen: vec![node],
            poorten: ports,
            ..Default::default()
        });
        let mut target = Target {
            gains: vec![1.; channels],
            options: None,
        };
        commands.refresh_config(target.readback(), false, false);
        let mut buses = BusConfig::default();
        buses.midi = false;
        buses.widths = widths;
        buses.states.clear();
        for def in buses.definitions().unwrap() {
            buses.states.insert(
                def.key,
                BusState {
                    armed: true,
                    ..Default::default()
                },
            );
        }
        commands
            .enqueue_config(bus_config::IdleConfigIntent {
                ticket: 1,
                expected: commands.config_snapshot().unwrap().revision,
                configuration: bus_config::IdleBusConfig {
                    buses,
                    clock: None,
                    start_stop: false,
                },
            })
            .unwrap();
        commands.drain_with_config(
            |_| true,
            |_| false,
            |_| LocalExecution::Unreported,
            |work| commands.apply_config(work, &mut target),
        );
        Self {
            commands,
            owner: Owner::new(),
            capture: RefCell::new(Vec::new()),
            root: TestDir::new("native-recording"),
            target,
        }
    }
    fn intent(&self) -> RecordingStart {
        RecordingStart {
            ticket: 7,
            origin: self.commands.current(),
            expected_config: self.commands.config_snapshot().unwrap().revision,
            root: self.root.0.clone(),
        }
    }
    fn drain(&self) {
        self.drain_with(opname::tracked_writer);
    }
    fn drain_with(
        &self,
        factory: impl FnOnce(
            &std::path::Path,
            Vec<Spoor>,
            u32,
            &Shutdown,
        ) -> std::io::Result<TrackedWriter>,
    ) {
        let mut factory = Some(factory);
        self.commands.drain_with_recording(
            |_| true,
            |_| false,
            |_| LocalExecution::Unreported,
            |work| {
                self.commands
                    .reject_config(work, bus_config::ConfigError::Unavailable)
            },
            |work| {
                self.commands.start_recording_with(
                    work,
                    &self.owner.completion,
                    false,
                    factory.take().unwrap(),
                )
            },
        );
    }
    fn pump(&self) {
        self.commands.pump_recording(&self.capture, false);
        self.commands.refresh_config(
            self.target.readback(),
            self.commands.recording_live(),
            false,
        );
    }
    fn start(&self) -> RecordingReceipt {
        let receipt = self.commands.enqueue_recording(self.intent()).unwrap();
        self.drain();
        until(&receipt, |r| {
            self.pump();
            r.started_once
        });
        receipt
    }
    fn samples(&self, blocks: &[Vec<f32>]) {
        let capture = self.capture.borrow();
        let capture = capture.first().unwrap();
        assert!(capture.accepts(blocks[0].len(), 1, 48000));
        capture.ingest(blocks, blocks[0].len());
    }
}
fn until(
    receipt: &RecordingReceipt,
    mut condition: impl FnMut(&RecordingReport) -> bool,
) -> RecordingReport {
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(r) = receipt.try_report() {
            if condition(&r) {
                return r;
            }
        }
        assert!(
            Instant::now() < end,
            "native recording deadline: {:?}",
            receipt.try_report()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn retained(result: RecordingStopResult) -> RecordingReceipt {
    match result {
        RecordingStopResult::Retained(r) => r,
        RecordingStopResult::NoOp => panic!("missing take"),
        RecordingStopResult::RetainedMany(_) => panic!("expected one take"),
    }
}

#[test]
fn readiness_is_not_start_until_capture_attaches_and_join_proves_real_stereo_wav() {
    let mut f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    assert_eq!(
        receipt.try_report().unwrap().progress,
        RecordingProgress::Queued
    );
    assert_eq!(std::fs::read_dir(&f.root.0).unwrap().count(), 0);
    f.drain();
    let ready = until(&receipt, |r| r.writer.as_ref().is_some_and(|w| w.was_ready));
    assert_eq!(ready.progress, RecordingProgress::Opening);
    assert!(!ready.started_once);
    f.pump();
    let started = receipt.try_report().unwrap();
    assert_eq!(started.progress, RecordingProgress::StartedLocally);
    let immutable = started.tracks.clone();
    f.samples(&[vec![0.25, 0.5], vec![-0.25, -0.5]]);
    let id = started.writer.unwrap().take;
    let stopped = retained(f.commands.stop_recording(RecordingStop::Take(id)).unwrap());
    let report = stopped.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(report.is_success(), "{report:?}");
    assert!(Arc::ptr_eq(&immutable, &report.tracks));
    let directory = report.writer.unwrap().directory.unwrap();
    assert_wav(
        &directory.join("Rec 1-2.wav"),
        2,
        48000,
        &[0.25, -0.25, 0.5, -0.5],
    );
    assert_eq!(f.owner.finish().writers_finalized, 1);
}

#[test]
fn stable_track_stop_finalizes_one_file_while_other_track_continues_then_joins() {
    let f = Fixture::new(true);
    let receipt = f.start();
    let before = receipt.try_report().unwrap();
    f.samples(&[vec![0.25], vec![0.5]]);
    let first = before.tracks[0].id;
    retained(
        f.commands
            .stop_recording(RecordingStop::Track(first))
            .unwrap(),
    );
    retained(
        f.commands
            .stop_recording(RecordingStop::Track(first))
            .unwrap(),
    );
    let closed = until(&receipt, |r| {
        f.pump();
        r.writer
            .as_ref()
            .is_some_and(|w| matches!(w.tracks[0].phase, opname::TrackPhase::Finalized(Ok(()))))
    });
    assert_eq!(closed.progress, RecordingProgress::StartedLocally);
    assert!(!closed.is_complete());
    f.samples(&[vec![9.], vec![0.75]]);
    retained(
        f.commands
            .stop_recording(RecordingStop::Track(before.tracks[1].id))
            .unwrap(),
    );
    let joined = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(joined.is_success(), "{joined:?}");
    let path = joined.writer.unwrap().directory.unwrap();
    assert_wav(&path.join("Rec 1.wav"), 1, 48000, &[0.25]);
    assert_wav(&path.join("Rec 2.wav"), 1, 48000, &[0.5, 0.75]);
}

#[test]
fn partial_open_failure_keeps_finalized_tracks_and_never_attaches_capture() {
    let f = Fixture::new(true);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, |path, spec| {
            if path.file_name().unwrap() == "Rec 2.wav" {
                return Err(std::io::Error::other("synthetic second-track open failure"));
            }
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        })
    });
    let failed = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(failed.progress, RecordingProgress::Failed);
    assert!(!failed.started_once);
    let writer = failed.writer.unwrap();
    assert_eq!(writer.phase, opname::WriterPhase::Joined);
    assert!(matches!(
        writer.tracks[0].phase,
        opname::TrackPhase::Finalized(Ok(()))
    ));
    assert!(matches!(
        writer.tracks[1].phase,
        opname::TrackPhase::NotOpened(_)
    ));
    f.pump();
    assert!(f.capture.borrow().is_empty());
    assert_wav(&writer.directory.unwrap().join("Rec 1.wav"), 1, 48000, &[]);
}

#[test]
fn queued_stop_all_reclaims_capacity_and_never_opens_a_directory() {
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    let stopped = retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    assert!(f.commands.0.lock().unwrap().pending.is_empty());
    f.drain();
    let report = stopped.wait_timeout(Duration::ZERO).unwrap();
    assert_eq!(report.failure, Some(RecordingError::CancelledBeforeStart));
    assert!(report.writer.is_none());
    assert_eq!(receipt.try_report().unwrap(), report);
    assert_eq!(std::fs::read_dir(&f.root.0).unwrap().count(), 0);
}

#[test]
fn opening_cancel_and_timeout_retain_the_same_writer_until_its_real_join() {
    let f = Fixture::new(true);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut gate = Some(gate);
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            if let Some(gate) = gate.take() {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(2)).unwrap();
            }
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        })
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let opening = receipt.try_report().unwrap();
    assert_eq!(opening.progress, RecordingProgress::Opening);
    assert!(!opening.started_once);
    let stopped = retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    assert!(Arc::ptr_eq(&receipt.0, &stopped.0));
    assert!(stopped.wait_timeout(Duration::from_millis(1)).is_err());
    release.send(()).unwrap();
    let report = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(report.progress, RecordingProgress::Failed);
    assert_eq!(report.failure, Some(RecordingError::CancelledBeforeStart));
    let writer = report.writer.unwrap();
    assert_eq!(writer.phase, opname::WriterPhase::Joined);
    assert_wav(&writer.directory.unwrap().join("Rec 1.wav"), 1, 48000, &[]);
}

#[test]
fn rejected_writer_admission_is_a_failed_start_without_a_finalization_claim() {
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    f.drain_with(|_, _, _, _| Err(std::io::Error::other("synthetic writer admission failure")));
    let report = receipt.wait_timeout(Duration::ZERO).unwrap();
    assert_eq!(report.progress, RecordingProgress::Failed);
    assert!(!report.started_once);
    assert!(report.writer.is_none());
    assert!(
        matches!(report.failure,Some(RecordingError::Writer(ref e))if e.contains("synthetic writer admission failure"))
    );
    assert_eq!(std::fs::read_dir(&f.root.0).unwrap().count(), 0);
}

#[test]
fn copied_origins_stale_configs_missing_ports_and_unarmed_starts_are_rejected_before_io() {
    let mut f = Fixture::new(false);
    let mut copied = f.intent();
    copied.origin.value = Arc::new((*copied.origin.value).clone());
    assert_eq!(
        f.commands.enqueue_recording(copied).unwrap_err(),
        RecordingError::OriginUnavailable
    );
    let mut stale = f.intent();
    stale.expected_config.revision += 1;
    assert_eq!(
        f.commands.enqueue_recording(stale).unwrap_err(),
        RecordingError::StaleConfig
    );
    let old = f.intent();
    let mut missing = (*f.commands.current().value).clone();
    missing.poorten.pop();
    f.commands.publish(missing);
    assert_eq!(
        f.commands.enqueue_recording(f.intent()).unwrap_err(),
        RecordingError::InvalidOrigin
    );
    f.commands.publish((*old.origin.value).clone());
    let mut buses = BusConfig::default();
    buses.midi = false;
    buses.states.get_mut("rec-1-2").unwrap().armed = false;
    f.commands
        .enqueue_config(bus_config::IdleConfigIntent {
            ticket: 9,
            expected: f.commands.config_snapshot().unwrap().revision,
            configuration: bus_config::IdleBusConfig {
                buses,
                clock: None,
                start_stop: false,
            },
        })
        .unwrap();
    f.commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |work| f.commands.apply_config(work, &mut f.target),
    );
    assert_eq!(
        f.commands.enqueue_recording(f.intent()).unwrap_err(),
        RecordingError::NoArmedTracks
    );
    assert_eq!(std::fs::read_dir(&f.root.0).unwrap().count(), 0);
}

#[test]
fn same_dto_registry_replacement_after_admission_never_opens_the_writer() {
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    f.commands.object_removed(901);
    f.commands.object_added(901);
    f.drain_with(|_, _, _, _| panic!("stale start reached writer factory"));
    let report = receipt.wait_timeout(Duration::ZERO).unwrap();
    assert_eq!(report.failure, Some(RecordingError::InvalidOrigin));
    assert!(report.writer.is_none());
}

#[test]
fn replacement_while_opening_finalizes_files_without_attaching_a_new_capture() {
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(2)).unwrap();
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        })
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    f.commands.object_removed(901);
    f.commands.object_added(901);
    release.send(()).unwrap();
    until(&receipt, |r| {
        f.pump();
        r.is_complete()
    });
    let report = receipt.try_report().unwrap();
    assert_eq!(report.failure, Some(RecordingError::InvalidOrigin));
    assert!(!report.started_once);
    assert!(f.capture.borrow().is_empty());
    assert_wav(
        &report
            .writer
            .unwrap()
            .directory
            .unwrap()
            .join("Rec 1-2.wav"),
        2,
        48000,
        &[],
    );
}

#[test]
fn ordinary_graph_revisions_and_origin_eviction_do_not_rebind_or_cancel_a_valid_take() {
    let f = Fixture::new(false);
    let origin = f.commands.current();
    let weak = Arc::downgrade(&origin.value);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    drop(origin);
    for _ in 0..pecan_core::native_command::NATIVE_SOURCE_HISTORY + 1 {
        let graph = (*f.commands.current().value).clone();
        f.commands.publish(graph);
    }
    assert!(
        weak.upgrade().is_none(),
        "a take must not retain evicted origin graphs"
    );
    f.drain();
    until(&receipt, |r| {
        f.pump();
        r.started_once
    });
    retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    assert!(receipt
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
}

#[test]
fn disconnect_stops_ingress_and_keeps_stable_stops_and_finalization_usable() {
    let f = Fixture::new(false);
    let receipt = f.start();
    let started = receipt.try_report().unwrap();
    let id = started.writer.unwrap().take;
    let track = started.tracks[0].id;
    f.samples(&[vec![0.25], vec![-0.25]]);
    let late = f.capture.borrow().first().unwrap().clone();
    f.commands.publish(Momentopname::default());
    retained(f.commands.stop_recording(RecordingStop::Take(id)).unwrap());
    late.ingest(&[vec![9.], vec![9.]], 1);
    let report = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(report.failure, Some(RecordingError::Disconnected));
    assert_eq!(report.progress, RecordingProgress::Failed);
    let writer = report.writer.unwrap();
    assert!(writer.is_success());
    retained(
        f.commands
            .stop_recording(RecordingStop::Track(track))
            .unwrap(),
    );
    assert_wav(
        &writer.directory.unwrap().join("Rec 1-2.wav"),
        2,
        48000,
        &[0.25, -0.25],
    );
    assert_eq!(
        f.commands.enqueue_recording(f.intent()).unwrap_err(),
        RecordingError::Disconnected
    );
}

#[test]
fn duplicate_and_old_take_stops_cannot_stop_a_new_or_foreign_take() {
    let f = Fixture::new(false);
    let old = f.start();
    let old_report = old.try_report().unwrap();
    let old_id = old_report.writer.unwrap().take;
    retained(
        f.commands
            .stop_recording(RecordingStop::Take(old_id))
            .unwrap(),
    );
    assert!(old
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
    f.pump();
    let next = f.start();
    let next_id = next.try_report().unwrap().writer.unwrap().take;
    assert_ne!(old_id, next_id);
    let duplicate = retained(
        f.commands
            .stop_recording(RecordingStop::Take(old_id))
            .unwrap(),
    );
    assert!(Arc::ptr_eq(&old.0, &duplicate.0));
    retained(
        f.commands
            .stop_recording(RecordingStop::Track(old_report.tracks[0].id))
            .unwrap(),
    );
    assert!(!next.try_report().unwrap().stop_requested);
    let foreign = Fixture::new(false);
    let other = foreign.start();
    let other_id = other.try_report().unwrap().writer.unwrap().take;
    assert!(matches!(
        f.commands.stop_recording(RecordingStop::Take(other_id)),
        Err(RecordingError::UnknownTake)
    ));
    retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    assert!(next
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
    retained(foreign.commands.stop_recording(RecordingStop::All).unwrap());
    assert!(other
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
}

#[test]
fn command_saturation_does_not_consume_stop_reservation_or_hide_queued_cancellation() {
    let f = Fixture::new(false);
    for _ in 0..NORMAL_CAPACITY {
        f.commands.legacy(Opdracht::Start).unwrap();
    }
    assert_eq!(
        f.commands.enqueue_recording(f.intent()).unwrap_err(),
        RecordingError::QueueFull
    );
    f.commands.legacy(Opdracht::Stop).unwrap();
    assert_eq!(f.commands.0.lock().unwrap().pending.len(), CAPACITY);
    f.commands.stop();
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    for _ in 1..NORMAL_CAPACITY {
        f.commands.legacy(Opdracht::Start).unwrap();
    }
    retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    assert_eq!(
        f.commands.0.lock().unwrap().pending.len(),
        NORMAL_CAPACITY - 1
    );
    f.commands.legacy(Opdracht::Stop).unwrap();
    assert_eq!(
        receipt.try_report().unwrap().failure,
        Some(RecordingError::CancelledBeforeStart)
    );
}

#[test]
fn shutdown_retains_native_cleanup_success_and_never_turns_it_into_disconnect_failure() {
    let mut f = Fixture::new(false);
    let receipt = f.start();
    f.samples(&[vec![0.5], vec![-0.5]]);
    f.owner.completion.stop_tracked();
    f.commands.stop();
    f.commands.publish(Momentopname::default());
    let native = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(native.is_success(), "{native:?}");
    let shutdown = f.owner.finish();
    assert!(shutdown.is_success());
    assert_eq!(shutdown.writers_finalized, 1);
    assert_wav(
        &native
            .writer
            .unwrap()
            .directory
            .unwrap()
            .join("Rec 1-2.wav"),
        2,
        48000,
        &[0.5, -0.5],
    );
}

struct Gate {
    armed: std::sync::atomic::AtomicBool,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct GatedFile {
    file: std::fs::File,
    gate: Arc<Gate>,
}
impl std::io::Write for GatedFile {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.gate.armed.swap(false, Ordering::AcqRel) {
            self.gate.entered.send(()).unwrap();
            self.gate
                .release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .map_err(std::io::Error::other)?;
        }
        std::io::Write::write(&mut self.file, data)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.file)
    }
}
impl std::io::Seek for GatedFile {
    fn seek(&mut self, p: std::io::SeekFrom) -> std::io::Result<u64> {
        std::io::Seek::seek(&mut self.file, p)
    }
}

#[test]
fn saturated_sample_ingress_is_an_explicit_failure_and_stop_still_drains_all_accepted_bytes() {
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Gate {
        armed: std::sync::atomic::AtomicBool::new(false),
        entered,
        release: Mutex::new(gate),
    });
    let thread_gate = gate.clone();
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            hound::WavWriter::new(
                GatedFile {
                    file: std::fs::File::create(path)?,
                    gate: thread_gate.clone(),
                },
                spec,
            )
            .map_err(std::io::Error::other)
        })
    });
    until(&receipt, |r| {
        f.pump();
        r.started_once
    });
    gate.armed.store(true, Ordering::Release);
    f.samples(&[vec![0.25], vec![-0.25]]);
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    for _ in 0..opname::WRITER_AUDIO_CAPACITY {
        f.samples(&[vec![0.25], vec![-0.25]]);
    }
    f.samples(&[vec![9.], vec![9.]]);
    let report = receipt.try_report().unwrap();
    assert_eq!(
        report.failure,
        Some(RecordingError::Ingress(opname::WriterSendError::Full))
    );
    retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    assert!(receipt.wait_timeout(Duration::from_millis(1)).is_err());
    release.send(()).unwrap();
    let final_report = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(final_report.progress, RecordingProgress::Failed);
    let writer = final_report.writer.unwrap();
    assert!(writer.is_success());
    assert_eq!(
        writer.tracks[0].samples_accepted,
        2 * (opname::WRITER_AUDIO_CAPACITY as u64 + 1)
    );
    assert_eq!(
        writer.tracks[0].samples_accepted,
        writer.tracks[0].samples_written
    );
    let samples = std::iter::repeat_n([0.25, -0.25], opname::WRITER_AUDIO_CAPACITY + 1)
        .flatten()
        .collect::<Vec<_>>();
    assert_wav(
        &writer.directory.unwrap().join("Rec 1-2.wav"),
        2,
        48000,
        &samples,
    );
}

struct ExitGate {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl Drop for ExitGate {
    fn drop(&mut self) {
        let _ = self.entered.send(());
        let _ = self.release.recv_timeout(Duration::from_secs(2));
    }
}
thread_local! { static EXIT: RefCell<Option<ExitGate>>=const{RefCell::new(None)}; }

#[test]
fn native_polling_and_timed_wait_never_join_a_finished_writer_with_slow_teardown() {
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut exit = Some(ExitGate {
        entered,
        release: gate,
    });
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            EXIT.with(|slot| *slot.borrow_mut() = exit.take());
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        })
    });
    until(&receipt, |r| {
        f.pump();
        r.started_once
    });
    f.samples(&[vec![0.5], vec![-0.5]]);
    retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let start = Instant::now();
    let report = receipt.try_report().unwrap();
    assert_eq!(report.progress, RecordingProgress::Finalizing);
    assert!(!report.is_complete());
    assert!(receipt.wait_timeout(Duration::from_millis(1)).is_err());
    let elapsed = start.elapsed();
    let writer = report.writer.unwrap();
    assert_eq!(writer.phase, opname::WriterPhase::Finished);
    assert_wav(
        &writer.directory.unwrap().join("Rec 1-2.wav"),
        2,
        48000,
        &[0.5, -0.5],
    );
    release.send(()).unwrap();
    assert!(elapsed < Duration::from_millis(100));
    assert!(receipt
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
}

#[test]
fn format_quantum_and_missing_capture_data_fail_without_successful_recording_claims() {
    for failure in 0..3 {
        let f = Fixture::new(false);
        let receipt = f.start();
        let capture = f.capture.borrow().first().unwrap().clone();
        let expected = match failure {
            0 => {
                assert!(!capture.accepts(1, 1, 44100));
                RecordingError::FormatChanged
            }
            1 => {
                assert!(!capture.accepts(opname::MAX_WRITER_SAMPLES, 1, 48000));
                RecordingError::QuantumTooLarge
            }
            _ => {
                capture.ingest(&[vec![1.]], 1);
                RecordingError::CaptureUnavailable
            }
        };
        let report = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(report.progress, RecordingProgress::Failed);
        assert_eq!(report.failure, Some(expected));
        assert!(report.writer.unwrap().is_success());
    }
}

#[test]
fn unconfirmed_worker_start_never_becomes_a_false_finalization_result() {
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.drain_with(|_, _, _, _| panic!("synthetic native start panic"))
    }));
    assert!(result.is_err());
    let report = receipt.try_report().unwrap();
    assert_eq!(report.progress, RecordingProgress::Unconfirmed);
    assert!(!report.is_complete());
    assert!(receipt.wait_timeout(Duration::from_millis(1)).is_err());
}

struct Locked<'a> {
    target: &'a mut Target,
    commands: &'a Commands,
}
impl ConfigTarget for Locked<'_> {
    fn readback(&self) -> Option<bus_config::BusReadback> {
        self.target.readback()
    }
    fn recording(&self) -> bool {
        self.commands.recording_live()
    }
    fn config_locked(&self) -> bool {
        self.commands.recording_owned()
    }
    fn routed(&self) -> bool {
        false
    }
    fn resize(&mut self, n: usize) -> bus_config::ChannelResizeResult {
        self.target.resize(n)
    }
    fn gains(&mut self, g: &[f32]) {
        self.target.gains(g)
    }
    fn options(&mut self, o: Opties) {
        self.target.options(o)
    }
    fn options_match(&self, o: &Opties) -> bool {
        self.target.options_match(o)
    }
}
fn muted_config(f: &Fixture) -> bus_config::IdleConfigIntent {
    let snapshot = f.commands.config_snapshot().unwrap();
    let mut config = snapshot.configuration.unwrap();
    config.buses.states.get_mut("rec-1-2").unwrap().mute = true;
    bus_config::IdleConfigIntent {
        ticket: 10,
        expected: snapshot.revision,
        configuration: config,
    }
}

#[test]
fn preceding_config_wins_fifo_but_config_behind_opening_is_rejected_and_metadata_survives() {
    for preceding in [false, true] {
        let mut f = Fixture::new(false);
        let config = muted_config(&f);
        let (cfg, receipt) = if preceding {
            let cfg = f.commands.enqueue_config(config).unwrap();
            let take = f.commands.enqueue_recording(f.intent()).unwrap();
            (cfg, take)
        } else {
            let take = f.commands.enqueue_recording(f.intent()).unwrap();
            let cfg = f.commands.enqueue_config(config).unwrap();
            (cfg, take)
        };
        f.commands.drain_with_recording(
            |_| true,
            |_| false,
            |_| LocalExecution::Unreported,
            |work| {
                f.commands.apply_config(
                    work,
                    &mut Locked {
                        target: &mut f.target,
                        commands: &f.commands,
                    },
                )
            },
            |work| {
                f.commands.start_recording_with(
                    work,
                    &f.owner.completion,
                    false,
                    opname::tracked_writer,
                )
            },
        );
        let bus_config::ConfigProgress::Completed(config) = cfg.try_report().unwrap().progress
        else {
            panic!("configuration incomplete")
        };
        if preceding {
            assert_eq!(config.outcome, bus_config::LocalConfigOutcome::Applied);
            assert_eq!(f.target.gains, [0., 0.]);
            assert_eq!(
                receipt.try_report().unwrap().failure,
                Some(RecordingError::StaleConfig)
            );
            assert!(receipt.try_report().unwrap().writer.is_none());
        } else {
            assert_eq!(
                config.outcome,
                bus_config::LocalConfigOutcome::Rejected(bus_config::ConfigError::RecordingActive)
            );
            assert_eq!(f.target.gains, [1., 1.]);
            until(&receipt, |r| {
                f.pump();
                r.started_once
            });
            let before = f.commands.config_snapshot().unwrap();
            assert!(before.recording);
            assert!(before.configuration.is_some());
            assert!(before.transport.is_some());
            retained(f.commands.stop_recording(RecordingStop::All).unwrap());
            assert!(receipt
                .wait_timeout(Duration::from_secs(2))
                .unwrap()
                .is_success());
            f.pump();
            let after = f.commands.config_snapshot().unwrap();
            assert!(!after.recording);
            assert_eq!(before.configuration, after.configuration);
            assert_eq!(before.transport, after.transport);
        }
    }
}

#[test]
fn earlier_legacy_stop_does_not_cancel_a_later_start_and_opening_blocks_new_mutation() {
    let f = Fixture::new(false);
    f.commands.legacy(Opdracht::Stop).unwrap();
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    assert!(!f.commands.legacy_recording_blocked(&Opdracht::Start));
    f.commands.drain_with_recording(
        |_| true,
        |_| false,
        |op| {
            assert!(matches!(op, Opdracht::Stop));
            f.commands.legacy_recording_stop(None);
            LocalExecution::Unreported
        },
        |work| {
            f.commands
                .reject_config(work, bus_config::ConfigError::Unavailable)
        },
        |work| {
            f.commands.start_recording_with(
                work,
                &f.owner.completion,
                false,
                opname::tracked_writer,
            )
        },
    );
    assert!(f.commands.legacy_recording_blocked(&Opdracht::Start));
    assert!(f.commands.legacy_recording_blocked(&Opdracht::ZetBus {
        kanalen: 1,
        gain: vec![1.]
    }));
    assert!(!f.commands.legacy_recording_blocked(&Opdracht::Stop));
    until(&receipt, |r| {
        f.pump();
        r.started_once
    });
    f.commands.legacy_recording_stop(None);
    assert!(receipt
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
}

#[test]
fn earlier_legacy_start_retains_precedence_and_native_start_does_not_open_another_writer() {
    let f = Fixture::new(false);
    let legacy = std::cell::Cell::new(false);
    f.commands.legacy(Opdracht::Start).unwrap();
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    f.commands.drain_with_recording(
        |_| true,
        |_| false,
        |op| {
            assert!(!f.commands.legacy_recording_blocked(&op));
            legacy.set(true);
            LocalExecution::Unreported
        },
        |work| {
            f.commands
                .reject_config(work, bus_config::ConfigError::Unavailable)
        },
        |work| {
            f.commands.start_recording_with(
                work,
                &f.owner.completion,
                legacy.get(),
                |_, _, _, _| panic!("native writer must not overlap the earlier legacy take"),
            )
        },
    );
    assert_eq!(
        receipt.try_report().unwrap().failure,
        Some(RecordingError::AlreadyRecording)
    );
    assert!(receipt.try_report().unwrap().writer.is_none());
}

#[test]
fn recent_stop_retention_is_bounded_and_evicted_ids_never_rebind() {
    let f = Fixture::new(false);
    let mut oldest = None;
    for _ in 0..RECENT_TAKES + 2 {
        let receipt = f.start();
        let id = receipt.try_report().unwrap().writer.unwrap().take;
        oldest.get_or_insert(id);
        retained(f.commands.stop_recording(RecordingStop::Take(id)).unwrap());
        assert!(receipt
            .wait_timeout(Duration::from_secs(2))
            .unwrap()
            .is_success());
        f.pump();
        assert!(f.commands.0.lock().unwrap().recording.recent.len() <= RECENT_TAKES);
    }
    assert!(matches!(
        f.commands
            .stop_recording(RecordingStop::Take(oldest.unwrap())),
        Err(RecordingError::UnknownTake)
    ));
}

#[test]
fn a_track_stop_during_opening_is_retained_without_cancelling_its_sibling() {
    let f = Fixture::new(true);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut gate = Some(gate);
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            if let Some(gate) = gate.take() {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(2)).unwrap();
            }
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        })
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let opening = receipt.try_report().unwrap();
    retained(
        f.commands
            .stop_recording(RecordingStop::Track(opening.tracks[0].id))
            .unwrap(),
    );
    assert_eq!(receipt.try_report().unwrap().track_stops, 1);
    assert!(!receipt.try_report().unwrap().stop_requested);
    release.send(()).unwrap();
    until(&receipt, |r| {
        f.pump();
        r.started_once
            && r.writer
                .as_ref()
                .is_some_and(|w| matches!(w.tracks[0].phase, opname::TrackPhase::Finalized(Ok(()))))
    });
    f.samples(&[vec![9.], vec![0.5]]);
    retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    let done = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(done.is_success(), "{done:?}");
    let dir = done.writer.unwrap().directory.unwrap();
    assert_wav(&dir.join("Rec 1.wav"), 1, 48000, &[]);
    assert_wav(&dir.join("Rec 2.wav"), 1, 48000, &[0.5]);
}
