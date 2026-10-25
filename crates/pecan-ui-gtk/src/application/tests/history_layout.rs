//! Retained synthetic publications only; configuration adoption emits no audio.
use super::*;
use pecan_core::domain::{buses::BusConfig, mixer::Mixer};

fn layout() -> BusConfig {
    default_idle_configuration().buses
}
fn split() -> BusConfig {
    layout().change_coupling(0).unwrap()
}
fn scene() -> (
    ApplicationOwner,
    Session,
    FixtureDriver,
    Graph,
    BTreeMap<u32, u64>,
) {
    let (owner, session, driver, origin, generations) = setup();
    let mut graph = (*origin.value).clone();
    graph.knopen.iter_mut().find(|n| n.id == 30).unwrap().naam =
        pecan_core::domain::audio::REC_NODE.into();
    let origin = driver.publish(graph, generations.clone());
    session.poll();
    (owner, session, driver, origin, generations)
}
fn finish_routes(
    session: &Session,
    driver: &FixtureDriver,
    origin: &Graph,
    generations: &BTreeMap<u32, u64>,
) -> Graph {
    let request = driver.take_requests().pop().unwrap();
    let mut graph = (*origin.value).clone();
    for (index, command) in request.intent.commands.iter().enumerate() {
        match command {
            Opdracht::Verbind { uit, naar } => graph.kabels.push(Kabel {
                id: 900 + index as u32,
                client: None,
                uit_knoop: 20,
                uit_poort: *uit,
                in_knoop: 30,
                in_poort: *naar,
            }),
            Opdracht::Ontkoppel { kabel } => graph.kabels.retain(|link| link.id != *kabel),
            other => panic!("unexpected synthetic Rec route command: {other:?}"),
        }
    }
    let graph = driver.publish(graph, generations.clone());
    observed(&request, graph.stamp, false);
    session.poll();
    graph
}
fn with_history(
    redo: bool,
) -> (
    ApplicationOwner,
    Session,
    FixtureDriver,
    Graph,
    BTreeMap<u32, u64>,
) {
    let (owner, session, driver, origin, generations) = scene();
    let origin = commit_mute(&session, &driver, &origin, &generations, true);
    let model = Mixer::build(&origin.value, &layout().definitions().unwrap());
    let source = model
        .apps
        .iter()
        .find(|s| s.node.id == 20)
        .unwrap()
        .key
        .clone();
    let destination = model
        .destinations
        .iter()
        .find(|s| s.node.id == 30)
        .unwrap()
        .key
        .clone();
    session
        .execute(
            &origin,
            "Synthetic Rec route",
            Action::Route {
                source,
                destination,
                enabled: true,
            },
        )
        .unwrap()
        .unwrap();
    let mut current = finish_routes(&session, &driver, &origin, &generations);
    if redo {
        session.undo(&current).unwrap().unwrap();
        current = finish_routes(&session, &driver, &current, &generations);
        assert_eq!(session.history_lengths(), (1, 1));
    } else {
        assert_eq!(session.history_lengths(), (2, 0));
    }
    (owner, session, driver, current, generations)
}

#[test]
fn history_layout_same_stamp_prunes_affected_undo_without_new_graph_or_commands() {
    let (_owner, session, driver, graph, _) = with_history(false);
    driver.forget_origins(); // the accepted witness is retained by the Session.
    session.adopt_buses(&graph, &split()).unwrap();
    assert_eq!(session.history_lengths(), (1, 0));
    assert_eq!(session.history_labels(), (Some("Mute".into()), None));
    assert_eq!(session.graph().unwrap().stamp, graph.stamp);
    assert!(Arc::ptr_eq(&session.graph().unwrap().value, &graph.value));
    assert!(session.poll().graph.is_none());
    assert!(driver.take_requests().is_empty());
}

#[test]
fn history_layout_same_stamp_prunes_affected_redo_and_preserves_unrelated_undo() {
    let (_owner, session, driver, graph, _) = with_history(true);
    session.adopt_buses(&graph, &split()).unwrap();
    assert_eq!(session.history_lengths(), (1, 0));
    assert_eq!(session.history_labels(), (Some("Mute".into()), None));
    assert_eq!(session.graph().unwrap().stamp, graph.stamp);
    assert!(driver.take_requests().is_empty());
    session.undo(&graph).unwrap().unwrap();
    assert_eq!(
        driver.take_requests()[0].intent.commands,
        vec![Opdracht::ZetMute {
            knoop: 20,
            mute: false
        }]
    );
}

#[test]
fn history_layout_unchanged_and_append_definitions_keep_still_valid_steps() {
    let (_owner, session, driver, graph, _) = with_history(false);
    session.adopt_buses(&graph, &layout()).unwrap();
    assert_eq!(session.history_lengths(), (2, 0));
    let appended = layout().add().unwrap();
    session.adopt_buses(&graph, &appended).unwrap();
    let key = appended.definitions().unwrap()[0].key.clone();
    let levels = appended
        .with_db(&key, -12.)
        .unwrap()
        .with_mute(&key, true)
        .unwrap()
        .arm(&key, false)
        .unwrap();
    session.adopt_buses(&graph, &levels).unwrap();
    assert_eq!(session.history_lengths(), (2, 0));
    assert_eq!(
        session.history_labels().0.as_deref(),
        Some("Synthetic Rec route")
    );
    assert!(driver.take_requests().is_empty());
}

#[test]
fn history_layout_pending_and_uncertain_requests_keep_history_and_actual_receipts() {
    let (_owner, session, driver, graph, _) = with_history(false);
    let request = mute(&session, &graph, false);
    let admitted = driver.take_requests().pop().unwrap();
    let prior = session.history_lengths();
    assert!(session.adopt_buses(&graph, &split()).is_err());
    assert_eq!(session.history_lengths(), prior);
    admitted.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            submitted: 1,
            in_progress: 1,
            ..Default::default()
        },
    });
    session.poll();
    let frozen = request.native_report();
    assert!(session.adopt_buses(&graph, &split()).is_err());
    assert_eq!(request.native_report(), frozen);
    assert_eq!(session.history_lengths(), prior);
    assert!(driver.take_requests().is_empty());
}

#[test]
fn history_layout_external_volume_and_copied_frame_gates_do_not_mutate_history() {
    let (_owner, session, driver, graph, _) = with_history(false);
    let (_, lease) = session.reserve_external().unwrap();
    assert!(session.adopt_buses(&graph, &split()).is_err());
    session.adopt_buses(&graph, &layout()).unwrap(); // existing equal-config no-op.
    assert_eq!(session.history_lengths(), (2, 0));
    session.release_external(&lease);
    let key = session
        .volume_model()
        .apps
        .iter()
        .find(|s| s.node.id == 20)
        .unwrap()
        .key
        .clone();
    let gesture = session.begin_volume(&graph, &key).unwrap();
    assert!(session.adopt_buses(&graph, &split()).is_err());
    assert_eq!(session.history_lengths(), (2, 0));
    session.end_volume(&gesture).unwrap();
    session.poll();
    let copy = Graph {
        stamp: graph.stamp,
        value: Arc::new((*graph.value).clone()),
    };
    assert!(session.adopt_buses(&copy, &split()).is_err());
    let mut invalid = layout();
    invalid.widths = vec![0];
    assert!(session.adopt_buses(&graph, &invalid).is_err());
    assert_eq!(session.history_lengths(), (2, 0));
    session.adopt_buses(&graph, &split()).unwrap();
    assert_eq!(session.history_lengths(), (1, 0));
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn history_layout_retired_undo_cannot_retarget_after_same_frame_pruning() {
    let (_owner, session, driver, graph, _) = with_history(false);
    let view = NativeWorkspace::new(session.clone());
    view.poll();
    let old = descendants(view.widget())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("Undo Synthetic Rec route"))
        .unwrap();
    session.adopt_buses(&graph, &split()).unwrap();
    view.poll();
    old.emit_clicked();
    assert!(driver.take_requests().is_empty());
    let current = descendants(view.widget())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("Undo Mute"))
        .unwrap();
    current.emit_clicked();
    assert_eq!(
        driver.take_requests()[0].intent.commands,
        vec![Opdracht::ZetMute {
            knoop: 20,
            mute: false
        }]
    );
}
