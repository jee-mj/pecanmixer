//! Synthetic dynamic-arming lifecycle, temporary WAVs and bounded fault gates.
use super::*;
use crate::recording_arming::*;

fn fixture(armed: &[&str]) -> Fixture {
    let mut f = Fixture::new_layout(vec![1, 1, 1, 1]);
    let snapshot = f.commands.config_snapshot().unwrap();
    let mut config = snapshot.configuration.unwrap();
    for (key, state) in &mut config.buses.states {
        state.armed = armed.contains(&key.as_str());
    }
    f.commands
        .enqueue_config(bus_config::IdleConfigIntent {
            ticket: 2,
            expected: snapshot.revision,
            configuration: config,
        })
        .unwrap();
    f.commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |work| f.commands.apply_config(work, &mut f.target),
    );
    f
}
fn intent(f: &Fixture, armed: &[&str]) -> RecordingArming {
    let snapshot = f.commands.recording_snapshot().unwrap();
    RecordingArming {
        ticket: 40,
        origin: f.commands.current(),
        expected_config: snapshot.configuration.revision,
        expected_recording: snapshot.revision,
        armed: armed.iter().map(|s| s.to_string()).collect(),
        root: f.root.0.clone(),
    }
}
fn tick(f: &mut Fixture) {
    tick_with(f, opname::tracked_writer);
}
fn tick_with(
    f: &mut Fixture,
    factory: impl FnOnce(&std::path::Path, Vec<Spoor>, u32, &Shutdown) -> std::io::Result<TrackedWriter>,
) {
    f.pump();
    f.commands
        .drive_arming(&mut f.target, &f.owner.completion, false, factory);
}
fn decide(f: &mut Fixture, receipt: &ArmingReceipt) -> ArmingReport {
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        tick(f);
        if let Some(r) = receipt.try_report() {
            if r.is_decided() {
                return r;
            }
        }
        assert!(Instant::now() < end, "{:?}", receipt.try_report());
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn arm(f: &mut Fixture, armed: &[&str]) -> ArmingReceipt {
    let r = f.commands.enqueue_arming(intent(f, armed)).unwrap();
    f.drain();
    r
}
fn armed(r: &ArmingReport) -> Vec<String> {
    r.configuration
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .armed()
        .unwrap()
        .into_iter()
        .map(|d| d.key)
        .collect()
}
fn stop(f: &Fixture) {
    let receipts = f.commands.stop_recording(RecordingStop::All).unwrap();
    for r in receipts.receipts() {
        assert!(r
            .wait_timeout(Duration::from_secs(2))
            .unwrap()
            .is_complete());
    }
}
fn samples(f: &Fixture, blocks: &[Vec<f32>]) {
    for c in f.capture.borrow().iter() {
        if c.accepts(blocks[0].len(), 1, 48000) {
            c.ingest_with(blocks, blocks[0].len(), |sender, track, packet| {
                // Offline arming/WAV fixture: a contended nonblocking send
                // accepts no packet. Retry only that unaccepted packet before
                // ingress fault handling; never replay an accepted sample.
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    match sender.try_audio(track, packet.clone()) {
                        Err(opname::WriterSendError::Busy) if Instant::now() < deadline => {
                            std::thread::yield_now();
                        }
                        result => return result,
                    }
                }
            });
        }
    }
}

#[test]
fn exclusive_arming_attaches_addition_before_stopping_captured_removal() {
    let mut f = fixture(&["rec-1", "rec-4"]);
    let old = f.start();
    let mapping = until(&old, |_| true).tracks;
    samples(&f, &[vec![0.1], vec![0.2], vec![0.3], vec![0.4]]);
    let initial = until(&old, |_| true);
    assert!(
        initial.failure.is_none(),
        "initial sample receipt: {initial:?}"
    );
    let request = arm(&mut f, &["rec-2", "rec-4"]);
    assert_eq!(
        request.try_report().unwrap().progress,
        ArmingProgress::Adding
    );
    assert!(!until(&old, |_| true).stop_requested);
    assert_eq!(until(&old, |_| true).track_stops, 0);
    tick(&mut f);
    let opening = request.try_report().unwrap();
    assert_eq!(opening.additions.len(), 1);
    assert_eq!(until(&old, |_| true).track_stops, 0);
    let result = decide(&mut f, &request);
    assert_eq!(
        result.progress,
        ArmingProgress::Completed(ArmingOutcome::AppliedLocally)
    );
    assert_eq!(result.removed, vec![mapping[0].id]);
    assert!(result.withheld.is_empty());
    assert_eq!(armed(&result), ["rec-2", "rec-4"]);
    let added = &result.additions[0].receipt;
    assert!(until(added, |_| true).started_once);
    assert!(!until(&old, |_| true).stop_requested);
    assert!(Arc::ptr_eq(&mapping, &until(&old, |_| true).tracks));
    f.pump();
    samples(&f, &[vec![-0.1], vec![-0.2], vec![-0.3], vec![-0.4]]);
    let continuing = until(&old, |_| true);
    assert!(
        continuing.failure.is_none(),
        "continued sample receipt: {continuing:?}"
    );
    stop(&f);
    let old = old.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(old.is_success(), "unchanged take receipt: {old:?}");
    let path = old.writer.unwrap().directory.unwrap();
    assert_wav(&path.join("Rec 1.wav"), 1, 48000, &[0.1]);
    assert_wav(&path.join("Rec 4.wav"), 1, 48000, &[0.4, -0.4]);
    let new = added.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(new.is_success());
    assert_wav(
        &new.writer.unwrap().directory.unwrap().join("Rec 2.wav"),
        1,
        48000,
        &[-0.2],
    );
}

#[test]
fn elapsed_arming_addition_owns_new_clock_while_unchanged_track_keeps_original_clock() {
    let mut f = fixture(&["rec-1", "rec-4"]);
    let old = f.start();
    let old_start = *old.0.first_attachment.get().unwrap();
    let request = arm(&mut f, &["rec-2", "rec-4"]);
    let result = decide(&mut f, &request);
    assert!(result.is_success());
    let added = &result.additions[0].receipt;
    let added_start = *added.0.first_attachment.get().unwrap();
    assert!(added_start >= old_start);
    let at = added_start + Duration::from_secs(20);
    let old_report = super::elapsed::report_at(&old, at);
    let added_report = super::elapsed::report_at(added, at);
    let elapsed = old_report.elapsed.unwrap();
    assert_eq!(elapsed.duration, at.duration_since(old_start));
    assert_eq!(
        elapsed.running_tracks,
        vec![
            old_report
                .tracks
                .iter()
                .find(|t| t.key == "rec-4")
                .unwrap()
                .id
        ]
    );
    assert_eq!(
        added_report.elapsed.unwrap().duration,
        Duration::from_secs(20)
    );
    f.pump();
    assert_eq!(*old.0.first_attachment.get().unwrap(), old_start);
    assert_eq!(*added.0.first_attachment.get().unwrap(), added_start);
    stop(&f);
    assert!(old.try_report().unwrap().elapsed.is_none());
    assert!(added.try_report().unwrap().elapsed.is_none());
}

#[test]
fn failed_first_addition_withholds_removals_and_leaves_old_recording_and_flags_alive() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2"]);
    tick_with(&mut f, |_, _, _, _| {
        Err(std::io::Error::other("synthetic admission failure"))
    });
    let result = decide(&mut f, &request);
    assert_eq!(
        result.progress,
        ArmingProgress::Completed(ArmingOutcome::Rejected)
    );
    assert!(matches!(result.failure, Some(RecordingError::Writer(_))));
    assert!(result.removed.is_empty());
    assert_eq!(result.withheld.len(), 1);
    assert_eq!(armed(&result), ["rec-1"]);
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn partial_addition_failure_preserves_old_and_started_takes_and_reports_actual_union() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2", "rec-3"]);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        tick_with(&mut f, |root, tracks, rate, owner| {
            if tracks[0].naam == "Rec 3" {
                return Err(std::io::Error::other("synthetic second addition failure"));
            }
            opname::tracked_writer(root, tracks, rate, owner)
        });
        if request.try_report().unwrap().is_decided() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let result = request.try_report().unwrap();
    assert_eq!(
        result.progress,
        ArmingProgress::Completed(ArmingOutcome::PartiallyApplied)
    );
    assert_eq!(armed(&result), ["rec-1", "rec-2"]);
    assert_eq!(result.additions.len(), 2);
    assert_eq!(result.withheld.len(), 1);
    assert!(result.removed.is_empty());
    assert!(
        result.additions[0]
            .receipt
            .try_report()
            .unwrap()
            .started_once
    );
    assert!(
        !result.additions[1]
            .receipt
            .try_report()
            .unwrap()
            .started_once
    );
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn unchanged_arm_set_is_local_noop_and_never_restarts_existing_takes() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let before = f.commands.config_snapshot().unwrap();
    let id = old.try_report().unwrap().tracks[0].id;
    let request = arm(&mut f, &["rec-1"]);
    let result = decide(&mut f, &request);
    assert_eq!(
        result.progress,
        ArmingProgress::Completed(ArmingOutcome::NoOpLocally)
    );
    assert!(result.additions.is_empty() && result.removed.is_empty());
    assert_eq!(result.configuration.revision, before.revision);
    assert_eq!(old.try_report().unwrap().tracks[0].id, id);
    stop(&f);
}

#[test]
fn empty_arm_set_disarms_all_without_claiming_writer_join_completion() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &[]);
    let result = decide(&mut f, &request);
    assert!(result.is_success());
    assert!(armed(&result).is_empty());
    assert_eq!(result.removed.len(), 1);
    assert!(!result.configuration.recording);
    assert!(old.try_report().unwrap().stop_requested);
    assert!(old
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
}

#[test]
fn stale_config_membership_and_copied_origins_fail_before_queue_admission() {
    let f = fixture(&["rec-1"]);
    let old = f.start();
    let mut stale = intent(&f, &["rec-2"]);
    stale.expected_config.revision -= 1;
    assert!(matches!(
        f.commands.enqueue_arming(stale),
        Err(RecordingError::StaleConfig)
    ));
    let mut stale = intent(&f, &["rec-2"]);
    stale.expected_recording.revision -= 1;
    assert!(matches!(
        f.commands.enqueue_arming(stale),
        Err(RecordingError::StaleRecording)
    ));
    let mut stale = intent(&f, &["rec-2"]);
    stale.origin.value = Arc::new((*stale.origin.value).clone());
    assert!(matches!(
        f.commands.enqueue_arming(stale),
        Err(RecordingError::OriginUnavailable)
    ));
    assert!(f.commands.0.lock().unwrap().pending.is_empty());
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn stop_all_cancels_queued_operation_without_starting_any_additions_even_when_fifo_is_full() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = f.commands.enqueue_arming(intent(&f, &["rec-2"])).unwrap();
    while f
        .commands
        .legacy(Opdracht::ZetMute {
            knoop: 900,
            mute: true,
        })
        .is_ok()
    {}
    assert_eq!(f.commands.0.lock().unwrap().pending.len(), NORMAL_CAPACITY);
    let all = f.commands.stop_recording(RecordingStop::All).unwrap();
    assert_eq!(all.receipts().len(), 1);
    let result = request.wait_timeout(Duration::from_millis(1)).unwrap();
    assert!(!result.is_success());
    assert_eq!(result.failure, Some(RecordingError::Cancelled));
    assert!(result.additions.is_empty());
    assert_eq!(
        f.commands.0.lock().unwrap().pending.len(),
        NORMAL_CAPACITY - 1
    );
    f.drain();
    assert!(old
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
    tick(&mut f);
    assert_eq!(f.commands.recording_snapshot().unwrap().takes.len(), 0);
    assert!(old
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
}

#[test]
fn saturation_rejects_new_arming_without_altering_active_tracks() {
    let f = fixture(&["rec-1"]);
    let old = f.start();
    while f
        .commands
        .legacy(Opdracht::ZetMute {
            knoop: 900,
            mute: true,
        })
        .is_ok()
    {}
    assert!(matches!(
        f.commands.enqueue_arming(intent(&f, &["rec-2"])),
        Err(RecordingError::QueueFull)
    ));
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn stop_during_opening_fences_late_attachment_and_remaining_additions() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2", "rec-3"]);
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut gate = Some(gate);
    tick_with(&mut f, |root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            if let Some(gate) = gate.take() {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(2)).unwrap();
            }
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        })
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(request.wait_timeout(Duration::from_millis(1)).is_err());
    let all = f.commands.stop_recording(RecordingStop::All).unwrap();
    assert_eq!(all.receipts().len(), 2);
    let cancelled = request.try_report().unwrap();
    assert_eq!(cancelled.failure, Some(RecordingError::Cancelled));
    assert_eq!(cancelled.additions.len(), 1);
    release.send(()).unwrap();
    for child in &cancelled.additions {
        let r = child.receipt.wait_timeout(Duration::from_secs(2)).unwrap();
        assert!(!r.started_once);
    }
    tick_with(&mut f, |_, _, _, _| {
        panic!("stop fence must prevent the next addition")
    });
    assert!(f.capture.borrow().is_empty());
    assert!(old
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
}

#[test]
fn stopping_one_original_track_cancels_pending_addition_without_stopping_other_originals() {
    let mut f = fixture(&["rec-1", "rec-4"]);
    let old = f.start();
    let tracks = old.try_report().unwrap().tracks;
    let request = arm(&mut f, &["rec-2", "rec-4"]);
    tick(&mut f);
    f.commands
        .stop_recording(RecordingStop::Track(tracks[0].id))
        .unwrap();
    let result = request.try_report().unwrap();
    assert_eq!(result.failure, Some(RecordingError::Cancelled));
    assert!(result.removed.is_empty());
    f.pump();
    assert!(!old.try_report().unwrap().stop_requested);
    assert_eq!(old.try_report().unwrap().track_stops, 1);
    stop(&f);
}

#[test]
fn repeated_rearming_uses_new_track_ids_and_retired_stops_do_not_cancel_new_operations() {
    let mut f = fixture(&["rec-1", "rec-4"]);
    let old = f.start();
    let retired = old.try_report().unwrap().tracks[0].id;
    let request = arm(&mut f, &["rec-4"]);
    assert!(decide(&mut f, &request).is_success());
    let request = arm(&mut f, &["rec-1", "rec-4"]);
    f.commands
        .stop_recording(RecordingStop::Track(retired))
        .unwrap();
    assert!(!request.try_report().unwrap().is_decided());
    let result = decide(&mut f, &request);
    assert!(result.is_success());
    assert_ne!(
        result.additions[0].receipt.try_report().unwrap().tracks[0].id,
        retired
    );
    stop(&f);
}

#[test]
fn already_captured_unarmed_bus_is_reused_without_a_new_take_or_false_start() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let snapshot = f.commands.recording_snapshot().unwrap();
    let RecordingBusStartResult::Admitted(existing) = f
        .commands
        .enqueue_recording_bus(RecordingBusStart {
            start: RecordingStart {
                expected_config: snapshot.configuration.revision,
                ..f.intent()
            },
            expected_recording: snapshot.revision,
            bus: "rec-2".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    f.drain();
    until(&existing, |r| {
        f.pump();
        r.started_once
    });
    let request = arm(&mut f, &["rec-1", "rec-2"]);
    let result = decide(&mut f, &request);
    assert!(result.is_success());
    assert_eq!(result.additions.len(), 1);
    assert!(!result.additions[0].new_take);
    assert!(Arc::ptr_eq(&existing.0, &result.additions[0].receipt.0));
    assert_eq!(f.commands.recording_snapshot().unwrap().takes.len(), 2);
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn invalid_or_overlapping_requests_do_not_consume_another_operation_slot() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    for keys in [vec!["unknown"], vec!["rec-2", "rec-2"], vec!["rec-2"; 65]] {
        assert!(matches!(
            f.commands.enqueue_arming(intent(&f, &keys)),
            Err(RecordingError::InvalidInput)
        ));
    }
    let request = arm(&mut f, &["rec-2"]);
    assert!(matches!(
        f.commands.enqueue_arming(intent(&f, &["rec-3"])),
        Err(RecordingError::Busy)
    ));
    let snapshot = f.commands.recording_snapshot().unwrap();
    assert!(Arc::ptr_eq(&snapshot.arming.unwrap().0, &request.0));
    assert_eq!(snapshot.revision, request.try_report().unwrap().recording);
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
    assert!(request.try_report().unwrap().is_decided());
}

#[test]
fn replacement_after_claim_rejects_before_opening_and_preserves_unaffected_original() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2"]);
    f.commands.object_removed(902);
    f.commands.object_added(902);
    let graph = (*f.commands.current().value).clone();
    f.commands.publish(graph);
    tick_with(&mut f, |_, _, _, _| {
        panic!("replacement must reject before writer admission")
    });
    let result = request.try_report().unwrap();
    assert_eq!(result.failure, Some(RecordingError::InvalidOrigin));
    assert!(result.additions.is_empty());
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn disconnect_cancels_opening_and_never_replays_additions_on_a_new_epoch() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2", "rec-3"]);
    tick(&mut f);
    f.commands.publish(Momentopname::default());
    let report = request.try_report().unwrap();
    assert_eq!(report.failure, Some(RecordingError::Disconnected));
    assert_eq!(report.additions.len(), 1);
    tick_with(&mut f, |_, _, _, _| {
        panic!("disconnect must cancel further additions")
    });
    for addition in &report.additions {
        assert!(addition
            .receipt
            .wait_timeout(Duration::from_secs(2))
            .unwrap()
            .is_complete());
    }
    assert_eq!(
        old.wait_timeout(Duration::from_secs(2)).unwrap().failure,
        Some(RecordingError::Disconnected)
    );
    f.commands.publish(Momentopname {
        verbonden: true,
        ..Default::default()
    });
    tick_with(&mut f, |_, _, _, _| {
        panic!("reconnect must not replay an arming operation")
    });
    assert!(f.commands.recording_snapshot().unwrap().arming.is_none());
}

#[test]
fn shutdown_cancels_queued_arming_and_common_join_retains_all_writer_obligations() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2"]);
    tick(&mut f);
    f.commands.stop();
    let report = request.try_report().unwrap();
    assert_eq!(report.failure, Some(RecordingError::Stopped));
    tick_with(&mut f, |_, _, _, _| {
        panic!("shutdown must fence all future additions")
    });
    let shutdown = f.owner.finish();
    assert!(shutdown.is_success());
    assert!(old
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_complete());
    for addition in &report.additions {
        assert!(addition
            .receipt
            .wait_timeout(Duration::from_secs(2))
            .unwrap()
            .is_complete());
    }
}

#[test]
fn concurrent_stop_inside_writer_admission_is_retained_and_prevents_attachment() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2", "rec-3"]);
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    std::thread::scope(|scope| {
        let commands = &f.commands;
        let stop = scope.spawn(move || {
            waiting.recv_timeout(Duration::from_secs(2)).unwrap();
            let stopped = commands.stop_recording(RecordingStop::All).unwrap();
            release.send(()).unwrap();
            stopped
        });
        f.pump();
        f.commands.drive_arming(
            &mut f.target,
            &f.owner.completion,
            false,
            |root, tracks, rate, owner| {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(2)).unwrap();
                opname::tracked_writer(root, tracks, rate, owner)
            },
        );
        let stopped = stop.join().unwrap();
        assert_eq!(stopped.receipts().len(), 2);
        for receipt in stopped.receipts() {
            assert!(receipt
                .wait_timeout(Duration::from_secs(2))
                .unwrap()
                .is_complete());
        }
    });
    let result = request.try_report().unwrap();
    assert_eq!(result.failure, Some(RecordingError::Cancelled));
    assert!(
        !result.additions[0]
            .receipt
            .try_report()
            .unwrap()
            .started_once
    );
    tick_with(&mut f, |_, _, _, _| {
        panic!("cancelled operation must not admit remaining child")
    });
    assert!(f.capture.borrow().is_empty());
    assert!(old.try_report().unwrap().is_success());
}

struct WrongOptions<'a>(&'a mut Target);
impl ConfigTarget for WrongOptions<'_> {
    fn readback(&self) -> Option<bus_config::BusReadback> {
        self.0.readback()
    }
    fn recording(&self) -> bool {
        false
    }
    fn routed(&self) -> bool {
        false
    }
    fn resize(&mut self, _: usize) -> bus_config::ChannelResizeResult {
        panic!("arming must not resize")
    }
    fn gains(&mut self, _: &[f32]) {
        panic!("arming must not change gains")
    }
    fn options(&mut self, _: Opties) {} // Inject an ignored local apply.
    fn options_match(&self, options: &Opties) -> bool {
        self.0.options_match(options)
    }
}
#[test]
fn local_options_readback_failure_never_stops_removals_or_fakes_configuration() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2"]);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        f.pump();
        f.commands.drive_arming(
            &mut WrongOptions(&mut f.target),
            &f.owner.completion,
            false,
            opname::tracked_writer,
        );
        if request.try_report().unwrap().is_decided() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let result = request.try_report().unwrap();
    assert_eq!(
        result.progress,
        ArmingProgress::Completed(ArmingOutcome::PartiallyApplied)
    );
    assert_eq!(
        result.failure,
        Some(RecordingError::ConfigApply(
            bus_config::ConfigError::ReadbackMismatch
        ))
    );
    assert!(result.configuration.configuration.is_none());
    assert!(result.removed.is_empty());
    assert_eq!(result.withheld.len(), 1);
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn late_stop_of_an_already_failed_take_does_not_fence_an_unrelated_arming_operation() {
    let mut f = fixture(&["rec-1"]);
    let failed = f.start();
    let retired = failed.try_report().unwrap().tracks[0].id;
    failed.0.ingress_fault(1);
    assert!(!failed
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
    f.pump();
    let snapshot = f.commands.recording_snapshot().unwrap();
    let RecordingBusStartResult::Admitted(independent) = f
        .commands
        .enqueue_recording_bus(RecordingBusStart {
            start: RecordingStart {
                expected_config: snapshot.configuration.revision,
                ..f.intent()
            },
            expected_recording: snapshot.revision,
            bus: "rec-4".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    f.drain();
    until(&independent, |r| {
        f.pump();
        r.started_once
    });
    let request = arm(&mut f, &["rec-2"]);
    f.commands
        .stop_recording(RecordingStop::Track(retired))
        .unwrap();
    assert!(!request.try_report().unwrap().is_decided());
    assert!(decide(&mut f, &request).is_success());
    assert!(!independent.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn ordinary_graph_revisions_do_not_rebind_or_invalidate_retained_arming_witnesses() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let original = intent(&f, &["rec-2"]);
    let source = original.origin.stamp;
    let request = f.commands.enqueue_arming(original).unwrap();
    f.drain();
    for _ in 0..64 {
        let graph = (*f.commands.current().value).clone();
        f.commands.publish(graph);
    }
    let result = decide(&mut f, &request);
    assert!(result.is_success());
    assert_eq!(result.source, source);
    assert_eq!(
        result.additions[0].receipt.try_report().unwrap().source,
        source
    );
    assert!(old.try_report().unwrap().stop_requested);
    stop(&f);
}

#[test]
fn all_additions_must_attach_and_writer_readiness_alone_never_releases_removals() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2", "rec-3"]);
    tick(&mut f);
    let first = request.try_report().unwrap().additions[0].receipt.clone();
    until(&first, |r| {
        f.pump();
        r.started_once
    });
    assert!(!old.try_report().unwrap().stop_requested);
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut gate = Some(gate);
    tick_with(&mut f, |root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            if let Some(gate) = gate.take() {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(2)).unwrap();
            }
            hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
        })
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!old.try_report().unwrap().stop_requested);
    let second = request.try_report().unwrap().additions[1].receipt.clone();
    assert!(first.same_request(&first.clone()));
    assert!(second.same_request(&second.clone()));
    assert!(
        !first.same_request(&second),
        "arming siblings need distinct receipt identity"
    );
    let first_source = until(&first, |_| true);
    let second_source = until(&second, |_| true);
    assert_eq!(first_source.ticket, second_source.ticket);
    assert_eq!(first_source.source, second_source.source);
    assert_eq!(first_source.config, second_source.config);
    release.send(()).unwrap();
    let ready = until(&second, |r| r.writer.as_ref().is_some_and(|w| w.was_ready));
    assert!(!ready.started_once);
    assert!(!old.try_report().unwrap().stop_requested);
    let result = decide(&mut f, &request);
    assert!(result.is_success());
    assert!(second.try_report().unwrap().started_once);
    // Stopping makes the writer finalize, which can briefly make status busy.
    // Assert the first readable result, without pumping or waiting for true.
    assert!(until(&old, |_| true).stop_requested);
    let a = first.try_report().unwrap().writer.unwrap().take;
    let b = second.try_report().unwrap().writer.unwrap().take;
    assert_ne!(a, b);
    stop(&f);
}

#[test]
fn addition_ingress_failure_is_partial_and_never_advances_to_later_additions_or_removals() {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    let request = arm(&mut f, &["rec-2", "rec-3"]);
    tick(&mut f);
    let added = request.try_report().unwrap().additions[0].receipt.clone();
    until(&added, |r| {
        f.pump();
        r.started_once
    });
    added.0.ingress_fault(1);
    f.pump();
    f.commands
        .drive_arming(&mut f.target, &f.owner.completion, false, |_, _, _, _| {
            panic!("failed addition must not advance")
        });
    let result = request.try_report().unwrap();
    assert_eq!(
        result.progress,
        ArmingProgress::Completed(ArmingOutcome::PartiallyApplied)
    );
    assert_eq!(
        result.failure,
        Some(RecordingError::Ingress(opname::WriterSendError::Full))
    );
    assert_eq!(result.additions.len(), 1);
    assert_eq!(armed(&result), ["rec-1"]);
    assert!(result.removed.is_empty());
    assert!(!old.try_report().unwrap().stop_requested);
    stop(&f);
}

// A real synthetic WAV writer fails only when the final local accessor releases
// it. The accessor never calls Commands, and the writer/reaper run independently.
struct LateWriteFault {
    file: std::fs::File,
    fail: Arc<std::sync::atomic::AtomicBool>,
}
impl std::io::Write for LateWriteFault {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.fail.load(Ordering::Acquire) {
            return Err(std::io::Error::other("synthetic late WAV write failure"));
        }
        std::io::Write::write(&mut self.file, bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.file)
    }
}
impl std::io::Seek for LateWriteFault {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        std::io::Seek::seek(&mut self.file, from)
    }
}
#[derive(Clone, Copy, PartialEq)]
enum CommitFaultPoint {
    BeforeOptions,
    InOptions,
    FinalReadback,
}
struct CommitFaultTarget<'a> {
    inner: &'a mut Target,
    point: CommitFaultPoint,
    applied: std::cell::Cell<bool>,
    hook: RefCell<Option<Box<dyn FnOnce()>>>,
    option_calls: std::cell::Cell<usize>,
    reject_repair: bool,
    during_repair: Option<Box<dyn FnOnce()>>,
}
impl CommitFaultTarget<'_> {
    fn fire(&self) {
        if let Some(hook) = self.hook.borrow_mut().take() {
            hook();
        }
    }
}
impl ConfigTarget for CommitFaultTarget<'_> {
    fn readback(&self) -> Option<bus_config::BusReadback> {
        if self.point == CommitFaultPoint::BeforeOptions
            || (self.point == CommitFaultPoint::FinalReadback && self.applied.get())
        {
            self.fire();
        }
        self.inner.readback()
    }
    fn recording(&self) -> bool {
        false
    }
    fn routed(&self) -> bool {
        false
    }
    fn resize(&mut self, _: usize) -> bus_config::ChannelResizeResult {
        panic!("arming cannot resize")
    }
    fn gains(&mut self, _: &[f32]) {
        panic!("arming cannot change gain")
    }
    fn options(&mut self, options: pecan_core::graph::Opties) {
        self.inner.options(options);
        self.option_calls.set(self.option_calls.get() + 1);
        self.applied.set(true);
        if self.point == CommitFaultPoint::InOptions {
            self.fire();
        }
        if self.option_calls.get() == 2 {
            if let Some(hook) = self.during_repair.take() {
                hook();
            }
        }
    }
    fn options_match(&self, options: &pecan_core::graph::Opties) -> bool {
        !(self.reject_repair && self.option_calls.get() >= 2) && self.inner.options_match(options)
    }
}

fn late_writer_failure_at_commit(point: CommitFaultPoint, another_success: bool) {
    late_writer_failure_with_repair_fault(point, another_success, false, false);
}
fn late_writer_failure_with_repair_fault(
    point: CommitFaultPoint,
    another_success: bool,
    reject_repair: bool,
    lose_success: bool,
) {
    let mut f = fixture(&["rec-1"]);
    let old = f.start();
    samples(&f, &[vec![0.1], vec![0.2], vec![0.3], vec![0.4]]);
    let desired: &[&str] = if another_success {
        &["rec-2", "rec-3"]
    } else {
        &["rec-2"]
    };
    let request = arm(&mut f, desired);
    let failing = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fault = failing.clone();
    tick_with(&mut f, |root, tracks, rate, owner| {
        opname::tracked_writer_with(root, tracks, rate, owner, move |path, spec| {
            let file = std::fs::File::create(path)?;
            hound::WavWriter::new(
                LateWriteFault {
                    file,
                    fail: fault.clone(),
                },
                spec,
            )
            .map_err(std::io::Error::other)
        })
    });
    let addition = request.try_report().unwrap().additions[0].receipt.clone();
    until(&addition, |r| {
        f.pump();
        r.started_once
    });
    let successful = if another_success {
        tick(&mut f);
        let r = request.try_report().unwrap().additions[1].receipt.clone();
        until(&r, |r| {
            f.pump();
            r.started_once
        });
        Some(r)
    } else {
        None
    };
    assert!(addition.0.live());
    assert!(!old.try_report().unwrap().stop_requested);
    let track = addition.try_report().unwrap().tracks[0].id;
    let observed = addition.clone();
    let mut target = CommitFaultTarget {
        inner: &mut f.target,
        point,
        applied: std::cell::Cell::new(false),
        option_calls: std::cell::Cell::new(0),
        reject_repair,
        during_repair: if lose_success {
            let receipt = successful.as_ref().unwrap().clone();
            Some(Box::new(move || receipt.0.ingress_fault(5)))
        } else {
            None
        },
        hook: RefCell::new(Some(Box::new(move || {
            let writer = &observed.0.installed.get().unwrap().writer;
            failing.store(true, Ordering::Release);
            writer.sender.try_audio(track, vec![0.5]).unwrap();
            // Test-only deterministic fault gate, bounded independently of Commands.
            let end = Instant::now() + Duration::from_secs(2);
            while !writer.receipt.signals.closed.load(Ordering::Acquire) {
                assert!(Instant::now() < end, "WAV failure gate timed out");
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!observed.0.live());
            assert!(observed.try_report().unwrap().failure.is_some());
        }))),
    };
    f.commands
        .drive_arming(&mut target, &f.owner.completion, false, |_, _, _, _| {
            panic!("no more additions")
        });
    let result = request.try_report().unwrap();
    assert_eq!(
        result.progress,
        ArmingProgress::Completed(ArmingOutcome::PartiallyApplied)
    );
    if reject_repair {
        assert_eq!(
            result.failure,
            Some(RecordingError::ConfigApply(
                bus_config::ConfigError::ReadbackMismatch
            ))
        );
    } else {
        assert!(matches!(result.failure, Some(RecordingError::Writer(_))));
    }
    assert!(result.removed.is_empty());
    assert_eq!(
        result.withheld,
        vec![old.try_report().unwrap().tracks[0].id]
    );
    assert!(!old.try_report().unwrap().stop_requested);
    assert_eq!(old.try_report().unwrap().track_stops, 0);
    if reject_repair || lose_success {
        assert!(result.configuration.configuration.is_none());
        assert!(result.configuration.transport.is_none());
    } else {
        assert_eq!(
            armed(&result),
            if another_success {
                vec!["rec-1", "rec-3"]
            } else {
                vec!["rec-1"]
            }
        );
    }
    if let Some(successful) = &successful {
        assert_eq!(successful.0.live(), !lose_success);
    }
    samples(&f, &[vec![-0.1], vec![-0.2], vec![-0.3], vec![-0.4]]);
    stop(&f);
    let completed = old.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(completed.is_success());
    assert_wav(
        &completed
            .writer
            .unwrap()
            .directory
            .unwrap()
            .join("Rec 1.wav"),
        1,
        48000,
        &[0.1, -0.1],
    );
    assert!(addition
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .failure
        .is_some());
}

#[test]
fn writer_failure_in_final_precheck_withholds_removals_and_reconciles_old_armed_flags() {
    late_writer_failure_at_commit(CommitFaultPoint::BeforeOptions, false);
}
#[test]
fn writer_failure_in_options_withholds_removals_and_reconciles_old_armed_flags() {
    late_writer_failure_at_commit(CommitFaultPoint::InOptions, false);
}
#[test]
fn writer_failure_in_final_readback_withholds_removals_and_reconciles_old_armed_flags() {
    late_writer_failure_at_commit(CommitFaultPoint::FinalReadback, false);
}
#[test]
fn late_failure_preserves_unchanged_and_other_successfully_added_tracks() {
    late_writer_failure_at_commit(CommitFaultPoint::InOptions, true);
}

#[test]
fn failed_reconciliation_invalidates_logical_metadata_and_preserves_old_recording() {
    late_writer_failure_with_repair_fault(CommitFaultPoint::InOptions, false, true, false);
}

#[test]
fn another_failure_during_reconciliation_cannot_publish_stale_success_flags() {
    late_writer_failure_with_repair_fault(CommitFaultPoint::InOptions, true, false, true);
}
