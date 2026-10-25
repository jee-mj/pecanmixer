//! Dynamic arming uses the existing shared configuration admission/barrier.
//! A local decision never completes the independently retained child writers.
use super::*;
use pecan_pipewire::recording_arming::{ArmingOutcome, ArmingProgress, RecordingArming};
use service::{ArmPoll, ArmingStatus};

pub(super) struct Work {
    source: pecan_core::backend::Stamp,
    membership: RecordingRevision,
    armed: Vec<String>,
    receipt: Option<Rc<dyn ArmPoll>>,
    // Preserve the actual decided readback, removed/withheld IDs and children.
    last: Option<ArmingStatus>,
}
impl BusOwner {
    pub(super) fn recording_active(&self) -> bool {
        let d = self.data.borrow();
        d.config.as_ref().is_some_and(|c| c.recording)
            || d.takes.iter().any(|t| {
                !t.status.complete && t.status.progress == RecordingProgress::StartedLocally
            })
    }
    pub(super) fn submit_arming(
        &self,
        origin: &Origin,
        buses: BusConfig,
    ) -> Result<Request, Error> {
        let backend = self
            .service
            .as_ref()
            .filter(|s| s.supports_arming())
            .ok_or_else(|| Self::error("Dynamic arming is unavailable on this endpoint."))?;
        let graph = origin
            .0
            .graph
            .as_ref()
            .ok_or_else(|| Self::error("No graph authority."))?;
        let config = origin
            .0
            .config
            .as_ref()
            .ok_or_else(|| Self::error("No configuration authority."))?;
        let membership = origin
            .0
            .membership
            .ok_or_else(|| Self::error("Recording membership is unknown."))?;
        let root = origin
            .0
            .root
            .clone()
            .ok_or_else(|| Self::error("Select an explicit recording directory first."))?;
        {
            let d = self.data.borrow();
            if self.session.busy()
                || d.all_stopped
                || d.unknown_owned
                || !d.orphans.is_empty()
                || d.takes.iter().any(|t| {
                    !t.status.complete
                        && matches!(
                            t.status.progress,
                            RecordingProgress::Queued | RecordingProgress::Opening
                        )
                })
                || d.operation.as_ref().is_some_and(|o| {
                    !o.terminal
                        || matches!(
                            o.progress,
                            ui::Progress::PartiallyApplied(_) | ui::Progress::Unconfirmed(_)
                        )
                })
                || self.view.awaited_request().is_some()
            {
                return Err(Self::error("Wait for recording/configuration handoff or explicitly reconcile its outcome first."));
            }
        }
        let fresh = backend.read().ok_or(Error::Recording(
            pecan_pipewire::recording::RecordingError::Busy,
        ))?;
        if fresh.revision != membership {
            return Err(Error::Recording(
                pecan_pipewire::recording::RecordingError::StaleRecording,
            ));
        }
        if &fresh.config != config {
            return Err(Error::Recording(
                pecan_pipewire::recording::RecordingError::StaleConfig,
            ));
        }
        if !fresh.config.recording {
            return Err(Self::error(
                "Dynamic arming requires an active recording; wait for an idle publication.",
            ));
        }
        self.validate(origin)?; // A reentrant inspection cannot restamp intent.
        let armed: Vec<String> = buses
            .armed()
            .map_err(|e| Self::error(&format!("{e:?}")))?
            .into_iter()
            .map(|b| b.key)
            .collect();
        let (ticket, lease) = self.session.reserve_external().map_err(Error::Owner)?;
        let request = Request(Rc::new(()));
        {
            let mut d = self.data.borrow_mut();
            Self::remember_arming(&mut d);
            Self::remember_migration(&mut d);
            d.operation = Some(ConfigWork {
                request: request.clone(),
                ticket,
                expected: config.revision,
                lease: lease.clone(),
                receipt: None,
                migration: None,
                arming: Some(Work {
                    source: graph.stamp,
                    membership,
                    armed: armed.clone(),
                    receipt: None,
                    last: None,
                }),
                progress: ui::Progress::Queued,
                terminal: false,
                completion: None,
                reconcile_requested: false,
            });
        }
        match backend.arm(RecordingArming {
            ticket,
            origin: graph.clone(),
            expected_config: config.revision,
            expected_recording: membership,
            armed,
            root,
        }) {
            Ok(receipt) => {
                self.data
                    .borrow_mut()
                    .operation
                    .as_mut()
                    .unwrap()
                    .arming
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
    pub(super) fn poll_arming(&self) {
        let pending = self.data.borrow().operation.as_ref().and_then(|o| {
            o.arming
                .as_ref()?
                .receipt
                .clone()
                .map(|r| (o.request.clone(), r))
        });
        let Some((request, report)) = pending.and_then(|(q, r)| r.report().map(|p| (q, p))) else {
            return;
        };
        // Injected/native receipt inspection must not run under a UI state
        // borrow. A child can inspect/reenter the owner; revalidate the original
        // request below before retaining results or releasing its lease.
        let children: Vec<_> = report
            .additions
            .iter()
            .map(|c| c.receipt.report())
            .collect();
        let mut d = self.data.borrow_mut();
        let Some(work) = d.operation.as_ref().filter(|w| w.request == request) else {
            return;
        };
        let arm = work.arming.as_ref().unwrap();
        if report.ticket != work.ticket
            || report.source != arm.source
            || report.expected_config != work.expected
            || report.expected_recording != arm.membership
            || report.configuration.revision.owner != work.expected.owner
            || report.recording.owner != arm.membership.owner
        {
            d.protocol_invalid = true;
            d.operation.as_mut().unwrap().progress = ui::Progress::Unconfirmed(
                "Arming receipt identity mismatch; original receipt/lease retained.".into(),
            );
            return;
        }
        let expected = Source {
            ticket: work.ticket,
            epoch: arm.source.epoch,
            revision: arm.source.revision,
            owner: work.expected.owner,
            config_epoch: work.expected.epoch,
            config_revision: work.expected.revision,
        };
        let desired = arm.armed.clone();
        for (child, status) in report.additions.iter().zip(children) {
            if !desired.contains(&child.bus) {
                d.protocol_invalid = true;
            }
            let receipt = child.receipt.clone();
            if let Some(status) = status {
                if child.new_take && status.source != expected {
                    d.protocol_invalid = true;
                    Self::pending_take(&mut d, Some(expected), receipt);
                } else {
                    Self::retain(&mut d, receipt, status);
                }
            } else {
                Self::pending_take(&mut d, child.new_take.then_some(expected), receipt);
            }
        }
        // Retain children before releasing shared admission or acknowledging UI.
        let invalid = d.protocol_invalid;
        let work = d.operation.as_mut().unwrap();
        let failure = report.failure.as_ref().map_or_else(
            || "Inspect actual arming/readback and child outcomes.".into(),
            |e| format!("{e:?}"),
        );
        work.progress = if invalid {
            ui::Progress::Unconfirmed(
                "Arming child identity mismatch; retained for shutdown.".into(),
            )
        } else {
            match report.progress {
                ArmingProgress::Queued => ui::Progress::Queued,
                ArmingProgress::Adding | ArmingProgress::ApplyingRemovals => ui::Progress::Applying,
                ArmingProgress::Completed(ArmingOutcome::AppliedLocally) => {
                    ui::Progress::AppliedLocally
                }
                ArmingProgress::Completed(ArmingOutcome::NoOpLocally) => ui::Progress::NoOpLocally,
                ArmingProgress::Completed(ArmingOutcome::PartiallyApplied) => {
                    ui::Progress::PartiallyApplied(failure)
                }
                ArmingProgress::Completed(ArmingOutcome::Rejected) => {
                    ui::Progress::Rejected(failure)
                }
                ArmingProgress::Unconfirmed => ui::Progress::Unconfirmed(failure),
            }
        };
        work.terminal = !invalid
            && matches!(
                report.progress,
                ArmingProgress::Completed(_) | ArmingProgress::Unconfirmed
            );
        if work.terminal {
            work.completion = Some(report.configuration.clone());
        }
        work.arming.as_mut().unwrap().last = Some(report);
        let release = work.terminal.then(|| work.lease.clone());
        drop(d);
        if let Some(lease) = release {
            self.session.release_external(&lease);
        }
    }
    fn pending_take(d: &mut Data, expected: Option<Source>, receipt: Rc<dyn TakePoll>) {
        if !d.takes.iter().any(|t| same_take(&t.receipt, &receipt))
            && !d.orphans.iter().any(|(_, r)| same_take(r, &receipt))
        {
            // The backend bounds admitted children; never discard their handle.
            d.orphans.push((expected, receipt));
        }
    }
    pub(super) fn remember_arming(d: &mut Data) {
        if let Some(error) = d
            .operation
            .as_ref()
            .and_then(|o| o.arming.as_ref()?.last.as_ref())
            .and_then(arming_error)
        {
            if d.past_failures.len() < 32 {
                d.past_failures.push(error);
            }
        }
    }
    pub(super) fn arming_finalization_errors(
        &self,
        joined_failed: bool,
        errors: &mut Vec<String>,
    ) -> Option<()> {
        let pending = self.data.borrow().operation.as_ref().and_then(|o| {
            o.arming.as_ref()?.receipt.clone().map(|r| {
                (
                    o.ticket,
                    o.expected,
                    o.arming.as_ref().unwrap().source,
                    o.arming.as_ref().unwrap().membership,
                    r,
                )
            })
        });
        if let Some((ticket, expected, source, membership, receipt)) = pending {
            match receipt.report() {
                None if !joined_failed => return None,
                Some(r)
                    if r.ticket == ticket
                        && r.expected_config == expected
                        && r.source == source
                        && r.expected_recording == membership =>
                {
                    if let Some(error) = arming_error(&r) {
                        errors.push(error);
                    }
                }
                _ => {
                    errors.push("Arming diagnostic identity unavailable after backend join.".into())
                }
            }
        }
        Some(())
    }
}
fn arming_error(r: &ArmingStatus) -> Option<String> {
    (!matches!(
        r.progress,
        ArmingProgress::Completed(
            ArmingOutcome::AppliedLocally | ArmingOutcome::NoOpLocally | ArmingOutcome::Rejected
        )
    ) || r.failure.as_ref().is_some_and(|e| {
        !matches!(
            e,
            pecan_pipewire::recording::RecordingError::CancelledBeforeStart
        )
    }))
    .then(|| {
        format!(
            "Arming request {}: {:?}; failure {:?}; removed {:?}; withheld {:?}.",
            r.ticket, r.progress, r.failure, r.removed, r.withheld
        )
    })
}
