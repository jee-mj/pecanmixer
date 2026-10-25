use super::*;
impl BusOwner {
    pub(super) fn publish(&self) {
        let graph = self.session.graph();
        let witness = self.session.witnessed_graph();
        let closing = self.session.admission_closed();
        let busy = self.session.busy();
        let (publication, context, tracks, configuration) = {
            let d = self.data.borrow();
            let connected = !d.protocol_invalid
                && graph.as_ref().zip(d.config.as_ref()).is_some_and(|(g, c)| {
                    g.value.verbonden && c.connected && g.stamp.epoch == c.revision.epoch
                })
                && d.membership
                    .zip(d.config.as_ref())
                    .is_some_and(|(r, c)| r.owner == c.revision.owner);
            let coherent = coherent(&d);
            let configuration = connected
                .then(|| {
                    d.config
                        .as_ref()?
                        .configuration
                        .as_ref()
                        .map(|c| c.buses.clone())
                })
                .flatten()
                .filter(|_| coherent);
            // Timing updates belong to the retained transport presentation,
            // not edit authority. Do not rebind bus controls on every sample.
            let statuses: Vec<_> = d
                .takes
                .iter()
                .map(|t| {
                    let mut status = t.status.clone();
                    status.elapsed_at = None;
                    for track in &mut status.tracks {
                        track.elapsed = None;
                    }
                    status
                })
                .collect();
            let owned = statuses.iter().filter(|t| !t.complete).collect::<Vec<_>>();
            let activity = if d.unknown_owned || !d.orphans.is_empty() {
                ui::Activity::Unknown
            } else if owned
                .iter()
                .any(|t| t.progress == RecordingProgress::StartedLocally)
                || d.config.as_ref().is_some_and(|c| c.recording)
            {
                ui::Activity::Recording
            } else if owned.iter().any(|t| {
                matches!(
                    t.progress,
                    RecordingProgress::Finalizing
                        | RecordingProgress::Failed
                        | RecordingProgress::Unconfirmed
                )
            }) {
                ui::Activity::Finalizing
            } else if !owned.is_empty() {
                ui::Activity::Unknown
            } else if d.config.is_some() {
                ui::Activity::Idle
            } else {
                ui::Activity::Unknown
            };
            let operation = d
                .operation
                .as_ref()
                .map(|o| (o.request.clone(), o.progress.clone()));
            let error = d.error.clone();
            let publication = Publication {
                graph: graph.as_ref().map(|g| (g.stamp, Arc::as_ptr(&g.value))),
                config: d.config.clone(),
                membership: d.membership,
                root: d.root.clone(),
                operation,
                takes: statuses,
                closing,
                activity,
                error,
                busy,
                invalid: d.protocol_invalid,
                level_witness: witness.is_some(),
            };
            if d.publication.as_ref() == Some(&publication) {
                return;
            }
            let tracks: Vec<_> = publication
                .takes
                .iter()
                .flat_map(|t| t.tracks.clone())
                .collect();
            let mut targets: BTreeMap<String, Vec<TrackKey>> = BTreeMap::new();
            for take in publication.takes.iter().filter(|t| !t.complete) {
                for track in &take.tracks {
                    if !matches!(
                        track.phase,
                        ui::TrackPhase::Joined(_)
                            | ui::TrackPhase::Finalized(_)
                            | ui::TrackPhase::NotOpened(_)
                    ) {
                        targets
                            .entry(track.bus_key.clone())
                            .or_default()
                            .push(track.identity);
                    }
                }
            }
            let context = Origin(Rc::new(Context {
                owner: self.authority.clone(),
                graph: graph.clone(),
                witness: witness.clone(),
                config: d.config.clone(),
                membership: d.membership,
                root: d.root.clone(),
                connected,
                tracks: targets,
            }));
            (publication, context, tracks, configuration)
        };
        let definitions = configuration
            .as_ref()
            .and_then(|c| c.definitions().ok())
            .unwrap_or_default();
        let model = Arc::new(
            graph
                .as_ref()
                .map_or_else(Mixer::default, |g| Mixer::build(&g.value, &definitions)),
        );
        let mut capabilities = ui::Capabilities::default();
        let levels = self.level_support();
        capabilities.idle_levels = levels.clone();
        capabilities.active_levels = levels;
        capabilities.idle_configuration = if self.service.is_none() {
            ui::Support::Unavailable("No recording/configuration endpoint is attached.".into())
        } else if busy && self.data.borrow().operation.is_none() {
            ui::Support::Unavailable("A shared graph operation/gesture is pending.".into())
        } else {
            ui::Support::Available
        };
        if publication.activity == ui::Activity::Recording
            && self.service.as_ref().is_some_and(|s| s.supports_arming())
        {
            let d = self.data.borrow();
            capabilities.active_arming = if d.root.is_none()
                || d.all_stopped
                || d.unknown_owned
                || !d.orphans.is_empty()
                || d.takes.iter().any(|t| {
                    !t.status.complete
                        && matches!(
                            t.status.progress,
                            RecordingProgress::Queued | RecordingProgress::Opening
                        )
                }) {
                ui::Support::Unavailable("Wait for active recording readback and an explicit directory; Stop all remains available.".into())
            } else {
                ui::Support::Available
            };
        }
        if publication.activity == ui::Activity::Idle
            && self
                .service
                .as_ref()
                .is_some_and(|s| s.supports_migration())
        {
            capabilities.routed_layout = ui::Support::Available;
            capabilities.shrink_layout = ui::Support::Available;
            // routed_add deliberately retains its separate pending gate.
        }
        let routes = self.has_routes(&context);
        let operation = publication.operation.as_ref().map(|(r, p)| ui::Operation {
            request: r.clone(),
            progress: p.clone(),
        });
        let snapshot = ui::Snapshot {
            origin: context.clone(),
            connected: graph.as_ref().is_some_and(|g| g.value.verbonden),
            closing,
            configuration,
            mixer: model,
            default_sink: graph.as_ref().and_then(|g| g.value.standaard_sink.clone()),
            has_bus_routes: routes,
            activity: publication.activity,
            tracks,
            operation,
            capabilities,
        };
        {
            let mut d = self.data.borrow_mut();
            d.current = Some(context);
            d.publication = Some(publication);
        }
        if let Err(error) = self.view.observe(snapshot) {
            self.data.borrow_mut().error = Some(format!(
                "Bus presentation rejected: {error:?}; receipts remain owned."
            ));
        }
        self.render_controls();
    }
    pub(super) fn render_controls(&self) {
        let focus = self
            .root
            .root()
            .and_then(|r| r.focus())
            .and_downcast::<gtk::Button>()
            .filter(|b| b.is_ancestor(&self.tools))
            .and_then(|b| b.label())
            .map(|l| l.to_string());
        self.input.retire();
        for live in self.controls.borrow_mut().drain(..) {
            live.set(false);
        }
        while let Some(child) = self.tools.first_child() {
            self.tools.remove(&child);
        }
        let Some(origin) = self.origin() else { return };
        let (message, statuses, config_pending) = {
            let d = self.data.borrow();
            (
            d.error.clone().unwrap_or_else(||if self.service.is_none(){"No synthetic/native bus endpoint attached; configuration unavailable.".into()}else if d.root.is_none(){"Choose an explicit recording directory to enable starts. Stops use retained identities; local config completion is not observed ports.".into()}else{"Recording status reflects local attachment/finalization; only retained joined evidence completes a take.".into()}),
            d.takes.iter().map(|t|t.status.clone()).collect::<Vec<_>>(),!coherent(&d)||d.operation.as_ref().is_some_and(|o|!o.terminal||matches!(o.progress,ui::Progress::PartiallyApplied(_)|ui::Progress::Unconfirmed(_))))
        };
        self.status.set_text(&message);
        // Keep a focus receiver during cancelled held input. The immutable
        // callback and window guard block activation until the real release.
        let enabled = !self.session.admission_closed();
        let transition = {
            let d = self.data.borrow();
            d.unknown_owned
                || !d.orphans.is_empty()
                || d.takes.iter().any(|t| {
                    !t.status.complete
                        && matches!(
                            t.status.progress,
                            RecordingProgress::Queued | RecordingProgress::Opening
                        )
                })
        };
        let start = enabled
            && origin.0.connected
            && origin.0.root.is_some()
            && origin
                .0
                .config
                .as_ref()
                .is_some_and(|c| c.configuration.is_some())
            && !self.session.busy()
            && !config_pending
            && !transition;
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        self.tools.append(&buttons);
        self.button(&buttons, "Record all armed", start, {
            let o = origin.clone();
            move |owner| owner.start(&o, None)
        });
        self.button(
            &buttons,
            "Stop all",
            enabled && self.service.is_some(),
            |owner| owner.stop_all(),
        );
        if origin
            .0
            .config
            .as_ref()
            .is_some_and(|c| c.configuration.is_none())
        {
            self.button(
                &buttons,
                "Initialize native bus defaults",
                enabled && origin.0.connected,
                {
                    let o = origin.clone();
                    move |owner| owner.initialize(&o).map(|_| ())
                },
            );
        }
        if let Some(config) = origin
            .0
            .config
            .as_ref()
            .and_then(|c| c.configuration.as_ref())
        {
            for bus in config.buses.definitions().unwrap_or_default() {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                self.tools.append(&row);
                self.button(&row, &format!("Record {}", bus.name), start, {
                    let o = origin.clone();
                    let key = bus.key.clone();
                    move |owner| owner.start(&o, Some(key.clone()))
                });
                self.button(
                    &row,
                    &format!("Stop {}", bus.name),
                    enabled && origin.0.tracks.get(&bus.key).is_some_and(|t| !t.is_empty()),
                    {
                        let o = origin.clone();
                        let key = bus.key.clone();
                        move |owner| owner.stop_bus(&o, &key)
                    },
                );
            }
        }
        for status in statuses {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let name = status
                .directory
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "New recording".into());
            let label = gtk::Label::builder()
                .label(format!(
                    "{}: {:?}; writer {:?} {:?}; failure {:?}",
                    name, status.progress, status.writer, status.writer_outcome, status.failure
                ))
                .xalign(0.)
                .wrap(true)
                .build();
            row.append(&label);
            if let Some(take) = status.take {
                self.button(
                    &row,
                    &format!("Stop take {name}"),
                    enabled && !status.complete,
                    move |owner| owner.stop_take(take),
                );
            }
            self.tools.append(&row);
        }
        if let Some(focus) = focus {
            let mut pending = vec![self.tools.clone().upcast::<gtk::Widget>()];
            let mut fallback = None;
            while let Some(widget) = pending.pop() {
                if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                    if button.is_sensitive() && button.label().as_deref() == Some(focus.as_str()) {
                        button.grab_focus();
                        fallback = None;
                        break;
                    }
                    if button.is_sensitive() && button.label().as_deref() == Some("Stop all") {
                        fallback = Some(button.clone());
                    }
                }
                let mut child = widget.first_child();
                while let Some(current) = child {
                    child = current.next_sibling();
                    pending.push(current);
                }
            }
            if let Some(button) = fallback {
                button.grab_focus();
            }
        }
    }
    fn button(
        &self,
        parent: &gtk::Box,
        label: &str,
        enabled: bool,
        action: impl Fn(&BusOwner) -> Result<(), Error> + 'static,
    ) {
        let button = gtk::Button::with_label(label);
        button.set_sensitive(enabled);
        let live = Rc::new(Cell::new(true));
        self.input.capture(&button, live.clone());
        self.controls.borrow_mut().push(live.clone());
        let weak = self.weak.clone();
        button.connect_clicked(move |button| {
            if let Some(owner) = weak.upgrade() {
                if !live.get() || !button.is_sensitive() || owner.input.blocked() {
                    return;
                }
                if let Err(error) = action(&owner) {
                    owner.data.borrow_mut().error = Some(error.to_string());
                }
                owner.poll();
            }
        });
        parent.append(&button);
    }
}
pub(super) fn coherent(d: &Data) -> bool {
    d.operation.as_ref().is_none_or(|work| {
        !work.reconcile_requested
            && (work.levels.is_some() && !work.terminal
                || work.completion.as_ref().is_none_or(|completed| {
                    d.config
                        .as_ref()
                        .is_some_and(|now| revision_covers(now.revision, completed.revision))
                }))
    })
}
