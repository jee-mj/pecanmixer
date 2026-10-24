//! Generic synthetic topology and injected local/observation faults only.
use super::*;
use pecan_core::{
    domain::buses::{BusConfig, BusState},
    graph::{Kabel, Knoop, Poort},
};
use std::{cell::RefCell, rc::Rc, time::Duration};

struct Target {
    channels: usize,
    gains: Vec<f32>,
    options: Option<pecan_core::graph::Opties>,
    failure: Option<bus_config::ChannelResizeResult>,
    ignore_gains: bool,
    recording: bool,
    resizes: usize,
    on_resize: Option<Box<dyn FnMut()>>,
    on_options: Option<Box<dyn FnMut()>>,
}
impl ConfigTarget for Target {
    fn readback(&self) -> Option<bus_config::BusReadback> {
        Some(bus_config::BusReadback {
            node: Some(900),
            channels: self.channels,
            gains: self.gains.clone(),
        })
    }
    fn recording(&self) -> bool {
        self.recording
    }
    fn routed(&self) -> bool {
        false
    }
    fn resize(&mut self, requested: usize) -> bus_config::ChannelResizeResult {
        self.resizes += 1;
        let result = self
            .failure
            .clone()
            .unwrap_or(bus_config::ChannelResizeResult {
                before: self.channels,
                requested,
                actual: requested,
                failure: None,
            });
        self.channels = result.actual;
        self.gains.resize(self.channels, 1.0);
        if let Some(hook) = self.on_resize.as_mut() {
            hook();
        }
        result
    }
    fn gains(&mut self, g: &[f32]) {
        if !self.ignore_gains {
            self.gains = g.to_vec();
        }
    }
    fn options(&mut self, o: pecan_core::graph::Opties) {
        self.options = Some(o);
        if let Some(hook) = self.on_options.as_mut() {
            hook();
        }
    }
    fn options_match(&self, o: &pecan_core::graph::Opties) -> bool {
        self.options.as_ref() == Some(o)
    }
}
struct Fixture {
    commands: Arc<Commands>,
    target: Target,
    log: Rc<RefCell<Vec<Opdracht>>>,
    effects: Rc<RefCell<Vec<Opdracht>>>,
    next: u32,
}
fn node(id: u32, name: &str) -> Knoop {
    serde_json::from_value(serde_json::json!({"id":id,"serial":id as u64*100,"naam":name})).unwrap()
}
fn port(id: u32, node: u32, name: &str, input: bool) -> Poort {
    Poort {
        id,
        knoop: node,
        volgnummer: None,
        naam: name.into(),
        alias: None,
        ingang: input,
        soort: Soort::Audio,
        kanaal: None,
        monitor: false,
        fysiek: false,
        groep: None,
    }
}
fn link(id: u32, source: u32, destination: u32, node: u32) -> Kabel {
    Kabel {
        id,
        uit_knoop: source / 100 * 10,
        uit_poort: source,
        in_knoop: node,
        in_poort: destination,
        client: None,
    }
}
impl Fixture {
    fn new(widths: Vec<usize>) -> Self {
        let commands = Arc::new(Commands::default());
        let count: usize = widths.iter().sum();
        let mut ports = vec![
            port(110, 10, "synthetic.output.a", false),
            port(210, 20, "synthetic.output.b", false),
            port(310, 30, "synthetic.output.c", false),
            port(701, 700, "rec_1", true),
        ];
        ports
            .extend((0..count).map(|i| port(901 + i as u32, 900, &format!("rec_{}", i + 1), true)));
        let mut graph = Momentopname {
            verbonden: true,
            rate: Some(48000),
            knopen: vec![
                node(700, crate::opname::BUS_NAAM),
                node(10, "synthetic.source.a"),
                node(20, "synthetic.source.b"),
                node(30, "synthetic.source.c"),
                node(900, crate::opname::BUS_NAAM),
            ],
            poorten: ports,
            ..Default::default()
        };
        graph.kabels.push(link(1000, 110, 701, 700));
        for id in graph
            .knopen
            .iter()
            .map(|n| n.id)
            .chain(graph.poorten.iter().map(|p| p.id))
            .chain(graph.kabels.iter().map(|l| l.id))
        {
            commands.object_added(id);
        }
        commands.publish(graph);
        let mut target = Target {
            channels: count,
            gains: vec![1.; count],
            options: None,
            failure: None,
            ignore_gains: false,
            recording: false,
            resizes: 0,
            on_resize: None,
            on_options: None,
        };
        commands.refresh_config(target.readback(), false, false);
        let mut buses = BusConfig {
            widths,
            states: BTreeMap::new(),
            midi: false,
        };
        for (i, d) in buses.definitions().unwrap().into_iter().enumerate() {
            buses.states.insert(
                d.key,
                BusState {
                    db: -(i as f64) * 3.,
                    mute: false,
                    armed: i % 2 == 0,
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
            |w| commands.apply_config(w, &mut target),
        );
        target.resizes = 0;
        Self {
            commands,
            target,
            log: Rc::default(),
            effects: Rc::default(),
            next: 2000,
        }
    }
    fn graph(&self) -> Momentopname {
        (*self.commands.current().value).clone()
    }
    fn publish(&self, g: Momentopname) {
        self.commands.publish(g);
    }
    fn add_link(&mut self, source: u32, destination: u32) -> u32 {
        let id = self.next;
        self.next += 1;
        let mut g = self.graph();
        self.commands.object_added(id);
        g.kabels.push(link(id, source, destination, 900));
        self.publish(g);
        id
    }
    fn routed(&mut self, overlap: bool) {
        assert_eq!(self.target.channels, 6);
        self.add_link(if overlap { 110 } else { 310 }, 903);
        self.add_link(110, 905);
        self.add_link(210, 906);
    }
    fn intent(&self, action: LayoutAction) -> LayoutIntent {
        let snapshot = self.commands.recording_snapshot().unwrap();
        LayoutIntent {
            ticket: 10,
            origin: self.commands.current(),
            expected_config: snapshot.configuration.revision,
            expected_recording: snapshot.revision,
            action,
        }
    }
    fn start(&self, action: LayoutAction) -> MigrationReceipt {
        self.commands
            .enqueue_migration(self.intent(action))
            .unwrap()
    }
    fn remove(&self) -> MigrationReceipt {
        self.start(LayoutAction::Remove {
            bus: "rec-3-4".into(),
        })
    }
    fn tick(&mut self, observe: bool) {
        self.commands.drive_migration_at(Instant::now());
        let log = self.log.clone();
        let effects = self.effects.clone();
        self.commands.drain_with_migration(
            |_| true,
            |_| false,
            move |o| {
                log.borrow_mut().push(o.clone());
                effects.borrow_mut().push(o);
                LocalExecution::Submitted
            },
            |_| panic!("unexpected idle config"),
            |_| panic!("migration must not start recording"),
            |r| self.commands.apply_migration(&r, &mut self.target),
        );
        if observe {
            self.observe();
        }
    }
    fn observe(&mut self) {
        let effects = std::mem::take(&mut *self.effects.borrow_mut());
        let mut g = self.graph();
        let mut changed = false;
        for command in effects {
            match command {
                Opdracht::Verbind { uit, naar } => {
                    if !g
                        .kabels
                        .iter()
                        .any(|l| l.uit_poort == uit && l.in_poort == naar)
                    {
                        let id = self.next;
                        self.next += 1;
                        self.commands.object_added(id);
                        g.kabels.push(link(id, uit, naar, 900));
                        changed = true;
                    }
                }
                Opdracht::Ontkoppel { kabel } => {
                    if g.kabels.iter().any(|l| l.id == kabel) {
                        g.kabels.retain(|l| l.id != kabel);
                        self.commands.object_removed(kabel);
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        let tails: Vec<_> = g
            .poorten
            .iter()
            .filter(|p| {
                p.knoop == 900
                    && p.ingang
                    && p.naam
                        .strip_prefix("rec_")
                        .and_then(|s| s.parse::<usize>().ok())
                        .is_some_and(|n| n > self.target.channels)
            })
            .map(|p| p.id)
            .collect();
        for id in tails {
            self.commands.object_removed(id);
            g.poorten.retain(|p| p.id != id);
            g.kabels.retain(|l| l.in_poort != id);
            changed = true;
        }
        if changed {
            self.publish(g);
        }
    }
    fn finish(&mut self, r: &MigrationReceipt) -> MigrationReport {
        for _ in 0..2048 {
            self.tick(true);
            let report = r.try_report().unwrap();
            if report.is_decided() {
                return report;
            }
        }
        panic!("migration did not decide: {:?}", r.try_report());
    }
    fn phase(&mut self, r: &MigrationReceipt, phase: MigrationProgress, observe: bool) {
        for _ in 0..2048 {
            if r.try_report().unwrap().progress == phase {
                return;
            }
            self.tick(observe);
            assert!(
                !r.try_report().unwrap().is_decided(),
                "{:?}",
                r.try_report()
            );
        }
        panic!("phase unavailable");
    }
}

#[test]
fn observed_barriers_precede_disconnect_shrink_and_final_configuration_publication() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    let original = f.commands.config_snapshot().unwrap().configuration.unwrap();
    let r = f.remove();
    f.phase(&r, MigrationProgress::AwaitingNew, false);
    assert_eq!(f.target.resizes, 0);
    assert!(f
        .log
        .borrow()
        .iter()
        .all(|o| matches!(o, Opdracht::Verbind { .. })));
    assert!(!r.try_report().unwrap().is_success());
    f.observe();
    f.phase(&r, MigrationProgress::AwaitingOld, false);
    assert_eq!(f.target.resizes, 0);
    assert!(r.try_report().unwrap().new_observed.is_some());
    f.observe();
    f.phase(&r, MigrationProgress::AwaitingPorts, false);
    assert_eq!(f.target.channels, 4);
    assert_eq!(f.target.resizes, 1);
    assert!(f
        .commands
        .config_snapshot()
        .unwrap()
        .configuration
        .is_none());
    assert!(r.try_report().unwrap().old_observed.is_some());
    f.observe();
    let result = f.finish(&r);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::Observed)
    );
    assert!(result.ports_observed.is_some());
    assert!(result.resize.unwrap().is_complete());
    let config = result.configuration.configuration.unwrap();
    assert_eq!(config.buses.widths, [2, 2]);
    assert_eq!(
        config.buses.state("rec-3-4"),
        original.buses.state("rec-5-6")
    );
    assert!(f
        .graph()
        .kabels
        .iter()
        .any(|l| l.in_knoop == 700 && l.in_poort == 701));
    assert!(f
        .graph()
        .kabels
        .iter()
        .any(|l| l.uit_poort == 110 && l.in_poort == 903));
    assert!(f
        .graph()
        .kabels
        .iter()
        .any(|l| l.uit_poort == 210 && l.in_poort == 904));
    let log = f.log.borrow();
    let first_disconnect = log
        .iter()
        .position(|o| matches!(o, Opdracht::Ontkoppel { .. }))
        .unwrap();
    assert!(log[..first_disconnect]
        .iter()
        .all(|o| matches!(o, Opdracht::Verbind { .. })));
}

#[test]
fn desired_edge_overlap_is_retained_instead_of_deleted_after_a_connect_noop() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(true);
    let preserved = f
        .graph()
        .kabels
        .iter()
        .find(|l| l.in_poort == 903)
        .unwrap()
        .id;
    let r = f.remove();
    let result = f.finish(&r);
    assert!(result.is_success(), "{result:?}");
    assert!(result.no_ops >= 1);
    assert!(!f
        .log
        .borrow()
        .iter()
        .any(|o| *o == Opdracht::Ontkoppel { kabel: preserved }));
    assert!(f.graph().kabels.iter().any(|l| l.id == preserved));
    assert!(!f.log.borrow().iter().any(|o| *o
        == Opdracht::Verbind {
            uit: 110,
            naar: 903
        }));
}

#[test]
fn split_join_and_last_bus_noop_complete_by_local_readback_without_graph_events() {
    let mut f = Fixture::new(vec![2, 2]);
    let stamp = f.commands.current().stamp;
    let split = f.start(LayoutAction::Split {
        bus: "rec-1-2".into(),
    });
    let result = f.finish(&split);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::AppliedLocally)
    );
    assert_eq!(f.commands.current().stamp, stamp);
    assert_eq!(f.target.resizes, 0);
    assert!(f.log.borrow().is_empty());
    let join = f.start(LayoutAction::Join {
        bus: "rec-1".into(),
    });
    assert!(f.finish(&join).is_success());
    assert_eq!(f.commands.current().stamp, stamp);
    let mut one = Fixture::new(vec![2]);
    let before = one.commands.config_snapshot().unwrap();
    let r = one.start(LayoutAction::Remove {
        bus: "rec-1-2".into(),
    });
    let result = one.finish(&r);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::NoOpLocally)
    );
    assert_eq!(result.configuration.revision, before.revision);
    assert_eq!(one.target.resizes, 0);
}

#[test]
fn recreated_obsolete_link_is_destroyed_by_fresh_id_before_shrink() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    let r = f.remove();
    f.phase(&r, MigrationProgress::AwaitingNew, false);
    f.observe();
    f.phase(&r, MigrationProgress::AwaitingOld, false);
    f.observe();
    let replacement = f.add_link(110, 905);
    let result = f.finish(&r);
    assert!(result.is_success(), "{result:?}");
    assert!(f
        .log
        .borrow()
        .contains(&Opdracht::Ontkoppel { kabel: replacement }));
    assert_eq!(f.target.channels, 4);
}

#[test]
fn source_or_owned_port_replacement_rejects_without_shrinking() {
    for id in [110, 903, 900] {
        let mut f = Fixture::new(vec![2, 2, 2]);
        f.routed(false);
        let r = f.remove();
        f.tick(false);
        f.commands.object_removed(id);
        f.commands.object_added(id);
        let graph = f.graph();
        f.publish(graph);
        let result = f.finish(&r);
        assert!(!result.is_success());
        assert_eq!(result.failure, Some(MigrationError::InvalidObject));
        assert_eq!(f.target.resizes, 0);
    }
}

#[test]
fn unexpected_routes_or_disappearing_desired_pairs_stop_destructive_progress() {
    for after_new in [false, true] {
        let mut f = Fixture::new(vec![2, 2, 2]);
        f.routed(false);
        let r = f.remove();
        f.phase(&r, MigrationProgress::AwaitingNew, false);
        f.observe();
        if after_new {
            f.tick(false);
            let mut g = f.graph();
            let l = g
                .kabels
                .iter()
                .find(|l| l.uit_poort == 110 && l.in_poort == 903)
                .unwrap()
                .id;
            f.commands.object_removed(l);
            g.kabels.retain(|k| k.id != l);
            f.publish(g);
        } else {
            f.add_link(210, 903);
        }
        let result = f.finish(&r);
        assert_eq!(result.failure, Some(MigrationError::ExternalConflict));
        assert_eq!(f.target.resizes, 0);
        assert!(!f
            .log
            .borrow()
            .iter()
            .any(|o| matches!(o, Opdracht::Ontkoppel { .. })));
    }
}

#[test]
fn barrier_timeout_never_disconnects_or_infers_success_from_local_submission() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    let r = f.remove();
    f.phase(&r, MigrationProgress::AwaitingNew, false);
    f.commands
        .drive_migration_at(Instant::now() + OBSERVATION_LIMIT + Duration::from_millis(1));
    let report = r.try_report().unwrap();
    assert_eq!(report.failure, Some(MigrationError::ObservationTimedOut));
    assert!(!report.is_success());
    assert!(report.submitted > 0);
    assert_eq!(f.target.resizes, 0);
    assert!(f
        .log
        .borrow()
        .iter()
        .all(|o| matches!(o, Opdracht::Verbind { .. })));
}

#[test]
fn stale_revisions_and_copied_origins_fail_before_admission() {
    let f = Fixture::new(vec![2, 2, 2]);
    let action = LayoutAction::Remove {
        bus: "rec-3-4".into(),
    };
    let mut i = f.intent(action.clone());
    i.expected_config.revision -= 1;
    assert!(matches!(
        f.commands.enqueue_migration(i),
        Err(MigrationError::StaleConfig)
    ));
    let mut i = f.intent(action.clone());
    i.expected_recording.revision += 1;
    assert!(matches!(
        f.commands.enqueue_migration(i),
        Err(MigrationError::StaleRecording)
    ));
    let mut i = f.intent(action);
    i.origin.value = Arc::new((*i.origin.value).clone());
    assert!(matches!(
        f.commands.enqueue_migration(i),
        Err(MigrationError::OriginUnavailable)
    ));
    assert!(f.commands.0.lock().unwrap().pending.is_empty());
}

#[test]
fn partial_resize_retains_actual_counts_and_unknown_metadata_without_false_completion() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    f.target.failure = Some(bus_config::ChannelResizeResult {
        before: 6,
        requested: 4,
        actual: 5,
        failure: Some(bus_config::ResizeFailure::RemoveFailed {
            channel: 4,
            code: -1,
        }),
    });
    let r = f.remove();
    let result = f.finish(&r);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::PartiallyApplied)
    );
    assert_eq!(result.resize.unwrap().actual, 5);
    assert_eq!(result.configuration.local.unwrap().channels, 5);
    assert!(result.configuration.configuration.is_none());
    assert_eq!(
        result.failure,
        Some(MigrationError::Configuration(
            bus_config::ConfigError::ResizeFailed
        ))
    );
}

#[test]
fn saturation_reserves_stop_and_cancel_discards_only_migration_work() {
    let f = Fixture::new(vec![2, 2, 2]);
    while f
        .commands
        .legacy(Opdracht::ZetMute {
            knoop: 10,
            mute: true,
        })
        .is_ok()
    {}
    assert!(matches!(
        f.commands.enqueue_migration(f.intent(LayoutAction::Remove {
            bus: "rec-3-4".into()
        })),
        Err(MigrationError::QueueFull)
    ));
    f.commands.stop();
    assert!(f.commands.0.lock().unwrap().pending.is_empty());
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    let r = f.remove();
    for _ in 1..NORMAL_CAPACITY {
        f.commands
            .legacy(Opdracht::ZetMute {
                knoop: 10,
                mute: true,
            })
            .unwrap();
    }
    f.commands
        .stop_recording(crate::recording::RecordingStop::All)
        .unwrap();
    assert_eq!(
        f.commands.0.lock().unwrap().pending.len(),
        NORMAL_CAPACITY - 1
    );
    assert_eq!(
        r.try_report().unwrap().failure,
        Some(MigrationError::Cancelled)
    );
    assert_eq!(f.target.resizes, 0);
}

#[test]
fn active_or_finalizing_recording_remains_an_explicit_gate() {
    let f = Fixture::new(vec![2, 2, 2]);
    f.commands.refresh_config(f.target.readback(), true, false);
    assert!(matches!(
        f.commands.enqueue_migration(f.intent(LayoutAction::Remove {
            bus: "rec-3-4".into()
        })),
        Err(MigrationError::RecordingActive)
    ));
    let mut f = Fixture::new(vec![2, 2, 2]);
    let r = f.remove();
    f.target.recording = true;
    let result = f.finish(&r);
    assert_eq!(result.failure, Some(MigrationError::RecordingActive));
    assert_eq!(f.target.resizes, 0);
}

#[test]
fn disconnect_or_shutdown_cancels_and_never_replays_a_plan() {
    for shutdown in [false, true] {
        let mut f = Fixture::new(vec![2, 2, 2]);
        f.routed(false);
        let r = f.remove();
        f.tick(false);
        if shutdown {
            f.commands.stop();
        } else {
            f.publish(Momentopname::default());
        }
        let report = r.try_report().unwrap();
        assert!(!report.is_success());
        assert!(report.cancellation_requested);
        assert_eq!(f.target.resizes, 0);
        assert!(f.commands.0.lock().unwrap().migration.is_none());
    }
}

#[test]
fn cancellation_during_claimed_rpc_retains_uncertain_submission_and_prevents_shrink() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    let r = f.remove();
    f.tick(false); // Begin, no RPC yet.
    f.commands.drive_migration_at(Instant::now());
    let mut calls = 0;
    f.commands.drain_with_migration(
        |_| true,
        |_| false,
        |_| {
            calls += 1;
            f.commands
                .stop_recording(crate::recording::RecordingStop::All)
                .unwrap();
            let during = r.try_report().unwrap();
            assert_eq!(during.progress, MigrationProgress::Cancelling);
            assert_eq!(during.in_progress, 1);
            assert!(!during.is_decided());
            LocalExecution::Submitted
        },
        |_| panic!("config"),
        |_| panic!("start"),
        |_| panic!("apply"),
    );
    let result = r.try_report().unwrap();
    assert_eq!(calls, 1);
    assert_eq!(result.submitted, 1);
    assert_eq!(result.in_progress, 0);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::Unconfirmed)
    );
    assert_eq!(result.failure, Some(MigrationError::Cancelled));
    f.tick(false);
    assert_eq!(f.target.resizes, 0);
}

#[test]
fn cancellation_or_disconnect_during_resize_retains_actual_local_result() {
    for disconnect in [false, true] {
        let mut f = Fixture::new(vec![2, 2, 2]);
        let r = f.remove();
        let commands = f.commands.clone();
        let receipt = r.clone();
        f.target.on_resize = Some(Box::new(move || {
            if disconnect {
                commands.publish(Momentopname::default());
            } else {
                commands
                    .stop_recording(crate::recording::RecordingStop::All)
                    .unwrap();
            }
            let during = receipt.try_report().unwrap();
            assert_eq!(during.progress, MigrationProgress::Cancelling);
            assert!(!during.is_decided());
        }));
        let result = f.finish(&r);
        assert_eq!(
            result.progress,
            MigrationProgress::Completed(MigrationOutcome::PartiallyApplied)
        );
        assert_eq!(result.resize.unwrap().actual, 4);
        assert_eq!(result.local_readback.unwrap().channels, 4);
        assert!(result.cancellation_requested);
        assert!(result.configuration.configuration.is_none());
        if disconnect {
            assert!(!result.configuration.connected);
            assert!(
                result.configuration.local.is_none(),
                "old-scope readback must not populate a new session"
            );
        } else {
            assert_eq!(result.configuration.local.unwrap().channels, 4);
        }
    }
}

#[test]
fn final_port_timeout_retains_partial_readback_instead_of_committing_configuration() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    let r = f.remove();
    f.phase(&r, MigrationProgress::AwaitingPorts, false);
    assert_eq!(f.target.channels, 4);
    assert_eq!(
        f.graph().poorten.iter().filter(|p| p.knoop == 900).count(),
        6
    );
    f.commands
        .drive_migration_at(Instant::now() + OBSERVATION_LIMIT + Duration::from_millis(1));
    let result = r.try_report().unwrap();
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::PartiallyApplied)
    );
    assert_eq!(result.failure, Some(MigrationError::ObservationTimedOut));
    assert!(result.ports_observed.is_none());
    assert!(result.configuration.configuration.is_none());
    assert_eq!(result.local_readback.unwrap().channels, 4);
}

#[test]
fn unrelated_graph_and_commands_continue_while_conflicting_mutation_is_rejected() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    let r = f.remove();
    f.commands
        .legacy(Opdracht::Verbind {
            uit: 110,
            naar: 901,
        })
        .unwrap();
    f.commands
        .legacy(Opdracht::ZetMute {
            knoop: 10,
            mute: true,
        })
        .unwrap();
    f.tick(false);
    assert_eq!(
        f.log.borrow().as_slice(),
        &[Opdracht::ZetMute {
            knoop: 10,
            mute: true
        }]
    );
    let mut graph = f.graph();
    graph.knopen.iter_mut().find(|n| n.id == 700).unwrap().naam = "synthetic.unrelated".into();
    f.publish(graph);
    assert!(f.finish(&r).is_success());
}

#[test]
fn unreported_rpc_is_explicitly_unconfirmed_even_without_confirmed_submission() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    let r = f.remove();
    f.tick(false);
    f.commands.drive_migration_at(Instant::now());
    f.commands.drain_with_migration(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |_| panic!("config"),
        |_| panic!("start"),
        |_| panic!("apply"),
    );
    let result = r.try_report().unwrap();
    assert_eq!(result.attempted, 1);
    assert_eq!(result.submitted, 0);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::Unconfirmed)
    );
    assert!(result.failure.is_some());
    assert_eq!(f.target.resizes, 0);
}

#[test]
fn local_gain_readback_mismatch_is_partial_and_does_not_invent_configuration() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.target.ignore_gains = true;
    let r = f.remove();
    let result = f.finish(&r);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::PartiallyApplied)
    );
    assert_eq!(
        result.failure,
        Some(MigrationError::Configuration(
            bus_config::ConfigError::ReadbackMismatch
        ))
    );
    assert!(result.configuration.configuration.is_none());
    assert!(result.configuration.transport.is_none());
    assert_eq!(result.resize.unwrap().actual, 4);
}

#[test]
fn excessive_initial_links_are_rejected_before_any_effect() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    for _ in 0..=MAX_MIGRATION_LINKS {
        f.add_link(110, 905);
    }
    assert!(matches!(
        f.commands.enqueue_migration(f.intent(LayoutAction::Remove {
            bus: "rec-3-4".into()
        })),
        Err(MigrationError::TooManyRoutes)
    ));
    assert!(f.log.borrow().is_empty());
    assert_eq!(f.target.resizes, 0);
    assert!(f.commands.0.lock().unwrap().migration.is_none());
}

#[test]
fn same_named_non_audio_ports_are_excluded_from_the_pure_planner_scope() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    f.routed(false);
    let mut g = f.graph();
    let mut foreign_kind = port(800, 900, "rec_3", true);
    foreign_kind.soort = Soort::Midi;
    g.poorten.insert(0, foreign_kind);
    f.commands.object_added(800);
    f.publish(g);
    let r = f.remove();
    assert!(f.finish(&r).is_success());
    assert!(!f
        .log
        .borrow()
        .iter()
        .any(|o| matches!(o, Opdracht::Verbind { naar: 800, .. })));
    assert!(f.graph().poorten.iter().any(|p| p.id == 800));
}

#[test]
fn repeated_polling_and_an_occupied_queue_do_not_grow_retained_migration_work() {
    let f = Fixture::new(vec![2, 2, 2]);
    let r = f.remove();
    for _ in 0..100_000 {
        f.commands.drive_migration_at(Instant::now());
        assert_eq!(r.try_report().unwrap().progress, MigrationProgress::Queued);
    }
    let state = f.commands.0.lock().unwrap();
    assert_eq!(state.pending.len(), 1);
    let work = state.migration.as_ref().unwrap();
    assert!(work.queued);
    assert!(work.connected.is_empty());
    assert!(work.destroyed.is_empty());
    assert!(work.initial.len() <= MAX_MIGRATION_LINKS);
    drop(state);
    f.commands.cancel_migration_receipt(&r).unwrap();
    assert!(f.commands.0.lock().unwrap().pending.is_empty());
}

#[test]
fn external_local_readback_change_is_retained_before_rejecting_apply() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    let r = f.remove();
    f.phase(&r, MigrationProgress::ApplyingLocal, true);
    f.target.gains[0] = 0.25;
    let result = f.finish(&r);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::Rejected)
    );
    assert_eq!(
        result.failure,
        Some(MigrationError::Configuration(
            bus_config::ConfigError::ReadbackMismatch
        ))
    );
    assert_eq!(result.configuration.local.unwrap().gains[0], 0.25);
    assert_eq!(result.local_readback.unwrap().gains[0], 0.25);
    assert!(result.configuration.configuration.is_none());
    assert_eq!(f.target.resizes, 0);
}

fn replacement_at_final_options(widths: Vec<usize>, action: LayoutAction, id: u32) {
    let mut f = Fixture::new(widths);
    let before = f.commands.config_snapshot().unwrap();
    let receipt = f.start(action);
    let commands = f.commands.clone();
    f.target.on_options = Some(Box::new(move || {
        // Deliberate contract interleaving, not evidence that production's
        // synchronous PipeWire target is reentered by registry callbacks.
        commands.object_removed(id);
        commands.object_added(id);
        let graph = (*commands.current().value).clone();
        commands.publish(graph);
    }));
    let report = f.finish(&receipt);
    assert_eq!(
        report.progress,
        MigrationProgress::Completed(MigrationOutcome::PartiallyApplied)
    );
    assert_eq!(report.failure, Some(MigrationError::InvalidObject));
    assert!(
        report.local_readback.is_some(),
        "retain the target result without assigning it to a replacement"
    );
    assert!(report.configuration.local.is_none());
    assert!(report.configuration.configuration.is_none());
    assert!(report.configuration.transport.is_none());
    assert_ne!(report.configuration.revision, before.revision);
    assert!(f.commands.0.lock().unwrap().migration.is_none());
}
#[test]
fn split_replacement_of_rec_node_in_final_options_cannot_complete_locally() {
    replacement_at_final_options(
        vec![2, 2],
        LayoutAction::Split {
            bus: "rec-1-2".into(),
        },
        900,
    );
}
#[test]
fn split_replacement_of_input_port_in_final_options_cannot_complete_locally() {
    replacement_at_final_options(
        vec![2, 2],
        LayoutAction::Split {
            bus: "rec-1-2".into(),
        },
        901,
    );
}
#[test]
fn join_replacement_of_rec_node_in_final_options_cannot_complete_locally() {
    replacement_at_final_options(
        vec![1, 1, 2],
        LayoutAction::Join {
            bus: "rec-1".into(),
        },
        900,
    );
}
#[test]
fn join_replacement_of_input_port_in_final_options_cannot_complete_locally() {
    replacement_at_final_options(
        vec![1, 1, 2],
        LayoutAction::Join {
            bus: "rec-1".into(),
        },
        901,
    );
}

#[test]
fn nominal_noop_with_final_replacement_is_partial_instead_of_noop_success() {
    replacement_at_final_options(
        vec![2],
        LayoutAction::Remove {
            bus: "rec-1-2".into(),
        },
        901,
    );
}

#[test]
fn shrink_retained_port_replacement_during_options_retains_actual_resize_without_commit() {
    replacement_at_final_options(
        vec![2, 2, 2],
        LayoutAction::Remove {
            bus: "rec-3-4".into(),
        },
        901,
    );
}

#[test]
fn ordinary_final_callback_graph_revisions_preserve_split_and_join_completion() {
    for (widths, action) in [
        (
            vec![2, 2],
            LayoutAction::Split {
                bus: "rec-1-2".into(),
            },
        ),
        (
            vec![1, 1, 2],
            LayoutAction::Join {
                bus: "rec-1".into(),
            },
        ),
    ] {
        let mut f = Fixture::new(widths);
        let r = f.start(action);
        let commands = f.commands.clone();
        f.target.on_options = Some(Box::new(move || {
            let mut graph = (*commands.current().value).clone();
            graph
                .knopen
                .iter_mut()
                .find(|n| n.id == 700)
                .unwrap()
                .beschrijving = Some("synthetic unrelated change".into());
            commands.publish(graph);
        }));
        let result = f.finish(&r);
        assert_eq!(
            result.progress,
            MigrationProgress::Completed(MigrationOutcome::AppliedLocally)
        );
        assert!(result.configuration.configuration.is_some());
        assert_eq!(f.target.resizes, 0);
    }
}

#[test]
fn expected_tail_retirement_inside_resize_remains_a_valid_staged_shrink() {
    let mut f = Fixture::new(vec![2, 2, 2]);
    let r = f.remove();
    let commands = f.commands.clone();
    f.target.on_resize = Some(Box::new(move || {
        let mut graph = (*commands.current().value).clone();
        for id in [905, 906] {
            commands.object_removed(id);
            graph.poorten.retain(|p| p.id != id);
        }
        commands.publish(graph);
    }));
    let result = f.finish(&r);
    assert_eq!(
        result.progress,
        MigrationProgress::Completed(MigrationOutcome::Observed)
    );
    assert_eq!(result.resize.unwrap().actual, 4);
    assert_eq!(result.configuration.local.unwrap().channels, 4);
    assert!(result.configuration.configuration.is_some());
}
