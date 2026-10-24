//! Synthetic registry and sample publications; no audio worker or real capture.
use super::*;
use crate::application::{ApplicationOwner, FixtureDriver};
use crate::meters::{MeterKey, MeterSurface, MeterWidget};
use pecan_core::{graph::Momentopname, inbox::InboxBatch};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn graph() -> Momentopname {
    serde_json::from_value(serde_json::json!({
        "verbonden":true,"wireplumber":[],
        "knopen":[
            {"id":10,"serial":1010,"naam":"synthetic.source","app":"Synthetic signal","klasse":"Stream/Output/Audio","volume":[1.0,0.5],"mute":false},
            {"id":20,"serial":1020,"naam":"synthetic.destination","klasse":"Audio/Sink","volume":[1.0,1.0],"mute":false}
        ],
        "poorten":[
            {"id":11,"knoop":10,"naam":"synthetic.out.left","soort":"audio","monitor":false,"fysiek":false,"ingang":false,"volgnummer":0,"kanaal":"FL"},
            {"id":12,"knoop":10,"naam":"synthetic.out.right","soort":"audio","monitor":false,"fysiek":false,"ingang":false,"volgnummer":1,"kanaal":"FR"},
            {"id":21,"knoop":20,"naam":"synthetic.in.left","soort":"audio","monitor":false,"fysiek":false,"ingang":true,"volgnummer":0,"kanaal":"FL"},
            {"id":22,"knoop":20,"naam":"synthetic.in.right","soort":"audio","monitor":false,"fysiek":false,"ingang":true,"volgnummer":1,"kanaal":"FR"}
        ],"kabels":[],"standaard_sink":"synthetic.destination"
    })).unwrap()
}
fn generations() -> BTreeMap<u32, u64> {
    [(10, 1), (11, 2), (12, 3), (20, 4), (21, 5), (22, 6)].into()
}
struct Window(gtk::Window);
impl Drop for Window {
    fn drop(&mut self) {
        self.0.destroy();
    }
}
fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        while gtk::glib::MainContext::default().pending() {
            gtk::glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn setup() -> (Rc<NativeWorkspace>, FixtureDriver, Window) {
    let owner = ApplicationOwner::fixture();
    let session = owner.activate();
    let driver = owner.fixture_driver().unwrap();
    driver.publish(graph(), generations());
    let view = NativeWorkspace::new(session);
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(1100)
        .default_height(800)
        .build();
    window.present();
    view.poll();
    pump(50);
    view.poll();
    (view, driver, Window(window))
}
fn meter(
    view: &NativeWorkspace,
    surface: MeterSurface,
    node: u32,
    bar: usize,
) -> (MeterKey, MeterWidget) {
    view.telemetry
        .borrow()
        .adapter
        .meters()
        .find(|(spec, _)| {
            spec.key.surface == surface && spec.key.node == node && spec.key.bar == bar
        })
        .map(|(s, w)| (s.key.clone(), w.clone()))
        .unwrap()
}
#[gtk::test]
fn workspace_single_poll_feeds_exact_graph_then_peaks_and_mounts_retained_widgets() {
    let (view, d, window) = setup();
    let (key, m) = meter(&view, MeterSurface::Mixer, 10, 0);
    let original = m.widget().clone();
    assert!(m.widget().is_mapped());
    assert!(m.widget().is_ancestor(view.mixer.widget()));
    assert_eq!(
        view.telemetry.borrow().adapter.stamp(),
        view.session.graph().map(|g| g.stamp)
    );
    d.publish_peaks([(10, vec![1.1, 0.25]), (20, vec![0.6, 0.4])].into());
    view.poll();
    pump(60);
    assert!(view.telemetry.borrow().report.peaks_accepted);
    assert!(m.reading().bound && m.reading().has_frame && m.reading().clipped);
    let count = view.telemetry.borrow().adapter.meters().count();
    for _ in 0..10000 {
        d.publish_peaks([(10, vec![0.75, 0.25])].into());
    }
    view.poll();
    pump(50);
    assert_eq!(
        view.telemetry
            .borrow()
            .adapter
            .meter(&key)
            .unwrap()
            .widget(),
        &original
    );
    assert_eq!(view.telemetry.borrow().adapter.meters().count(), count);
    view.show_patchbay();
    pump(50);
    view.poll();
    pump(50);
    let (_, mini) = meter(&view, MeterSurface::Patchbay, 10, 0);
    assert!(mini.widget().is_mapped());
    assert!(mini.widget().is_ancestor(view.patchbay.borrow().widget()));
    assert!(!m.widget().is_mapped());
    window.0.set_visible(false);
    view.poll();
    pump(30);
    assert!(!mini.widget().is_mapped());
    window.0.present();
    pump(50);
    view.poll();
    pump(50);
    assert!(mini.widget().is_mapped());
    assert!(d.take_requests().is_empty());
}
#[gtk::test]
fn full_registry_port_binding_resets_identical_dto_replacement_and_missing_generation() {
    let (view, d, _window) = setup();
    let (_, m) = meter(&view, MeterSurface::Mixer, 10, 0);
    let (_, other) = meter(&view, MeterSurface::Mixer, 20, 0);
    d.publish_peaks([(10, vec![1.1, 0.9]), (20, vec![1.1, 0.8])].into());
    view.poll();
    pump(60);
    assert!(m.reading().clipped && other.reading().clipped);
    let mut gens = generations();
    gens.insert(11, 100);
    d.publish(graph(), gens.clone());
    view.poll();
    assert!(!m.reading().clipped && m.reading().held == 0.);
    assert!(other.reading().clipped);
    assert_eq!(
        meter(&view, MeterSurface::Mixer, 10, 0).1.widget(),
        m.widget()
    );
    gens.remove(&12);
    d.publish(graph(), gens);
    view.poll();
    assert!(!m.reading().bound);
    assert!(other.reading().bound);
    d.publish(graph(), generations());
    view.poll();
    assert!(m.reading().bound && !m.reading().has_frame);
    let mut no_serial = graph();
    no_serial.knopen[0].serial = None;
    d.publish(no_serial, generations());
    view.poll();
    assert!(!m.reading().bound);
}
#[gtk::test]
fn unavailable_witness_and_disconnect_retire_meters_without_pruning_full_graph() {
    let (view, d, _window) = setup();
    let (_, old) = meter(&view, MeterSurface::Mixer, 10, 0);
    let latest = d.publish(graph(), generations());
    d.forget_origins();
    view.poll();
    assert!(!old.reading().bound);
    assert_eq!(view.session.graph().unwrap().stamp, latest.stamp);
    assert!(view.telemetry.borrow().report.projection_error.is_some());
    d.publish(graph(), generations());
    view.poll();
    assert!(old.reading().bound);
    d.publish(Momentopname::default(), BTreeMap::new());
    view.poll();
    assert!(!old.reading().bound);
    assert_eq!(view.telemetry.borrow().adapter.meters().count(), 0);
    assert!(!view.session.enabled());
}
#[gtk::test]
fn copied_graph_authority_is_rejected_and_explicit_mapping_changes_reset() {
    let (view, d, _window) = setup();
    let (_, m) = meter(&view, MeterSurface::Mixer, 10, 0);
    d.publish_peaks([(10, vec![1.1, 0.8])].into());
    view.poll();
    pump(50);
    let mut batch = InboxBatch::default();
    let mut copy = view.session.graph().unwrap();
    copy.stamp.revision += 1;
    copy.value = std::sync::Arc::new((*copy.value).clone());
    batch.graph = Some(copy);
    view.telemetry.borrow_mut().apply(&view.session, &batch);
    assert!(!m.reading().bound);
    assert!(view.telemetry.borrow().report.projection_error.is_some());
    // A fresh accepted publication restores the exact owner-witness pair.
    let mut changed = graph();
    changed.poorten[0].volgnummer = Some(1);
    changed.poorten[1].volgnummer = Some(0);
    d.publish(changed.clone(), generations());
    d.publish(changed, generations());
    view.poll();
    // Channel-order changes can change the pure strip key. The retained old
    // wrapper stays retired; the current projected wrapper has no old clipping.
    let (_, current) = meter(&view, MeterSurface::Mixer, 10, 0);
    assert!(current.reading().bound && !current.reading().clipped);
}
#[gtk::test]
fn actual_viewport_clipping_excludes_mapped_offscreen_meters_until_scrolled() {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_height_request(600);
    let meter = gtk::Box::new(gtk::Orientation::Vertical, 0);
    meter.set_size_request(100, 60);
    content.append(&spacer);
    content.append(&meter);
    let scroll = gtk::ScrolledWindow::builder().child(&content).build();
    let window = Window(
        gtk::Window::builder()
            .child(&scroll)
            .default_width(300)
            .default_height(200)
            .build(),
    );
    window.0.present();
    pump(50);
    assert!(meter.is_mapped());
    assert!(!crate::application::telemetry::on_screen(&meter));
    scroll.vadjustment().set_value(600.);
    pump(50);
    assert!(crate::application::telemetry::on_screen(&meter));
    window.0.set_visible(false);
    pump(20);
    assert!(!crate::application::telemetry::on_screen(&meter));
}
