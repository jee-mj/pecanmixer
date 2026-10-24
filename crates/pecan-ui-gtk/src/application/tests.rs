//! Synthetic public-fixture tests. No audio worker/server/user profile is used.
use super::*;
use crate::controls::{Controller, Phase};
use gtk::prelude::*;
use pecan_core::{
    backend::Stamp,
    domain::history::Action,
    graph::{Kabel, Opdracht},
    native_command::{native_receipt, Counts, NativeFailure, NativeProgress},
};
use std::collections::BTreeMap;
fn graph() -> Momentopname {
    let mut graph: Momentopname =
        serde_json::from_str(include_str!("../../../../fixtures/graph/audio-basic.json")).unwrap();
    for node in &mut graph.knopen {
        node.app = Some("Synthetic application".into());
        node.media = None;
    }
    graph
}
fn generations(g: &Momentopname) -> BTreeMap<u32, u64> {
    g.knopen
        .iter()
        .map(|n| (n.id, n.id as u64))
        .chain(g.poorten.iter().map(|p| (p.id, p.id as u64)))
        .collect()
}
fn setup() -> (
    ApplicationOwner,
    Session,
    FixtureDriver,
    Graph,
    BTreeMap<u32, u64>,
) {
    let owner = ApplicationOwner::fixture();
    let session = owner.activate();
    let driver = owner.fixture_driver().unwrap();
    let graph = graph();
    let gens = generations(&graph);
    let origin = driver.publish(graph, gens.clone());
    session.poll();
    (owner, session, driver, origin, gens)
}
fn mute(s: &Session, origin: &Graph, value: bool) -> Request {
    s.execute(
        origin,
        "Mute",
        Action::Mute {
            node: 20,
            muted: value,
        },
    )
    .unwrap()
    .unwrap()
}
fn observed(request: &FixtureRequest, stamp: Stamp, noop: bool) {
    request.reporter.set(if noop {
        NativeProgress::ObservedNoOp {
            stamp,
            counts: Counts::default(),
        }
    } else {
        NativeProgress::Observed {
            stamp,
            counts: Counts::default(),
        }
    });
}
fn commit_mute(
    s: &Session,
    d: &FixtureDriver,
    origin: &Graph,
    gens: &BTreeMap<u32, u64>,
    value: bool,
) -> Graph {
    mute(s, origin, value);
    let req = d.take_requests().pop().unwrap();
    let mut next = (*origin.value).clone();
    next.knopen.iter_mut().find(|n| n.id == 20).unwrap().mute = Some(value);
    let frame = d.publish(next, gens.clone());
    observed(&req, frame.stamp, false);
    s.poll();
    frame
}
#[test]
fn repeated_activation_has_one_session_consumer_and_no_native_worker() {
    let (owner, s, d, origin, _) = setup();
    for _ in 0..1000 {
        assert!(s.same_session(&owner.activate()));
    }
    assert!(owner.native_handle().is_none());
    assert!(Arc::ptr_eq(&origin.value, &s.graph().unwrap().value));
    assert_eq!(s.status(), Status::Ready);
    mute(&s, &origin, true);
    owner.activate().poll();
    assert_eq!(d.take_requests().len(), 1);
    assert_eq!(s.history_lengths(), (0, 0));
}
#[test]
fn shared_mixer_and_patch_pipeline_keeps_exact_arc_and_waits_for_graph() {
    let (_, s, d, origin, gens) = setup();
    let mut mixer = Controller::new(s.adapter(), Default::default());
    mixer.observe(origin.clone());
    let key = mixer.model().apps[0].key.clone();
    let binding = mixer.bind(&key).unwrap();
    mixer.set_mute(&binding, true).unwrap();
    let req = d.take_requests().pop().unwrap();
    assert!(Arc::ptr_eq(&req.intent.origin.value, &origin.value));
    assert_eq!(req.intent.ticket, 1);
    assert!(s
        .execute(&origin, "Connect", Action::Connect([201, 301]))
        .is_err());
    req.reporter.set(NativeProgress::ExecutedLocally(Counts {
        submitted: 1,
        ..Default::default()
    }));
    s.poll();
    assert!(mixer.poll_session());
    assert_eq!(mixer.phase(), Phase::WaitingForObservation);
    assert_eq!(mixer.model().apps[0].node.mute, Some(false));
    let next_stamp = Stamp {
        revision: origin.stamp.revision + 1,
        ..origin.stamp
    };
    observed(&req, next_stamp, false);
    s.poll();
    mixer.poll_session();
    assert!(s.busy());
    assert_eq!(s.status(), Status::WaitingForGraph);
    assert_eq!(mixer.phase(), Phase::WaitingForGraph);
    assert_eq!(s.history_lengths(), (0, 0));
    let mut next = (*origin.value).clone();
    next.knopen.iter_mut().find(|n| n.id == 20).unwrap().mute = Some(true);
    let next = d.publish(next, gens);
    s.poll();
    mixer.observe(next.clone());
    mixer.poll_session();
    assert_eq!(mixer.phase(), Phase::Ready);
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (1, 0));
    s.execute(&next, "Connect", Action::Connect([201, 301]))
        .unwrap();
    assert_eq!(d.take_requests()[0].intent.ticket, 2);
}
#[test]
fn backend_noop_requires_fresh_revision_and_duplicate_planned_connect_records_nothing() {
    let (_, s, d, origin, gens) = setup();
    assert!(s
        .execute(&origin, "Duplicate", Action::Connect([201, 101]))
        .unwrap()
        .is_none());
    assert_eq!(s.history_lengths(), (0, 0));
    assert!(d.take_requests().is_empty());
    mute(&s, &origin, false);
    let req = d.take_requests().pop().unwrap();
    let next = Stamp {
        revision: origin.stamp.revision + 1,
        ..origin.stamp
    };
    observed(&req, next, true);
    s.poll();
    assert!(s.busy());
    s.poll();
    assert!(s.busy());
    d.publish((*origin.value).clone(), gens);
    s.poll();
    assert!(!s.busy());
    assert_eq!(s.status(), Status::Observed { no_op: true });
}
#[test]
fn graph_before_delayed_receipt_can_complete_and_undo_redo_share_tickets() {
    let (_, s, d, origin, gens) = setup();
    mute(&s, &origin, true);
    let req = d.take_requests().pop().unwrap();
    let mut next = (*origin.value).clone();
    next.knopen.iter_mut().find(|n| n.id == 20).unwrap().mute = Some(true);
    let next = d.publish(next, gens.clone());
    s.poll();
    assert!(s.busy());
    observed(&req, next.stamp, false);
    s.poll();
    assert_eq!(s.history_lengths(), (1, 0));
    s.undo(&next).unwrap();
    let undo = d.take_requests().pop().unwrap();
    assert_eq!(undo.intent.ticket, 2);
    assert!(matches!(
        &undo.intent.commands[0],
        Opdracht::ZetMute {
            knoop: 20,
            mute: false
        }
    ));
    let mut restored = (*next.value).clone();
    restored
        .knopen
        .iter_mut()
        .find(|n| n.id == 20)
        .unwrap()
        .mute = Some(false);
    let restored = d.publish(restored, gens.clone());
    observed(&undo, restored.stamp, false);
    s.poll();
    assert_eq!(s.history_lengths(), (0, 1));
    s.redo(&restored).unwrap();
    let redo = d.take_requests().pop().unwrap();
    assert_eq!(redo.intent.ticket, 3);
    observed(&req, next.stamp, false);
    s.poll();
    assert!(s.busy(), "old receipt cannot resolve redo");
    let replay = d.publish((*next.value).clone(), gens);
    observed(&redo, replay.stamp, false);
    s.poll();
    assert_eq!(s.history_lengths(), (1, 0));
}
#[test]
fn copied_and_superseded_origins_fail_without_dispatch() {
    let (_, s, d, origin, gens) = setup();
    let copy = Graph {
        stamp: origin.stamp,
        value: Arc::new((*origin.value).clone()),
    };
    assert!(s
        .execute(
            &copy,
            "Mute",
            Action::Mute {
                node: 20,
                muted: true
            }
        )
        .is_err());
    d.publish((*origin.value).clone(), gens);
    s.poll();
    assert!(s
        .execute(
            &origin,
            "Mute",
            Action::Mute {
                node: 20,
                muted: true
            }
        )
        .is_err());
    assert!(d.take_requests().is_empty());
}
#[test]
fn identical_numeric_replacement_invalidates_pending_and_late_result_is_isolated() {
    let (_, s, d, origin, mut gens) = setup();
    let mut mixer = Controller::new(s.adapter(), Default::default());
    mixer.observe(origin.clone());
    let binding = mixer.bind(&mixer.model().apps[0].key).unwrap();
    mixer.set_mute(&binding, true).unwrap();
    let old = d.take_requests().pop().unwrap();
    gens.insert(20, 999);
    let replacement = d.publish((*origin.value).clone(), gens);
    s.poll();
    mixer.observe(replacement.clone());
    mixer.poll_session();
    assert!(!s.busy());
    assert!(matches!(mixer.phase(), Phase::Unconfirmed(_)));
    assert_eq!(s.history_lengths(), (0, 0));
    mute(&s, &replacement, true);
    let fresh = d.take_requests().pop().unwrap();
    observed(&old, replacement.stamp, false);
    s.poll();
    assert!(s.busy());
    assert_eq!(fresh.intent.ticket, 2);
    assert_eq!(s.history_lengths(), (0, 0));
}
#[test]
fn ordinary_revisions_preserve_history_but_replacement_prunes_only_affected_steps() {
    let (_, s, d, origin, mut gens) = setup();
    let next = commit_mute(&s, &d, &origin, &gens, true);
    s.execute(
        &next,
        "Other mute",
        Action::Mute {
            node: 10,
            muted: true,
        },
    )
    .unwrap();
    let req = d.take_requests().pop().unwrap();
    let mut g = (*next.value).clone();
    g.knopen.iter_mut().find(|n| n.id == 10).unwrap().mute = Some(true);
    let next = d.publish(g, gens.clone());
    observed(&req, next.stamp, false);
    s.poll();
    assert_eq!(s.history_lengths(), (2, 0));
    let ordinary = d.publish((*next.value).clone(), gens.clone());
    s.poll();
    assert_eq!(s.history_lengths(), (2, 0));
    gens.insert(20, 909);
    let replacement = d.publish((*ordinary.value).clone(), gens);
    s.poll();
    assert_eq!(s.history_lengths(), (1, 0));
    s.undo(&replacement).unwrap();
    let undo = d.take_requests().pop().unwrap();
    assert!(matches!(
        undo.intent.commands[0],
        Opdracht::ZetMute {
            knoop: 10,
            mute: false
        }
    ));
}
#[test]
fn recreated_link_ids_are_resolved_for_undo_in_shared_history() {
    let (_, s, d, origin, gens) = setup();
    s.execute(&origin, "Connect", Action::Connect([201, 301]))
        .unwrap();
    let req = d.take_requests().pop().unwrap();
    let mut g = (*origin.value).clone();
    g.kabels.push(Kabel {
        id: 900,
        client: None,
        uit_knoop: 20,
        uit_poort: 201,
        in_knoop: 30,
        in_poort: 301,
    });
    let frame = d.publish(g, gens.clone());
    observed(&req, frame.stamp, false);
    s.poll();
    let mut g = (*frame.value).clone();
    g.kabels.iter_mut().find(|l| l.id == 900).unwrap().id = 901;
    let frame = d.publish(g, gens);
    s.poll();
    s.undo(&frame).unwrap();
    let undo = d.take_requests().pop().unwrap();
    assert_eq!(
        undo.intent.commands,
        vec![Opdracht::Ontkoppel { kabel: 901 }]
    );
}
#[test]
fn reconnect_clears_history_and_ignores_abandoned_receipt() {
    let (_, s, d, origin, gens) = setup();
    let next = commit_mute(&s, &d, &origin, &gens, true);
    mute(&s, &next, false);
    let old = d.take_requests().pop().unwrap();
    let mut g = (*next.value).clone();
    g.verbonden = false;
    d.publish(g, gens.clone());
    s.poll();
    assert_eq!(s.status(), Status::Disconnected);
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (0, 0));
    let connected = d.publish((*next.value).clone(), gens);
    s.poll();
    mute(&s, &connected, false);
    d.take_requests();
    observed(&old, connected.stamp, false);
    s.poll();
    assert!(s.busy());
    assert_eq!(s.history_lengths(), (0, 0));
}
#[test]
fn missing_witness_downgrades_and_pauses_until_new_trusted_frame() {
    let (_, s, d, origin, gens) = setup();
    let next = commit_mute(&s, &d, &origin, &gens, true);
    let frame = d.publish((*next.value).clone(), gens.clone());
    d.forget_origins();
    s.poll();
    assert_eq!(s.status(), Status::IdentityUnavailable);
    assert_eq!(s.history_lengths(), (0, 0));
    assert!(!s.enabled());
    assert!(s.undo(&frame).is_err());
    assert!(d.take_requests().is_empty());
    let frame = d.publish((*next.value).clone(), gens);
    s.poll();
    assert!(s.enabled());
    mute(&s, &frame, false);
}
#[test]
fn admission_failure_and_invalid_observation_do_not_strand_history() {
    let (_, s, d, origin, gens) = setup();
    d.forget_origins();
    assert!(s
        .execute(
            &origin,
            "Mute",
            Action::Mute {
                node: 20,
                muted: true
            }
        )
        .is_err());
    assert!(!s.busy());
    let frame = d.publish((*origin.value).clone(), gens.clone());
    s.poll();
    mute(&s, &frame, true);
    let req = d.take_requests().pop().unwrap();
    observed(&req, frame.stamp, false);
    s.poll();
    assert!(!s.busy());
    assert!(matches!(s.status(), Status::Unconfirmed(_)));
    let frame = d.publish((*origin.value).clone(), gens);
    s.poll();
    mute(&s, &frame, true);
    assert_eq!(d.take_requests().len(), 1);
}
#[test]
fn unconfirmed_is_distinct_and_never_creates_an_undo_step() {
    let (_, s, d, origin, _) = setup();
    mute(&s, &origin, true);
    let req = d.take_requests().pop().unwrap();
    req.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            submitted: 1,
            ..Default::default()
        },
    });
    s.poll();
    assert!(matches!(s.status(), Status::Unconfirmed(_)));
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (0, 0));
}
fn descendants(root: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut widgets = vec![root.as_ref().clone()];
    let mut i = 0;
    while i < widgets.len() {
        let mut child = widgets[i].first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            widgets.push(w);
        }
        i += 1;
    }
    widgets
}
#[gtk::test]
fn integrated_widgets_mute_patch_and_undo_use_one_coordinator() {
    let (_, s, d, origin, gens) = setup();
    let view = NativeWorkspace::new(s.clone());
    view.poll();
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(1100)
        .default_height(800)
        .build();
    window.present();
    let controls = descendants(view.widget());
    let mute = controls
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::ToggleButton>().ok())
        .find(|b| b.label().as_deref() == Some("Mute") && b.is_sensitive())
        .unwrap();
    mute.set_active(true);
    view.poll();
    let req = d.take_requests().pop().unwrap();
    assert!(matches!(
        req.intent.commands[0],
        Opdracht::ZetMute { mute: true, .. }
    ));
    assert_eq!(s.status(), Status::Queued);
    let mut g = (*origin.value).clone();
    if let Opdracht::ZetMute { knoop, .. } = req.intent.commands[0] {
        g.knopen.iter_mut().find(|n| n.id == knoop).unwrap().mute = Some(true)
    };
    let frame = d.publish(g, gens.clone());
    observed(&req, frame.stamp, false);
    view.poll();
    assert_eq!(s.history_lengths(), (1, 0));
    let dropdowns: Vec<_> = controls
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::DropDown>().ok())
        .collect();
    assert_eq!(dropdowns.len(), 2);
    // Labels are generated from this public synthetic fixture; find endpoints
    // via the models rather than assuming dropdown order or localized labels.
    let choose = |dropdown: &gtk::DropDown, port: u32| {
        let m = dropdown.model().unwrap();
        let i = (0..m.n_items())
            .find(|i| {
                m.item(*i)
                    .and_downcast::<gtk::StringObject>()
                    .is_some_and(|s| s.string().contains(&format!("port {port}")))
            })
            .unwrap();
        dropdown.set_selected(i);
    };
    choose(&dropdowns[0], 201);
    choose(&dropdowns[1], 301);
    let connect = controls
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("Connect"))
        .unwrap();
    assert!(connect.is_sensitive());
    connect.emit_clicked();
    view.poll();
    let req = d.take_requests().pop().unwrap();
    assert_eq!(req.intent.ticket, 2);
    assert_eq!(
        req.intent.commands,
        vec![Opdracht::Verbind {
            uit: 201,
            naar: 301
        }]
    );
    let mut g = (*frame.value).clone();
    g.kabels.push(Kabel {
        id: 900,
        client: None,
        uit_knoop: 20,
        uit_poort: 201,
        in_knoop: 30,
        in_poort: 301,
    });
    let frame = d.publish(g, gens);
    observed(&req, frame.stamp, false);
    view.poll();
    let undo = controls
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::Button>().ok())
        .find(|b| b.label().is_some_and(|s| s.starts_with("Undo ")))
        .unwrap();
    assert!(undo.is_sensitive());
    undo.emit_clicked();
    view.poll();
    let req = d.take_requests().pop().unwrap();
    assert_eq!(req.intent.ticket, 3);
    assert_eq!(
        req.intent.commands,
        vec![Opdracht::Ontkoppel { kabel: 900 }]
    );
    window.close();
}

#[test]
fn mismatched_receipt_fails_closed_without_leaving_history_busy() {
    struct BadEndpoint(RefCell<Option<HistoryGraph>>);
    impl Endpoint for BadEndpoint {
        fn witness(&self, g: &Graph) -> Result<HistoryGraph, AdmissionError> {
            self.0
                .borrow()
                .as_ref()
                .filter(|f| Arc::ptr_eq(&f.graph().value, &g.value))
                .cloned()
                .ok_or(AdmissionError::OriginUnavailable)
        }
        fn dispatch(&self, i: NativeIntent) -> Result<NativeReceipt, AdmissionError> {
            Ok(native_receipt(i.ticket + 1, i.origin.stamp, i.commands.len()).0)
        }
    }
    let (inbox, _) = LatestInbox::new(16);
    let ep = Rc::new(BadEndpoint(RefCell::new(None)));
    let s = Session::new(inbox.clone(), ep.clone());
    let graph = graph();
    let gens = generations(&graph);
    let origin = Graph {
        stamp: Stamp {
            epoch: 1,
            revision: 1,
        },
        value: Arc::new(graph),
    };
    *ep.0.borrow_mut() = Some(HistoryGraph::from_registry(
        origin.clone(),
        Arc::new(()),
        gens,
    ));
    inbox.publish(pecan_core::backend::BackendEvent::Graph(origin.clone()));
    s.poll();
    assert!(s
        .execute(
            &origin,
            "Mute",
            Action::Mute {
                node: 20,
                muted: true
            }
        )
        .is_err());
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (0, 0));
    assert!(matches!(s.status(), Status::Unconfirmed(_)));
}

#[gtk::test]
fn undo_button_retains_presented_origin_until_workspace_refresh() {
    let (_, s, d, origin, gens) = setup();
    let next = commit_mute(&s, &d, &origin, &gens, true);
    let view = NativeWorkspace::new(s.clone());
    view.poll();
    let undo = descendants(view.widget())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().is_some_and(|s| s.starts_with("Undo ")))
        .unwrap();
    assert!(undo.is_sensitive());
    d.publish((*next.value).clone(), gens);
    s.poll(); // callback still belongs to old presentation
    undo.emit_clicked();
    assert!(d.take_requests().is_empty());
    assert_eq!(s.history_lengths(), (1, 0));
    assert!(!s.busy());
    undo.emit_clicked(); // previous callback refreshed presentation after refusing stale input
    assert_eq!(d.take_requests().len(), 1);
    assert!(s.busy());
}
#[gtk::test]
fn missing_identity_disables_workspace_without_inventing_a_pending_receipt() {
    let (_, s, d, origin, gens) = setup();
    let view = NativeWorkspace::new(s.clone());
    view.poll();
    d.publish((*origin.value).clone(), gens);
    d.forget_origins();
    view.poll();
    assert_eq!(s.status(), Status::IdentityUnavailable);
    assert!(!s.busy());
    assert!(d.take_requests().is_empty());
    let all = descendants(view.widget());
    for button in all
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::Button>().ok())
        .filter(|b| b.label().as_deref() == Some("Connect") || b.label().as_deref() == Some("Mute"))
    {
        assert!(!button.is_sensitive());
    }
}

#[test]
fn inspecting_native_owner_does_not_activate_it_or_offer_fixture_access() {
    let owner = ApplicationOwner::native();
    assert!(owner.native_handle().is_none());
    assert!(owner.fixture_driver().is_none());
    assert!(owner.native_handle().is_none());
    assert!(owner.active.borrow().is_none());
}

#[gtk::test]
fn idle_polling_preserves_the_patchbay_draft_instructions() {
    let (_, s, _, _, _) = setup();
    let view = NativeWorkspace::new(s);
    view.poll();
    let all = descendants(view.widget());
    let dropdowns: Vec<_> = all
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::DropDown>().ok())
        .collect();
    for (d, port) in [(&dropdowns[0], 201), (&dropdowns[1], 301)] {
        let model = d.model().unwrap();
        let row = (0..model.n_items())
            .find(|i| {
                model
                    .item(*i)
                    .and_downcast::<gtk::StringObject>()
                    .is_some_and(|s| s.string().contains(&format!("port {port}")))
            })
            .unwrap();
        d.set_selected(row);
    }
    let hint = all
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Label>().ok())
        .find(|l| l.text().contains("Ready to connect the chosen ports"))
        .unwrap();
    let before = hint.text();
    for _ in 0..100 {
        view.poll();
    }
    assert_eq!(hint.text(), before);
}
