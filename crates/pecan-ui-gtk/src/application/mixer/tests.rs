use super::*;
use crate::application::{ApplicationOwner, FixtureDriver, FixtureRequest};
use pecan_core::{
    graph::{Momentopname, Opdracht},
    native_command::{Counts, NativeProgress},
};
use std::sync::Arc;
pub(super) fn setup() -> (
    Session,
    FixtureDriver,
    CoordinatedMixer,
    BTreeMap<u32, u64>,
    String,
) {
    let owner = ApplicationOwner::fixture();
    let s = owner.activate();
    let d = owner.fixture_driver().unwrap();
    let mut g: Momentopname = serde_json::from_str(include_str!(
        "../../../../../fixtures/graph/audio-basic.json"
    ))
    .unwrap();
    let n = g.knopen.iter_mut().find(|n| n.id == 20).unwrap();
    n.klasse = Some("Audio/Source".into());
    n.volume = Some(vec![0.2, 0.6, 0.3, 0.7]);
    n.app = Some("Synthetic source".into());
    n.media = None;
    for p in g.poorten.iter_mut().filter(|p| p.knoop == 20) {
        p.kanaal = Some(format!("AUX{}", p.volgnummer.unwrap()));
    }
    for (id, index) in [(203, 2), (204, 3)] {
        let mut p = g.poorten.iter().find(|p| p.id == 201).unwrap().clone();
        p.id = id;
        p.volgnummer = Some(index);
        p.kanaal = Some(format!("AUX{index}"));
        p.naam = format!("synthetic_{index}");
        g.poorten.push(p);
    }
    let gens = g
        .knopen
        .iter()
        .map(|n| (n.id, n.id as u64))
        .chain(g.poorten.iter().map(|p| (p.id, p.id as u64)))
        .collect::<BTreeMap<_, _>>();
    d.publish(g, gens.clone());
    s.poll();
    let key = s.volume_model().hardware[0].key.clone();
    let view = CoordinatedMixer::new(s.clone());
    (s, d, view, gens, key)
}
pub(super) fn row(view: &CoordinatedMixer, key: &str) -> Rc<Row> {
    view.0.rows.borrow()[key].clone()
}
pub(super) fn apply(
    s: &Session,
    d: &FixtureDriver,
    view: &CoordinatedMixer,
    req: &FixtureRequest,
    gens: &BTreeMap<u32, u64>,
) {
    let mut g = (*s.graph().unwrap().value).clone();
    let [Opdracht::ZetVolume { knoop, volume }] = &req.intent.commands[..] else {
        panic!()
    };
    g.knopen.iter_mut().find(|n| n.id == *knoop).unwrap().volume = Some(volume.clone());
    let next = d.publish(g, gens.clone());
    req.reporter.set(NativeProgress::Observed {
        stamp: next.stamp,
        counts: Counts::default(),
    });
    s.poll();
    view.refresh();
}
#[gtk::test]
fn relative_drag_remains_sensitive_across_observation_and_finalizes_once() {
    let (s, d, v, gens, key) = setup();
    let r = row(&v, &key);
    r.scale.allocate(72, 200, -1, None);
    r.pointer_down(100.);
    r.pointer_move(75., false);
    let first = d.take_requests().pop().unwrap();
    v.refresh();
    assert!(
        r.scale.is_sensitive(),
        "admitted sample must not cancel gesture"
    );
    assert!(r.observed.text().contains("-8.0"));
    r.pointer_move(50., true);
    r.pointer_move(25., false);
    assert!(d.take_requests().is_empty());
    apply(&s, &d, &v, &first, &gens);
    let last = d.take_requests().pop().unwrap();
    assert!(Rc::ptr_eq(&r, &row(&v, &key)));
    assert!(r.policy.borrow().dragging());
    r.pointer_end(true);
    assert_eq!(s.history_lengths(), (0, 0));
    apply(&s, &d, &v, &last, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
    assert!(!s.busy());
    assert!(!r.policy.borrow().dragging());
    // Fresh input after pointer cancellation uses a new token and no old origin.
    r.pointer_down(70.);
    r.pointer_move(90., false);
    r.pointer_end(false);
    let req = d.take_requests().pop().unwrap();
    apply(&s, &d, &v, &req, &gens);
    assert_eq!(s.history_lengths(), (2, 0));
}
#[gtk::test]
fn wheel_groups_six_hundred_ms_uses_observed_db_fine_step_and_reset() {
    let (s, d, v, gens, key) = setup();
    let r = row(&v, &key);
    let db = r.policy.borrow().observed_db();
    r.wheel(-1., true, Duration::ZERO);
    let first = d.take_requests().pop().unwrap();
    r.wheel(-1., true, Duration::from_millis(100));
    assert!(d.take_requests().is_empty());
    v.advance(Duration::from_millis(699));
    assert_eq!(s.history_lengths(), (0, 0));
    apply(&s, &d, &v, &first, &gens);
    let last = d.take_requests().pop().unwrap();
    assert_eq!(
        first.intent.commands, last.intent.commands,
        "legacy wheel uses observed value, not optimistic previous proposal"
    );
    let [Opdracht::ZetVolume { volume, .. }] = &first.intent.commands[..] else {
        panic!()
    };
    let actual = 20. * f64::from(volume[0]).log10();
    assert!((actual - (db * 10. + 1.).round() / 10.).abs() < 1e-5);
    apply(&s, &d, &v, &last, &gens);
    assert!(s.busy());
    v.advance(Duration::from_millis(700));
    assert!(!s.busy());
    assert_eq!(s.history_lengths(), (1, 0));
    r.reset();
    let req = d.take_requests().pop().unwrap();
    let [Opdracht::ZetVolume { volume, .. }] = &req.intent.commands[..] else {
        panic!()
    };
    assert_eq!(&volume[..2], &[1., 1.]);
    assert_eq!(&volume[2..], &[0.3, 0.7]);
    apply(&s, &d, &v, &req, &gens);
    assert_eq!(s.history_lengths(), (2, 0));
}
#[gtk::test]
fn retired_rows_cannot_adopt_replacement_and_fresh_input_recovers() {
    let (s, d, v, mut gens, key) = setup();
    let old = row(&v, &key);
    old.pointer_down(100.);
    old.pointer_move(90., false);
    let req = d.take_requests().pop().unwrap();
    *gens.get_mut(&20).unwrap() += 100;
    let next = d.publish((*s.graph().unwrap().value).clone(), gens.clone());
    s.poll();
    v.refresh();
    let fresh = row(&v, &key);
    assert!(!Rc::ptr_eq(&old, &fresh));
    assert!(!old.live.get());
    old.pointer_move(10., false);
    old.wheel(-1., false, Duration::ZERO);
    old.scale.set_value(1.);
    assert!(d.take_requests().is_empty());
    assert!(!fresh.scale.is_sensitive());
    req.reporter.set(NativeProgress::Observed {
        stamp: next.stamp,
        counts: Counts::default(),
    });
    s.poll();
    v.refresh();
    assert!(fresh.scale.is_sensitive());
    fresh.pointer_down(50.);
    fresh.pointer_move(40., false);
    fresh.pointer_end(false);
    let req = d.take_requests().pop().unwrap();
    apply(&s, &d, &v, &req, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
}
#[gtk::test]
fn partial_mute_click_and_discrete_accessibility_gain_wait_for_real_receipt() {
    let (s, d, v, gens, key) = setup();
    let r = row(&v, &key);
    r.mute.emit_clicked();
    let req = d.take_requests().pop().unwrap();
    assert!(!r.mute.is_active(), "refresh must retain observed mute");
    assert!(!r.scale.is_sensitive());
    apply(&s, &d, &v, &req, &gens);
    assert!(r.mute.is_active());
    r.scale.set_value(0.8);
    let req = d.take_requests().pop().unwrap();
    assert!(s.partial_muted(&key).unwrap());
    apply(&s, &d, &v, &req, &gens);
    assert!(!r.mute.is_active());
    assert!(!s.partial_muted(&key).unwrap());
    assert_eq!(s.history_lengths(), (2, 0));
}
#[gtk::test]
fn held_keyboard_focus_end_and_repeated_poll_do_not_strand_fresh_input() {
    let (s, d, v, gens, key) = setup();
    let r = row(&v, &key);
    assert!(r.key(gdk::Key::Right, false));
    let first = d.take_requests().pop().unwrap();
    for _ in 0..100 {
        r.key(gdk::Key::Right, true);
    }
    r.end_keys();
    assert!(d.take_requests().is_empty());
    apply(&s, &d, &v, &first, &gens);
    let last = d.take_requests().pop().unwrap();
    apply(&s, &d, &v, &last, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
    assert!(r.keys.borrow().is_empty());
    r.key(gdk::Key::Right, false);
    r.end_keys();
    let req = d.take_requests().pop().unwrap();
    apply(&s, &d, &v, &req, &gens);
    assert_eq!(s.history_lengths(), (2, 0));
    for _ in 0..100 {
        v.poll();
    }
    assert!(d.take_requests().is_empty());
}

#[gtk::test]
fn whole_node_mute_does_not_require_a_gain_parameter_and_replacement_retires_it() {
    let (s, d, v, mut gens, key) = setup();
    let mut g = (*s.graph().unwrap().value).clone();
    let n = g.knopen.iter_mut().find(|n| n.id == 20).unwrap();
    n.volume = None;
    n.serial = None;
    d.publish(g, gens.clone());
    s.poll();
    v.refresh();
    let old = row(&v, &key);
    assert!(!old.scale.is_sensitive());
    assert!(old.mute.is_sensitive());
    old.mute.set_active(true);
    let req = d.take_requests().pop().unwrap();
    assert_eq!(
        req.intent.commands,
        vec![Opdracht::ZetMute {
            knoop: 20,
            mute: true
        }]
    );
    *gens.get_mut(&20).unwrap() += 100;
    let next = d.publish((*s.graph().unwrap().value).clone(), gens.clone());
    s.poll();
    v.refresh();
    req.reporter.set(NativeProgress::Rejected {
        reason: pecan_core::native_command::NativeFailure::InvalidObject,
        counts: Counts::default(),
    });
    s.poll();
    v.refresh();
    assert!(!old.live.get());
    old.mute.set_active(false);
    assert!(d.take_requests().is_empty());
    let fresh = row(&v, &key);
    assert!(fresh.mute.is_sensitive());
    assert!(Arc::ptr_eq(&fresh.origin.borrow().value, &next.value));
}

#[gtk::test]
fn dropping_view_retires_unsubmitted_and_inflight_input_without_losing_receipt() {
    for submit in [false, true] {
        let (s, d, v, gens, key) = setup();
        let old = row(&v, &key);
        old.pointer_down(50.);
        let req = if submit {
            old.pointer_move(60., false);
            Some(d.take_requests().pop().unwrap())
        } else {
            None
        };
        assert!(s.busy());
        drop(v);
        assert!(!old.live.get());
        assert_eq!(s.busy(), submit);
        old.pointer_move(90., false);
        old.scale.set_value(1.);
        assert!(d.take_requests().is_empty());
        if let Some(req) = req {
            let mut g = (*s.graph().unwrap().value).clone();
            let [Opdracht::ZetVolume { knoop, volume }] = &req.intent.commands[..] else {
                panic!()
            };
            g.knopen.iter_mut().find(|n| n.id == *knoop).unwrap().volume = Some(volume.clone());
            let next = d.publish(g, gens);
            req.reporter.set(NativeProgress::Observed {
                stamp: next.stamp,
                counts: Counts::default(),
            });
            s.poll();
            assert!(!s.busy());
            assert_eq!(s.history_lengths(), (0, 0));
        }
        let fresh = CoordinatedMixer::new(s.clone());
        let r = row(&fresh, &key);
        r.pointer_down(70.);
        r.pointer_move(80., false);
        r.pointer_end(false);
        assert_eq!(d.take_requests().len(), 1);
    }
}
