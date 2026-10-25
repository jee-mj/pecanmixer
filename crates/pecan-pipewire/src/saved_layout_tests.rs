//! Explicit synthetic saved proposals; no profile, audio server or owner wiring.
use super::*;
use pecan_core::history_identity::HistoryGraph;

fn proposal(widths: Vec<usize>) -> bus_config::IdleBusConfig {
    let mut buses = BusConfig {
        widths,
        states: BTreeMap::new(),
        midi: false,
    };
    for (i, def) in buses.definitions().unwrap().into_iter().enumerate() {
        buses.states.insert(
            def.key,
            BusState {
                db: -(i as f64 + 1.),
                mute: i % 3 == 1,
                armed: i % 2 == 0,
            },
        );
    }
    buses.states.insert(
        "rec-64".into(),
        BusState {
            db: -23.,
            mute: true,
            armed: true,
        },
    );
    bus_config::IdleBusConfig {
        buses,
        clock: None,
        start_stop: false,
    }
}
fn intent(f: &Fixture, configuration: bus_config::IdleBusConfig) -> SavedLayoutIntent {
    let snapshot = f.commands.recording_snapshot().unwrap();
    SavedLayoutIntent {
        ticket: 20,
        origin: f.commands.history_graph(&f.commands.current()).unwrap(),
        expected_config: snapshot.configuration.revision,
        expected_recording: snapshot.revision,
        configuration,
    }
}
fn unknown(f: &Fixture) {
    // The actual legacy/bootstrap invalidation leaves honest local readback.
    f.commands.refresh_config(f.target.readback(), false, true);
    let snapshot = f.commands.config_snapshot().unwrap();
    assert!(snapshot.configuration.is_none() && snapshot.transport.is_none());
}
fn arrive(f: &Fixture, channel: usize) {
    let mut graph = f.graph();
    let id = 901 + channel as u32;
    f.commands.object_added(id);
    graph
        .poorten
        .push(port(id, 900, &format!("rec_{}", channel + 1), true));
    f.publish(graph);
}
fn decided(r: &MigrationReceipt, outcome: MigrationOutcome) -> MigrationReport {
    let report = r.try_report().unwrap();
    assert_eq!(report.progress, MigrationProgress::Completed(outcome));
    assert_eq!(report.in_progress, 0);
    report
}

#[test]
fn unknown_or_known_routed_metadata_accepts_exact_same_capacity_regroup_without_routes() {
    for initialize in [false, true] {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        f.add_link(210, 902);
        let routes = f.graph().kabels;
        if initialize {
            unknown(&f);
            f.target.options = None;
        }
        let desired = proposal(vec![1, 1]);
        let original = intent(&f, desired.clone());
        let source = original.origin.graph().stamp;
        let r = f.commands.enqueue_saved_layout(original.clone()).unwrap();
        assert_eq!(
            f.commands.enqueue_saved_layout(original).unwrap_err(),
            MigrationError::Busy
        );
        f.finish(&r);
        let report = decided(&r, MigrationOutcome::AppliedLocally);
        assert_eq!(report.source, source);
        assert_eq!(report.proposed, desired);
        assert_eq!(report.configuration.configuration, Some(desired.clone()));
        assert_eq!(
            report.configuration.local.unwrap().gains,
            bus_config::validate_configuration(&desired).unwrap().1
        );
        assert!(report.observed_inputs.is_none());
        assert!(report.ports_observed.is_none());
        assert_eq!(f.graph().kabels, routes);
        assert_eq!(f.target.resizes, 0);
        assert!(f.log.borrow().is_empty());
    }
    let mut f = Fixture::new(vec![2]);
    f.add_link(110, 901);
    let desired = f.commands.config_snapshot().unwrap().configuration.unwrap();
    let r = f
        .commands
        .enqueue_saved_layout(intent(&f, desired))
        .unwrap();
    f.finish(&r);
    decided(&r, MigrationOutcome::NoOpLocally);
    assert_eq!(f.target.resizes, 0);
}

#[test]
fn optional_logical_and_transport_metadata_are_independently_accounted_for() {
    for (logical_known, transport_known) in
        [(true, false), (true, true), (false, false), (false, true)]
    {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        let desired = f.commands.config_snapshot().unwrap().configuration.unwrap();
        // Exercise the documented optional fields independently. No current
        // normal producer of the known-layout/unknown-transport state is assumed.
        {
            let mut state = f.commands.0.lock().unwrap();
            if !logical_known {
                state.config.snapshot.configuration = None;
            }
            if !transport_known {
                state.config.snapshot.transport = None;
            }
        }
        if !transport_known {
            f.target.options = None;
        }
        let before = f.commands.config_snapshot().unwrap();
        let r = f
            .commands
            .enqueue_saved_layout(intent(&f, desired.clone()))
            .unwrap();
        f.finish(&r);
        let changed = !logical_known || !transport_known;
        let report = decided(
            &r,
            if changed {
                MigrationOutcome::AppliedLocally
            } else {
                MigrationOutcome::NoOpLocally
            },
        );
        assert_eq!(
            report.configuration.revision.revision > before.revision.revision,
            changed
        );
        assert_eq!(report.configuration.configuration.as_ref(), Some(&desired));
        assert_eq!(
            report.configuration.transport,
            Some(bus_config::validate_configuration(&desired).unwrap().2)
        );
        assert_eq!(f.commands.config_snapshot().unwrap(), report.configuration);
        // With both metadata fields known, a fresh exact restore is a no-op.
        let r = f
            .commands
            .enqueue_saved_layout(intent(&f, desired))
            .unwrap();
        f.finish(&r);
        let no_op = decided(&r, MigrationOutcome::NoOpLocally);
        assert_eq!(no_op.configuration.revision, report.configuration.revision);
        assert_eq!(f.target.resizes, 0);
        assert!(f.log.borrow().is_empty());
    }
}

#[test]
fn unknown_capacity_growth_retains_complete_saved_state_until_exact_final_observation() {
    for widths in [vec![1, 2], vec![2, 2], vec![1, 1, 2], vec![2; 32]] {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        f.add_link(210, 902);
        unknown(&f);
        let routes = f.graph().kabels;
        let desired = proposal(widths);
        let channels = desired.buses.widths.iter().sum::<usize>();
        let original = intent(&f, desired.clone());
        let prefix = original.origin.bind_objects([900, 901, 902]).unwrap();
        let r = f.commands.enqueue_saved_layout(original).unwrap();
        f.phase(&r, MigrationProgress::AwaitingPorts, false);
        let pending = r.try_report().unwrap();
        assert!(pending.configuration.configuration.is_none());
        assert!(pending.observed_inputs.is_none());
        assert_eq!(pending.proposed, desired);
        assert_eq!(pending.local_readback.unwrap().channels, channels);
        for channel in (2..channels).rev() {
            arrive(&f, channel);
            f.tick(false);
            if channel != 2 {
                assert_eq!(
                    r.try_report().unwrap().progress,
                    MigrationProgress::AwaitingPorts
                );
            }
        }
        let report = decided(&r, MigrationOutcome::Observed);
        assert_eq!(report.configuration.configuration, Some(desired.clone()));
        assert_eq!(
            report.configuration.transport,
            Some(bus_config::validate_configuration(&desired).unwrap().2)
        );
        assert_eq!(
            report.local_readback.unwrap().gains,
            bus_config::validate_configuration(&desired).unwrap().1
        );
        let witness = report.observed_inputs.unwrap();
        assert_eq!(Some(witness.graph().stamp), report.ports_observed);
        let current = f.commands.history_graph(&f.commands.current()).unwrap();
        assert!(prefix.matches(&current));
        assert!(witness
            .bind_objects(900..=900 + channels as u32)
            .unwrap()
            .matches(&current));
        assert!(witness.bind_objects([110]).is_none());
        assert_eq!(
            (report.attempted, report.submitted, report.no_ops),
            (0, 0, 0)
        );
        assert_eq!(f.graph().kabels, routes);
        assert_eq!(f.target.resizes, 1);
        assert!(f.log.borrow().is_empty());
    }
}

#[test]
fn retained_original_witness_survives_cache_eviction_but_foreign_authority_does_not() {
    let mut f = Fixture::new(vec![2]);
    f.add_link(110, 901);
    unknown(&f);
    let original = intent(&f, proposal(vec![2, 1]));
    for _ in 0..pecan_core::native_command::NATIVE_SOURCE_HISTORY + 2 {
        f.publish(f.graph());
    }
    assert!(f.commands.history_graph(original.origin.graph()).is_err());
    for foreign in [false, true] {
        let mut bad = original.clone();
        bad.origin = if foreign {
            let other = Fixture::new(vec![2]);
            other
                .commands
                .history_graph(&other.commands.current())
                .unwrap()
        } else {
            HistoryGraph::from_registry(
                original.origin.graph().clone(),
                Arc::new(()),
                f.commands.0.lock().unwrap().published_objects.clone(),
            )
        };
        assert_eq!(
            f.commands.enqueue_saved_layout(bad).unwrap_err(),
            MigrationError::InvalidObject
        );
    }
    let r = f.commands.enqueue_saved_layout(original).unwrap();
    f.phase(&r, MigrationProgress::AwaitingPorts, false);
    arrive(&f, 2);
    f.tick(false);
    decided(&r, MigrationOutcome::Observed);
}

#[test]
fn original_prefix_and_route_sources_cannot_be_rebound_before_admission_or_execution() {
    for id in [900, 901, 10, 110] {
        for admitted in [false, true] {
            let mut f = Fixture::new(vec![2]);
            f.add_link(110, 901);
            unknown(&f);
            let original = intent(&f, proposal(vec![2, 1]));
            let r = admitted.then(|| f.commands.enqueue_saved_layout(original.clone()).unwrap());
            f.commands.object_removed(id);
            f.commands.object_added(id);
            // Deliberately keep the old DTO: registry knowledge fences even
            // a replacement not yet included in the next graph publication.
            if let Some(r) = r {
                f.finish(&r);
                assert_eq!(
                    decided(&r, MigrationOutcome::Rejected).failure,
                    Some(MigrationError::InvalidObject)
                );
            } else {
                assert_eq!(
                    f.commands.enqueue_saved_layout(original).unwrap_err(),
                    MigrationError::InvalidObject
                );
            }
            assert_eq!(f.target.resizes, 0);
            assert!(f.log.borrow().is_empty());
        }
    }
}

#[test]
fn stale_incomplete_shrink_and_invalid_saved_proposals_have_no_effects() {
    for mode in 0..7 {
        let f = Fixture::new(vec![2]);
        unknown(&f);
        let mut request = intent(&f, proposal(vec![2, 1]));
        let expected = match mode {
            0 => {
                request.expected_config.revision += 1;
                MigrationError::StaleConfig
            }
            1 => {
                request.expected_recording.revision += 1;
                MigrationError::StaleRecording
            }
            2 => {
                request.configuration = proposal(vec![1]);
                MigrationError::Configuration(bus_config::ConfigError::LayoutMigrationRequired)
            }
            3 => {
                request.configuration.buses.widths = vec![2; 33];
                MigrationError::Configuration(bus_config::ConfigError::InvalidInput)
            }
            4 => {
                request.configuration.buses.midi = true;
                MigrationError::Configuration(bus_config::ConfigError::UnsupportedMidi)
            }
            5 => {
                let mut graph = f.graph();
                graph.poorten.retain(|p| p.id != 902);
                f.publish(graph);
                request.origin = f.commands.history_graph(&f.commands.current()).unwrap();
                MigrationError::InvalidObject
            }
            _ => {
                request
                    .configuration
                    .buses
                    .states
                    .get_mut("rec-1-2")
                    .unwrap()
                    .db = f64::NAN;
                MigrationError::Configuration(bus_config::ConfigError::InvalidInput)
            }
        };
        assert_eq!(
            f.commands.enqueue_saved_layout(request).unwrap_err(),
            expected
        );
        assert_eq!(f.target.resizes, 0);
        assert!(f.commands.0.lock().unwrap().pending.is_empty());
        assert!(f
            .commands
            .config_snapshot()
            .unwrap()
            .configuration
            .is_none());
    }
}

#[test]
fn immediate_suffix_and_recreated_routes_still_need_a_later_observation_tick() {
    let mut f = Fixture::new(vec![2]);
    let old = f.add_link(110, 901);
    unknown(&f);
    let r = f
        .commands
        .enqueue_saved_layout(intent(&f, proposal(vec![1, 2, 1])))
        .unwrap();
    let commands = f.commands.clone();
    f.target.on_resize = Some(Box::new(move || {
        let mut graph = (*commands.current().value).clone();
        commands.object_removed(old);
        graph.kabels.retain(|l| l.id != old);
        for (id, src, dst, node) in [(4000, 110, 901, 900), (4001, 310, 701, 700)] {
            commands.object_added(id);
            graph.kabels.push(link(id, src, dst, node));
        }
        for channel in (2..4).rev() {
            let id = 901 + channel as u32;
            commands.object_added(id);
            graph
                .poorten
                .push(port(id, 900, &format!("rec_{}", channel + 1), true));
        }
        commands.publish(graph);
    }));
    f.phase(&r, MigrationProgress::AwaitingPorts, false);
    assert!(r.try_report().unwrap().observed_inputs.is_none());
    f.tick(false);
    let report = decided(&r, MigrationOutcome::Observed);
    assert!(report.observed_inputs.is_some());
    assert_eq!(f.target.resizes, 1);
    assert!(f.log.borrow().is_empty());
}

#[test]
fn saved_initialization_does_not_ignore_known_options_or_readback_conflicts() {
    for options in [false, true] {
        let mut f = Fixture::new(vec![2]);
        let r = f
            .commands
            .enqueue_saved_layout(intent(&f, proposal(vec![2, 1])))
            .unwrap();
        if options {
            f.target.options = None;
        } else {
            f.target.gains[0] = 0.125;
        }
        f.finish(&r);
        let report = decided(&r, MigrationOutcome::Rejected);
        assert_eq!(
            report.failure,
            Some(MigrationError::Configuration(
                bus_config::ConfigError::ReadbackMismatch
            ))
        );
        assert!(report.observed_inputs.is_none());
        assert_eq!(f.target.resizes, 0);
        assert!(f.log.borrow().is_empty());
    }
}

#[test]
fn partial_resize_and_timeout_keep_saved_proposal_frozen_without_replay() {
    for partial in [false, true] {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        unknown(&f);
        let desired = proposal(vec![1, 2, 2]);
        if partial {
            f.target.failure = Some(bus_config::ChannelResizeResult {
                before: 2,
                requested: 5,
                actual: 3,
                failure: Some(bus_config::ResizeFailure::AddFailed { channel: 3 }),
            });
        }
        let original = intent(&f, desired.clone());
        let r = f.commands.enqueue_saved_layout(original.clone()).unwrap();
        if partial {
            f.finish(&r);
        } else {
            f.phase(&r, MigrationProgress::AwaitingPorts, false);
            arrive(&f, 4);
            f.tick(false);
            f.commands
                .drive_migration_at(Instant::now() + OBSERVATION_LIMIT + Duration::from_millis(1));
        }
        let report = decided(&r, MigrationOutcome::PartiallyApplied);
        assert_eq!(report.proposed, desired);
        assert_eq!(report.resize.unwrap().actual, if partial { 3 } else { 5 });
        assert!(report.configuration.configuration.is_none());
        assert!(report.observed_inputs.is_none());
        for channel in 2..5 {
            if !f
                .graph()
                .poorten
                .iter()
                .any(|p| p.id == 901 + channel as u32)
            {
                arrive(&f, channel);
            }
        }
        for _ in 0..16 {
            f.tick(false);
        }
        assert_eq!(
            decided(&r, MigrationOutcome::PartiallyApplied).failure,
            report.failure
        );
        assert_eq!(
            f.commands.enqueue_saved_layout(original).unwrap_err(),
            MigrationError::StaleConfig
        );
        assert_eq!(f.target.resizes, 1);
        assert!(f.log.borrow().is_empty());
    }
}

#[test]
fn latched_suffix_replacement_or_unexpected_routes_cannot_become_successful_restore() {
    for mode in 0..4 {
        let mut f = Fixture::new(vec![2]);
        let old = f.add_link(110, 901);
        unknown(&f);
        let r = f
            .commands
            .enqueue_saved_layout(intent(&f, proposal(vec![2, 2])))
            .unwrap();
        f.phase(&r, MigrationProgress::AwaitingPorts, false);
        arrive(&f, 3);
        f.tick(false);
        let mut graph = f.graph();
        match mode {
            0 => {
                f.commands.object_removed(904);
                f.commands.object_added(904);
                f.publish(graph);
            }
            1 => {
                f.add_link(210, 904);
            }
            2 => {
                f.commands.object_removed(old);
                graph.kabels.retain(|l| l.id != old);
                f.publish(graph);
            }
            _ => {
                graph.poorten.iter_mut().find(|p| p.id == 904).unwrap().naam = "rec_3".into();
                f.publish(graph);
            }
        }
        f.tick(false);
        let report = decided(&r, MigrationOutcome::PartiallyApplied);
        assert!(report.observed_inputs.is_none());
        assert!(report.configuration.configuration.is_none());
        assert!(f.log.borrow().is_empty());
    }
}

#[test]
fn final_accessor_replacement_preserves_actual_readback_and_invalidates_metadata() {
    for id in [900, 901, 10, 110] {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        unknown(&f);
        let r = f
            .commands
            .enqueue_saved_layout(intent(&f, proposal(vec![2, 1])))
            .unwrap();
        let commands = f.commands.clone();
        f.target.on_options = Some(Box::new(move || {
            commands.object_removed(id);
            commands.object_added(id);
            commands.publish((*commands.current().value).clone());
        }));
        f.finish(&r);
        let report = decided(&r, MigrationOutcome::PartiallyApplied);
        assert_eq!(report.failure, Some(MigrationError::InvalidObject));
        assert_eq!(report.local_readback.unwrap().channels, 3);
        assert!(report.configuration.local.is_none());
        assert!(report.configuration.configuration.is_none());
        assert!(report.observed_inputs.is_none());
        assert!(f.log.borrow().is_empty());
    }
}

#[test]
fn cancellation_disconnect_shutdown_and_recording_keep_existing_idle_fences() {
    for mode in 0..5 {
        let mut f = Fixture::new(vec![2]);
        unknown(&f);
        let r = f
            .commands
            .enqueue_saved_layout(intent(&f, proposal(vec![2, 1])))
            .unwrap();
        if mode == 0 {
            f.commands.cancel_migration_receipt(&r).unwrap();
        } else if mode == 1 {
            f.commands.stop();
        } else {
            let commands = f.commands.clone();
            f.target.on_resize = Some(Box::new(move || {
                if mode == 2 {
                    commands.publish(Momentopname::default());
                } else if mode == 3 {
                    commands.stop();
                } else {
                    commands
                        .stop_recording(crate::recording::RecordingStop::All)
                        .unwrap();
                }
            }));
        }
        f.finish(&r);
        let report = decided(
            &r,
            if mode < 2 {
                MigrationOutcome::Rejected
            } else {
                MigrationOutcome::PartiallyApplied
            },
        );
        assert!(report.cancellation_requested);
        assert!(report.observed_inputs.is_none());
        assert_eq!(f.target.resizes, usize::from(mode >= 2));
        assert!(f.log.borrow().is_empty());
    }
    let f = Fixture::new(vec![2]);
    unknown(&f);
    f.commands.refresh_config(f.target.readback(), true, false);
    assert_eq!(
        f.commands
            .enqueue_saved_layout(intent(&f, proposal(vec![2, 1])))
            .unwrap_err(),
        MigrationError::RecordingActive
    );
}

#[test]
fn saved_requests_keep_queue_route_and_generic_configuration_bounds() {
    let mut f = Fixture::new(vec![2]);
    f.add_link(110, 901);
    let larger = proposal(vec![2, 1]);
    let original = intent(&f, larger.clone());
    assert_eq!(
        f.commands
            .enqueue_config_from(
                &original.origin,
                bus_config::IdleConfigIntent {
                    ticket: 21,
                    expected: original.expected_config,
                    configuration: larger,
                }
            )
            .unwrap_err(),
        bus_config::ConfigError::LayoutMigrationRequired
    );
    for _ in 0..NORMAL_CAPACITY {
        f.commands
            .legacy(Opdracht::ZetMute {
                knoop: 700,
                mute: true,
            })
            .unwrap();
    }
    assert_eq!(
        f.commands.enqueue_saved_layout(original).unwrap_err(),
        MigrationError::QueueFull
    );
    f.commands.legacy(Opdracht::Stop).unwrap();
    assert_eq!(f.commands.0.lock().unwrap().pending.len(), CAPACITY);
    assert_eq!(f.target.resizes, 0);
    let mut f = Fixture::new(vec![2]);
    for _ in 0..=MAX_MIGRATION_LINKS {
        f.add_link(110, 901);
    }
    assert_eq!(
        f.commands
            .enqueue_saved_layout(intent(&f, proposal(vec![2, 1])))
            .unwrap_err(),
        MigrationError::TooManyRoutes
    );
    assert_eq!(f.target.resizes, 0);
}
