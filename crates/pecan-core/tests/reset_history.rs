//! Synthetic native witness coverage for target-clear -> default-restore phases.
use pecan_core::{
    backend::{Stamp, Stamped},
    domain::{
        audio::REC_NODE,
        buses::BusConfig,
        history::{Action, Controller, Input},
    },
    graph::{Momentopname, Opdracht},
    history_identity::HistoryGraph,
};
use std::{collections::BTreeMap, sync::Arc};

fn graph() -> Momentopname {
    let mut graph: Momentopname =
        serde_json::from_str(include_str!("../../../fixtures/graph/audio-basic.json")).unwrap();
    graph.kabels.clear();
    graph.knopen.iter_mut().find(|n| n.id == 30).unwrap().naam = REC_NODE.into();
    graph
}
fn generations(graph: &Momentopname) -> BTreeMap<u32, u64> {
    graph
        .knopen
        .iter()
        .map(|n| n.id)
        .chain(graph.poorten.iter().map(|p| p.id))
        .chain(graph.kabels.iter().map(|l| l.id))
        .map(|id| (id, u64::from(id) + 100))
        .collect()
}
fn frame(
    value: Momentopname,
    revision: u64,
    authority: Arc<()>,
    ids: BTreeMap<u32, u64>,
) -> HistoryGraph {
    HistoryGraph::from_registry(
        Stamped {
            stamp: Stamp { epoch: 1, revision },
            value: Arc::new(value),
        },
        authority,
        ids,
    )
}
fn begin(value: Momentopname) -> (Controller, Arc<()>, BTreeMap<u32, u64>, u64, Momentopname) {
    let authority = Arc::new(());
    let ids = generations(&value);
    let mut c = Controller::default();
    c.buses = BusConfig {
        midi: false,
        ..BusConfig::default()
    };
    c.reduce(Input::NativeGraph(frame(
        value.clone(),
        1,
        authority.clone(),
        ids.clone(),
    )))
    .unwrap();
    let plan = c
        .reduce(Input::Execute {
            label: "Back to auto".into(),
            action: Action::BackToAuto,
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        plan.commands,
        vec![Opdracht::ZetDoel {
            knoop: 20,
            doel: None
        }]
    );
    let mut detached = value;
    detached
        .knopen
        .iter_mut()
        .find(|n| n.id == 20)
        .unwrap()
        .doel = None;
    (c, authority, ids, plan.ticket, detached)
}
fn observe(
    c: &mut Controller,
    ticket: u64,
    graph: Momentopname,
    revision: u64,
    authority: Arc<()>,
    ids: BTreeMap<u32, u64>,
) {
    c.reduce(Input::CommandResult {
        ticket,
        result: Ok(Stamp { epoch: 1, revision }),
    })
    .unwrap();
    c.reduce(Input::NativeGraph(frame(graph, revision, authority, ids)))
        .unwrap();
}
fn other_output(graph: &mut Momentopname) {
    let mut node = graph.knopen.iter().find(|n| n.id == 10).unwrap().clone();
    node.id = 40;
    node.serial = Some(40);
    node.naam = "synthetic.other-output".into();
    graph.knopen.push(node);
    let ports: Vec<_> = graph
        .poorten
        .iter()
        .filter(|p| p.knoop == 10)
        .cloned()
        .collect();
    for mut port in ports {
        port.id += 300;
        port.knoop = 40;
        graph.poorten.push(port);
    }
}

#[test]
fn reset_target_clear_before_policy_restore_keeps_one_undo_redo_step() {
    let (mut c, owner, ids, ticket, detached) = begin(graph());
    observe(
        &mut c,
        ticket,
        detached.clone(),
        2,
        owner.clone(),
        ids.clone(),
    );
    assert_eq!(c.lengths(), (1, 0));
    let undo = c.reduce(Input::Undo).unwrap().unwrap();
    assert_eq!(
        undo.commands,
        vec![Opdracht::ZetDoel {
            knoop: 20,
            doel: Some("10".into())
        }]
    );
    observe(&mut c, undo.ticket, graph(), 3, owner.clone(), ids.clone());
    assert_eq!(c.lengths(), (0, 1));
    let redo = c.reduce(Input::Redo).unwrap().unwrap();
    observe(&mut c, redo.ticket, detached, 4, owner, ids);
    assert_eq!(c.lengths(), (1, 0));
}

#[test]
fn reset_potential_default_port_replacement_prunes_instead_of_rebinding() {
    let (mut c, owner, mut ids, ticket, detached) = begin(graph());
    *ids.get_mut(&101).unwrap() += 1;
    observe(&mut c, ticket, detached, 2, owner, ids);
    assert_eq!(c.lengths(), (0, 0));
    assert!(c.reduce(Input::Undo).unwrap().is_none());
}

#[test]
fn reset_ordinary_gain_and_label_observations_keep_original_witness() {
    let (mut c, owner, ids, ticket, mut detached) = begin(graph());
    observe(
        &mut c,
        ticket,
        detached.clone(),
        2,
        owner.clone(),
        ids.clone(),
    );
    detached.knopen[0].volume = Some(vec![0.2, 0.3]);
    detached.knopen[0].media = Some("Synthetic updated label".into());
    c.reduce(Input::NativeGraph(frame(detached, 3, owner, ids)))
        .unwrap();
    assert_eq!(c.lengths(), (1, 0));
}

#[test]
fn reset_new_unrelated_manual_stream_cannot_expand_old_replay_scope() {
    let (mut c, owner, ids, ticket, mut detached) = begin(graph());
    observe(&mut c, ticket, detached.clone(), 2, owner.clone(), ids);
    let mut stream = detached.knopen.iter().find(|n| n.id == 20).unwrap().clone();
    stream.id = 40;
    stream.serial = Some(40);
    stream.naam = "synthetic.other-stream".into();
    stream.doel = Some("10".into());
    detached.knopen.push(stream);
    let ports: Vec<_> = detached
        .poorten
        .iter()
        .filter(|p| p.knoop == 20)
        .cloned()
        .collect();
    for mut port in ports {
        port.id += 200;
        port.knoop = 40;
        detached.poorten.push(port);
    }
    let ids = generations(&detached);
    c.reduce(Input::NativeGraph(frame(detached, 3, owner, ids)))
        .unwrap();
    assert_eq!(c.lengths(), (0, 0));
}

#[test]
fn reset_changed_default_does_not_authorize_an_existing_uncaptured_destination() {
    let mut g = graph();
    other_output(&mut g);
    let (mut c, owner, ids, ticket, mut detached) = begin(g);
    observe(
        &mut c,
        ticket,
        detached.clone(),
        2,
        owner.clone(),
        ids.clone(),
    );
    detached.standaard_sink = Some("synthetic.other-output".into());
    c.reduce(Input::NativeGraph(frame(detached, 3, owner, ids)))
        .unwrap();
    assert_eq!(c.lengths(), (0, 0));
}
