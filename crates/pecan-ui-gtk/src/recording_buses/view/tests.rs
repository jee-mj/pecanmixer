//! Synthetic GTK fixtures only. No application owner, server or writer is used.
use super::*;
use pecan_core::graph::Momentopname;
pub(super) type View = BusControls<Rc<()>, u64, u64>;
pub(super) fn frame() -> Snapshot<Rc<()>, u64, u64> {
    let graph: Momentopname = serde_json::from_str(include_str!(
        "../../../../../fixtures/domain/audio-policy.json"
    ))
    .unwrap();
    let mut config: BusConfig = serde_json::from_str(include_str!(
        "../../../../../fixtures/domain/bus-config.json"
    ))
    .unwrap();
    config.midi = false;
    Snapshot {
        origin: Rc::new(()),
        connected: true,
        closing: false,
        mixer: Arc::new(Mixer::build(&graph, &config.definitions().unwrap())),
        configuration: Some(config),
        default_sink: graph.standaard_sink,
        has_bus_routes: false,
        activity: Activity::Idle,
        tracks: vec![],
        operation: None,
        capabilities: Capabilities {
            idle_configuration: Support::Available,
            ..Default::default()
        },
    }
}
pub(super) fn descendants(root: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut all = vec![root.as_ref().clone()];
    let mut i = 0;
    while i < all.len() {
        let mut child = all[i].first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            all.push(w);
        }
        i += 1;
    }
    all
}
pub(super) fn button(view: &View, text: &str) -> gtk::Button {
    descendants(view.widget())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some(text))
        .unwrap()
}
fn text(view: &View) -> String {
    descendants(view.widget())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Label>().ok())
        .map(|l| l.text().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
#[cfg(feature = "a11y-tests")]
fn named(widget: &impl IsA<gtk::Widget>, expected: &str) {
    use gtk::glib::translate::IntoGlib;
    let expected = std::ffi::CString::new(expected).unwrap();
    // SAFETY: GTK's test backend expects one string for the Label property;
    // the borrowed widget and NUL-terminated expected string outlive this call.
    unsafe {
        let error = gtk::ffi::gtk_test_accessible_check_property(
            widget.as_ref().as_ptr().cast(),
            gtk::AccessibleProperty::Label.into_glib(),
            expected.as_ptr(),
        );
        if !error.is_null() {
            let error: gtk::glib::GString = gtk::glib::translate::from_glib_full(error);
            panic!("Accessible label mismatch: {error}");
        }
    }
}
pub(super) fn connected() -> (View, Rc<RefCell<Vec<Event<Rc<()>>>>>) {
    let view = View::new(Budget::default());
    let events = Rc::new(RefCell::new(vec![]));
    let log = events.clone();
    view.bind_intents(move |event| {
        let config = matches!(&event, Event::Configuration(_));
        log.borrow_mut().push(event);
        Ok(if config {
            Handoff::Tracked(42)
        } else {
            Handoff::UiOnly
        })
    });
    view.observe(frame()).unwrap();
    (view, events)
}
#[gtk::test]
fn unattached_view_is_observational_with_named_unsupported_recording_controls() {
    let view = View::new(Budget::default());
    view.observe(frame()).unwrap();
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    assert!(!button(&view, "Record armed buses").is_sensitive());
    assert_eq!(view.0.rows.borrow().len(), 4);
    assert!(text(&view).contains("Gain -6.0 dB"));
    assert!(text(&view).contains("absence does not confirm completion"));
    let record = button(&view, "Record armed buses");
    assert!(record
        .tooltip_text()
        .unwrap()
        .contains("pending integration"));
    #[cfg(feature = "a11y-tests")]
    {
        named(&record, "Record armed buses · unavailable");
        let p = view.0.presentation.borrow();
        let p = p.as_ref().unwrap();
        named(&p.output, "Output destination to edit");
        named(&p.rec, "Rec destination to edit");
    }
}
#[gtk::test]
fn dropdown_choices_are_independent_ui_events_without_configuration_or_recording() {
    let (view, events) = connected();
    let initial_rec = view.selected_rec_key();
    let (output, rec, output_last, rec_second) = {
        let p = view.0.presentation.borrow();
        let p = p.as_ref().unwrap();
        (
            p.output.clone(),
            p.rec.clone(),
            p.output_keys.len() as u32 - 1,
            1,
        )
    };
    output.set_selected(output_last);
    assert_eq!(view.selected_rec_key(), initial_rec);
    let selected_output = view.selected_output_key();
    rec.set_selected(rec_second);
    assert_eq!(view.selected_output_key(), selected_output);
    assert_eq!(events.borrow().len(), 2);
    assert!(events
        .borrow()
        .iter()
        .all(|e| matches!(e, Event::Selection { .. })));
    assert!(!view.0.awaiting());
    assert!(button(&view, "Add stereo bus").is_sensitive());
}
#[gtk::test]
fn handoff_waits_for_its_real_identity_and_never_optimistically_applies_a_preview() {
    let (view, events) = connected();
    button(&view, "Add stereo bus").emit_clicked();
    assert_eq!(events.borrow().len(), 1);
    assert!(view.0.awaiting());
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    assert_eq!(
        view.0
            .model
            .borrow()
            .snapshot()
            .unwrap()
            .configuration
            .as_ref()
            .unwrap()
            .widths
            .len(),
        4
    );
    view.observe(frame()).unwrap();
    assert!(
        view.0.awaiting(),
        "unrelated frame cannot resolve a tracked handoff"
    );
    let mut unrelated = frame();
    unrelated.operation = Some(Operation {
        request: 41,
        progress: Progress::NoOpLocally,
    });
    view.observe(unrelated).unwrap();
    assert!(view.0.awaiting());
    let mut queued = frame();
    queued.operation = Some(Operation {
        request: 42,
        progress: Progress::Queued,
    });
    view.observe(queued).unwrap();
    assert!(view.0.awaiting());
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    assert!(text(&view).contains("queued; not applied"));
    let mut applied = frame();
    applied.configuration = Some(applied.configuration.as_ref().unwrap().add().unwrap());
    applied.operation = Some(Operation {
        request: 42,
        progress: Progress::AppliedLocally,
    });
    view.observe(applied).unwrap();
    assert_eq!(view.0.rows.borrow().len(), 5);
    assert!(text(&view).contains("applied locally"));
    assert!(!button(&view, "Record armed buses").is_sensitive());
}
#[gtk::test]
fn errors_refresh_and_fresh_retry_preserve_pending_and_do_not_replay_old_actions() {
    let (view, events) = connected();
    let attempts = Rc::new(Cell::new(0));
    let count = attempts.clone();
    let log = events.clone();
    view.bind_intents(move |event| {
        let config = matches!(&event, Event::Configuration(_));
        log.borrow_mut().push(event);
        if config {
            count.set(count.get() + 1);
            if count.get() == 1 {
                return Err("Synthetic queue full".into());
            }
            Ok(Handoff::Tracked(77))
        } else {
            Ok(Handoff::UiOnly)
        }
    });
    let old = button(&view, "Add stereo bus");
    old.emit_clicked();
    assert!(text(&view).contains("Synthetic queue full"));
    assert!(!view.0.awaiting());
    button(&view, "Refresh status").emit_clicked();
    assert!(matches!(
        events.borrow().last(),
        Some(Event::Refresh { .. })
    ));
    assert_eq!(attempts.get(), 1);
    view.observe(frame()).unwrap();
    old.emit_clicked();
    assert_eq!(
        attempts.get(),
        1,
        "retired callback cannot retry against fresh state"
    );
    button(&view, "Add stereo bus").emit_clicked();
    assert_eq!(attempts.get(), 2);
    assert!(view.0.awaiting());
    let mut rejected = frame();
    rejected.operation = Some(Operation {
        request: 77,
        progress: Progress::Rejected("Synthetic stale revision".into()),
    });
    view.observe(rejected).unwrap();
    assert!(!view.0.awaiting());
    assert!(text(&view).contains("rejected"));
    assert!(button(&view, "Add stereo bus").is_sensitive());
    let mut partial = frame();
    partial.operation = Some(Operation {
        request: 77,
        progress: Progress::PartiallyApplied("Synthetic partial growth".into()),
    });
    view.observe(partial).unwrap();
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    assert!(button(&view, "Refresh status").is_sensitive());
    assert!(text(&view).contains("partially applied"));
}
#[gtk::test]
fn active_capabilities_and_stale_controls_remain_disabled_until_an_owner_publication() {
    let (view, events) = connected();
    let stale = button(&view, "Add stereo bus");
    let old_dropdown = view
        .0
        .presentation
        .borrow()
        .as_ref()
        .unwrap()
        .output
        .clone();
    let mut active = frame();
    active.activity = Activity::Recording;
    view.observe(active).unwrap();
    stale.emit_clicked();
    old_dropdown.set_selected(0);
    assert!(events.borrow().is_empty());
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    assert!(button(&view, "Add stereo bus")
        .tooltip_text()
        .unwrap()
        .contains("during recording"));
    let mut disconnected = frame();
    disconnected.connected = false;
    view.observe(disconnected).unwrap();
    assert!(view.selected_output_key().is_none());
    assert!(view.selected_rec_key().is_none());
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    view.observe(frame()).unwrap();
    assert!(button(&view, "Add stereo bus").is_sensitive());
    stale.emit_clicked();
    assert!(events.borrow().is_empty());
}
#[gtk::test]
fn track_widgets_follow_stable_identity_across_phase_changes_missing_buses_and_reconnect() {
    let (view, _) = connected();
    let mut first = frame();
    first.activity = Activity::Recording;
    first.tracks = vec![
        Track {
            identity: 1,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::WriterReady,
            elapsed: None,
        },
        Track {
            identity: 2,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::Finalizing,
            elapsed: Some(Duration::from_secs(2)),
        },
    ];
    view.observe(first).unwrap();
    let original = view.0.tracks.borrow()[&1].clone();
    assert!(text(&view).contains("capture unconfirmed"));
    assert!(text(&view).contains("join pending"));
    let mut next = frame();
    next.tracks = vec![
        Track {
            identity: 1,
            bus_key: "retired-synthetic-bus".into(),
            phase: TrackPhase::Finalized(Err("Synthetic finalize failure".into())),
            elapsed: None,
        },
        Track {
            identity: 3,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::Recording,
            elapsed: Some(Duration::from_secs(1)),
        },
    ];
    view.observe(next).unwrap();
    assert_eq!(view.0.tracks.borrow()[&1], original);
    assert!(!view.0.tracks.borrow().contains_key(&2));
    assert!(view.0.orphan_frame.is_visible());
    assert!(text(&view).contains("Synthetic finalize failure"));
    assert!(!text(&view).contains("Joined and finalized"));
    let mut missing = frame();
    missing.connected = false;
    view.observe(missing).unwrap();
    assert!(view.0.tracks.borrow().is_empty());
    assert!(text(&view).contains("absence does not confirm completion"));
    view.observe(frame()).unwrap();
    assert!(!text(&view).contains("Joined and finalized"));
}
#[gtk::test]
fn malformed_publications_and_duplicate_track_ids_retire_controls_without_claiming_completion() {
    let (view, events) = connected();
    let stale = button(&view, "Add stereo bus");
    let mut duplicate = frame();
    duplicate.tracks = vec![
        Track {
            identity: 5,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::Starting,
            elapsed: None
        };
        2
    ];
    assert_eq!(
        view.observe(duplicate),
        Err(ViewError::DuplicateTrackIdentity)
    );
    stale.emit_clicked();
    assert!(events.borrow().is_empty());
    assert!(view.selected_output_key().is_none());
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    assert!(text(&view).contains("repeated track identity"));
    let mut invalid = frame();
    invalid.configuration.as_mut().unwrap().widths = vec![3];
    assert!(matches!(view.observe(invalid), Err(ViewError::Model(_))));
    view.observe(frame()).unwrap();
    assert!(button(&view, "Add stereo bus").is_sensitive());
}
#[gtk::test]
fn reentrant_publication_is_correlated_and_view_drop_releases_callback_owners() {
    let view = View::new(Budget::default());
    let weak = Rc::downgrade(&view.0);
    let callback_weak = weak.clone();
    view.bind_intents(move |event| {
        if matches!(event, Event::Configuration(_)) {
            let state = callback_weak.upgrade().unwrap();
            let wrapper = BusControls(state);
            let mut queued = frame();
            queued.operation = Some(Operation {
                request: 88,
                progress: Progress::Queued,
            });
            wrapper.observe(queued).unwrap();
            Ok(Handoff::Tracked(88))
        } else {
            Ok(Handoff::UiOnly)
        }
    });
    view.observe(frame()).unwrap();
    button(&view, "Add stereo bus").emit_clicked();
    assert!(view.0.awaiting());
    assert!(text(&view).contains("queued; not applied"));
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(1000)
        .default_height(700)
        .build();
    window.present();
    window.set_child(None::<&gtk::Widget>);
    drop(view);
    assert!(weak.upgrade().is_none());
    window.close();
}

#[gtk::test]
fn additive_arm_restores_observed_check_state_until_matching_owner_readback() {
    let (view, events) = connected();
    let arm = view.0.rows.borrow()["rec-4"]
        .controls
        .first_child()
        .unwrap()
        .next_sibling()
        .unwrap()
        .downcast::<gtk::CheckButton>()
        .unwrap();
    assert!(!arm.is_active());
    arm.set_active(true);
    assert!(
        matches!(&events.borrow()[0],Event::Configuration(i) if i.action==Action::Arm{bus:"rec-4".into(),additive:true})
    );
    let shown = view.0.rows.borrow()["rec-4"]
        .controls
        .first_child()
        .unwrap()
        .next_sibling()
        .unwrap()
        .downcast::<gtk::CheckButton>()
        .unwrap();
    assert!(!shown.is_active());
    assert!(!shown.is_sensitive());
    let mut applied = frame();
    applied.configuration = Some(
        applied
            .configuration
            .as_ref()
            .unwrap()
            .arm("rec-4", true)
            .unwrap(),
    );
    applied.operation = Some(Operation {
        request: 42,
        progress: Progress::AppliedLocally,
    });
    view.observe(applied).unwrap();
    let shown = view.0.rows.borrow()["rec-4"]
        .controls
        .first_child()
        .unwrap()
        .next_sibling()
        .unwrap()
        .downcast::<gtk::CheckButton>()
        .unwrap();
    assert!(shown.is_active());
    assert!(shown.is_sensitive());
    assert_eq!(events.borrow().len(), 1);
}

#[gtk::test]
fn malformed_adapter_reply_and_missing_model_can_recover_only_through_readonly_refresh() {
    let (view, events) = connected();
    let log = events.clone();
    view.bind_intents(move |event| {
        log.borrow_mut().push(event);
        Ok(Handoff::UiOnly)
    });
    button(&view, "Add stereo bus").emit_clicked();
    assert_eq!(
        view.publication_error(),
        Some(ViewError::UnexpectedAdapterReply)
    );
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    assert!(text(&view).contains("effects unknown"));
    button(&view, "Refresh status").emit_clicked();
    assert!(matches!(
        events.borrow().last(),
        Some(Event::Refresh { .. })
    ));
    view.observe(frame()).unwrap();
    let mut invalid = frame();
    invalid.configuration.as_mut().unwrap().widths = vec![3];
    assert!(view.observe(invalid).is_err());
    assert!(button(&view, "Refresh status").is_sensitive());
    button(&view, "Refresh status").emit_clicked();
    assert!(matches!(
        events.borrow().last(),
        Some(Event::Refresh { .. })
    ));
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, Event::Configuration(_)))
            .count(),
        1
    );
}

fn settle() {
    let until = std::time::Instant::now() + Duration::from_millis(70);
    let context = gtk::glib::MainContext::default();
    while std::time::Instant::now() < until {
        while context.pending() && std::time::Instant::now() < until {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[gtk::test]
fn publication_storm_has_one_latest_slot_and_cannot_release_a_different_request() {
    let (view, events) = connected();
    button(&view, "Add stereo bus").emit_clicked();
    assert_eq!(view.awaited_request(), Some(42));
    let retired = button(&view, "Add stereo bus");
    let renders = view.0.renders.get();
    let mut latest = None;
    for _ in 0..1000 {
        let mut s = frame();
        s.operation = Some(Operation {
            request: 41,
            progress: Progress::NoOpLocally,
        });
        latest = Some(s.origin.clone());
        view.offer_snapshot(s);
    }
    retired.emit_clicked();
    assert_eq!(events.borrow().len(), 1);
    assert_eq!(view.0.renders.get(), renders);
    assert!(view.selected_output_key().is_none());
    settle();
    assert!(Rc::ptr_eq(
        &view.0.model.borrow().snapshot().unwrap().origin,
        latest.as_ref().unwrap()
    ));
    assert_eq!(view.awaited_request(), Some(42));
    assert_eq!(view.0.renders.get(), renders + 1);
    let mut terminal = frame();
    terminal.operation = Some(Operation {
        request: 42,
        progress: Progress::Rejected("Synthetic rejection".into()),
    });
    view.offer_snapshot(terminal);
    settle();
    assert!(view.awaited_request().is_none());
    assert!(button(&view, "Add stereo bus").is_sensitive());
}

#[gtk::test]
fn long_labels_resize_and_track_reordering_keep_focus_and_identity_without_new_intents() {
    let (view, events) = connected();
    let mut s = frame();
    let mut projection = (*s.mixer).clone();
    for d in &mut projection.destinations {
        d.name = format!("Synthetic long destination {}", "label ".repeat(100));
    }
    s.mixer = Arc::new(projection);
    s.tracks = vec![
        Track {
            identity: 1,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::Recording,
            elapsed: None,
        },
        Track {
            identity: 2,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::Finalizing,
            elapsed: None,
        },
    ];
    view.observe(s).unwrap();
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(1000)
        .default_height(700)
        .build();
    window.present();
    settle();
    let add = button(&view, "Add stereo bus");
    assert!(add.grab_focus());
    let first = view.0.tracks.borrow()[&1].clone();
    let second = view.0.tracks.borrow()[&2].clone();
    let mut next = frame();
    next.tracks = vec![
        Track {
            identity: 2,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::Finalized(Ok(())),
            elapsed: None,
        },
        Track {
            identity: 1,
            bus_key: "rec-1-2".into(),
            phase: TrackPhase::Recording,
            elapsed: None,
        },
    ];
    view.observe(next).unwrap();
    settle();
    assert!(button(&view, "Add stereo bus").has_focus());
    assert_eq!(view.0.tracks.borrow()[&1], first);
    assert_eq!(view.0.tracks.borrow()[&2], second);
    assert_eq!(
        view.0.rows.borrow()["rec-1-2"]
            .tracks
            .first_child()
            .unwrap(),
        second.upcast::<gtk::Widget>()
    );
    for (width, height) in [(420, 320), (1400, 900)] {
        window.set_default_size(width, height);
        settle();
        assert!(view.widget().width() > 0);
    }
    assert!(events.borrow().is_empty());
    window.close();
}

#[gtk::test]
fn reentrant_selection_publication_keeps_accepted_choice_and_rejection_restores_default_policy() {
    let view = View::new(Budget::default());
    let weak = Rc::downgrade(&view.0);
    view.bind_intents(move |event| {
        if matches!(event, Event::Selection { .. }) {
            let wrapper = BusControls(weak.upgrade().unwrap());
            wrapper.observe(frame()).unwrap();
        }
        Ok(Handoff::UiOnly)
    });
    view.observe(frame()).unwrap();
    let (dropdown, index, key) = {
        let p = view.0.presentation.borrow();
        let p = p.as_ref().unwrap();
        (
            p.output.clone(),
            p.output_keys.len() as u32 - 1,
            p.output_keys.last().unwrap().clone(),
        )
    };
    dropdown.set_selected(index);
    assert_eq!(view.selected_output_key(), Some(key));
    let weak = Rc::downgrade(&view.0);
    view.bind_intents(move |_| {
        let wrapper = BusControls(weak.upgrade().unwrap());
        wrapper.observe(frame()).unwrap();
        Err("Synthetic selection rejection".into())
    });
    let before = view.selected_output_key();
    let dropdown = view
        .0
        .presentation
        .borrow()
        .as_ref()
        .unwrap()
        .output
        .clone();
    dropdown.set_selected(0);
    assert_eq!(view.selected_output_key(), before);
    assert!(text(&view).contains("Synthetic selection rejection"));
}

fn operation_frame(request: u64, progress: Progress) -> Snapshot<Rc<()>, u64, u64> {
    let mut s = frame();
    s.operation = Some(Operation { request, progress });
    s
}

#[gtk::test]
fn retention_matching_queued_survives_unrelated_snapshot_without_a_second_handoff() {
    let (view, events) = connected();
    button(&view, "Add stereo bus").emit_clicked();
    view.observe(operation_frame(42, Progress::Queued)).unwrap();
    assert_eq!(view.awaited_request(), Some(42));
    view.observe(frame()).unwrap();
    assert_eq!(view.awaited_request(), Some(42));
    assert!(!button(&view, "Add stereo bus").is_sensitive());
    button(&view, "Add stereo bus").emit_clicked();
    assert_eq!(events.borrow().len(), 1);
}

#[gtk::test]
fn retention_matching_applying_survives_missing_mismatched_and_reconnect_publications() {
    let (view, events) = connected();
    button(&view, "Add stereo bus").emit_clicked();
    view.observe(operation_frame(42, Progress::Applying))
        .unwrap();
    assert_eq!(view.awaited_request(), Some(42));
    for snapshot in [frame(), operation_frame(41, Progress::AppliedLocally)] {
        view.observe(snapshot).unwrap();
        assert_eq!(view.awaited_request(), Some(42));
        assert!(!button(&view, "Add stereo bus").is_sensitive());
    }
    let mut offline = frame();
    offline.connected = false;
    view.observe(offline).unwrap();
    view.observe(frame()).unwrap();
    assert_eq!(view.awaited_request(), Some(42));
    button(&view, "Add stereo bus").emit_clicked();
    assert_eq!(events.borrow().len(), 1);
}

#[gtk::test]
fn retention_reentrant_queued_and_applying_reports_do_not_complete_the_handoff() {
    for progress in [Progress::Queued, Progress::Applying] {
        let view = View::new(Budget::default());
        let weak = Rc::downgrade(&view.0);
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        view.bind_intents(move |event| {
            if matches!(event, Event::Configuration(_)) {
                count.set(count.get() + 1);
                BusControls(weak.upgrade().unwrap())
                    .observe(operation_frame(88, progress.clone()))
                    .unwrap();
                Ok(Handoff::Tracked(88))
            } else {
                Ok(Handoff::UiOnly)
            }
        });
        view.observe(frame()).unwrap();
        button(&view, "Add stereo bus").emit_clicked();
        assert_eq!(view.awaited_request(), Some(88));
        view.observe(frame()).unwrap();
        assert!(!button(&view, "Add stereo bus").is_sensitive());
        button(&view, "Add stereo bus").emit_clicked();
        assert_eq!(calls.get(), 1);
    }
}

#[gtk::test]
fn retention_only_exact_terminal_results_resolve_and_reconciliation_still_gates_fresh_retry() {
    for terminal in [
        Progress::Observed,
        Progress::AppliedLocally,
        Progress::NoOpLocally,
        Progress::Rejected("Synthetic rejection".into()),
        Progress::PartiallyApplied("Synthetic incomplete resize".into()),
        Progress::Unconfirmed("Synthetic missing evidence".into()),
    ] {
        let reconcile = matches!(
            terminal,
            Progress::PartiallyApplied(_) | Progress::Unconfirmed(_)
        );
        let view = View::new(Budget::default());
        let next = Rc::new(Cell::new(41));
        let count = next.clone();
        view.bind_intents(move |event| {
            Ok(if matches!(event, Event::Configuration(_)) {
                count.set(count.get() + 1);
                Handoff::Tracked(count.get())
            } else {
                Handoff::UiOnly
            })
        });
        view.observe(frame()).unwrap();
        let stale = button(&view, "Add stereo bus");
        stale.emit_clicked();
        view.observe(operation_frame(41, terminal.clone())).unwrap();
        assert_eq!(view.awaited_request(), Some(42));
        view.observe(operation_frame(42, terminal.clone())).unwrap();
        assert_eq!(view.awaited_request(), None);
        assert_eq!(button(&view, "Add stereo bus").is_sensitive(), !reconcile);
        stale.emit_clicked();
        assert_eq!(next.get(), 42, "retired controls cannot start a retry");
        if reconcile {
            button(&view, "Refresh status").emit_clicked();
            assert_eq!(next.get(), 42, "refresh never retries the write");
            view.observe(operation_frame(42, terminal)).unwrap();
            assert!(!button(&view, "Add stereo bus").is_sensitive());
            // The owner explicitly publishes its reconciled authoritative readback.
            view.observe(frame()).unwrap();
        }
        button(&view, "Add stereo bus").emit_clicked();
        assert_eq!(view.awaited_request(), Some(43));
        view.observe(operation_frame(42, Progress::AppliedLocally))
            .unwrap();
        assert_eq!(
            view.awaited_request(),
            Some(43),
            "old terminal cannot finish a fresh retry"
        );
        assert!(!button(&view, "Add stereo bus").is_sensitive());
        view.observe(operation_frame(43, Progress::NoOpLocally))
            .unwrap();
        assert_eq!(view.awaited_request(), None);
        assert!(button(&view, "Add stereo bus").is_sensitive());
    }
}

#[gtk::test]
fn retention_invalid_matching_terminal_does_not_release_pending_work() {
    let (view, _) = connected();
    button(&view, "Add stereo bus").emit_clicked();
    for duplicate in [false, true] {
        let mut invalid = operation_frame(42, Progress::AppliedLocally);
        if duplicate {
            invalid.tracks = vec![
                Track {
                    identity: 9,
                    bus_key: "rec-1-2".into(),
                    phase: TrackPhase::Starting,
                    elapsed: None
                };
                2
            ];
        } else {
            invalid.configuration.as_mut().unwrap().widths = vec![3];
        }
        assert!(view.observe(invalid).is_err());
        assert_eq!(view.awaited_request(), Some(42));
        view.observe(frame()).unwrap();
        assert_eq!(view.awaited_request(), Some(42));
        assert!(!button(&view, "Add stereo bus").is_sensitive());
    }
    view.observe(operation_frame(42, Progress::AppliedLocally))
        .unwrap();
    assert_eq!(view.awaited_request(), None);
    assert!(button(&view, "Add stereo bus").is_sensitive());
}

#[gtk::test]
fn retention_reentrant_terminal_requires_a_valid_publication() {
    for invalid in [false, true] {
        let view = View::new(Budget::default());
        let weak = Rc::downgrade(&view.0);
        view.bind_intents(move |event| {
            if matches!(event, Event::Configuration(_)) {
                let mut snapshot = operation_frame(88, Progress::NoOpLocally);
                if invalid {
                    snapshot.tracks = vec![
                        Track {
                            identity: 9,
                            bus_key: "rec-1-2".into(),
                            phase: TrackPhase::Starting,
                            elapsed: None
                        };
                        2
                    ];
                }
                let result = BusControls(weak.upgrade().unwrap()).observe(snapshot);
                assert_eq!(result.is_err(), invalid);
                Ok(Handoff::Tracked(88))
            } else {
                Ok(Handoff::UiOnly)
            }
        });
        view.observe(frame()).unwrap();
        button(&view, "Add stereo bus").emit_clicked();
        assert_eq!(view.awaited_request(), invalid.then_some(88));
        view.observe(frame()).unwrap();
        assert_eq!(button(&view, "Add stereo bus").is_sensitive(), !invalid);
        if invalid {
            view.observe(operation_frame(88, Progress::NoOpLocally))
                .unwrap();
            assert_eq!(view.awaited_request(), None);
        }
    }
}

#[gtk::test]
fn retention_coalesced_pending_frames_require_terminal_republication_if_it_was_replaced() {
    let (view, events) = connected();
    button(&view, "Add stereo bus").emit_clicked();
    for i in 0..1000 {
        view.offer_snapshot(operation_frame(
            42,
            if i % 2 == 0 {
                Progress::Queued
            } else {
                Progress::Applying
            },
        ));
    }
    settle();
    assert_eq!(view.awaited_request(), Some(42));
    view.offer_snapshot(operation_frame(42, Progress::AppliedLocally));
    view.offer_snapshot(frame());
    settle();
    assert_eq!(
        view.awaited_request(),
        Some(42),
        "an unconsumed coalesced result is not completion evidence"
    );
    button(&view, "Add stereo bus").emit_clicked();
    assert_eq!(events.borrow().len(), 1);
    view.offer_snapshot(operation_frame(42, Progress::AppliedLocally));
    settle();
    assert_eq!(view.awaited_request(), None);
    assert!(button(&view, "Add stereo bus").is_sensitive());
}
