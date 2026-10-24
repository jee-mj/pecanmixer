//! Run on one GTK thread with a display (e.g. Xvfb); never opens an audio server.
use std::time::{Duration, Instant};

use gtk::{glib, prelude::*};
use pecan_core::{
    backend::{EventSequencer, Notice},
    graph::{Momentopname, Soort},
    inbox::LatestInbox,
    overview::{project, OverviewState, PortOverview},
};

use super::MixerOverview;

const AUDIO: &str = include_str!("../../../../fixtures/graph/audio-basic.json");
const DISCONNECTED: &str = include_str!("../../../../fixtures/graph/disconnected.json");
const RECONNECTED: &str = include_str!("../../../../fixtures/graph/reconnect-after.json");

fn settle() {
    // Let layout/frame-clock sources run, with an explicit wall-time bound.
    // Allocation assertions below do not prove frame presentation/performance.
    let context = glib::MainContext::default();
    let until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < until {
        while context.pending() && Instant::now() < until {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn text_in(widget: &impl IsA<gtk::Widget>) -> Vec<String> {
    let widget = widget.as_ref();
    let mut values = vec![];
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        values.push(label.text().to_string());
    }
    let mut next = widget.first_child();
    while let Some(child) = next {
        values.extend(text_in(&child));
        next = child.next_sibling();
    }
    values
}

fn snapshot_if_requested(window: &gtk::Window, name: &str) {
    if let Some(directory) = std::env::var_os("PECAN_GTK_TEST_SNAPSHOTS") {
        settle();
        let paintable = gtk::WidgetPaintable::new(Some(window));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            &snapshot,
            f64::from(window.width()),
            f64::from(window.height()),
        );
        let node = snapshot.to_node().expect("allocated fixture snapshot");
        let texture = window.renderer().unwrap().render_texture(&node, None);
        texture
            .save_to_png(std::path::PathBuf::from(directory).join(name))
            .unwrap();
    }
}

#[test]
fn fixture_widget_contracts() {
    gtk::init().expect("GTK display required: run this test under xvfb-run + dbus-run-session");
    let input: Momentopname = serde_json::from_str(AUDIO).unwrap();
    let overview = project(&input);
    let mut view = MixerOverview::new();
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(600)
        .child(view.widget())
        .build();
    window.present();
    settle();
    assert!(view.empty.is_visible());
    assert!(!view.scroll.is_visible());
    assert_eq!(view.status.text(), "Disconnected · waiting for PipeWire");
    assert_eq!(
        view.status
            .property::<gtk::AccessibleRole>("accessible-role"),
        gtk::AccessibleRole::Status
    );

    view.update(&overview);
    settle();
    assert_eq!(view.nodes.len(), 3);
    assert_eq!(
        view.order,
        overview.nodes.iter().map(|n| n.id).collect::<Vec<_>>()
    );
    assert!(view.scroll.is_visible());
    assert!(!view.empty.is_visible());
    let values = text_in(view.widget());
    for value in [
        "Fixture speakers",
        "Spotify",
        "Rec bus",
        "Inputs · 2",
        "Outputs · 2",
        "Audio · FL · port 101",
        "Audio · FR · port 202",
        "Rate 48000 Hz · quantum 256 · 3 nodes · 6 ports",
    ] {
        assert!(values.iter().any(|text| text == value), "missing {value}");
    }
    snapshot_if_requested(&window, "connected.png");
    let first = &view.nodes[&view.order[0]];
    let first_root = first.root.clone();
    let first_expander = first.ports.clone();
    first_expander.set_expanded(false);
    assert!(
        first_expander.grab_focus(),
        "port disclosure accepts keyboard focus"
    );
    settle();
    assert!(first_expander.has_focus());
    let mut changed = overview.clone();
    changed.rate = Some(96000);
    view.update(&changed);
    assert_eq!(view.nodes[&view.order[0]].root, first_root);
    assert!(!view.nodes[&view.order[0]].ports.is_expanded());
    assert!(first_expander.has_focus(), "rate-only update keeps focus");
    changed.nodes[0].label = "Renamed observed node".into();
    view.update(&changed);
    assert_eq!(view.nodes[&view.order[0]].root, first_root);
    assert!(
        !first_expander.is_expanded(),
        "node refresh keeps disclosure state"
    );
    assert!(first_expander.has_focus());
    changed.nodes.reverse();
    view.update(&changed);
    assert!(
        first_expander.has_focus(),
        "reordering existing strips restores focus"
    );

    // Check widget allocations and keyboard traversal at wide/narrow sizes.
    for (width, height) in [(1400, 900), (900, 600), (420, 500)] {
        window.set_default_size(width, height);
        settle();
        assert_eq!(
            window.width(),
            width,
            "test display must allow the requested width"
        );
        assert_eq!(view.scroll.hscrollbar_policy(), gtk::PolicyType::Never);
        assert!(view.status.grab_focus());
        assert!(view.widget().child_focus(gtk::DirectionType::TabForward));
        assert!(gtk::prelude::RootExt::focus(&window).is_some());
        for strip in view.nodes.values() {
            assert!(strip.root.width() > 0);
            assert!(strip.root.width() <= view.scroll.width());
        }
    }

    // A single high-port-count node is bounded by its own native scroller.
    let mut large = overview.clone();
    large.nodes.truncate(1);
    large.nodes[0].ports = (0..300)
        .map(|id| PortOverview {
            id: 10000 + id,
            label: format!("A very long observed port name {id} {}", "x".repeat(120)),
            input: id % 2 == 0,
            kind: match id % 3 {
                0 => Soort::Audio,
                1 => Soort::Midi,
                _ => Soort::Anders,
            },
            channel: Some(format!("AUX{id}")),
        })
        .collect();
    large.nodes[0].label = "Long node label ".repeat(30);
    large.nodes[0].name = "long.name".repeat(30);
    view.update(&large);
    view.nodes[&large.nodes[0].id].ports.set_expanded(true);
    settle();
    assert!(text_in(view.widget())
        .iter()
        .any(|text| text.contains("MIDI · unsupported")));
    let inner = view.nodes[&large.nodes[0].id]
        .ports
        .child()
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    assert!(inner.vadjustment().upper() > inner.vadjustment().page_size());
    assert!(inner.height() <= 200);
    assert!(
        window.width() <= 420,
        "long names must ellipsize inside narrow strips"
    );

    // Synthetic large graph: count population/scrollability, not throughput.
    let template = overview.nodes[0].clone();
    large.nodes = (0..120)
        .map(|id| {
            let mut node = template.clone();
            node.id = 20000 + id;
            node.label = format!("Synthetic node {id}");
            node
        })
        .collect();
    view.update(&large);
    settle();
    assert_eq!(view.nodes.len(), 120);
    assert!(view.scroll.vadjustment().upper() > view.scroll.vadjustment().page_size());
    view.scroll
        .vadjustment()
        .set_value(view.scroll.vadjustment().upper());
    assert!(view.scroll.vadjustment().value() > 0.0);
    let last = view.nodes[&large.nodes.last().unwrap().id].ports.clone();
    assert!(last.grab_focus());
    assert!(last.has_focus());

    snapshot_if_requested(&window, "large-narrow.png");
    view.show_notices(&[
        Notice::Message("Observer unavailable".into()),
        Notice::InboxOverflow { dropped: 2 },
    ]);
    assert!(view.notices.is_visible());
    assert!(view
        .notices
        .text()
        .contains("2 observer notices were dropped"));
    assert_eq!(
        view.notices
            .property::<gtk::AccessibleRole>("accessible-role"),
        gtk::AccessibleRole::Status
    );
    view.show_notices(&[Notice::Message("Observer recovered".into())]);
    assert_eq!(
        view.notices.text(),
        "Observer recovered",
        "notice text replaces, never appends"
    );

    // Drive reconnect/disconnect through the same bounded inbox + core reducer
    // as the application, including delayed replay from the first epoch.
    let (inbox, _) = LatestInbox::new(2);
    let mut sequence = EventSequencer::default();
    let old = sequence.graph(input);
    inbox.publish(old.clone());
    let mut state = OverviewState::default();
    assert!(state.apply(inbox.drain()));
    view.update(&state.overview());
    assert!(view.nodes[&view.order[0]].ports.grab_focus());
    inbox.publish(sequence.graph(serde_json::from_str(DISCONNECTED).unwrap()));
    assert!(state.apply(inbox.drain()));
    view.update(&state.overview());
    assert!(view.nodes.is_empty());
    assert!(view.empty.is_visible());
    assert!(
        view.status.has_focus(),
        "disconnect gives removed focus a status target"
    );
    inbox.publish(sequence.graph(serde_json::from_str(RECONNECTED).unwrap()));
    inbox.publish(old);
    assert!(state.apply(inbox.drain()));
    view.update(&state.overview());
    assert_eq!(view.nodes.len(), 3);
    assert!(view.nodes.contains_key(&1010));
    assert!(!view.nodes.contains_key(&10));

    let mut empty = overview.clone();
    empty.nodes.clear();
    view.update(&empty);
    assert_eq!(view.empty_title.text(), "No observed nodes");
    assert!(view.empty.is_visible());

    // No command controls exist anywhere in this presentation tree.
    fn no_mutation_controls(widget: &impl IsA<gtk::Widget>) {
        let widget = widget.as_ref();
        assert!(!widget.is::<gtk::Scale>());
        assert!(!widget.is::<gtk::ToggleButton>());
        assert!(!widget.is::<gtk::DropDown>());
        let mut next = widget.first_child();
        while let Some(child) = next {
            no_mutation_controls(&child);
            next = child.next_sibling();
        }
    }
    no_mutation_controls(view.widget());
    view.update(&overview);
    #[cfg(feature = "a11y-tests")]
    {
        // GTK's test backend checks published names, not AT-SPI/Orca output.
        assert!(gtk::test_accessible_has_property(
            &view.scroll,
            gtk::AccessibleProperty::Label
        ));
        for strip in view.nodes.values() {
            assert!(gtk::test_accessible_has_property(
                &strip.root,
                gtk::AccessibleProperty::Label
            ));
            assert!(gtk::test_accessible_has_property(
                &strip.ports,
                gtk::AccessibleProperty::Label
            ));
        }
    }
    window.close();
    settle();
}
