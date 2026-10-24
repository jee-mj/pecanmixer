use super::*;
use gtk::glib;
use pecan_core::{
    backend::{BackendEvent, Stamp, Stamped},
    domain::levels::linear_to_meter,
    graph::{Momentopname, Pieken},
    inbox::LatestInbox,
};
use std::{
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
fn graph(epoch: u64, revision: u64) -> Stamped<Arc<Momentopname>> {
    let ports:Vec<_>=(0..4).map(|i|serde_json::json!({"id":11+i,"knoop":10,"volgnummer":i,"naam":format!("synthetic.port.{i}"),"ingang":false,"soort":"audio","kanaal":format!("AUX{i}"),"monitor":false,"fysiek":false})).collect();
    let value=serde_json::from_value(serde_json::json!({"verbonden":true,"rate":48000,"quantum":128,"wireplumber":[],"kabels":[],"knopen":[{"id":10,"serial":10010,"naam":"synthetic.meter-source","klasse":"Audio/Source","volume":[1.0,1.0,1.0,1.0]}],"poorten":ports})).unwrap();
    Stamped {
        stamp: Stamp { epoch, revision },
        value: Arc::new(value),
    }
}
fn peaks(stamp: Stamp, values: &[f32]) -> Stamped<Arc<Pieken>> {
    Stamped {
        stamp,
        value: Arc::new(BTreeMap::from([(10, values.to_vec())])),
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
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
fn mapped(mode: MeterMode, channels: &[usize]) -> (MeterFeed, MeterWidget, gtk::Window, Stamp) {
    let feed = MeterFeed::default();
    let graph = graph(1, 1);
    let stamp = graph.stamp;
    feed.observe_graph(graph);
    let widget = MeterWidget::new(&feed, "Synthetic channel", mode);
    assert!(widget.bind(10, channels));
    let window = gtk::Window::builder()
        .default_width(180)
        .default_height(260)
        .child(widget.widget())
        .build();
    window.present();
    settle();
    widget.0.stop_ticks();
    widget.0.reset();
    assert!(widget.widget().is_mapped());
    (feed, widget, window, stamp)
}
#[test]
fn frame_slot_replaces_equal_stamps_and_rejects_wrong_epoch_old_and_future_revisions() {
    let feed = MeterFeed::default();
    let initial = graph(7, 10);
    let stamp = initial.stamp;
    assert!(
        !feed.publish_peaks(peaks(stamp, &[1.0])),
        "peaks need an accepted graph first"
    );
    assert!(feed.observe_graph(initial.clone()));
    assert!(!feed.observe_graph(initial));
    for invalid in [
        Stamp {
            epoch: 6,
            revision: 999,
        },
        Stamp {
            epoch: 8,
            revision: 1,
        },
        Stamp {
            epoch: 7,
            revision: 9,
        },
        Stamp {
            epoch: 7,
            revision: 11,
        },
    ] {
        assert!(!feed.publish_peaks(peaks(invalid, &[1.0])));
    }
    let newest = peaks(stamp, &[0.25]);
    assert!(feed.publish_peaks(peaks(stamp, &[1.0])));
    assert!(feed.publish_peaks(newest.clone()));
    assert!(Arc::ptr_eq(
        &feed.0.frames.borrow().peaks.as_ref().unwrap().value,
        &newest.value
    ));
    assert!(feed.observe_graph(graph(7, 11)));
    assert!(feed.0.frames.borrow().peaks.is_none());
}
#[gtk::test]
fn visible_ticks_delegate_decay_hold_clip_and_elapsed_cap_to_core() {
    let (feed, widget, window, stamp) = mapped(MeterMode::Channel, &[0]);
    feed.publish_peaks(peaks(stamp, &[1.0]));
    widget.0.advance_at(0.0);
    close(widget.reading().level, 1.0);
    close(widget.reading().held, 1.0);
    assert!(widget.reading().clipped);
    feed.publish_peaks(peaks(stamp, &[0.0]));
    widget.0.advance_at(100.0);
    close(widget.reading().level, 0.96);
    widget.0.advance_at(1000.0);
    close(widget.reading().level, 0.92);
    widget.0.advance_at(1500.0);
    close(widget.reading().held, 1.0);
    widget.0.advance_at(1501.0);
    close(widget.reading().held, 0.0);
    assert!(widget.reading().clipped);
    widget.0.advance_at(2000.0);
    assert!(!widget.reading().clipped);
    assert!(!widget.0.clip.is_visible());
    let before = widget.reading();
    widget.0.advance_at(f64::NAN);
    widget.0.advance_at(1.0);
    assert_eq!(widget.reading(), before);
    window.close();
    settle();
}
#[gtk::test]
fn channel_selection_mini_maxima_missing_values_and_nonfinite_samples_are_safe() {
    let (feed, channel, window, stamp) = mapped(MeterMode::Channel, &[1]);
    let mini = MeterWidget::new(&feed, "Synthetic mini group", MeterMode::Mini);
    mini.bind(10, &[0, 2]);
    let both = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    window.set_child(gtk::Widget::NONE);
    both.append(channel.widget());
    both.append(mini.widget());
    window.set_child(Some(&both));
    settle();
    for widget in [&channel, &mini] {
        widget.0.stop_ticks();
        widget.0.reset();
    }
    feed.publish_peaks(peaks(stamp, &[0.1, 0.25, 0.5]));
    channel.0.advance_at(0.0);
    mini.0.advance_at(0.0);
    close(channel.reading().level, linear_to_meter(0.25));
    close(mini.reading().level, linear_to_meter(0.5));
    assert!(
        !mini.0.title.is_visible() && !mini.0.readout.is_visible() && !mini.0.clip.is_visible()
    );
    channel.bind(10, &[9]);
    channel.0.stop_ticks();
    channel.0.advance_at(1.0);
    close(channel.reading().level, 0.0);
    assert!(channel.reading().has_frame);
    channel.bind(10, &[0, 1, 2]);
    channel.0.stop_ticks();
    feed.publish_peaks(peaks(stamp, &[f32::NAN, f32::INFINITY, -1.0]));
    channel.0.advance_at(2.0);
    close(channel.reading().level, 0.0);
    assert!(!channel.reading().clipped);
    feed.publish_peaks(Stamped {
        stamp,
        value: Arc::new(BTreeMap::from([(999, vec![1.0])])),
    });
    channel.0.advance_at(3.0);
    assert!(!channel.reading().has_frame);
    assert_eq!(channel.0.readout.text(), "No current frame");
    assert!(!channel.bind(999, &[0]));
    assert!(
        channel.0.tick.borrow().is_none(),
        "invalid rebind unregisters the old tick"
    );
    assert!(!channel.bind(10, &[]));
    assert!(!channel.reading().bound);
    #[cfg(feature = "a11y-tests")]
    assert!(gtk::test_accessible_has_property(
        &mini.0.bar,
        gtk::AccessibleProperty::Label
    ));
    window.close();
    settle();
}
#[gtk::test]
fn source_replacement_channel_layout_epoch_and_disconnect_reset_without_rebuilding() {
    for change in 0..7 {
        let (feed, widget, window, stamp) = mapped(MeterMode::Channel, &[0]);
        let bar = widget.0.bar.clone();
        let root = widget.widget().clone();
        feed.publish_peaks(peaks(stamp, &[1.0]));
        widget.0.advance_at(0.0);
        let mut next = graph(1, 2);
        match change {
            0 => Arc::make_mut(&mut next.value).knopen[0].serial = Some(20010),
            1 => {
                Arc::make_mut(&mut next.value).knopen.clear();
                Arc::make_mut(&mut next.value).poorten.clear();
            }
            2 => Arc::make_mut(&mut next.value).poorten[0].id = 99,
            3 => next.stamp.epoch = 2,
            4 => Arc::make_mut(&mut next.value).verbonden = false,
            5 => Arc::make_mut(&mut next.value).knopen[0].naam = "synthetic.replacement".into(),
            _ => Arc::make_mut(&mut next.value).knopen[0].klasse = Some("Audio/Sink".into()),
        }
        feed.observe_graph(next.clone());
        assert!(!widget.reading().bound);
        close(widget.reading().level, 0.0);
        assert!(!widget.reading().clipped);
        assert!(widget.0.tick.borrow().is_none());
        assert_eq!(widget.0.bar, bar);
        assert_eq!(widget.widget(), &root);
        assert!(!feed.publish_peaks(peaks(stamp, &[1.0])));
        let mut returned = graph(next.stamp.epoch, 3);
        if change == 0 {
            Arc::make_mut(&mut returned.value).knopen[0].serial = Some(20010);
        }
        feed.observe_graph(returned.clone());
        feed.publish_peaks(peaks(returned.stamp, &[0.1]));
        widget.0.advance_at(100.0);
        assert!(
            !widget.reading().bound,
            "a replaced source must not silently reattach"
        );
        assert!(widget.bind(10, &[0]));
        widget.0.stop_ticks();
        widget.0.advance_at(200.0);
        close(widget.reading().level, linear_to_meter(0.1));
        assert!(!widget.reading().clipped);
        window.close();
        settle();
    }
}
#[gtk::test]
fn unrelated_graph_updates_clear_samples_but_preserve_known_source_hold_state() {
    let (feed, widget, window, stamp) = mapped(MeterMode::Channel, &[0]);
    feed.publish_peaks(peaks(stamp, &[1.0]));
    widget.0.advance_at(0.0);
    let mut next = graph(1, 2);
    Arc::make_mut(&mut next.value).quantum = Some(256);
    feed.observe_graph(next.clone());
    assert!(widget.reading().bound);
    assert!(!widget.reading().has_frame);
    close(widget.reading().held, 1.0);
    assert!(widget.bind(10, &[0]));
    assert!(!feed.publish_peaks(peaks(stamp, &[1.0])));
    feed.publish_peaks(peaks(next.stamp, &[0.0]));
    widget.0.advance_at(100.0);
    close(widget.reading().level, 0.96);
    close(widget.reading().held, 1.0);
    let mut unknown = graph(1, 3);
    Arc::make_mut(&mut unknown.value).knopen[0].serial = None;
    feed.observe_graph(unknown);
    assert!(!widget.bind(10, &[0]));
    assert!(!widget.reading().bound);
    window.close();
    settle();
}
#[gtk::test]
fn hidden_and_inactive_widgets_stop_ticks_reset_and_resume_from_only_the_latest_frame() {
    let (feed, widget, window, stamp) = mapped(MeterMode::Channel, &[0]);
    feed.publish_peaks(peaks(stamp, &[1.0]));
    widget.0.advance_at(0.0);
    widget.set_active(false);
    assert!(widget.0.tick.borrow().is_none());
    close(widget.reading().level, 0.0);
    let advances = widget.0.advances.get();
    for i in 0..100 {
        feed.publish_peaks(peaks(stamp, &[i as f32 / 1000.0]));
        widget.0.advance_at(i as f64 + 100.0);
    }
    assert_eq!(widget.0.advances.get(), advances);
    widget.set_active(true);
    assert!(widget.0.tick.borrow().is_some());
    widget.0.stop_ticks();
    widget.0.advance_at(10000.0);
    close(widget.reading().level, linear_to_meter(0.099));
    assert!(!widget.reading().clipped);
    window.set_visible(false);
    settle();
    assert!(!widget.widget().is_mapped());
    assert!(widget.0.tick.borrow().is_none());
    close(widget.reading().level, 0.0);
    let advances = widget.0.advances.get();
    feed.publish_peaks(peaks(stamp, &[0.25]));
    widget.0.advance_at(20000.0);
    assert_eq!(widget.0.advances.get(), advances);
    window.present();
    settle();
    assert!(widget.0.tick.borrow().is_some());
    close(widget.reading().level, linear_to_meter(0.25));
    assert!(!widget.reading().clipped);
    let weak = Rc::downgrade(&widget.0);
    let retained_root = widget.widget().clone();
    window.set_child(gtk::Widget::NONE);
    drop(widget);
    assert!(
        weak.upgrade().is_none(),
        "GTK callbacks/feed registry cannot retain the view"
    );
    drop(retained_root);
    window.close();
    settle();
}
#[gtk::test]
fn coalesced_frame_storms_never_rebuild_widgets_or_render_until_a_visible_tick() {
    let (feed, widget, window, stamp) = mapped(MeterMode::Channel, &[0]);
    let bar = widget.0.bar.clone();
    let root = widget.widget().clone();
    let renders = widget.0.renders.get();
    let (inbox, wake) = LatestInbox::new(4);
    inbox.publish(BackendEvent::Graph(graph(1, 1)));
    for i in 0..10000 {
        inbox.publish(BackendEvent::Peaks(peaks(stamp, &[i as f32 / 20000.0])));
    }
    assert_eq!(wake.len(), 1);
    feed.apply(&inbox.drain());
    assert_eq!(widget.0.renders.get(), renders);
    close(
        f64::from(feed.0.frames.borrow().peaks.as_ref().unwrap().value[&10][0]),
        f64::from(9999.0_f32 / 20000.0),
    );
    widget.0.advance_at(0.0);
    close(
        widget.reading().level,
        linear_to_meter(9999.0_f32 / 20000.0),
    );
    assert_eq!(widget.0.renders.get(), renders + 1);
    assert_eq!(widget.0.bar, bar);
    assert_eq!(widget.widget(), &root);
    feed.publish_peaks(peaks(stamp, &[0.0]));
    let before = widget.0.renders.get();
    for time in 1..=20 {
        widget.0.advance_at(time as f64);
    }
    assert!(
        widget.0.renders.get() > before,
        "small steps must accumulate relative to the last rendered value"
    );
    close(widget.0.bar.value(), widget.0.rendered.get().level);
    feed.clear_peaks();
    assert!(feed.0.frames.borrow().peaks.is_none());
    assert!(widget.reading().bound && !widget.reading().has_frame);
    close(widget.reading().level, 0.0);
    assert!(!widget.reading().clipped);
    window.close();
    settle();
}

#[gtk::test]
fn normal_and_mini_fills_grow_from_the_bottom_of_the_allocated_trough() {
    for mode in [MeterMode::Channel, MeterMode::Mini] {
        let (feed, widget, window, stamp) = mapped(mode, &[0]);
        feed.publish_peaks(peaks(stamp, &[0.031622775]));
        widget.0.advance_at(0.0);
        settle();
        close(widget.reading().level, 0.5);
        let trough = widget.0.bar.first_child().expect("GTK LevelBar trough");
        let mut child = trough.first_child();
        let mut filled = None;
        while let Some(block) = child {
            child = block.next_sibling();
            if block.has_css_class("filled") {
                filled = Some(block);
                break;
            }
        }
        let filled = filled.expect("GTK continuous filled block");
        let bounds = filled.compute_bounds(&trough).unwrap();
        let height = trough.height() as f32;
        assert!(height > 0.0);
        assert!(
            bounds.y() > height * 0.2,
            "half-scale fill must leave empty space above it: {bounds:?}"
        );
        assert!(
            bounds.y() + bounds.height() >= height - 4.0,
            "fill must reach the bottom: {bounds:?}, trough={height}"
        );
        window.close();
        settle();
    }
}
