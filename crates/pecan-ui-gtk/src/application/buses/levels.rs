//! One local Rec-level interaction, original authority and shared admission.
//! Native readback is local completion; these edits never enter graph Undo.
use super::*;
use crate::recording_buses::levels::{Capture, Change, Event, Reply};
use pecan_core::{
    graph::Soort,
    history_identity::{HistoryGraph, RegistryBinding},
};

#[derive(Clone)]
pub(super) struct Binding {
    registry: RegistryBinding,
    node: u32,
    serial: Option<u64>,
    inputs: Vec<u32>,
}
impl Binding {
    pub(super) fn capture(frame: &HistoryGraph, config: &ConfigSnapshot) -> Result<Self, String> {
        let local = config
            .local
            .as_ref()
            .ok_or("Rec local readback is unavailable.")?;
        let node = local.node.ok_or("Rec filter is unavailable.")?;
        let graph = &frame.graph().value;
        let owned = graph
            .knopen
            .iter()
            .find(|n| n.id == node && pecan_core::domain::audio::is_bus(n))
            .ok_or("Captured Rec identity is unavailable.")?;
        if graph
            .poorten
            .iter()
            .filter(|p| p.knoop == node && p.ingang && p.soort == Soort::Audio)
            .count()
            != local.channels
        {
            return Err("Wait for the complete Rec input publication.".into());
        }
        let mut inputs = Vec::with_capacity(local.channels);
        for channel in 0..local.channels {
            let name = format!("rec_{}", channel + 1);
            let mut ports = graph.poorten.iter().filter(|p| {
                p.knoop == node && p.ingang && p.soort == Soort::Audio && p.naam == name
            });
            let port = ports.next().ok_or("Captured Rec channel is unavailable.")?;
            if ports.next().is_some() {
                return Err("Rec channel mapping is ambiguous.".into());
            }
            inputs.push(port.id);
        }
        let registry = frame
            .bind_objects(std::iter::once(node).chain(inputs.iter().copied()))
            .ok_or("Rec registry authority is unavailable.")?;
        Ok(Self {
            registry,
            node,
            serial: owned.serial,
            inputs,
        })
    }
    pub(super) fn matches(&self, frame: &HistoryGraph) -> bool {
        if !self.registry.matches(frame) {
            return false;
        }
        let graph = &frame.graph().value;
        graph.knopen.iter().any(|n| {
            n.id == self.node && n.serial == self.serial && pecan_core::domain::audio::is_bus(n)
        }) && graph
            .poorten
            .iter()
            .filter(|p| p.knoop == self.node && p.ingang && p.soort == Soort::Audio)
            .count()
            == self.inputs.len()
            && self.inputs.iter().enumerate().all(|(channel, id)| {
                graph.poorten.iter().any(|p| {
                    p.id == *id
                        && p.knoop == self.node
                        && p.ingang
                        && p.soort == Soort::Audio
                        && p.naam == format!("rec_{}", channel + 1)
                })
            })
    }
}
pub(super) struct Work {
    witness: HistoryGraph,
    binding: Binding,
    key: String,
    baseline: IdleBusConfig,
    desired: Option<Change>,
    sent: Option<(Change, IdleBusConfig)>,
    committed: Option<Change>,
    ended: bool,
    flight: bool,
    dispatching: bool,
    ticket_free: bool,
    retry_on_clock: bool,
    awaiting_inspection: bool,
    applied: bool,
    failed: Option<ui::Progress>,
    last: Option<ConfigReport>,
    pub(super) released: bool,
}
impl BusOwner {
    pub(crate) fn tick_levels(&self, now: std::time::Duration) {
        self.view.tick_levels(now);
    }
    pub(super) fn level_support(&self) -> ui::Support {
        if !self.service.as_ref().is_some_and(|s| s.supports_levels()) {
            return ui::Support::Pending(ui::Dependency::IdleLevels);
        }
        if self.session.admission_closed() {
            return ui::Support::Unavailable("Finalization has started.".into());
        }
        let d = self.data.borrow();
        let owned = d
            .operation
            .as_ref()
            .and_then(|w| w.levels.as_ref())
            .filter(|l| !l.released);
        if d.protocol_invalid || !view::coherent(&d) || self.session.busy() && owned.is_none() {
            return ui::Support::Unavailable(
                "Wait for the shared operation and actual readback.".into(),
            );
        }
        let Some(frame) = self.session.witnessed_graph() else {
            return ui::Support::Unavailable("Native registry authority is unavailable.".into());
        };
        let valid = if let Some(work) = owned {
            work.binding.matches(&frame)
                && d.config
                    .as_ref()
                    .and_then(|c| c.configuration.as_ref())
                    .is_some_and(|c| c.buses.widths == work.baseline.buses.widths)
        } else {
            !d.unknown_owned
                && d.orphans.is_empty()
                && (!d.config.as_ref().is_some_and(|c| c.recording)
                    || d.takes.iter().any(|t| {
                        !t.status.complete && t.status.progress == RecordingProgress::StartedLocally
                    }))
                && !d.takes.iter().any(|t| {
                    !t.status.complete
                        && matches!(
                            t.status.progress,
                            RecordingProgress::Queued
                                | RecordingProgress::Opening
                                | RecordingProgress::Unconfirmed
                                | RecordingProgress::Finalizing
                        )
                })
                && d.config.as_ref().is_some_and(|c| {
                    c.connected && c.configuration.is_some() && Binding::capture(&frame, c).is_ok()
                })
        };
        if valid {
            ui::Support::Available
        } else {
            ui::Support::Unavailable("Wait for known Rec identity and recording ownership.".into())
        }
    }
    pub(super) fn level_event(
        &self,
        event: Event<Origin, Request>,
    ) -> Result<Reply<Request>, String> {
        match event {
            Event::Begin { capture, .. } => self.begin_level(capture, None).map(Reply::Tracked),
            Event::Set(intent) => {
                let proposed = intent
                    .capture
                    .clone()
                    .propose(intent.change.clone())
                    .map_err(|e| format!("{e:?}"))?;
                if proposed.proposed_configuration != intent.proposed_configuration {
                    return Err("The level proposal does not match its captured policy.".into());
                }
                self.begin_level(intent.capture, Some(intent.change))
                    .map(Reply::Tracked)
            }
            Event::Update { request, db } => {
                self.change_level(&request, Change::Db(db), false)?;
                Ok(Reply::Accepted)
            }
            Event::End { request, db, .. } => {
                self.change_level(&request, Change::Db(db), true)?;
                Ok(Reply::Accepted)
            }
            Event::Abandon { request } => {
                let mut d = self.data.borrow_mut();
                let w = d
                    .operation
                    .as_mut()
                    .filter(|w| w.request == request)
                    .ok_or("The level interaction is no longer current.")?;
                w.levels.as_mut().ok_or("Not a level interaction.")?.ended = true;
                drop(d);
                self.publish();
                Ok(Reply::Accepted)
            }
        }
    }
    fn begin_level(
        &self,
        capture: Capture<Origin>,
        change: Option<Change>,
    ) -> Result<Request, String> {
        if self.level_support() != ui::Support::Available
            || self.session.busy()
            || self.view.awaited_request().is_some()
            || self.data.borrow().operation.as_ref().is_some_and(|w| {
                !w.terminal
                    || matches!(
                        w.progress,
                        ui::Progress::PartiallyApplied(_) | ui::Progress::Unconfirmed(_)
                    )
            })
        {
            return Err("Rec levels require a known, available owner.".into());
        }
        if !Rc::ptr_eq(&capture.origin.0.owner, &self.authority) {
            return Err("Foreign level capture.".into());
        }
        let frame = capture
            .origin
            .0
            .witness
            .clone()
            .ok_or("The original native witness is unavailable.")?;
        let graph = capture
            .origin
            .0
            .graph
            .as_ref()
            .ok_or("The original graph is unavailable.")?;
        if frame.graph().stamp != graph.stamp || !Arc::ptr_eq(&frame.graph().value, &graph.value) {
            return Err("The original graph/witness do not match.".into());
        }
        let config = capture
            .origin
            .0
            .config
            .as_ref()
            .ok_or("Captured configuration is unavailable.")?;
        let baseline = config
            .configuration
            .as_ref()
            .ok_or("Logical metadata is unavailable.")?;
        if capture.configuration != baseline.buses
            || !baseline
                .buses
                .definitions()
                .map_err(|e| format!("{e:?}"))?
                .contains(&capture.bus)
        {
            return Err("The captured bus/layout does not match its configuration.".into());
        }
        let binding = Binding::capture(&frame, config)?;
        if !self
            .session
            .witnessed_graph()
            .as_ref()
            .is_some_and(|f| binding.matches(f))
        {
            return Err("The captured Rec identity changed; use fresh input.".into());
        }
        let backend = self.service.as_ref().ok_or("No level endpoint.")?;
        let fresh = backend.read().ok_or("Configuration inspection is busy.")?;
        if fresh.config != *config || fresh.revision.owner != config.revision.owner {
            return Err("Captured configuration changed before admission.".into());
        }
        if self.level_support() != ui::Support::Available
            || !self
                .session
                .witnessed_graph()
                .as_ref()
                .is_some_and(|f| binding.matches(f))
        {
            return Err("Level authority changed during inspection.".into());
        }
        let (ticket, lease) = self.session.reserve_external()?;
        if let Some(change) = &change {
            if let Err(error) = self.record_level_preference(&capture.bus.key, change) {
                self.session.release_external(&lease);
                return Err(error);
            }
        }
        if self.session.admission_closed()
            || !self
                .session
                .witnessed_graph()
                .as_ref()
                .is_some_and(|f| binding.matches(f))
        {
            self.session.release_external(&lease);
            return Err("Original Rec authority changed during preference handoff.".into());
        }
        let request = Request(Rc::new(()));
        {
            let mut d = self.data.borrow_mut();
            Self::remember_arming(&mut d);
            Self::remember_migration(&mut d);
            Self::remember_levels(&mut d);
            d.operation = Some(ConfigWork {
                restore: None,
                request: request.clone(),
                ticket,
                expected: config.revision,
                lease,
                receipt: None,
                arming: None,
                migration: None,
                levels: Some(Work {
                    witness: frame,
                    binding,
                    key: capture.bus.key,
                    baseline: baseline.clone(),
                    desired: change.clone(),
                    sent: None,
                    committed: None,
                    ended: change.is_some(),
                    flight: false,
                    dispatching: false,
                    ticket_free: true,
                    retry_on_clock: false,
                    awaiting_inspection: false,
                    applied: false,
                    failed: None,
                    last: None,
                    released: false,
                }),
                progress: ui::Progress::Queued,
                terminal: false,
                completion: None,
                reconcile_requested: false,
            });
        }
        if change.is_some() {
            self.send_level_sample(&request, false);
        }
        self.publish();
        Ok(request)
    }
    fn record_level_preference(&self, key: &str, change: &Change) -> Result<(), String> {
        use super::super::preferences::RecEdit;
        self.record_preference(match change {
            Change::Db(db) => RecEdit::Db {
                bus: key.into(),
                db: *db,
            },
            Change::Mute(mute) => RecEdit::Mute {
                bus: key.into(),
                mute: *mute,
            },
        })
        .map_err(|e| e.to_string())
    }
    fn change_level(&self, request: &Request, change: Change, ended: bool) -> Result<(), String> {
        let key = {
            let d = self.data.borrow();
            let w = d
                .operation
                .as_ref()
                .filter(|w| &w.request == request && !w.terminal)
                .ok_or("The original level interaction is unavailable.")?;
            let l = w
                .levels
                .as_ref()
                .filter(|l| !l.ended && l.failed.is_none())
                .ok_or("Level input already ended; use fresh input.")?;
            if self.session.admission_closed()
                || !self
                    .session
                    .witnessed_graph()
                    .as_ref()
                    .is_some_and(|f| l.binding.matches(f))
            {
                return Err("Original Rec authority is no longer available for input.".into());
            }
            l.key.clone()
        };
        self.record_level_preference(&key, &change)?;
        let send = {
            let mut d = self.data.borrow_mut();
            let w = d
                .operation
                .as_mut()
                .filter(|w| &w.request == request && !w.terminal)
                .ok_or("The original level interaction is unavailable.")?;
            let l = w.levels.as_mut().ok_or("Not a level interaction.")?;
            if l.ended || l.failed.is_some() {
                return Err("Level input already ended; use fresh input.".into());
            }
            match change {
                Change::Db(db) => {
                    l.baseline
                        .buses
                        .with_db(&l.key, db)
                        .map_err(|e| format!("{e:?}"))?;
                }
                Change::Mute(mute) => {
                    l.baseline
                        .buses
                        .with_mute(&l.key, mute)
                        .map_err(|e| format!("{e:?}"))?;
                }
            }
            l.desired = Some(change);
            l.ended = ended;
            !l.flight && !l.dispatching && !l.retry_on_clock
        };
        if send {
            self.send_level_sample(request, false);
        }
        self.publish();
        Ok(())
    }
    fn send_level_sample(&self, request: &Request, clock: bool) {
        let current = {
            let d = self.data.borrow();
            let Some(w) = d
                .operation
                .as_ref()
                .filter(|w| &w.request == request && !w.terminal)
            else {
                return;
            };
            let Some(l) = w.levels.as_ref().filter(|l| {
                !l.flight
                    && !l.dispatching
                    && !l.awaiting_inspection
                    && l.failed.is_none()
                    && (clock || !l.retry_on_clock)
            }) else {
                return;
            };
            let Some(change) = l
                .desired
                .clone()
                .filter(|c| Some(c) != l.committed.as_ref())
            else {
                return;
            };
            if w.completion
                .as_ref()
                .is_some_and(|c| !d.config.as_ref().is_some_and(|now| same_readback(now, c)))
            {
                return;
            }
            (
                l.witness.clone(),
                l.binding.clone(),
                l.baseline.clone(),
                l.key.clone(),
                change,
                w.lease.clone(),
                l.ticket_free,
            )
        };
        if self.session.admission_closed() {
            return;
        }
        let Some(backend) = self.service.as_ref() else {
            return;
        };
        {
            let mut d = self.data.borrow_mut();
            let Some(w) = d
                .operation
                .as_mut()
                .filter(|w| &w.request == request && !w.terminal)
            else {
                return;
            };
            let l = w.levels.as_mut().unwrap();
            if l.flight || l.dispatching {
                return;
            }
            l.dispatching = true;
        }
        let Some(fresh) = backend.read() else {
            self.clear_level_preflight(request);
            return;
        };
        if self.session.admission_closed() {
            self.clear_level_preflight(request);
            return;
        }
        if !self
            .session
            .witnessed_graph()
            .as_ref()
            .is_some_and(|f| current.1.matches(f))
        {
            self.fail_level(
                request,
                "The original Rec identity changed; no replacement write admitted.",
            );
            return;
        }
        if !fresh.config.connected
            || fresh.revision.owner != fresh.config.revision.owner
            || fresh.config.configuration.as_ref() != Some(&current.2)
        {
            self.fail_level(
                request,
                "Actual configuration changed outside the level interaction.",
            );
            return;
        }
        let mut next = current.2.clone();
        let changed = match current.4 {
            Change::Db(db) => next.buses.with_db(&current.3, db),
            Change::Mute(mute) => next.buses.with_mute(&current.3, mute),
        };
        let Ok(buses) = changed else {
            self.fail_level(request, "Invalid desired level.");
            return;
        };
        next.buses = buses;
        let ticket = if current.6 {
            self.data.borrow().operation.as_ref().unwrap().ticket
        } else {
            let Ok(ticket) = self.session.external_sample_ticket(&current.5) else {
                self.clear_level_preflight(request);
                return;
            };
            ticket
        };
        {
            let mut d = self.data.borrow_mut();
            let Some(w) = d
                .operation
                .as_mut()
                .filter(|w| &w.request == request && !w.terminal)
            else {
                return;
            };
            let l = w.levels.as_mut().unwrap();
            if l.flight || l.failed.is_some() {
                l.dispatching = false;
                return;
            }
            w.ticket = ticket;
            w.expected = fresh.config.revision;
            w.receipt = None;
            l.sent = Some((current.4, next.clone()));
            l.flight = true;
            l.dispatching = true;
            l.ticket_free = true;
            l.retry_on_clock = false;
            w.progress = ui::Progress::Applying;
        }
        let result = backend.configure_levels(
            &current.0,
            IdleConfigIntent {
                ticket,
                expected: fresh.config.revision,
                configuration: next,
            },
        );
        let mut d = self.data.borrow_mut();
        let Some(w) = d
            .operation
            .as_mut()
            .filter(|w| &w.request == request && w.ticket == ticket)
        else {
            return;
        };
        let l = w.levels.as_mut().unwrap();
        l.dispatching = false;
        match result {
            Ok(receipt) => w.receipt = Some(receipt),
            Err(Error::Configuration(
                pecan_pipewire::bus_config::ConfigError::Busy
                | pecan_pipewire::bus_config::ConfigError::QueueFull,
            )) => {
                l.flight = false;
                l.retry_on_clock = true;
                w.progress = ui::Progress::Queued;
            }
            Err(error) => {
                l.flight = false;
                l.ended = true;
                l.awaiting_inspection = true;
                l.failed.get_or_insert_with(|| {
                    if l.applied {
                        ui::Progress::PartiallyApplied(format!("{error}"))
                    } else {
                        ui::Progress::Rejected(format!("{error}"))
                    }
                });
            }
        }
    }
    fn fail_level(&self, request: &Request, message: &str) {
        if let Some(w) = self
            .data
            .borrow_mut()
            .operation
            .as_mut()
            .filter(|w| &w.request == request)
        {
            let l = w.levels.as_mut().unwrap();
            l.ended = true;
            if l.failed.is_none() {
                l.failed = Some(ui::Progress::Unconfirmed(message.into()));
                l.awaiting_inspection = true;
            }
            if !l.flight {
                l.dispatching = false;
            }
        }
    }
    fn clear_level_preflight(&self, request: &Request) {
        if let Some(w) = self
            .data
            .borrow_mut()
            .operation
            .as_mut()
            .filter(|w| &w.request == request)
        {
            let l = w.levels.as_mut().unwrap();
            if !l.flight {
                l.dispatching = false;
            }
        }
    }
    pub(super) fn poll_levels(&self, inspected: Option<&ConfigSnapshot>) {
        // This inspection occurred after any terminal report seen on an older
        // poll. Equal-revision NoOp/rejection still needs this temporal barrier.
        {
            let mut d = self.data.borrow_mut();
            if let Some(w) = d.operation.as_mut().filter(|w| !w.terminal) {
                if let Some(l) = w.levels.as_mut() {
                    if l.awaiting_inspection
                        && !l.flight
                        && !l.dispatching
                        && inspected.is_some_and(|c| c.revision.owner == w.expected.owner)
                    {
                        l.awaiting_inspection = false;
                    }
                }
            }
        }
        let pending = self
            .data
            .borrow()
            .operation
            .as_ref()
            .filter(|w| !w.terminal)
            .and_then(|w| {
                let l = w.levels.as_ref()?;
                (l.flight && !l.dispatching)
                    .then(|| {
                        w.receipt
                            .clone()
                            .map(|r| (w.request.clone(), w.ticket, w.expected, r))
                    })
                    .flatten()
            });
        if let Some((request, ticket, expected, receipt)) = pending {
            if let Some(report) = receipt.report() {
                let mut d = self.data.borrow_mut();
                if let Some(w) = d.operation.as_mut().filter(|w| {
                    w.request == request
                        && w.ticket == ticket
                        && w.receipt.as_ref().is_some_and(|r| Rc::ptr_eq(r, &receipt))
                }) {
                    if report.ticket != ticket || report.expected != expected {
                        d.protocol_invalid = true;
                    } else {
                        let l = w.levels.as_mut().unwrap();
                        match &report.progress {
                            ConfigProgress::Queued | ConfigProgress::Applying => {}
                            ConfigProgress::Unconfirmed => {
                                l.flight = false;
                                l.ended = true;
                                l.awaiting_inspection = true;
                                l.failed.get_or_insert_with(|| {
                                    ui::Progress::Unconfirmed(
                                        "Native level completion is unconfirmed.".into(),
                                    )
                                });
                            }
                            ConfigProgress::Completed(result) => {
                                l.flight = false;
                                l.awaiting_inspection = true;
                                l.ticket_free = false;
                                w.completion = Some(result.snapshot.clone());
                                match &result.outcome {
                                    LocalConfigOutcome::Applied | LocalConfigOutcome::NoOp
                                        if l.sent.as_ref().is_some_and(|(_, sent)| {
                                            result.snapshot.configuration.as_ref() == Some(sent)
                                        }) =>
                                    {
                                        l.applied |= result.outcome == LocalConfigOutcome::Applied;
                                        let (change, config) = l.sent.as_ref().unwrap();
                                        l.committed = Some(change.clone());
                                        l.baseline = config.clone();
                                    }
                                    LocalConfigOutcome::Rejected(error) => {
                                        l.ended = true;
                                        l.failed.get_or_insert_with(|| {
                                            if l.applied {
                                                ui::Progress::PartiallyApplied(format!("{error:?}"))
                                            } else {
                                                ui::Progress::Rejected(format!("{error:?}"))
                                            }
                                        });
                                    }
                                    LocalConfigOutcome::PartiallyApplied(error) => {
                                        l.ended = true;
                                        l.failed.get_or_insert_with(|| {
                                            ui::Progress::PartiallyApplied(format!("{error:?}"))
                                        });
                                    }
                                    _ => {
                                        l.ended = true;
                                        l.failed.get_or_insert_with(|| {
                                            ui::Progress::Unconfirmed(
                                                "Native level readback did not match its proposal."
                                                    .into(),
                                            )
                                        });
                                    }
                                }
                            }
                        }
                        l.last = Some(report);
                    }
                }
            }
        }
        let request = self
            .data
            .borrow()
            .operation
            .as_ref()
            .filter(|w| w.levels.is_some() && !w.terminal)
            .map(|w| w.request.clone());
        let Some(request) = request else { return };
        if self.session.admission_closed() {
            self.fail_level(
                &request,
                "Level input ended during application finalization.",
            );
        }
        let invalid = self
            .data
            .borrow()
            .operation
            .as_ref()
            .and_then(|w| w.levels.as_ref())
            .is_some_and(|l| {
                !self
                    .session
                    .witnessed_graph()
                    .as_ref()
                    .is_some_and(|f| l.binding.matches(f))
            });
        if invalid {
            self.fail_level(
                &request,
                "Original Rec authority was lost; retained outcomes are historical.",
            );
        }
        self.send_level_sample(&request, true);
        let mut d = self.data.borrow_mut();
        let Some(w) = d
            .operation
            .as_mut()
            .filter(|w| w.request == request && !w.terminal)
        else {
            return;
        };
        let l = w.levels.as_mut().unwrap();
        if !l.ended || l.flight || l.dispatching || l.awaiting_inspection {
            return;
        }
        let Some(fresh) = inspected.filter(|c| c.revision.owner == w.expected.owner) else {
            return;
        };
        if l.failed.is_none() {
            if w.completion
                .as_ref()
                .is_some_and(|c| !same_readback(fresh, c))
            {
                let completed = w.completion.as_ref().unwrap();
                if fresh.revision.epoch == completed.revision.epoch
                    && !revision_covers(fresh.revision, completed.revision)
                {
                    return; // Readable, but genuinely older than the completion.
                }
                // A newer incompatible readback cannot satisfy an old success.
                // Retain that report and expose uncertainty instead of waiting
                // forever for a superseded configuration or old epoch.
                l.failed = Some(ui::Progress::Unconfirmed(
                    "Current Rec readback superseded the local level completion.".into(),
                ));
            }
            if l.failed.is_none() && l.desired != l.committed {
                return;
            }
        }
        w.progress = l.failed.clone().unwrap_or_else(|| {
            if l.applied {
                ui::Progress::AppliedLocally
            } else if l.committed.is_some() {
                ui::Progress::NoOpLocally
            } else {
                ui::Progress::Rejected("Input ended before any native level sample.".into())
            }
        });
        w.completion = Some(fresh.clone());
        w.terminal = true;
    }
    pub(super) fn finish_level_publication(&self) -> bool {
        let (request, progress, lease) = {
            let d = self.data.borrow();
            let Some(w) = d
                .operation
                .as_ref()
                .filter(|w| w.terminal && w.levels.as_ref().is_some_and(|l| !l.released))
            else {
                return false;
            };
            (w.request.clone(), w.progress.clone(), w.lease.clone())
        };
        let accepted = self.view.awaited_request().is_none()
            && self
                .view
                .inspect_model(|m| {
                    m.snapshot()
                        .and_then(|s| s.operation.as_ref())
                        .is_some_and(|o| o.request == request && o.progress == progress)
                })
                .unwrap_or(false);
        if !accepted {
            return false;
        }
        {
            let mut d = self.data.borrow_mut();
            let Some(w) = d.operation.as_mut().filter(|w| w.request == request) else {
                return false;
            };
            w.levels.as_mut().unwrap().released = true;
        }
        self.session.release_external(&lease);
        true
    }
    pub(super) fn remember_levels(d: &mut Data) {
        if let Some(l) = d.operation.as_ref().and_then(|w| w.levels.as_ref()) {
            if let Some(report) = &l.last {
                d.level_reports.push_back(report.clone());
                while d.level_reports.len() > 16 {
                    d.level_reports.pop_front();
                }
            }
            if let Some(error) = l.failed.as_ref().filter(|_| d.past_failures.len() < 32) {
                d.past_failures.push(format!(
                    "Rec levels: {error:?}; last native report {:?}",
                    l.last
                ));
            }
        }
    }
    pub(super) fn recover_level_input(&self) {
        let ready = {
            let d = self.data.borrow();
            d.level_recovery
                && !d.protocol_invalid
                && view::coherent(&d)
                && d.config
                    .as_ref()
                    .is_some_and(|c| c.connected && c.configuration.is_some())
                && !self.session.busy()
        };
        if ready && self.view.levels_reconciled().is_ok() {
            self.data.borrow_mut().level_recovery = false;
        }
    }
    pub(super) fn level_finalization_errors(
        &self,
        joined_failed: bool,
        errors: &mut Vec<String>,
    ) -> Option<()> {
        let pending = self
            .data
            .borrow()
            .operation
            .as_ref()
            .filter(|w| w.levels.is_some())
            .map(|w| {
                if let Some(ui::Progress::PartiallyApplied(message)) =
                    &w.levels.as_ref().unwrap().failed
                {
                    errors.push(format!("Rec levels partially applied: {message}"));
                }
                (
                    w.ticket,
                    w.expected,
                    w.receipt.clone(),
                    w.levels.as_ref().unwrap().flight,
                )
            });
        if let Some((ticket, expected, receipt, flight)) = pending {
            if let Some(receipt) = receipt {
                match receipt.report() {
                    None if !joined_failed => return None,
                    Some(r) if r.ticket == ticket && r.expected == expected => match r.progress {
                        ConfigProgress::Completed(ref result)
                            if !matches!(
                                result.outcome,
                                LocalConfigOutcome::PartiallyApplied(_)
                            ) => {}
                        _ => errors.push(
                            "Rec-level outcome remains partial/unconfirmed after backend join."
                                .into(),
                        ),
                    },
                    _ => errors.push(
                        "Rec-level diagnostic identity is unavailable after backend join.".into(),
                    ),
                }
            } else if flight && !joined_failed {
                return None;
            }
        }
        Some(())
    }
}
fn same_readback(now: &ConfigSnapshot, completed: &ConfigSnapshot) -> bool {
    now.revision.owner == completed.revision.owner
        && now.revision.epoch == completed.revision.epoch
        && revision_covers(now.revision, completed.revision)
        && now.connected == completed.connected
        && now.local == completed.local
        && now.configuration == completed.configuration
        && now.transport == completed.transport
}
