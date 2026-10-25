//! One retained reset/inverse, one real native child at a time. This owner
//! evidence is not a NativeReport and never acknowledges child history steps.
use super::*;
use pecan_core::{
    domain::history::CommandPlan,
    graph::Momentopname,
    history_backend::MAX_HISTORY_SEQUENCE_COMMANDS,
    history_identity::HistoryGraph,
    native_command::{NativeIntent, MAX_NATIVE_COMMANDS},
};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;

const TOTAL_LIMIT: Duration = Duration::from_secs(30);
const MAX_CHILDREN: usize = MAX_HISTORY_SEQUENCE_COMMANDS / MAX_NATIVE_COMMANDS;
struct Unit {
    command: Opdracht,
    pair: Option<[u32; 2]>,
}
struct Child {
    receipt: NativeReceipt,
    ticket: u64,
    source: Stamp,
    commands: usize,
    next: usize,
}
pub(super) struct Sequence {
    ticket: u64,
    source: Stamp,
    units: Vec<Unit>,
    last_disconnect: BTreeMap<[u32; 2], usize>,
    cursor: usize,
    children: Vec<Child>,
    active: Option<usize>,
    progress: SequenceProgress,
    started: Instant,
    driving: Cell<bool>,
}
impl Sequence {
    pub(super) fn new(
        history: &history::Controller,
        frame: &HistoryGraph,
        plan: CommandPlan,
    ) -> Result<Self, String> {
        if plan.commands.len() > MAX_HISTORY_SEQUENCE_COMMANDS
            || !history.pending_reset_matches(plan.ticket, frame)
        {
            return Err(
                "Only a bounded Back-to-auto step and its inverse can use sequential batches."
                    .into(),
            );
        }
        let units = plan
            .commands
            .into_iter()
            .map(|command| {
                let pair = match command {
                    Opdracht::Ontkoppel { kabel } => Some(
                        frame
                            .graph()
                            .value
                            .kabels
                            .iter()
                            .find(|l| l.id == kabel)
                            .map(|l| [l.uit_poort, l.in_poort])
                            .ok_or("Original reset link unavailable.")?,
                    ),
                    Opdracht::Verbind { .. } | Opdracht::ZetDoel { .. } => None,
                    _ => return Err("Unsupported reset command."),
                };
                Ok(Unit { command, pair })
            })
            .collect::<Result<Vec<_>, &str>>()
            .map_err(str::to_owned)?;
        let last_disconnect = units
            .iter()
            .enumerate()
            .filter_map(|(i, u)| u.pair.map(|p| (p, i)))
            .collect();
        Ok(Self {
            ticket: plan.ticket,
            source: frame.graph().stamp,
            units,
            last_disconnect,
            cursor: 0,
            children: vec![],
            active: None,
            progress: SequenceProgress::Running,
            started: Instant::now(),
            driving: Cell::new(false),
        })
    }
    pub(super) fn report(&self) -> SequenceReport {
        SequenceReport {
            ticket: self.ticket,
            source: self.source,
            command_count: self.units.len(),
            completed_commands: self.cursor,
            progress: self.progress.clone(),
            children: self.children.iter().map(|c| c.receipt.report()).collect(),
        }
    }
    pub(super) fn execution(&self) -> NativeExecution {
        if self
            .children
            .iter()
            .all(|c| c.receipt.execution() == NativeExecution::Closed)
        {
            NativeExecution::Closed
        } else {
            NativeExecution::Pending
        }
    }
    fn matches_prefix(&self, graph: &Momentopname, through: usize) -> bool {
        let mut pairs = BTreeMap::new();
        let mut targets = BTreeMap::new();
        for unit in &self.units[..through] {
            match &unit.command {
                Opdracht::Ontkoppel { .. } => {
                    pairs.insert(unit.pair.unwrap(), false);
                }
                Opdracht::Verbind { uit, naar } => {
                    pairs.insert([*uit, *naar], true);
                }
                Opdracht::ZetDoel { knoop, doel } => {
                    targets.insert(*knoop, doel);
                }
                _ => return false,
            }
        }
        let live: BTreeSet<_> = graph
            .kabels
            .iter()
            .map(|l| [l.uit_poort, l.in_poort])
            .collect();
        pairs.iter().all(|(pair, present)| {
            (!*present
                && self
                    .last_disconnect
                    .get(pair)
                    .is_some_and(|i| *i >= through))
                || live.contains(pair) == *present
        }) && targets.iter().all(|(node, target)| {
            graph
                .knopen
                .iter()
                .any(|n| n.id == *node && &n.doel == *target)
        })
    }
    fn render(&self, graph: &Momentopname) -> (Vec<Opdracht>, usize) {
        let mut links: BTreeMap<[u32; 2], Vec<u32>> = BTreeMap::new();
        for l in &graph.kabels {
            links
                .entry([l.uit_poort, l.in_poort])
                .or_default()
                .push(l.id);
        }
        let mut chosen = BTreeSet::new();
        let mut commands = vec![];
        let mut cursor = self.cursor;
        // Bound both real commands and zero-command semantic progress per tick.
        for _ in 0..MAX_NATIVE_COMMANDS {
            let Some(unit) = self.units.get(cursor) else {
                break;
            };
            if let Some(pair) = unit.pair {
                let live: Vec<_> = links
                    .get(&pair)
                    .into_iter()
                    .flatten()
                    .filter(|id| !chosen.contains(*id))
                    .copied()
                    .collect();
                let original = match unit.command {
                    Opdracht::Ontkoppel { kabel } => kabel,
                    _ => unreachable!(),
                };
                if let Some(id) = live.iter().find(|id| **id == original).or(live.first()) {
                    chosen.insert(*id);
                    commands.push(Opdracht::Ontkoppel { kabel: *id });
                }
            } else {
                commands.push(unit.command.clone());
            }
            cursor += 1;
            if commands.len() == MAX_NATIVE_COMMANDS {
                break;
            }
        }
        (commands, cursor)
    }
}
struct Driving(Rc<RefCell<Sequence>>);
impl Drop for Driving {
    fn drop(&mut self) {
        self.0.borrow().driving.set(false);
    }
}

impl Session {
    pub(super) fn drive_reset(&self) {
        self.drive_reset_at(Instant::now());
    }
    fn drive_reset_at(&self, now: Instant) {
        let Some(request) = self.0.state.borrow().sequence.clone() else {
            return;
        };
        let Receipt::Sequence(sequence) = &request.1 else {
            return;
        };
        if sequence.borrow().driving.replace(true) {
            return;
        }
        let _driving = Driving(sequence.clone());
        let fail = |message: String| self.fail_reset(&request, sequence, message);
        let (graph, closing) = {
            let s = self.0.state.borrow();
            (s.graph.clone(), s.admission_closed)
        };
        if closing {
            fail("Finalization started; no further reset batches will be sent.".into());
            return;
        }
        if now.saturating_duration_since(sequence.borrow().started) >= TOTAL_LIMIT {
            fail(
                "Reset sequence timed out; previous batches may have effects. No automatic replay."
                    .into(),
            );
            return;
        }
        let Some(graph) =
            graph.filter(|g| g.value.verbonden && g.stamp.epoch == sequence.borrow().source.epoch)
        else {
            fail("Reset connection/epoch changed; original receipts retained.".into());
            return;
        };
        // A child may still finish after the parent stops. Every actual receipt
        // remains in the parent's handle; explicit recovery checks their closure.
        let active = {
            let q = sequence.borrow();
            q.active.map(|i| {
                let c = &q.children[i];
                (i, c.receipt.clone(), c.ticket, c.source, c.commands, c.next)
            })
        };
        let mut observed = None;
        if let Some((index, receipt, ticket, source, commands, next)) = active {
            let r = receipt.report();
            if r.ticket != ticket || r.source != source || r.command_count != commands {
                fail("Reset child receipt correlation mismatch; effects unconfirmed.".into());
                return;
            }
            match r.progress {
                NativeProgress::Rejected { .. } | NativeProgress::Unconfirmed { .. } => {
                    fail(format!("Reset stopped at native child {}: {:?}. Previous effects are not rolled back.",index+1,r.progress));
                    return;
                }
                NativeProgress::Observed { stamp, counts }
                | NativeProgress::ObservedNoOp { stamp, counts } => {
                    if stamp.epoch != source.epoch
                        || stamp.revision <= source.revision
                        || counts.in_progress != 0
                    {
                        fail("Invalid reset child observation; original evidence retained.".into());
                        return;
                    }
                    if receipt.execution() != NativeExecution::Closed || graph.stamp < stamp {
                        return;
                    }
                    observed = Some(next);
                }
                _ => return,
            }
        }
        // No UI state borrow crosses the endpoint accessor/dispatch. It checks
        // the exact current native Arc, not a copied DTO or old publication cache.
        let Ok(frame) = self.0.endpoint.recovery_witness(&graph) else {
            return;
        };
        {
            let s = self.0.state.borrow();
            if !s
                .sequence
                .as_ref()
                .is_some_and(|q| q.same_request(&request))
            {
                return;
            }
            if s.admission_closed {
                drop(s);
                fail("Finalization stopped remaining reset batches.".into());
                return;
            }
            if !s
                .graph
                .as_ref()
                .is_some_and(|g| g.stamp == graph.stamp && Arc::ptr_eq(&g.value, &graph.value))
            {
                return;
            }
            if frame.graph().stamp != graph.stamp
                || !Arc::ptr_eq(&frame.graph().value, &graph.value)
                || !s
                    .history
                    .pending_reset_matches(sequence.borrow().ticket, &frame)
            {
                drop(s);
                fail(
                    "Original reset semantic/registry authority changed; no further batches sent."
                        .into(),
                );
                return;
            }
        }
        let through = observed.unwrap_or_else(|| sequence.borrow().cursor);
        if !sequence.borrow().matches_prefix(&graph.value, through) {
            fail(
                "An earlier reset effect changed before the next batch; inspect current state."
                    .into(),
            );
            return;
        }
        if let Some(next) = observed {
            let mut q = sequence.borrow_mut();
            q.cursor = next;
            q.active = None;
        }
        if sequence.borrow().cursor == sequence.borrow().units.len() {
            self.complete_reset(&request, sequence, &graph);
            return;
        }
        let (commands, next) = sequence.borrow().render(&graph.value);
        if commands.is_empty() {
            sequence.borrow_mut().cursor = next;
            if next == sequence.borrow().units.len() {
                self.complete_reset(&request, sequence, &graph);
            }
            return;
        }
        if sequence.borrow().children.len() >= MAX_CHILDREN {
            fail("Reset native batch budget exhausted; inspect partial effects.".into());
            return;
        }
        let ticket = self.0.state.borrow_mut().history.reserve_ticket();
        let count = commands.len();
        let result = self.0.endpoint.dispatch(NativeIntent {
            ticket,
            origin: graph.clone(),
            commands,
        });
        match result {
            Ok(receipt) => {
                let mut q = sequence.borrow_mut();
                let index = q.children.len();
                q.children.push(Child {
                    receipt,
                    ticket,
                    source: graph.stamp,
                    commands: count,
                    next,
                });
                q.active = Some(index);
            }
            Err(error) => fail(format!(
                "Reset child admission rejected: {error:?}; no future batches sent."
            )),
        }
    }
    fn fail_reset(&self, request: &Request, sequence: &Rc<RefCell<Sequence>>, message: String) {
        {
            let mut q = sequence.borrow_mut();
            if !matches!(q.progress, SequenceProgress::Running) {
                return;
            }
            q.progress = SequenceProgress::Failed {
                message: message.clone(),
            };
        }
        let report = sequence.borrow().report();
        let mut s = self.0.state.borrow_mut();
        if !s.sequence.as_ref().is_some_and(|q| q.same_request(request)) {
            return;
        }
        if let Some(input) = s.results.sequence_terminal(&report) {
            let _ = s.history.reduce(input);
        }
        *request.0.borrow_mut() = Delivery::Sequence(report.progress);
        s.sequence = None;
        s.uncertain = Some(Uncertain {
            request: request.clone(),
            ticket: report.ticket,
            source: report.source,
            commands: report.command_count,
        });
        s.history_context = Rc::new(());
        s.status = Status::Unconfirmed(message);
    }
    fn complete_reset(&self, request: &Request, sequence: &Rc<RefCell<Sequence>>, graph: &Graph) {
        if graph.stamp.revision <= sequence.borrow().source.revision
            || !sequence
                .borrow()
                .matches_prefix(&graph.value, sequence.borrow().units.len())
        {
            self.fail_reset(
                request,
                sequence,
                "Combined reset postcondition is not present; no history acknowledgement.".into(),
            );
            return;
        }
        let mut q = sequence.borrow_mut();
        let no_op = q
            .children
            .iter()
            .all(|c| match c.receipt.report().progress {
                NativeProgress::Observed { counts, .. }
                | NativeProgress::ObservedNoOp { counts, .. } => counts.submitted == 0,
                _ => false,
            });
        q.progress = SequenceProgress::Observed {
            stamp: graph.stamp,
            no_op,
        };
        let report = q.report();
        drop(q);
        let mut s = self.0.state.borrow_mut();
        if let Some(input) = s.results.sequence_terminal(&report) {
            let _ = s.history.reduce(input);
        }
        *request.0.borrow_mut() = Delivery::Sequence(report.progress);
        s.sequence = None;
        s.history_context = Rc::new(());
        s.status = Status::Observed { no_op };
    }
}
