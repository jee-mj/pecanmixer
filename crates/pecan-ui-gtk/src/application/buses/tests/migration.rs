//! Synthetic owner/service handoffs only; no backend constructor or audio.
use super::*;
use pecan_core::graph::{Kabel, Soort};
use pecan_pipewire::bus_migration::*;

pub(super) struct Migration {
    report: RefCell<Option<MigrationReport>>,
    hook: RefCell<Option<Box<dyn Fn()>>>,
}
impl Migration {
    pub fn new(fake: &Fake, intent: &LayoutIntent) -> Self {
        let configuration = fake.config.borrow().clone();
        let mut proposed = configuration.configuration.clone().unwrap();
        let key = match &intent.action {
            LayoutAction::Split { bus }
            | LayoutAction::Join { bus }
            | LayoutAction::Remove { bus } => bus,
        };
        let def = proposed
            .buses
            .definitions()
            .unwrap()
            .into_iter()
            .find(|d| &d.key == key)
            .unwrap();
        proposed.buses = match intent.action {
            LayoutAction::Remove { .. } => {
                proposed
                    .buses
                    .remove(def.index, &intent.origin.value)
                    .unwrap()
                    .config
            }
            _ => proposed.buses.change_coupling(def.index).unwrap(),
        };
        Self {
            report: RefCell::new(Some(MigrationReport {
                ticket: intent.ticket,
                source: intent.origin.stamp,
                expected_config: intent.expected_config,
                expected_recording: intent.expected_recording,
                progress: MigrationProgress::Queued,
                configuration,
                proposed,
                attempted: 0,
                submitted: 0,
                no_ops: 0,
                in_progress: 0,
                new_observed: None,
                old_observed: None,
                ports_observed: None,
                resize: None,
                local_readback: None,
                cancellation_requested: false,
                failure: None,
            })),
            hook: RefCell::default(),
        }
    }
}
impl MigrationPoll for Migration {
    fn report(&self) -> Option<MigrationReport> {
        let report = self.report.borrow().clone();
        let hook = self.hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        report
    }
}
fn routed() -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver, BTreeMap<u32, u64>) {
    let (owner, fake, driver) = setup();
    let mut graph = graph();
    let node = graph.knopen.iter_mut().find(|n| n.id == 30).unwrap();
    node.naam = pecan_core::domain::audio::REC_NODE.into();
    node.volume = Some(vec![1.; 4]);
    let template = graph
        .poorten
        .iter()
        .find(|p| p.knoop == 30)
        .unwrap()
        .clone();
    for n in 3..=4 {
        let mut p = template.clone();
        p.id = 300 + n;
        p.naam = format!("rec_{n}");
        p.volgnummer = Some(n - 1);
        graph.poorten.push(p);
    }
    graph.kabels = vec![
        Kabel {
            id: 1000,
            client: None,
            uit_knoop: 20,
            uit_poort: 201,
            in_knoop: 30,
            in_poort: 301,
        },
        Kabel {
            id: 1001,
            client: None,
            uit_knoop: 20,
            uit_poort: 202,
            in_knoop: 30,
            in_poort: 303,
        },
    ];
    let generations = generations(&graph);
    driver.publish(graph, generations.clone());
    owner.session.poll();
    {
        let mut c = fake.config.borrow_mut();
        c.configuration.as_mut().unwrap().buses.widths = vec![2, 2];
        c.local.as_mut().unwrap().channels = 4;
        c.local.as_mut().unwrap().gains = vec![1.; 4];
    }
    owner.poll();
    (owner, fake, driver, generations)
}
fn edit(owner: &BusOwner, action: ui::Action) -> Result<Request, Error> {
    let origin = owner.origin().unwrap();
    let config = &origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses;
    let proposed = match &action {
        ui::Action::Split { bus } | ui::Action::Join { bus } => Some(
            config
                .change_coupling(
                    config
                        .definitions()
                        .unwrap()
                        .iter()
                        .find(|d| &d.key == bus)
                        .unwrap()
                        .index,
                )
                .unwrap(),
        ),
        _ => None,
    };
    owner.configure(&origin, action, proposed)
}
fn complete(
    fake: &Fake,
    index: usize,
    outcome: MigrationOutcome,
    publish: bool,
) -> MigrationReport {
    let receipt = fake.migrations.borrow()[index].1.clone();
    let mut r = receipt.report.borrow().clone().unwrap();
    r.progress = MigrationProgress::Completed(outcome);
    r.configuration.revision.revision += 1;
    r.configuration.configuration = Some(r.proposed.clone());
    let channels = r.proposed.buses.widths.iter().sum();
    r.configuration.local.as_mut().unwrap().channels = channels;
    r.configuration.local.as_mut().unwrap().gains = vec![1.; channels];
    r.local_readback = r.configuration.local.clone();
    if publish {
        *fake.config.borrow_mut() = r.configuration.clone();
    }
    *receipt.report.borrow_mut() = Some(r.clone());
    r
}
fn removed(owner: &BusOwner, driver: &FixtureDriver, gens: &BTreeMap<u32, u64>) -> Graph {
    let mut graph = (*owner.session.graph().unwrap().value).clone();
    graph.poorten.retain(|p| {
        p.knoop != 30
            || !p.ingang
            || p.soort != Soort::Audio
            || p.naam == "rec_1"
            || p.naam == "rec_2"
    });
    graph.knopen.iter_mut().find(|n| n.id == 30).unwrap().volume = Some(vec![1.; 2]);
    graph.kabels = vec![Kabel {
        id: 2000,
        client: None,
        uit_knoop: 20,
        uit_poort: 202,
        in_knoop: 30,
        in_poort: 301,
    }];
    let gens = gens
        .iter()
        .filter(|(id, _)| {
            graph.knopen.iter().any(|n| n.id == **id) || graph.poorten.iter().any(|p| p.id == **id)
        })
        .map(|(a, b)| (*a, *b))
        .collect();
    driver.publish(graph, gens)
}
#[gtk::test]
fn routed_split_widget_retains_real_receipt_and_lease_until_local_result_is_displayed() {
    let (owner, fake, _, _) = routed();
    let origin = owner.origin().unwrap();
    assert!(!button(owner.widget(), "Add stereo bus").is_sensitive());
    button(owner.widget(), "Split stereo").emit_clicked();
    let (intent, receipt) = fake.migrations.borrow()[0].clone();
    assert!(
        owner.migration_report().is_none(),
        "a pending request cannot inherit prior evidence"
    );
    assert!(Arc::ptr_eq(
        &intent.origin.value,
        &origin.0.graph.as_ref().unwrap().value
    ));
    assert_eq!(Some(intent.expected_recording), origin.0.membership);
    assert_eq!(
        intent.expected_config,
        origin.0.config.as_ref().unwrap().revision
    );
    assert!(owner.session.busy());
    assert!(fake.configs.borrow().is_empty());
    *receipt.report.borrow_mut() = None;
    owner.refresh();
    assert!(owner.session.busy());
    // Restore the retained report without inventing progress from its absence.
    *receipt.report.borrow_mut() = Some(
        Migration::new(&fake, &intent)
            .report
            .borrow()
            .clone()
            .unwrap(),
    );
    complete(&fake, 0, MigrationOutcome::AppliedLocally, true);
    owner.poll();
    assert!(!owner.session.busy());
    assert!(owner.view.awaited_request().is_none());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::AppliedLocally
    );
    assert_eq!(
        owner.migration_report().unwrap().progress,
        MigrationProgress::Completed(MigrationOutcome::AppliedLocally)
    );
    let actual = owner
        .migration_report()
        .unwrap()
        .configuration
        .configuration
        .unwrap()
        .buses;
    assert!(owner
        .view
        .inspect_model(|m| m.snapshot().unwrap().configuration.as_ref() == Some(&actual))
        .unwrap());
    fake.busy.set(true);
    owner.poll();
    assert!(!owner.session.busy());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::AppliedLocally
    );
}
#[gtk::test]
fn routed_join_uses_migration_but_unrouted_add_keeps_its_existing_separate_path() {
    let (owner, fake, _, _) = routed();
    fake.config
        .borrow_mut()
        .configuration
        .as_mut()
        .unwrap()
        .buses
        .widths = vec![1, 1, 2];
    owner.poll();
    edit(
        &owner,
        ui::Action::Join {
            bus: "rec-1".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        fake.migrations.borrow()[0].0.action,
        LayoutAction::Join { .. }
    ));
    assert!(fake.configs.borrow().is_empty());
    let (other, f, driver) = setup();
    let mut g = graph();
    g.kabels.clear();
    driver.publish(g.clone(), generations(&g));
    other.session.poll();
    other.poll();
    let origin = other.origin().unwrap();
    let cfg = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .add()
        .unwrap();
    other
        .configure(&origin, ui::Action::AddStereo, Some(cfg))
        .unwrap();
    assert_eq!(f.configs.borrow().len(), 1);
    assert!(f.migrations.borrow().is_empty());
}
#[gtk::test]
fn observed_remove_waits_for_owned_graph_fresh_config_and_accepted_display_then_survives_busy_reads(
) {
    let (owner, fake, driver, gens) = routed();
    edit(
        &owner,
        ui::Action::Remove {
            bus: "rec-1-2".into(),
        },
    )
    .unwrap();
    let future = removed(&owner, &driver, &gens);
    complete(&fake, 0, MigrationOutcome::Observed, true);
    fake.migrations.borrow()[0]
        .1
        .report
        .borrow_mut()
        .as_mut()
        .unwrap()
        .ports_observed = Some(future.stamp);
    owner.poll();
    assert!(owner.session.busy());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::AwaitingObservation
    );
    owner.session.poll();
    fake.busy.set(true);
    owner.poll();
    assert!(owner.session.busy());
    fake.busy.set(false);
    owner.poll();
    assert!(!owner.session.busy());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::Observed
    );
    owner
        .view
        .inspect_model(|m| {
            let s = m.snapshot().unwrap();
            assert_eq!(s.configuration.as_ref().unwrap().widths, vec![2]);
            assert_eq!(s.origin.0.graph.as_ref().unwrap().stamp, future.stamp);
            assert_eq!(
                s.operation.as_ref().unwrap().progress,
                ui::Progress::Observed
            );
        })
        .unwrap();
    fake.busy.set(true);
    owner.poll();
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::Observed
    );
    assert!(!owner.session.busy());
    assert!(fake.starts.borrow().is_empty());
}
#[gtk::test]
fn ordinary_coalesced_revision_is_allowed_but_replaced_port_does_not_inherit_observed_status() {
    for replace in [false, true] {
        let (owner, fake, driver, gens) = routed();
        edit(
            &owner,
            ui::Action::Remove {
                bus: "rec-1-2".into(),
            },
        )
        .unwrap();
        let observed = removed(&owner, &driver, &gens);
        complete(&fake, 0, MigrationOutcome::Observed, true);
        fake.migrations.borrow()[0]
            .1
            .report
            .borrow_mut()
            .as_mut()
            .unwrap()
            .ports_observed = Some(observed.stamp);
        let mut updated = (*observed.value).clone();
        updated
            .knopen
            .iter_mut()
            .find(|n| n.id == 10)
            .unwrap()
            .beschrijving = Some("Synthetic unrelated observation".into());
        let mut bound = gens.clone();
        if replace {
            bound.insert(301, 9001);
        }
        driver.publish(updated, bound);
        owner.session.poll();
        owner.poll();
        assert!(!owner.session.busy());
        if replace {
            assert!(matches!(
                owner.data.borrow().operation.as_ref().unwrap().progress,
                ui::Progress::Unconfirmed(_)
            ));
        } else {
            assert_eq!(
                owner.data.borrow().operation.as_ref().unwrap().progress,
                ui::Progress::Observed
            );
        }
        assert_eq!(
            owner.migration_report().unwrap().progress,
            MigrationProgress::Completed(MigrationOutcome::Observed)
        );
    }
}
#[gtk::test]
fn partial_busy_or_inflight_results_keep_ownership_and_fresh_reconciliation_preserves_raw_failure()
{
    let (owner, fake, _, _) = routed();
    let request = edit(
        &owner,
        ui::Action::Split {
            bus: "rec-1-2".into(),
        },
    )
    .unwrap();
    let mut partial = complete(&fake, 0, MigrationOutcome::PartiallyApplied, false);
    partial.failure = Some(MigrationError::ExternalConflict);
    partial.configuration.configuration = None;
    partial.in_progress = 1;
    *fake.migrations.borrow()[0].1.report.borrow_mut() = Some(partial.clone());
    owner.poll();
    owner.refresh();
    assert!(owner.session.busy());
    partial.in_progress = 0;
    *fake.migrations.borrow()[0].1.report.borrow_mut() = Some(partial.clone());
    fake.busy.set(true);
    owner.poll();
    assert!(owner.session.busy());
    fake.busy.set(false);
    owner.poll();
    assert!(
        owner.session.busy(),
        "older config cannot release the terminal barrier"
    );
    *fake.config.borrow_mut() = partial.configuration.clone();
    owner.poll();
    assert!(!owner.session.busy());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert!(owner
        .origin()
        .unwrap()
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .is_none());
    owner.refresh();
    assert!(owner.data.borrow().operation.is_none());
    let retained = owner.migration_report().unwrap();
    assert_eq!(retained.progress, partial.progress);
    assert_eq!(retained.failure, partial.failure);
    assert_eq!(retained.in_progress, 0);
    assert_eq!(fake.migrations.borrow().len(), 1);
    assert!(!owner.data.borrow().past_failures.is_empty());
}
#[gtk::test]
fn migration_poll_is_reentrant_and_stale_preflights_do_not_submit() {
    let (owner, fake, _, _) = routed();
    let old = fake.membership.get();
    fake.membership.set(RecordingRevision {
        revision: old.revision + 1,
        ..old
    });
    assert!(edit(
        &owner,
        ui::Action::Split {
            bus: "rec-1-2".into()
        }
    )
    .is_err());
    owner.poll();
    fake.config.borrow_mut().revision.revision += 1;
    assert!(edit(
        &owner,
        ui::Action::Split {
            bus: "rec-1-2".into()
        }
    )
    .is_err());
    owner.poll();
    let weak = Rc::downgrade(&owner);
    *fake.hook.borrow_mut() = Some(Box::new(move || {
        let o = weak.upgrade().unwrap();
        o.poll();
        assert!(edit(
            &o,
            ui::Action::Split {
                bus: "rec-1-2".into()
            }
        )
        .is_err());
    }));
    edit(
        &owner,
        ui::Action::Split {
            bus: "rec-1-2".into(),
        },
    )
    .unwrap();
    let weak = Rc::downgrade(&owner);
    *fake.migrations.borrow()[0].1.hook.borrow_mut() = Some(Box::new(move || {
        let o = weak.upgrade().unwrap();
        let _ = o.recording_status();
        let _ = o.migration_report();
        assert!(o.session.busy());
    }));
    complete(&fake, 0, MigrationOutcome::NoOpLocally, true);
    owner.poll();
    assert!(!owner.session.busy());
    assert_eq!(fake.migrations.borrow().len(), 1);
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::NoOpLocally
    );
}
#[gtk::test]
fn wrong_proposal_source_and_routed_add_never_gain_migration_authority() {
    let (owner, fake, _, _) = routed();
    let origin = owner.origin().unwrap();
    let buses = &origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses;
    assert!(owner
        .configure(&origin, ui::Action::AddStereo, Some(buses.add().unwrap()))
        .is_err());
    assert!(owner
        .configure(
            &origin,
            ui::Action::Split {
                bus: "rec-1-2".into()
            },
            Some(buses.clone())
        )
        .is_err());
    assert!(owner
        .configure(
            &origin,
            ui::Action::Join {
                bus: "rec-1-2".into()
            },
            Some(buses.clone())
        )
        .is_err());
    assert!(owner
        .configure(
            &origin,
            ui::Action::Remove {
                bus: "rec-1-2".into()
            },
            Some(buses.clone())
        )
        .is_err());
    assert!(fake.migrations.borrow().is_empty());
    assert!(fake.configs.borrow().is_empty());
    edit(
        &owner,
        ui::Action::Split {
            bus: "rec-1-2".into(),
        },
    )
    .unwrap();
    complete(&fake, 0, MigrationOutcome::AppliedLocally, true);
    fake.migrations.borrow()[0]
        .1
        .report
        .borrow_mut()
        .as_mut()
        .unwrap()
        .source
        .revision += 1;
    owner.poll();
    assert!(owner.session.busy());
    assert!(owner.data.borrow().protocol_invalid);
    assert!(!owner.data.borrow().operation.as_ref().unwrap().terminal);
    owner.stop_all().unwrap();
    assert_eq!(fake.stops.borrow().as_slice(), &[Stop::All]);
}
#[gtk::test]
fn active_opening_finalizing_unknown_and_quit_keep_structural_edits_gated() {
    for phase in [
        RecordingProgress::Queued,
        RecordingProgress::Opening,
        RecordingProgress::StartedLocally,
        RecordingProgress::Finalizing,
        RecordingProgress::Unconfirmed,
    ] {
        let (owner, fake, _, _) = routed();
        owner.start(&owner.origin().unwrap(), None).unwrap();
        fake.takes.borrow()[0]
            .0
            .borrow_mut()
            .as_mut()
            .unwrap()
            .progress = phase;
        owner.poll();
        assert!(edit(
            &owner,
            ui::Action::Remove {
                bus: "rec-1-2".into()
            }
        )
        .is_err());
        assert!(fake.migrations.borrow().is_empty());
        owner.stop_all().unwrap();
    }
    let (owner, fake, _, _) = routed();
    edit(
        &owner,
        ui::Action::Remove {
            bus: "rec-1-2".into(),
        },
    )
    .unwrap();
    assert!(button(owner.widget(), "Stop all").is_sensitive());
    owner.stop_all().unwrap();
    owner.session.close_admission();
    assert!(edit(
        &owner,
        ui::Action::Split {
            bus: "rec-1-2".into()
        }
    )
    .is_err());
    let receipt = fake.migrations.borrow()[0].1.clone();
    let mut report = receipt.report.borrow_mut();
    let r = report.as_mut().unwrap();
    r.progress = MigrationProgress::Completed(MigrationOutcome::Rejected);
    r.failure = Some(MigrationError::Cancelled);
    drop(report);
    assert!(owner.finalization_errors(false).unwrap().is_empty());
}

#[gtk::test]
fn rejected_bus_publication_cannot_release_observed_migration_ownership() {
    let (owner, fake, driver, gens) = routed();
    edit(
        &owner,
        ui::Action::Remove {
            bus: "rec-1-2".into(),
        },
    )
    .unwrap();
    let observed = removed(&owner, &driver, &gens);
    owner.session.poll();
    complete(&fake, 0, MigrationOutcome::Observed, true);
    fake.migrations.borrow()[0]
        .1
        .report
        .borrow_mut()
        .as_mut()
        .unwrap()
        .ports_observed = Some(observed.stamp);
    let source = Source::new(99, &observed, fake.config.borrow().revision);
    for id in [91, 92] {
        fake.takes
            .borrow_mut()
            .push(Rc::new(Take(RefCell::new(Some(TakeStatus {
                source: Source {
                    ticket: id,
                    ..source
                },
                progress: RecordingProgress::JoinedSuccess,
                writer: Some(WriterPhase::Joined),
                writer_outcome: Some(WriterOutcome::Finalized),
                failure: None,
                complete: true,
                take: Some(TakeKey::Synthetic(id)),
                directory: None,
                tracks: vec![ui::Track {
                    identity: TrackKey::Synthetic(900),
                    bus_key: "rec-1-2".into(),
                    phase: ui::TrackPhase::Joined(Ok(())),
                    elapsed: None,
                }],
            })))));
    }
    owner.poll();
    assert!(owner.view.publication_error().is_some());
    assert!(owner.session.busy());
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .tracks
        .clear();
    owner.poll();
    assert!(owner.view.publication_error().is_none());
    assert!(!owner.session.busy());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::Observed
    );
}
