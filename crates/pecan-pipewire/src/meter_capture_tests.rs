//! Actual graph selector, registry lifecycle and reconciliation; synthetic
//! capture handles replace only stream creation, never a PipeWire session.
use super::*;
use std::cell::Cell;

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Created(usize, CaptureIdentity),
    Dropped(usize),
}
#[derive(Default)]
struct Tracking {
    next: Cell<usize>,
    events: RefCell<Vec<Event>>,
}
struct SyntheticCapture {
    id: usize,
    tracking: Rc<Tracking>,
}
impl SyntheticCapture {
    fn new(tracking: &Rc<Tracking>, identity: &CaptureIdentity) -> Self {
        let id = tracking.next.get() + 1;
        tracking.next.set(id);
        tracking
            .events
            .borrow_mut()
            .push(Event::Created(id, identity.clone()));
        Self {
            id,
            tracking: tracking.clone(),
        }
    }
}
impl Drop for SyntheticCapture {
    fn drop(&mut self) {
        self.tracking
            .events
            .borrow_mut()
            .push(Event::Dropped(self.id));
    }
}
type Captures = BTreeMap<u32, Capture<SyntheticCapture>>;
fn setup() -> (Staat, Commands, Rc<Tracking>, Captures) {
    let graph: Momentopname = serde_json::from_value(serde_json::json!({
        "verbonden":true,"wireplumber":[],"kabels":[],
        "knopen":[{"id":10,"serial":10010,"naam":"synthetic.capture.source","klasse":"Audio/Source","volume":[1.0,1.0],"mute":false}],
        "poorten":[
            {"id":11,"knoop":10,"volgnummer":0,"naam":"synthetic.output.0","ingang":false,"soort":"audio","kanaal":"FL","monitor":false,"fysiek":false},
            {"id":12,"knoop":10,"volgnummer":1,"naam":"synthetic.output.1","ingang":false,"soort":"audio","kanaal":"FR","monitor":false,"fysiek":false}
        ]
    })).unwrap();
    let state = Staat {
        knopen: graph.knopen.iter().map(|n| (n.id, n.clone())).collect(),
        poorten: graph.poorten.iter().map(|p| (p.id, p.clone())).collect(),
        ..Default::default()
    };
    let commands = Commands::default();
    for id in [10, 11, 12] {
        commands.object_added(id);
    }
    commands.publish(graph);
    let tracking = Rc::new(Tracking::default());
    let mut captures = BTreeMap::new();
    reconcile(&state, &commands, &tracking, &mut captures);
    (state, commands, tracking, captures)
}
fn reconcile(state: &Staat, commands: &Commands, tracking: &Rc<Tracking>, captures: &mut Captures) {
    meter_captures::reconcile(
        captures,
        state.meterbaar(&commands.registry_generations()),
        |_, identity| Some(SyntheticCapture::new(tracking, identity)),
    );
}

#[test]
fn coalesced_serial_and_direction_changes_drop_before_recreating_the_capture() {
    for sink in [false, true] {
        let (mut state, commands, tracking, mut captures) = setup();
        if sink {
            state.knopen.get_mut(&10).unwrap().klasse = Some("Audio/Sink".into());
            for port in state.poorten.values_mut() {
                port.ingang = true;
            }
        } else {
            state.knopen.get_mut(&10).unwrap().serial = Some(20020);
        }
        reconcile(&state, &commands, &tracking, &mut captures);
        assert_eq!(captures[&10].source.id, 2);
        let events = tracking.events.borrow();
        assert_eq!(events[1], Event::Dropped(1));
        let Event::Created(2, identity) = &events[2] else {
            panic!("replacement must be created after dropping the old source")
        };
        assert_eq!(identity.serial, if sink { 10010 } else { 20020 });
        assert_eq!(identity.sink, sink);
        assert_eq!(identity.channels, ["FL", "FR"]);
    }
}

#[test]
fn identical_node_or_port_registry_replacement_recreates_without_a_dto_change() {
    for id in [10, 11] {
        let (state, commands, tracking, mut captures) = setup();
        let before = state.momentopname();
        commands.object_removed(id);
        commands.object_added(id);
        assert_eq!(
            state.momentopname(),
            before,
            "replacement can be hidden by graph coalescing"
        );
        reconcile(&state, &commands, &tracking, &mut captures);
        assert_eq!(captures[&10].source.id, 2);
        assert_eq!(tracking.events.borrow()[1], Event::Dropped(1));
    }
}

#[test]
fn ordinary_graph_revisions_and_unrelated_registry_changes_preserve_capture() {
    let (mut state, commands, tracking, mut captures) = setup();
    for i in 0..200 {
        state.quantum = Some(128 + i);
        state.rate = Some(48000);
        let node = state.knopen.get_mut(&10).unwrap();
        node.volume = Some(vec![0.5, 0.25]);
        node.mute = Some(i % 2 == 0);
        node.beschrijving = Some("Synthetic display label".into());
        commands.object_added(9000 + i);
        reconcile(&state, &commands, &tracking, &mut captures);
    }
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[&10].source.id, 1);
    assert_eq!(tracking.next.get(), 1);
    assert_eq!(tracking.events.borrow().len(), 1);
}

#[test]
fn channel_names_order_and_selected_port_lifecycle_are_capture_identity() {
    for change in 0..3 {
        let (mut state, commands, tracking, mut captures) = setup();
        match change {
            0 => state.poorten.get_mut(&11).unwrap().kanaal = Some("MONO".into()),
            1 => {
                state.poorten.get_mut(&11).unwrap().volgnummer = Some(1);
                state.poorten.get_mut(&12).unwrap().volgnummer = Some(0);
            }
            _ => {
                state.poorten.remove(&12);
                commands.object_removed(12);
            }
        }
        reconcile(&state, &commands, &tracking, &mut captures);
        assert_eq!(captures[&10].source.id, 2);
        let events = tracking.events.borrow();
        let Event::Created(2, identity) = &events[2] else {
            panic!("replacement capture")
        };
        assert_eq!(
            identity.channels,
            match change {
                0 => vec!["MONO", "FR"],
                1 => vec!["FR", "FL"],
                _ => vec!["FL"],
            }
        );
    }
}

#[test]
fn missing_capture_requirements_remove_old_source_without_fabricating_a_replacement() {
    for change in 0..5 {
        let (mut state, commands, tracking, mut captures) = setup();
        match change {
            0 => {
                state.knopen.remove(&10);
            }
            1 => state.knopen.get_mut(&10).unwrap().serial = None,
            2 => state.poorten.get_mut(&11).unwrap().kanaal = None,
            3 => commands.object_removed(10),
            _ => commands.object_removed(11),
        }
        meter_captures::reconcile(
            &mut captures,
            state.meterbaar(&commands.registry_generations()),
            |_, _| -> Option<SyntheticCapture> {
                panic!("unavailable identity cannot create a capture")
            },
        );
        assert!(captures.is_empty());
        assert_eq!(tracking.next.get(), 1);
        assert_eq!(tracking.events.borrow()[1], Event::Dropped(1));
    }
}

#[test]
fn failed_replacement_factory_cannot_keep_old_capture_and_a_later_update_can_recover() {
    let (mut state, commands, tracking, mut captures) = setup();
    state.knopen.get_mut(&10).unwrap().serial = Some(20020);
    let attempts = Cell::new(0);
    meter_captures::reconcile(
        &mut captures,
        state.meterbaar(&commands.registry_generations()),
        |_, _| {
            attempts.set(attempts.get() + 1);
            assert_eq!(tracking.events.borrow()[1], Event::Dropped(1));
            None
        },
    );
    assert!(captures.is_empty());
    assert_eq!(attempts.get(), 1);
    reconcile(&state, &commands, &tracking, &mut captures);
    assert_eq!(captures[&10].source.id, 2);
    assert_eq!(captures.len(), 1);
}

#[test]
fn reconnect_and_read_only_cleanup_release_captures_before_any_new_session_capture() {
    let (state, commands, tracking, mut captures) = setup();
    // The production session clears captures before reconnect. Reconciliation
    // with the read-only/absent desired set has the same drop-only behavior.
    meter_captures::reconcile(
        &mut captures,
        BTreeMap::new(),
        |_, _| -> Option<SyntheticCapture> { panic!("read-only mode must not create streams") },
    );
    assert!(captures.is_empty());
    assert_eq!(tracking.events.borrow()[1], Event::Dropped(1));
    commands.publish(Momentopname::default());
    for id in [10, 11, 12] {
        commands.object_added(id);
    }
    reconcile(&state, &commands, &tracking, &mut captures);
    assert_eq!(captures[&10].source.id, 2);
    captures.clear(); // Actual session-end path, no reconnect-time retention.
    assert_eq!(tracking.events.borrow().last(), Some(&Event::Dropped(2)));
}
