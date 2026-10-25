//! Synthetic lifecycle timing. Fixed observation instants, no timing sleeps,
//! live server, device, sample-duration inference or alignment assertion.
use super::*;

pub(super) fn report_at(receipt: &RecordingReceipt, at: Instant) -> RecordingReport {
    let mut observed = None;
    until(receipt, |_| {
        observed = receipt.0.report_at(at);
        observed.is_some()
    });
    observed.unwrap()
}
fn start_bus(f: &Fixture, key: &str) -> RecordingReceipt {
    let snapshot = f.commands.recording_snapshot().unwrap();
    let RecordingBusStartResult::Admitted(receipt) = f
        .commands
        .enqueue_recording_bus(RecordingBusStart {
            start: RecordingStart {
                expected_config: snapshot.configuration.revision,
                ..f.intent()
            },
            expected_recording: snapshot.revision,
            bus: key.into(),
        })
        .unwrap()
    else {
        panic!("new synthetic take")
    };
    f.drain();
    until(&receipt, |r| {
        f.pump();
        r.started_once
    });
    receipt
}

#[test]
fn elapsed_starts_at_attachment_not_queue_opening_readiness_bytes_or_delivery() {
    let f = Fixture::new(false);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    assert!(receipt.try_report().unwrap().elapsed.is_none());
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
    assert!(receipt.try_report().unwrap().elapsed.is_none());
    release.send(()).unwrap();
    let ready = until(&receipt, |r| r.writer.as_ref().is_some_and(|w| w.was_ready));
    assert!(!ready.started_once);
    assert!(ready.elapsed.is_none());
    assert!(receipt.0.first_attachment.get().is_none());
    let before = Instant::now();
    f.pump();
    let after = Instant::now();
    let start = *receipt.0.first_attachment.get().unwrap();
    assert!(before <= start && start <= after);
    assert!(report_at(&receipt, start - Duration::from_nanos(1))
        .elapsed
        .is_none());
    let at = start + Duration::from_secs(5);
    let cached = report_at(&receipt, at);
    let elapsed = cached.elapsed.as_ref().unwrap();
    assert_eq!(elapsed.observed_at, at);
    assert_eq!(elapsed.duration, Duration::from_secs(5));
    assert_eq!(elapsed.running_tracks, vec![cached.tracks[0].id]);
    assert!(cached
        .writer
        .as_ref()
        .unwrap()
        .tracks
        .iter()
        .all(|t| t.samples_accepted == 0 && t.samples_written == 0));
    let revision = f.commands.recording_snapshot().unwrap().revision;
    for _ in 0..10_000 {
        assert_eq!(report_at(&receipt, at).elapsed, cached.elapsed);
    }
    f.pump();
    assert_eq!(f.commands.recording_snapshot().unwrap().revision, revision);
    assert_eq!(*receipt.0.first_attachment.get().unwrap(), start);
    assert_eq!(
        report_at(&receipt, at + Duration::from_secs(5))
            .elapsed
            .unwrap()
            .duration,
        Duration::from_secs(10)
    );
    assert_eq!(
        cached.elapsed.unwrap().observed_at,
        at,
        "cached reports do not restamp themselves"
    );
}

#[test]
fn elapsed_partial_track_stop_preserves_surviving_take_clock_and_stable_ids() {
    let f = Fixture::new(true);
    let receipt = f.start();
    let start = *receipt.0.first_attachment.get().unwrap();
    let initial = report_at(&receipt, start + Duration::from_secs(5));
    let ids: Vec<_> = initial.tracks.iter().map(|t| t.id).collect();
    assert_eq!(initial.elapsed.unwrap().running_tracks, ids);
    f.commands
        .stop_recording(RecordingStop::Track(ids[0]))
        .unwrap();
    let remaining = report_at(&receipt, start + Duration::from_secs(6))
        .elapsed
        .unwrap();
    assert_eq!(remaining.duration, Duration::from_secs(6));
    assert_eq!(remaining.running_tracks, vec![ids[1]]);
    f.pump();
    assert_eq!(*receipt.0.first_attachment.get().unwrap(), start);
    f.commands
        .stop_recording(RecordingStop::Track(ids[1]))
        .unwrap();
    assert!(report_at(&receipt, start + Duration::from_secs(20))
        .elapsed
        .is_none());
    assert!(receipt
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .elapsed
        .is_none());
}

#[test]
fn elapsed_independent_takes_and_noop_start_do_not_share_or_restart_origins() {
    let f = Fixture::new(true);
    let first = start_bus(&f, "rec-1");
    let first_start = *first.0.first_attachment.get().unwrap();
    let second = start_bus(&f, "rec-2");
    let second_start = *second.0.first_attachment.get().unwrap();
    assert!(second_start >= first_start);
    let snapshot = f.commands.recording_snapshot().unwrap();
    let RecordingBusStartResult::NoOp(noop) = f
        .commands
        .enqueue_recording_bus(RecordingBusStart {
            start: RecordingStart {
                expected_config: snapshot.configuration.revision,
                ..f.intent()
            },
            expected_recording: snapshot.revision,
            bus: "rec-1".into(),
        })
        .unwrap()
    else {
        panic!("existing synthetic take")
    };
    assert!(noop.same_request(&first));
    f.pump();
    let at = second_start + Duration::from_secs(20);
    let a = report_at(&first, at).elapsed.unwrap();
    let b = report_at(&second, at).elapsed.unwrap();
    assert_eq!(a.observed_at, b.observed_at);
    assert_eq!(a.duration, at.duration_since(first_start));
    assert_eq!(b.duration, Duration::from_secs(20));
    assert_ne!(a.running_tracks, b.running_tracks);
    assert_eq!(*first.0.first_attachment.get().unwrap(), first_start);
    f.commands
        .stop_recording(RecordingStop::Take(b.running_tracks[0].take()))
        .unwrap();
    assert!(report_at(&second, at).elapsed.is_none());
    assert!(report_at(&first, at).elapsed.is_some());
}

#[test]
fn elapsed_stop_ingress_fault_disconnect_and_common_shutdown_remove_running_evidence() {
    for end in 0..4 {
        let f = Fixture::new(false);
        let receipt = f.start();
        let at = *receipt.0.first_attachment.get().unwrap() + Duration::from_secs(10);
        assert!(report_at(&receipt, at).elapsed.is_some());
        match end {
            0 => {
                f.commands.stop_recording(RecordingStop::All).unwrap();
            }
            1 => receipt.0.ingress_fault(1),
            2 => {
                f.commands.publish(Momentopname::default());
            }
            _ => f.owner.completion.stop_tracked(),
        }
        assert!(report_at(&receipt, at).elapsed.is_none());
        assert!(receipt
            .wait_timeout(Duration::from_secs(2))
            .unwrap()
            .elapsed
            .is_none());
    }
}

#[test]
fn elapsed_known_track_write_failure_is_unavailable_before_whole_writer_completion() {
    use std::io::{Seek, SeekFrom, Write};
    use std::sync::{atomic::AtomicBool, Mutex};

    struct FaultFile {
        file: std::fs::File,
        index: usize,
        armed: Arc<AtomicBool>,
        gate: Arc<Mutex<Option<mpsc::Receiver<()>>>>,
        entered: mpsc::Sender<()>,
    }
    impl Write for FaultFile {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.index == 0 && self.armed.load(Ordering::Acquire) {
                return Err(std::io::Error::other("synthetic timing write failure"));
            }
            self.file.write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()
        }
    }
    impl Seek for FaultFile {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            if self.index == 1 && self.armed.load(Ordering::Acquire) {
                if let Some(gate) = self.gate.lock().unwrap().take() {
                    self.entered.send(()).unwrap();
                    gate.recv_timeout(Duration::from_secs(2)).unwrap();
                }
            }
            self.file.seek(position)
        }
    }
    let f = Fixture::new(true);
    let receipt = f.commands.enqueue_recording(f.intent()).unwrap();
    let armed = Arc::new(AtomicBool::new(false));
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Mutex::new(Some(gate)));
    let writer_armed = armed.clone();
    f.drain_with(|root, tracks, rate, owner| {
        let mut index = 0;
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            let file = FaultFile {
                file: std::fs::File::create(path)?,
                index,
                armed: writer_armed.clone(),
                gate: gate.clone(),
                entered: entered.clone(),
            };
            index += 1;
            hound::WavWriter::new(file, spec).map_err(std::io::Error::other)
        })
    });
    until(&receipt, |r| {
        f.pump();
        r.started_once
    });
    let at = *receipt.0.first_attachment.get().unwrap() + Duration::from_secs(10);
    assert!(report_at(&receipt, at).elapsed.is_some());
    armed.store(true, Ordering::Release);
    let installed = receipt.0.installed.get().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match installed
            .writer
            .sender
            .try_audio(installed.tracks[0].id, vec![0.25])
        {
            Ok(()) => break,
            Err(opname::WriterSendError::Busy) if Instant::now() < deadline => {
                std::thread::yield_now()
            }
            other => panic!("synthetic write input not admitted: {other:?}"),
        }
    }
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let failed = report_at(&receipt, at);
    release.send(()).unwrap(); // Always release the other track before asserting.
    assert!(matches!(
        failed.writer.as_ref().unwrap().tracks[0].phase,
        opname::TrackPhase::Finalized(Err(_))
    ));
    assert!(failed.writer.as_ref().unwrap().outcome.is_none());
    assert!(
        failed.elapsed.is_none(),
        "known track failure must not leave a running clock"
    );
}
