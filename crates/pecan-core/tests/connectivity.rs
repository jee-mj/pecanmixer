use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pecan_core::backend::{
    validate_command, Backend, BackendCommand, BackendEvent, DispatchError, EventSequencer, Notice,
    Stamped,
};
use pecan_core::graph::{Activiteit, Momentopname, Opdracht, Opties, Soort, TransportStand};
use pecan_core::inbox::{InboxBatch, LatestInbox};
use pecan_core::overview::{project, Overview, OverviewState};

const AUDIO: &str = include_str!("../../../fixtures/graph/audio-basic.json");
const DISCONNECTED: &str = include_str!("../../../fixtures/graph/disconnected.json");
const RECONNECTED: &str = include_str!("../../../fixtures/graph/reconnect-after.json");

fn graph(json: &str) -> Momentopname {
    serde_json::from_str(json).expect("committed graph fixture")
}

#[test]
fn wire_payloads_round_trip_without_borrowed_strings_or_field_changes() {
    fn send_sync<T: Send + Sync + 'static>() {}
    send_sync::<Momentopname>();
    send_sync::<BackendEvent>();
    for input in [AUDIO, DISCONNECTED, RECONNECTED] {
        let owned = graph(input);
        assert_eq!(
            serde_json::to_value(&owned).unwrap(),
            serde_json::from_str::<serde_json::Value>(input).unwrap()
        );
    }
    let input = include_str!("../../../fixtures/transport.json");
    let transport: TransportStand = serde_json::from_str(input).unwrap();
    assert_eq!(
        serde_json::to_value(&transport).unwrap(),
        serde_json::from_str::<serde_json::Value>(input).unwrap()
    );
    let options = serde_json::json!({
        "bussen": [{ "naam": "Stereo", "kanalen": [0, 1] }],
        "midi": false, "klok": null, "start_stop": false
    });
    let decoded: Opties = serde_json::from_value(options.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), options);
    let activity: Activiteit = serde_json::from_str(
        r#"{"events":12,"kanalen":32769,"soorten":15,"laatste":"Note On C4"}"#,
    )
    .unwrap();
    assert_eq!(activity.kanalen, 32769);
    assert_eq!(
        serde_json::from_value::<Activiteit>(serde_json::to_value(&activity).unwrap()).unwrap(),
        activity
    );
    for (kind, name) in [
        (Soort::Audio, "audio"),
        (Soort::Midi, "midi"),
        (Soort::Anders, "anders"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), name);
        assert_eq!(serde_json::from_value::<Soort>(name.into()).unwrap(), kind);
    }
}

#[test]
fn overview_is_stable_across_input_order_and_matches_reviewed_expectation() {
    let mut input = graph(AUDIO);
    let expected: Overview = serde_json::from_str(include_str!(
        "../../../fixtures/expected/audio-basic-overview.json"
    ))
    .unwrap();
    assert_eq!(project(&input), expected);
    input.knopen.reverse();
    input.poorten.reverse();
    assert_eq!(project(&input), expected);
    // A disconnect must never retain nodes or a previous rate, even if a
    // producer accidentally leaves stale data in its disconnected snapshot.
    input.verbonden = false;
    assert_eq!(project(&input), Overview::default());
}

#[test]
fn ten_thousand_frames_keep_one_latest_slot_and_one_wake() {
    let (inbox, wake) = LatestInbox::new(4);
    let mut sequence = EventSequencer::default();
    for value in 0..10_000 {
        let mut input = graph(AUDIO);
        input.quantum = Some(value);
        inbox.publish(sequence.graph(input));
        inbox.publish(sequence.peaks(BTreeMap::from([(20, vec![value as f32])])));
        inbox.publish(sequence.midi(BTreeMap::from([(
            201,
            Activiteit {
                events: value,
                ..Default::default()
            },
        )])));
        inbox.publish(sequence.transport(TransportStand {
            seconden: f64::from(value),
            ..Default::default()
        }));
    }
    assert_eq!(wake.len(), 1);
    wake.try_recv().unwrap();
    assert!(wake.try_recv().is_err());
    let batch = inbox.drain();
    assert_eq!(batch.graph.unwrap().value.quantum, Some(9999));
    assert_eq!(batch.peaks.unwrap().value[&20], [9999.0]);
    assert_eq!(batch.midi.unwrap().value[&201].events, 9999);
    assert_eq!(batch.transport.unwrap().value.seconden, 9999.0);
    let empty = inbox.drain();
    assert!(
        empty.graph.is_none()
            && empty.peaks.is_none()
            && empty.midi.is_none()
            && empty.transport.is_none()
    );
    inbox.publish(sequence.graph(graph(DISCONNECTED)));
    assert_eq!(
        wake.len(),
        1,
        "a later publication wakes the receiver again"
    );
}

#[test]
fn low_rate_notices_preserve_order_and_report_bounded_overflow() {
    let (inbox, wake) = LatestInbox::new(2);
    for text in ["first", "second", "third", "fourth"] {
        inbox.publish(BackendEvent::Notice(Notice::Message(text.into())));
    }
    assert_eq!(wake.len(), 1);
    assert_eq!(
        inbox.drain().notices,
        [
            Notice::Message("first".into()),
            Notice::Message("second".into()),
            Notice::InboxOverflow { dropped: 2 }
        ]
    );
    assert!(inbox.drain().notices.is_empty());
}

struct FakeBackend {
    current: Mutex<Stamped<Arc<Momentopname>>>,
    dispatched: Mutex<Vec<Opdracht>>,
}

impl Backend for FakeBackend {
    fn current_graph(&self) -> Stamped<Arc<Momentopname>> {
        self.current.lock().unwrap().clone()
    }

    fn dispatch(&self, command: BackendCommand) -> Result<(), DispatchError> {
        let current = self.current.lock().unwrap();
        validate_command(&command, &current)?;
        self.dispatched.lock().unwrap().push(command.command);
        Ok(())
    }
}

#[test]
fn reconnect_clears_session_frames_and_rejects_stale_fake_backend_commands() {
    let (inbox, _) = LatestInbox::new(2);
    let mut sequence = EventSequencer::default();
    let first = sequence.graph(graph(AUDIO));
    inbox.publish(first.clone());
    let mut state = OverviewState::default();
    assert!(state.apply(inbox.drain()));
    let initial_epoch = state.stamp().unwrap().epoch;
    let BackendEvent::Graph(initial) = first else {
        unreachable!()
    };
    let backend = FakeBackend {
        current: Mutex::new(initial.clone()),
        dispatched: Mutex::new(vec![]),
    };
    let old_command = BackendCommand {
        epoch: initial_epoch,
        command: Opdracht::Verbind {
            uit: 201,
            naar: 101,
        },
    };
    backend.dispatch(old_command.clone()).unwrap();
    assert_eq!(backend.dispatched.lock().unwrap().len(), 1);

    inbox.publish(sequence.peaks(BTreeMap::from([(20, vec![1.0])])));
    inbox.publish(sequence.graph(graph(DISCONNECTED)));
    let batch = inbox.drain();
    assert!(batch.peaks.is_none());
    *backend.current.lock().unwrap() = batch.graph.clone().unwrap();
    assert_eq!(
        backend.dispatch(old_command.clone()),
        Err(DispatchError::StaleEpoch)
    );
    let disconnected_command = BackendCommand {
        epoch: backend.current_graph().stamp.epoch,
        command: Opdracht::Start,
    };
    assert_eq!(
        backend.dispatch(disconnected_command),
        Err(DispatchError::Disconnected)
    );
    assert!(state.apply(batch));
    assert!(state.overview().nodes.is_empty());

    inbox.publish(sequence.graph(graph(RECONNECTED)));
    // Delayed data from the old epoch must not repopulate the new inbox.
    inbox.publish(BackendEvent::Peaks(Stamped {
        stamp: initial.stamp,
        value: Arc::new(BTreeMap::from([(20, vec![9.0])])),
    }));
    inbox.publish(BackendEvent::Graph(initial.clone()));
    let batch = inbox.drain();
    assert!(batch.peaks.is_none());
    *backend.current.lock().unwrap() = batch.graph.clone().unwrap();
    assert!(state.apply(batch));
    assert!(state.stamp().unwrap().epoch > initial_epoch);
    assert_eq!(state.overview().nodes[0].id, 1010);
    assert_eq!(
        backend.dispatch(old_command),
        Err(DispatchError::StaleEpoch)
    );
    assert_eq!(
        backend.dispatched.lock().unwrap().len(),
        1,
        "stale commands never reach fake execution"
    );
    assert!(!state.apply(InboxBatch {
        graph: Some(initial),
        ..Default::default()
    }));
    assert_eq!(state.overview().nodes[0].id, 1010);
}

#[test]
fn fake_producer_owned_event_reaches_the_same_read_only_controller() {
    let (inbox, wake) = LatestInbox::new(1);
    let producer = Arc::clone(&inbox);
    std::thread::spawn(move || {
        let mut sequence = EventSequencer::default();
        producer.publish(sequence.graph(graph(AUDIO)));
    })
    .join()
    .unwrap();
    wake.recv_blocking().unwrap();
    let mut state = OverviewState::default();
    assert!(state.apply(inbox.drain()));
    assert!(state.overview().connected);
    assert_eq!(state.overview().nodes.len(), 3);
    assert!(!state.apply(InboxBatch::default()));
}
