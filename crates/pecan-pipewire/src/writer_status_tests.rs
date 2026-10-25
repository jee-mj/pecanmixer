//! Temporary synthetic roots and fault-injected sinks only. No audio session.
use super::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
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
    fn finish(&mut self) -> ShutdownReport {
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
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        let _ = self.completion.wait_timeout(Duration::from_secs(2));
    }
}
fn report_when(receipt: &WriterReceipt, condition: impl Fn(&WriterReport) -> bool) -> WriterReport {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(report) = receipt.try_report() {
            if condition(&report) {
                return report;
            }
        }
        assert!(
            Instant::now() < deadline,
            "writer status deadline: {:?}",
            receipt.try_report()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn ready(receipt: &WriterReceipt) -> WriterReport {
    report_when(receipt, |r| r.phase == WriterPhase::Ready)
}
fn accepted(mut send: impl FnMut() -> Result<(), WriterSendError>) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match send() {
            Ok(()) => return,
            Err(WriterSendError::Busy) if Instant::now() < deadline => std::thread::yield_now(),
            error => panic!("send was not accepted: {error:?}"),
        }
    }
}
fn audio(writer: &TrackedWriter, index: usize, values: &[f32]) {
    let id = writer.receipt.track_id(index).unwrap();
    accepted(|| writer.sender.try_audio(id, values.to_vec()));
}
fn stop(writer: &TrackedWriter) {
    accepted(|| writer.sender.try_stop());
}

#[test]
fn readiness_and_one_track_acknowledge_real_wav_bytes_before_take_join() {
    let dir = TestDir::new("tracked-ready-close");
    let mut owner = Owner::new();
    let writer = tracked_writer(
        &dir.0,
        vec![track("First", 1), track("Other", 1)],
        48000,
        &owner.completion,
    )
    .unwrap();
    let ready = ready(&writer.receipt);
    assert!(ready.was_ready);
    assert!(!ready.stop_requested);
    let first = writer.receipt.track_id(0).unwrap();
    audio(&writer, 0, &[0.25, -0.5]);
    audio(&writer, 1, &[0.5]);
    accepted(|| writer.sender.try_close(first));
    accepted(|| writer.sender.try_close(first));
    let closed = report_when(&writer.receipt, |r| {
        matches!(&r.tracks[0].phase, TrackPhase::Finalized(Ok(())))
    });
    assert_eq!(closed.phase, WriterPhase::Ready);
    assert_eq!(closed.tracks[1].phase, TrackPhase::Open);
    assert!(writer.receipt.try_complete().is_none());
    assert_eq!(
        writer.sender.try_audio(first, vec![9.0]),
        Err(WriterSendError::Closed)
    );
    let path = closed.directory.unwrap();
    assert_wav(&path.join("First.wav"), 1, 48000, &[0.25, -0.5]);
    audio(&writer, 1, &[0.75]);
    stop(&writer);
    stop(&writer);
    let joined = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(joined.is_success());
    assert!(joined
        .tracks
        .iter()
        .all(|t| matches!(t.phase, TrackPhase::Finalized(Ok(())))));
    assert_wav(&path.join("Other.wav"), 1, 48000, &[0.5, 0.75]);
    assert_eq!(owner.finish().writers_finalized, 1);
}

#[test]
fn partial_initialization_never_reports_ready_and_finalizes_opened_tracks() {
    let dir = TestDir::new("tracked-open-failure");
    let mut owner = Owner::new();
    let writer = tracked_writer_with(
        &dir.0,
        vec![track("First", 1), track("Blocked", 1), track("Later", 1)],
        48000,
        &owner.completion,
        |path, spec| {
            if path.file_name().unwrap() == "Blocked.wav" {
                return Err(std::io::Error::other("injected open failure"));
            }
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        },
    )
    .unwrap();
    let joined = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(!joined.was_ready);
    assert!(!joined.is_success());
    assert!(
        matches!(joined.outcome,Some(WriterOutcome::Failed(ref e)) if e.contains("injected open failure"))
    );
    assert_eq!(joined.tracks[0].phase, TrackPhase::Finalized(Ok(())));
    assert_eq!(
        joined.tracks[1].phase,
        TrackPhase::NotOpened(NotOpened::InitializationFailed)
    );
    assert_eq!(
        joined.tracks[2].phase,
        TrackPhase::NotOpened(NotOpened::InitializationFailed)
    );
    assert_wav(&joined.directory.unwrap().join("First.wav"), 1, 48000, &[]);
    assert_eq!(owner.finish().writers_failed, 1);
}

#[test]
fn stop_during_opening_is_retained_idempotent_and_cannot_be_reported_as_start_ready() {
    let dir = TestDir::new("tracked-opening-stop");
    let mut owner = Owner::new();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let writer = tracked_writer_with(
        &dir.0,
        vec![track("First", 1), track("Later", 1)],
        48000,
        &owner.completion,
        move |path, spec| {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(2)).unwrap();
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        },
    )
    .unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        writer
            .sender
            .try_audio(writer.receipt.track_id(0).unwrap(), vec![1.0]),
        Err(WriterSendError::NotReady)
    );
    stop(&writer);
    stop(&writer);
    let before = writer.receipt.try_report().unwrap();
    assert_eq!(before.phase, WriterPhase::Finalizing);
    assert!(!before.was_ready);
    assert!(writer
        .receipt
        .wait_timeout(Duration::from_millis(1))
        .is_err());
    release.send(()).unwrap();
    let joined = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(joined.outcome, Some(WriterOutcome::StoppedBeforeReady));
    assert!(!joined.was_ready);
    assert_eq!(joined.tracks[0].phase, TrackPhase::Finalized(Ok(())));
    assert_eq!(
        joined.tracks[1].phase,
        TrackPhase::NotOpened(NotOpened::Stopped)
    );
    assert_wav(&joined.directory.unwrap().join("First.wav"), 1, 48000, &[]);
    assert!(owner.finish().is_success());
}

#[test]
fn write_and_close_failures_are_retained_per_track_without_hiding_other_finalization() {
    for failure in [Failure::Write, Failure::Seek, Failure::Flush] {
        let dir = TestDir::new("tracked-fault");
        let mut owner = Owner::new();
        let bad = Arc::new(Mutex::new(SinkState::default()));
        let good = Arc::new(Mutex::new(SinkState::default()));
        let b = bad.clone();
        let g = good.clone();
        let writer = tracked_writer_with(
            &dir.0,
            vec![track("Bad", 1), track("Good", 1)],
            48000,
            &owner.completion,
            move |path, spec| {
                let state = if path.file_name().unwrap() == "Bad.wav" {
                    b.clone()
                } else {
                    g.clone()
                };
                hound::WavWriter::new(Sink(state), spec).map_err(std::io::Error::other)
            },
        )
        .unwrap();
        ready(&writer.receipt);
        audio(&writer, 1, &[0.25, 0.5]);
        bad.lock().unwrap().fail = Some(failure);
        if matches!(failure, Failure::Write) {
            audio(&writer, 0, &[0.75]);
        } else {
            accepted(|| writer.sender.try_close(writer.receipt.track_id(0).unwrap()));
        }
        let joined = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            matches!(joined.tracks[0].phase,TrackPhase::Finalized(Err(ref e)) if e.contains("injected"))
        );
        assert_eq!(joined.tracks[1].phase, TrackPhase::Finalized(Ok(())));
        assert!(!joined.is_success());
        let good = good.lock().unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(good.data.get_ref())).unwrap();
        assert_eq!(
            reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            [0.25, 0.5]
        );
        assert_eq!(owner.finish().writers_failed, 1);
    }
}

#[test]
fn stable_ids_reject_late_track_commands_from_another_take() {
    let dir = TestDir::new("tracked-ids");
    let mut owner = Owner::new();
    let first = tracked_writer(
        &dir.0,
        vec![track("Same name", 1)],
        48000,
        &owner.completion,
    )
    .unwrap();
    ready(&first.receipt);
    let stale = first.receipt.track_id(0).unwrap();
    stop(&first);
    let old = first.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    let next = tracked_writer(
        &dir.0,
        vec![track("Same name", 1)],
        48000,
        &owner.completion,
    )
    .unwrap();
    ready(&next.receipt);
    assert_ne!(first.receipt.take_id(), next.receipt.take_id());
    assert_eq!(
        next.sender.try_close(stale),
        Err(WriterSendError::WrongTrack)
    );
    assert_eq!(
        next.sender.try_audio(stale, vec![1.0]),
        Err(WriterSendError::WrongTrack)
    );
    stop(&next);
    let new = next.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_ne!(old.directory, new.directory);
    assert_eq!(first.receipt.try_complete(), Some(old));
    assert_eq!(owner.finish().writers_finalized, 2);
}

struct Gate {
    armed: AtomicBool,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct GatedSink(Sink, Arc<Gate>);
impl Write for GatedSink {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.1.armed.swap(false, Ordering::AcqRel) {
            let _ = self.1.entered.send(());
            self.1
                .release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .map_err(std::io::Error::other)?;
        }
        self.0.write(data)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
impl Seek for GatedSink {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

#[test]
fn bounded_audio_queue_reserves_ordered_close_and_stop_capacity() {
    let dir = TestDir::new("tracked-capacity");
    let mut owner = Owner::new();
    let state = Arc::new(Mutex::new(SinkState::default()));
    let s = state.clone();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Gate {
        armed: AtomicBool::new(false),
        entered,
        release: Mutex::new(gate),
    });
    let g = gate.clone();
    let writer = tracked_writer_with(
        &dir.0,
        vec![track("Audio", 1)],
        48000,
        &owner.completion,
        move |_, spec| {
            hound::WavWriter::new(GatedSink(Sink(s.clone()), g.clone()), spec)
                .map_err(std::io::Error::other)
        },
    )
    .unwrap();
    ready(&writer.receipt);
    let id = writer.receipt.track_id(0).unwrap();
    let mut oversized = vec![0.0; MAX_WRITER_SAMPLES + 1];
    oversized.truncate(1);
    assert_eq!(
        writer.sender.try_audio(id, oversized),
        Err(WriterSendError::InvalidSamples)
    );
    assert_eq!(
        writer.sender.try_audio(id, vec![f32::NAN]),
        Err(WriterSendError::InvalidSamples)
    );
    assert_eq!(
        writer.receipt.try_report().unwrap().tracks[0].samples_accepted,
        0
    );
    gate.armed.store(true, Ordering::Release);
    audio(&writer, 0, &[0.25]);
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    for _ in 0..WRITER_AUDIO_CAPACITY {
        audio(&writer, 0, &[0.25]);
    }
    let id = writer.receipt.track_id(0).unwrap();
    assert_eq!(
        writer.sender.try_audio(id, vec![9.0]),
        Err(WriterSendError::Full)
    );
    accepted(|| writer.sender.try_close(id));
    accepted(|| writer.sender.try_close(id));
    stop(&writer);
    stop(&writer);
    assert!(writer.receipt.try_complete().is_none());
    release.send(()).unwrap();
    let report = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(report.is_success());
    let state = state.lock().unwrap();
    let mut wav = hound::WavReader::new(Cursor::new(state.data.get_ref())).unwrap();
    assert_eq!(
        wav.samples::<f32>().collect::<Result<Vec<_>, _>>().unwrap(),
        vec![0.25; WRITER_AUDIO_CAPACITY + 1]
    );
    assert_eq!(owner.finish().writers_finalized, 1);
}

#[test]
fn all_senders_dropped_finalize_accepted_samples_without_a_stop_message() {
    let dir = TestDir::new("tracked-eof");
    let mut owner = Owner::new();
    let writer = tracked_writer(&dir.0, vec![track("Audio", 1)], 48000, &owner.completion).unwrap();
    ready(&writer.receipt);
    audio(&writer, 0, &[0.25]);
    let receipt = writer.receipt.clone();
    let clone = writer.sender.clone();
    drop(writer);
    assert!(receipt.try_complete().is_none());
    drop(clone);
    let report = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(report.is_success());
    assert_wav(
        &report.directory.unwrap().join("Audio.wav"),
        1,
        48000,
        &[0.25],
    );
    assert!(owner.finish().is_success());
}

#[test]
fn tracked_resource_limits_fail_before_io_and_capacity_returns_only_after_join() {
    let dir = TestDir::new("tracked-limits");
    let mut owner = Owner::new();
    let root = dir.0.join("invalid");
    let too_many = (0..MAX_TRACKED_TRACKS + 1)
        .map(|i| track(&format!("Synthetic {i}"), 1))
        .collect();
    assert!(tracked_writer(&root, too_many, 48000, &owner.completion).is_err());
    assert!(!root.exists());
    assert!(tracked_writer(
        &root,
        vec![track("A/B", 1), track("A?B", 1)],
        48000,
        &owner.completion
    )
    .is_err());
    assert!(!root.exists());
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let many = (0..MAX_TRACKED_TRACKS)
        .map(|i| track(&format!("Synthetic {i}"), 1))
        .collect();
    let writer = tracked_writer_with(&dir.0, many, 48000, &owner.completion, move |path, spec| {
        entered.send(()).unwrap();
        gate.recv_timeout(Duration::from_secs(2)).unwrap();
        hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
    })
    .unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        tracked_writer(&dir.0, vec![track("Extra", 1)], 48000, &owner.completion)
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    stop(&writer);
    release.send(()).unwrap();
    writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    let next = tracked_writer(&dir.0, vec![track("Next", 1)], 48000, &owner.completion).unwrap();
    ready(&next.receipt);
    stop(&next);
    next.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(owner.finish().is_success());
    assert_eq!(
        tracked_writer(&dir.0, vec![track("Too late", 1)], 48000, &owner.completion)
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::BrokenPipe
    );
}

struct HeldExit {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl Drop for HeldExit {
    fn drop(&mut self) {
        let _ = self.entered.send(());
        let _ = self.release.recv_timeout(Duration::from_secs(2));
    }
}
thread_local! {static EXIT:RefCell<Option<HeldExit>>=const{RefCell::new(None)};}
#[test]
fn finalized_bytes_are_not_joined_completion_while_thread_local_teardown_is_blocked() {
    let dir = TestDir::new("tracked-tls");
    let mut owner = Owner::new();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut exit = Some(HeldExit {
        entered,
        release: gate,
    });
    let writer = tracked_writer_with(
        &dir.0,
        vec![track("Audio", 1)],
        48000,
        &owner.completion,
        move |path, spec| {
            if let Some(exit) = exit.take() {
                EXIT.with(|slot| *slot.borrow_mut() = Some(exit));
            }
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        },
    )
    .unwrap();
    ready(&writer.receipt);
    audio(&writer, 0, &[0.5]);
    stop(&writer);
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let start = Instant::now();
    let report = writer.receipt.try_report().unwrap();
    assert_eq!(report.phase, WriterPhase::Finished);
    assert_eq!(report.tracks[0].phase, TrackPhase::Finalized(Ok(())));
    assert!(!report.is_success());
    assert!(writer.receipt.try_complete().is_none());
    assert!(writer
        .receipt
        .wait_timeout(Duration::from_millis(1))
        .is_err());
    assert!(start.elapsed() < Duration::from_millis(100));
    assert_wav(
        &report.directory.unwrap().join("Audio.wav"),
        1,
        48000,
        &[0.5],
    );
    release.send(()).unwrap();
    assert!(writer
        .receipt
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
    assert!(owner.finish().is_success());
}

#[test]
fn panicked_initialization_does_not_invent_track_finalization_or_ready() {
    let dir = TestDir::new("tracked-panic");
    let mut owner = Owner::new();
    let writer = tracked_writer_with::<Cursor<Vec<u8>>, _>(
        &dir.0,
        vec![track("Audio", 1)],
        48000,
        &owner.completion,
        |_, _| panic!("synthetic open panic"),
    )
    .unwrap();
    let report = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(!report.was_ready);
    assert!(!report.is_success());
    assert!(matches!(report.outcome, Some(WriterOutcome::Failed(_))));
    assert!(!matches!(
        report.tracks[0].phase,
        TrackPhase::Finalized(Ok(()))
    ));
    assert_eq!(owner.finish().writers_failed, 1);
}

#[test]
fn take_failure_marks_accepted_but_unwritten_sibling_samples_as_loss() {
    let dir = TestDir::new("tracked-unwritten");
    let mut owner = Owner::new();
    let bad = Arc::new(Mutex::new(SinkState::default()));
    let good = Arc::new(Mutex::new(SinkState::default()));
    let b = bad.clone();
    let g = good.clone();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Gate {
        armed: AtomicBool::new(false),
        entered,
        release: Mutex::new(gate),
    });
    let hold = gate.clone();
    let writer = tracked_writer_with(
        &dir.0,
        vec![track("Bad", 1), track("Good", 1)],
        48000,
        &owner.completion,
        move |path, spec| {
            let state = if path.file_name().unwrap() == "Bad.wav" {
                b.clone()
            } else {
                g.clone()
            };
            hound::WavWriter::new(GatedSink(Sink(state), hold.clone()), spec)
                .map_err(std::io::Error::other)
        },
    )
    .unwrap();
    ready(&writer.receipt);
    gate.armed.store(true, Ordering::Release);
    audio(&writer, 0, &[0.1]);
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    audio(&writer, 1, &[0.5]);
    bad.lock().unwrap().fail = Some(Failure::Write);
    release.send(()).unwrap();
    let report = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(report.tracks[1].samples_accepted, 1);
    assert_eq!(report.tracks[1].samples_written, 0);
    assert!(
        matches!(report.tracks[1].phase,TrackPhase::Finalized(Err(ref e)) if e.contains("accepted samples were not written"))
    );
    let good = good.lock().unwrap();
    let reader = hound::WavReader::new(Cursor::new(good.data.get_ref())).unwrap();
    assert_eq!(reader.duration(), 0);
    assert!(!report.is_success());
    assert_eq!(owner.finish().writers_failed, 1);
}

#[test]
fn owner_shutdown_drains_tracked_writers_even_while_senders_are_retained() {
    let dir = TestDir::new("tracked-owner-stop");
    let mut owner = Owner::new();
    let writer = tracked_writer(&dir.0, vec![track("Audio", 1)], 48000, &owner.completion).unwrap();
    ready(&writer.receipt);
    audio(&writer, 0, &[0.25, -0.5]);
    owner.completion.stop_tracked(); // Same signal as Graaf::request_shutdown.
    let report = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(report.is_success());
    assert!(report.stop_requested);
    assert_wav(
        &report.directory.unwrap().join("Audio.wav"),
        1,
        48000,
        &[0.25, -0.5],
    );
    assert_eq!(
        writer
            .sender
            .try_audio(writer.receipt.track_id(0).unwrap(), vec![1.0]),
        Err(WriterSendError::Closed)
    );
    assert!(owner.finish().is_success());
}

#[test]
fn closing_one_of_sixty_four_tracks_releases_open_capacity_while_siblings_keep_recording() {
    let dir = TestDir::new("tracked-slot-rearm");
    let mut owner = Owner::new();
    let writer = tracked_writer(
        &dir.0,
        (0..MAX_TRACKED_TRACKS)
            .map(|i| track(&format!("Synthetic {i}"), 1))
            .collect(),
        48000,
        &owner.completion,
    )
    .unwrap();
    ready(&writer.receipt);
    audio(&writer, 0, &[0.25]);
    audio(&writer, 63, &[0.5]);
    assert!(tracked_writer(&dir.0, vec![track("Extra", 1)], 48000, &owner.completion).is_err());
    accepted(|| writer.sender.try_close(writer.receipt.track_id(0).unwrap()));
    let closed = report_when(&writer.receipt, |r| {
        matches!(r.tracks[0].phase, TrackPhase::Finalized(Ok(())))
    });
    assert_eq!(closed.phase, WriterPhase::Ready);
    assert_eq!(closed.tracks[63].phase, TrackPhase::Open);
    let replacement =
        tracked_writer(&dir.0, vec![track("Rearmed", 1)], 48000, &owner.completion).unwrap();
    ready(&replacement.receipt);
    audio(&replacement, 0, &[-0.25]);
    audio(&writer, 63, &[-0.5]);
    stop(&replacement);
    stop(&writer);
    let old = writer.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    let new = replacement
        .receipt
        .wait_timeout(Duration::from_secs(2))
        .unwrap();
    assert_eq!(old.tracks.len(), MAX_TRACKED_TRACKS);
    assert!(old.is_success() && new.is_success());
    let path = old.directory.unwrap();
    assert_wav(&path.join("Synthetic 0.wav"), 1, 48000, &[0.25]);
    assert_wav(&path.join("Synthetic 63.wav"), 1, 48000, &[0.5, -0.5]);
    assert_wav(
        &new.directory.unwrap().join("Rearmed.wav"),
        1,
        48000,
        &[-0.25],
    );
    assert_eq!(owner.finish().writers_finalized, 2);
}

#[test]
fn close_error_releases_file_capacity_but_never_claims_join_or_success() {
    let dir = TestDir::new("tracked-slot-error");
    let mut owner = Owner::new();
    let bad = Arc::new(Mutex::new(SinkState::default()));
    let b = bad.clone();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut exit = Some(HeldExit {
        entered,
        release: gate,
    });
    let mut index = 0;
    let writer = tracked_writer_with(
        &dir.0,
        (0..MAX_TRACKED_TRACKS)
            .map(|i| track(&format!("Synthetic {i}"), 1))
            .collect(),
        48000,
        &owner.completion,
        move |_, spec| {
            if let Some(exit) = exit.take() {
                EXIT.with(|slot| *slot.borrow_mut() = Some(exit));
            }
            let sink = if index == 0 {
                b.clone()
            } else {
                Arc::new(Mutex::new(SinkState::default()))
            };
            index += 1;
            hound::WavWriter::new(Sink(sink), spec).map_err(std::io::Error::other)
        },
    )
    .unwrap();
    ready(&writer.receipt);
    bad.lock().unwrap().fail = Some(Failure::Seek);
    accepted(|| writer.sender.try_close(writer.receipt.track_id(0).unwrap()));
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let failed = writer.receipt.try_report().unwrap();
    assert_eq!(failed.phase, WriterPhase::Finished);
    assert!(matches!(
        failed.tracks[0].phase,
        TrackPhase::Finalized(Err(_))
    ));
    assert!(writer
        .receipt
        .wait_timeout(Duration::from_millis(1))
        .is_err());
    let next = tracked_writer(
        &dir.0,
        vec![track("After error", 1)],
        48000,
        &owner.completion,
    )
    .unwrap();
    ready(&next.receipt);
    stop(&next);
    release.send(()).unwrap();
    assert!(!writer
        .receipt
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
    assert!(next
        .receipt
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
    let done = owner.finish();
    assert_eq!(done.writers_failed, 1);
    assert_eq!(done.writers_finalized, 1);
}
