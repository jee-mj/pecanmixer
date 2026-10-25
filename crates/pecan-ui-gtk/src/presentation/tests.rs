use super::GraphPresentation;
use gtk::prelude::*;
use pecan_core::{
    backend::{BackendEvent, EventSequencer},
    graph::Momentopname,
    inbox::{InboxBatch, LatestInbox},
};
fn fixture() -> Momentopname {
    serde_json::from_str(include_str!("../../../../fixtures/graph/audio-basic.json")).unwrap()
}
fn contains_text(widget: &impl IsA<gtk::Widget>, needle: &str) -> bool {
    let widget = widget.as_ref();
    if widget
        .downcast_ref::<gtk::Label>()
        .is_some_and(|label| label.text().contains(needle))
    {
        return true;
    }
    let mut next = widget.first_child();
    while let Some(child) = next {
        if contains_text(&child, needle) {
            return true;
        }
        next = child.next_sibling();
    }
    false
}
#[gtk::test]
fn accepted_link_only_frames_reach_both_views_while_stale_and_meter_frames_do_not() {
    let mut view = GraphPresentation::new();
    let (inbox, wake) = LatestInbox::new(2);
    let mut sequence = EventSequencer::default();
    let graph = fixture();
    let first = sequence.graph(graph.clone());
    inbox.publish(first.clone());
    assert!(view.apply(inbox.drain()).is_some());
    assert!(contains_text(view.patchbay.widget(), "Link 500"));
    // Overview equality ignores links; a newer accepted graph still must pass.
    let mut no_links = graph.clone();
    no_links.kabels.clear();
    inbox.publish(sequence.graph(no_links));
    assert!(view.apply(inbox.drain()).is_some());
    assert!(!contains_text(view.patchbay.widget(), "Link 500"));
    assert!(contains_text(
        view.patchbay.widget(),
        "No observed connections"
    ));
    assert!(view
        .apply(InboxBatch {
            graph: match first.clone() {
                BackendEvent::Graph(g) => Some(g),
                _ => None,
            },
            ..Default::default()
        })
        .is_none());
    inbox.publish(sequence.peaks(Default::default()));
    assert!(view.apply(inbox.drain()).is_none());
    wake.try_recv().unwrap();
    for i in 0..10000 {
        let mut next = graph.clone();
        if i % 2 == 0 {
            next.kabels.clear();
        }
        inbox.publish(sequence.graph(next));
    }
    assert_eq!(wake.len(), 1);
    assert!(view.apply(inbox.drain()).is_some());
    inbox.publish(sequence.graph(Momentopname::default()));
    assert!(view.apply(inbox.drain()).is_some());
    inbox.publish(first);
    assert!(view.apply(inbox.drain()).is_none());
    view.show_patchbay();
    assert_eq!(view.pages.visible_child_name().as_deref(), Some("patchbay"));
    assert!(view.widget().first_child().is_some());
}
