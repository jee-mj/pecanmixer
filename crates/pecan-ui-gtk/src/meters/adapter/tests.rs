//! Public synthetic DTOs/tokens only; no owner, server, capture or user desktop.
use super::*;
use gtk::{glib, prelude::*};
use pecan_core::{
    domain::{buses::BusConfig, levels::linear_to_meter},
    graph::Pieken,
};
use std::time::{Duration, Instant};

fn graph(epoch: u64, revision: u64) -> Graph {
    let ports: Vec<_> = (0..4).map(|i| serde_json::json!({
        "id":11+i,"knoop":10,"volgnummer":i,"naam":format!("synthetic.port.{i}"),
        "ingang":false,"soort":"audio","kanaal":format!("AUX{i}"),"monitor":false,"fysiek":false
    })).collect();
    Graph { stamp: Stamp { epoch, revision }, value: Arc::new(serde_json::from_value(serde_json::json!({
        "verbonden":true,"rate":48000,"quantum":128,"wireplumber":[],"kabels":[],
        "knopen":[{"id":10,"serial":10010,"naam":"synthetic.meter-source","klasse":"Audio/Source","volume":[1.0,1.0,1.0,1.0]}],
        "poorten":ports
    })).unwrap()) }
}
fn peaks(stamp: Stamp, values: &[f32]) -> Stamped<Arc<Pieken>> {
    Stamped {
        stamp,
        value: Arc::new(BTreeMap::from([(10, values.to_vec())])),
    }
}
fn apply(
    a: &mut TelemetryAdapter<u64>,
    g: &Graph,
    token: u64,
    values: Option<&[f32]>,
) -> TelemetryReport {
    let bindings = BTreeMap::from([(10, token)]);
    let buses = BusConfig::default().definitions().unwrap();
    a.apply_batch(
        &InboxBatch {
            graph: Some(g.clone()),
            peaks: values.map(|v| peaks(g.stamp, v)),
            ..Default::default()
        },
        Some(MeterSources {
            graph: g,
            node_bindings: &bindings,
            buses: &buses,
        }),
    )
}
fn peak(a: &mut TelemetryAdapter<u64>, stamp: Stamp, values: &[f32]) -> TelemetryReport {
    a.apply_batch(
        &InboxBatch {
            peaks: Some(peaks(stamp, values)),
            ..Default::default()
        },
        None,
    )
}
fn keys(a: &TelemetryAdapter<u64>, surface: MeterSurface) -> BTreeSet<MeterKey> {
    a.meters()
        .filter(|(s, _)| s.key.surface == surface)
        .map(|(s, _)| s.key.clone())
        .collect()
}
fn settle() {
    let context = glib::MainContext::default();
    let end = Instant::now() + Duration::from_millis(50);
    while Instant::now() < end {
        while context.pending() && Instant::now() < end {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn mapped(
    a: &mut TelemetryAdapter<u64>,
    surface: MeterSurface,
) -> (MeterKey, MeterWidget, gtk::Window) {
    let key = keys(a, surface).into_iter().next().unwrap();
    let meter = a.meter(&key).unwrap().clone();
    a.set_visible_keys(surface, &BTreeSet::from([key.clone()]));
    a.set_surface_active(surface, true);
    let window = gtk::Window::builder()
        .default_width(220)
        .default_height(300)
        .child(meter.widget())
        .build();
    window.present();
    settle();
    // Reset the tick origin without iterating GTK between this reset/manual ticks.
    a.set_surface_active(surface, false);
    a.set_surface_active(surface, true);
    (key, meter, window)
}
fn close(x: f64, y: f64) {
    assert!((x - y).abs() < 1e-6, "{x} != {y}");
}

#[gtk::test]
fn domain_indices_are_used_verbatim_for_retained_channel_and_module_meters() {
    let mut a = TelemetryAdapter::default();
    let g = graph(1, 1);
    assert!(apply(&mut a, &g, 1, None).projection_error.is_none());
    let buses = BusConfig::default().definitions().unwrap();
    let mixer = Mixer::build(&g.value, &buses);
    let layout = audio::layout(&g.value);
    let expected_channels: usize = mixer
        .apps
        .iter()
        .chain(&mixer.hardware)
        .chain(&mixer.destinations)
        .map(|s| s.channels.len())
        .sum();
    assert_eq!(keys(&a, MeterSurface::Mixer).len(), expected_channels);
    for strip in mixer
        .apps
        .iter()
        .chain(&mixer.hardware)
        .chain(&mixer.destinations)
    {
        for channel in &strip.channels {
            let key = MeterKey {
                surface: MeterSurface::Mixer,
                node: strip.node.id,
                item: strip.key.clone(),
                bar: *channel,
            };
            assert_eq!(a.entries[&key].spec.channels, vec![*channel]);
        }
    }
    let expected_modules: usize = layout
        .groups
        .iter()
        .flat_map(|g| &g.modules)
        .map(|m| m.meter_channels.len())
        .sum();
    assert_eq!(keys(&a, MeterSurface::Patchbay).len(), expected_modules);
    for module in layout.groups.iter().flat_map(|g| &g.modules) {
        for (bar, channels) in module.meter_channels.iter().enumerate() {
            let key = MeterKey {
                surface: MeterSurface::Patchbay,
                node: module.node,
                item: module.key.clone(),
                bar,
            };
            assert_eq!(&a.entries[&key].spec.channels, channels);
        }
    }
    assert!(a.entries.values().all(|e| !e.on_screen));
}

#[gtk::test]
fn same_batch_applies_graph_before_peaks_and_equal_stamp_order_is_latest_delivery() {
    let mut a = TelemetryAdapter::default();
    let g = graph(4, 9);
    assert!(!peak(&mut a, g.stamp, &[1.]).peaks_accepted);
    let r = apply(&mut a, &g, 1, Some(&[0.2, 0.5, 0.7, 1.]));
    assert!(r.graph_accepted && r.peaks_accepted && r.projection_error.is_none());
    let (_, m, w) = mapped(&mut a, MeterSurface::Mixer);
    m.0.advance_at(0.);
    close(m.reading().level, linear_to_meter(0.2));
    assert!(peak(&mut a, g.stamp, &[0.8, 0.5, 0.7, 1.]).peaks_accepted);
    assert!(peak(&mut a, g.stamp, &[0.3, 0.5, 0.7, 1.]).peaks_accepted);
    m.0.advance_at(1.);
    close(m.reading().level, linear_to_meter(0.3));
    for stamp in [
        Stamp {
            epoch: 3,
            revision: 999,
        },
        Stamp {
            epoch: 5,
            revision: 1,
        },
        Stamp {
            epoch: 4,
            revision: 8,
        },
        Stamp {
            epoch: 4,
            revision: 10,
        },
    ] {
        assert!(!peak(&mut a, stamp, &[1.]).peaks_accepted);
    }
    w.close();
}

#[gtk::test]
fn authoritative_generation_change_resets_byte_identical_replacement_without_rebuilding_widget() {
    let mut a = TelemetryAdapter::default();
    let first = graph(1, 1);
    apply(&mut a, &first, 11, Some(&[1.2, 0., 0., 0.]));
    let (key, m, w) = mapped(&mut a, MeterSurface::Mixer);
    m.0.advance_at(0.);
    assert!(m.reading().clipped);
    let ordinary = Graph {
        stamp: Stamp {
            revision: 2,
            ..first.stamp
        },
        value: Arc::new((*first.value).clone()),
    };
    apply(&mut a, &ordinary, 11, None);
    assert_eq!(m.widget(), a.meter(&key).unwrap().widget());
    assert!(
        m.reading().clipped,
        "unchanged registry authority retains hold/clip across ordinary revisions"
    );
    assert!(!m.reading().has_frame);
    // The owner changed an audio-port generation despite identical public DTOs.
    let replacement = Graph {
        stamp: Stamp {
            revision: 3,
            ..first.stamp
        },
        value: Arc::new((*first.value).clone()),
    };
    apply(&mut a, &replacement, 12, None);
    assert_eq!(m.widget(), a.meter(&key).unwrap().widget());
    assert!(m.reading().bound);
    assert!(!m.reading().clipped);
    assert_eq!(m.reading().held, 0.);
    assert_eq!(m.reading().level, 0.);
    assert!(!peak(&mut a, ordinary.stamp, &[1.]).peaks_accepted);
    peak(&mut a, replacement.stamp, &[0.4, 0., 0., 0.]);
    m.0.advance_at(0.);
    close(m.reading().level, linear_to_meter(0.4));
    w.close();
}

#[gtk::test]
fn missing_or_copied_registry_origin_fails_closed_and_explicit_rebind_clears_old_samples() {
    let mut a = TelemetryAdapter::default();
    let first = graph(1, 1);
    apply(&mut a, &first, 1, Some(&[1.2, 0., 0., 0.]));
    let (_, m, w) = mapped(&mut a, MeterSurface::Mixer);
    m.0.advance_at(0.);
    let next = graph(1, 2);
    let r = a.apply_batch(
        &InboxBatch {
            graph: Some(next.clone()),
            ..Default::default()
        },
        None,
    );
    assert_eq!(r.projection_error, Some(TelemetryError::MissingSources));
    assert!(!m.reading().bound && !m.reading().clipped);
    let copied = Graph {
        stamp: next.stamp,
        value: Arc::new((*next.value).clone()),
    };
    let bindings = BTreeMap::from([(10, 2)]);
    let buses = BusConfig::default().definitions().unwrap();
    assert_eq!(
        a.rebind_current(MeterSources {
            graph: &copied,
            node_bindings: &bindings,
            buses: &buses
        }),
        Some(TelemetryError::DifferentOrigin)
    );
    peak(&mut a, next.stamp, &[1.2, 0., 0., 0.]);
    assert_eq!(
        a.rebind_current(MeterSources {
            graph: &next,
            node_bindings: &bindings,
            buses: &buses
        }),
        None
    );
    m.0.advance_at(0.);
    assert!(!m.reading().has_frame && !m.reading().clipped);
    peak(&mut a, next.stamp, &[0.2, 0., 0., 0.]);
    m.0.advance_at(1.);
    close(m.reading().level, linear_to_meter(0.2));
    a.clear_samples();
    assert!(m.reading().bound && !m.reading().has_frame);
    assert_eq!(m.reading().level, 0.);
    a.reset_sources();
    assert!(!m.reading().bound);
    w.close();
}

#[gtk::test]
fn hidden_surface_and_offscreen_widgets_resume_latest_frame_without_ticks_or_catchup() {
    let mut a = TelemetryAdapter::default();
    let g = graph(1, 1);
    apply(&mut a, &g, 1, None);
    let (key, m, w) = mapped(&mut a, MeterSurface::Mixer);
    peak(&mut a, g.stamp, &[1.2, 0., 0., 0.]);
    m.0.advance_at(0.);
    a.set_surface_active(MeterSurface::Mixer, false);
    let advances = m.0.advances.get();
    for _ in 0..1000 {
        peak(&mut a, g.stamp, &[0.2, 0., 0., 0.]);
    }
    m.0.advance_at(100_000.);
    settle();
    assert_eq!(m.0.advances.get(), advances);
    assert!(!m.reading().clipped);
    a.set_surface_active(MeterSurface::Mixer, true);
    m.0.advance_at(200_000.);
    close(m.reading().level, linear_to_meter(0.2));
    a.set_visible_keys(MeterSurface::Mixer, &BTreeSet::new());
    let advances = m.0.advances.get();
    peak(&mut a, g.stamp, &[0.4, 0., 0., 0.]);
    m.0.advance_at(300_000.);
    assert_eq!(m.0.advances.get(), advances);
    a.set_visible_keys(MeterSurface::Mixer, &BTreeSet::from([key]));
    m.0.advance_at(400_000.);
    close(m.reading().level, linear_to_meter(0.4));
    w.close();
}

#[gtk::test]
fn disconnected_and_retired_widgets_cannot_follow_reconnect_or_adapter_drop() {
    let mut a = TelemetryAdapter::default();
    let g = graph(1, 1);
    apply(&mut a, &g, 1, Some(&[1.2, 0., 0., 0.]));
    let (key, old, w) = mapped(&mut a, MeterSurface::Mixer);
    old.0.advance_at(0.);
    assert!(old.reading().clipped);
    let offline = Graph {
        stamp: Stamp {
            epoch: 2,
            revision: 1,
        },
        value: Arc::default(),
    };
    a.apply_batch(
        &InboxBatch {
            graph: Some(offline),
            ..Default::default()
        },
        None,
    );
    assert!(a.entries.is_empty());
    assert!(!old.reading().bound);
    let next = graph(3, 1);
    apply(&mut a, &next, 3, Some(&[0.5, 0., 0., 0.]));
    let fresh = a.meter(&key).unwrap().clone();
    assert_ne!(old.widget(), fresh.widget());
    old.0.advance_at(100.);
    assert!(!old.reading().bound);
    drop(a);
    assert!(!fresh.reading().bound);
    assert!(!fresh.reading().has_frame);
    w.close();
}

#[gtk::test]
fn frame_storm_replaces_one_slot_without_projection_widget_or_render_churn() {
    let mut a = TelemetryAdapter::default();
    let g = graph(1, 1);
    apply(&mut a, &g, 1, None);
    let (_, m, w) = mapped(&mut a, MeterSurface::Patchbay);
    let renders = m.0.renders.get();
    let projection = a.projections;
    let count = a.entries.len();
    let mut first = None;
    let mut latest = None;
    for i in 0..10_000 {
        let p = peaks(
            g.stamp,
            &[if i == 9_999 { 0.7 } else { 0.2 }, 0.3, 0.1, 0.1],
        );
        if i == 0 {
            first = Some(Arc::downgrade(&p.value));
        }
        if i == 9_999 {
            latest = Some(p.value.clone());
        }
        a.apply_batch(
            &InboxBatch {
                peaks: Some(p),
                ..Default::default()
            },
            None,
        );
    }
    assert!(first.unwrap().upgrade().is_none());
    assert!(Arc::ptr_eq(
        &a.feed.0.frames.borrow().peaks.as_ref().unwrap().value,
        latest.as_ref().unwrap()
    ));
    assert_eq!(m.0.renders.get(), renders);
    assert_eq!(a.projections, projection);
    assert_eq!(a.entries.len(), count);
    m.0.advance_at(0.);
    close(m.reading().level, linear_to_meter(0.7));
    w.close();
}

#[gtk::test]
fn projection_budget_is_atomic_and_missing_node_authority_or_serial_never_binds() {
    let g = graph(1, 1);
    let mut small = TelemetryAdapter::new(1);
    assert_eq!(
        apply(&mut small, &g, 1, None).projection_error,
        Some(TelemetryError::TooManyMeters { limit: 1 })
    );
    assert!(small.entries.is_empty());
    let mut a = TelemetryAdapter::<u64>::default();
    let buses = BusConfig::default().definitions().unwrap();
    let bindings = BTreeMap::new();
    a.apply_batch(
        &InboxBatch {
            graph: Some(g.clone()),
            ..Default::default()
        },
        Some(MeterSources {
            graph: &g,
            node_bindings: &bindings,
            buses: &buses,
        }),
    );
    assert!(a.meters().all(|(_, w)| !w.reading().bound));
    let mut missing = (*g.value).clone();
    missing.knopen[0].serial = None;
    let missing = Graph {
        stamp: Stamp {
            revision: 2,
            ..g.stamp
        },
        value: Arc::new(missing),
    };
    apply(&mut a, &missing, 2, None);
    assert!(a.meters().all(|(_, w)| !w.reading().bound));
}

#[gtk::test]
fn large_projection_failure_retires_old_sources_and_smaller_graph_recovers_within_budget() {
    let mut a = TelemetryAdapter::new(16);
    let first = graph(1, 1);
    apply(&mut a, &first, 1, Some(&[1.2, 0., 0., 0.]));
    let (_, old, window) = mapped(&mut a, MeterSurface::Mixer);
    old.0.advance_at(0.);
    let mut large = (*first.value).clone();
    large.knopen.clear();
    large.poorten.clear();
    let mut tokens = BTreeMap::new();
    for index in 0..100 {
        let id = 1000 + index * 10;
        let mut n = first.value.knopen[0].clone();
        n.id = id;
        n.serial = Some(u64::from(id));
        n.naam = format!("synthetic.large.{index}");
        large.knopen.push(n);
        for (channel, original) in first.value.poorten.iter().enumerate() {
            let mut p = original.clone();
            p.id = id + 1 + channel as u32;
            p.knoop = id;
            large.poorten.push(p);
        }
        tokens.insert(id, u64::from(id));
    }
    let large = Graph {
        stamp: Stamp {
            revision: 2,
            ..first.stamp
        },
        value: Arc::new(large),
    };
    let buses = BusConfig::default().definitions().unwrap();
    let result = a.apply_batch(
        &InboxBatch {
            graph: Some(large.clone()),
            ..Default::default()
        },
        Some(MeterSources {
            graph: &large,
            node_bindings: &tokens,
            buses: &buses,
        }),
    );
    assert_eq!(
        result.projection_error,
        Some(TelemetryError::TooManyMeters { limit: 16 })
    );
    assert!(a.entries.len() <= 16);
    assert!(a.meters().all(|(_, w)| !w.reading().bound));
    assert!(!old.reading().clipped);
    let next = graph(1, 3);
    assert!(apply(&mut a, &next, 2, None).projection_error.is_none());
    assert!(a.entries.len() <= 16);
    assert!(a.meters().all(|(_, w)| w.reading().bound));
    window.close();
}

#[gtk::test]
fn stale_graph_cannot_replace_authority_and_missing_channels_do_not_invent_capture_status() {
    let mut a = TelemetryAdapter::default();
    let current = graph(5, 10);
    apply(&mut a, &current, 100, Some(&[0.5, 0.5, 0.5, 0.5]));
    let (_, m, window) = mapped(&mut a, MeterSurface::Mixer);
    let stale = graph(4, 999);
    assert!(!apply(&mut a, &stale, 999, None).graph_accepted);
    assert_eq!(a.stamp(), Some(current.stamp));
    assert!(a.entries.values().all(|e| e.identity == Some(100)));
    // Presence of a node frame is not proof that every requested channel exists.
    peak(&mut a, current.stamp, &[]);
    m.0.advance_at(0.);
    assert!(m.reading().bound && m.reading().has_frame);
    close(m.reading().level, 0.);
    a.apply_batch(
        &InboxBatch {
            peaks: Some(Stamped {
                stamp: current.stamp,
                value: Arc::default(),
            }),
            ..Default::default()
        },
        None,
    );
    m.0.advance_at(10.);
    assert!(m.reading().bound && !m.reading().has_frame);
    assert!(
        a.error.is_none(),
        "the DTO supplies no capture-status reason or freshness timestamp"
    );
    window.close();
}
