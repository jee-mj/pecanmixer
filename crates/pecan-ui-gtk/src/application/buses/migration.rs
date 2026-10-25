//! Owner projection of the existing staged backend receipt, never a sequencer.
use super::*;
use pecan_core::{
    backend::Stamp,
    graph::{Opdracht, Soort},
    history_identity::{HistoryGraph, RegistryBinding},
};
use pecan_pipewire::bus_migration::{
    LayoutAction, LayoutIntent, MigrationOutcome, MigrationProgress, MigrationReport,
};
use service::MigrationPoll;

pub(super) struct Work {
    source: Stamp,
    membership: RecordingRevision,
    proposed: IdleBusConfig,
    proof: Postcondition,
    append: bool,
    // Keep the backend's actual completion witness. A later observation/cache
    // lookup cannot supply the generations that were observed at completion.
    observed_inputs: Option<HistoryGraph>,
    receipt: Option<Rc<dyn MigrationPoll>>,
    last: Option<MigrationReport>,
    publication: Option<ConfigSnapshot>,
    pub(super) released: bool,
}
struct Postcondition {
    binding: RegistryBinding,
    node: u32,
    serial: Option<u64>,
    ports: Vec<(u32, String)>,
    pairs: BTreeSet<(u32, u32)>,
}
impl Postcondition {
    fn matches(
        &self,
        session: &Session,
        routes: bool,
        appended: Option<(&HistoryGraph, usize)>,
    ) -> bool {
        let Some(frame) = session.witnessed_graph() else {
            return false;
        };
        let ports = if let Some((observed, channels)) = appended {
            let graph = &observed.graph().value;
            if !graph.knopen.iter().any(|n| {
                n.id == self.node && n.serial == self.serial && pecan_core::domain::audio::is_bus(n)
            }) {
                return false;
            }
            let actual: Vec<_> = graph
                .poorten
                .iter()
                .filter(|p| p.knoop == self.node && p.ingang && p.soort == Soort::Audio)
                .collect();
            let mut ids = BTreeSet::new();
            let mapped = (1..=channels)
                .map(|i| {
                    let name = format!("rec_{i}");
                    let mut matches = actual.iter().filter(|p| p.naam == name);
                    let port = matches.next()?;
                    (matches.next().is_none() && ids.insert(port.id)).then_some((port.id, name))
                })
                .collect::<Option<Vec<_>>>();
            let Some(ports) = mapped.filter(|_| actual.len() == channels) else {
                return false;
            };
            if !self.ports.iter().all(|p| ports.contains(p))
                || !observed
                    .bind_objects(ids.into_iter().chain(std::iter::once(self.node)))
                    .is_some_and(|binding| binding.matches(&frame))
            {
                return false;
            }
            ports
        } else {
            self.ports.clone()
        };
        let graph = &frame.graph().value;
        self.binding.matches(&frame)
            && graph.knopen.iter().any(|n| {
                n.id == self.node && n.serial == self.serial && pecan_core::domain::audio::is_bus(n)
            })
            && graph
                .poorten
                .iter()
                .filter(|p| p.knoop == self.node && p.ingang && p.soort == Soort::Audio)
                .count()
                == ports.len()
            && ports.iter().all(|(id, name)| {
                graph.poorten.iter().any(|p| {
                    p.id == *id
                        && p.knoop == self.node
                        && p.ingang
                        && p.soort == Soort::Audio
                        && &p.naam == name
                })
            })
            && (!routes
                || graph
                    .kabels
                    .iter()
                    .filter(|l| l.in_knoop == self.node && ports.iter().any(|p| p.0 == l.in_poort))
                    .map(|l| (l.uit_poort, l.in_poort))
                    .collect::<BTreeSet<_>>()
                    == self.pairs)
    }
}
impl Work {
    pub(super) fn closed_for_reconnect(&self, ticket: u64, expected: ConfigRevision) -> bool {
        self.released
            && self.last.as_ref().is_some_and(|r| {
                r.ticket == ticket
                    && r.expected_config == expected
                    && r.source == self.source
                    && r.expected_recording == self.membership
                    && r.in_progress == 0
                    && !r.cancellation_requested
                    && (!self.append
                        || (matches!(
                            r.progress,
                            MigrationProgress::Completed(MigrationOutcome::Observed)
                        ) && self.observed_inputs.is_some()))
                    && matches!(
                        r.progress,
                        MigrationProgress::Completed(
                            MigrationOutcome::AppliedLocally
                                | MigrationOutcome::NoOpLocally
                                | MigrationOutcome::Observed
                        )
                    )
            })
    }
}
impl BusOwner {
    /// Actual retained evidence, including local-only outcomes and failed work.
    pub fn migration_report(&self) -> Option<MigrationReport> {
        let d = self.data.borrow();
        if let Some(m) = d.operation.as_ref().and_then(|w| w.migration.as_ref()) {
            // A busy new request must not be presented as an older outcome.
            return m.last.clone();
        }
        d.migrations.back().cloned()
    }
    pub(super) fn submit_migration(
        &self,
        origin: &Origin,
        action: ui::Action,
        proposed: Option<BusConfig>,
    ) -> Result<Request, Error> {
        let backend = self
            .service
            .as_ref()
            .filter(|s| s.supports_migration())
            .ok_or_else(|| Self::error("Idle migration is unavailable on this endpoint."))?;
        let graph = origin
            .0
            .graph
            .as_ref()
            .ok_or_else(|| Self::error("No captured native graph."))?;
        let config = origin
            .0
            .config
            .as_ref()
            .ok_or_else(|| Self::error("No captured configuration."))?;
        let before = config
            .configuration
            .as_ref()
            .ok_or_else(|| Self::error("Logical metadata is unknown."))?;
        let membership = origin
            .0
            .membership
            .ok_or_else(|| Self::error("Recording membership is unknown."))?;
        let definitions = before
            .buses
            .definitions()
            .map_err(|e| Self::error(&format!("{e:?}")))?;
        let definition = match &action {
            ui::Action::AddStereo => None,
            ui::Action::Split { bus } | ui::Action::Join { bus } | ui::Action::Remove { bus } => {
                Some(
                    definitions
                        .iter()
                        .find(|b| &b.key == bus)
                        .ok_or_else(|| Self::error("Captured bus no longer exists."))?,
                )
            }
            _ => {
                return Err(Self::error(
                    "Only idle Add, split, join and remove are supported by migration.",
                ))
            }
        };
        let node = config
            .local
            .as_ref()
            .and_then(|l| l.node)
            .ok_or_else(|| Self::error("Local Rec filter is unavailable."))?;
        let owned = graph
            .value
            .knopen
            .iter()
            .find(|n| n.id == node && pecan_core::domain::audio::is_bus(n))
            .ok_or_else(|| Self::error("Captured Rec filter identity is unavailable."))?;
        let mut scoped = (*graph.value).clone();
        scoped.knopen.retain(|n| n.id == node);
        scoped
            .poorten
            .retain(|p| p.knoop == node && p.ingang && p.soort == Soort::Audio);
        scoped
            .kabels
            .retain(|l| l.in_knoop == node && scoped.poorten.iter().any(|p| p.id == l.in_poort));
        let (layout, buses, pairs) = match &action {
            ui::Action::AddStereo => {
                let next = before
                    .buses
                    .add()
                    .map_err(|e| Self::error(&format!("{e:?}")))?;
                if proposed.as_ref() != Some(&next) {
                    return Err(Self::error("Proposal does not match captured core Add."));
                }
                (
                    LayoutAction::AddStereo,
                    next,
                    scoped
                        .kabels
                        .iter()
                        .map(|l| (l.uit_poort, l.in_poort))
                        .collect(),
                )
            }
            ui::Action::Split { bus } | ui::Action::Join { bus } => {
                let definition = definition.unwrap();
                let coupling = if matches!(action, ui::Action::Split { .. }) {
                    pecan_core::domain::buses::Coupling::Split
                } else {
                    pecan_core::domain::buses::Coupling::Join
                };
                if definition.coupling != Some(coupling) {
                    return Err(Self::error(
                        "Captured coupling action does not match the bus.",
                    ));
                }
                let next = before
                    .buses
                    .change_coupling(definition.index)
                    .map_err(|e| Self::error(&format!("{e:?}")))?;
                if proposed.as_ref() != Some(&next) {
                    return Err(Self::error("Proposal does not match captured core action."));
                }
                (
                    if matches!(action, ui::Action::Split { .. }) {
                        LayoutAction::Split { bus: bus.clone() }
                    } else {
                        LayoutAction::Join { bus: bus.clone() }
                    },
                    next,
                    scoped
                        .kabels
                        .iter()
                        .map(|l| (l.uit_poort, l.in_poort))
                        .collect(),
                )
            }
            ui::Action::Remove { bus } => {
                let definition = definition.unwrap();
                if proposed.is_some() || definitions.len() == 1 {
                    return Err(Self::error(
                        "Remove needs the owner core plan and cannot remove the last bus.",
                    ));
                }
                let edit = before
                    .buses
                    .remove(definition.index, &scoped)
                    .map_err(|e| Self::error(&format!("{e:?}")))?;
                let mut pairs: BTreeSet<_> = scoped
                    .kabels
                    .iter()
                    .filter(|l| {
                        scoped
                            .poorten
                            .iter()
                            .find(|p| p.id == l.in_poort)
                            .and_then(|p| p.naam.strip_prefix("rec_"))
                            .and_then(|n| n.parse::<usize>().ok())
                            .is_some_and(|n| n <= definition.channels[0])
                    })
                    .map(|l| (l.uit_poort, l.in_poort))
                    .collect();
                pairs.extend(edit.commands.iter().filter_map(|c| {
                    if let Opdracht::Verbind { uit, naar } = c {
                        Some((*uit, *naar))
                    } else {
                        None
                    }
                }));
                (
                    LayoutAction::Remove { bus: bus.clone() },
                    edit.config,
                    pairs,
                )
            }
            _ => unreachable!(),
        };
        let channels: usize = buses.widths.iter().sum();
        let captured_channels = channels.min(before.buses.widths.iter().sum());
        let ports: Vec<_> = (1..=captured_channels)
            .map(|i| {
                let name = format!("rec_{i}");
                let found: Vec<_> = scoped.poorten.iter().filter(|p| p.naam == name).collect();
                (found.len() == 1).then(|| (found[0].id, name))
            })
            .collect::<Option<_>>()
            .ok_or_else(|| {
                Self::error("Captured surviving input map is unavailable or ambiguous.")
            })?;
        let frame = self
            .session
            .witnessed_graph()
            .filter(|f| {
                f.graph().stamp == graph.stamp && Arc::ptr_eq(&f.graph().value, &graph.value)
            })
            .ok_or_else(|| Self::error("Native registry witness is unavailable."))?;
        let mut ids: BTreeSet<_> = ports
            .iter()
            .map(|p| p.0)
            .chain(std::iter::once(node))
            .collect();
        for link in &scoped.kabels {
            let port = graph
                .value
                .poorten
                .iter()
                .find(|p| p.id == link.uit_poort && !p.ingang && p.soort == Soort::Audio)
                .ok_or_else(|| Self::error("Captured source endpoint is unavailable."))?;
            ids.extend([port.id, port.knoop]);
        }
        let binding = frame
            .bind_objects(ids)
            .ok_or_else(|| Self::error("Native migration object generations unavailable."))?;
        let fresh = backend.read().ok_or(Error::Migration(
            pecan_pipewire::bus_migration::MigrationError::Busy,
        ))?;
        if fresh.revision != membership {
            return Err(Error::Migration(
                pecan_pipewire::bus_migration::MigrationError::StaleRecording,
            ));
        }
        if &fresh.config != config {
            return Err(Error::Migration(
                pecan_pipewire::bus_migration::MigrationError::StaleConfig,
            ));
        }
        if fresh.config.recording
            || fresh
                .takes
                .iter()
                .any(|r| r.report().is_none_or(|s| !s.complete))
        {
            return Err(Self::error(
                "Migration requires idle, joined and fully known recording ownership.",
            ));
        }
        self.validate(origin)?;
        self.idle()?;
        let mut expected = before.clone();
        let preference =
            super::super::preferences::RecEdit::configuration(&action, &before.buses, &buses)
                .map_err(Error::Owner)?;
        expected.buses = buses;
        let (ticket, lease) = self.session.reserve_external().map_err(Error::Owner)?;
        if let Err(error) = self.record_preference(preference) {
            self.session.release_external(&lease);
            return Err(error);
        }
        if let Err(error) = self.validate(origin) {
            self.session.release_external(&lease);
            return Err(error);
        }
        let request = Request(Rc::new(()));
        {
            let mut d = self.data.borrow_mut();
            Self::remember_arming(&mut d);
            Self::remember_migration(&mut d);
            Self::remember_levels(&mut d);
            d.operation = Some(ConfigWork {
                restore: None,
                levels: None,
                request: request.clone(),
                ticket,
                expected: config.revision,
                lease: lease.clone(),
                receipt: None,
                arming: None,
                migration: Some(Work {
                    append: matches!(action, ui::Action::AddStereo),
                    observed_inputs: None,
                    source: graph.stamp,
                    membership,
                    proposed: expected,
                    proof: Postcondition {
                        binding,
                        node,
                        serial: owned.serial,
                        ports,
                        pairs,
                    },
                    receipt: None,
                    last: None,
                    publication: None,
                    released: false,
                }),
                progress: ui::Progress::Queued,
                terminal: false,
                completion: None,
                reconcile_requested: false,
            });
        }
        match backend.migrate_idle(LayoutIntent {
            ticket,
            origin: graph.clone(),
            expected_config: config.revision,
            expected_recording: membership,
            action: layout,
        }) {
            Ok(receipt) => {
                self.data
                    .borrow_mut()
                    .operation
                    .as_mut()
                    .unwrap()
                    .migration
                    .as_mut()
                    .unwrap()
                    .receipt = Some(receipt);
                Ok(request)
            }
            Err(error) => {
                self.session.release_external(&lease);
                self.data.borrow_mut().operation = None;
                Err(error)
            }
        }
    }
    pub(super) fn poll_migration(&self, inspected: Option<&ConfigSnapshot>) {
        let pending = self.data.borrow().operation.as_ref().and_then(|w| {
            w.migration
                .as_ref()?
                .receipt
                .clone()
                .map(|r| (w.request.clone(), r))
        });
        let Some((request, report)) = pending.and_then(|(q, r)| r.report().map(|r| (q, r))) else {
            return;
        };
        let graph = self.session.graph();
        let mut d = self.data.borrow_mut();
        let actual = d.config.clone();
        let Some(work) = d.operation.as_mut().filter(|w| w.request == request) else {
            return;
        };
        let m = work.migration.as_mut().unwrap();
        if report.ticket != work.ticket
            || report.source != m.source
            || report.expected_config != work.expected
            || report.expected_recording != m.membership
            || report.proposed != m.proposed
            || report.configuration.revision.owner != work.expected.owner
        {
            work.progress = ui::Progress::Unconfirmed(
                "Migration receipt correlation mismatch; original receipt/lease retained.".into(),
            );
            d.protocol_invalid = true;
            return;
        }
        // Completed evidence stays historical; a later busy inspection must not
        // turn an already displayed/released result back into pending work.
        if m.released {
            return;
        }
        if m.append
            && m.observed_inputs.is_none()
            && matches!(
                report.progress,
                MigrationProgress::Completed(MigrationOutcome::Observed)
            )
        {
            m.observed_inputs = report.observed_inputs.clone();
        }
        let decided = report.is_decided() && report.in_progress == 0;
        let current = inspected.filter(|c| {
            c.revision.owner == work.expected.owner
                && revision_covers(c.revision, report.configuration.revision)
        });
        let fresh = current.is_some_and(|c| Some(c) == actual.as_ref());
        // No backend report is substituted for the owner's actual readback.
        work.completion = report.is_decided().then(|| report.configuration.clone());
        work.terminal = decided && fresh;
        let detail = report.failure.as_ref().map_or_else(
            || "Inspect actual migration/readback before retry.".into(),
            |e| format!("{e:?}"),
        );
        work.progress = match report.progress {
            // A backend terminal report is not yet an owner terminal
            // publication. Keep the widget's request until fresh readback and
            // zero in-progress evidence satisfy the same owner barrier.
            _ if report.is_decided() && !work.terminal => ui::Progress::AwaitingObservation,
            MigrationProgress::Queued => ui::Progress::Queued,
            MigrationProgress::AwaitingNew
            | MigrationProgress::AwaitingOld
            | MigrationProgress::AwaitingPorts => ui::Progress::AwaitingObservation,
            MigrationProgress::Completed(MigrationOutcome::Rejected) => {
                ui::Progress::Rejected(detail)
            }
            MigrationProgress::Completed(MigrationOutcome::PartiallyApplied) => {
                ui::Progress::PartiallyApplied(detail)
            }
            MigrationProgress::Completed(MigrationOutcome::Unconfirmed)
            | MigrationProgress::Unconfirmed => ui::Progress::Unconfirmed(detail),
            MigrationProgress::Completed(outcome) => {
                let covers = graph.as_ref().is_some_and(|g| {
                    g.value.verbonden
                        && g.stamp.epoch == m.source.epoch
                        && report.ports_observed.is_none_or(|s| {
                            g.stamp.epoch == s.epoch && g.stamp.revision >= s.revision
                        })
                });
                let observed = outcome == MigrationOutcome::Observed;
                let proof = if m.append {
                    observed
                        && m.observed_inputs.as_ref().is_some_and(|inputs| {
                            Some(inputs.graph().stamp) == report.ports_observed
                                && m.proof.matches(
                                    &self.session,
                                    true,
                                    Some((inputs, m.proposed.buses.widths.iter().sum())),
                                )
                        })
                } else {
                    m.proof.matches(&self.session, observed, None)
                };
                let exact = current.is_some_and(|c| {
                    c == &report.configuration && c.configuration.as_ref() == Some(&m.proposed)
                });
                let lost_authority = graph
                    .as_ref()
                    .is_some_and(|g| !g.value.verbonden || g.stamp.epoch != m.source.epoch);
                if lost_authority {
                    // The old observation cannot arrive in a new epoch. Keep
                    // its report intact and expose current uncertainty so a
                    // fresh explicit inspection can reconcile without replay.
                    ui::Progress::Unconfirmed("Original migration completed, but its native epoch/connection was lost before owner publication. Inspect current state before retry; the original report is retained.".into())
                } else if !covers {
                    work.terminal = false;
                    ui::Progress::AwaitingObservation
                } else if !exact || !proof || (observed && report.ports_observed.is_none()) {
                    ui::Progress::Unconfirmed("Original migration completed, but current authenticated layout/identity no longer matches its result; inspect before retry. The original report is retained.".into())
                } else {
                    match outcome {
                        MigrationOutcome::Observed => ui::Progress::Observed,
                        MigrationOutcome::NoOpLocally => ui::Progress::NoOpLocally,
                        _ => ui::Progress::AppliedLocally,
                    }
                }
            }
            _ => ui::Progress::Applying,
        };
        m.publication = work.terminal.then(|| current.unwrap().clone());
        m.last = Some(report);
    }
    pub(super) fn finish_migration_publication(&self) -> bool {
        let pending = {
            let d = self.data.borrow();
            d.operation.as_ref().and_then(|w| {
                let m = w.migration.as_ref()?;
                if !w.terminal || m.released {
                    return None;
                }
                Some((
                    w.request.clone(),
                    w.progress.clone(),
                    w.lease.clone(),
                    m.publication.clone()?,
                    d.current.clone()?,
                ))
            })
        };
        let Some((request, progress, lease, config, origin)) = pending else {
            return false;
        };
        let graph = self.session.graph();
        let shown =
            self.view
                .inspect_model(|model| {
                    model.snapshot().is_some_and(|s| {
                        Rc::ptr_eq(&s.origin.0, &origin.0)
                            && s.origin.0.config.as_ref() == Some(&config)
                            && s.operation
                                .as_ref()
                                .is_some_and(|o| o.request == request && o.progress == progress)
                            && s.origin.0.graph.as_ref().zip(graph.as_ref()).is_some_and(
                                |(a, b)| a.stamp == b.stamp && Arc::ptr_eq(&a.value, &b.value),
                            )
                            && (!matches!(progress, ui::Progress::Observed)
                                || s.configuration.as_ref()
                                    == config.configuration.as_ref().map(|c| &c.buses))
                    })
                })
                .unwrap_or(false);
        if !shown {
            return false;
        }
        {
            let mut d = self.data.borrow_mut();
            let Some(w) = d
                .operation
                .as_mut()
                .filter(|w| w.request == request && w.terminal && w.progress == progress)
            else {
                return false;
            };
            w.migration.as_mut().unwrap().released = true;
        }
        self.session.release_external(&lease);
        true
    }
    pub(super) fn remember_migration(d: &mut Data) {
        if let Some(w) = d.operation.as_ref().filter(|w| w.migration.is_some()) {
            if let Some(r) = w.migration.as_ref().unwrap().last.clone() {
                if matches!(
                    w.progress,
                    ui::Progress::PartiallyApplied(_) | ui::Progress::Unconfirmed(_)
                ) && d.past_failures.len() < 32
                {
                    d.past_failures.push(format!(
                        "Migration request {} retained {:?}; {:?}; failure {:?}.",
                        r.ticket, w.progress, r.progress, r.failure
                    ));
                }
                d.migrations.push_back(r);
                if d.migrations.len() > 16 {
                    d.migrations.pop_front();
                }
            }
        }
    }
    pub(super) fn migration_finalization_errors(
        &self,
        joined_failed: bool,
        errors: &mut Vec<String>,
    ) -> Option<()> {
        let pending = self.data.borrow().operation.as_ref().and_then(|w| {
            w.migration.as_ref()?.receipt.clone().map(|r| {
                (
                    w.ticket,
                    w.expected,
                    w.migration.as_ref().unwrap().source,
                    w.migration.as_ref().unwrap().membership,
                    matches!(
                        w.progress,
                        ui::Progress::PartiallyApplied(_) | ui::Progress::Unconfirmed(_)
                    ),
                    r,
                )
            })
        });
        if let Some((ticket, expected, source, membership, uncertain, receipt)) = pending {
            match receipt.report() {
                None if !joined_failed => return None,
                Some(r)
                    if r.ticket == ticket
                        && r.expected_config == expected
                        && r.source == source
                        && r.expected_recording == membership =>
                {
                    if uncertain
                        || !r.is_decided()
                        || r.in_progress != 0
                        || matches!(
                            r.progress,
                            MigrationProgress::Completed(
                                MigrationOutcome::PartiallyApplied | MigrationOutcome::Unconfirmed
                            ) | MigrationProgress::Unconfirmed
                        )
                    {
                        errors.push(format!("Migration request {ticket} remains {:?}; in progress {}; failure {:?}.",r.progress,r.in_progress,r.failure));
                    }
                }
                _ => errors
                    .push("Migration diagnostic identity unavailable after backend join.".into()),
            }
        }
        Some(())
    }
}
