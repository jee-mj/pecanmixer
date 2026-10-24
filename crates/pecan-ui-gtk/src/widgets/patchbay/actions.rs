//! Patchbay selection/draft state. Routing and command planning stay in core.
use pecan_core::{
    backend::Stamped,
    domain::{
        audio,
        buses::BusConfig,
        history::{self, Action},
    },
    graph::Momentopname,
};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug)]
pub(super) struct Endpoint {
    pub id: u32,
    pub label: String,
}
#[derive(Clone, Copy)]
pub(super) enum Operation {
    Connect,
    Disconnect,
    Move,
}
#[derive(Default)]
pub(super) struct Editor {
    pub graph: Option<Stamped<Arc<Momentopname>>>,
    pub outputs: Vec<Endpoint>,
    pub inputs: Vec<Endpoint>,
    pub source: Option<u32>,
    pub destination: Option<u32>,
    pub selected: BTreeSet<u32>,
    pub moving: Option<[u32; 2]>,
}
impl Editor {
    pub fn update(&mut self, graph: Stamped<Arc<Momentopname>>) -> bool {
        if self
            .graph
            .as_ref()
            .is_some_and(|old| old.stamp >= graph.stamp)
        {
            return false;
        }
        // No UI selection is silently rebound to a new snapshot/object ID.
        // The application retains any in-flight request and its original origin.
        self.cancel();
        self.outputs.clear();
        self.inputs.clear();
        if graph.value.verbonden {
            for group in audio::layout(&graph.value).groups {
                for module in group.modules {
                    for port in module.ports {
                        let endpoint = Endpoint {
                            id: port.id,
                            label: format!("{} · {} · port {}", module.title, port.label, port.id),
                        };
                        match module.side {
                            audio::Side::Left => self.outputs.push(endpoint),
                            audio::Side::Right => self.inputs.push(endpoint),
                        }
                    }
                }
            }
        }
        self.graph = Some(graph);
        true
    }
    pub fn connected(&self) -> bool {
        self.graph.as_ref().is_some_and(|g| g.value.verbonden)
    }
    pub fn cancel(&mut self) {
        self.source = None;
        self.destination = None;
        self.selected.clear();
        self.moving = None;
    }
    pub fn pair(&self, id: u32) -> Option<[u32; 2]> {
        let graph = &self.graph.as_ref()?.value;
        let link = graph.kabels.iter().find(|l| l.id == id)?;
        let output = graph
            .poorten
            .iter()
            .find(|p| p.id == link.uit_poort && p.knoop == link.uit_knoop)?;
        let input = graph
            .poorten
            .iter()
            .find(|p| p.id == link.in_poort && p.knoop == link.in_knoop)?;
        (self.outputs.iter().any(|p| p.id == output.id)
            && self.inputs.iter().any(|p| p.id == input.id))
        .then_some([output.id, input.id])
    }
    pub fn begin_move(&mut self) -> Result<(), String> {
        if self.selected.len() != 1 {
            return Err("Select one available audio connection to move.".into());
        }
        let pair = self
            .pair(*self.selected.first().unwrap())
            .ok_or("The selected connection has unavailable or unsupported endpoints.")?;
        self.source = Some(pair[0]);
        self.destination = None;
        self.moving = Some(pair);
        Ok(())
    }
    pub fn prepare(&self, operation: Operation) -> Result<Action, String> {
        let graph = self
            .graph
            .as_ref()
            .filter(|g| g.value.verbonden)
            .ok_or("Disconnected; editing is unavailable.")?;
        let action = match operation {
            Operation::Disconnect => {
                if self.selected.is_empty() {
                    return Err("Select connections to disconnect.".into());
                }
                let pairs=self.selected.iter().map(|id|self.pair(*id).ok_or_else(||"A selected connection has unavailable or unsupported endpoints; deselect it first.".to_owned())).collect::<Result<Vec<_>,_>>()?;
                Action::Disconnect(pairs)
            }
            Operation::Connect | Operation::Move => {
                if matches!(operation, Operation::Move) && self.moving.is_none() {
                    return Err("Select a connection to move first.".into());
                }
                let pair = [
                    self.source.ok_or("Choose an audio output.")?,
                    self.destination.ok_or("Choose an audio input.")?,
                ];
                if !self.outputs.iter().any(|p| p.id == pair[0])
                    || !self.inputs.iter().any(|p| p.id == pair[1])
                {
                    return Err("Choose currently available audio endpoints.".into());
                }
                if graph
                    .value
                    .kabels
                    .iter()
                    .any(|l| [l.uit_poort, l.in_poort] == pair)
                {
                    return Err("Those ports are already connected.".into());
                }
                if let Some(old) = self.moving {
                    Action::Move { old, new: pair }
                } else {
                    Action::Connect(pair)
                }
            }
        };
        if history::plan(&action, &graph.value, &BusConfig::default())
            .map_err(|e| format!("Cannot plan patch: {e:?}"))?
            .is_empty()
        {
            return Err("The graph already satisfies this action.".into());
        }
        Ok(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecan_core::{
        backend::Stamp,
        graph::{Kabel, Opdracht},
    };
    fn fixture(revision: u64) -> Stamped<Arc<Momentopname>> {
        Stamped {
            stamp: Stamp { epoch: 1, revision },
            value: Arc::new(
                serde_json::from_str(include_str!(
                    "../../../../../fixtures/graph/audio-basic.json"
                ))
                .unwrap(),
            ),
        }
    }
    #[test]
    fn connect_duplicate_and_direction_validation_use_core_plans() {
        let mut editor = Editor::default();
        editor.update(fixture(1));
        editor.source = Some(201);
        editor.destination = Some(301);
        let action = editor.prepare(Operation::Connect).unwrap();
        assert_eq!(action, Action::Connect([201, 301]));
        assert_eq!(
            history::plan(
                &action,
                &editor.graph.as_ref().unwrap().value,
                &BusConfig::default()
            )
            .unwrap(),
            vec![Opdracht::Verbind {
                uit: 201,
                naar: 301
            }]
        );
        editor.destination = Some(101);
        assert!(editor
            .prepare(Operation::Connect)
            .unwrap_err()
            .contains("already connected"));
        editor.source = Some(101);
        assert!(editor.prepare(Operation::Connect).is_err());
    }
    #[test]
    fn selection_disconnect_move_and_cancel_preserve_history_boundaries() {
        let mut editor = Editor::default();
        editor.update(fixture(1));
        editor.selected.extend([500, 501]);
        assert_eq!(
            editor.prepare(Operation::Disconnect).unwrap(),
            Action::Disconnect(vec![[201, 101], [202, 102]])
        );
        assert!(editor.begin_move().is_err());
        editor.selected.remove(&501);
        editor.begin_move().unwrap();
        editor.destination = Some(301);
        let action = editor.prepare(Operation::Move).unwrap();
        assert_eq!(
            action,
            Action::Move {
                old: [201, 101],
                new: [201, 301]
            }
        );
        assert_eq!(
            history::plan(
                &action,
                &editor.graph.as_ref().unwrap().value,
                &BusConfig::default()
            )
            .unwrap(),
            vec![
                Opdracht::Ontkoppel { kabel: 500 },
                Opdracht::Verbind {
                    uit: 201,
                    naar: 301
                }
            ]
        );
        editor.cancel();
        assert!(editor.selected.is_empty() && editor.moving.is_none());
        assert!(
            editor.prepare(Operation::Move).is_err(),
            "cancel must not become a disconnect"
        );
        assert_eq!(
            editor.graph.as_ref().unwrap().value.kabels.len(),
            2,
            "drafts never mutate snapshots"
        );
    }
    #[test]
    fn missing_endpoints_are_not_editable_and_new_stamps_cancel_stale_choices() {
        let mut graph = fixture(1);
        Arc::make_mut(&mut graph.value).kabels.push(Kabel {
            id: 900,
            uit_knoop: 99,
            uit_poort: 98,
            in_knoop: 10,
            in_poort: 101,
            client: None,
        });
        let mut editor = Editor::default();
        editor.update(graph);
        editor.selected.insert(900);
        assert!(editor
            .prepare(Operation::Disconnect)
            .unwrap_err()
            .contains("unavailable"));
        editor.source = Some(201);
        editor.destination = Some(301);
        assert!(!editor.update(fixture(1)));
        assert_eq!(editor.source, Some(201));
        assert!(editor.update(fixture(2)));
        assert!(editor.source.is_none() && editor.selected.is_empty());
        let mut disconnected = fixture(3);
        Arc::make_mut(&mut disconnected.value).verbonden = false;
        editor.update(disconnected);
        assert!(editor.outputs.is_empty() && editor.inputs.is_empty());
        assert!(editor.prepare(Operation::Connect).is_err());
    }
}
