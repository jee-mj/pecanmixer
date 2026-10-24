//! Thin bridge to the exact native backend contract; no alternate completion API.
use super::{Adapter, Controller};
use crate::native_backend::NativeClient;
use pecan_core::{
    backend::Stamped,
    graph::{Momentopname, Opdracht},
    native_command::{NativeProgress, NativeReceipt},
};
use std::sync::Arc;

impl Adapter for NativeClient {
    type Snapshot = Stamped<Arc<Momentopname>>;
    type Request = NativeReceipt;
    fn graph(snapshot: &Self::Snapshot) -> &Stamped<Arc<Momentopname>> {
        snapshot
    }
    fn same_request(a: &NativeReceipt, b: &NativeReceipt) -> bool {
        let a = a.report();
        let b = b.report();
        a.ticket == b.ticket && a.source == b.source
    }
    fn submit(
        &mut self,
        origin: &Self::Snapshot,
        command: Opdracht,
    ) -> Result<NativeReceipt, String> {
        NativeClient::submit(self, origin, vec![command])
            .map_err(|error| format!("Native admission rejected: {error:?}"))
    }
}
impl Controller<NativeClient> {
    /// Poll on the main context. A terminal report is owned by its receipt and
    /// consumed once; queued/local execution never acknowledge observed success.
    pub fn poll_native(&mut self) -> bool {
        let Some(receipt) = self.pending_request().cloned() else {
            return false;
        };
        let report = receipt.report();
        match report.progress {
            NativeProgress::Queued => false,
            NativeProgress::ExecutedLocally(_) => self.executed_locally(&receipt),
            NativeProgress::Observed { stamp, .. } | NativeProgress::ObservedNoOp { stamp, .. } => {
                self.observed_result(&receipt, stamp)
            }
            NativeProgress::Rejected { reason, .. } => {
                self.rejected(&receipt, format!("Native command rejected: {reason:?}"))
            }
            NativeProgress::Unconfirmed { reason, .. } => {
                self.unconfirmed(&receipt, format!("{reason:?}; effects may be present."))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::{tests::snapshot, ControlError, Phase};
    use pecan_core::{
        backend::Stamp,
        domain::buses::BusConfig,
        native_command::{native_receipt, Counts, NativeFailure, NativeIntent, NativeReporter},
    };
    use std::{cell::RefCell, rc::Rc};

    type Graph = Stamped<Arc<Momentopname>>;
    fn setup() -> (
        Controller<NativeClient>,
        Rc<RefCell<Vec<NativeIntent>>>,
        Rc<RefCell<Vec<NativeReporter>>>,
        Graph,
    ) {
        let intents = Rc::new(RefCell::new(Vec::new()));
        let reporters = Rc::new(RefCell::new(Vec::new()));
        let output = intents.clone();
        let producer = reporters.clone();
        let client = NativeClient::for_test(move |intent| {
            let (receipt, reporter) =
                native_receipt(intent.ticket, intent.origin.stamp, intent.commands.len());
            output.borrow_mut().push(intent);
            producer.borrow_mut().push(reporter);
            Ok(receipt)
        });
        let mut controller = Controller::new(client, BusConfig::default());
        let source = snapshot(2, 1).graph;
        controller.observe(source.clone());
        (controller, intents, reporters, source)
    }
    fn binding(c: &Controller<NativeClient>) -> super::super::Binding<Graph> {
        c.bind(&c.model().apps[0].key).unwrap()
    }
    #[test]
    fn client_clones_share_tickets_and_preserve_exact_origin_arc() {
        let intents = Rc::new(RefCell::new(Vec::new()));
        let output = intents.clone();
        let client = NativeClient::for_test(move |intent| {
            let (receipt, _) =
                native_receipt(intent.ticket, intent.origin.stamp, intent.commands.len());
            output.borrow_mut().push(intent);
            Ok(receipt)
        });
        let source = snapshot(2, 1).graph;
        client.submit(&source, vec![]).unwrap();
        client.clone().submit(&source, vec![]).unwrap();
        let calls = intents.borrow();
        assert_eq!(calls.iter().map(|i| i.ticket).collect::<Vec<_>>(), [1, 2]);
        assert!(
            calls
                .iter()
                .all(|i| i.origin.stamp == source.stamp
                    && Arc::ptr_eq(&i.origin.value, &source.value))
        );
    }
    #[test]
    fn local_submission_and_older_frames_cannot_release_observation_gate() {
        let (mut c, intents, reporters, mut graph) = setup();
        c.set_mute(&binding(&c), true).unwrap();
        assert!(Arc::ptr_eq(&intents.borrow()[0].origin.value, &graph.value));
        assert!(!c.poll_native());
        assert_eq!(c.phase(), Phase::WaitingForCompletion);
        reporters.borrow()[0].set(NativeProgress::ExecutedLocally(Counts {
            attempted: 1,
            submitted: 1,
            ..Counts::default()
        }));
        assert!(c.poll_native());
        assert_eq!(c.phase(), Phase::WaitingForObservation);
        assert_eq!(c.model().apps[0].node.mute, Some(false));
        reporters.borrow()[0].set(NativeProgress::Observed {
            stamp: Stamp {
                epoch: 2,
                revision: 9,
            },
            counts: Counts {
                attempted: 1,
                submitted: 1,
                observed: 1,
                ..Counts::default()
            },
        });
        assert!(c.poll_native());
        assert_eq!(c.phase(), Phase::WaitingForGraph);
        assert!(!c.poll_native(), "terminal evidence is consumed once");
        graph.stamp.revision = 2;
        let id = c.model().apps[0].node.id;
        Arc::make_mut(&mut graph.value)
            .knopen
            .iter_mut()
            .find(|n| n.id == id)
            .unwrap()
            .mute = Some(true);
        c.observe(graph.clone());
        assert_eq!(c.phase(), Phase::WaitingForGraph);
        // Another client may change the value after the reported observation.
        // Keep that actual state while accepting the already-proven postcondition.
        graph.stamp.revision = 10;
        Arc::make_mut(&mut graph.value)
            .knopen
            .iter_mut()
            .find(|n| n.id == id)
            .unwrap()
            .mute = Some(false);
        c.observe(graph);
        assert_eq!(c.phase(), Phase::Ready);
        assert_eq!(c.model().apps[0].node.mute, Some(false));
    }
    #[test]
    fn observed_noop_requires_the_new_published_revision() {
        let (mut c, _, reporters, mut graph) = setup();
        c.set_mute(&binding(&c), false).unwrap();
        reporters.borrow()[0].set(NativeProgress::ObservedNoOp {
            stamp: Stamp {
                epoch: 2,
                revision: 2,
            },
            counts: Counts {
                attempted: 1,
                no_ops: 1,
                observed: 1,
                ..Counts::default()
            },
        });
        c.poll_native();
        assert_eq!(c.phase(), Phase::WaitingForGraph);
        assert!(!c.observe(graph.clone()));
        graph.stamp.revision += 1;
        c.observe(graph);
        assert_eq!(c.phase(), Phase::Ready);
    }
    #[test]
    fn rejected_and_unconfirmed_remain_distinct_and_keep_actual_values() {
        for uncertain in [false, true] {
            let (mut c, _, reporters, _) = setup();
            c.set_mute(&binding(&c), true).unwrap();
            reporters.borrow()[0].set(if uncertain {
                NativeProgress::Unconfirmed {
                    reason: NativeFailure::ObservationTimedOut,
                    counts: Counts {
                        attempted: 1,
                        submitted: 1,
                        ..Counts::default()
                    },
                }
            } else {
                NativeProgress::Rejected {
                    reason: NativeFailure::InvalidObject,
                    counts: Counts::default(),
                }
            });
            assert!(c.poll_native());
            if uncertain {
                assert!(
                    matches!(c.phase(), Phase::Unconfirmed(message) if message.contains("effects may be present"))
                );
            } else {
                assert!(matches!(c.phase(), Phase::Failed(_)));
            }
            assert_eq!(c.model().apps[0].node.mute, Some(false));
            assert!(c.enabled());
            assert!(!c.poll_native());
        }
    }
    #[test]
    fn native_admission_error_is_visible_without_optimistic_changes() {
        let client =
            NativeClient::for_test(|_| Err(pecan_core::native_command::AdmissionError::QueueFull));
        let mut c = Controller::new(client, BusConfig::default());
        c.observe(snapshot(2, 1).graph);
        let before = c.model().clone();
        assert!(
            matches!(c.set_mute(&binding(&c), true), Err(ControlError::Rejected(message)) if message.contains("QueueFull"))
        );
        assert!(matches!(c.phase(), Phase::Failed(_)));
        assert_eq!(c.model(), &before);
    }
}
