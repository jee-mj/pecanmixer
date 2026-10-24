use super::*;
use crate::routing::tests::*;
use pecan_core::native_command::{Counts, NativeFailure};

pub(super) fn view() -> (RoutingControls<Rc<()>>, Stamped<Arc<Momentopname>>, Owner) {
    let g = graph();
    let bus = owner(&g);
    let v = RoutingControls::new();
    v.observe(capture(&g, &bus)).unwrap();
    (v, g, bus)
}
pub(super) fn button(v: &RoutingControls<Rc<()>>, key: &Key) -> gtk::Button {
    v.0.buttons
        .borrow()
        .iter()
        .find(|(k, _)| k == key)
        .unwrap()
        .1
        .clone()
}
fn pump() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}
fn settle(ms: u64) {
    let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        pump();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[gtk::test]
fn both_surfaces_use_one_bus_model_and_selection_does_not_release_configuration_pending() {
    let g = graph();
    let bus = Rc::new(buses::BusControls::<Rc<()>, u64, u64>::new(
        buses::Budget::default(),
    ));
    let mut s = snapshot(&g);
    s.capabilities.idle_configuration = buses::Support::Available;
    s.capabilities.routed_layout = buses::Support::Available;
    bus.observe(s).unwrap();
    let external = Rc::new(Cell::new(0));
    let events = external.clone();
    bus.bind_intents(move |e| {
        events.set(events.get() + 1);
        Ok(if matches!(e, buses::Event::Configuration(_)) {
            buses::Handoff::Tracked(42)
        } else {
            buses::Handoff::UiOnly
        })
    });
    let mut descendants = vec![bus.widget().clone().upcast::<gtk::Widget>()];
    let mut index = 0;
    while index < descendants.len() {
        let mut child = descendants[index].first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            descendants.push(c);
        }
        index += 1;
    }
    let add = descendants
        .iter()
        .filter_map(|w| w.downcast_ref::<gtk::Button>())
        .find(|b| b.label().as_deref() == Some("Add stereo bus"))
        .unwrap();
    assert!(add.is_sensitive());
    add.emit_clicked();
    assert_eq!(bus.awaited_request(), Some(42));
    let v = RoutingControls::new();
    v.observe(Capture::from_bus_controls(g.clone(), &bus, Budget::default()).unwrap())
        .unwrap();
    let weak = Rc::downgrade(&v.0);
    let bus_handler = bus.clone();
    let g_handler = g.clone();
    v.bind_intents(move |e| {
        let Event::Selection { binding, key, .. } = e else {
            return Err("routing must wait for bus configuration".into());
        };
        bus_handler
            .select_destination(&binding, &key)
            .map_err(|e| format!("{e:?}"))?;
        RoutingControls(weak.upgrade().unwrap())
            .observe(
                Capture::from_bus_controls(g_handler.clone(), &bus_handler, Budget::default())
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    });
    assert_eq!(v.0.model.borrow().availability(), Err(Error::Pending));
    let rec =
        v.0.model
            .borrow()
            .choices()
            .into_iter()
            .find(|c| c.kind == Destination::Rec && !c.selected)
            .unwrap();
    button(&v, &Key::Choice(rec.key.clone())).emit_clicked();
    assert_eq!(bus.selected_rec_key(), Some(rec.key.clone()));
    assert_eq!(bus.awaited_request(), Some(42));
    let output =
        v.0.model
            .borrow()
            .choices()
            .into_iter()
            .find(|c| c.kind == Destination::Output && !c.selected)
            .unwrap();
    button(&v, &Key::Choice(output.key.clone())).emit_clicked();
    assert_eq!(bus.selected_output_key(), Some(output.key));
    assert_eq!(bus.selected_rec_key(), Some(rec.key));
    assert_eq!(
        external.get(),
        1,
        "selection bridge must not send a second configuration callback"
    );
    let stale = bus.inspect_model(|m| m.bind().unwrap()).unwrap();
    bus.offer_snapshot(snapshot(&g));
    assert!(Capture::from_bus_controls(g.clone(), &bus, Budget::default()).is_err());
    assert!(bus.select_destination(&stale, "bus|rec-1-2").is_err());
    pump();
    assert!(matches!(
        bus.select_destination(&stale, "bus|rec-1-2"),
        Err(buses::ViewError::Model(buses::Error::StaleBinding))
    ));
    assert_eq!(bus.awaited_request(), Some(42));
}

#[gtk::test]
fn unavailable_publication_retains_pending_and_disables_old_callbacks_until_fresh_capture() {
    let (v, g, bus) = view();
    let events = Rc::new(RefCell::new(vec![]));
    let seen = events.clone();
    v.bind_intents(move |e| {
        seen.borrow_mut().push(e);
        Ok(())
    });
    let (s, d) = first(&v.0.model.borrow());
    let key = Key::Route(s, d);
    let old = button(&v, &key);
    v.offer(capture(&g, &bus)).unwrap();
    v.unavailable("Synthetic publication unavailable");
    pump();
    assert!(
        v.0.unavailable.get(),
        "old queued captures must not undo lost authority"
    );
    old.emit_clicked();
    assert!(events.borrow().is_empty());
    assert!(v.0.buttons.borrow().iter().all(|(_, b)| !b.is_sensitive()));
    v.observe(capture(&g, &bus)).unwrap();
    button(&v, &key).emit_clicked();
    let i = intent(events.borrow_mut().pop().unwrap());
    v.unavailable("Synthetic busy read");
    assert_eq!(v.pending(), Some(i.request.clone()));
    assert!(v.report(&i.request, report(&i, NativeProgress::Queued)));
    assert_eq!(v.pending(), Some(i.request));
}

#[gtk::test]
fn reentrant_bus_handoff_without_request_id_already_blocks_routing_capture() {
    let g = graph();
    let bus = Rc::new(buses::BusControls::<Rc<()>, u64, u64>::new(
        buses::Budget::default(),
    ));
    let mut s = snapshot(&g);
    s.capabilities.idle_configuration = buses::Support::Available;
    s.capabilities.routed_layout = buses::Support::Available;
    bus.observe(s).unwrap();
    let seen = Rc::new(Cell::new(false));
    let during = seen.clone();
    let weak = Rc::downgrade(&bus);
    let g_handler = g.clone();
    bus.bind_intents(move |e| {
        assert!(matches!(e, buses::Event::Configuration(_)));
        let bus = weak.upgrade().unwrap();
        assert!(bus.awaited_request().is_none());
        assert!(bus.configuration_pending());
        let mut m = Model::default();
        m.observe(Capture::from_bus_controls(g_handler.clone(), &bus, Budget::default()).unwrap())
            .unwrap();
        assert_eq!(m.availability(), Err(Error::Pending));
        during.set(true);
        Ok(buses::Handoff::Tracked(9))
    });
    let mut descendants = vec![bus.widget().clone().upcast::<gtk::Widget>()];
    let mut index = 0;
    while index < descendants.len() {
        let mut child = descendants[index].first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            descendants.push(c);
        }
        index += 1;
    }
    descendants
        .iter()
        .filter_map(|w| w.downcast_ref::<gtk::Button>())
        .find(|b| b.label().as_deref() == Some("Add stereo bus"))
        .unwrap()
        .emit_clicked();
    assert!(seen.get());
    assert_eq!(bus.awaited_request(), Some(9));
}

#[gtk::test]
fn direct_old_publication_and_failed_reconciliation_do_not_discard_newest_queued_graph() {
    let (v, g, bus) = view();
    v.bind_intents(|_| Ok(()));
    let next = Stamped {
        stamp: Stamp {
            epoch: 3,
            revision: 2,
        },
        value: Arc::new((*g.value).clone()),
    };
    let bus2 = owner(&next);
    v.offer(capture(&next, &bus2)).unwrap();
    assert_eq!(v.observe(capture(&g, &bus)), Err(Error::OldGraph));
    assert_eq!(v.reconciled(capture(&g, &bus)), Err(Error::OldGraph));
    pump();
    assert_eq!(
        v.0.model.borrow().capture.as_ref().unwrap().graph.stamp,
        next.stamp
    );
    let (s, d) = first(&v.0.model.borrow());
    button(&v, &Key::Route(s, d)).emit_clicked();
    let request = v.pending().unwrap();
    let latest = Stamped {
        stamp: Stamp {
            epoch: 3,
            revision: 3,
        },
        value: Arc::new((*next.value).clone()),
    };
    let bus3 = owner(&latest);
    v.offer(capture(&latest, &bus3)).unwrap();
    assert_eq!(v.reconciled(capture(&latest, &bus3)), Err(Error::Pending));
    pump();
    assert_eq!(v.pending(), Some(request));
    assert_eq!(
        v.0.model.borrow().capture.as_ref().unwrap().graph.stamp,
        latest.stamp
    );
    assert!(v.widget().is_sensitive());
}

#[gtk::test]
fn unmap_remap_cancels_draft_but_keeps_real_pending_and_rebinds_fresh_controls() {
    let (v, _, _) = view();
    let seen = Rc::new(RefCell::new(vec![]));
    let events = seen.clone();
    v.bind_intents(move |e| {
        events.borrow_mut().push(e);
        Ok(())
    });
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(700)
        .child(v.widget())
        .build();
    window.present();
    settle(50);
    button(&v, &Key::Reset).emit_clicked();
    let stale = button(&v, &Key::Confirm);
    window.set_visible(false);
    pump();
    window.present();
    settle(50);
    assert!(!v.0.confirmation.is_visible());
    stale.emit_clicked();
    assert!(seen.borrow().is_empty());
    let (s, d) = first(&v.0.model.borrow());
    let key = Key::Route(s, d);
    button(&v, &key).emit_clicked();
    let i = intent(seen.borrow_mut().pop().unwrap());
    window.set_visible(false);
    pump();
    window.present();
    settle(50);
    assert_eq!(v.pending(), Some(i.request.clone()));
    assert!(!button(&v, &key).is_sensitive());
    v.report(
        &i.request,
        report(
            &i,
            NativeProgress::Rejected {
                reason: NativeFailure::Disconnected,
                counts: Counts::default(),
            },
        ),
    );
    assert!(v.pending().is_none());
    assert!(button(&v, &key).is_sensitive());
    button(&v, &key).emit_clicked();
    assert_eq!(seen.borrow().len(), 1);
    window.close();
    pump();
}

#[gtk::test]
fn custom_route_widget_opens_patchbay_without_history_and_accessible_names_describe_state() {
    let (v, g, mut bus) = view();
    let key = bus
        .snapshot()
        .unwrap()
        .mixer
        .destinations
        .iter()
        .find(|d| d.node.id == 21)
        .unwrap()
        .key
        .clone();
    let bind = bus.bind().unwrap();
    bus.select_destination(&bind, &key).unwrap();
    v.observe(capture(&g, &bus)).unwrap();
    let seen = Rc::new(RefCell::new(vec![]));
    let events = seen.clone();
    v.bind_intents(move |e| {
        events.borrow_mut().push(e);
        Ok(())
    });
    let row =
        v.0.model
            .borrow()
            .rows()
            .into_iter()
            .find(|r| {
                r.output
                    .as_ref()
                    .is_some_and(|r| matches!(r.state, RouteState::Custom { .. }))
            })
            .unwrap();
    let dest = row.output.unwrap();
    let button = button(&v, &Key::Route(row.key, dest.destination));
    #[cfg(feature = "a11y-tests")]
    {
        use gtk::glib::translate::IntoGlib;
        let text = format!(
            "{} to {}: {}: custom; open patchbay",
            row.name, dest.name, dest.name
        );
        let expected = std::ffi::CString::new(text).unwrap();
        // SAFETY: test backend accepts one borrowed string for the Label property.
        unsafe {
            let error = gtk::ffi::gtk_test_accessible_check_property(
                button.as_ptr().cast(),
                gtk::AccessibleProperty::Label.into_glib(),
                expected.as_ptr(),
            );
            if !error.is_null() {
                let error: glib::GString = glib::translate::from_glib_full(error);
                panic!("{error}");
            }
        }
    }
    button.emit_clicked();
    assert!(matches!(
        seen.borrow_mut().pop(),
        Some(Event::OpenPatchbay { .. })
    ));
    assert!(v.pending().is_none());
}

#[gtk::test]
fn widget_labels_custom_navigation_and_exact_history_handoff_have_no_default_dispatch() {
    let (v, g, bus) = view();
    assert!(v.status_text().contains("controls unavailable"));
    assert!(v.0.buttons.borrow().iter().all(|(_, b)| !b.is_sensitive()));
    let events = Rc::new(RefCell::new(vec![]));
    let seen = events.clone();
    v.bind_intents(move |e| {
        seen.borrow_mut().push(e);
        Ok(())
    });
    let row = v.0.model.borrow().rows().into_iter().next().unwrap();
    let dest = row.output.unwrap().destination;
    button(&v, &Key::Route(row.key.clone(), dest)).emit_clicked();
    assert!(v.pending().is_some());
    assert!(v.status_text().contains("pending"));
    let event = events.borrow_mut().pop().unwrap();
    let i = intent(event);
    assert!(Arc::ptr_eq(&i.graph.value, &g.value));
    assert!(Rc::ptr_eq(&i.origin, &bus.snapshot().unwrap().origin));
    assert!(v.report(
        &i.request,
        report(
            &i,
            NativeProgress::Rejected {
                reason: NativeFailure::Stopped,
                counts: Counts::default()
            }
        )
    ));
    assert!(v.pending().is_none());
    assert!(v.status_text().contains("rejected"));
}

#[gtk::test]
fn confirmation_cancel_reopen_stale_widget_and_replacement_never_dispatch_old_reset() {
    let (v, g, bus) = view();
    let events = Rc::new(RefCell::new(vec![]));
    let seen = events.clone();
    v.bind_intents(move |e| {
        seen.borrow_mut().push(e);
        Ok(())
    });
    button(&v, &Key::Reset).emit_clicked();
    let stale = button(&v, &Key::Confirm);
    assert!(v.0.confirmation.is_visible());
    button(&v, &Key::Cancel).emit_clicked();
    assert!(!v.0.confirmation.is_visible());
    assert!(events.borrow().is_empty());
    button(&v, &Key::Reset).emit_clicked();
    stale.emit_clicked();
    assert!(events.borrow().is_empty());
    let stale = button(&v, &Key::Confirm);
    v.observe(capture(&g, &bus)).unwrap();
    stale.emit_clicked();
    assert!(events.borrow().is_empty());
    button(&v, &Key::Reset).emit_clicked();
    button(&v, &Key::Confirm).emit_clicked();
    assert_eq!(events.borrow().len(), 1);
    assert!(v.pending().is_some());
}

#[gtk::test]
fn queued_reports_reentrant_handoff_and_coalesced_graphs_do_not_release_pending() {
    let (v, g, bus) = view();
    let weak = Rc::downgrade(&v.0);
    let seen = Rc::new(RefCell::new(None));
    let keep = seen.clone();
    v.bind_intents(move |e| {
        if let Event::History(i) = e {
            let v = RoutingControls(weak.upgrade().unwrap());
            v.report(&i.request, report(&i, NativeProgress::Queued));
            *keep.borrow_mut() = Some(i);
        }
        Ok(())
    });
    let (s, d) = first(&v.0.model.borrow());
    button(&v, &Key::Route(s, d)).emit_clicked();
    let token = v.pending().unwrap();
    let before = v.0.renders.get();
    for _ in 0..1000 {
        v.offer(capture(&g, &bus)).unwrap();
    }
    assert_eq!(v.0.renders.get(), before);
    assert_eq!(v.pending(), Some(token.clone()));
    pump();
    assert_eq!(v.0.renders.get(), before + 1);
    assert_eq!(v.pending(), Some(token.clone()));
    let i = seen.borrow_mut().take().unwrap();
    assert!(v.report(
        &token,
        report(
            &i,
            NativeProgress::ExecutedLocally(Counts {
                submitted: 1,
                ..Default::default()
            })
        )
    ));
    assert_eq!(v.pending(), Some(token.clone()));
    assert!(v.report(
        &token,
        report(
            &i,
            NativeProgress::Unconfirmed {
                reason: NativeFailure::ObservationTimedOut,
                counts: Counts::default()
            }
        )
    ));
    assert!(v.pending().is_none());
    v.observe(capture(&g, &bus)).unwrap();
    assert_eq!(
        v.0.model.borrow().availability(),
        Err(Error::ReconciliationRequired)
    );
}

#[gtk::test]
fn stale_route_callbacks_selection_handoff_and_fresh_retry_are_scoped_to_the_publication() {
    let (v, g, bus) = view();
    let seen = Rc::new(Cell::new(0));
    let events = seen.clone();
    v.bind_intents(move |_| {
        events.set(events.get() + 1);
        Err("synthetic admission full".into())
    });
    let (s, d) = first(&v.0.model.borrow());
    let key = Key::Route(s, d);
    let stale = button(&v, &key);
    v.observe(capture(&g, &bus)).unwrap();
    stale.emit_clicked();
    assert_eq!(seen.get(), 0);
    button(&v, &key).emit_clicked();
    assert_eq!(seen.get(), 1);
    assert!(v.pending().is_none());
    assert!(v.status_text().contains("admission full"));
    button(&v, &key).emit_clicked();
    assert_eq!(seen.get(), 2);
    let selected =
        v.0.model
            .borrow()
            .choices()
            .into_iter()
            .find(|c| !c.selected)
            .unwrap();
    button(&v, &Key::Choice(selected.key)).emit_clicked();
    assert_eq!(seen.get(), 3);
    assert!(v.pending().is_none());
}

#[gtk::test]
fn disconnected_unknown_configuration_and_large_wrapped_labels_remain_bounded_and_resizable() {
    let (v, g, mut bus) = view();
    v.bind_intents(|_| Ok(()));
    let mut large = (*g.value).clone();
    for index in 0..64 {
        let mut node = large.knopen[0].clone();
        node.id = 1000 + index;
        node.serial = Some(100000 + u64::from(index));
        node.naam = format!("synthetic.long-source-{index}-{}", "label".repeat(80));
        large.knopen.push(node.clone());
        let mut port = large.poorten[0].clone();
        port.id = 10000 + index;
        port.knoop = node.id;
        large.poorten.push(port);
    }
    for index in 0..40 {
        let mut node = large.knopen.iter().find(|n| n.id == 20).unwrap().clone();
        node.id = 2000 + index;
        node.serial = Some(200000 + u64::from(index));
        node.naam = format!("synthetic.long-destination-{index}-{}", "label".repeat(80));
        large.knopen.push(node.clone());
        let mut port = large.poorten.iter().find(|p| p.id == 210).unwrap().clone();
        port.id = 20000 + index;
        port.knoop = node.id;
        large.poorten.push(port);
    }
    let large = Stamped {
        stamp: Stamp {
            epoch: 3,
            revision: 2,
        },
        value: Arc::new(large),
    };
    bus.observe(snapshot(&large)).unwrap();
    v.observe(capture(&large, &bus)).unwrap();
    let window = gtk::Window::builder()
        .default_width(420)
        .default_height(320)
        .child(v.widget())
        .build();
    window.present();
    settle(80);
    assert_eq!(v.0.model.borrow().rows().len(), 68);
    assert!(
        window.width() <= 450,
        "narrow layout expanded to {}",
        window.width()
    );
    window.set_default_size(800, 500);
    settle(80);
    assert!(window.width() <= 850);
    assert!(v.widget().is_mapped());
    let mut unknown = snapshot(&large);
    unknown.configuration = None;
    bus.observe(unknown).unwrap();
    v.observe(capture(&large, &bus)).unwrap();
    assert!(v.status_text().contains("configuration unavailable"));
    let disconnected = Stamped {
        stamp: Stamp {
            epoch: 4,
            revision: 1,
        },
        value: Arc::new(Momentopname::default()),
    };
    bus.observe(snapshot(&disconnected)).unwrap();
    v.observe(capture(&disconnected, &bus)).unwrap();
    assert!(v.0.model.borrow().rows().is_empty());
    assert!(v.pending().is_none());
    window.close();
    pump();
}
