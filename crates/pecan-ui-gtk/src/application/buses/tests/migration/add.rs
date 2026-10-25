//! Synthetic append receipts and publications, never a PipeWire constructor.
use super::*;
use pecan_core::history_identity::HistoryGraph;

fn append_inputs(
    owner: &BusOwner,
    driver: &FixtureDriver,
    generations: &BTreeMap<u32, u64>,
) -> (Graph, HistoryGraph, BTreeMap<u32, u64>) {
    let mut graph = (*owner.session.graph().unwrap().value).clone();
    let template = graph
        .poorten
        .iter()
        .find(|p| p.knoop == 30)
        .unwrap()
        .clone();
    let mut generations = generations.clone();
    for channel in 5..=6 {
        let mut p = template.clone();
        p.id = 300 + channel;
        p.naam = format!("rec_{channel}");
        p.volgnummer = Some(channel - 1);
        generations.insert(p.id, u64::from(p.id) + 10_000);
        graph.poorten.push(p);
    }
    graph.knopen.iter_mut().find(|n| n.id == 30).unwrap().volume = Some(vec![1.; 6]);
    // The real backend completion witness contains only the owned node/final
    // inputs. It deliberately cannot authorize the original source endpoints.
    let owned = generations
        .iter()
        .filter(|(id, _)| {
            **id == 30
                || graph
                    .poorten
                    .iter()
                    .any(|p| p.id == **id && p.knoop == 30 && p.ingang && p.soort == Soort::Audio)
        })
        .map(|(id, generation)| (*id, *generation))
        .collect();
    driver.publish(graph.clone(), owned);
    owner.session.poll();
    let proof = owner.session.witnessed_graph().unwrap();
    let later = driver.publish(graph, generations.clone());
    (later, proof, generations)
}
fn observed(fake: &Fake, proof: HistoryGraph) -> MigrationReport {
    let mut report = complete(fake, 0, MigrationOutcome::Observed, true);
    report.ports_observed = Some(proof.graph().stamp);
    report.observed_inputs = Some(proof);
    set_report(&fake.migrations.borrow()[0].1, report.clone());
    report
}
fn progress(owner: &BusOwner) -> ui::Progress {
    owner
        .data
        .borrow()
        .operation
        .as_ref()
        .unwrap()
        .progress
        .clone()
}

#[gtk::test]
fn routed_add_widget_uses_original_intent_and_retains_barriers_until_display() {
    let (owner, fake, driver, gens) = routed();
    let origin = owner.origin().unwrap();
    assert!(button(owner.widget(), "Add stereo bus").is_sensitive());
    button(owner.widget(), "Add stereo bus").emit_clicked();
    let (intent, receipt) = fake.migrations.borrow()[0].clone();
    assert_eq!(intent.action, LayoutAction::AddStereo);
    assert!(Arc::ptr_eq(
        &intent.origin.value,
        &origin.0.graph.as_ref().unwrap().value
    ));
    assert_eq!(Some(intent.expected_recording), origin.0.membership);
    assert_eq!(
        intent.expected_config,
        origin.0.config.as_ref().unwrap().revision
    );
    assert!(fake.configs.borrow().is_empty());
    assert!(owner.session.busy());
    assert!(!button(owner.widget(), "Add stereo bus").is_sensitive());
    let (_, proof, _) = append_inputs(&owner, &driver, &gens);
    let mut report = observed(&fake, proof);
    report.in_progress = 1;
    set_report(&receipt, report.clone());
    owner.session.poll();
    owner.poll();
    assert_eq!(progress(&owner), ui::Progress::AwaitingObservation);
    assert!(owner.session.busy());
    report.in_progress = 0;
    set_report(&receipt, report);
    fake.busy.set(true);
    owner.poll();
    assert!(owner.session.busy());
    assert!(owner.view.awaited_request().is_some());
    fake.busy.set(false);
    // Reuse the existing rejected-publication fixture instead of holding a
    // read-only model inspection borrow across owner publication.
    let source = Source::new(
        99,
        &owner.session.graph().unwrap(),
        fake.config.borrow().revision,
    );
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
                elapsed_at: None,
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
    assert!(
        owner.session.busy(),
        "model publication has not been accepted"
    );
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .tracks
        .clear();
    owner.poll();
    assert!(owner.view.publication_error().is_none());
    assert_eq!(progress(&owner), ui::Progress::Observed);
    assert!(!owner.session.busy());
    assert!(owner.view.awaited_request().is_none());
    assert_eq!(
        owner
            .view
            .inspect_model(|m| m
                .snapshot()
                .unwrap()
                .configuration
                .as_ref()
                .unwrap()
                .widths
                .clone())
            .unwrap(),
        vec![2, 2, 2]
    );
    assert_eq!(fake.migrations.borrow().len(), 1);
    assert!(
        driver.take_requests().is_empty(),
        "append creates no graph-history or route commands"
    );
}

#[gtk::test]
fn routed_add_retains_actual_input_witness_across_cache_eviction_and_recreated_links() {
    let (owner, fake, driver, gens) = routed();
    edit(&owner, ui::Action::AddStereo).unwrap();
    let (latest, proof, gens) = append_inputs(&owner, &driver, &gens);
    let old = proof.graph().clone();
    let mut graph = (*latest.value).clone();
    for cable in &mut graph.kabels {
        cable.id += 10_000;
    }
    for _ in 0..100 {
        driver.publish(graph.clone(), gens.clone());
    }
    assert!(
        driver.endpoint().witness(&old).is_err(),
        "the reported observation frame was evicted"
    );
    observed(&fake, proof);
    owner.session.poll();
    owner.poll();
    assert_eq!(progress(&owner), ui::Progress::Observed);
    assert!(!owner.session.busy());
    assert_eq!(
        owner.migration_report().unwrap().ports_observed,
        Some(old.stamp)
    );
    assert_eq!(fake.migrations.borrow().len(), 1);
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn routed_add_rejects_replacement_foreign_witness_and_unexpected_suffix_routes() {
    for case in 0..8 {
        let (owner, fake, driver, gens) = routed();
        edit(&owner, ui::Action::AddStereo).unwrap();
        let (latest, proof, mut gens) = append_inputs(&owner, &driver, &gens);
        let mut report = observed(&fake, proof.clone());
        let mut graph = (*latest.value).clone();
        match case {
            0 => *gens.get_mut(&305).unwrap() += 1,
            1 => *gens.get_mut(&301).unwrap() += 1,
            2 => *gens.get_mut(&201).unwrap() += 1,
            3 => *gens.get_mut(&30).unwrap() += 1,
            4 => {
                let mut link = graph.kabels[0].clone();
                link.id = 5000;
                link.in_poort = 305;
                graph.kabels.push(link);
            }
            5 => {
                graph.poorten.retain(|p| p.id != 306);
                gens.remove(&306);
            }
            6 => {
                report.observed_inputs = Some(HistoryGraph::from_registry(
                    proof.graph().clone(),
                    Arc::new(()),
                    gens.clone(),
                ));
            }
            _ => {
                report.ports_observed.as_mut().unwrap().revision += 1;
            }
        }
        set_report(&fake.migrations.borrow()[0].1, report);
        driver.publish(graph, gens);
        owner.session.poll();
        owner.poll();
        assert!(
            matches!(progress(&owner), ui::Progress::Unconfirmed(_)),
            "case {case}: {:?}",
            progress(&owner)
        );
        assert!(!button(owner.widget(), "Add stereo bus").is_sensitive());
        assert_eq!(fake.migrations.borrow().len(), 1);
        assert!(driver.take_requests().is_empty());
    }
}

#[gtk::test]
fn routed_add_never_rebinds_first_observation_to_later_suffix_generation() {
    let (owner, fake, driver, gens) = routed();
    edit(&owner, ui::Action::AddStereo).unwrap();
    let (latest, first, mut gens) = append_inputs(&owner, &driver, &gens);
    observed(&fake, first);
    owner.session.poll();
    fake.busy.set(true);
    owner.poll();
    assert!(owner.session.busy());
    *gens.get_mut(&305).unwrap() += 1;
    driver.publish((*latest.value).clone(), gens);
    owner.session.poll();
    let rebound = owner.session.witnessed_graph().unwrap();
    let mut report = fake.migrations.borrow()[0]
        .1
        .report
        .borrow()
        .clone()
        .unwrap();
    report.ports_observed = Some(rebound.graph().stamp);
    report.observed_inputs = Some(rebound);
    set_report(&fake.migrations.borrow()[0].1, report);
    fake.busy.set(false);
    owner.poll();
    assert!(matches!(progress(&owner), ui::Progress::Unconfirmed(_)));
    assert_eq!(fake.migrations.borrow().len(), 1);
}

#[gtk::test]
fn routed_add_requires_observed_proof_and_never_upgrades_partial_or_cancelled_outcome() {
    for outcome in [
        MigrationOutcome::AppliedLocally,
        MigrationOutcome::NoOpLocally,
        MigrationOutcome::Observed,
        MigrationOutcome::PartiallyApplied,
    ] {
        let (owner, fake, driver, gens) = routed();
        edit(&owner, ui::Action::AddStereo).unwrap();
        let (latest, _, gens) = append_inputs(&owner, &driver, &gens);
        let mut report = complete(&fake, 0, outcome.clone(), true);
        report.ports_observed = Some(latest.stamp);
        if outcome == MigrationOutcome::PartiallyApplied {
            report.cancellation_requested = true;
        }
        set_report(&fake.migrations.borrow()[0].1, report);
        owner.session.poll();
        owner.poll();
        assert!(matches!(
            progress(&owner),
            ui::Progress::Unconfirmed(_) | ui::Progress::PartiallyApplied(_)
        ));
        for _ in 0..3 {
            driver.publish((*latest.value).clone(), gens.clone());
            owner.session.poll();
            owner.poll();
        }
        assert!(matches!(
            progress(&owner),
            ui::Progress::Unconfirmed(_) | ui::Progress::PartiallyApplied(_)
        ));
        assert_eq!(
            owner.migration_report().unwrap().progress,
            MigrationProgress::Completed(outcome)
        );
        assert_eq!(fake.migrations.borrow().len(), 1);
        assert!(driver.take_requests().is_empty());
    }
}

#[gtk::test]
fn routed_add_keeps_stale_proposal_recording_and_shared_history_gates() {
    let (owner, fake, driver, gens) = routed();
    let origin = owner.origin().unwrap();
    assert!(owner
        .configure(&origin, ui::Action::AddStereo, None)
        .is_err());
    assert!(fake.migrations.borrow().is_empty());
    let proposed = origin
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
    fake.config.borrow_mut().recording = true;
    owner.poll();
    let active = owner.origin().unwrap();
    assert!(!button(owner.widget(), "Add stereo bus").is_sensitive());
    assert!(owner
        .configure(&active, ui::Action::AddStereo, Some(proposed.clone()))
        .is_err());
    fake.config.borrow_mut().recording = false;
    owner.poll();
    let fresh = owner.origin().unwrap();
    let (_, lease) = owner.session.reserve_external().unwrap();
    assert!(edit(&owner, ui::Action::AddStereo).is_err());
    owner.session.release_external(&lease);
    driver.publish((*owner.session.graph().unwrap().value).clone(), gens);
    owner.session.poll();
    owner.poll();
    assert!(owner
        .configure(&fresh, ui::Action::AddStereo, Some(proposed))
        .is_err());
    assert!(fake.migrations.borrow().is_empty());
}
