//! Independent synthetic takes; no server, hardware, or UI adoption.
use super::*;

impl Fixture {
    fn bus_intent(&self, bus: &str) -> RecordingBusStart {
        let snapshot = self.commands.recording_snapshot().unwrap();
        RecordingBusStart {
            start: RecordingStart {
                expected_config: snapshot.configuration.revision,
                ..self.intent()
            },
            expected_recording: snapshot.revision,
            bus: bus.into(),
        }
    }
    fn bus_start(&self, bus: &str) -> RecordingReceipt {
        let RecordingBusStartResult::Admitted(receipt) = self
            .commands
            .enqueue_recording_bus(self.bus_intent(bus))
            .unwrap()
        else {
            panic!("new take")
        };
        self.drain();
        until(&receipt, |r| {
            self.pump();
            r.started_once
        });
        receipt
    }
    fn fanout(&self, blocks: &[Vec<f32>]) {
        for capture in self.capture.borrow().iter() {
            if capture.accepts(blocks[0].len(), 1, 48000) {
                capture.ingest(blocks, blocks[0].len());
            }
        }
    }
}
fn finish(receipt: &RecordingReceipt) -> RecordingReport {
    let report = receipt.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(report.is_success(), "{report:?}");
    report
}

#[test]
fn per_bus_starts_use_separate_immutable_takes_and_do_not_implicitly_arm() {
    let mut f = Fixture::new(true);
    let before = f.commands.config_snapshot().unwrap();
    let mut config = before.configuration.unwrap();
    config.buses.states.get_mut("rec-2").unwrap().armed = false;
    f.commands
        .enqueue_config(bus_config::IdleConfigIntent {
            ticket: 2,
            expected: before.revision,
            configuration: config.clone(),
        })
        .unwrap();
    f.commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |work| f.commands.apply_config(work, &mut f.target),
    );
    let first = f.bus_start("rec-1");
    f.fanout(&[vec![0.25], vec![0.5]]);
    let first_mapping = first.try_report().unwrap().tracks;
    let second = f.bus_start("rec-2");
    f.fanout(&[vec![-0.25], vec![-0.5]]);
    let all = f.commands.stop_recording(RecordingStop::All).unwrap();
    assert_eq!(all.receipts().len(), 2);
    let a = finish(&first);
    let b = finish(&second);
    assert_ne!(
        a.writer.as_ref().unwrap().take,
        b.writer.as_ref().unwrap().take
    );
    assert_ne!(
        a.writer.as_ref().unwrap().directory,
        b.writer.as_ref().unwrap().directory
    );
    assert!(Arc::ptr_eq(&first_mapping, &a.tracks));
    assert_eq!(a.tracks[0].channels, [0]);
    assert_eq!(b.tracks[0].channels, [1]);
    assert_wav(
        &a.writer.unwrap().directory.unwrap().join("Rec 1.wav"),
        1,
        48000,
        &[0.25, -0.25],
    );
    assert_wav(
        &b.writer.unwrap().directory.unwrap().join("Rec 2.wav"),
        1,
        48000,
        &[-0.5],
    );
    assert_eq!(
        f.commands.config_snapshot().unwrap().configuration,
        Some(config)
    );
}

#[test]
fn duplicate_bus_is_explicit_noop_in_queued_opening_and_started_phases() {
    let f = Fixture::new(true);
    let RecordingBusStartResult::Admitted(first) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-1"))
        .unwrap()
    else {
        panic!()
    };
    let before = f.commands.recording_snapshot().unwrap();
    let RecordingBusStartResult::NoOp(again) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-1"))
        .unwrap()
    else {
        panic!()
    };
    assert!(Arc::ptr_eq(&first.0, &again.0));
    assert_eq!(
        again.try_report().unwrap().progress,
        RecordingProgress::Queued
    );
    assert_eq!(
        f.commands.recording_snapshot().unwrap().revision,
        before.revision
    );
    assert_eq!(f.commands.0.lock().unwrap().pending.len(), 1);
    assert_eq!(std::fs::read_dir(&f.root.0).unwrap().count(), 0);
    assert!(matches!(
        f.commands.enqueue_recording_bus(f.bus_intent("rec-2")),
        Err(RecordingError::Busy)
    ));
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
    let RecordingBusStartResult::NoOp(opening) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-1"))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        opening.try_report().unwrap().progress,
        RecordingProgress::Opening
    );
    assert!(!opening.try_report().unwrap().started_once);
    release.send(()).unwrap();
    until(&first, |r| {
        f.pump();
        r.started_once
    });
    let RecordingBusStartResult::NoOp(active) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-1"))
        .unwrap()
    else {
        panic!()
    };
    assert!(Arc::ptr_eq(&first.0, &active.0));
    f.commands.stop_recording(RecordingStop::All).unwrap();
    finish(&first);
}

#[test]
fn membership_revision_rejects_old_intent_without_a_config_or_graph_change() {
    let f = Fixture::new(true);
    let first = f.bus_start("rec-1");
    let second = f.bus_start("rec-2");
    let old = f.bus_intent("rec-1");
    let before = f.commands.recording_snapshot().unwrap();
    let id = first.try_report().unwrap().tracks[0].id;
    f.commands.stop_recording(RecordingStop::Track(id)).unwrap();
    f.pump();
    let after = f.commands.recording_snapshot().unwrap();
    assert_eq!(before.configuration.revision, after.configuration.revision);
    assert_ne!(before.revision, after.revision);
    assert!(matches!(
        f.commands.enqueue_recording_bus(old),
        Err(RecordingError::StaleRecording)
    ));
    finish(&first);
    let replacement = f.bus_start("rec-1");
    assert_ne!(replacement.try_report().unwrap().tracks[0].id, id);
    f.commands.stop_recording(RecordingStop::Track(id)).unwrap();
    assert!(!replacement.try_report().unwrap().stop_requested);
    f.commands.stop_recording(RecordingStop::All).unwrap();
    finish(&replacement);
    finish(&second);
}

#[test]
fn failed_addition_never_stops_an_existing_take() {
    for before_writer in [false, true] {
        let f = Fixture::new(true);
        let first = f.bus_start("rec-1");
        f.fanout(&[vec![0.25], vec![0.5]]);
        let RecordingBusStartResult::Admitted(added) = f
            .commands
            .enqueue_recording_bus(f.bus_intent("rec-2"))
            .unwrap()
        else {
            panic!()
        };
        if before_writer {
            f.drain_with(|_, _, _, _| {
                Err(std::io::Error::other("synthetic writer admission failure"))
            });
        } else {
            f.drain_with(|root, tracks, rate, owner| {
                opname::tracked_writer_with::<std::io::Cursor<Vec<u8>>, _>(
                    root,
                    tracks,
                    rate,
                    owner,
                    |_, _| Err(std::io::Error::other("synthetic opening failure")),
                )
            });
        }
        let failure = added.wait_timeout(Duration::from_secs(2)).unwrap();
        assert!(!failure.is_success());
        assert!(!failure.started_once);
        assert!(matches!(failure.failure, Some(RecordingError::Writer(_))));
        assert!(!first.try_report().unwrap().stop_requested);
        f.pump();
        f.fanout(&[vec![-0.25], vec![-0.5]]);
        f.commands.stop_recording(RecordingStop::All).unwrap();
        let report = finish(&first);
        assert_wav(
            &report.writer.unwrap().directory.unwrap().join("Rec 1.wav"),
            1,
            48000,
            &[0.25, -0.25],
        );
    }
}

#[test]
fn saturation_failure_is_scoped_to_one_take_and_healthy_capture_continues() {
    let f = Fixture::new(true);
    let RecordingBusStartResult::Admitted(first) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-1"))
        .unwrap()
    else {
        panic!()
    };
    let (good_entered, good_waiting) = mpsc::channel();
    let (good_release, good_gate) = mpsc::channel();
    let good_gate = Arc::new(Gate {
        armed: std::sync::atomic::AtomicBool::new(false),
        entered: good_entered,
        release: Mutex::new(good_gate),
    });
    let thread_good = good_gate.clone();
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            hound::WavWriter::new(
                GatedFile {
                    file: std::fs::File::create(path)?,
                    gate: thread_good.clone(),
                },
                spec,
            )
            .map_err(std::io::Error::other)
        })
    });
    until(&first, |r| {
        f.pump();
        r.started_once
    });
    good_gate.armed.store(true, Ordering::Release);
    // Put the healthy consumer at a deterministic file-write gate, outside its
    // queue mutex. This probe injects Full only into the other take, not a
    // scheduler-dependent Busy failure into the healthy one.
    let good = first.0.installed.get().unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match good.writer.sender.try_audio(good.tracks[0].id, vec![0.125]) {
            Ok(()) => break,
            Err(opname::WriterSendError::Busy) if Instant::now() < deadline => {
                std::thread::yield_now()
            }
            e => panic!("{e:?}"),
        }
    }
    good_waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let RecordingBusStartResult::Admitted(second) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-2"))
        .unwrap()
    else {
        panic!()
    };
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Gate {
        armed: std::sync::atomic::AtomicBool::new(false),
        entered,
        release: Mutex::new(gate),
    });
    let worker_gate = gate.clone();
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            hound::WavWriter::new(
                GatedFile {
                    file: std::fs::File::create(path)?,
                    gate: worker_gate.clone(),
                },
                spec,
            )
            .map_err(std::io::Error::other)
        })
    });
    until(&second, |r| {
        f.pump();
        r.started_once
    });
    gate.armed.store(true, Ordering::Release);
    let writer = second.0.installed.get().unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match writer
            .writer
            .sender
            .try_audio(writer.tracks[0].id, vec![0.5])
        {
            Ok(()) => break,
            Err(opname::WriterSendError::Busy) if Instant::now() < deadline => {
                std::thread::yield_now()
            }
            error => panic!("{error:?}"),
        }
    }
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    f.fanout(&[vec![0.25], vec![0.5]]);
    let bad = f
        .capture
        .borrow()
        .iter()
        .find(|c| Arc::ptr_eq(&c.0, &second.0))
        .unwrap()
        .clone();
    for _ in 0..=opname::WRITER_AUDIO_CAPACITY {
        bad.ingest(&[vec![0.25], vec![0.5]], 1);
    }
    assert_eq!(
        second.try_report().unwrap().failure,
        Some(RecordingError::Ingress(opname::WriterSendError::Full))
    );
    assert!(!first.try_report().unwrap().stop_requested);
    f.pump();
    f.fanout(&[vec![-0.25], vec![-0.5]]);
    let all = f.commands.stop_recording(RecordingStop::All).unwrap();
    assert_eq!(all.receipts().len(), 2);
    assert!(second.wait_timeout(Duration::from_millis(1)).is_err());
    release.send(()).unwrap();
    good_release.send(()).unwrap();
    let bad = second.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(!bad.is_success());
    assert!(bad.writer.unwrap().is_success());
    let good = finish(&first);
    assert_wav(
        &good.writer.unwrap().directory.unwrap().join("Rec 1.wav"),
        1,
        48000,
        &[0.125, 0.25, -0.25],
    );
}

#[test]
fn reserved_stop_all_returns_active_and_queued_receipts_under_command_saturation() {
    let f = Fixture::new(true);
    let first = f.bus_start("rec-1");
    let RecordingBusStartResult::Admitted(queued) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-2"))
        .unwrap()
    else {
        panic!()
    };
    while f
        .commands
        .legacy(Opdracht::ZetMute {
            knoop: 900,
            mute: true,
        })
        .is_ok()
    {}
    assert_eq!(f.commands.0.lock().unwrap().pending.len(), NORMAL_CAPACITY);
    let result = f.commands.stop_recording(RecordingStop::All).unwrap();
    assert_eq!(result.receipts().len(), 2);
    assert_eq!(
        f.commands.0.lock().unwrap().pending.len(),
        NORMAL_CAPACITY - 1
    );
    assert_eq!(
        queued.try_report().unwrap().failure,
        Some(RecordingError::CancelledBeforeStart)
    );
    assert!(queued.try_report().unwrap().writer.is_none());
    finish(&first);
    f.drain_with(|_, _, _, _| panic!("cancelled start must not open files"));
}

#[test]
fn disconnect_preserves_intentional_stop_and_fails_other_takes_without_replay() {
    let f = Fixture::new(true);
    let first = f.bus_start("rec-1");
    let second = f.bus_start("rec-2");
    let old = f.bus_intent("rec-1");
    let first_id = first.try_report().unwrap().tracks[0].id;
    f.commands
        .stop_recording(RecordingStop::Track(first_id))
        .unwrap();
    f.commands.publish(Momentopname::default());
    let result = f.commands.stop_recording(RecordingStop::All).unwrap();
    assert_eq!(result.receipts().len(), 2);
    finish(&first);
    let failed = second.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(failed.failure, Some(RecordingError::Disconnected));
    assert!(matches!(
        f.commands.enqueue_recording_bus(old),
        Err(RecordingError::Disconnected)
    ));
    f.pump();
    assert!(f.capture.borrow().is_empty());
    assert!(f.commands.recording_snapshot().unwrap().takes.is_empty());
    assert!(matches!(
        f.commands.stop_recording(RecordingStop::Track(first_id)),
        Ok(RecordingStopResult::Retained(_))
    ));
}

#[test]
fn stale_origin_and_reused_port_cannot_report_noop_against_another_identity() {
    let f = Fixture::new(true);
    let first = f.bus_start("rec-1");
    let mut old = f.bus_intent("rec-1");
    f.commands.object_removed(901);
    f.commands.object_added(901);
    let graph = (*f.commands.current().value).clone();
    f.commands.publish(graph);
    old.expected_recording = f.commands.recording_snapshot().unwrap().revision;
    assert!(matches!(
        f.commands.enqueue_recording_bus(old),
        Err(RecordingError::InvalidOrigin)
    ));
    assert!(matches!(
        f.commands.enqueue_recording_bus(f.bus_intent("rec-1")),
        Err(RecordingError::AlreadyRecording)
    ));
    f.pump();
    let failure = first.wait_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(failure.failure, Some(RecordingError::InvalidOrigin));
    let replacement = f.bus_start("rec-1");
    f.commands.stop_recording(RecordingStop::All).unwrap();
    finish(&replacement);
}

#[test]
fn stopping_one_track_then_rearming_never_changes_original_take_mapping() {
    let f = Fixture::new(true);
    let original = f.start();
    let tracks = original.try_report().unwrap().tracks;
    f.samples(&[vec![0.25], vec![0.5]]);
    f.commands
        .stop_recording(RecordingStop::Track(tracks[0].id))
        .unwrap();
    until(&original, |r| {
        f.pump();
        matches!(
            r.writer.as_ref().unwrap().tracks[0].phase,
            opname::TrackPhase::Finalized(_)
        )
    });
    let added = f.bus_start("rec-1");
    f.fanout(&[vec![-0.25], vec![-0.5]]);
    f.commands.stop_recording(RecordingStop::All).unwrap();
    let old = finish(&original);
    let new = finish(&added);
    assert!(Arc::ptr_eq(&tracks, &old.tracks));
    assert_ne!(new.tracks[0].id, tracks[0].id);
    let path = old.writer.unwrap().directory.unwrap();
    assert_wav(&path.join("Rec 1.wav"), 1, 48000, &[0.25]);
    assert_wav(&path.join("Rec 2.wav"), 1, 48000, &[0.5, -0.5]);
    assert_wav(
        &new.writer.unwrap().directory.unwrap().join("Rec 1.wav"),
        1,
        48000,
        &[-0.25],
    );
}

#[test]
fn membership_revision_is_stable_for_samples_and_exhaustion_does_not_disable_stop() {
    let f = Fixture::new(true);
    let first = f.bus_start("rec-1");
    let before = f.commands.recording_snapshot().unwrap();
    f.fanout(&[vec![0.25], vec![0.5]]);
    f.pump();
    assert_eq!(
        f.commands.recording_snapshot().unwrap().revision,
        before.revision
    );
    f.commands.0.lock().unwrap().recording.revision = u64::MAX;
    assert!(matches!(
        f.commands.enqueue_recording_bus(f.bus_intent("rec-2")),
        Err(RecordingError::RevisionExhausted)
    ));
    let stop = f.commands.stop_recording(RecordingStop::All).unwrap();
    assert_eq!(stop.receipts().len(), 1);
    finish(&first);
}

struct LongExit(mpsc::Sender<()>, mpsc::Receiver<()>);
impl Drop for LongExit {
    fn drop(&mut self) {
        let _ = self.0.send(());
        let _ = self.1.recv_timeout(Duration::from_secs(10));
    }
}

#[test]
fn recording_flag_change_after_admission_preserves_start_context_but_not_stale_admission() {
    let f = Fixture::new(true);
    let RecordingBusStartResult::Admitted(old) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-1"))
        .unwrap()
    else {
        panic!()
    };
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let releases = ReleaseAll(vec![release]);
    let mut exit = Some(LongExit(entered, gate));
    f.drain_with(|root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            LONG_EXIT.with(|slot| *slot.borrow_mut() = exit.take());
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        })
    });
    until(&old, |r| {
        f.pump();
        r.started_once
    });
    let id = old.try_report().unwrap().tracks[0].id;
    f.commands.stop_recording(RecordingStop::Track(id)).unwrap();
    // Freeze membership after stop/close while deliberately withholding join.
    // This test changes only the aggregate flag after admission; a concurrent
    // membership change must still be rejected by the production contract.
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!old.try_report().unwrap().is_complete());
    let captured = f.bus_intent("rec-2");
    let expected = captured.start.expected_config;
    let RecordingBusStartResult::Admitted(new) =
        f.commands.enqueue_recording_bus(captured.clone()).unwrap()
    else {
        panic!()
    };
    // The production timer refreshes the aggregate flag before pumping/draining.
    f.commands.refresh_config(f.target.readback(), false, false);
    assert_ne!(f.commands.config_snapshot().unwrap().revision, expected);
    let mut stale = captured;
    stale.expected_recording = f.commands.recording_snapshot().unwrap().revision;
    assert!(matches!(
        f.commands.enqueue_recording_bus(stale),
        Err(RecordingError::StaleConfig)
    ));
    f.drain();
    until(&new, |r| {
        f.pump();
        r.started_once
    });
    assert_eq!(
        new.try_report().unwrap().config,
        expected,
        "never stamp with latest revision"
    );
    f.commands.stop_recording(RecordingStop::All).unwrap();
    drop(releases);
    finish(&old);
    finish(&new);
}

#[test]
fn preceding_logical_config_change_rejects_per_bus_start_before_any_writer_admission() {
    let mut f = Fixture::new(true);
    let snapshot = f.commands.config_snapshot().unwrap();
    let mut configuration = snapshot.configuration.unwrap();
    configuration.buses.states.get_mut("rec-1").unwrap().mute = true;
    f.commands
        .enqueue_config(bus_config::IdleConfigIntent {
            ticket: 12,
            expected: snapshot.revision,
            configuration,
        })
        .unwrap();
    let RecordingBusStartResult::Admitted(receipt) = f
        .commands
        .enqueue_recording_bus(f.bus_intent("rec-1"))
        .unwrap()
    else {
        panic!()
    };
    f.commands.drain_with_recording(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |work| f.commands.apply_config(work, &mut f.target),
        |work| {
            f.commands
                .start_recording_with(work, &f.owner.completion, false, |_, _, _, _| {
                    panic!("stale configuration must not open a writer")
                })
        },
    );
    let result = receipt.try_report().unwrap();
    assert!(result.is_complete());
    assert_eq!(result.failure, Some(RecordingError::StaleConfig));
    assert!(result.writer.is_none());
}
thread_local! {static LONG_EXIT:RefCell<Option<LongExit>>=const{RefCell::new(None)};}
struct ReleaseAll(Vec<mpsc::Sender<()>>);
impl Drop for ReleaseAll {
    fn drop(&mut self) {
        for release in &self.0 {
            let _ = release.send(());
        }
    }
}

#[test]
fn owned_take_capacity_includes_slow_teardown_and_stop_all_retains_every_result() {
    let mut f = Fixture::new(true);
    let mut releases = ReleaseAll(Vec::new());
    let mut receipts = Vec::new();
    for _ in 0..MAX_RECORDING_TAKES {
        let RecordingBusStartResult::Admitted(receipt) = f
            .commands
            .enqueue_recording_bus(f.bus_intent("rec-1"))
            .unwrap()
        else {
            panic!()
        };
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        releases.0.push(release);
        let mut exit = Some(LongExit(entered, gate));
        f.drain_with(|root, tracks, rate, owner| {
            opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
                LONG_EXIT.with(|slot| *slot.borrow_mut() = exit.take());
                hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
            })
        });
        until(&receipt, |r| {
            f.pump();
            r.started_once
        });
        let id = receipt.try_report().unwrap().tracks[0].id;
        f.commands.stop_recording(RecordingStop::Track(id)).unwrap();
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!receipt.try_report().unwrap().is_complete());
        receipts.push(receipt);
    }
    assert_eq!(
        f.commands.recording_snapshot().unwrap().takes.len(),
        MAX_RECORDING_TAKES
    );
    assert!(matches!(
        f.commands.enqueue_recording_bus(f.bus_intent("rec-1")),
        Err(RecordingError::Capacity)
    ));
    let all = f.commands.stop_recording(RecordingStop::All).unwrap();
    assert_eq!(all.receipts().len(), MAX_RECORDING_TAKES);
    for (a, b) in all.receipts().iter().zip(&receipts) {
        assert!(Arc::ptr_eq(&a.0, &b.0));
    }
    assert!(receipts[0].wait_timeout(Duration::from_millis(1)).is_err());
    drop(releases);
    for receipt in &receipts {
        finish(receipt);
    }
    assert_eq!(f.owner.finish().writers_finalized, MAX_RECORDING_TAKES);
}
