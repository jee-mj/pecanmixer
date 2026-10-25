//! Synthetic graph/receipts only: no worker, PipeWire server or captured data.
use super::*;
use crate::application::{ApplicationOwner, FixtureDriver, FixtureRequest};
use pecan_core::{
    graph::Momentopname,
    native_command::{Counts, NativeFailure},
};
fn setup() -> (Session, FixtureDriver, Graph, BTreeMap<u32, u64>, String) {
    let owner = ApplicationOwner::fixture();
    let s = owner.activate();
    let d = owner.fixture_driver().unwrap();
    let mut g: Momentopname = serde_json::from_str(include_str!(
        "../../../../../fixtures/graph/audio-basic.json"
    ))
    .unwrap();
    let n = g.knopen.iter_mut().find(|n| n.id == 20).unwrap();
    n.klasse = Some("Audio/Source".into());
    n.app = Some("Synthetic source".into());
    n.media = None;
    n.volume = Some(vec![0.2, 0.6, 0.3, 0.7]);
    for p in g.poorten.iter_mut().filter(|p| p.knoop == 20) {
        p.kanaal = Some(format!("AUX{}", p.volgnummer.unwrap()));
    }
    for (id, index, channel) in [(203, 2, "AUX2"), (204, 3, "AUX3")] {
        let mut p = g.poorten.iter().find(|p| p.id == 201).unwrap().clone();
        p.id = id;
        p.volgnummer = Some(index);
        p.kanaal = Some(channel.into());
        p.naam = format!("synthetic_{index}");
        g.poorten.push(p);
    }
    let gens = g
        .knopen
        .iter()
        .map(|n| (n.id, n.id as u64))
        .chain(g.poorten.iter().map(|p| (p.id, p.id as u64)))
        .collect();
    let origin = d.publish(g, gens);
    s.poll();
    let gens = dummy_generations(&origin.value);
    let key = s.volume_model().hardware[0].key.clone();
    assert_eq!(
        Volumes::strip(&s.0.state.borrow(), &key).unwrap().channels,
        vec![0, 1]
    );
    (s, d, origin, gens, key)
}
fn dummy_generations(g: &Momentopname) -> BTreeMap<u32, u64> {
    g.knopen
        .iter()
        .map(|n| (n.id, n.id as u64))
        .chain(g.poorten.iter().map(|p| (p.id, p.id as u64)))
        .collect()
}
fn values(req: &FixtureRequest) -> Vec<f32> {
    match &req.intent.commands[..] {
        [Opdracht::ZetVolume { knoop: 20, volume }] => volume.clone(),
        other => panic!("{other:?}"),
    }
}
fn report(req: &FixtureRequest, stamp: Stamp, noop: bool) {
    req.reporter.set(if noop {
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
fn apply(s: &Session, d: &FixtureDriver, req: &FixtureRequest, gens: &BTreeMap<u32, u64>) -> Graph {
    let mut next = (*s.graph().unwrap().value).clone();
    next.knopen.iter_mut().find(|n| n.id == 20).unwrap().volume = Some(values(req));
    let next = d.publish(next, gens.clone());
    report(req, next.stamp, false);
    s.poll();
    next
}
#[test]
fn motion_storm_coalesces_one_flight_and_records_one_exact_full_vector_step() {
    let (s, d, origin, gens, key) = setup();
    let before = origin
        .value
        .knopen
        .iter()
        .find(|n| n.id == 20)
        .unwrap()
        .volume
        .clone()
        .unwrap();
    let token = s.begin_volume(&origin, &key).unwrap();
    assert!(d.take_requests().is_empty());
    s.update_volume(&token, -12.).unwrap();
    let first = d.take_requests().pop().unwrap();
    assert_eq!(first.intent.ticket, 1);
    for i in 0..100_000 {
        s.update_volume(&token, -20. + f64::from(i % 100) / 10.)
            .unwrap();
    }
    s.update_volume(&token, -3.).unwrap();
    s.end_volume(&token).unwrap();
    assert!(d.take_requests().is_empty());
    assert_eq!(s.history_lengths(), (0, 0));
    assert!(!s.enabled());
    let firstgraph = apply(&s, &d, &first, &gens);
    let finalreq = d.take_requests().pop().unwrap();
    assert_eq!(finalreq.intent.ticket, 2);
    assert!(Arc::ptr_eq(
        &finalreq.intent.origin.value,
        &firstgraph.value
    ));
    assert_eq!(&values(&finalreq)[2..], &before[2..]);
    assert_eq!(s.history_lengths(), (0, 0));
    let finalgraph = apply(&s, &d, &finalreq, &gens);
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (1, 0));
    s.undo(&finalgraph).unwrap();
    let undo = d.take_requests().pop().unwrap();
    assert_eq!(undo.intent.ticket, 3);
    assert_eq!(values(&undo), before);
    let restored = apply(&s, &d, &undo, &gens);
    assert_eq!(s.history_lengths(), (0, 1));
    s.redo(&restored).unwrap();
    let redo = d.take_requests().pop().unwrap();
    assert_eq!(values(&redo), values(&finalreq));
    apply(&s, &d, &redo, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
}
#[test]
fn empty_end_no_write_and_fresh_noop_requires_both_receipt_and_graph() {
    let (s, d, origin, gens, key) = setup();
    let t = s.begin_volume(&origin, &key).unwrap();
    s.end_volume(&t).unwrap();
    assert!(!s.busy());
    assert!(d.take_requests().is_empty());
    assert_eq!(s.history_lengths(), (0, 0));
    let db = mixer::strip_db(&Volumes::strip(&s.0.state.borrow(), &key).unwrap()).unwrap();
    let t = s.begin_volume(&origin, &key).unwrap();
    s.update_volume(&t, db).unwrap();
    s.end_volume(&t).unwrap();
    let req = d.take_requests().pop().unwrap();
    req.reporter
        .set(NativeProgress::ExecutedLocally(Counts::default()));
    s.poll();
    assert!(s.busy());
    assert_eq!(s.history_lengths(), (0, 0));
    let stamp = Stamp {
        revision: origin.stamp.revision + 1,
        ..origin.stamp
    };
    report(&req, stamp, true);
    s.poll();
    assert!(s.busy());
    assert_eq!(s.history_lengths(), (0, 0));
    apply(&s, &d, &req, &gens);
    assert!(!s.busy());
    assert_eq!(
        s.history_lengths(),
        (1, 0),
        "equal mean still flattens unequal selected gains"
    );
    let origin = s.graph().unwrap();
    let t = s.begin_volume(&origin, &key).unwrap();
    s.update_volume(&t, db).unwrap();
    s.end_volume(&t).unwrap();
    let req = d.take_requests().pop().unwrap();
    let next = d.publish((*origin.value).clone(), gens);
    s.poll();
    assert!(s.busy());
    report(&req, next.stamp, true);
    s.poll();
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (1, 0));
}
#[test]
fn ordinary_frames_survive_cache_eviction_but_port_replacement_cancels_and_prunes_history() {
    let (s, d, origin, mut gens, key) = setup();
    let identity = s.strip_identity(&key).unwrap();
    let t = s.begin_volume(&origin, &key).unwrap();
    for _ in 0..100 {
        d.publish((*origin.value).clone(), gens.clone());
        s.poll();
        assert!(s.volume_active(&t));
        assert_eq!(s.strip_identity(&key).unwrap(), identity);
    }
    s.update_volume(&t, -2.).unwrap();
    s.end_volume(&t).unwrap();
    let req = d.take_requests().pop().unwrap();
    apply(&s, &d, &req, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
    // An unselected channel is part of the full-vector undo mapping too.
    *gens.get_mut(&204).unwrap() += 10_000;
    d.publish((*s.graph().unwrap().value).clone(), gens.clone());
    s.poll();
    assert_eq!(s.history_lengths(), (0, 0));
    let origin = s.graph().unwrap();
    let t = s.begin_volume(&origin, &key).unwrap();
    s.update_volume(&t, -10.).unwrap();
    let req = d.take_requests().pop().unwrap();
    *gens.get_mut(&201).unwrap() += 10_000;
    let replacement = d.publish((*origin.value).clone(), gens.clone());
    s.poll();
    assert!(!s.volume_active(&t));
    assert!(s.busy(), "old admitted receipt retained");
    assert!(s.begin_volume(&replacement, &key).is_err());
    assert!(s.update_volume(&t, 0.).is_err());
    report(&req, replacement.stamp, false);
    s.poll();
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (0, 0));
    let fresh = s.begin_volume(&replacement, &key).unwrap();
    s.update_volume(&fresh, -6.).unwrap();
    let req2 = d.take_requests().pop().unwrap();
    assert!(req2.intent.ticket > req.intent.ticket);
    report(&req, replacement.stamp, false);
    s.poll();
    assert!(s.busy());
    s.end_volume(&fresh).unwrap();
    apply(&s, &d, &req2, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
}
#[test]
fn partial_mute_memory_commits_only_on_observation_restores_mean_and_exact_undo() {
    let (s, d, origin, gens, key) = setup();
    let before = origin
        .value
        .knopen
        .iter()
        .find(|n| n.id == 20)
        .unwrap()
        .volume
        .clone()
        .unwrap();
    s.toggle_partial_mute(&origin, &key).unwrap();
    let req = d.take_requests().pop().unwrap();
    assert_eq!(values(&req), vec![0., 0., 0.3, 0.7]);
    assert!(!s.partial_muted(&key).unwrap());
    let muted = apply(&s, &d, &req, &gens);
    assert!(s.partial_muted(&key).unwrap());
    assert_eq!(s.history_lengths(), (1, 0));
    s.undo(&muted).unwrap();
    let req = d.take_requests().pop().unwrap();
    assert_eq!(values(&req), before);
    let origin = apply(&s, &d, &req, &gens);
    assert!(!s.partial_muted(&key).unwrap());
    s.redo(&origin).unwrap();
    let req = d.take_requests().pop().unwrap();
    let muted = apply(&s, &d, &req, &gens);
    assert!(s.partial_muted(&key).unwrap());
    s.toggle_partial_mute(&muted, &key).unwrap();
    let req = d.take_requests().pop().unwrap();
    let restored = values(&req);
    assert!((restored[0] - 0.4).abs() < 1e-6);
    assert_eq!(restored[0], restored[1]);
    assert_eq!(&restored[2..], &before[2..]);
    assert!(s.partial_muted(&key).unwrap());
    apply(&s, &d, &req, &gens);
    assert!(!s.partial_muted(&key).unwrap());
}
#[test]
fn overlap_and_unknown_completion_reject_without_false_memory_or_history() {
    let (s, d, origin, gens, key) = setup();
    s.toggle_partial_mute(&origin, &key).unwrap();
    let mute = d.take_requests().pop().unwrap();
    let muted = apply(&s, &d, &mute, &gens);
    assert!(s.partial_muted(&key).unwrap());
    let token = s.begin_volume(&muted, &key).unwrap();
    s.update_volume(&token, -15.).unwrap();
    let req = d.take_requests().pop().unwrap();
    assert!(
        s.partial_muted(&key).unwrap(),
        "fader forget waits for completion"
    );
    assert!(s.toggle_partial_mute(&muted, &key).is_err());
    assert!(s.undo(&muted).is_err());
    assert!(s
        .execute(&muted, "route", Action::Connect([201, 301]))
        .is_err());
    s.end_volume(&token).unwrap();
    req.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts::default(),
    });
    s.poll();
    assert!(!s.busy());
    assert!(!s.partial_muted(&key).unwrap());
    assert_eq!(s.history_lengths(), (1, 0));
    let token = s.begin_volume(&muted, &key).unwrap();
    s.update_volume(&token, -4.).unwrap();
    let req = d.take_requests().pop().unwrap();
    s.abandon_volume(&token);
    assert!(s.busy());
    req.reporter.set(NativeProgress::Rejected {
        reason: NativeFailure::Disconnected,
        counts: Counts::default(),
    });
    s.poll();
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (1, 0));
    assert!(s.end_volume(&token).is_err());
}
#[test]
fn disconnect_and_missing_witness_cancel_without_retargeting() {
    let (s, d, origin, gens, key) = setup();
    let t = s.begin_volume(&origin, &key).unwrap();
    s.update_volume(&t, -3.).unwrap();
    let req = d.take_requests().pop().unwrap();
    let mut offline = (*origin.value).clone();
    offline.verbonden = false;
    let offline = d.publish(offline, gens.clone());
    s.poll();
    assert!(!s.volume_active(&t));
    report(&req, offline.stamp, false);
    s.poll();
    assert!(!s.busy());
    let online = d.publish((*origin.value).clone(), gens);
    s.poll();
    assert!(s.begin_volume(&origin, &key).is_err());
    let t = s.begin_volume(&online, &key).unwrap();
    d.forget_origins();
    d.publish((*online.value).clone(), BTreeMap::new());
    s.poll();
    assert!(!s.volume_active(&t));
    assert!(s.begin_volume(&online, &key).is_err());
    assert!(d.take_requests().is_empty());
}

#[test]
fn noop_fader_forget_requires_fresh_observation_and_does_not_add_history() {
    let (s, d, origin, gens, key) = setup();
    s.toggle_partial_mute(&origin, &key).unwrap();
    let mute = d.take_requests().pop().unwrap();
    let muted = apply(&s, &d, &mute, &gens);
    assert!(s.partial_muted(&key).unwrap());
    let token = s.begin_volume(&muted, &key).unwrap();
    s.update_volume(&token, OFF_DB).unwrap();
    s.end_volume(&token).unwrap();
    let req = d.take_requests().pop().unwrap();
    assert_eq!(values(&req), values(&mute));
    assert!(s.partial_muted(&key).unwrap());
    let next = d.publish((*muted.value).clone(), gens);
    s.poll();
    assert!(s.partial_muted(&key).unwrap());
    report(&req, next.stamp, true);
    s.poll();
    assert!(!s.partial_muted(&key).unwrap());
    assert_eq!(s.history_lengths(), (1, 0));
    assert!(!s.busy());
}
#[test]
fn current_vector_preserves_unselected_changes_while_undo_keeps_initial_full_vector() {
    let (s, d, origin, gens, key) = setup();
    let before = origin
        .value
        .knopen
        .iter()
        .find(|n| n.id == 20)
        .unwrap()
        .volume
        .clone()
        .unwrap();
    let token = s.begin_volume(&origin, &key).unwrap();
    let mut next = (*origin.value).clone();
    let n = next.knopen.iter_mut().find(|n| n.id == 20).unwrap();
    n.serial = None;
    n.volume.as_mut().unwrap()[2] = 0.9;
    d.publish(next, gens.clone());
    s.poll();
    assert!(
        !s.volume_active(&token),
        "serial change is a conservative binding change"
    );
    let origin = s.graph().unwrap();
    let token = s.begin_volume(&origin, &key).unwrap();
    let initial = origin
        .value
        .knopen
        .iter()
        .find(|n| n.id == 20)
        .unwrap()
        .volume
        .clone()
        .unwrap();
    let mut next = (*origin.value).clone();
    next.knopen
        .iter_mut()
        .find(|n| n.id == 20)
        .unwrap()
        .volume
        .as_mut()
        .unwrap()[3] = 0.8;
    d.publish(next, gens.clone());
    s.poll();
    assert!(
        s.volume_active(&token),
        "ordinary gain changes keep the registry identity"
    );
    s.update_volume(&token, -4.).unwrap();
    s.end_volume(&token).unwrap();
    let req = d.take_requests().pop().unwrap();
    assert_eq!(&values(&req)[2..], &[0.9, 0.8]);
    let after = apply(&s, &d, &req, &gens);
    s.undo(&after).unwrap();
    let undo = d.take_requests().pop().unwrap();
    assert_eq!(values(&undo), initial);
    assert_ne!(initial, before);
}
#[test]
fn changed_channel_mapping_cancels_before_dispatch_and_admission_failure_can_recover() {
    let (s, d, origin, gens, key) = setup();
    let token = s.begin_volume(&origin, &key).unwrap();
    let mut next = (*origin.value).clone();
    next.poorten
        .iter_mut()
        .find(|p| p.id == 204)
        .unwrap()
        .volgnummer = Some(9);
    let next = d.publish(next, gens.clone());
    s.poll();
    assert!(!s.volume_active(&token));
    assert!(s.update_volume(&token, -4.).is_err());
    assert!(d.take_requests().is_empty());
    let token = s.begin_volume(&next, &key).unwrap();
    d.forget_origins();
    s.update_volume(&token, -6.).unwrap();
    assert!(!s.busy());
    assert!(matches!(s.status(), Status::Failed(_)));
    assert!(d.take_requests().is_empty());
    let current = d.publish((*next.value).clone(), gens.clone());
    s.poll();
    let token = s.begin_volume(&current, &key).unwrap();
    s.update_volume(&token, -2.).unwrap();
    s.end_volume(&token).unwrap();
    let req = d.take_requests().pop().unwrap();
    apply(&s, &d, &req, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
}

#[test]
fn mismatched_admission_retains_receipt_and_blocks_a_second_group_until_terminal() {
    use pecan_core::{
        history_identity::HistoryGraph,
        inbox::LatestInbox,
        native_command::{native_receipt, AdmissionError, NativeReporter},
    };
    struct Wrong {
        inner: Rc<dyn Endpoint>,
        reporter: RefCell<Option<NativeReporter>>,
    }
    impl Endpoint for Wrong {
        fn witness(&self, g: &Graph) -> Result<HistoryGraph, AdmissionError> {
            self.inner.witness(g)
        }
        fn dispatch(&self, intent: NativeIntent) -> Result<NativeReceipt, AdmissionError> {
            let _admitted = self.inner.dispatch(intent.clone())?;
            let (receipt, reporter) = native_receipt(
                intent.ticket + 1,
                intent.origin.stamp,
                intent.commands.len(),
            );
            *self.reporter.borrow_mut() = Some(reporter);
            Ok(receipt)
        }
    }
    let (_, d, origin, gens, _) = setup();
    let endpoint = Rc::new(Wrong {
        inner: d.endpoint(),
        reporter: RefCell::default(),
    });
    let (inbox, _) = LatestInbox::new(16);
    let s = Session::new(inbox.clone(), endpoint.clone());
    inbox.publish(pecan_core::backend::BackendEvent::Graph(origin.clone()));
    s.poll();
    let key = s.volume_model().hardware[0].key.clone();
    let token = s.begin_volume(&origin, &key).unwrap();
    s.update_volume(&token, -6.).unwrap();
    assert_eq!(d.take_requests().len(), 1);
    assert!(s.busy());
    assert!(s.has_receipt());
    assert!(!s.volume_active(&token));
    assert!(s.begin_volume(&origin, &key).is_err());
    assert!(matches!(s.status(), Status::Unconfirmed(_)));
    endpoint
        .reporter
        .borrow()
        .as_ref()
        .unwrap()
        .set(NativeProgress::Unconfirmed {
            reason: NativeFailure::ObservationTimedOut,
            counts: Counts::default(),
        });
    s.poll();
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (0, 0));
    let next = d.publish((*origin.value).clone(), gens);
    inbox.publish(pecan_core::backend::BackendEvent::Graph(next));
    s.poll();
    assert!(s.begin_volume(&s.graph().unwrap(), &key).is_ok());
}
