//! One routing surface over BusOwner selection and Session receipts/history.
use super::{buses::RoutingKey, BusOrigin, BusOwner, Request, Session};
use crate::routing::{Event, RequestToken, RoutingControls};
use pecan_core::{
    domain::history::Input,
    native_command::{NativeProgress, NativeReport},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Tracked {
    token: RequestToken,
    request: Request,
    last: Option<NativeReport>,
}
pub(super) struct Routes {
    session: Session,
    buses: RefCell<Rc<BusOwner>>,
    controls: RoutingControls<BusOrigin>,
    publication: RefCell<Option<Option<RoutingKey>>>,
    pending: RefCell<Option<Tracked>>,
    recovery_bound: RefCell<Option<RequestToken>>,
    recovering: Cell<bool>,
    weak: std::rc::Weak<Self>,
    // One last terminal diagnostic, not an unbounded per-tick report queue.
    completed: RefCell<Option<Tracked>>,
    navigate: RefCell<Option<Box<dyn Fn()>>>,
}
impl Routes {
    pub fn new(session: Session, buses: Rc<BusOwner>) -> Rc<Self> {
        let routes = Rc::new_cyclic(|weak| Self {
            session,
            buses: RefCell::new(buses),
            controls: RoutingControls::new(),
            publication: RefCell::new(None),
            pending: RefCell::new(None),
            recovery_bound: RefCell::new(None),
            recovering: Cell::new(false),
            weak: weak.clone(),
            completed: RefCell::new(None),
            navigate: RefCell::new(None),
        });
        let weak = Rc::downgrade(&routes);
        routes.controls.bind_intents(move |event| {
            weak.upgrade()
                .ok_or("Routing owner is no longer available.")?
                .handle(event)
        });
        routes.refresh_capture(true);
        routes
    }
    pub fn widget(&self) -> &gtk::Box {
        self.controls.widget()
    }
    pub fn bind_navigation(&self, callback: impl Fn() + 'static) {
        *self.navigate.borrow_mut() = Some(Box::new(callback));
    }
    fn handle(&self, event: Event<BusOrigin>) -> Result<(), String> {
        let buses = self.buses.borrow().clone();
        match event {
            Event::Selection {
                origin,
                graph,
                binding,
                key,
                ..
            } => {
                buses.routing_selection(&origin, &graph, &binding, &key)?;
                self.refresh_capture(true);
                Ok(())
            }
            Event::OpenPatchbay { origin, graph, .. } => {
                buses.routing_navigation(&origin, &graph)?;
                if let Some(callback) = self.navigate.borrow().as_ref() {
                    callback();
                }
                Ok(())
            }
            Event::History(intent) => {
                if self.pending.borrow().is_some() {
                    return Err(
                        "Retain the preceding routing receipt before another handoff.".into(),
                    );
                }
                buses.authorize_routing(
                    &intent.origin,
                    &intent.graph,
                    &intent.bus_configuration,
                )?;
                let Input::Execute { label, action } = intent.history else {
                    return Err("Routing may submit actions, not history/result metadata.".into());
                };
                let request = self
                    .session
                    .execute(&intent.graph, label, action)?
                    .ok_or("No native commands planned; refresh the routing presentation.")?;
                *self.pending.borrow_mut() = Some(Tracked {
                    token: intent.request,
                    request,
                    last: None,
                });
                Ok(())
            }
        }
    }
    fn refresh_capture(&self, force: bool) {
        let buses = self.buses.borrow().clone();
        let key = buses.routing_key();
        if !force && self.publication.borrow().as_ref() == Some(&key) {
            return;
        }
        *self.publication.borrow_mut() = Some(key);
        if self.session.busy() && self.pending.borrow().is_none() {
            self.controls
                .unavailable("Another shared request is pending; await truthful closure.");
            return;
        }
        match buses.routing_capture() {
            Ok(capture) => {
                if let Err(error) = self.controls.observe(capture) {
                    self.controls.unavailable(&error.to_string());
                }
            }
            Err(error) => self.controls.unavailable(&error),
        }
    }
    fn bind_recovery(&self, token: &RequestToken, request: &Request) {
        if self.recovery_bound.borrow().as_ref() == Some(token) {
            return;
        }
        *self.recovery_bound.borrow_mut() = Some(token.clone());
        let (token, request, weak) = (token.clone(), request.clone(), self.weak.clone());
        self.controls.bind_recovery(Some(Rc::new(move || {
            weak.upgrade()
                .ok_or("Routing owner is no longer available.")?
                .recover(&token, &request)
        })));
    }
    fn recover(&self, token: &RequestToken, request: &Request) -> Result<(), String> {
        if self.recovering.replace(true) {
            return Err("Routing recovery inspection is already running.".into());
        }
        let result = self.recover_inner(token, request);
        self.recovering.set(false);
        result
    }
    fn recover_inner(&self, token: &RequestToken, request: &Request) -> Result<(), String> {
        if !self.pending.borrow().as_ref().is_some_and(|p| {
            &p.token == token
                && p.request.same_request(request)
                && p.last.as_ref().is_some_and(needs_recovery)
        }) {
            return Err("This recovery control belongs to a retired request.".into());
        }
        let graph = self.session.graph().ok_or("Native graph unavailable.")?;
        self.session.recovery_ready(request, &graph)?;
        let buses = self.buses.borrow().clone();
        let (capture, graph, configuration) = buses.routing_recovery_capture()?;
        if !self
            .pending
            .borrow()
            .as_ref()
            .is_some_and(|p| &p.token == token && p.request.same_request(request))
        {
            return Err("The retained request changed during recovery inspection.".into());
        }
        self.controls
            .reconciled(capture)
            .map_err(|e| e.to_string())?;
        if let Err(error) = self
            .session
            .recover_request(request, &graph, &configuration)
        {
            self.controls.recovery_failed(&error);
            return Err(error);
        }
        *self.completed.borrow_mut() = self.pending.borrow_mut().take();
        *self.recovery_bound.borrow_mut() = None;
        self.controls.bind_recovery(None);
        self.refresh_capture(true);
        Ok(())
    }
    /// Called from the existing workspace clock even hidden/disconnected. Read
    /// the retained actual receipt without an inbox drain or synthetic status.
    pub fn poll(&self) {
        let pending = self
            .pending
            .borrow()
            .as_ref()
            .map(|p| (p.token.clone(), p.request.clone(), p.last.clone()));
        if let Some((token, request, last)) = pending {
            let report = request.native_report();
            if last.as_ref() != Some(&report) {
                let accepted = self.controls.report(&token, report.clone());
                if !accepted {
                    self.controls.unavailable("Native routing report did not satisfy its captured identity/barrier; receipt retained.");
                }
                if let Some(p) = self
                    .pending
                    .borrow_mut()
                    .as_mut()
                    .filter(|p| p.token == token)
                {
                    p.last = Some(report.clone());
                }
            }
            if needs_recovery(&report) {
                self.bind_recovery(&token, &request);
            }
            let in_progress = match &report.progress {
                NativeProgress::ExecutedLocally(c)
                | NativeProgress::Observed { counts: c, .. }
                | NativeProgress::ObservedNoOp { counts: c, .. }
                | NativeProgress::Rejected { counts: c, .. }
                | NativeProgress::Unconfirmed { counts: c, .. } => c.in_progress,
                NativeProgress::Queued => 0,
            };
            if report.progress.is_terminal()
                && !needs_recovery(&report)
                && in_progress == 0
                && !self.session.request_pending(&request)
                && self.controls.pending().is_none()
            {
                let mut pending = self.pending.borrow_mut();
                if pending.as_ref().is_some_and(|p| p.token == token) {
                    *self.completed.borrow_mut() = pending.take();
                }
            }
        }
        self.refresh_capture(false);
        // Closure/graphs alone never call reconciled(): only the explicit
        // original-request recovery action performs fresh owner inspection.
    }
    #[cfg(test)]
    pub fn attach_test_buses(&self, buses: Rc<BusOwner>) {
        assert!(self.pending.borrow().is_none());
        assert!(self.session.same_session(&buses.session));
        *self.buses.borrow_mut() = buses;
        self.refresh_capture(true);
    }
    #[cfg(test)]
    pub fn retained_report(&self) -> Option<NativeReport> {
        self.pending
            .borrow()
            .as_ref()
            .or(self.completed.borrow().as_ref())
            .map(|p| p.request.native_report())
    }
}
fn needs_recovery(report: &NativeReport) -> bool {
    match &report.progress {
        NativeProgress::Unconfirmed { .. } => true,
        NativeProgress::Rejected { counts, .. } => counts.submitted > 0 || counts.in_progress > 0,
        _ => false,
    }
}
