//! Public synthetic core fixtures and explicit owner-presentation status only.
//! No backend, settings path, recording writer or GTK application is started.
use super::*;
use pecan_core::graph::Momentopname;

type Ui = Model<Rc<()>, u64, u64>;
type Frame = Snapshot<Rc<()>, u64, u64>;
fn frame() -> Frame {
    let graph: Momentopname = serde_json::from_str(include_str!(
        "../../../../fixtures/domain/audio-policy.json"
    ))
    .unwrap();
    let mut config: BusConfig =
        serde_json::from_str(include_str!("../../../../fixtures/domain/bus-config.json")).unwrap();
    // Explicit synthetic native idle input, not an automatic preference migration.
    config.midi = false;
    let mixer = Arc::new(Mixer::build(&graph, &config.definitions().unwrap()));
    Frame {
        origin: Rc::new(()),
        connected: true,
        closing: false,
        configuration: Some(config),
        mixer,
        default_sink: graph.standaard_sink,
        has_bus_routes: false,
        activity: Activity::Idle,
        tracks: vec![],
        operation: None,
        capabilities: Capabilities {
            idle_configuration: Support::Available,
            ..Default::default()
        },
    }
}
fn model() -> Ui {
    let mut model = Ui::new(Budget::default());
    model.observe(frame()).unwrap();
    model
}
fn split() -> Action {
    Action::Split {
        bus: "rec-1-2".into(),
    }
}
fn arm(bus: &str, additive: bool) -> Action {
    Action::Arm {
        bus: bus.into(),
        additive,
    }
}
#[test]
fn add_split_join_previews_delegate_to_core_without_applying_or_migrating() {
    let model = model();
    let binding = model.bind().unwrap();
    let before = model.snapshot().unwrap().configuration.as_ref().unwrap();
    for (action, expected) in [
        (Action::AddStereo, before.add().unwrap()),
        (split(), before.change_coupling(0).unwrap()),
        (
            Action::Join {
                bus: "rec-3".into(),
            },
            before.change_coupling(1).unwrap(),
        ),
    ] {
        let intent = model.request(&binding, action).unwrap();
        assert!(Rc::ptr_eq(&intent.origin, &binding.origin));
        assert_eq!(intent.proposed_configuration, Some(expected));
        assert_eq!(
            model.snapshot().unwrap().configuration.as_ref().unwrap(),
            before
        );
        assert!(model.snapshot().unwrap().operation.is_none());
    }
    assert_eq!(
        model
            .rows()
            .unwrap()
            .iter()
            .map(|r| r.definition.coupling)
            .collect::<Vec<_>>(),
        [
            Some(Coupling::Split),
            Some(Coupling::Join),
            None,
            Some(Coupling::Split)
        ]
    );
    assert_eq!(
        model
            .availability(&Action::Join {
                bus: "rec-4".into()
            })
            .unwrap(),
        Availability::Unavailable(Unavailable::WrongCoupling)
    );
}
#[test]
fn exclusive_and_additive_arming_follow_original_core_policy_and_leave_current_state_untouched() {
    let mut model = model();
    let initial = model.snapshot().unwrap().configuration.clone().unwrap();
    let only = model
        .request(&model.bind().unwrap(), arm("rec-4", false))
        .unwrap()
        .proposed_configuration
        .unwrap();
    assert_eq!(only, initial.arm("rec-4", false).unwrap());
    assert_eq!(
        only.armed()
            .unwrap()
            .iter()
            .map(|d| d.key.as_str())
            .collect::<Vec<_>>(),
        ["rec-4"]
    );
    assert_eq!(
        model.snapshot().unwrap().configuration.as_ref().unwrap(),
        &initial
    );
    let mut next = frame();
    next.configuration = Some(only.clone());
    model.observe(next).unwrap();
    let off = model
        .request(&model.bind().unwrap(), arm("rec-4", false))
        .unwrap()
        .proposed_configuration
        .unwrap();
    assert!(off.armed().unwrap().is_empty());
    let additive = model
        .request(&model.bind().unwrap(), arm("rec-1-2", true))
        .unwrap()
        .proposed_configuration
        .unwrap();
    assert_eq!(additive, only.arm("rec-1-2", true).unwrap());
    assert_eq!(additive.armed().unwrap().len(), 2);
}
#[test]
fn output_and_rec_choices_are_independent_with_js_default_and_missing_choice_fallbacks() {
    let mut model = model();
    let origin = model.bind().unwrap();
    let initial_output = model.selected_output().unwrap();
    assert_eq!(initial_output.node.id, 10);
    assert!(initial_output.targetable);
    let initial_rec = model.selected_rec().unwrap().key.clone();
    let output = model
        .snapshot()
        .unwrap()
        .mixer
        .destinations
        .iter()
        .filter(|d| d.bus.is_none())
        .last()
        .unwrap()
        .key
        .clone();
    let rec = model
        .snapshot()
        .unwrap()
        .mixer
        .destinations
        .iter()
        .filter(|d| d.bus.is_some())
        .nth(1)
        .unwrap()
        .key
        .clone();
    model.select_destination(&origin, &output).unwrap();
    assert_eq!(model.selected_rec().unwrap().key, initial_rec);
    model.select_destination(&origin, &rec).unwrap();
    assert_eq!(model.selected_output().unwrap().key, output);
    assert_eq!(model.selected_rec().unwrap().key, rec);
    let mut missing = frame();
    let mut projection = (*missing.mixer).clone();
    projection
        .destinations
        .retain(|d| d.key != output && d.key != rec);
    missing.mixer = Arc::new(projection);
    model.observe(missing).unwrap();
    assert_eq!(model.selected_output().unwrap().node.id, 10);
    assert_eq!(model.selected_rec().unwrap().key, initial_rec);
    model.observe(frame()).unwrap();
    assert_eq!(model.selected_output().unwrap().key, output);
    assert_eq!(model.selected_rec().unwrap().key, rec);
}
#[test]
fn removal_is_a_snapshot_bound_owner_intent_with_no_ui_migration_plan() {
    let mut model = model();
    let action = Action::Remove {
        bus: "rec-3".into(),
    };
    assert_eq!(
        model.availability(&action).unwrap(),
        Availability::PendingCapability(Dependency::ChannelShrinkMigration)
    );
    assert!(matches!(
        model.request(&model.bind().unwrap(), action.clone()),
        Err(Error::Disabled(_))
    ));
    let mut snapshot = frame();
    snapshot.capabilities.shrink_layout = Support::Available;
    model.observe(snapshot).unwrap();
    let intent = model
        .request(&model.bind().unwrap(), action.clone())
        .unwrap();
    assert_eq!(intent.action, action);
    assert!(intent.proposed_configuration.is_none());
    let mut one = frame();
    let mut config = BusConfig::default();
    config.midi = false;
    one.configuration = Some(config);
    model.observe(one).unwrap();
    assert_eq!(
        model
            .availability(&Action::Remove {
                bus: "rec-1-2".into()
            })
            .unwrap(),
        Availability::Unavailable(Unavailable::LastBus)
    );
}
#[test]
fn active_layout_and_dynamic_arming_remain_explicit_capabilities_not_permanent_restrictions() {
    let mut model = model();
    for activity in [Activity::Recording, Activity::Finalizing] {
        let mut s = frame();
        s.activity = activity;
        model.observe(s).unwrap();
        assert_eq!(
            model.availability(&split()).unwrap(),
            Availability::PendingCapability(Dependency::ActiveLayoutMigration)
        );
        assert_eq!(
            model.availability(&arm("rec-4", true)).unwrap(),
            Availability::PendingCapability(Dependency::DynamicArming)
        );
        assert!(matches!(
            model.request(&model.bind().unwrap(), split()),
            Err(Error::Disabled(_))
        ));
        let mut s = frame();
        s.activity = activity;
        s.capabilities.active_layout = Support::Available;
        s.capabilities.active_arming = Support::Available;
        model.observe(s).unwrap();
        assert!(model.request(&model.bind().unwrap(), split()).is_ok());
        assert!(model
            .request(&model.bind().unwrap(), arm("rec-4", true))
            .is_ok());
    }
    assert_eq!(
        model.recording_control(),
        Availability::PendingCapability(Dependency::RecordingReceipts)
    );
}
#[test]
fn routes_require_reviewed_migration_even_while_idle() {
    let mut model = model();
    let mut s = frame();
    s.has_bus_routes = true;
    model.observe(s).unwrap();
    assert_eq!(
        model.availability(&Action::AddStereo).unwrap(),
        Availability::PendingCapability(Dependency::RoutedLayoutMigration)
    );
    assert_eq!(
        model.availability(&split()).unwrap(),
        Availability::PendingCapability(Dependency::RoutedLayoutMigration)
    );
    assert_eq!(
        model.availability(&arm("rec-4", true)).unwrap(),
        Availability::Available
    );
    let mut s = frame();
    s.has_bus_routes = true;
    s.capabilities.routed_layout = Support::Available;
    model.observe(s).unwrap();
    assert!(model.request(&model.bind().unwrap(), split()).is_ok());
    assert_eq!(
        model.availability(&Action::AddStereo).unwrap(),
        Availability::PendingCapability(Dependency::RoutedLayoutMigration)
    );
    assert!(Progress::Observed.message().contains("observed"));
    assert!(!Progress::AppliedLocally.message().contains("observed"));
}
#[test]
fn queued_applying_partial_and_unconfirmed_status_never_fake_applied_configuration() {
    let mut model = model();
    let initial = model.snapshot().unwrap().configuration.clone();
    for progress in [
        Progress::Queued,
        Progress::Applying,
        Progress::AwaitingObservation,
    ] {
        let mut s = frame();
        s.operation = Some(Operation {
            request: 7,
            progress,
        });
        model.observe(s).unwrap();
        assert_eq!(
            model.availability(&split()).unwrap(),
            Availability::PendingOperation
        );
        assert_eq!(model.snapshot().unwrap().configuration, initial);
    }
    for progress in [
        Progress::PartiallyApplied("Synthetic partial resize".into()),
        Progress::Unconfirmed("Synthetic unknown outcome".into()),
    ] {
        let mut s = frame();
        s.operation = Some(Operation {
            request: 7,
            progress,
        });
        model.observe(s).unwrap();
        assert_eq!(
            model.availability(&split()).unwrap(),
            Availability::Unavailable(Unavailable::ReconciliationRequired)
        );
    }
    let mut s = frame();
    s.operation = Some(Operation {
        request: 7,
        progress: Progress::Rejected("Synthetic queue full".into()),
    });
    model.observe(s).unwrap();
    assert!(model
        .snapshot()
        .unwrap()
        .operation
        .as_ref()
        .unwrap()
        .progress
        .message()
        .contains("rejected"));
    assert_eq!(model.snapshot().unwrap().configuration, initial);
    assert_eq!(
        model.availability(&split()).unwrap(),
        Availability::Available
    );
}
#[test]
fn applied_local_configuration_does_not_invent_graph_ports_or_writer_readiness() {
    let mut model = model();
    let mut s = frame();
    let mut projection = (*s.mixer).clone();
    projection.destinations.retain(|d| d.bus.is_none());
    s.mixer = Arc::new(projection);
    s.operation = Some(Operation {
        request: 7,
        progress: Progress::AppliedLocally,
    });
    model.observe(s).unwrap();
    let rows = model.rows().unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows
        .iter()
        .all(|r| r.destination_key.is_none() && r.tracks.is_empty()));
    assert!(model.selected_rec().is_none());
    assert!(model.selected_output().is_some());
    assert_eq!(
        model.recording_control(),
        Availability::PendingCapability(Dependency::RecordingReceipts)
    );
    assert_eq!(
        model
            .snapshot()
            .unwrap()
            .operation
            .as_ref()
            .unwrap()
            .progress,
        Progress::AppliedLocally
    );
}
#[test]
fn multiple_track_identities_and_finalization_errors_remain_visible_without_false_join() {
    let mut model = model();
    let mut s = frame();
    s.activity = Activity::Recording;
    s.tracks = vec![
        Track {
            identity: 101,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::WriterReady,
            elapsed: None,
        },
        Track {
            identity: 99,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::Finalized(Err("Synthetic finalization error".into())),
            elapsed: Some(Duration::from_secs(2)),
        },
        Track {
            identity: 98,
            bus_key: "retired-synthetic-bus".into(),
            phase: TrackPhase::Finalizing,
            elapsed: None,
        },
    ];
    model.observe(s).unwrap();
    let rows = model.rows().unwrap();
    let row = rows.iter().find(|r| r.definition.key == "rec-1-2").unwrap();
    assert_eq!(row.tracks.len(), 2);
    assert_eq!(row.tracks[0].identity, 101);
    assert!(matches!(
        &row.tracks[1].phase,
        TrackPhase::Finalized(Err(_))
    ));
    assert!(!row
        .tracks
        .iter()
        .any(|t| matches!(t.phase, TrackPhase::Joined(_))));
    assert_eq!(model.unmatched_tracks().unwrap()[0].identity, 98);
}
#[test]
fn new_publications_reconnect_and_cross_model_bindings_cannot_retarget_old_intents() {
    let mut model = model();
    let old = model.bind().unwrap();
    let other = self::model();
    assert!(matches!(
        other.request(&old, split()),
        Err(Error::StaleBinding)
    ));
    let mut disconnected = frame();
    disconnected.connected = false;
    model.observe(disconnected).unwrap();
    assert!(model.selected_output().is_none());
    assert!(model.selected_rec().is_none());
    assert!(model
        .rows()
        .unwrap()
        .iter()
        .all(|r| r.destination_key.is_none()));
    assert_eq!(
        model.availability(&split()).unwrap(),
        Availability::Unavailable(Unavailable::Disconnected)
    );
    model.observe(frame()).unwrap();
    assert!(matches!(
        model.request(&old, split()),
        Err(Error::StaleBinding)
    ));
    assert!(model.request(&model.bind().unwrap(), split()).is_ok());
}
#[test]
fn unknown_config_activity_and_closing_are_truthful_disabled_surfaces() {
    let mut model = model();
    let mut s = frame();
    s.configuration = None;
    let rec = s
        .mixer
        .destinations
        .iter()
        .find(|d| d.bus.is_some())
        .unwrap()
        .key
        .clone();
    model.observe(s).unwrap();
    assert!(model.rows().unwrap().is_empty());
    assert!(model.selected_rec().is_none());
    assert!(model.selected_output().is_some());
    assert_eq!(
        model.availability(&split()).unwrap(),
        Availability::Unavailable(Unavailable::UnknownConfiguration)
    );
    assert!(matches!(
        model.select_destination(&model.bind().unwrap(), &rec),
        Err(Error::Disabled(_))
    ));
    let mut s = frame();
    s.activity = Activity::Unknown;
    model.observe(s).unwrap();
    assert_eq!(
        model.availability(&split()).unwrap(),
        Availability::Unavailable(Unavailable::UnknownActivity)
    );
    let mut s = frame();
    s.closing = true;
    model.observe(s).unwrap();
    assert_eq!(
        model.availability(&split()).unwrap(),
        Availability::Unavailable(Unavailable::Closing)
    );
}
#[test]
fn default_capabilities_and_input_budgets_do_not_silently_enable_or_truncate() {
    let mut model = model();
    let mut s = frame();
    s.capabilities = Capabilities::default();
    model.observe(s).unwrap();
    assert_eq!(
        model.availability(&split()).unwrap(),
        Availability::PendingCapability(Dependency::IdleConfigurationAdapter)
    );
    let mut bounded = Ui::new(Budget {
        buses: 1,
        tracks: 1,
    });
    assert_eq!(bounded.observe(frame()), Err(Error::BudgetExceeded));
    assert!(bounded.snapshot().is_none());
    let mut s = frame();
    s.configuration = None;
    s.tracks = vec![
        Track {
            identity: 1,
            bus_key: "synthetic".into(),
            phase: TrackPhase::Starting,
            elapsed: None
        };
        2
    ];
    assert_eq!(bounded.observe(s), Err(Error::BudgetExceeded));
    assert!(bounded.snapshot().is_none());
    let mut invalid = frame();
    let stale = model.bind().unwrap();
    invalid.configuration.as_mut().unwrap().widths = vec![3];
    assert_eq!(
        model.observe(invalid),
        Err(Error::Domain(BusError::InvalidWidth))
    );
    assert_eq!(
        model.publication_error(),
        Some(&Error::Domain(BusError::InvalidWidth))
    );
    assert!(model.snapshot().is_none());
    assert!(matches!(
        model.request(&stale, split()),
        Err(Error::StaleBinding)
    ));
    model.observe(frame()).unwrap();
    assert!(model.publication_error().is_none());
    assert!(model.request(&model.bind().unwrap(), split()).is_ok());
}

#[test]
fn growth_preflight_cannot_emit_a_layout_beyond_the_models_row_budget() {
    let mut model = Ui::new(Budget {
        buses: 4,
        tracks: 8,
    });
    model.observe(frame()).unwrap();
    for action in [Action::AddStereo, split()] {
        assert_eq!(
            model.availability(&action).unwrap(),
            Availability::Unavailable(Unavailable::RowBudget)
        );
        assert!(matches!(
            model.request(&model.bind().unwrap(), action),
            Err(Error::Disabled(Availability::Unavailable(
                Unavailable::RowBudget
            )))
        ));
    }
    let join = model
        .request(
            &model.bind().unwrap(),
            Action::Join {
                bus: "rec-3".into(),
            },
        )
        .unwrap();
    assert_eq!(join.proposed_configuration.unwrap().widths.len(), 3);
}
