//! Idle append only: authored graph publications, no PipeWire or audio.
use super::*;

fn arrive(f: &Fixture, channel: usize) {
    let mut g = f.graph();
    let id = 901 + channel as u32;
    assert!(!g.poorten.iter().any(|p| p.id == id));
    f.commands.object_added(id);
    g.poorten
        .push(port(id, 900, &format!("rec_{}", channel + 1), true));
    f.publish(g);
}
fn local(f: &mut Fixture) -> MigrationReceipt {
    let r = f.start(LayoutAction::AddStereo);
    f.phase(&r, MigrationProgress::AwaitingPorts, false);
    r
}
fn observe_all(f: &mut Fixture, old: usize) {
    arrive(f, old);
    arrive(f, old + 1);
    f.tick(false);
}
fn outcome(r: &MigrationReceipt) -> MigrationOutcome {
    match r.try_report().unwrap().progress {
        MigrationProgress::Completed(o) => o,
        p => panic!("not complete: {p:?}"),
    }
}

#[test]
fn idle_add_preserves_prefix_routes_and_state_until_both_suffix_ports_are_observed() {
    for widths in [vec![2], vec![1, 2]] {
        let mut f = Fixture::new(widths);
        let count = f.target.channels;
        f.add_link(110, 901);
        f.add_link(210, 900 + count as u32);
        let routes = f.graph().kabels;
        let before = f.commands.config_snapshot().unwrap().configuration.unwrap();
        let gains = f.target.gains.clone();
        let original = f.intent(LayoutAction::AddStereo);
        let r = f.commands.enqueue_migration(original.clone()).unwrap();
        assert_eq!(
            f.commands.enqueue_migration(original.clone()).unwrap_err(),
            MigrationError::Busy
        );
        f.phase(&r, MigrationProgress::AwaitingPorts, false);
        let pending = r.try_report().unwrap();
        assert_eq!(f.target.channels, count + 2);
        assert!(pending.configuration.configuration.is_none());
        assert!(pending.observed_inputs.is_none());
        assert_eq!(pending.resize.unwrap().actual, count + 2);
        assert_eq!(&f.target.gains[..count], gains.as_slice());
        // Registry publications can arrive in either order.
        arrive(&f, count + 1);
        f.tick(false);
        assert_eq!(
            r.try_report().unwrap().progress,
            MigrationProgress::AwaitingPorts
        );
        arrive(&f, count);
        f.tick(false);
        let report = r.try_report().unwrap();
        assert_eq!(outcome(&r), MigrationOutcome::Observed);
        assert_eq!(report.proposed.buses, before.buses.add().unwrap());
        assert_eq!(
            report.configuration.configuration.as_ref(),
            Some(&report.proposed)
        );
        assert_eq!(f.graph().kabels, routes);
        assert_eq!(
            (report.attempted, report.submitted, report.no_ops),
            (0, 0, 0)
        );
        assert!(report.new_observed.is_none() && report.old_observed.is_none());
        assert!(
            f.log.borrow().is_empty(),
            "Add must not connect/disconnect routes"
        );
        let witness = report.observed_inputs.unwrap();
        assert_eq!(Some(witness.graph().stamp), report.ports_observed);
        assert!(
            witness.bind_objects([10]).is_none(),
            "result generations are scoped to owned inputs"
        );
        let binding = witness.bind_objects(900..=900 + count as u32 + 2).unwrap();
        assert!(binding.matches(&f.commands.history_graph(&f.commands.current()).unwrap()));
        assert_eq!(
            f.commands.enqueue_migration(original).unwrap_err(),
            MigrationError::StaleConfig
        );
        assert_eq!(f.target.resizes, 1);
    }
}

#[test]
fn add_observation_witness_survives_cache_eviction_and_never_rebinds_a_replacement() {
    let mut f = Fixture::new(vec![2]);
    let r = local(&mut f);
    observe_all(&mut f, 2);
    let report = r.try_report().unwrap();
    let original = report.observed_inputs.unwrap();
    let binding = original.bind_objects(900..=904).unwrap();
    for _ in 0..pecan_core::native_command::NATIVE_SOURCE_HISTORY + 2 {
        f.publish(f.graph());
    }
    assert!(f.commands.history_graph(original.graph()).is_err());
    assert!(binding.matches(&f.commands.history_graph(&f.commands.current()).unwrap()));
    f.commands.object_removed(904);
    f.commands.object_added(904);
    f.publish(f.graph());
    assert!(!binding.matches(&f.commands.history_graph(&f.commands.current()).unwrap()));
    assert_eq!(outcome(&r), MigrationOutcome::Observed);
    assert_eq!(
        r.try_report().unwrap().ports_observed,
        Some(original.graph().stamp)
    );
}

#[test]
fn immediate_growth_publication_during_local_resize_uses_the_same_observation_barrier() {
    let mut f = Fixture::new(vec![2]);
    let commands = f.commands.clone();
    let mut graph = f.graph();
    f.target.on_resize = Some(Box::new(move || {
        for c in 2..4 {
            let id = 901 + c;
            commands.object_added(id);
            graph
                .poorten
                .push(port(id, 900, &format!("rec_{}", c + 1), true));
        }
        commands.publish(graph.clone());
    }));
    let r = local(&mut f);
    assert!(r.try_report().unwrap().observed_inputs.is_none());
    f.tick(false);
    assert_eq!(outcome(&r), MigrationOutcome::Observed);
    assert!(r.try_report().unwrap().observed_inputs.is_some());
    assert_eq!(f.target.resizes, 1);
}

#[test]
fn suffix_replacement_removal_or_rename_cannot_be_adopted_after_first_publication() {
    for mode in 0..5 {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        let routes = f.graph().kabels;
        let r = local(&mut f);
        arrive(&f, 2);
        if mode != 4 {
            f.tick(false);
        }
        let mut g = f.graph();
        match mode {
            0 | 1 | 4 => {
                f.commands.object_removed(903);
                f.commands.object_added(903);
                if mode == 1 {
                    f.publish(g);
                }
            }
            2 => {
                f.commands.object_removed(903);
                g.poorten.retain(|p| p.id != 903);
                f.publish(g);
            }
            _ => {
                g.poorten.iter_mut().find(|p| p.id == 903).unwrap().naam = "rec_4".into();
                f.publish(g);
            }
        }
        f.tick(false);
        let report = r.try_report().unwrap();
        assert_eq!(outcome(&r), MigrationOutcome::PartiallyApplied);
        assert_eq!(report.failure, Some(MigrationError::InvalidObject));
        assert!(report.observed_inputs.is_none());
        assert_eq!(report.local_readback.unwrap().channels, 4);
        assert_eq!(f.graph().kabels, routes);
        assert!(f.log.borrow().is_empty());
    }
    // An unobserved historical generation is not an intent identity. The first
    // coherent suffix publication may already include registry coalescing.
    let mut f = Fixture::new(vec![2]);
    let r = local(&mut f);
    arrive(&f, 2);
    f.commands.object_removed(903);
    f.commands.object_added(903);
    f.publish(f.graph());
    arrive(&f, 3);
    f.tick(false);
    assert_eq!(outcome(&r), MigrationOutcome::Observed);
}

#[test]
fn original_owned_and_source_generations_are_checked_before_add_has_effects() {
    for id in [900, 901, 110] {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        let intent = f.intent(LayoutAction::AddStereo);
        f.commands.object_removed(id);
        f.commands.object_added(id);
        assert_eq!(
            f.commands.enqueue_migration(intent).unwrap_err(),
            MigrationError::InvalidObject
        );
        assert_eq!(f.target.resizes, 0);
        assert!(f.commands.0.lock().unwrap().pending.is_empty());
    }
}

#[test]
fn dispatcher_blocks_new_owned_suffix_commands_without_blocking_foreign_rec() {
    let mut f = Fixture::new(vec![2]);
    let r = local(&mut f);
    arrive(&f, 2);
    f.tick(false);
    f.commands
        .legacy(Opdracht::Verbind {
            uit: 310,
            naar: 903,
        })
        .unwrap();
    f.commands
        .legacy(Opdracht::Verbind {
            uit: 310,
            naar: 701,
        })
        .unwrap();
    let log = f.log.clone();
    let rejected = f.commands.drain_with_migration(
        |_| true,
        |_| false,
        move |command| {
            log.borrow_mut().push(command);
            LocalExecution::Submitted
        },
        |_| panic!("unexpected config"),
        |_| panic!("unexpected recording"),
        |receipt| f.commands.apply_migration(&receipt, &mut f.target),
    );
    assert_eq!(rejected, [DispatchError::InvalidObject]);
    assert_eq!(
        &*f.log.borrow(),
        &[Opdracht::Verbind {
            uit: 310,
            naar: 701
        }]
    );
    assert_eq!(
        r.try_report().unwrap().progress,
        MigrationProgress::AwaitingPorts
    );
}

#[test]
fn unexpected_owned_ports_or_routes_fail_partially_but_foreign_routes_are_ignored() {
    for mode in 0..4 {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        let r = local(&mut f);
        let mut g = f.graph();
        match mode {
            0 | 1 => {
                for c in 2..4 {
                    let id = 901 + c;
                    f.commands.object_added(id);
                    g.poorten.push(port(
                        id,
                        900,
                        if mode == 0 { "unexpected" } else { "rec_3" },
                        true,
                    ));
                }
                f.publish(g);
            }
            2 => {
                arrive(&f, 2);
                arrive(&f, 3);
                f.add_link(310, 904);
            }
            _ => {
                let id = g.kabels.iter().find(|l| l.in_knoop == 900).unwrap().id;
                f.commands.object_removed(id);
                g.kabels.retain(|l| l.id != id);
                f.publish(g);
            }
        }
        f.tick(false);
        assert_eq!(outcome(&r), MigrationOutcome::PartiallyApplied);
        assert_eq!(
            r.try_report().unwrap().failure,
            Some(MigrationError::ExternalConflict)
        );
        assert!(r.try_report().unwrap().observed_inputs.is_none());
        assert!(f.log.borrow().is_empty());
    }
    let mut f = Fixture::new(vec![2]);
    let original = f.add_link(110, 901);
    let r = local(&mut f);
    // Recreated link IDs preserve endpoint topology; foreign Rec links do not
    // become owned merely because their node has the same name.
    let mut g = f.graph();
    f.commands.object_removed(original);
    g.kabels.retain(|l| l.id != original);
    for (id, src, dst, node) in [(4000, 110, 901, 900), (4001, 310, 701, 700)] {
        f.commands.object_added(id);
        g.kabels.push(link(id, src, dst, node));
    }
    f.publish(g);
    observe_all(&mut f, 2);
    assert_eq!(outcome(&r), MigrationOutcome::Observed);
}

#[test]
fn partial_resize_and_timeout_keep_original_proposal_and_cannot_automatically_retry() {
    for fail_resize in [false, true] {
        let mut f = Fixture::new(vec![2]);
        f.add_link(110, 901);
        let original = f.intent(LayoutAction::AddStereo);
        if fail_resize {
            f.target.failure = Some(bus_config::ChannelResizeResult {
                before: 2,
                requested: 4,
                actual: 3,
                failure: Some(bus_config::ResizeFailure::AddFailed { channel: 3 }),
            });
        }
        let r = f.commands.enqueue_migration(original.clone()).unwrap();
        if fail_resize {
            assert_eq!(
                f.finish(&r).progress,
                MigrationProgress::Completed(MigrationOutcome::PartiallyApplied)
            );
        } else {
            f.phase(&r, MigrationProgress::AwaitingPorts, false);
            f.commands
                .drive_migration_at(Instant::now() + OBSERVATION_LIMIT + Duration::from_millis(1));
        }
        let report = r.try_report().unwrap();
        assert_eq!(outcome(&r), MigrationOutcome::PartiallyApplied);
        assert_eq!(report.proposed.buses.widths, [2, 2]);
        assert_eq!(
            report.resize.unwrap().actual,
            if fail_resize { 3 } else { 4 }
        );
        assert_eq!(
            report.failure,
            Some(if fail_resize {
                MigrationError::Configuration(bus_config::ConfigError::ResizeFailed)
            } else {
                MigrationError::ObservationTimedOut
            })
        );
        assert!(report.configuration.configuration.is_none());
        assert!(report.observed_inputs.is_none());
        observe_all(&mut f, 2); // late graph arrival cannot upgrade a frozen outcome
        assert_eq!(outcome(&r), MigrationOutcome::PartiallyApplied);
        assert_eq!(
            f.commands.enqueue_migration(original).unwrap_err(),
            MigrationError::StaleConfig
        );
        assert_eq!(
            f.commands
                .enqueue_migration(f.intent(LayoutAction::AddStereo))
                .unwrap_err(),
            MigrationError::Configuration(bus_config::ConfigError::Unavailable)
        );
        assert_eq!(f.target.resizes, 1);
        assert!(f.log.borrow().is_empty());
    }
}

#[test]
fn add_keeps_generic_growth_and_recording_gates_and_existing_capacity_bounds() {
    let mut f = Fixture::new(vec![2]);
    f.add_link(110, 901);
    let snapshot = f.commands.config_snapshot().unwrap();
    let mut larger = snapshot.configuration.unwrap();
    larger.buses = larger.buses.add().unwrap();
    let witness = f.commands.history_graph(&f.commands.current()).unwrap();
    assert_eq!(
        f.commands
            .enqueue_config_from(
                &witness,
                bus_config::IdleConfigIntent {
                    ticket: 90,
                    expected: snapshot.revision,
                    configuration: larger,
                }
            )
            .unwrap_err(),
        bus_config::ConfigError::LayoutMigrationRequired
    );
    f.commands.refresh_config(f.target.readback(), true, false);
    assert_eq!(
        f.commands
            .enqueue_migration(f.intent(LayoutAction::AddStereo))
            .unwrap_err(),
        MigrationError::RecordingActive
    );
    assert_eq!(f.target.resizes, 0);
    let f = Fixture::new(vec![2; 32]);
    assert_eq!(
        f.commands
            .enqueue_migration(f.intent(LayoutAction::AddStereo))
            .unwrap_err(),
        MigrationError::Configuration(bus_config::ConfigError::InvalidInput)
    );
    assert_eq!(f.target.resizes, 0);
    let mut f = Fixture::new(vec![2]);
    f.add_link(110, 901);
    for _ in 0..NORMAL_CAPACITY {
        f.commands
            .legacy(Opdracht::ZetMute {
                knoop: 700,
                mute: true,
            })
            .unwrap();
    }
    assert_eq!(
        f.commands
            .enqueue_migration(f.intent(LayoutAction::AddStereo))
            .unwrap_err(),
        MigrationError::QueueFull
    );
    f.commands.legacy(Opdracht::Stop).unwrap();
    assert_eq!(f.commands.0.lock().unwrap().pending.len(), CAPACITY);
    assert_eq!(f.target.resizes, 0);
}

#[test]
fn add_cancellation_before_or_during_apply_retains_truthful_effects_and_stable_receipt() {
    for mode in 0..3 {
        let mut f = Fixture::new(vec![2]);
        if mode == 1 {
            let commands = f.commands.clone();
            f.target.on_resize = Some(Box::new(move || commands.stop()));
        }
        let r = f.start(LayoutAction::AddStereo);
        if mode == 0 {
            f.commands.cancel_migration_receipt(&r).unwrap();
        } else if mode == 1 {
            f.finish(&r);
        } else {
            f.phase(&r, MigrationProgress::AwaitingPorts, false);
            f.commands.cancel_migration_receipt(&r).unwrap();
        }
        let report = r.try_report().unwrap();
        assert_eq!(
            outcome(&r),
            if mode == 0 {
                MigrationOutcome::Rejected
            } else {
                MigrationOutcome::PartiallyApplied
            }
        );
        assert!(report.cancellation_requested);
        assert!(report.observed_inputs.is_none());
        assert_eq!(f.target.resizes, usize::from(mode != 0));
        assert!(f.log.borrow().is_empty());
    }
}
