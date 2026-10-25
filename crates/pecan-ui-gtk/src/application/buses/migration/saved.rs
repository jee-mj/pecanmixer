//! Automatic saved intent uses the existing migration receipt and publication
//! barriers. No derived edits, preference writes, history or retry sequencer.
use super::*;
use pecan_pipewire::bus_migration::SavedLayoutIntent;

impl Work {
    pub(super) fn saved_readback_matches(&self, snapshot: &ConfigSnapshot) -> bool {
        if self.saved_restore.is_none() {
            return true;
        }
        let Ok(Opdracht::ZetBus { kanalen, gain }) = self.proposed.buses.backend_config() else {
            return false;
        };
        let Ok(armed) = self.proposed.buses.armed() else {
            return false;
        };
        let expected = pecan_core::graph::Opties {
            bussen: armed
                .into_iter()
                .map(|b| pecan_core::graph::BusOpname {
                    naam: b.file,
                    kanalen: b.channels,
                })
                .collect(),
            midi: false,
            klok: None,
            start_stop: false,
        };
        snapshot.connected
            && !snapshot.recording
            && snapshot.local.as_ref()
                == Some(&pecan_pipewire::bus_config::BusReadback {
                    node: Some(self.proof.node),
                    channels: kanalen,
                    gains: gain,
                })
            && snapshot.transport.as_ref() == Some(&expected)
    }
}

impl BusOwner {
    pub(in crate::application::buses) fn submit_saved_restore(
        &self,
        origin: &Origin,
        desired: IdleBusConfig,
    ) -> Result<(), Error> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| Self::error("No endpoint."))?;
        let frame = origin
            .0
            .witness
            .as_ref()
            .ok_or_else(|| Self::error("No original witness."))?;
        let config = origin
            .0
            .config
            .as_ref()
            .ok_or_else(|| Self::error("No configuration."))?;
        let membership = origin
            .0
            .membership
            .ok_or_else(|| Self::error("No membership authority."))?;
        let local = config
            .local
            .as_ref()
            .ok_or_else(|| Self::error("No local readback."))?;
        let node = local
            .node
            .ok_or_else(|| Self::error("No local Rec filter."))?;
        levels::Binding::capture(frame, config).map_err(Error::Owner)?;
        let graph = &frame.graph().value;
        let owned = graph
            .knopen
            .iter()
            .find(|n| n.id == node)
            .ok_or_else(|| Self::error("Original Rec node is absent."))?;
        let ports: Vec<_> = (1..=local.channels)
            .map(|i| {
                let name = format!("rec_{i}");
                let p = graph.poorten.iter().find(|p| {
                    p.knoop == node && p.ingang && p.soort == Soort::Audio && p.naam == name
                })?;
                Some((p.id, name))
            })
            .collect::<Option<_>>()
            .ok_or_else(|| Self::error("Incomplete original prefix."))?;
        let pairs: BTreeSet<_> = graph
            .kabels
            .iter()
            .filter(|l| l.in_knoop == node)
            .map(|l| (l.uit_poort, l.in_poort))
            .collect();
        let mut ids: BTreeSet<_> = ports
            .iter()
            .map(|p| p.0)
            .chain(std::iter::once(node))
            .collect();
        for (source, input) in &pairs {
            if !ports.iter().any(|p| p.0 == *input) {
                return Err(Self::error("Route is outside the captured audio prefix."));
            }
            let p = graph
                .poorten
                .iter()
                .find(|p| p.id == *source && !p.ingang && p.soort == Soort::Audio)
                .ok_or_else(|| Self::error("Original source endpoint is absent."))?;
            ids.extend([p.id, p.knoop]);
        }
        let binding = frame
            .bind_objects(ids)
            .ok_or_else(|| Self::error("Original generations are unavailable."))?;
        let fresh = service
            .read()
            .ok_or_else(|| Self::error("Inspection is busy."))?;
        // Queued/opening ownership precedes the filter's recording flag. Do
        // not defer its epoch skip until a later owner tick after it may join.
        // Poll each injected reader once, outside any mutable UI borrow.
        let unfinished = fresh
            .takes
            .iter()
            .any(|r| r.report().is_none_or(|s| !s.complete));
        if (fresh.config.recording || unfinished)
            && fresh.config.revision.owner == config.revision.owner
            && fresh.revision.owner == config.revision.owner
            && fresh.config.revision.epoch == config.revision.epoch
        {
            self.skip_recording_restore_epoch(fresh.config.revision);
        }
        if fresh.config != *config
            || fresh.revision != membership
            || fresh.config.recording
            || unfinished
            || self.recording_blocks_restore()
            || self.restore.borrow().attempted.is_some_and(|r| {
                r.owner != config.revision.owner || r.epoch >= config.revision.epoch
            })
        {
            return Err(Self::error(
                "Saved configuration no longer has captured idle authority.",
            ));
        }
        self.validate(origin)?;
        self.idle()?;
        let (ticket, lease) = self.session.reserve_external().map_err(Error::Owner)?;
        if let Err(error) = self.validate(origin) {
            self.session.release_external(&lease);
            return Err(error);
        }
        self.restore.borrow_mut().attempted = Some(config.revision);
        let request = Request(Rc::new(()));
        self.data.borrow_mut().operation = Some(ConfigWork {
            restore: None,
            levels: None,
            arming: None,
            migration: Some(Work {
                source: frame.graph().stamp,
                membership,
                proposed: desired.clone(),
                proof: Postcondition {
                    binding,
                    node,
                    serial: owned.serial,
                    ports,
                    pairs,
                },
                append: desired.buses.widths.iter().sum::<usize>() > local.channels,
                saved_restore: Some(frame.clone()),
                admission_error: None,
                observed_inputs: None,
                receipt: None,
                last: None,
                publication: None,
                released: false,
            }),
            request: request.clone(),
            ticket,
            expected: config.revision,
            lease,
            receipt: None,
            progress: ui::Progress::Queued,
            terminal: false,
            completion: None,
            reconcile_requested: false,
        });
        self.restore_message("Restoring saved Rec layout; waiting for terminal evidence, fresh authenticated inputs/readback and accepted presentation.");
        // Store ownership before calling the endpoint. Reentrant readers see
        // the same request/epoch latch; no UI borrow crosses admission.
        let admitted = service.restore_saved_layout(SavedLayoutIntent {
            ticket,
            origin: frame.clone(),
            expected_config: config.revision,
            expected_recording: membership,
            configuration: desired,
        });
        let mut d = self.data.borrow_mut();
        if let Some(m) = d
            .operation
            .as_mut()
            .filter(|w| w.request == request)
            .and_then(|w| w.migration.as_mut())
        {
            match admitted {
                Ok(receipt) => m.receipt = Some(receipt),
                Err(error) => m.admission_error = Some(error.to_string()),
            }
        }
        drop(d);
        self.publish();
        Ok(())
    }

    pub(super) fn poll_saved_rejection(&self, inspected: Option<&ConfigSnapshot>) {
        let Some(fresh) = inspected else { return };
        let mut d = self.data.borrow_mut();
        let Some(w) = d.operation.as_mut().filter(|w| !w.terminal) else {
            return;
        };
        let Some(m) = w.migration.as_mut().filter(|m| m.saved_restore.is_some()) else {
            return;
        };
        let Some(error) = m.admission_error.as_ref() else {
            return;
        };
        if fresh.revision.owner != w.expected.owner {
            return;
        }
        // Admission rejection is not a manufactured native completion. Keep
        // its error until a post-admission inspection is actually presented.
        w.progress =
            ui::Progress::Rejected(format!("Saved Rec admission: {error}; no automatic retry."));
        w.completion = Some(fresh.clone());
        w.terminal = true;
        m.publication = Some(fresh.clone());
    }
}
