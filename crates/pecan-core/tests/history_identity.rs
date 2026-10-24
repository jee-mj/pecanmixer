//! Synthetic registry identities; no runtime captures or device data.
use pecan_core::{
    backend::{Stamp, Stamped},
    domain::history::{Action, Controller, DomainError, Input},
    graph::{Kabel, Momentopname, Opdracht},
    history_backend::HistoryResults,
    history_identity::HistoryGraph,
    native_command::{Counts, NativeProgress, NativeReport},
};
use std::{collections::BTreeMap, sync::Arc};

struct Registry {
    graph: Momentopname,
    authority: Arc<()>,
    generations: BTreeMap<u32, u64>,
    stamp: Stamp,
}
impl Registry {
    fn new() -> Self {
        let graph: Momentopname =
            serde_json::from_str(include_str!("../../../fixtures/domain/audio-policy.json"))
                .unwrap();
        let generations = graph
            .knopen
            .iter()
            .map(|n| n.id)
            .chain(graph.poorten.iter().map(|p| p.id))
            .map(|id| (id, u64::from(id) + 1))
            .collect();
        Self {
            graph,
            authority: Arc::new(()),
            generations,
            stamp: Stamp {
                epoch: 1,
                revision: 1,
            },
        }
    }
    fn frame(&self) -> HistoryGraph {
        HistoryGraph::from_registry(
            Stamped {
                stamp: self.stamp,
                value: Arc::new(self.graph.clone()),
            },
            self.authority.clone(),
            self.generations.clone(),
        )
    }
    fn next(&mut self, c: &mut Controller) {
        self.stamp.revision += 1;
        c.reduce(Input::NativeGraph(self.frame())).unwrap();
    }
    fn replace(&mut self, id: u32) {
        *self.generations.get_mut(&id).unwrap() += 10_000;
    }
    fn start(&self) -> Controller {
        let mut c = Controller::default();
        c.reduce(Input::NativeGraph(self.frame())).unwrap();
        c
    }
    fn done(&mut self, c: &mut Controller, ticket: u64) {
        self.next(c);
        c.reduce(Input::CommandResult {
            ticket,
            result: Ok(self.stamp),
        })
        .unwrap();
    }
}
fn mute(c: &mut Controller, node: u32) -> u64 {
    c.reduce(Input::Execute {
        label: format!("Synthetic mute {node}"),
        action: Action::Mute { node, muted: true },
    })
    .unwrap()
    .unwrap()
    .ticket
}

#[test]
fn numeric_ids_without_a_registry_witness_cannot_authorize_undo() {
    let mut r = Registry::new();
    let mut c = Controller::default();
    c.reduce(Input::Graph(r.frame().graph().clone())).unwrap();
    let ticket = mute(&mut c, 20);
    r.stamp.revision += 1;
    c.reduce(Input::Graph(r.frame().graph().clone())).unwrap();
    c.reduce(Input::CommandResult {
        ticket,
        result: Ok(r.stamp),
    })
    .unwrap();
    r.graph
        .knopen
        .iter_mut()
        .find(|n| n.id == 20)
        .unwrap()
        .serial = Some(9999);
    r.stamp.revision += 1;
    c.reduce(Input::Graph(r.frame().graph().clone())).unwrap();
    assert_eq!(c.reduce(Input::Undo), Err(DomainError::IdentityUnavailable));
    assert!(!c.busy());
}

#[test]
fn unrelated_replacement_and_many_ordinary_revisions_preserve_undo_redo() {
    let mut r = Registry::new();
    let mut c = r.start();
    let ticket = mute(&mut c, 20);
    r.done(&mut c, ticket);
    r.replace(10); // A different node is irrelevant to this mute.
    for _ in 0..100 {
        r.graph.quantum = Some(128);
        r.next(&mut c);
    }
    let undo = c.reduce(Input::Undo).unwrap().unwrap();
    assert_eq!(
        undo.commands,
        [Opdracht::ZetMute {
            knoop: 20,
            mute: false
        }]
    );
    r.done(&mut c, undo.ticket);
    assert_eq!(c.lengths(), (0, 1));
    let redo = c.reduce(Input::Redo).unwrap().unwrap();
    assert_eq!(
        redo.commands,
        [Opdracht::ZetMute {
            knoop: 20,
            mute: true
        }]
    );
    r.done(&mut c, redo.ticket);
    assert_eq!(c.lengths(), (1, 0));
}

#[test]
fn identical_node_replacement_prunes_only_affected_steps() {
    let mut r = Registry::new();
    let mut c = r.start();
    let first = mute(&mut c, 10);
    r.done(&mut c, first);
    let second = mute(&mut c, 20);
    r.done(&mut c, second);
    let before = r.graph.clone();
    r.replace(20);
    r.next(&mut c);
    assert_eq!(
        r.graph, before,
        "DTO equality does not prove registry identity"
    );
    assert_eq!(c.lengths(), (1, 0));
    assert_eq!(
        c.reduce(Input::Undo).unwrap().unwrap().commands,
        [Opdracht::ZetMute {
            knoop: 10,
            mute: false
        }]
    );
}

#[test]
fn identical_port_replacement_or_owner_replacement_invalidates_pair_history() {
    for id in [200, 20] {
        let mut r = Registry::new();
        let mut c = r.start();
        let p = c
            .reduce(Input::Execute {
                label: "Synthetic disconnect".into(),
                action: Action::Disconnect(vec![[200, 100]]),
            })
            .unwrap()
            .unwrap();
        r.graph.kabels.retain(|l| l.id != 1);
        r.done(&mut c, p.ticket);
        r.replace(id);
        r.next(&mut c);
        assert_eq!(c.lengths(), (0, 0));
        assert!(c.reduce(Input::Undo).unwrap().is_none());
    }
}

#[test]
fn recreated_link_id_is_resolved_without_invalidating_endpoint_identity() {
    let mut r = Registry::new();
    let mut c = r.start();
    let p = c
        .reduce(Input::Execute {
            label: "Synthetic disconnect".into(),
            action: Action::Disconnect(vec![[200, 100]]),
        })
        .unwrap()
        .unwrap();
    r.graph.kabels.retain(|l| l.id != 1);
    r.done(&mut c, p.ticket);
    let undo = c.reduce(Input::Undo).unwrap().unwrap();
    assert_eq!(
        undo.commands,
        [Opdracht::Verbind {
            uit: 200,
            naar: 100
        }]
    );
    r.graph.kabels.push(Kabel {
        id: 9001,
        uit_knoop: 20,
        uit_poort: 200,
        in_knoop: 10,
        in_poort: 100,
        client: None,
    });
    r.done(&mut c, undo.ticket);
    assert_eq!(
        c.reduce(Input::Redo).unwrap().unwrap().commands,
        [Opdracht::Ontkoppel { kabel: 9001 }]
    );
}

#[test]
fn disappeared_endpoint_never_resurrects_a_pruned_step() {
    let mut r = Registry::new();
    let mut c = r.start();
    assert!(c
        .reduce(Input::Execute {
            label: "Synthetic existing connect".into(),
            action: Action::Connect([200, 100])
        })
        .unwrap()
        .is_none());
    assert_eq!(c.lengths(), (1, 0)); // Existing-pair no-op still progresses.
    let port = r
        .graph
        .poorten
        .iter()
        .find(|p| p.id == 200)
        .unwrap()
        .clone();
    r.graph.poorten.retain(|p| p.id != 200);
    r.generations.remove(&200);
    r.next(&mut c);
    r.graph.poorten.push(port);
    r.generations.insert(200, 9000);
    r.next(&mut c);
    assert!(c.reduce(Input::Undo).unwrap().is_none());
}

#[test]
fn target_name_or_serial_reuse_invalidates_original_destination_identity() {
    for serial in [false, true] {
        let mut r = Registry::new();
        let mut c = r.start();
        let node = r.graph.knopen.iter().find(|n| n.id == 10).unwrap();
        let target = if serial {
            node.serial.unwrap().to_string()
        } else {
            node.naam.clone()
        };
        let p = c
            .reduce(Input::Execute {
                label: "Synthetic target".into(),
                action: Action::Target {
                    node: 20,
                    target: Some(target),
                },
            })
            .unwrap()
            .unwrap();
        r.done(&mut c, p.ticket);
        r.replace(10);
        r.next(&mut c);
        assert_eq!(c.lengths(), (0, 0));
    }
}

#[test]
fn pending_replacement_and_delayed_receipt_cannot_complete_new_work() {
    let mut r = Registry::new();
    let mut c = r.start();
    let mut bridge = HistoryResults::default();
    let old_source = r.stamp;
    let old = mute(&mut c, 20);
    r.replace(20);
    r.next(&mut c);
    assert!(!c.busy());
    let current = mute(&mut c, 10);
    let source = r.stamp;
    let old_report = NativeReport {
        ticket: old,
        source: old_source,
        command_count: 1,
        progress: NativeProgress::Observed {
            stamp: r.stamp,
            counts: Counts::default(),
        },
    };
    assert_eq!(
        c.reduce(bridge.terminal(&old_report).unwrap()),
        Err(DomainError::UnknownTicket)
    );
    assert!(c.busy());
    let report = NativeReport {
        ticket: current,
        source,
        command_count: 1,
        progress: NativeProgress::ObservedNoOp {
            stamp: Stamp {
                revision: source.revision + 1,
                ..source
            },
            counts: Counts {
                no_ops: 1,
                ..Counts::default()
            },
        },
    };
    c.reduce(bridge.terminal(&report).unwrap()).unwrap();
    assert!(c.busy());
    assert!(bridge.terminal(&old_report).is_none());
    r.next(&mut c);
    assert!(!c.busy());
    assert_eq!(c.lengths(), (1, 0));
}

#[test]
fn reconnect_new_authority_and_witness_downgrade_clear_history() {
    for mode in 0..4 {
        let mut r = Registry::new();
        let mut c = r.start();
        let ticket = mute(&mut c, 20);
        r.done(&mut c, ticket);
        match mode {
            0 => r.stamp.epoch += 1,
            1 => r.authority = Arc::new(()),
            2 => r.graph.verbonden = false,
            _ => {
                r.stamp.revision += 1;
                c.reduce(Input::Graph(r.frame().graph().clone())).unwrap();
            }
        }
        if mode != 3 {
            r.next(&mut c);
        }
        assert_eq!(c.lengths(), (0, 0));
        assert!(!c.busy());
    }
}

#[test]
fn redo_and_pending_undo_cannot_cross_replacement() {
    for completed_undo in [false, true] {
        let mut r = Registry::new();
        let mut c = r.start();
        let ticket = mute(&mut c, 20);
        r.done(&mut c, ticket);
        let undo = c.reduce(Input::Undo).unwrap().unwrap();
        if completed_undo {
            r.done(&mut c, undo.ticket);
            assert_eq!(c.lengths(), (0, 1));
        }
        r.replace(20);
        r.next(&mut c);
        assert_eq!(c.lengths(), (0, 0));
        assert!(!c.busy());
        assert!(c.reduce(Input::Redo).unwrap().is_none());
        assert_eq!(
            c.reduce(Input::CommandResult {
                ticket: undo.ticket,
                result: Ok(r.stamp)
            }),
            Err(DomainError::UnknownTicket)
        );
    }
}

#[test]
fn route_and_batch_dependencies_are_bound_but_unrelated_steps_survive() {
    for route in [false, true] {
        let mut r = Registry::new();
        let mut c = r.start();
        let unrelated = mute(&mut c, 11);
        r.done(&mut c, unrelated);
        let action = if route {
            let m =
                pecan_core::domain::mixer::Mixer::build(&r.graph, &c.buses.definitions().unwrap());
            Action::Route {
                source: m.apps.iter().find(|s| s.node.id == 20).unwrap().key.clone(),
                destination: m
                    .destinations
                    .iter()
                    .find(|s| s.node.id == 10)
                    .unwrap()
                    .key
                    .clone(),
                enabled: false,
            }
        } else {
            Action::Batch(vec![
                Action::Mute {
                    node: 20,
                    muted: true,
                },
                Action::Connect([200, 100]),
            ])
        };
        let p = c
            .reduce(Input::Execute {
                label: "Synthetic composite".into(),
                action,
            })
            .unwrap()
            .unwrap();
        r.done(&mut c, p.ticket);
        r.replace(200);
        r.next(&mut c);
        assert_eq!(c.lengths(), (1, 0));
        assert_eq!(
            c.reduce(Input::Undo).unwrap().unwrap().commands,
            [Opdracht::ZetMute {
                knoop: 11,
                mute: false
            }]
        );
    }
}

#[test]
fn incomplete_witness_cannot_record_or_replay_missing_dependencies() {
    let mut r = Registry::new();
    r.generations.remove(&200);
    let mut c = r.start();
    assert_eq!(
        c.reduce(Input::Execute {
            label: "Synthetic incomplete connect".into(),
            action: Action::Connect([200, 100])
        }),
        Err(DomainError::IdentityUnavailable)
    );
    assert_eq!(c.lengths(), (0, 0));
    assert!(!c.busy());
}

#[test]
fn unresolved_target_cannot_adopt_an_existing_other_batch_dependency() {
    let mut r = Registry::new();
    let mut c = r.start();
    let p = c
        .reduce(Input::Execute {
            label: "Synthetic selector batch".into(),
            action: Action::Batch(vec![
                Action::Mute {
                    node: 10,
                    muted: true,
                },
                Action::Target {
                    node: 20,
                    target: Some("synthetic.future.target".into()),
                },
            ]),
        })
        .unwrap()
        .unwrap();
    r.done(&mut c, p.ticket);
    // Same registry objects and generations; only name resolution changes.
    r.graph.knopen.iter_mut().find(|n| n.id == 10).unwrap().naam = "synthetic.future.target".into();
    r.next(&mut c);
    assert_eq!(c.lengths(), (0, 0));
    assert!(c.reduce(Input::Undo).unwrap().is_none());
}

#[test]
fn route_key_cannot_adopt_another_existing_batch_dependency() {
    let mut r = Registry::new();
    let mut c = r.start();
    let m = pecan_core::domain::mixer::Mixer::build(&r.graph, &c.buses.definitions().unwrap());
    let source = m.apps.iter().find(|s| s.node.id == 20).unwrap().key.clone();
    let destination = m
        .destinations
        .iter()
        .find(|s| s.node.id == 10)
        .unwrap()
        .key
        .clone();
    assert_eq!(m.source(&source).unwrap().node.id, 20);
    let p = c
        .reduce(Input::Execute {
            label: "Synthetic route selector batch".into(),
            action: Action::Batch(vec![
                Action::Route {
                    source: source.clone(),
                    destination,
                    enabled: false,
                },
                Action::Connect([210, 100]),
                Action::Connect([211, 101]),
            ]),
        })
        .unwrap()
        .unwrap();
    r.done(&mut c, p.ticket);
    // The public fixture deliberately includes another same-key stream. Its
    // ports are already batch dependencies; a generation union cannot catch
    // this selector changing to that different, still-existing stream.
    r.graph.knopen.iter_mut().find(|n| n.id == 20).unwrap().naam =
        "synthetic.retired.source".into();
    let m = pecan_core::domain::mixer::Mixer::build(&r.graph, &c.buses.definitions().unwrap());
    assert_eq!(m.source(&source).unwrap().node.id, 21);
    r.next(&mut c);
    assert_eq!(c.lengths(), (0, 0));
    assert!(c.reduce(Input::Undo).unwrap().is_none());
}
