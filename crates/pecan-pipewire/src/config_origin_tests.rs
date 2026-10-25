//! Retained native full-configuration authority. Synthetic graph/local filter only.
use super::*;

// Synthetic owned Rec identity; no server/filter is opened.
fn owned_setup() -> (Commands, Target) {
    let (commands, target) = setup();
    let node = serde_json::from_value(serde_json::json!({
        "id":900,"serial":9900,"naam":crate::opname::BUS_NAAM
    }))
    .unwrap();
    let ports = (0..2)
        .map(|channel| {
            serde_json::from_value(serde_json::json!({
                "id":901 + channel,"knoop":900,"naam":format!("rec_{}",channel+1),
                "ingang":true,"soort":"audio","monitor":false,"fysiek":false
            }))
            .unwrap()
        })
        .collect();
    for id in 900..=902 {
        commands.object_added(id);
    }
    commands.publish(Momentopname {
        verbonden: true,
        knopen: vec![node],
        poorten: ports,
        ..Default::default()
    });
    (commands, target)
}
#[test]
fn full_configuration_rejects_retained_replacement_before_admission() {
    for id in [900, 901] {
        let (commands, _) = owned_setup();
        let original = commands.history_graph(&commands.current()).unwrap();
        let edit = request(&commands, value());
        commands.object_removed(id);
        commands.object_added(id);
        assert_eq!(
            commands
                .enqueue_config_from(&original, edit.clone())
                .unwrap_err(),
            ConfigError::StaleRevision,
            "unpublished replacement is already stale"
        );
        commands.publish((*commands.current().value).clone());
        assert_eq!(commands.config_snapshot().unwrap().revision, edit.expected);
        assert_eq!(
            commands.enqueue_config_from(&original, edit).unwrap_err(),
            ConfigError::StaleRevision
        );
        assert!(commands.0.lock().unwrap().pending.is_empty());
    }
}

fn origin(commands: &Commands) -> pecan_core::history_identity::HistoryGraph {
    commands.history_graph(&commands.current()).unwrap()
}
fn initialize(commands: &Commands, target: &mut Target) {
    let receipt = commands
        .enqueue_config_from(&origin(commands), request(commands, value()))
        .unwrap();
    drain(commands, target);
    assert_eq!(result(&receipt).outcome, LocalConfigOutcome::Applied);
}
fn lower(commands: &Commands) -> IdleConfigIntent {
    let mut value = value();
    value.buses = value.buses.with_db("rec-1-2", -6.).unwrap();
    request(commands, value)
}
fn replace(commands: &Commands, id: u32) {
    commands.object_removed(id);
    commands.object_added(id);
    commands.publish((*commands.current().value).clone());
}

#[test]
fn original_authority_survives_unrelated_revisions_but_not_foreign_or_forged_witnesses() {
    use pecan_core::history_identity::HistoryGraph;
    let (commands, mut target) = owned_setup();
    let (foreign, _) = owned_setup();
    let original = origin(&commands);
    let forged = HistoryGraph::from_registry(
        original.graph().clone(),
        Arc::new(()),
        commands.registry_generations(),
    );
    for invalid in [&origin(&foreign), &forged] {
        assert_eq!(
            commands
                .enqueue_config_from(invalid, request(&commands, value()))
                .unwrap_err(),
            ConfigError::StaleRevision
        );
    }
    for _ in 0..pecan_core::native_command::NATIVE_SOURCE_HISTORY + 2 {
        let mut graph = (*commands.current().value).clone();
        graph.standaard_sink = Some("synthetic-unrelated-output".into());
        commands.publish(graph);
    }
    assert!(
        commands.history_graph(original.graph()).is_err(),
        "publication cache evicted"
    );
    let receipt = commands
        .enqueue_config_from(&original, request(&commands, value()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&receipt).outcome, LocalConfigOutcome::Applied);
    let revision = commands.config_snapshot().unwrap().revision;
    let receipt = commands
        .enqueue_config_from(&original, request(&commands, value()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&receipt).outcome, LocalConfigOutcome::NoOp);
    assert_eq!(result(&receipt).snapshot.revision, revision);
    assert_eq!(target.apply_count, 1);
}

#[test]
fn full_configuration_binds_original_serial_and_channel_assignment() {
    for serial in [false, true] {
        let (commands, mut target) = owned_setup();
        let original = origin(&commands);
        let edit = request(&commands, value());
        let mut graph = (*commands.current().value).clone();
        if serial {
            graph.knopen[0].serial = Some(9901);
        } else {
            graph.poorten[0].naam = "rec_2".into();
            graph.poorten[1].naam = "rec_1".into();
        }
        commands.publish(graph);
        assert_eq!(commands.config_snapshot().unwrap().revision, edit.expected);
        assert_eq!(
            commands
                .enqueue_config_from(&original, edit.clone())
                .unwrap_err(),
            ConfigError::StaleRevision
        );
        let receipt = commands
            .enqueue_config_from(&origin(&commands), edit)
            .unwrap();
        drain(&commands, &mut target);
        assert_eq!(result(&receipt).outcome, LocalConfigOutcome::Applied);
    }
}

#[test]
fn queued_replacement_is_rejected_and_fresh_original_can_apply() {
    for id in [900, 901] {
        let (commands, mut target) = owned_setup();
        let original = origin(&commands);
        let receipt = commands
            .enqueue_config_from(&original, lower(&commands))
            .unwrap();
        replace(&commands, id);
        drain(&commands, &mut target);
        assert_eq!(
            result(&receipt).outcome,
            LocalConfigOutcome::Rejected(ConfigError::StaleRevision)
        );
        assert_eq!(target.apply_count, 0);
        let fresh = commands
            .enqueue_config_from(&origin(&commands), lower(&commands))
            .unwrap();
        drain(&commands, &mut target);
        assert_eq!(result(&fresh).outcome, LocalConfigOutcome::Applied);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Point {
    Readback,
    Resize,
    Gains,
    Options,
    OptionsMatch,
}

#[test]
fn stop_during_a_claimed_call_fences_later_setters_and_retains_partial_status() {
    for point in [Point::Resize, Point::Gains] {
        let (commands, mut target) = owned_setup();
        let receipt = commands
            .enqueue_config_from(&origin(&commands), lower(&commands))
            .unwrap();
        drain(
            &commands,
            &mut Interrupt {
                inner: &mut target,
                hook: Box::new(|p| {
                    if p == point {
                        commands.stop();
                    }
                }),
            },
        );
        let completion = result(&receipt);
        assert_eq!(
            completion.outcome,
            LocalConfigOutcome::PartiallyApplied(ConfigError::Stopped)
        );
        assert!(completion.cancellation_requested);
        assert!(!completion.snapshot.connected);
        assert!(
            target.options.is_none(),
            "stop fences the following options call"
        );
        assert_eq!(target.apply_count, usize::from(point == Point::Gains));
    }
}
// The real target is synchronous. Reentry probes exercise its public local
// callback boundary without pretending a real PipeWire race was reproduced.
struct Interrupt<'a> {
    inner: &'a mut Target,
    hook: Box<dyn Fn(Point) + 'a>,
}
impl ConfigTarget for Interrupt<'_> {
    fn readback(&self) -> Option<BusReadback> {
        let v = self.inner.readback();
        (self.hook)(Point::Readback);
        v
    }
    fn recording(&self) -> bool {
        self.inner.recording()
    }
    fn routed(&self) -> bool {
        self.inner.routed()
    }
    fn resize(&mut self, n: usize) -> ChannelResizeResult {
        let v = self.inner.resize(n);
        (self.hook)(Point::Resize);
        v
    }
    fn gains(&mut self, g: &[f32]) {
        self.inner.gains(g);
        (self.hook)(Point::Gains);
    }
    fn options(&mut self, o: pecan_core::graph::Opties) {
        self.inner.options(o);
        (self.hook)(Point::Options);
    }
    fn options_match(&self, o: &pecan_core::graph::Opties) -> bool {
        let v = self.inner.options_match(o);
        (self.hook)(Point::OptionsMatch);
        v
    }
}

#[test]
fn callback_replacement_fences_remaining_calls_and_never_reports_applied_or_noop() {
    for point in [
        Point::Readback,
        Point::Resize,
        Point::Gains,
        Point::Options,
        Point::OptionsMatch,
    ] {
        for id in [900, 901] {
            let (commands, mut target) = owned_setup();
            initialize(&commands, &mut target);
            let old_count = target.apply_count;
            let receipt = commands
                .enqueue_config_from(&origin(&commands), lower(&commands))
                .unwrap();
            let fired = Cell::new(false);
            drain(
                &commands,
                &mut Interrupt {
                    inner: &mut target,
                    hook: Box::new(|p| {
                        if p == point && !fired.replace(true) {
                            replace(&commands, id);
                        }
                    }),
                },
            );
            assert!(fired.get());
            let completion = result(&receipt);
            assert_eq!(
                completion.outcome,
                if point == Point::Readback {
                    LocalConfigOutcome::Rejected(ConfigError::StaleRevision)
                } else {
                    LocalConfigOutcome::PartiallyApplied(ConfigError::StaleRevision)
                }
            );
            if point == Point::Readback || point == Point::Resize {
                assert_eq!(target.apply_count, old_count, "no later gain setter");
            }
            if point != Point::Readback {
                assert!(completion.snapshot.local.is_none());
                assert!(completion.snapshot.configuration.is_none());
                assert!(completion.snapshot.transport.is_none());
            }
        }
    }
    let (commands, mut target) = owned_setup();
    initialize(&commands, &mut target);
    let receipt = commands
        .enqueue_config_from(&origin(&commands), request(&commands, value()))
        .unwrap();
    drain(
        &commands,
        &mut Interrupt {
            inner: &mut target,
            hook: Box::new(|p| {
                if p == Point::OptionsMatch {
                    replace(&commands, 900);
                }
            }),
        },
    );
    assert_eq!(
        result(&receipt).outcome,
        LocalConfigOutcome::Rejected(ConfigError::StaleRevision),
        "even a local no-op must retain the caller's identity"
    );
}

#[test]
fn final_callback_cannot_overwrite_newer_configuration_or_reconnect() {
    for disconnect in [false, true] {
        let (commands, mut target) = owned_setup();
        initialize(&commands, &mut target);
        let receipt = commands
            .enqueue_config_from(&origin(&commands), lower(&commands))
            .unwrap();
        let saved = RefCell::new(None);
        drain(
            &commands,
            &mut Interrupt {
                inner: &mut target,
                hook: Box::new(|p| {
                    if p == Point::Options {
                        if disconnect {
                            commands.publish(Momentopname::default());
                        } else {
                            commands.refresh_config(
                                Some(BusReadback {
                                    node: Some(900),
                                    channels: 2,
                                    gains: vec![0.25; 2],
                                }),
                                false,
                                true,
                            );
                        }
                        *saved.borrow_mut() = commands.config_snapshot();
                    }
                }),
            },
        );
        assert_eq!(result(&receipt).snapshot, saved.into_inner().unwrap());
        assert_eq!(
            result(&receipt).outcome,
            LocalConfigOutcome::PartiallyApplied(if disconnect {
                ConfigError::Disconnected
            } else {
                ConfigError::StaleRevision
            })
        );
    }
}

fn publish_growth(commands: &Commands, channels: usize) {
    let mut graph = (*commands.current().value).clone();
    for channel in graph.poorten.len()..channels {
        let mut port = graph.poorten[0].clone();
        port.id = 901 + channel as u32;
        port.naam = format!("rec_{}", channel + 1);
        commands.object_added(port.id);
        graph.poorten.push(port);
    }
    commands.publish(graph);
}

fn publish_route(commands: &Commands) {
    let mut graph = (*commands.current().value).clone();
    let mut node = graph.knopen[0].clone();
    node.id = 40;
    node.serial = Some(4040);
    node.naam = "synthetic-source".into();
    let mut port = graph.poorten[0].clone();
    port.id = 41;
    port.knoop = 40;
    port.ingang = false;
    port.naam = "out_1".into();
    graph.knopen.push(node);
    graph.poorten.push(port);
    graph.kabels.push(pecan_core::graph::Kabel {
        id: 42,
        uit_knoop: 40,
        uit_poort: 41,
        in_knoop: 900,
        in_poort: 901,
        client: None,
    });
    for id in 40..=42 {
        commands.object_added(id);
    }
    commands.publish(graph);
}

#[test]
fn newly_observed_route_fences_layout_growth_despite_an_older_target_query() {
    let (commands, mut target) = owned_setup();
    initialize(&commands, &mut target);
    publish_route(&commands);
    let mut larger = value();
    larger.buses.widths = vec![2, 2];
    assert_eq!(
        commands
            .enqueue_config_from(&origin(&commands), request(&commands, larger))
            .unwrap_err(),
        ConfigError::LayoutMigrationRequired
    );
    target.routed = true;
    let receipt = commands
        .enqueue_config_from(&origin(&commands), lower(&commands))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(
        result(&receipt).outcome,
        LocalConfigOutcome::Applied,
        "routes do not prohibit idle in-place gains/options"
    );
    for point in [Point::Readback, Point::Resize] {
        let (commands, mut target) = owned_setup();
        let mut larger = value();
        larger.buses.widths = vec![2, 2];
        let receipt = commands
            .enqueue_config_from(&origin(&commands), request(&commands, larger))
            .unwrap();
        let fired = Cell::new(false);
        drain(
            &commands,
            &mut Interrupt {
                inner: &mut target,
                hook: Box::new(|p| {
                    if p == point && !fired.replace(true) {
                        publish_route(&commands);
                    }
                }),
            },
        );
        assert_eq!(
            result(&receipt).outcome,
            if point == Point::Readback {
                LocalConfigOutcome::Rejected(ConfigError::LayoutMigrationRequired)
            } else {
                LocalConfigOutcome::PartiallyApplied(ConfigError::LayoutMigrationRequired)
            }
        );
        assert_eq!(target.apply_count, 0);
    }
}
#[test]
fn unrouted_growth_allows_delayed_or_immediate_port_publication_and_typed_partial_retry() {
    for immediate in [false, true] {
        let (commands, mut target) = owned_setup();
        let mut larger = value();
        larger.buses.widths = vec![2, 2];
        let receipt = commands
            .enqueue_config_from(&origin(&commands), request(&commands, larger))
            .unwrap();
        let fired = Cell::new(false);
        drain(
            &commands,
            &mut Interrupt {
                inner: &mut target,
                hook: Box::new(|p| {
                    if immediate && p == Point::Resize && !fired.replace(true) {
                        publish_growth(&commands, 4);
                    }
                }),
            },
        );
        assert_eq!(result(&receipt).outcome, LocalConfigOutcome::Applied);
        assert_eq!(result(&receipt).resize.unwrap().actual, 4);
        assert_eq!(target.ports.len(), 4);
        assert_eq!(
            commands.current().value.poorten.len(),
            if immediate { 4 } else { 2 }
        );
    }
    let (commands, mut target) = owned_setup();
    let mut larger = value();
    larger.buses.widths = vec![2, 2];
    target.fail_add = Some(3);
    let receipt = commands
        .enqueue_config_from(&origin(&commands), request(&commands, larger.clone()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(
        result(&receipt).outcome,
        LocalConfigOutcome::PartiallyApplied(ConfigError::ResizeFailed)
    );
    assert_eq!(result(&receipt).resize.unwrap().actual, 3);
    assert_eq!(
        commands
            .enqueue_config_from(&origin(&commands), request(&commands, larger.clone()))
            .unwrap_err(),
        ConfigError::Unavailable,
        "retry waits for a witness of the actual three-port prefix"
    );
    publish_growth(&commands, 3);
    target.fail_add = None;
    let receipt = commands
        .enqueue_config_from(&origin(&commands), request(&commands, larger))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&receipt).outcome, LocalConfigOutcome::Applied);
}

#[test]
fn captured_configuration_keeps_shrink_routed_layout_and_recording_gates() {
    for mode in 0..4 {
        let (commands, mut target) = owned_setup();
        initialize(&commands, &mut target);
        let mut next = value();
        match mode {
            0 => next.buses.widths = vec![1],
            1 => {
                next.buses.widths = vec![2, 2];
                target.routed = true;
            }
            2 => {
                next.buses.widths = vec![1, 1];
                target.routed = true;
            }
            _ => target.recording = true,
        }
        let receipt = commands
            .enqueue_config_from(&origin(&commands), request(&commands, next))
            .unwrap();
        drain(&commands, &mut target);
        assert_eq!(
            result(&receipt).outcome,
            LocalConfigOutcome::Rejected(if mode == 3 {
                ConfigError::RecordingActive
            } else {
                ConfigError::LayoutMigrationRequired
            })
        );
        assert_eq!(target.apply_count, 1);
        assert_eq!(target.ports.len(), 2);
    }
}

#[test]
fn retained_configuration_uses_existing_capacity_stop_and_disconnect_fences() {
    for stop in [false, true] {
        let (commands, mut target) = owned_setup();
        let original = origin(&commands);
        let mut receipts = Vec::new();
        for _ in 0..MAX_CONFIG_PENDING {
            receipts.push(
                commands
                    .enqueue_config_from(&original, request(&commands, value()))
                    .unwrap(),
            );
        }
        assert_eq!(
            commands
                .enqueue_config_from(&original, request(&commands, value()))
                .unwrap_err(),
            ConfigError::QueueFull
        );
        for _ in MAX_CONFIG_PENDING..NORMAL_CAPACITY {
            commands.legacy(Opdracht::Start).unwrap();
        }
        commands.legacy(Opdracht::Stop).unwrap();
        assert_eq!(commands.0.lock().unwrap().pending.len(), CAPACITY);
        let graph = (*commands.current().value).clone();
        if stop {
            commands.stop();
        } else {
            commands.publish(Momentopname::default());
        }
        assert!(commands.0.lock().unwrap().pending.is_empty());
        for receipt in receipts {
            assert_eq!(
                result(&receipt).outcome,
                LocalConfigOutcome::Rejected(if stop {
                    ConfigError::Stopped
                } else {
                    ConfigError::Disconnected
                })
            );
        }
        drain(&commands, &mut target);
        assert_eq!(target.apply_count, 0);
        if !stop {
            for id in 900..=902 {
                commands.object_added(id);
            }
            commands.publish(graph);
            commands.refresh_config(target.readback(), false, false);
            assert_eq!(
                commands
                    .enqueue_config_from(&original, request(&commands, value()))
                    .unwrap_err(),
                ConfigError::StaleRevision
            );
            initialize(&commands, &mut target);
        }
    }
}
