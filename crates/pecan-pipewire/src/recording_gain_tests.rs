//! Native level-only configuration with real temporary writers, no server.
use super::*;
use bus_config::*;

#[test]
fn caller_origin_rejects_same_id_replacement_before_admission() {
    for id in [900, 901] {
        let mut f = Fixture::new(true);
        let original = f.commands.history_graph(&f.commands.current()).unwrap();
        let edit = lower(&f);
        let revision = edit.expected;
        f.commands.object_removed(id);
        f.commands.object_added(id);
        assert_eq!(
            f.commands
                .enqueue_gains(&original, edit.clone())
                .unwrap_err(),
            ConfigError::StaleRevision,
            "registry replacement not published yet"
        );
        f.commands.publish((*f.commands.current().value).clone());
        assert_eq!(f.commands.config_snapshot().unwrap().revision, revision);
        assert_eq!(
            original.graph().stamp.epoch,
            f.commands.current().stamp.epoch
        );
        let result = f.commands.enqueue_gains(&original, edit.clone());
        assert_eq!(result.unwrap_err(), ConfigError::StaleRevision);
        assert_eq!(f.target.gains, [1., 1.]);
        assert!(f.commands.0.lock().unwrap().pending.is_empty());
        let fresh = f.commands.history_graph(&f.commands.current()).unwrap();
        let receipt = f.commands.enqueue_gains(&fresh, edit).unwrap();
        assert_eq!(
            run(&mut f, &receipt, false, false, false, None).outcome,
            LocalConfigOutcome::Applied
        );
    }
}

#[test]
fn caller_origin_rejects_serial_or_channel_mapping_changes_without_rebinding() {
    for serial in [false, true] {
        let mut f = Fixture::new(true);
        let original = f.commands.history_graph(&f.commands.current()).unwrap();
        let edit = lower(&f);
        let mut graph = (*f.commands.current().value).clone();
        if serial {
            graph.knopen[0].serial = Some(9910);
        } else {
            graph.poorten[0].naam = "rec_2".into();
            graph.poorten[1].naam = "rec_1".into();
        }
        f.commands.publish(graph);
        assert_eq!(
            f.commands.config_snapshot().unwrap().revision,
            edit.expected
        );
        assert_eq!(
            f.commands
                .enqueue_gains(&original, edit.clone())
                .unwrap_err(),
            ConfigError::StaleRevision
        );
        let fresh = f.commands.history_graph(&f.commands.current()).unwrap();
        let receipt = f.commands.enqueue_gains(&fresh, edit).unwrap();
        assert_eq!(
            run(&mut f, &receipt, false, false, false, None).outcome,
            LocalConfigOutcome::Applied
        );
    }
}

#[test]
fn caller_origin_authentication_survives_revision_cache_eviction_but_rejects_foreign_authority() {
    use pecan_core::history_identity::HistoryGraph;
    let mut f = Fixture::new(true);
    let other = Fixture::new(true);
    let original = f.commands.history_graph(&f.commands.current()).unwrap();
    let foreign = other
        .commands
        .history_graph(&other.commands.current())
        .unwrap();
    let forged = HistoryGraph::from_registry(
        original.graph().clone(),
        Arc::new(()),
        f.commands.registry_generations(),
    );
    for invalid in [&foreign, &forged] {
        assert_eq!(
            f.commands.enqueue_gains(invalid, lower(&f)).unwrap_err(),
            ConfigError::StaleRevision
        );
    }
    let copy = pecan_core::backend::Stamped {
        stamp: original.graph().stamp,
        value: Arc::new((*original.graph().value).clone()),
    };
    assert!(
        f.commands.history_graph(&copy).is_err(),
        "copied DTO is not an authenticated witness"
    );
    for _ in 0..pecan_core::native_command::NATIVE_SOURCE_HISTORY + 2 {
        let mut graph = (*f.commands.current().value).clone();
        graph.standaard_sink = Some("synthetic-unrelated-destination".into());
        f.commands.publish(graph);
    }
    assert!(
        f.commands.history_graph(original.graph()).is_err(),
        "original cache entry was evicted"
    );
    let receipt = f.commands.enqueue_gains(&original, lower(&f)).unwrap();
    assert_eq!(
        run(&mut f, &receipt, false, false, false, None).outcome,
        LocalConfigOutcome::Applied
    );
}

#[test]
fn caller_origin_cannot_cross_reconnect_even_with_fresh_configuration_revision() {
    let mut f = Fixture::new(true);
    let original = f.commands.history_graph(&f.commands.current()).unwrap();
    let graph = (*f.commands.current().value).clone();
    let configuration = f.commands.config_snapshot().unwrap().configuration.unwrap();
    f.commands.publish(Momentopname::default());
    for id in 900..=902 {
        f.commands.object_added(id);
    }
    f.commands.publish(graph);
    f.commands.refresh_config(f.target.readback(), false, false);
    let receipt = f
        .commands
        .enqueue_config(IdleConfigIntent {
            ticket: 91,
            expected: f.commands.config_snapshot().unwrap().revision,
            configuration,
        })
        .unwrap();
    f.commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |work| f.commands.apply_config(work, &mut f.target),
    );
    assert!(matches!(
        receipt.try_report().unwrap().progress,
        ConfigProgress::Completed(_)
    ));
    let edit = lower(&f);
    assert_ne!(edit.expected.epoch, original.graph().stamp.epoch);
    assert_eq!(
        f.commands.enqueue_gains(&original, edit).unwrap_err(),
        ConfigError::StaleRevision
    );
    let edit = lower(&f);
    assert_eq!(apply(&mut f, edit).outcome, LocalConfigOutcome::Applied);
}

// Existing lifecycle tests use a newly authenticated origin. Stale-origin
// regressions explicitly retain and pass the original witness instead.
fn enqueue(commands: &Commands, intent: IdleConfigIntent) -> Result<ConfigReceipt, ConfigError> {
    let origin = commands.history_graph(&commands.current()).unwrap();
    commands.enqueue_gains(&origin, intent)
}

struct Levels<'a> {
    target: &'a mut Target,
    commands: &'a Commands,
    legacy: bool,
    ignore: bool,
    partial: bool,
    after_gain: Option<Box<dyn FnOnce() + 'a>>,
}
impl ConfigTarget for Levels<'_> {
    fn readback(&self) -> Option<BusReadback> {
        self.target.readback()
    }
    fn recording(&self) -> bool {
        self.commands.recording_live()
    }
    fn config_locked(&self) -> bool {
        true
    }
    fn gains_locked(&self) -> bool {
        self.legacy
    }
    fn routed(&self) -> bool {
        true
    }
    fn resize(&mut self, _: usize) -> ChannelResizeResult {
        panic!("gain edit resized ports")
    }
    fn options(&mut self, _: Opties) {
        panic!("gain edit replaced recording options")
    }
    fn options_match(&self, options: &Opties) -> bool {
        self.target.options_match(options)
    }
    fn gains(&mut self, gains: &[f32]) {
        if !self.ignore {
            if self.partial {
                self.target.gains[0] = gains[0];
            } else {
                self.target.gains(gains);
            }
        }
        if let Some(callback) = self.after_gain.take() {
            callback();
        }
    }
}
fn intent(f: &Fixture, edit: impl FnOnce(&mut IdleBusConfig)) -> IdleConfigIntent {
    let snapshot = f.commands.config_snapshot().unwrap();
    let mut configuration = snapshot.configuration.unwrap();
    edit(&mut configuration);
    IdleConfigIntent {
        ticket: 90,
        expected: snapshot.revision,
        configuration,
    }
}
fn lower(f: &Fixture) -> IdleConfigIntent {
    intent(f, |c| c.buses.states.get_mut("rec-1").unwrap().db = -6.)
}
fn run(
    f: &mut Fixture,
    receipt: &ConfigReceipt,
    ignore: bool,
    partial: bool,
    legacy: bool,
    after_gain: Option<Box<dyn FnOnce() + '_>>,
) -> ConfigCompletion {
    let mut target = Levels {
        target: &mut f.target,
        commands: &f.commands,
        ignore,
        partial,
        legacy,
        after_gain,
    };
    f.commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |w| f.commands.apply_config(w, &mut target),
    );
    match receipt.try_report().unwrap().progress {
        ConfigProgress::Completed(result) => {
            assert!(result.resize.is_none());
            result
        }
        other => panic!("not completed: {other:?}"),
    }
}
fn apply(f: &mut Fixture, intent: IdleConfigIntent) -> ConfigCompletion {
    let receipt = enqueue(&f.commands, intent).unwrap();
    assert_eq!(
        receipt.try_report().unwrap().progress,
        ConfigProgress::Queued
    );
    let result = run(f, &receipt, false, false, false, None);
    // Exercise the normal running-capture validation after config identity
    // changes, not just a retained capture that has skipped worker polling.
    f.pump();
    result
}
fn sample(f: &Fixture, raw: &[f32]) {
    let block: Vec<_> = raw
        .iter()
        .zip(&f.target.gains)
        .map(|(sample, gain)| vec![opname::gained_sample(*sample, *gain)])
        .collect();
    let captures = f.capture.borrow();
    for capture in captures.iter() {
        assert!(capture.accepts(1, 1, 48000));
        capture.ingest_with(&block, 1, |sender, track, packet| {
            // Synthetic offline fixture only: retry the still-unaccepted
            // packet on writer-lock contention, never replay accepted audio.
            let end = Instant::now() + Duration::from_secs(2);
            loop {
                match sender.try_audio(track, packet.clone()) {
                    Err(opname::WriterSendError::Busy) if Instant::now() < end => {
                        std::thread::yield_now()
                    }
                    result => break result,
                }
            }
        });
    }
}

#[test]
fn active_gain_mute_and_muted_db_edits_change_wav_samples_without_restarting() {
    let mut f = Fixture::new(true);
    let recording = f.start();
    let before = recording.try_report().unwrap();
    let options = f.target.options.clone();
    let membership = f.commands.recording_snapshot().unwrap().revision;
    let graph = f.commands.current().stamp;
    sample(&f, &[0.5, 0.25]);
    let request = lower(&f);
    let applied = apply(&mut f, request);
    assert_eq!(applied.outcome, LocalConfigOutcome::Applied);
    let gain = f.target.gains[0];
    assert_eq!(f.target.gains[1].to_bits(), 1_f32.to_bits());
    sample(&f, &[0.5, 0.25]);
    let request = intent(&f, |c| c.buses.states.get_mut("rec-1").unwrap().mute = true);
    assert_eq!(apply(&mut f, request).outcome, LocalConfigOutcome::Applied);
    assert_eq!(
        f.commands
            .config_snapshot()
            .unwrap()
            .configuration
            .unwrap()
            .buses
            .state("rec-1")
            .db,
        -6.
    );
    sample(&f, &[0.5, 0.25]);
    let request = intent(&f, |c| c.buses.states.get_mut("rec-1").unwrap().db = -12.);
    assert_eq!(apply(&mut f, request).outcome, LocalConfigOutcome::Applied);
    assert_eq!(f.target.gains[0], 0.);
    sample(&f, &[0.5, 0.25]);
    let request = intent(&f, |c| {
        c.buses.states.get_mut("rec-1").unwrap().mute = false
    });
    assert_eq!(apply(&mut f, request).outcome, LocalConfigOutcome::Applied);
    sample(&f, &[0.5, 0.25]);
    assert_eq!(f.target.options, options);
    assert_eq!(f.commands.current().stamp, graph);
    assert_eq!(
        f.commands.recording_snapshot().unwrap().revision,
        membership
    );
    let after = recording.try_report().unwrap();
    assert_eq!(after.progress, RecordingProgress::StartedLocally);
    assert!(Arc::ptr_eq(&before.tracks, &after.tracks));
    assert_eq!(
        before.writer.as_ref().unwrap().take,
        after.writer.as_ref().unwrap().take
    );
    assert_eq!(
        before.writer.as_ref().unwrap().directory,
        after.writer.as_ref().unwrap().directory
    );
    let stopped = retained(f.commands.stop_recording(RecordingStop::All).unwrap());
    let done = stopped.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(done.is_success(), "{done:?}");
    let dir = done.writer.unwrap().directory.unwrap();
    assert_wav(
        &dir.join("Rec 1.wav"),
        1,
        48000,
        &[0.5, 0.5 * gain, 0., 0., 0.5 * f.target.gains[0]],
    );
    assert_wav(&dir.join("Rec 2.wav"), 1, 48000, &[0.25; 5]);
    assert_eq!(f.owner.finish().writers_finalized, 1);
}

#[test]
fn gain_only_admission_rejects_layout_arming_transport_and_unknown_state_changes() {
    let f = Fixture::new(true);
    let _take = f.start();
    for mode in 0..8 {
        let invalid = intent(&f, |c| match mode {
            0 => c.buses.widths = vec![2],
            1 => c.buses.widths.push(1),
            2 => c.buses.states.get_mut("rec-2").unwrap().armed = false,
            3 => c.buses.midi = true,
            4 => c.clock = Some(123),
            5 => c.start_stop = true,
            6 => {
                c.buses
                    .states
                    .insert("synthetic-stale-key".into(), BusState::default());
            }
            _ => c.buses.states.get_mut("rec-1").unwrap().db = f64::NAN,
        });
        let err = enqueue(&f.commands, invalid).unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::InvalidInput | ConfigError::UnsupportedMidi
            ),
            "{err:?}"
        );
    }
    assert!(f.commands.0.lock().unwrap().pending.is_empty());
    assert_eq!(f.target.gains, [1., 1.]);
}

#[test]
fn stale_revision_noop_and_legacy_lock_keep_original_authority() {
    let mut f = Fixture::new(true);
    let _take = f.start();
    let original = lower(&f);
    let competing = enqueue(&f.commands, original.clone()).unwrap();
    let noop = intent(&f, |_| {});
    let snapshot = f.commands.config_snapshot().unwrap();
    // Both enqueue against the same revision; the real edit goes first.
    let second = enqueue(&f.commands, noop).unwrap();
    assert_eq!(
        run(&mut f, &competing, false, false, false, None).outcome,
        LocalConfigOutcome::Applied
    );
    assert!(matches!(
        second.try_report().unwrap().progress,
        ConfigProgress::Completed(ConfigCompletion {
            outcome: LocalConfigOutcome::Rejected(ConfigError::StaleRevision),
            ..
        })
    ));
    assert_eq!(
        enqueue(&f.commands, original).unwrap_err(),
        ConfigError::StaleRevision
    );
    assert_ne!(
        snapshot.revision,
        f.commands.config_snapshot().unwrap().revision
    );
    let noop = intent(&f, |_| {});
    let current = f.commands.config_snapshot().unwrap();
    assert_eq!(apply(&mut f, noop).outcome, LocalConfigOutcome::NoOp);
    assert_eq!(f.commands.config_snapshot().unwrap(), current);
    let changed = intent(&f, |c| c.buses.states.get_mut("rec-2").unwrap().db = -3.);
    let receipt = enqueue(&f.commands, changed).unwrap();
    assert_eq!(
        run(&mut f, &receipt, false, false, true, None).outcome,
        LocalConfigOutcome::Rejected(ConfigError::RecordingActive)
    );
    assert_eq!(f.commands.config_snapshot().unwrap(), current);
}

#[test]
fn opening_start_is_busy_and_old_idle_entrypoint_stays_locked() {
    let mut f = Fixture::new(true);
    let take = f.commands.enqueue_recording(f.intent()).unwrap();
    assert_eq!(
        enqueue(&f.commands, lower(&f)).unwrap_err(),
        ConfigError::Busy
    );
    f.drain();
    until(&take, |r| {
        f.pump();
        r.started_once
    });
    let receipt = f.commands.enqueue_config(lower(&f)).unwrap();
    assert_eq!(
        run(&mut f, &receipt, false, false, false, None).outcome,
        LocalConfigOutcome::Rejected(ConfigError::RecordingActive)
    );
    assert_eq!(f.target.gains, [1., 1.]);
    // The dedicated entrypoint can now change levels on exactly that take.
    let request = lower(&f);
    assert_eq!(apply(&mut f, request).outcome, LocalConfigOutcome::Applied);
}

#[test]
fn same_id_replacements_never_rebind_admitted_or_running_gain_edits() {
    for id in [900, 901] {
        let mut f = Fixture::new(true);
        let _take = f.start();
        let receipt = enqueue(&f.commands, lower(&f)).unwrap();
        f.commands.object_removed(id);
        f.commands.object_added(id);
        let graph = (*f.commands.current().value).clone();
        f.commands.publish(graph);
        assert_eq!(
            run(&mut f, &receipt, false, false, false, None).outcome,
            LocalConfigOutcome::Rejected(ConfigError::StaleRevision)
        );
        assert_eq!(
            enqueue(&f.commands, lower(&f)).unwrap_err(),
            ConfigError::StaleRevision
        );
        assert_eq!(f.target.gains, [1., 1.]);
    }
}

#[test]
fn failed_or_partial_gain_readback_is_truthful_and_preserves_transport() {
    for partial in [false, true] {
        let mut f = Fixture::new(true);
        let _take = f.start();
        let request = intent(&f, |c| {
            for s in c.buses.states.values_mut() {
                s.db = -6.;
            }
        });
        let old = f.commands.config_snapshot().unwrap();
        let receipt = enqueue(&f.commands, request).unwrap();
        let result = run(&mut f, &receipt, !partial, partial, false, None);
        assert_eq!(result.snapshot.transport, old.transport);
        if partial {
            assert_eq!(
                result.outcome,
                LocalConfigOutcome::PartiallyApplied(ConfigError::ReadbackMismatch)
            );
            assert!(result.snapshot.configuration.is_none());
            assert_ne!(result.snapshot.revision, old.revision);
        } else {
            assert_eq!(
                result.outcome,
                LocalConfigOutcome::Rejected(ConfigError::ReadbackMismatch)
            );
            assert_eq!(result.snapshot, old);
        }
    }
}

#[test]
fn disconnect_during_claimed_setter_cannot_publish_old_readback_into_new_epoch() {
    let mut f = Fixture::new(true);
    let _take = f.start();
    let commands = &f.commands;
    let receipt = enqueue(commands, lower(&f)).unwrap();
    let mut target = Levels {
        target: &mut f.target,
        commands,
        ignore: false,
        partial: false,
        legacy: false,
        after_gain: Some(Box::new(|| {
            commands.publish(Momentopname::default());
        })),
    };
    commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |w| commands.apply_config(w, &mut target),
    );
    let ConfigProgress::Completed(result) = receipt.try_report().unwrap().progress else {
        panic!()
    };
    assert!(matches!(
        result.outcome,
        LocalConfigOutcome::PartiallyApplied(_)
    ));
    assert_eq!(result.snapshot, commands.config_snapshot().unwrap());
    assert!(!result.snapshot.connected);
    assert!(result.snapshot.local.is_none());
    assert!(result.snapshot.configuration.is_none());
}

#[test]
fn gain_queue_shares_bounds_and_stop_reservation_and_disconnect_reclaims_work() {
    for disconnect in [false, true] {
        let f = Fixture::new(true);
        let _take = f.start();
        let receipts: Vec<_> = (0..MAX_CONFIG_PENDING)
            .map(|_| enqueue(&f.commands, lower(&f)).unwrap())
            .collect();
        assert_eq!(
            enqueue(&f.commands, lower(&f)).unwrap_err(),
            ConfigError::QueueFull
        );
        for _ in MAX_CONFIG_PENDING..NORMAL_CAPACITY {
            f.commands.legacy(Opdracht::Start).unwrap();
        }
        f.commands.legacy(Opdracht::Stop).unwrap();
        assert_eq!(f.commands.0.lock().unwrap().pending.len(), CAPACITY);
        if disconnect {
            f.commands.publish(Momentopname::default());
        } else {
            f.commands.stop();
        }
        assert!(f.commands.0.lock().unwrap().pending.is_empty());
        for receipt in receipts {
            let ConfigProgress::Completed(result) = receipt.try_report().unwrap().progress else {
                panic!()
            };
            assert_eq!(
                result.outcome,
                LocalConfigOutcome::Rejected(if disconnect {
                    ConfigError::Disconnected
                } else {
                    ConfigError::Stopped
                })
            );
        }
        assert_eq!(f.target.gains, [1., 1.]);
    }
}

#[test]
fn replacement_during_claimed_gain_call_is_partial_and_cannot_claim_valid_configuration() {
    for id in [900, 901] {
        let mut f = Fixture::new(true);
        let _take = f.start();
        let commands = &f.commands;
        let receipt = enqueue(commands, lower(&f)).unwrap();
        let mut target = Levels {
            target: &mut f.target,
            commands,
            legacy: false,
            ignore: false,
            partial: false,
            after_gain: Some(Box::new(move || {
                commands.object_removed(id);
                commands.object_added(id);
            })),
        };
        commands.drain_with_config(
            |_| true,
            |_| false,
            |_| LocalExecution::Unreported,
            |w| commands.apply_config(w, &mut target),
        );
        let ConfigProgress::Completed(result) = receipt.try_report().unwrap().progress else {
            panic!()
        };
        assert_eq!(
            result.outcome,
            LocalConfigOutcome::PartiallyApplied(ConfigError::StaleRevision)
        );
        assert!(result.snapshot.configuration.is_none());
        assert!(result.snapshot.local.is_none());
    }
}

#[test]
fn unrelated_graph_revision_and_missing_default_state_do_not_block_gain_changes() {
    let mut f = Fixture::new(true);
    // A known unarmed bus may have an implicit default state. This is normal
    // after loading a layout and is different from editing an unknown key.
    let config = intent(&f, |c| {
        c.buses.states.remove("rec-2");
    });
    let old = f.commands.enqueue_config(config).unwrap();
    f.commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |w| f.commands.apply_config(w, &mut f.target),
    );
    assert!(matches!(
        old.try_report().unwrap().progress,
        ConfigProgress::Completed(_)
    ));
    let take = f.start();
    let edit = intent(&f, |c| {
        c.buses.states.insert(
            "rec-2".into(),
            BusState {
                db: -3.,
                ..Default::default()
            },
        );
    });
    let receipt = enqueue(&f.commands, edit).unwrap();
    let graph = (*f.commands.current().value).clone();
    f.commands.publish(graph);
    assert_eq!(
        run(&mut f, &receipt, false, false, false, None).outcome,
        LocalConfigOutcome::Applied
    );
    assert_eq!(take.try_report().unwrap().tracks.len(), 1);
    assert_eq!(f.target.gains[0], 1.);
}

#[test]
fn claimed_gain_call_can_finish_on_original_filter_after_stop_with_cancellation_flag() {
    let mut f = Fixture::new(true);
    let take = f.start();
    let commands = &f.commands;
    let receipt = enqueue(commands, lower(&f)).unwrap();
    let mut target = Levels {
        target: &mut f.target,
        commands,
        legacy: false,
        ignore: false,
        partial: false,
        after_gain: Some(Box::new(|| commands.stop())),
    };
    commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |w| commands.apply_config(w, &mut target),
    );
    let ConfigProgress::Completed(result) = receipt.try_report().unwrap().progress else {
        panic!()
    };
    assert_eq!(result.outcome, LocalConfigOutcome::Applied);
    assert!(result.cancellation_requested);
    assert!(!result.snapshot.connected);
    assert!(take
        .wait_timeout(Duration::from_secs(2))
        .unwrap()
        .is_success());
}

#[test]
fn stale_readback_options_and_owner_never_mutate_or_fabricate_metadata() {
    for mode in 0..3 {
        let mut f = Fixture::new(true);
        let _take = f.start();
        let mut edit = lower(&f);
        if mode == 2 {
            edit.expected.owner += 1;
            assert_eq!(
                enqueue(&f.commands, edit).unwrap_err(),
                ConfigError::StaleRevision
            );
            continue;
        }
        let receipt = enqueue(&f.commands, edit).unwrap();
        if mode == 0 {
            f.target.gains[1] = 0.125;
        } else {
            f.target.options = None;
        }
        let old = f.target.gains.clone();
        let result = run(&mut f, &receipt, false, false, false, None);
        assert_eq!(
            result.outcome,
            LocalConfigOutcome::Rejected(ConfigError::ReadbackMismatch)
        );
        assert_eq!(f.target.gains, old);
        assert!(result.snapshot.configuration.is_none());
        assert!(result.snapshot.transport.is_none());
    }
}
