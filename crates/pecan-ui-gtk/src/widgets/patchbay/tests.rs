use super::PatchbayOverview;
use gtk::{glib, prelude::*};
use pecan_core::{
    graph::{Kabel, Momentopname},
    overview::project,
};
use std::time::{Duration, Instant};
fn settle() {
    let context = glib::MainContext::default();
    let until = Instant::now() + Duration::from_millis(120);
    while Instant::now() < until {
        while context.pending() && Instant::now() < until {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn fixture() -> Momentopname {
    serde_json::from_str(include_str!(
        "../../../../../fixtures/graph/audio-basic.json"
    ))
    .unwrap()
}
#[gtk::test]
fn patchbay_fixture_population_connections_focus_resize_and_large_graph() {
    let mut view = PatchbayOverview::new();
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(700)
        .child(view.widget())
        .build();
    window.present();
    settle();
    assert!(!view.paned.is_visible());
    assert!(view.empty.is_visible());
    let mut graph = fixture();
    let overview = project(&graph);
    assert!(view.update(&overview, &graph.kabels));
    settle();
    assert_eq!(view.rows.len(), 2);
    assert_eq!(view.geometry.borrow().modules.len(), 3);
    assert_eq!(view.geometry.borrow().cables.len(), 2);
    assert!(
        !view.update(&overview, &graph.kabels),
        "identical snapshot does not rebuild widgets"
    );
    let selected = view.rows[&500].clone();
    view.connections.select_row(Some(&selected));
    assert!(selected.grab_focus());
    view.connections
        .emit_move_cursor(gtk::MovementStep::DisplayLines, 1, false, false);
    assert!(
        view.rows[&501].has_focus(),
        "native row traversal moves to the next connection"
    );
    view.connections.select_row(Some(&selected));
    selected.grab_focus();
    let old_drawing = view.drawing.clone();
    graph.kabels.retain(|l| l.id != 501);
    assert!(view.update(&project(&graph), &graph.kabels));
    settle();
    assert_eq!(view.geometry.borrow().cables.len(), 1);
    assert_eq!(view.drawing, old_drawing);
    assert_eq!(
        view.connections.selected_row(),
        Some(view.rows[&500].clone())
    );
    assert!(view.rows[&500].has_focus());
    graph.kabels.push(Kabel {
        id: 900,
        uit_knoop: 9999,
        uit_poort: 9998,
        in_knoop: 10,
        in_poort: 101,
        client: None,
    });
    view.update(&project(&graph), &graph.kabels);
    settle();
    assert_eq!(view.rows.len(), 2);
    assert_eq!(view.geometry.borrow().undrawn, 1);
    let label = view.rows[&900]
        .child()
        .unwrap()
        .downcast::<gtk::Label>()
        .unwrap();
    assert!(label.text().contains("Unavailable node"));
    assert!(view
        .connection_title
        .text()
        .contains("1 not shown in diagram"));
    // Keep complete names available to assistive technology and tooltips while
    // truncating visual rows to fit native scrollers.
    graph.knopen[0].beschrijving = Some("Very long label ".repeat(80));
    graph.poorten[0].alias = Some("Very long port ".repeat(80));
    view.update(&project(&graph), &graph.kabels);
    for (width, height) in [(1400, 900), (900, 600), (420, 500)] {
        window.set_default_size(width, height);
        settle();
        assert_eq!(window.width(), width);
        assert!(view.status.grab_focus());
        assert!(view.widget().child_focus(gtk::DirectionType::TabForward));
        assert_eq!(
            view.connection_scroll.hscrollbar_policy(),
            gtk::PolicyType::Never
        );
        assert!(
            view.canvas_scroll.hadjustment().upper()
                >= view.canvas_scroll.hadjustment().page_size()
        );
    }
    assert!(
        view.canvas_scroll.hadjustment().upper() > view.canvas_scroll.hadjustment().page_size()
    );
    let template = fixture();
    graph = template.clone();
    graph.knopen.clear();
    graph.poorten.clear();
    graph.kabels.clear();
    for i in 0..80_u32 {
        for node in &template.knopen {
            let mut node = node.clone();
            node.id += i * 1000;
            node.naam = format!("{}.{}", node.naam, i);
            graph.knopen.push(node);
        }
        for port in &template.poorten {
            let mut port = port.clone();
            port.id += i * 1000;
            port.knoop += i * 1000;
            graph.poorten.push(port);
        }
        for link in &template.kabels {
            let mut link = link.clone();
            link.id += i * 1000;
            link.uit_knoop += i * 1000;
            link.in_knoop += i * 1000;
            link.uit_poort += i * 1000;
            link.in_poort += i * 1000;
            graph.kabels.push(link);
        }
    }
    view.update(&project(&graph), &graph.kabels);
    settle();
    assert_eq!(
        window.width(),
        420,
        "large connection counts must not widen a narrow window"
    );
    assert_eq!(view.rows.len(), 160);
    assert_eq!(view.geometry.borrow().cables.len(), 160);
    assert_eq!(view.geometry.borrow().modules.len(), 240);
    assert!(
        view.canvas_scroll.vadjustment().upper() > view.canvas_scroll.vadjustment().page_size()
    );
    assert!(
        view.connection_scroll.vadjustment().upper()
            > view.connection_scroll.vadjustment().page_size()
    );
    let last = view.rows.last_key_value().unwrap().1.clone();
    assert!(last.grab_focus());
    view.connections.select_row(Some(&last));
    #[cfg(feature = "a11y-tests")]
    {
        assert!(gtk::test_accessible_has_property(
            &last,
            gtk::AccessibleProperty::Label
        ));
        assert!(gtk::test_accessible_has_property(
            &view.connections,
            gtk::AccessibleProperty::Label
        ));
        assert!(gtk::test_accessible_has_property(
            &view.canvas_scroll,
            gtk::AccessibleProperty::Label
        ));
    }
    graph.verbonden = false;
    view.update(&project(&graph), &graph.kabels);
    settle();
    assert!(view.rows.is_empty());
    assert!(view.geometry.borrow().cables.is_empty());
    assert!(view.geometry.borrow().modules.is_empty());
    assert!(!view.paned.is_visible());
    assert!(view.status.has_focus());
    graph = fixture();
    graph.kabels.clear();
    view.update(&project(&graph), &graph.kabels);
    assert!(view.connection_title.text().contains("Connections · 0"));
    if let Some(directory) = std::env::var_os("PECAN_GTK_TEST_SNAPSHOTS") {
        graph = fixture();
        view.update(&project(&graph), &graph.kabels);
        window.set_default_size(900, 700);
        settle();
        let paintable = gtk::WidgetPaintable::new(Some(&window));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            &snapshot,
            f64::from(window.width()),
            f64::from(window.height()),
        );
        let node = snapshot.to_node().unwrap();
        window
            .renderer()
            .unwrap()
            .render_texture(&node, None)
            .save_to_png(std::path::PathBuf::from(directory).join("patchbay.png"))
            .unwrap();
    }
    window.close();
    settle();
}
