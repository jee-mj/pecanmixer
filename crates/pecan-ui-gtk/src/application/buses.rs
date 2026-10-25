//! One owner for bus/configuration handoffs and retained recording receipts.
//! Local readback is not graph observation; writer exit is not joined completion.
mod arming;
pub(super) mod folder;
mod routing;
mod service;
mod view;
use super::{Graph, Session};
use crate::recording_buses as ui;
use gtk::prelude::*;
use pecan_core::domain::{buses::BusConfig, mixer::Mixer};
use pecan_pipewire::{
    bus_config::{
        ConfigProgress, ConfigReport, ConfigRevision, ConfigSnapshot, IdleBusConfig,
        IdleConfigIntent, LocalConfigOutcome,
    },
    recording::{RecordingBusStart, RecordingProgress, RecordingRevision, RecordingStart},
};
pub(crate) use routing::RoutingKey;
use service::{same_take, ConfigPoll, Readback, Service, Started, Stop, TakePoll};
pub use service::{Error, Source, TakeKey, TakeStatus, TrackKey};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    rc::{Rc, Weak},
    sync::Arc,
};

#[derive(Clone)]
pub struct Origin(Rc<Context>);
struct Context {
    owner: Rc<()>,
    graph: Option<Graph>,
    config: Option<ConfigSnapshot>,
    membership: Option<RecordingRevision>,
    root: Option<PathBuf>,
    connected: bool,
    tracks: BTreeMap<String, Vec<TrackKey>>,
}
#[derive(Clone, Debug)]
pub struct Request(Rc<()>);
impl PartialEq for Request {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Request {}
struct ConfigWork {
    arming: Option<arming::Work>,
    request: Request,
    ticket: u64,
    expected: ConfigRevision,
    lease: Rc<()>,
    receipt: Option<Rc<dyn ConfigPoll>>,
    progress: ui::Progress,
    terminal: bool,
    completion: Option<ConfigSnapshot>,
    reconcile_requested: bool,
}
struct Retained {
    receipt: Rc<dyn TakePoll>,
    status: TakeStatus,
}
#[derive(Clone, PartialEq)]
struct Publication {
    graph: Option<(
        pecan_core::backend::Stamp,
        *const pecan_core::graph::Momentopname,
    )>,
    config: Option<ConfigSnapshot>,
    membership: Option<RecordingRevision>,
    root: Option<PathBuf>,
    operation: Option<(Request, ui::Progress)>,
    takes: Vec<TakeStatus>,
    closing: bool,
    activity: ui::Activity,
    error: Option<String>,
    busy: bool,
    invalid: bool,
}
struct Data {
    config: Option<ConfigSnapshot>,
    membership: Option<RecordingRevision>,
    latest_started: Option<(Rc<dyn TakePoll>, Option<TakeStatus>)>,
    folder_origin: Option<folder::FolderOrigin>,
    folder_fresh: bool,
    unknown_owned: bool,
    protocol_invalid: bool,
    current: Option<Origin>,
    publication: Option<Publication>,
    operation: Option<ConfigWork>,
    takes: Vec<Retained>,
    orphans: Vec<(Option<Source>, Rc<dyn TakePoll>)>,
    all_stopped: bool,
    stopped: BTreeSet<TrackKey>,
    stopped_takes: BTreeSet<TakeKey>,
    root: Option<PathBuf>,
    error: Option<String>,
    retired: VecDeque<Rc<dyn TakePoll>>,
    past_failures: Vec<String>,
}
pub struct BusOwner {
    pub(super) session: Session,
    service: Option<Rc<dyn Service>>,
    data: RefCell<Data>,
    polling: Cell<bool>,
    root: gtk::Box,
    view: ui::BusControls<Origin, Request, TrackKey>,
    tools: gtk::Box,
    status: gtk::Label,
    input: Rc<super::history_input::HistoryInput>,
    controls: RefCell<Vec<Rc<Cell<bool>>>>,
    weak: Weak<Self>,
    authority: Rc<()>,
}
impl BusOwner {
    pub(crate) fn native(
        session: Session,
        owner: Option<Rc<pecan_pipewire::graaf::Graaf>>,
    ) -> Rc<Self> {
        Self::new(
            session,
            owner.map(|o| Rc::new(service::Native(o)) as Rc<dyn Service>),
        )
    }
    fn new(session: Session, service: Option<Rc<dyn Service>>) -> Rc<Self> {
        let view = ui::BusControls::new(ui::Budget {
            buses: 64,
            tracks: 80 * 64,
        });
        if service.is_some() {
            view.recording_owner_attached();
        }
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let status = gtk::Label::builder()
            .wrap(true)
            .xalign(0.)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let tools = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.append(&status);
        let transport_scroll = gtk::ScrolledWindow::builder()
            .child(&tools)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(70)
            .max_content_height(220)
            .propagate_natural_height(true)
            .build();
        root.append(&transport_scroll);
        root.append(view.widget());
        let input = super::history_input::HistoryInput::new();
        input.observe(&root);
        let owner = Rc::new_cyclic(|weak| Self {
            session,
            service,
            data: RefCell::new(Data {
                config: None,
                membership: None,
                latest_started: None,
                folder_origin: None,
                folder_fresh: false,
                unknown_owned: false,
                protocol_invalid: false,
                current: None,
                publication: None,
                operation: None,
                takes: vec![],
                orphans: vec![],
                all_stopped: false,
                stopped: BTreeSet::new(),
                stopped_takes: BTreeSet::new(),
                root: None,
                error: None,
                retired: VecDeque::new(),
                past_failures: vec![],
            }),
            polling: Cell::new(false),
            root,
            view,
            tools,
            status,
            input,
            controls: RefCell::default(),
            weak: weak.clone(),
            authority: Rc::new(()),
        });
        let weak = Rc::downgrade(&owner);
        owner.view.bind_intents(move |event| {
            let owner = weak.upgrade().ok_or("Bus owner is no longer available.")?;
            match event {
                ui::Event::Configuration(intent) => owner
                    .configure(&intent.origin, intent.action, intent.proposed_configuration)
                    .map(ui::Handoff::Tracked)
                    .map_err(|e| e.to_string()),
                ui::Event::Selection { origin, .. } => owner
                    .validate_selection(&origin)
                    .map(|_| ui::Handoff::UiOnly)
                    .map_err(|e| e.to_string()),
                ui::Event::Refresh { .. } => {
                    owner.refresh();
                    Ok(ui::Handoff::UiOnly)
                }
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.input.notify(move || {
            if let Some(o) = weak.upgrade() {
                o.render_controls();
            }
        });
        owner.poll();
        owner
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
    pub fn origin(&self) -> Option<Origin> {
        self.data.borrow().current.clone()
    }
    pub fn configuration_report(&self) -> Option<ConfigReport> {
        let receipt = self
            .data
            .borrow()
            .operation
            .as_ref()
            .and_then(|o| o.receipt.clone());
        receipt.and_then(|r| r.report())
    }
    pub fn recording_status(&self) -> Vec<TakeStatus> {
        self.data
            .borrow()
            .takes
            .iter()
            .map(|t| t.status.clone())
            .collect()
    }
    pub fn set_recording_root(&self, root: Option<PathBuf>) -> Result<(), Error> {
        if self.session.admission_closed() {
            return Err(Self::error("Finalization has started."));
        }
        if root.as_ref().is_some_and(|p| !p.is_absolute()) {
            return Err(Self::error(
                "Recording location must be an explicit absolute directory.",
            ));
        }
        self.data.borrow_mut().root = root;
        self.publish();
        Ok(())
    }
    fn error(message: &str) -> Error {
        Error::Owner(message.into())
    }
    fn validate_selection(&self, origin: &Origin) -> Result<(), Error> {
        if self.session.admission_closed() {
            return Err(Self::error("Finalization has started."));
        }
        let d = self.data.borrow();
        if !d
            .current
            .as_ref()
            .is_some_and(|o| Rc::ptr_eq(&o.0, &origin.0))
        {
            return Err(Self::error(
                "The bus presentation changed; start a fresh action.",
            ));
        }
        if !self
            .session
            .graph()
            .as_ref()
            .zip(origin.0.graph.as_ref())
            .is_some_and(|(a, b)| {
                a.value.verbonden && a.stamp == b.stamp && Arc::ptr_eq(&a.value, &b.value)
            })
        {
            return Err(Self::error("The native graph presentation is unavailable."));
        }
        Ok(())
    }
    fn validate(&self, origin: &Origin) -> Result<(), Error> {
        self.validate_selection(origin)?;
        let d = self.data.borrow();
        if d.protocol_invalid {
            return Err(Self::error("Recording/configuration identity is invalid; retain work and restart the owner after finalization."));
        }
        if !view::coherent(&d) {
            return Err(Self::error(
                "Wait for fresh authenticated configuration readback.",
            ));
        }
        if !origin.0.connected {
            return Err(Self::error(
                "Native configuration authority is unavailable; the graph may still be connected.",
            ));
        }
        Ok(())
    }
    fn idle(&self) -> Result<(), Error> {
        let d = self.data.borrow();
        if self.session.busy()
            || !d.orphans.is_empty()
            || d.unknown_owned
            || d.takes.iter().any(|t| !t.status.complete)
            || d.config.as_ref().is_none_or(|s| s.recording)
        {
            return Err(Self::error(
                "Bus edits require an idle, reconciled owner; active edits remain pending.",
            ));
        }
        if d.operation.as_ref().is_some_and(|o| {
            !o.terminal
                || matches!(
                    o.progress,
                    ui::Progress::PartiallyApplied(_) | ui::Progress::Unconfirmed(_)
                )
        }) || self.view.awaited_request().is_some()
        {
            return Err(Self::error(
                "Retain/reconcile the current configuration result first.",
            ));
        }
        Ok(())
    }
    pub fn configure(
        &self,
        origin: &Origin,
        action: ui::Action,
        proposed: Option<BusConfig>,
    ) -> Result<Request, Error> {
        self.validate(origin)?;
        let current = origin
            .0
            .config
            .as_ref()
            .and_then(|s| s.configuration.as_ref())
            .ok_or_else(|| {
                Self::error("Logical bus metadata is unknown; explicitly initialize it first.")
            })?;
        let buses = &current.buses;
        let expected = match action {
            ui::Action::AddStereo => buses.add(),
            ui::Action::Split { ref bus } | ui::Action::Join { ref bus } => {
                let definition = buses
                    .definitions()
                    .map_err(|e| Self::error(&format!("{e:?}")))?
                    .into_iter()
                    .find(|b| &b.key == bus)
                    .ok_or_else(|| Self::error("Bus no longer exists."))?;
                let required = if matches!(action, ui::Action::Split { .. }) {
                    pecan_core::domain::buses::Coupling::Split
                } else {
                    pecan_core::domain::buses::Coupling::Join
                };
                if definition.coupling != Some(required) {
                    return Err(Self::error(
                        "The captured coupling action does not match this bus.",
                    ));
                }
                buses.change_coupling(definition.index)
            }
            ui::Action::Arm { ref bus, additive } => buses.arm(bus, additive),
            ui::Action::Remove { .. } => {
                return Err(Self::error(
                    "Channel shrink/routed migration remains a pending capability.",
                ))
            }
        }
        .map_err(|e| Self::error(&format!("{e:?}")))?;
        if proposed.as_ref() != Some(&expected) {
            return Err(Self::error(
                "The proposal does not match its captured core action.",
            ));
        }
        if matches!(action, ui::Action::Arm { .. }) && self.recording_active() {
            return self.submit_arming(origin, expected);
        }
        self.idle()?;
        if !matches!(action, ui::Action::Arm { .. }) && self.has_routes(origin) {
            return Err(Self::error("Routed layout migration remains pending."));
        }
        let mut configuration = current.clone();
        configuration.buses = expected;
        self.submit_config(origin, configuration)
    }
    pub fn initialize(&self, origin: &Origin) -> Result<Request, Error> {
        self.validate(origin)?;
        self.idle()?;
        let snapshot = origin
            .0
            .config
            .as_ref()
            .ok_or_else(|| Self::error("No authenticated local readback."))?;
        if snapshot.configuration.is_some() {
            return Err(Self::error("Logical metadata is already known."));
        }
        if snapshot.local.as_ref().is_none_or(|b| b.channels != 2) {
            return Err(Self::error("Default metadata adoption needs an observed local two-channel filter; other layouts remain pending."));
        }
        self.submit_config(origin, super::default_idle_configuration())
    }
    fn submit_config(
        &self,
        origin: &Origin,
        configuration: IdleBusConfig,
    ) -> Result<Request, Error> {
        let backend = self
            .service
            .as_ref()
            .ok_or_else(|| Self::error("No bus backend is attached."))?;
        let expected = origin
            .0
            .config
            .as_ref()
            .ok_or_else(|| Self::error("No configuration revision."))?
            .revision;
        let (ticket, lease) = self.session.reserve_external().map_err(Error::Owner)?;
        let request = Request(Rc::new(()));
        Self::remember_arming(&mut self.data.borrow_mut());
        self.data.borrow_mut().operation = Some(ConfigWork {
            arming: None,
            request: request.clone(),
            ticket,
            expected,
            lease: lease.clone(),
            receipt: None,
            progress: ui::Progress::Queued,
            terminal: false,
            completion: None,
            reconcile_requested: false,
        });
        // Do not hold UI state/model borrows across an injected/native handoff.
        match backend.configure(IdleConfigIntent {
            ticket,
            expected,
            configuration,
        }) {
            Ok(receipt) => {
                self.data.borrow_mut().operation.as_mut().unwrap().receipt = Some(receipt);
                Ok(request)
            }
            Err(error) => {
                self.session.release_external(&lease);
                self.data.borrow_mut().operation = None;
                Err(error)
            }
        }
    }
    pub fn start(&self, origin: &Origin, bus: Option<String>) -> Result<(), Error> {
        self.validate(origin)?;
        let backend = self
            .service
            .as_ref()
            .ok_or_else(|| Self::error("No recording backend is attached."))?;
        let config = origin
            .0
            .config
            .as_ref()
            .filter(|s| s.configuration.is_some())
            .ok_or_else(|| Self::error("Native logical configuration is unknown."))?;
        let membership = origin
            .0
            .membership
            .ok_or_else(|| Self::error("Recording membership is unknown."))?;
        let root = origin
            .0
            .root
            .clone()
            .ok_or_else(|| Self::error("Select an explicit recording directory first."))?;
        let fresh = backend.read().ok_or(Error::Recording(
            pecan_pipewire::recording::RecordingError::Busy,
        ))?;
        if fresh.revision != membership {
            return Err(Error::Recording(
                pecan_pipewire::recording::RecordingError::StaleRecording,
            ));
        }
        if fresh.config.revision != config.revision {
            return Err(Error::Recording(
                pecan_pipewire::recording::RecordingError::StaleConfig,
            ));
        }
        self.validate(origin)?;
        {
            let d = self.data.borrow();
            if d.takes.iter().filter(|t| !t.status.complete).count() >= 64
                || !d.orphans.is_empty()
                || d.unknown_owned
                || d.operation.as_ref().is_some_and(|o| {
                    !o.terminal
                        || matches!(
                            o.progress,
                            ui::Progress::PartiallyApplied(_) | ui::Progress::Unconfirmed(_)
                        )
                })
                || d.takes.iter().any(|t| {
                    !t.status.complete
                        && matches!(
                            t.status.progress,
                            RecordingProgress::Queued | RecordingProgress::Opening
                        )
                })
            {
                return Err(Self::error(
                    "A start/configuration transition or capacity readback is pending.",
                ));
            }
        }
        let (ticket, lease) = self.session.reserve_external().map_err(Error::Owner)?;
        let expected = Source::new(ticket, origin.0.graph.as_ref().unwrap(), config.revision);
        let start = RecordingStart {
            ticket,
            origin: origin.0.graph.clone().unwrap(),
            expected_config: config.revision,
            root,
        };
        let result = if let Some(bus) = bus {
            backend.start_bus(RecordingBusStart {
                start,
                expected_recording: membership,
                bus,
            })
        } else {
            backend.start_all(start)
        };
        self.session.release_external(&lease);
        let Started { receipt, no_op } = result?;
        let report = receipt.report();
        if !no_op && report.as_ref().is_some_and(|r| r.source != expected) {
            self.data
                .borrow_mut()
                .orphans
                .push((Some(expected), receipt));
            self.data.borrow_mut().protocol_invalid = true;
            return Err(Self::error("Mismatched admitted recording receipt; retained for shutdown, effects unconfirmed."));
        }
        let mut d = self.data.borrow_mut();
        d.all_stopped = false;
        if let Some(status) = report {
            Self::retain(&mut d, receipt, status);
        } else {
            d.orphans.push(((!no_op).then_some(expected), receipt));
        }
        Ok(())
    }
    pub fn stop_all(&self) -> Result<(), Error> {
        if self.data.borrow().all_stopped {
            return Ok(());
        }
        self.stop(Stop::All)?;
        self.data.borrow_mut().all_stopped = true;
        Ok(())
    }
    pub fn stop_take(&self, take: TakeKey) -> Result<(), Error> {
        if self.data.borrow().all_stopped || self.data.borrow().stopped_takes.contains(&take) {
            return Ok(());
        }
        self.stop(Stop::Take(take))?;
        self.data.borrow_mut().stopped_takes.insert(take);
        Ok(())
    }
    fn stop(&self, target: Stop) -> Result<(), Error> {
        let receipts = self
            .service
            .as_ref()
            .ok_or_else(|| Self::error("No recorder owner."))?
            .stop(target)?;
        let mut d = self.data.borrow_mut();
        for receipt in receipts {
            if let Some(status) = receipt.report() {
                Self::retain(&mut d, receipt, status);
            } else if d.orphans.len() < 256 {
                d.orphans.push((None, receipt));
            } else {
                d.error=Some("Stop was requested; opaque diagnostic aliases are saturated. Original admission receipts and common shutdown remain retained.".into());
            }
        }
        Ok(())
    }
    pub fn stop_bus(&self, origin: &Origin, bus: &str) -> Result<(), Error> {
        // Stable IDs are deliberately captured from the edited publication;
        // disconnect does not require restamping them against another graph.
        if !Rc::ptr_eq(&origin.0.owner, &self.authority) {
            return Err(Self::error("This stop belongs to another recorder owner."));
        }
        if self.data.borrow().all_stopped {
            return Ok(());
        }
        let ids = origin.0.tracks.get(bus).cloned().unwrap_or_default();
        if ids.is_empty() {
            return Err(Self::error("Stable track IDs are not yet available; Stop all remains available during opening."));
        }
        for id in ids {
            if !self.data.borrow().stopped.contains(&id) {
                self.stop(Stop::Track(id))?;
                self.data.borrow_mut().stopped.insert(id);
            }
        }
        Ok(())
    }
    fn retain(d: &mut Data, receipt: Rc<dyn TakePoll>, status: TakeStatus) {
        if d.retired.iter().any(|r| same_take(r, &receipt)) {
            return;
        }
        if status.tracks.len() > 64 {
            d.error =
                Some("Malformed recording track budget; receipt retained as unavailable.".into());
            if d.orphans.len() < 256 {
                d.orphans.push((None, receipt));
            }
            return;
        }
        if let Some(old) = d.takes.iter_mut().find(|t| same_take(&t.receipt, &receipt)) {
            if old.status.source != status.source {
                d.protocol_invalid = true;
                d.error =
                    Some("Recording receipt identity changed; original handle retained.".into());
            } else {
                old.status = status;
            }
            return;
        }
        if d.takes.len() >= 80 {
            if let Some(index) = d.takes.iter().position(|t| t.status.complete) {
                Self::retire(d, index);
            } else {
                d.error = Some("Retained take capacity reached; no new starts allowed.".into());
                if d.orphans.len() < 256 {
                    d.orphans.push((None, receipt));
                }
                return;
            }
        }
        d.takes.push(Retained { receipt, status });
    }
    pub fn refresh(&self) {
        {
            let mut d = self.data.borrow_mut();
            if let Some(work) = d.operation.as_mut().filter(|w| w.terminal) {
                // A request to inspect is not evidence that inspection worked.
                // Keep the receipt and its barrier across busy/stale reads.
                work.reconcile_requested = true;
            } else if !d.protocol_invalid {
                d.error = None;
            }
            // Publish the retained barrier even if inspection is busy, and
            // acknowledge the widget's explicit Refresh without an audio write.
            d.publication = None;
        }
        self.poll();
    }
    pub fn poll(&self) {
        if self.polling.replace(true) {
            return;
        }
        self.poll_inner();
        self.polling.set(false);
    }
    fn poll_inner(&self) {
        // Capture the requested operation before inspecting. A reentrant
        // Refresh during receipt polling cannot consume an earlier inspection.
        let reconciliation = self
            .data
            .borrow()
            .operation
            .as_ref()
            .filter(|w| w.reconcile_requested)
            .map(|w| w.request.clone());
        let mut inspected = None;
        self.data.borrow_mut().folder_fresh = false;
        let read = self.service.as_ref().and_then(|s| s.read());
        if let Some(Readback {
            revision,
            config,
            latest_started,
            takes,
        }) = read
        {
            if revision.owner == config.revision.owner {
                inspected = Some(config.clone());
                let reports: Vec<_> = takes
                    .into_iter()
                    .map(|receipt| {
                        let report = receipt.report();
                        (receipt, report)
                    })
                    .collect();
                let latest = latest_started.map(|r| {
                    let report = r.report();
                    (r, report)
                });
                let unknown = reports.iter().any(|(_, r)| r.is_none());
                let mut d = self.data.borrow_mut();
                d.folder_fresh = true;
                if let Some((receipt, report)) = latest {
                    let previous = d
                        .latest_started
                        .as_ref()
                        .filter(|(r, _)| same_take(r, &receipt))
                        .and_then(|(_, s)| s.clone());
                    d.latest_started = Some((receipt, report.or(previous)));
                } else {
                    d.latest_started = None;
                }
                d.config = Some(config);
                d.membership = Some(revision);
                d.unknown_owned = unknown;
                for (receipt, report) in reports {
                    if let Some(status) = report {
                        Self::retain(&mut d, receipt, status);
                    }
                }
            } else {
                let mut d = self.data.borrow_mut();
                d.protocol_invalid = true;
                d.error =
                    Some("Mismatched configuration/membership owner; no edit authority.".into());
            }
        }
        self.poll_arming();
        let receipts: Vec<_> = {
            let mut d = self.data.borrow_mut();
            let mut receipts: Vec<_> = d
                .takes
                .iter()
                .map(|t| (Some(t.status.source), t.receipt.clone()))
                .collect();
            receipts.extend(std::mem::take(&mut d.orphans));
            receipts
        };
        let mut resolved = Vec::new();
        for (expected, receipt) in receipts {
            match receipt.report() {
                Some(status) => {
                    if expected.is_none_or(|s| s == status.source) {
                        resolved.push((receipt, status));
                    } else {
                        let mut d = self.data.borrow_mut();
                        d.protocol_invalid = true;
                        d.error = Some(
                            "Recording receipt identity changed; original handle retained.".into(),
                        );
                        if !d.takes.iter().any(|t| same_take(&t.receipt, &receipt))
                            && !d.orphans.iter().any(|(_, r)| same_take(r, &receipt))
                        {
                            d.orphans.push((expected, receipt));
                        }
                    }
                }
                None => {
                    if !self
                        .data
                        .borrow()
                        .takes
                        .iter()
                        .any(|t| same_take(&t.receipt, &receipt))
                    {
                        self.data.borrow_mut().orphans.push((expected, receipt));
                    }
                }
            }
        }
        {
            let mut d = self.data.borrow_mut();
            for (receipt, status) in resolved {
                Self::retain(&mut d, receipt, status);
            }
        }
        let pending = self
            .data
            .borrow()
            .operation
            .as_ref()
            .and_then(|o| o.receipt.clone().map(|r| (o.request.clone(), r)));
        if let Some((request, report)) =
            pending.and_then(|(request, r)| r.report().map(|report| (request, report)))
        {
            self.config_report(&request, report);
        }
        self.reconcile(reconciliation.as_ref(), inspected.as_ref());
        {
            let mut d = self.data.borrow_mut();
            while d.takes.iter().filter(|t| t.status.complete).count() > 16 {
                let Some(index) = d.takes.iter().position(|t| t.status.complete) else {
                    break;
                };
                Self::retire(&mut d, index);
            }
            let live: BTreeSet<_> = d
                .takes
                .iter()
                .flat_map(|t| t.status.tracks.iter().map(|t| t.identity))
                .collect();
            d.stopped.retain(|id| live.contains(id));
            let takes: BTreeSet<_> = d.takes.iter().filter_map(|t| t.status.take).collect();
            d.stopped_takes.retain(|id| takes.contains(id));
        }
        self.publish();
    }
    fn reconcile(&self, requested: Option<&Request>, inspected: Option<&ConfigSnapshot>) {
        let (Some(requested), Some(inspected)) = (requested, inspected) else {
            return;
        };
        let graph = self.session.graph();
        let mut d = self.data.borrow_mut();
        let ready = !d.protocol_invalid
            && self.view.awaited_request().is_none()
            && inspected.connected
            && graph
                .as_ref()
                .is_some_and(|g| g.value.verbonden && g.stamp.epoch == inspected.revision.epoch)
            && d.operation.as_ref().is_some_and(|w| {
                &w.request == requested
                    && w.reconcile_requested
                    && w.terminal
                    && revision_covers(
                        inspected.revision,
                        w.completion.as_ref().map_or(w.expected, |c| c.revision),
                    )
            });
        if ready {
            Self::remember_arming(&mut d);
            d.operation = None;
            d.error = None;
        }
    }
    fn config_report(&self, request: &Request, report: ConfigReport) {
        let release = {
            let mut d = self.data.borrow_mut();
            let Some(work) = d.operation.as_mut() else {
                return;
            };
            if &work.request != request {
                return;
            }
            if report.ticket != work.ticket || report.expected != work.expected {
                work.progress = ui::Progress::Unconfirmed(
                    "Receipt correlation mismatch; original receipt/lease retained.".into(),
                );
                return;
            }
            work.progress = match report.progress {
                ConfigProgress::Queued => ui::Progress::Queued,
                ConfigProgress::Applying => ui::Progress::Applying,
                ConfigProgress::Unconfirmed => {
                    work.terminal = true;
                    ui::Progress::Unconfirmed(
                        "Backend configuration completion is unconfirmed.".into(),
                    )
                }
                ConfigProgress::Completed(result) => {
                    work.terminal = true;
                    work.completion = Some(result.snapshot);
                    match result.outcome {
                        LocalConfigOutcome::Applied => ui::Progress::AppliedLocally,
                        LocalConfigOutcome::NoOp => ui::Progress::NoOpLocally,
                        LocalConfigOutcome::Rejected(e) => ui::Progress::Rejected(format!("{e:?}")),
                        LocalConfigOutcome::PartiallyApplied(e) => {
                            ui::Progress::PartiallyApplied(format!("{e:?}"))
                        }
                    }
                }
            };
            work.terminal.then(|| work.lease.clone())
        };
        if let Some(lease) = release {
            self.session.release_external(&lease);
        }
    }
    fn has_routes(&self, origin: &Origin) -> bool {
        origin.0.graph.as_ref().is_some_and(|g| {
            let ids: BTreeSet<_> = g
                .value
                .knopen
                .iter()
                .filter(|n| pecan_core::domain::audio::is_bus(n))
                .map(|n| n.id)
                .collect();
            g.value
                .kabels
                .iter()
                .any(|k| ids.contains(&k.in_knoop) || ids.contains(&k.uit_knoop))
        })
    }
    fn retire(d: &mut Data, index: usize) {
        let take = d.takes.remove(index);
        for error in take_errors(&take.status) {
            if d.past_failures.len() < 32 {
                d.past_failures.push(error);
            }
        }
        d.retired.push_back(take.receipt);
        if d.retired.len() > 80 {
            d.retired.pop_front();
        }
    }
    /// Inspect only after the common backend handle has really joined. Never
    /// rewrite its writer counters or pretend ingress failure was a join failure.
    pub(crate) fn finalization_errors(&self, joined_failed: bool) -> Option<Vec<String>> {
        self.poll_arming();
        let mut receipts: Vec<(Option<Source>, Rc<dyn TakePoll>)> = {
            let d = self.data.borrow();
            d.takes
                .iter()
                .map(|t| (Some(t.status.source), t.receipt.clone()))
                .chain(d.orphans.iter().cloned())
                .collect()
        };
        if let Some(service) = &self.service {
            match service.read() {
                Some(read) => receipts.extend(read.takes.into_iter().map(|r| (None, r))),
                None if !joined_failed => return None,
                None => {}
            }
        }
        let mut errors = self.data.borrow().past_failures.clone();
        self.arming_finalization_errors(joined_failed, &mut errors)?;
        if self.data.borrow().protocol_invalid {
            errors.push(
                "Recording/configuration identity was invalid; completion remains unconfirmed."
                    .into(),
            );
        }
        let mut seen: Vec<Rc<dyn TakePoll>> = vec![];
        let configuration = self
            .data
            .borrow()
            .operation
            .as_ref()
            .and_then(|w| w.receipt.clone().map(|r| (w.ticket, w.expected, r)));
        if let Some((ticket, expected, receipt)) = configuration {
            match receipt.report(){
                None if !joined_failed=>return None,
                Some(report) if report.ticket==ticket&&report.expected==expected=>match report.progress{
                    ConfigProgress::Completed(result) if !matches!(result.outcome,LocalConfigOutcome::PartiallyApplied(_))=>{},
                    _=>errors.push("Configuration outcome remains partial/unconfirmed after common backend join.".into()),
                },
                _=>errors.push("Configuration diagnostic identity unavailable after backend join.".into()),
            }
        }
        for (expected, receipt) in receipts {
            let Some(report) = receipt.report() else {
                if !joined_failed {
                    return None;
                }
                if errors.len() < 32 {
                    errors.push(
                                "Recording receipt unavailable after failed backend join; outcome unconfirmed."
                            .into(),
                    );
                }
                continue;
            };
            if expected.is_some_and(|key| key != report.source) {
                if errors.len() < 32 {
                    errors.push("Recording receipt identity mismatch after backend join.".into());
                }
                continue;
            }
            if !seen.iter().any(|r| same_take(r, &receipt)) {
                seen.push(receipt);
                for error in take_errors(&report) {
                    if errors.len() < 32 {
                        errors.push(error);
                    }
                }
            }
        }
        Some(errors)
    }
}
fn revision_covers(now: ConfigRevision, required: ConfigRevision) -> bool {
    now.owner == required.owner
        && (now.epoch > required.epoch
            || (now.epoch == required.epoch && now.revision >= required.revision))
}
fn take_errors(report: &TakeStatus) -> Vec<String> {
    let mut errors = vec![];
    if let Some(failure) = &report.failure {
        if !matches!(
            failure,
            pecan_pipewire::recording::RecordingError::CancelledBeforeStart
        ) {
            errors.push(format!(
                "Recording request {} failed: {failure:?}; writer {:?} {:?}",
                report.source.ticket, report.writer, report.writer_outcome
            ));
        }
    }
    if !report.complete || report.progress == RecordingProgress::Unconfirmed {
        errors.push(format!(
            "Recording request {} remains unconfirmed after common backend join.",
            report.source.ticket
        ));
    }
    errors
}
#[cfg(test)]
mod tests;
