//! Hand-reviewed expectations traced from the existing TypeScript policy.
use pecan_core::{
    backend::{Stamp, Stamped},
    domain::{
        audio,
        buses::{BusConfig, BusError, Coupling},
        history::{Action, Controller, DomainError, Input, HISTORY_LIMIT},
        levels::{self, Meter},
        mixer::{self, Mixer, RouteState},
    },
    graph::{Kabel, Momentopname, Opdracht},
};
use serde_json::{json, Value};
use std::sync::Arc;
fn graph() -> Momentopname {
    serde_json::from_str(include_str!("../../../fixtures/domain/audio-policy.json")).unwrap()
}
fn config() -> BusConfig {
    serde_json::from_str(include_str!("../../../fixtures/domain/bus-config.json")).unwrap()
}
fn model(graph: &Momentopname) -> Mixer {
    Mixer::build(graph, &config().definitions().unwrap())
}
fn summary(items: &[mixer::Strip]) -> Value {
    json!(items.iter().map(|s|json!({"node":s.node.id,"key":s.key,"name":s.name,"channels":s.channels,"ports":s.ports.iter().map(|p|p.id).collect::<Vec<_>>(),"targetable":s.targetable,"bus":s.bus.as_ref().map(|b|&b.key)})).collect::<Vec<_>>())
}
fn snapshot(graph: Momentopname, epoch: u64, revision: u64) -> Input {
    Input::Graph(Stamped {
        stamp: Stamp { epoch, revision },
        value: Arc::new(graph),
    })
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}")
}
fn execute(
    controller: &mut Controller,
    label: &str,
    action: Action,
) -> pecan_core::domain::history::CommandPlan {
    controller
        .reduce(Input::Execute {
            label: label.into(),
            action,
        })
        .unwrap()
        .unwrap()
}
fn result(controller: &mut Controller, ticket: u64, epoch: u64, revision: u64) {
    controller
        .reduce(Input::CommandResult {
            ticket,
            result: Ok(Stamp { epoch, revision }),
        })
        .unwrap();
}

#[test]
fn audio_fixture_matches_hand_reviewed_strip_group_and_manual_route_expectations() {
    let graph = graph();
    let mixer = model(&graph);
    let layout = audio::layout(&graph);
    let expected: Value = serde_json::from_str(include_str!(
        "../../../fixtures/domain/audio-policy-expected.json"
    ))
    .unwrap();
    let actual = json!({"apps":summary(&mixer.apps),"hardware":summary(&mixer.hardware),"destinations":summary(&mixer.destinations),"manual":mixer.manual,
 "groups":layout.groups.iter().map(|g|json!({"title":g.title,"keys":g.modules.iter().map(|m|&m.key).collect::<Vec<_>>()})).collect::<Vec<_>>(),"visible_links":layout.links.iter().map(|l|l.id).collect::<Vec<_>>(),"hidden":layout.hidden});
    assert_eq!(actual, expected);
    let playing = &layout.groups[0].modules;
    assert!(!playing[0].silent);
    assert!(playing[1].silent);
    assert_eq!(playing[0].subtitle, "playing · Fixture tab");
    assert_eq!(playing[0].icon, "wereld");
    assert_eq!(playing[0].meter_channels, vec![vec![0], vec![1]]);
    let hardware = layout
        .groups
        .iter()
        .find(|g| g.title == "Hardware out")
        .unwrap();
    assert_eq!(
        hardware.modules[0]
            .ports
            .iter()
            .map(|p| p.label.as_str())
            .collect::<Vec<_>>(),
        ["Out 1", "Out 2", "Out 3", "Out 4"]
    );
    assert_eq!(
        hardware.modules[0].meter_channels,
        vec![vec![0, 2], vec![1, 3]]
    );
    assert_eq!(mixer.apps[1].key, "firefox|apps|output_FL,output_FR#1");
    assert_eq!(mixer.manual_count(), 7);
}

#[test]
fn channel_pairing_preserves_mono_fan_in_out_and_name_before_order() {
    let mut graph = graph();
    let m = model(&graph);
    assert_eq!(
        mixer::expected_pairs(&m.apps[2], &m.destinations[1]),
        [[220, 100], [220, 101]]
    );
    assert_eq!(
        mixer::expected_pairs(&m.apps[0], &m.destinations[4]),
        [[200, 302], [201, 302]]
    );
    let mut destination = m.destinations[1].clone();
    destination.ports[0].kanaal = Some("FR".into());
    destination.ports[1].kanaal = Some("FL".into());
    assert_eq!(
        mixer::expected_pairs(&m.apps[0], &destination),
        [[200, 101], [201, 100]]
    );
    let mut source = m.apps[0].clone();
    for p in &mut source.ports {
        p.kanaal = None
    }
    for p in &mut destination.ports {
        p.kanaal = None
    }
    // JS `null === null` matches the first channel; preserve this surprising
    // behavior until a separately reviewed behavior change replaces it.
    assert_eq!(
        mixer::expected_pairs(&source, &destination),
        [[200, 100], [201, 100]]
    );
    graph.poorten.reverse();
    let shuffled = model(&graph);
    assert_eq!(summary(&shuffled.destinations), summary(&m.destinations));
}

#[test]
fn route_classification_and_toggle_plans_match_auto_manual_custom_and_rec_semantics() {
    let graph = graph();
    let m = model(&graph);
    assert_eq!(
        m.route(&graph, &m.apps[0], &m.destinations[1]).state,
        RouteState::On { manual: false }
    );
    assert_eq!(
        m.route(&graph, &m.apps[0], &m.destinations[2]).state,
        RouteState::Custom {
            explanation: "only FL → Out 3".into()
        }
    );
    assert_eq!(
        m.route(&graph, &m.apps[0], &m.destinations[3]).state,
        RouteState::On { manual: false }
    );
    assert_eq!(
        m.route(&graph, &m.apps[2], &m.destinations[0]).state,
        RouteState::On { manual: true }
    );
    assert!(m
        .plan_toggle(&graph, &m.apps[0], &m.destinations[2])
        .is_empty());
    assert_eq!(
        m.plan_toggle(&graph, &m.apps[0], &m.destinations[1]),
        [
            Opdracht::Ontkoppel { kabel: 1 },
            Opdracht::Ontkoppel { kabel: 2 }
        ]
    );
    // The default target did not change: restore links directly, rather than
    // setting the same metadata and expecting WirePlumber to reconsider it.
    assert_eq!(
        m.plan_toggle(&graph, &m.apps[1], &m.destinations[1]),
        [
            Opdracht::Verbind {
                uit: 210,
                naar: 100
            },
            Opdracht::Verbind {
                uit: 211,
                naar: 101
            }
        ]
    );
    // Whole alternate target: metadata; later hardware pair: explicit links.
    assert_eq!(
        m.plan_toggle(&graph, &m.apps[1], &m.destinations[0]),
        [Opdracht::ZetDoel {
            knoop: 21,
            doel: Some("50".into())
        }]
    );
    assert_eq!(
        m.plan_toggle(&graph, &m.apps[1], &m.destinations[2]),
        [
            Opdracht::Verbind {
                uit: 210,
                naar: 102
            },
            Opdracht::Verbind {
                uit: 211,
                naar: 103
            }
        ]
    );
    assert_eq!(
        m.plan_toggle(&graph, &m.hardware[0], &m.destinations[7]),
        [
            Opdracht::Verbind {
                uit: 110,
                naar: 400
            },
            Opdracht::Verbind {
                uit: 111,
                naar: 401
            }
        ]
    );
}

#[test]
fn back_to_auto_plan_leaves_rec_routes_and_restores_detached_default_streams() {
    let m = model(&graph());
    assert_eq!(
        m.plan_reset_auto(),
        [
            Opdracht::Ontkoppel { kabel: 3 },
            Opdracht::Ontkoppel { kabel: 9 },
            Opdracht::Ontkoppel { kabel: 12 },
            Opdracht::Ontkoppel { kabel: 13 },
            Opdracht::ZetDoel {
                knoop: 22,
                doel: None
            },
            Opdracht::Verbind {
                uit: 210,
                naar: 100
            },
            Opdracht::Verbind {
                uit: 211,
                naar: 101
            },
            Opdracht::Verbind {
                uit: 110,
                naar: 400
            },
            Opdracht::Verbind {
                uit: 111,
                naar: 401
            }
        ]
    );
}

#[test]
fn bus_layout_coupling_arming_and_gains_preserve_existing_state_rules() {
    let c = config();
    let definitions = c.definitions().unwrap();
    assert_eq!(
        definitions.iter().map(|b| b.coupling).collect::<Vec<_>>(),
        [
            Some(Coupling::Split),
            Some(Coupling::Join),
            None,
            Some(Coupling::Split)
        ]
    );
    let split = c.change_coupling(0).unwrap();
    assert_eq!(split.widths, [1, 1, 1, 1, 2]);
    assert_eq!(split.state("rec-1"), c.state("rec-1-2"));
    assert_eq!(split.state("rec-2"), c.state("rec-1-2"));
    let joined = c.change_coupling(1).unwrap();
    assert_eq!(joined.widths, [2, 2, 2]);
    assert_eq!(joined.state("rec-3-4"), c.state("rec-3"));
    let only = c.arm("rec-4", false).unwrap();
    assert_eq!(
        only.armed()
            .unwrap()
            .iter()
            .map(|b| b.key.as_str())
            .collect::<Vec<_>>(),
        ["rec-4"]
    );
    assert!(only
        .arm("rec-4", false)
        .unwrap()
        .armed()
        .unwrap()
        .is_empty());
    assert_eq!(only.arm("rec-1-2", true).unwrap().armed().unwrap().len(), 2);
    let Opdracht::ZetBus { kanalen, gain } = c.backend_config().unwrap() else {
        panic!("bus command")
    };
    assert_eq!(kanalen, 6);
    close(f64::from(gain[2]), 10_f64.powf(-6.0 / 20.0));
    assert_eq!(gain[3], 0.0);
    close(f64::from(gain[4]), 10_f64.powf(6.0 / 20.0));
    let added = c.add().unwrap();
    assert_eq!(added.widths, [2, 1, 1, 2, 2]);
    assert_eq!(added.state("rec-7-8").db, 0.0);
    assert!(!added.state("rec-7-8").armed);
}

#[test]
fn bus_removal_migrates_links_before_removal_and_resize_and_keeps_fader_arming() {
    let edit = config().remove(1, &graph()).unwrap();
    assert_eq!(edit.config.widths, [2, 1, 2]);
    assert_eq!(edit.config.state("rec-3"), config().state("rec-4"));
    assert_eq!(edit.config.state("rec-4-5"), config().state("rec-5-6"));
    assert_eq!(
        &edit.commands[..5],
        [
            Opdracht::Verbind {
                uit: 110,
                naar: 303
            },
            Opdracht::Verbind {
                uit: 111,
                naar: 304
            },
            Opdracht::Ontkoppel { kabel: 6 },
            Opdracht::Ontkoppel { kabel: 7 },
            Opdracht::Ontkoppel { kabel: 8 }
        ]
    );
    let Opdracht::ZetBus { kanalen, gain } = edit.commands.last().unwrap() else {
        panic!("resize last")
    };
    assert_eq!(*kanalen, 5);
    assert_eq!(&gain[..3], &[1.0, 1.0, 0.0]);
    assert!(BusConfig::default()
        .remove(0, &graph())
        .unwrap()
        .commands
        .is_empty());
    assert_eq!(config().remove(99, &graph()), Err(BusError::InvalidIndex));
    let mut bad = config();
    bad.widths = vec![3];
    assert_eq!(bad.definitions(), Err(BusError::InvalidWidth));
    bad.widths.clear();
    assert_eq!(bad.backend_config(), Err(BusError::Empty));
    let mut bad = config();
    bad.states.get_mut("rec-3").unwrap().db = f64::NAN;
    assert_eq!(bad.backend_config(), Err(BusError::NonFiniteGain));
}

#[test]
fn fader_curve_and_selected_channel_volume_match_legacy_math() {
    for (position, db) in [
        (0.0, -90.0),
        (0.04, -60.0),
        (0.2, -40.0),
        (0.42, -20.0),
        (0.62, -10.0),
        (0.76, -4.0),
        (0.86, 0.0),
        (1.0, 6.0),
    ] {
        close(levels::position_to_db(position), db);
        close(levels::db_to_position(db), position);
    }
    assert_eq!(levels::position_to_db(0.005), -90.0);
    close(levels::position_to_db(0.0051), -86.175);
    close(levels::position_to_db(0.69), -7.0);
    assert_eq!(levels::db_to_linear(-90.0), 0.0);
    let m = model(&graph());
    let hardware = &m.destinations[2];
    close(
        mixer::strip_db(hardware).unwrap(),
        20.0_f64 * (0.375_f64).log10(),
    );
    assert_eq!(mixer::with_db(hardware, 0.0).unwrap(), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(
        mixer::with_db(hardware, -90.0).unwrap(),
        [1.0, 1.0, 0.0, 0.0]
    );
    assert!(mixer::with_db(hardware, f64::NAN).is_none());
}

#[test]
fn meter_decay_hold_clip_and_elapsed_cap_match_frontend_boundaries() {
    close(levels::linear_to_meter(0.001), 0.0);
    close(levels::linear_to_meter(0.01), 1.0 / 3.0);
    close(levels::linear_to_meter(1.0), 1.0);
    let mut meter = Meter::default();
    meter.tick(&[0.1, 1.0], &[1], 0.0, 0.0);
    close(meter.level, 1.0);
    assert!(meter.clipped);
    meter.tick(&[0.0], &[0, 99], 100.0, 1000.0);
    close(meter.level, 0.96);
    close(meter.held, 1.0);
    meter.tick(&[0.0], &[0], 1500.0, 10.0);
    close(meter.held, 1.0);
    meter.tick(&[0.0], &[0], 1501.0, 1.0);
    close(meter.held, 0.0);
    assert!(meter.clipped);
    meter.tick(&[0.0], &[0], 2000.0, 1.0);
    assert!(!meter.clipped);
    let before = meter.clone();
    meter.tick(&[f32::NAN], &[0], f64::NAN, 1.0);
    assert_eq!(meter, before);
}

#[test]
fn history_waits_for_acknowledged_graph_and_re_resolves_recreated_link_ids() {
    let mut controller = Controller::default();
    let mut graph = graph();
    controller.reduce(snapshot(graph.clone(), 1, 1)).unwrap();
    let pending = execute(
        &mut controller,
        "Disconnect",
        Action::Disconnect(vec![[200, 100]]),
    );
    assert_eq!(pending.commands, [Opdracht::Ontkoppel { kabel: 1 }]);
    assert_eq!(controller.reduce(Input::Undo), Err(DomainError::Busy));
    result(&mut controller, pending.ticket, 1, 2);
    assert!(controller.busy());
    assert_eq!(controller.labels(), (None, None));
    graph.kabels.retain(|l| l.id != 1);
    controller.reduce(snapshot(graph.clone(), 1, 2)).unwrap();
    assert!(!controller.busy());
    assert_eq!(controller.labels(), (Some("Disconnect"), None));
    let undo = controller.reduce(Input::Undo).unwrap().unwrap();
    assert_eq!(
        undo.commands,
        [Opdracht::Verbind {
            uit: 200,
            naar: 100
        }]
    );
    graph.kabels.push(Kabel {
        id: 901,
        uit_knoop: 20,
        uit_poort: 200,
        in_knoop: 10,
        in_poort: 100,
        client: Some(999),
    });
    controller.reduce(snapshot(graph.clone(), 1, 3)).unwrap();
    assert!(controller.busy());
    result(&mut controller, undo.ticket, 1, 3);
    let redo = controller.reduce(Input::Redo).unwrap().unwrap();
    assert_eq!(redo.commands, [Opdracht::Ontkoppel { kabel: 901 }]);
    result(&mut controller, redo.ticket, 1, 4);
    graph.kabels.retain(|l| l.id != 901);
    controller.reduce(snapshot(graph, 1, 4)).unwrap();
    assert_eq!(controller.labels(), (Some("Disconnect"), None));
}

#[test]
fn history_failures_and_reconnect_do_not_replay_stale_object_ids() {
    let mut c = Controller::default();
    let graph = graph();
    assert_eq!(c.reduce(Input::Undo), Err(DomainError::Disconnected));
    c.reduce(snapshot(graph.clone(), 4, 1)).unwrap();
    let first = execute(
        &mut c,
        "Mute",
        Action::Mute {
            node: 20,
            muted: true,
        },
    );
    assert_eq!(
        c.reduce(Input::CommandResult {
            ticket: first.ticket,
            result: Ok(Stamp {
                epoch: 3,
                revision: 8
            })
        }),
        Err(DomainError::InvalidAcknowledgement)
    );
    assert!(c.busy());
    assert_eq!(
        c.reduce(Input::CommandResult {
            ticket: first.ticket,
            result: Err("denied".into())
        }),
        Err(DomainError::Backend("denied".into()))
    );
    assert_eq!(c.lengths(), (0, 0));
    let second = execute(
        &mut c,
        "Mute",
        Action::Mute {
            node: 20,
            muted: true,
        },
    );
    result(&mut c, second.ticket, 4, 2);
    c.reduce(snapshot(graph.clone(), 4, 2)).unwrap();
    assert_eq!(c.lengths(), (1, 0));
    let pending = c.reduce(Input::Undo).unwrap().unwrap();
    c.reduce(snapshot(graph.clone(), 5, 1)).unwrap();
    assert_eq!(c.lengths(), (0, 0));
    assert!(!c.busy());
    assert_eq!(
        c.reduce(Input::CommandResult {
            ticket: pending.ticket,
            result: Ok(Stamp {
                epoch: 4,
                revision: 3
            })
        }),
        Err(DomainError::UnknownTicket)
    );
    c.reduce(snapshot(graph, 4, 100)).unwrap();
    assert_eq!(c.graph().unwrap().stamp.epoch, 5);
    let mut disconnected = c.graph().unwrap().value.as_ref().clone();
    disconnected.verbonden = false;
    c.reduce(snapshot(disconnected, 6, 1)).unwrap();
    assert_eq!(c.reduce(Input::Redo), Err(DomainError::Disconnected));
}

#[test]
fn history_bounds_records_gesture_once_and_invalidates_redo_on_new_action() {
    let mut c = Controller::default();
    let graph = graph();
    c.reduce(snapshot(graph.clone(), 1, 1)).unwrap();
    for i in 0..HISTORY_LIMIT + 5 {
        c.reduce(Input::RecordVolume {
            node: 20,
            before: vec![0.5, 0.5],
            after: vec![i as f32 + 1.0, 1.0],
        })
        .unwrap();
    }
    assert_eq!(c.lengths(), (HISTORY_LIMIT, 0));
    c.reduce(Input::RecordVolume {
        node: 20,
        before: vec![1.0],
        after: vec![1.0 + 1e-7],
    })
    .unwrap();
    assert_eq!(c.lengths(), (HISTORY_LIMIT, 0));
    let undo = c.reduce(Input::Undo).unwrap().unwrap();
    result(&mut c, undo.ticket, 1, 1);
    assert!(c.busy());
    c.reduce(snapshot(graph.clone(), 1, 2)).unwrap();
    assert_eq!(c.lengths(), (HISTORY_LIMIT - 1, 1));
    c.reduce(Input::RecordVolume {
        node: 20,
        before: vec![1.0],
        after: vec![0.0],
    })
    .unwrap();
    assert_eq!(c.lengths(), (HISTORY_LIMIT, 0));
}

#[test]
fn back_to_auto_undo_captures_targets_and_reconstructs_old_pairs() {
    let mut c = Controller::default();
    c.buses = config();
    let graph = graph();
    c.reduce(snapshot(graph.clone(), 1, 1)).unwrap();
    let reset = execute(&mut c, "Back to auto", Action::BackToAuto);
    let mut after = graph.clone();
    after.kabels.retain(|l| ![3, 9, 12, 13].contains(&l.id));
    after.knopen.iter_mut().find(|n| n.id == 22).unwrap().doel = None;
    for (id, (from, to, source, dest)) in [
        (210, 100, 21, 10),
        (211, 101, 21, 10),
        (110, 400, 11, 40),
        (111, 401, 11, 40),
    ]
    .into_iter()
    .enumerate()
    {
        after.kabels.push(Kabel {
            id: 1000 + id as u32,
            uit_knoop: source,
            uit_poort: from,
            in_knoop: dest,
            in_poort: to,
            client: Some(999),
        });
    }
    result(&mut c, reset.ticket, 1, 2);
    c.reduce(snapshot(after, 1, 2)).unwrap();
    let undo = c.reduce(Input::Undo).unwrap().unwrap();
    assert_eq!(
        undo.commands,
        [
            Opdracht::Ontkoppel { kabel: 1000 },
            Opdracht::Ontkoppel { kabel: 1001 },
            Opdracht::Ontkoppel { kabel: 1002 },
            Opdracht::Ontkoppel { kabel: 1003 },
            Opdracht::Verbind {
                uit: 200,
                naar: 102
            },
            Opdracht::Verbind {
                uit: 600,
                naar: 400
            },
            Opdracht::Verbind {
                uit: 220,
                naar: 500
            },
            Opdracht::Verbind {
                uit: 220,
                naar: 501
            },
            Opdracht::ZetDoel {
                knoop: 22,
                doel: Some("50".into())
            }
        ]
    );
}

#[test]
fn disconnected_projection_and_planner_never_expose_stale_graph_commands() {
    let mut graph = graph();
    graph.verbonden = false;
    assert!(audio::layout(&graph).groups.is_empty());
    assert!(model(&graph).apps.is_empty());
    assert_eq!(
        pecan_core::domain::history::plan(&Action::Connect([200, 100]), &graph, &config()),
        Err(DomainError::Disconnected)
    );
}

#[test]
fn alternate_capture_target_and_absent_serial_follow_link_fallback_rules() {
    let mut graph = graph();
    graph.kabels.retain(|l| l.id != 9);
    graph.knopen.iter_mut().find(|n| n.id == 50).unwrap().klasse = Some("Audio/Source".into());
    for port in graph.poorten.iter_mut().filter(|p| p.knoop == 50) {
        port.ingang = false;
    }
    let m = model(&graph);
    let source = m.hardware.iter().find(|s| s.node.id == 50).unwrap();
    let dest = m.destinations.iter().find(|s| s.node.id == 40).unwrap();
    assert_eq!(
        m.plan_toggle(&graph, source, dest),
        [Opdracht::ZetDoel {
            knoop: 40,
            doel: Some("50".into())
        }]
    );
    graph.knopen.iter_mut().find(|n| n.id == 50).unwrap().serial = None;
    let m = model(&graph);
    let source = m.hardware.iter().find(|s| s.node.id == 50).unwrap();
    let dest = m.destinations.iter().find(|s| s.node.id == 40).unwrap();
    assert_eq!(
        m.plan_toggle(&graph, source, dest),
        [
            Opdracht::Verbind {
                uit: 500,
                naar: 400
            },
            Opdracht::Verbind {
                uit: 501,
                naar: 401
            }
        ]
    );
}

#[test]
fn audio_labels_odd_aux_pairs_and_missing_bus_ports_keep_source_contract() {
    let mut graph = graph();
    let mut port = graph.poorten.iter().find(|p| p.id == 100).unwrap().clone();
    port.naam = "rec_001".into();
    assert_eq!(audio::port_label(&port), "Rec 001");
    port.naam = "playback_UNK".into();
    port.kanaal = None;
    assert_eq!(audio::port_label(&port), "UNK");
    graph.poorten.retain(|p| p.id != 103 && p.id != 301);
    let m = model(&graph);
    assert!(!m.destinations.iter().any(|s| s.key == "bus|rec-1-2"));
    let last = m
        .destinations
        .iter()
        .find(|s| s.node.id == 10 && s.channels == vec![2])
        .unwrap();
    assert_eq!(last.name, "Out 3");
    assert!(!last.targetable);
    let mut node = graph.knopen[0].clone();
    node.beschrijving = Some("A very long description for the hardware".into());
    node.bijnaam = Some("Short".into());
    assert_eq!(audio::device_title(&node), "Short");
    let mut disconnected = graph;
    disconnected.verbonden = false;
    let edit = config().remove(1, &disconnected).unwrap();
    assert_eq!(edit.commands.len(), 1);
    assert!(matches!(edit.commands[0], Opdracht::ZetBus { .. }));
}

#[test]
fn route_intents_are_idempotent_and_custom_routes_are_not_mutated() {
    let graph = graph();
    let m = model(&graph);
    let desired = Action::Route {
        source: m.apps[1].key.clone(),
        destination: m.destinations[1].key.clone(),
        enabled: true,
    };
    let commands = pecan_core::domain::history::plan(&desired, &graph, &config()).unwrap();
    assert_eq!(commands.len(), 2);
    let custom = Action::Route {
        source: m.apps[0].key.clone(),
        destination: m.destinations[2].key.clone(),
        enabled: true,
    };
    assert!(
        pecan_core::domain::history::plan(&custom, &graph, &config())
            .unwrap()
            .is_empty()
    );
    let already = Action::Route {
        source: m.apps[0].key.clone(),
        destination: m.destinations[1].key.clone(),
        enabled: true,
    };
    assert!(
        pecan_core::domain::history::plan(&already, &graph, &config())
            .unwrap()
            .is_empty()
    );
    let mut c = Controller::default();
    c.buses = config();
    c.reduce(snapshot(graph, 1, 1)).unwrap();
    assert_eq!(
        c.reduce(Input::Execute {
            label: "Already on".into(),
            action: already
        })
        .unwrap(),
        None
    );
    assert!(!c.busy());
    assert_eq!(c.lengths(), (1, 0));
}

#[test]
fn batch_plans_use_one_snapshot_and_require_independent_operations() {
    let graph = graph();
    let commands = pecan_core::domain::history::plan(
        &Action::Batch(vec![
            Action::Connect([210, 100]),
            Action::Disconnect(vec![[210, 100]]),
        ]),
        &graph,
        &config(),
    )
    .unwrap();
    // The disconnect observes the original absent link, not the planned connect.
    assert_eq!(
        commands,
        [Opdracht::Verbind {
            uit: 210,
            naar: 100
        }]
    );
    let commands = pecan_core::domain::history::plan(
        &Action::Batch(vec![
            Action::Disconnect(vec![[200, 100]]),
            Action::Connect([200, 100]),
        ]),
        &graph,
        &config(),
    )
    .unwrap();
    assert_eq!(commands, [Opdracht::Ontkoppel { kabel: 1 }]);
}

#[test]
fn nonempty_noop_plan_requires_completion_and_new_graph_progress() {
    let mut c = Controller::default();
    let graph = graph();
    assert_eq!(
        graph.knopen.iter().find(|n| n.id == 20).unwrap().mute,
        Some(false)
    );
    c.reduce(snapshot(graph.clone(), 1, 1)).unwrap();
    let plan = execute(
        &mut c,
        "Unmute",
        Action::Mute {
            node: 20,
            muted: false,
        },
    );
    result(&mut c, plan.ticket, 1, 1);
    assert!(
        c.busy(),
        "admission/old observation cannot complete a history step"
    );
    c.reduce(snapshot(graph, 1, 2)).unwrap();
    assert!(!c.busy());
    assert_eq!(c.labels(), (Some("Unmute"), None));
}
