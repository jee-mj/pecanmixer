//! Hand-authored synthetic graphs only; no audio server or user profile access.
use super::*;
use pecan_core::{
    backend::{Stamp, Stamped},
    domain::{buses::BusConfig, levels::db_to_linear},
    graph::{Momentopname, Opdracht},
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) graph: Stamped<Arc<Momentopname>>,
    witness: u64,
}
#[derive(Default)]
pub(super) struct Log {
    pub(super) calls: Vec<(u64, Stamp, Opdracht)>,
    reject: bool,
}
pub(super) struct Fake(Rc<RefCell<Log>>);
impl Adapter for Fake {
    type Snapshot = Snapshot;
    type Request = usize;
    fn same_request(a: &usize, b: &usize) -> bool {
        a == b
    }
    fn graph(snapshot: &Snapshot) -> &Stamped<Arc<Momentopname>> {
        &snapshot.graph
    }
    fn submit(&mut self, origin: &Snapshot, command: Opdracht) -> Result<usize, String> {
        let mut log = self.0.borrow_mut();
        if log.reject {
            return Err("Synthetic rejection".into());
        }
        log.calls
            .push((origin.witness, origin.graph.stamp, command));
        Ok(log.calls.len())
    }
}
pub(super) fn snapshot(epoch: u64, revision: u64) -> Snapshot {
    Snapshot {
        graph: Stamped {
            stamp: Stamp { epoch, revision },
            value: Arc::new(
                serde_json::from_str(include_str!(
                    "../../../../fixtures/domain/audio-policy.json"
                ))
                .unwrap(),
            ),
        },
        witness: 1000 + revision,
    }
}
pub(super) fn mutate(snapshot: &mut Snapshot, f: impl FnOnce(&mut Momentopname)) {
    f(Arc::make_mut(&mut snapshot.graph.value));
}
pub(super) fn setup() -> (Controller<Fake>, Rc<RefCell<Log>>, Snapshot) {
    let log = Rc::new(RefCell::new(Log::default()));
    let mut controller = Controller::new(Fake(log.clone()), BusConfig::default());
    let graph = snapshot(2, 1);
    assert!(controller.observe(graph.clone()));
    (controller, log, graph)
}
pub(super) fn app_binding(controller: &Controller<Fake>) -> Binding<Snapshot> {
    controller.bind(&controller.model().apps[0].key).unwrap()
}

#[test]
fn admission_is_not_completion_or_observation() {
    let (mut c, log, mut next) = setup();
    let binding = app_binding(&c);
    let before = c.model().apps[0].node.volume.clone();
    c.set_volume(&binding, -6.0).unwrap();
    assert_eq!(c.phase(), Phase::WaitingForCompletion);
    assert_eq!(c.model().apps[0].node.volume, before);
    assert!(!c.enabled());
    assert_eq!(c.set_mute(&binding, true), Err(ControlError::Busy));
    assert_eq!(log.borrow().calls[0].0, 1001);
    assert_eq!(
        log.borrow().calls[0].1,
        Stamp {
            epoch: 2,
            revision: 1
        }
    );
    let Opdracht::ZetVolume { knoop, volume } = log.borrow().calls[0].2.clone() else {
        panic!("volume")
    };
    next.graph.stamp.revision += 1;
    mutate(&mut next, |g| {
        g.knopen.iter_mut().find(|n| n.id == knoop).unwrap().volume = Some(volume)
    });
    assert!(c.observe(next));
    assert_eq!(c.phase(), Phase::WaitingForCompletion);
    assert!(!c.observed_result(
        &999,
        Stamp {
            epoch: 2,
            revision: 2
        }
    ));
    assert!(c.observed_result(
        &1,
        Stamp {
            epoch: 2,
            revision: 2
        }
    ));
    assert_eq!(c.phase(), Phase::Ready);
    assert!(c.enabled());
}

#[test]
fn completion_requires_newer_observation_including_noop() {
    let (mut c, _, mut next) = setup();
    c.set_mute(&app_binding(&c), false).unwrap();
    assert!(c.observed_result(
        &1,
        Stamp {
            epoch: 2,
            revision: 2
        }
    ));
    assert_eq!(c.phase(), Phase::WaitingForGraph);
    assert!(!c.observe(next.clone()));
    assert_eq!(c.phase(), Phase::WaitingForGraph);
    next.graph.stamp.revision += 1;
    assert!(c.observe(next));
    assert_eq!(c.phase(), Phase::Ready);
}

#[test]
fn frames_older_than_observed_result_do_not_clear_pending() {
    let (mut c, log, mut next) = setup();
    c.set_mute(&app_binding(&c), true).unwrap();
    c.observed_result(
        &1,
        Stamp {
            epoch: 2,
            revision: 3,
        },
    );
    next.graph.stamp.revision += 1;
    c.observe(next.clone());
    assert_eq!(c.phase(), Phase::WaitingForGraph);
    let Opdracht::ZetMute { knoop, .. } = log.borrow().calls[0].2 else {
        panic!("mute")
    };
    next.graph.stamp.revision += 1;
    mutate(&mut next, |g| {
        g.knopen.iter_mut().find(|n| n.id == knoop).unwrap().mute = Some(true)
    });
    c.observe(next);
    assert_eq!(c.phase(), Phase::Ready);
}

#[test]
fn old_binding_is_rejected_without_adopting_new_origin() {
    let (mut c, log, mut next) = setup();
    let binding = app_binding(&c);
    next.graph.stamp.revision += 1;
    next.witness = 9999;
    c.observe(next);
    assert_eq!(c.set_volume(&binding, 0.0), Err(ControlError::StaleView));
    assert!(log.borrow().calls.is_empty());
    c.set_volume(&app_binding(&c), 0.0).unwrap();
    assert_eq!(log.borrow().calls[0].0, 9999);
}

#[test]
fn epoch_disconnect_and_generation_changes_invalidate_pending() {
    for mode in 0..4 {
        let (mut c, log, mut next) = setup();
        c.set_mute(&app_binding(&c), true).unwrap();
        let Opdracht::ZetMute { knoop, .. } = log.borrow().calls[0].2 else {
            panic!("mute")
        };
        next.graph.stamp.revision += 1;
        match mode {
            0 => next.graph.stamp.epoch += 1,
            1 => mutate(&mut next, |g| g.verbonden = false),
            2 => mutate(&mut next, |g| {
                g.knopen.iter_mut().find(|n| n.id == knoop).unwrap().serial = Some(9000)
            }),
            _ => mutate(&mut next, |g| g.knopen.retain(|n| n.id != knoop)),
        }
        c.observe(next);
        assert!(c.pending_request().is_none());
        assert!(!c.observed_result(
            &1,
            Stamp {
                epoch: 2,
                revision: 2
            }
        ));
        assert!(matches!(
            c.phase(),
            Phase::Disconnected | Phase::Unconfirmed(_)
        ));
    }
}

#[test]
fn rejection_leaves_observed_values_and_allows_retry() {
    let (mut c, log, _) = setup();
    let before = c.model().clone();
    log.borrow_mut().reject = true;
    assert_eq!(
        c.set_mute(&app_binding(&c), true),
        Err(ControlError::Rejected("Synthetic rejection".into()))
    );
    assert_eq!(c.model(), &before);
    assert!(matches!(c.phase(), Phase::Failed(_)));
    log.borrow_mut().reject = false;
    c.set_mute(&app_binding(&c), true).unwrap();
    assert!(!c.rejected(&999, "Unrelated request"));
    assert!(c.rejected(&1, "Synthetic execution failure"));
    assert_eq!(c.model(), &before);
    assert!(c.enabled());
    assert!(matches!(c.phase(), Phase::Failed(_)));
}

#[test]
fn selected_pair_volume_preserves_other_channels_and_mute_is_explicitly_limited() {
    let (mut c, log, _) = setup();
    let strip = c
        .model()
        .destinations
        .iter()
        .find(|s| s.node.id == 10 && s.channels == [2, 3])
        .unwrap()
        .clone();
    let binding = c.bind(&strip.key).unwrap();
    assert_eq!(c.set_mute(&binding, true), Err(ControlError::Unsupported));
    c.set_volume(&binding, -12.0).unwrap();
    let Opdracht::ZetVolume { volume, .. } = &log.borrow().calls[0].2 else {
        panic!("volume")
    };
    assert_eq!(
        volume,
        &vec![1.0, 1.0, db_to_linear(-12.0), db_to_linear(-12.0)]
    );
}

#[test]
fn invalid_values_missing_serial_and_rec_bus_are_not_dispatched() {
    let (mut c, log, mut next) = setup();
    let binding = app_binding(&c);
    for value in [f64::NAN, f64::INFINITY, -91.0, 6.01] {
        assert_eq!(
            c.set_volume(&binding, value),
            Err(ControlError::InvalidValue)
        );
    }
    let bus = c
        .model()
        .destinations
        .iter()
        .find(|s| s.bus.is_some())
        .unwrap()
        .key
        .clone();
    assert_eq!(
        c.set_volume(&c.bind(&bus).unwrap(), 0.0),
        Err(ControlError::Unsupported)
    );
    next.graph.stamp.revision += 1;
    let id = c.model().apps[0].node.id;
    mutate(&mut next, |g| {
        g.knopen.iter_mut().find(|n| n.id == id).unwrap().serial = None
    });
    c.observe(next);
    assert_eq!(
        c.set_volume(&app_binding(&c), 0.0),
        Err(ControlError::Unsupported)
    );
    assert!(log.borrow().calls.is_empty());
}

#[test]
fn default_controller_stays_read_only() {
    let mut c = Controller::default();
    c.observe(snapshot(2, 1).graph);
    assert_eq!(c.phase(), Phase::ReadOnly);
    assert!(!c.enabled());
    let binding = c.bind(&c.model().apps[0].key).unwrap();
    assert_eq!(c.set_mute(&binding, true), Err(ControlError::ReadOnly));
    assert_eq!(c.set_volume(&binding, 0.0), Err(ControlError::ReadOnly));
}
