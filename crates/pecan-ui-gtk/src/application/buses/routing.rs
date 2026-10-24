//! Narrow shared-selection/configuration bridge. No idle-only gate for routes.
use super::*;
use crate::routing::{Budget, Capture};

#[derive(Clone)]
pub(crate) struct RoutingKey {
    origin: Origin,
    output: Option<String>,
    rec: Option<String>,
    pending: bool,
}
impl PartialEq for RoutingKey {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.origin.0, &other.origin.0)
            && self.output == other.output
            && self.rec == other.rec
            && self.pending == other.pending
    }
}
impl BusOwner {
    pub(crate) fn routing_key(&self) -> Option<RoutingKey> {
        Some(RoutingKey {
            origin: self.origin()?,
            output: self.view.selected_output_key(),
            rec: self.view.selected_rec_key(),
            pending: self.view.configuration_pending(),
        })
    }
    pub(crate) fn routing_capture(&self) -> Result<Capture<Origin>, String> {
        let origin = self.origin().ok_or("Bus presentation unavailable.")?;
        let graph = self.routing_graph(&origin)?;
        if self.validate(&origin).is_ok() {
            if let Some(configuration) = origin
                .0
                .config
                .as_ref()
                .and_then(|c| c.configuration.as_ref())
            {
                self.session.adopt_buses(&graph, &configuration.buses)?;
            }
        }
        Capture::from_bus_controls(graph, &self.view, Budget::default()).map_err(|e| e.to_string())
    }
    fn routing_graph(&self, origin: &Origin) -> Result<Graph, String> {
        let graph = self.session.graph().ok_or("Native graph unavailable.")?;
        let captured = origin
            .0
            .graph
            .as_ref()
            .ok_or("Captured graph unavailable.")?;
        if graph.stamp != captured.stamp
            || !Arc::ptr_eq(&graph.value, &captured.value)
            || self.session.witnessed_graph().is_none()
        {
            return Err("Routing graph authority changed; await a fresh publication.".into());
        }
        Ok(graph)
    }
    pub(crate) fn routing_navigation(&self, origin: &Origin, graph: &Graph) -> Result<(), String> {
        self.validate_selection(origin).map_err(|e| e.to_string())?;
        let current = self.routing_graph(origin)?;
        if current.stamp != graph.stamp || !Arc::ptr_eq(&current.value, &graph.value) {
            return Err("The routing graph changed; start a fresh action.".into());
        }
        Ok(())
    }
    pub(crate) fn routing_selection(
        &self,
        origin: &Origin,
        graph: &Graph,
        binding: &ui::Binding<Origin>,
        key: &str,
    ) -> Result<(), String> {
        self.routing_navigation(origin, graph)?;
        self.view
            .select_destination(binding, key)
            .map_err(|e| format!("{e:?}"))
    }
    pub(crate) fn authorize_routing(
        &self,
        origin: &Origin,
        graph: &Graph,
        buses: &BusConfig,
    ) -> Result<(), String> {
        self.routing_navigation(origin, graph)?;
        self.validate(origin).map_err(|e| e.to_string())?;
        let expected = origin
            .0
            .config
            .as_ref()
            .ok_or("Configuration unavailable.")?;
        let current = self.data.borrow().config.clone();
        if current.as_ref() != Some(expected)
            || expected.configuration.as_ref().map(|c| &c.buses) != Some(buses)
        {
            return Err("Captured bus configuration changed.".into());
        }
        // Configuration/membership snapshot from the same owner. Recording
        // activity alone does not block routing; membership is not a route CAS.
        let fresh = self
            .service
            .as_ref()
            .and_then(|s| s.read())
            .ok_or("Configuration inspection is busy; no routing request submitted.")?;
        if fresh.revision.owner != expected.revision.owner
            || fresh.config.revision != expected.revision
            || !fresh.config.connected
            || fresh.config.configuration.as_ref().map(|c| &c.buses) != Some(buses)
        {
            return Err("Native bus configuration changed; await fresh readback.".into());
        }
        self.routing_navigation(origin, graph)?;
        self.validate(origin).map_err(|e| e.to_string())?;
        if self.data.borrow().config.as_ref() != Some(expected) {
            return Err("Configuration changed during routing preflight.".into());
        }
        self.session.adopt_buses(graph, buses)
    }
}
