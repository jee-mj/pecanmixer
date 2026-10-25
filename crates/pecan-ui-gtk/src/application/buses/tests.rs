//! Controlled UI-side projections only. No native constructor, capture, writer
//! file, real tray or user directory is used.
mod reconciliation;
mod routing_owner;
use super::*;
use crate::application::{ApplicationOwner, ApplicationShell, FixtureDriver, ShellOptions};
use pecan_core::{domain::history::Action as GraphAction, graph::Momentopname};
use pecan_pipewire::{
    bus_config::{BusReadback, ConfigCompletion, ConfigError},
    opname::{WriterOutcome, WriterPhase, WriterSendError},
    recording::RecordingError,
};
use service::*;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

struct Config {
    report: RefCell<Option<ConfigReport>>,
    hook: RefCell<Option<Box<dyn Fn()>>>,
}
impl ConfigPoll for Config {
    fn report(&self) -> Option<ConfigReport> {
        let report = self.report.borrow().clone();
        let hook = self.hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        report
    }
}
struct Take(RefCell<Option<TakeStatus>>);
impl TakePoll for Take {
    fn report(&self) -> Option<TakeStatus> {
        self.0.borrow().clone()
    }
}
struct Fake {
    config: RefCell<ConfigSnapshot>,
    membership: Cell<RecordingRevision>,
    busy: Cell<bool>,
    visible: Cell<bool>,
    no_op: Cell<bool>,
    configs: RefCell<Vec<(IdleConfigIntent, Rc<Config>)>>,
    starts: RefCell<Vec<(RecordingStart, Option<RecordingRevision>, Option<String>)>>,
    takes: RefCell<Vec<Rc<Take>>>,
    stops: RefCell<Vec<Stop>>,
    hook: RefCell<Option<Box<dyn Fn()>>>,
    read_hook: RefCell<Option<Box<dyn Fn()>>>,
}
impl Fake {
    fn new(epoch: u64) -> Rc<Self> {
        Rc::new(Self {
            config: RefCell::new(ConfigSnapshot {
                revision: ConfigRevision {
                    owner: 700,
                    epoch,
                    revision: 1,
                },
                connected: true,
                local: Some(BusReadback {
                    node: Some(30),
                    channels: 2,
                    gains: vec![1., 1.],
                }),
                configuration: Some(super::super::default_idle_configuration()),
                transport: None,
                recording: false,
            }),
            membership: Cell::new(RecordingRevision {
                owner: 700,
                revision: 1,
            }),
            busy: Cell::new(false),
            visible: Cell::new(true),
            no_op: Cell::new(false),
            configs: RefCell::default(),
            starts: RefCell::default(),
            takes: RefCell::default(),
            stops: RefCell::default(),
            hook: RefCell::default(),
            read_hook: RefCell::default(),
        })
    }
    fn admit(
        &self,
        start: RecordingStart,
        membership: Option<RecordingRevision>,
        bus: Option<String>,
    ) -> Result<Started, Error> {
        self.starts
            .borrow_mut()
            .push((start.clone(), membership, bus));
        if self.no_op.replace(false) {
            return Ok(Started {
                receipt: self.takes.borrow()[0].clone(),
                no_op: true,
            });
        }
        let source = Source::new(start.ticket, &start.origin, start.expected_config);
        let take = Rc::new(Take(RefCell::new(Some(TakeStatus {
            source,
            progress: RecordingProgress::Queued,
            writer: None,
            writer_outcome: None,
            failure: None,
            complete: false,
            take: None,
            directory: None,
            tracks: vec![],
        }))));
        self.takes.borrow_mut().push(take.clone());
        let mut revision = self.membership.get();
        revision.revision += 1;
        self.membership.set(revision);
        Ok(Started {
            receipt: take,
            no_op: false,
        })
    }
    fn started(&self, index: usize, id: u64) {
        let take = self.takes.borrow()[index].clone();
        let mut value = take.0.borrow_mut();
        let r = value.as_mut().unwrap();
        r.progress = RecordingProgress::StartedLocally;
        r.writer = Some(WriterPhase::Ready);
        r.take = Some(TakeKey::Synthetic(id));
        r.directory = Some(std::env::temp_dir().join(format!("pecan-synthetic-take-{id}")));
        r.tracks = vec![ui::Track {
            identity: TrackKey::Synthetic(id),
            bus_key: "rec-1-2".into(),
            phase: ui::TrackPhase::Recording,
            elapsed: None,
        }];
    }
    fn joined(&self, index: usize, failure: Option<RecordingError>) {
        let take = self.takes.borrow()[index].clone();
        let mut value = take.0.borrow_mut();
        let r = value.as_mut().unwrap();
        r.complete = true;
        r.progress = if failure.is_some() {
            RecordingProgress::Failed
        } else {
            RecordingProgress::JoinedSuccess
        };
        r.failure = failure;
        r.writer = Some(WriterPhase::Joined);
        r.writer_outcome = Some(WriterOutcome::Finalized);
        for track in &mut r.tracks {
            track.phase = ui::TrackPhase::Joined(Ok(()));
        }
    }
    fn complete_config(
        &self,
        index: usize,
        outcome: LocalConfigOutcome,
        publish: bool,
    ) -> ConfigReport {
        let (intent, receipt) = self.configs.borrow()[index].clone();
        let mut snapshot = self.config.borrow().clone();
        snapshot.revision.revision += 1;
        snapshot.configuration = Some(intent.configuration.clone());
        if matches!(outcome, LocalConfigOutcome::PartiallyApplied(_)) {
            snapshot.configuration = None;
        }
        if publish {
            *self.config.borrow_mut() = snapshot.clone();
        }
        let report = ConfigReport {
            ticket: intent.ticket,
            expected: intent.expected,
            progress: ConfigProgress::Completed(ConfigCompletion {
                outcome,
                snapshot,
                resize: None,
                cancellation_requested: false,
            }),
        };
        *receipt.report.borrow_mut() = Some(report.clone());
        report
    }
}
impl Service for Fake {
    fn read(&self) -> Option<Readback> {
        if self.busy.get() {
            return None;
        }
        let takes = if self.visible.get() {
            self.takes
                .borrow()
                .iter()
                .rev()
                .filter(|t| t.0.borrow().as_ref().is_none_or(|r| !r.complete))
                .chain(
                    self.takes
                        .borrow()
                        .iter()
                        .rev()
                        .filter(|t| t.0.borrow().as_ref().is_some_and(|r| r.complete))
                        .take(16),
                )
                .cloned()
                .map(|r| r as Rc<dyn TakePoll>)
                .collect()
        } else {
            vec![]
        };
        let readback = Readback {
            revision: self.membership.get(),
            config: self.config.borrow().clone(),
            takes,
        };
        let hook = self.read_hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        Some(readback)
    }
    fn configure(&self, intent: IdleConfigIntent) -> Result<Rc<dyn ConfigPoll>, Error> {
        let receipt = Rc::new(Config {
            report: RefCell::new(Some(ConfigReport {
                ticket: intent.ticket,
                expected: intent.expected,
                progress: ConfigProgress::Queued,
            })),
            hook: RefCell::default(),
        });
        self.configs.borrow_mut().push((intent, receipt.clone()));
        let hook = self.hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        Ok(receipt)
    }
    fn start_all(&self, start: RecordingStart) -> Result<Started, Error> {
        self.admit(start, None, None)
    }
    fn start_bus(&self, start: RecordingBusStart) -> Result<Started, Error> {
        self.admit(start.start, Some(start.expected_recording), Some(start.bus))
    }
    fn stop(&self, target: Stop) -> Result<Vec<Rc<dyn TakePoll>>, Error> {
        self.stops.borrow_mut().push(target);
        Ok(self
            .takes
            .borrow()
            .iter()
            .filter(|take| match target {
                Stop::All => true,
                Stop::Take(id) => take.0.borrow().as_ref().is_some_and(|r| r.take == Some(id)),
                Stop::Track(id) => take
                    .0
                    .borrow()
                    .as_ref()
                    .is_some_and(|r| r.tracks.iter().any(|t| t.identity == id)),
            })
            .cloned()
            .map(|r| r as Rc<dyn TakePoll>)
            .collect())
    }
}
fn graph() -> Momentopname {
    let mut g: Momentopname = serde_json::from_str(include_str!(
        "../../../../../fixtures/graph/audio-basic.json"
    ))
    .unwrap();
    g.kabels.clear();
    g
}
fn generations(g: &Momentopname) -> BTreeMap<u32, u64> {
    g.knopen
        .iter()
        .map(|n| n.id)
        .chain(g.poorten.iter().map(|p| p.id))
        .enumerate()
        .map(|(i, id)| (id, i as u64 + 1))
        .collect()
}
fn setup() -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver) {
    let owner = ApplicationOwner::fixture();
    let session = owner.activate();
    let d = owner.fixture_driver().unwrap();
    let g = graph();
    let origin = d.publish(g.clone(), generations(&g));
    session.poll();
    let fake = Fake::new(origin.stamp.epoch);
    let buses = BusOwner::new(session, Some(fake.clone()));
    buses
        .set_recording_root(Some(
            std::env::temp_dir().join("pecan-bus-synthetic-recording-root"),
        ))
        .unwrap();
    (buses, fake, d)
}
fn arm(owner: &BusOwner) -> Request {
    let origin = owner.origin().unwrap();
    let buses = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .arm("rec-1-2", true)
        .unwrap();
    owner
        .configure(
            &origin,
            ui::Action::Arm {
                bus: "rec-1-2".into(),
                additive: true,
            },
            Some(buses),
        )
        .unwrap()
}
fn button(root: &impl IsA<gtk::Widget>, label: &str) -> gtk::Button {
    let mut stack = vec![root.as_ref().clone()];
    while let Some(widget) = stack.pop() {
        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
            if button.label().as_deref() == Some(label) {
                return button.clone();
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            stack.push(current);
        }
    }
    panic!("missing synthetic button {label}")
}
#[gtk::test]
fn configuration_uses_captured_revision_shared_ticket_and_retains_lease_until_terminal() {
    let (owner, fake, _) = setup();
    let old = owner.origin().unwrap();
    let request = arm(&owner);
    owner.poll();
    assert!(owner.session.busy());
    assert_eq!(
        fake.configs.borrow()[0].0.expected,
        old.0.config.as_ref().unwrap().revision
    );
    assert!(owner
        .session
        .execute(
            old.0.graph.as_ref().unwrap(),
            "Blocked",
            GraphAction::Mute {
                node: 20,
                muted: true
            }
        )
        .is_err());
    *fake.configs.borrow()[0].1.report.borrow_mut() = None;
    owner.poll();
    assert!(owner.session.busy());
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    assert!(!owner.session.busy());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert!(owner.configure(&old, ui::Action::AddStereo, None).is_err());
    owner
        .start(&owner.origin().unwrap(), Some("rec-1-2".into()))
        .unwrap();
    assert!(fake.starts.borrow()[0].0.ticket > fake.configs.borrow()[0].0.ticket);
}
#[gtk::test]
fn actual_bus_handoff_keeps_correlation_through_pending_reentrant_and_terminal_reports() {
    let (owner, fake, _) = setup();
    let weak = Rc::downgrade(&owner);
    let service = fake.clone();
    *fake.hook.borrow_mut() = Some(Box::new(move || {
        service.complete_config(0, LocalConfigOutcome::Applied, true);
        weak.upgrade().unwrap().poll();
    }));
    button(owner.view.widget(), "Disarm").emit_clicked();
    let request = owner
        .view
        .awaited_request()
        .expect("actual handoff retained");
    assert!(owner.session.busy());
    owner.poll();
    assert!(owner.view.awaited_request().is_none());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert!(!owner.session.busy());
}
#[gtk::test]
fn delayed_old_config_poll_cannot_deliver_into_a_reentrant_new_operation() {
    let (owner, fake, _) = setup();
    arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    let weak = Rc::downgrade(&owner);
    *fake.configs.borrow()[0].1.hook.borrow_mut() = Some(Box::new(move || {
        arm(&weak.upgrade().unwrap());
    }));
    owner.poll();
    assert_eq!(fake.configs.borrow().len(), 2);
    assert!(owner.session.busy());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::Queued
    );
}
#[gtk::test]
fn terminal_readback_never_restamps_old_result_and_partial_metadata_needs_explicit_recovery() {
    let (owner, fake, _) = setup();
    arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, false);
    owner.poll();
    assert!(!owner.session.busy());
    assert!(arm_result(&owner).is_err());
    let completed = owner
        .data
        .borrow()
        .operation
        .as_ref()
        .unwrap()
        .completion
        .clone()
        .unwrap();
    *fake.config.borrow_mut() = completed;
    owner.poll();
    arm(&owner);
    fake.complete_config(
        1,
        LocalConfigOutcome::PartiallyApplied(ConfigError::ResizeFailed),
        true,
    );
    owner.poll();
    assert!(owner.configuration_report().is_some());
    assert!(arm_result(&owner).is_err());
    owner.refresh();
    assert!(owner
        .origin()
        .unwrap()
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .is_none());
}
fn arm_result(owner: &BusOwner) -> Result<Request, Error> {
    let origin = owner.origin().unwrap();
    owner.configure(
        &origin,
        ui::Action::Arm {
            bus: "rec-1-2".into(),
            additive: true,
        },
        Some(super::super::default_idle_configuration().buses),
    )
}
#[gtk::test]
fn per_bus_start_keeps_both_revisions_and_no_op_receipt_is_not_a_new_take() {
    let (owner, fake, _) = setup();
    let origin = owner.origin().unwrap();
    owner.start(&origin, Some("rec-1-2".into())).unwrap();
    let call = fake.starts.borrow()[0].clone();
    assert_eq!(call.1, origin.0.membership);
    assert_eq!(
        call.0.expected_config,
        origin.0.config.as_ref().unwrap().revision
    );
    assert!(Arc::ptr_eq(
        &call.0.origin.value,
        &origin.0.graph.as_ref().unwrap().value
    ));
    assert_eq!(
        owner.recording_status()[0].progress,
        RecordingProgress::Queued
    );
    assert!(owner.stop_bus(&origin, "rec-1-2").is_err());
    owner.stop_all().unwrap();
    owner.stop_all().unwrap();
    assert_eq!(fake.stops.borrow().len(), 1);
    fake.started(0, 11);
    owner.poll();
    fake.no_op.set(true);
    owner
        .start(&owner.origin().unwrap(), Some("rec-1-2".into()))
        .unwrap();
    assert_eq!(owner.recording_status().len(), 1);
    assert_eq!(fake.takes.borrow().len(), 1);
}
#[gtk::test]
fn stale_membership_and_directory_handoffs_are_rejected_without_restamping() {
    let (owner, fake, _) = setup();
    let old = owner.origin().unwrap();
    let mut rev = fake.membership.get();
    rev.revision += 1;
    fake.membership.set(rev);
    assert_eq!(
        owner.start(&old, Some("rec-1-2".into())),
        Err(Error::Recording(RecordingError::StaleRecording))
    );
    assert!(fake.starts.borrow().is_empty());
    owner.poll();
    let old = owner.origin().unwrap();
    owner
        .set_recording_root(Some(
            std::env::temp_dir().join("pecan-another-synthetic-root"),
        ))
        .unwrap();
    assert!(owner.start(&old, None).is_err());
}
#[gtk::test]
fn original_stable_stop_ids_survive_disconnect_and_never_target_a_new_bus_track() {
    let (owner, fake, d) = setup();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 11);
    owner.poll();
    let captured = owner.origin().unwrap();
    d.publish(Momentopname::default(), BTreeMap::new());
    owner.session.poll();
    fake.config.borrow_mut().connected = false;
    fake.visible.set(false);
    owner.poll();
    assert_eq!(owner.recording_status().len(), 1);
    // A replacement publication cannot change the original stop's target set.
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .tracks[0]
        .identity = TrackKey::Synthetic(99);
    owner.poll();
    owner.stop_bus(&captured, "rec-1-2").unwrap();
    assert_eq!(
        fake.stops.borrow().last(),
        Some(&Stop::Track(TrackKey::Synthetic(11)))
    );
    owner.stop_take(TakeKey::Synthetic(11)).unwrap();
    assert_eq!(
        fake.stops.borrow().last(),
        Some(&Stop::Take(TakeKey::Synthetic(11)))
    );
}
#[gtk::test]
fn omitted_snapshots_and_busy_receipts_cannot_drop_live_ownership_or_make_edits_idle() {
    let (owner, fake, _) = setup();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 11);
    owner.poll();
    let receipt = fake.takes.borrow()[0].clone();
    let status = receipt.0.borrow_mut().take().unwrap();
    fake.visible.set(false);
    fake.busy.set(true);
    for _ in 0..100 {
        owner.poll();
    }
    assert_eq!(owner.recording_status().len(), 1);
    assert!(arm_result(&owner).is_err());
    owner.stop_all().unwrap();
    owner.stop_all().unwrap();
    assert_eq!(fake.stops.borrow().len(), 1);
    assert!(owner.data.borrow().orphans.len() <= 1);
    *receipt.0.borrow_mut() = Some(status);
    fake.busy.set(false);
    owner.poll();
    assert_eq!(owner.recording_status().len(), 1);
}
#[gtk::test]
fn complete_history_is_bounded_without_losing_evicted_ingress_failure() {
    let (owner, fake, _) = setup();
    for i in 0..100 {
        owner.start(&owner.origin().unwrap(), None).unwrap();
        fake.started(i, i as u64 + 1);
        fake.joined(
            i,
            (i == 0).then_some(RecordingError::Ingress(WriterSendError::Full)),
        );
        owner.poll();
    }
    assert!(owner.data.borrow().takes.len() <= 16);
    assert!(owner.data.borrow().retired.len() <= 80);
    assert!(owner
        .finalization_errors(false)
        .unwrap()
        .iter()
        .any(|e| e.contains("Ingress")));
}
#[gtk::test]
fn retired_record_controls_cannot_adopt_a_new_directory_or_membership() {
    let (owner, fake, _) = setup();
    let old = button(owner.widget(), "Record all armed");
    owner
        .set_recording_root(Some(
            std::env::temp_dir().join("pecan-new-synthetic-selection"),
        ))
        .unwrap();
    old.emit_clicked();
    assert!(fake.starts.borrow().is_empty());
    let fresh = button(owner.widget(), "Record all armed");
    fresh.emit_clicked();
    assert_eq!(fake.starts.borrow().len(), 1);
    fresh.emit_clicked();
    assert_eq!(fake.starts.borrow().len(), 1);
}
#[gtk::test]
fn direct_coupling_action_cannot_relabel_the_opposite_core_operation() {
    let (owner, fake, _) = setup();
    let origin = owner.origin().unwrap();
    let proposed = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .change_coupling(0)
        .unwrap();
    assert!(owner
        .configure(
            &origin,
            ui::Action::Join {
                bus: "rec-1-2".into()
            },
            Some(proposed.clone())
        )
        .is_err());
    assert!(fake.configs.borrow().is_empty());
    owner
        .configure(
            &origin,
            ui::Action::Split {
                bus: "rec-1-2".into(),
            },
            Some(proposed),
        )
        .unwrap();
    assert_eq!(fake.configs.borrow().len(), 1);
}
#[gtk::test]
fn unconfirmed_config_requires_explicit_fresh_reconciliation_before_recording() {
    let (owner, fake, _) = setup();
    arm(&owner);
    let receipt = fake.configs.borrow()[0].1.clone();
    receipt.report.borrow_mut().as_mut().unwrap().progress = ConfigProgress::Unconfirmed;
    owner.poll();
    assert!(!owner.session.busy());
    assert!(owner.start(&owner.origin().unwrap(), None).is_err());
    assert!(fake.starts.borrow().is_empty());
    owner.refresh();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    assert_eq!(fake.starts.borrow().len(), 1);
}
#[gtk::test]
fn changed_receipt_identity_is_fail_closed_without_unbounded_alias_retention() {
    let (owner, fake, _) = setup();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 11);
    owner.poll();
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .source
        .ticket += 100;
    for _ in 0..100 {
        owner.poll();
    }
    assert!(owner.data.borrow().protocol_invalid);
    assert!(owner.data.borrow().orphans.len() <= 1);
    assert!(owner.start(&owner.origin().unwrap(), None).is_err());
    owner.stop_all().unwrap();
    assert!(owner
        .finalization_errors(false)
        .unwrap()
        .iter()
        .any(|e| e.contains("identity")));
}
#[gtk::test]
fn initially_busy_configuration_does_not_hide_the_connected_graph_or_output_choices() {
    let (existing, fake, _) = setup();
    fake.busy.set(true);
    let owner = BusOwner::new(existing.session.clone(), Some(fake.clone()));
    let origin = owner.origin().unwrap();
    assert!(owner.session.graph().unwrap().value.verbonden);
    assert!(origin.0.config.is_none());
    assert!(owner.view.selected_output_key().is_some());
    assert!(owner.validate_selection(&origin).is_ok());
    assert!(owner.validate(&origin).is_err());
    assert!(fake.configs.borrow().is_empty());
    assert!(fake.starts.borrow().is_empty());
    fake.busy.set(false);
    owner.poll();
    assert!(owner.validate(&owner.origin().unwrap()).is_ok());
}
static APP: AtomicUsize = AtomicUsize::new(1);
fn shell() -> (Rc<ApplicationShell>, Rc<BusOwner>, Rc<Fake>, FixtureDriver) {
    let app = gtk::Application::builder()
        .application_id(format!(
            "org.example.PecanBusOwner.Case{}",
            APP.fetch_add(1, Ordering::SeqCst)
        ))
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    let application = Rc::new(ApplicationOwner::fixture());
    let d = application.fixture_driver().unwrap();
    let g = graph();
    d.publish(g.clone(), generations(&g));
    let shell =
        ApplicationShell::install(&app, application, ShellOptions::default(), None).unwrap();
    shell.tick();
    let fake = Fake::new(shell.session().graph().unwrap().stamp.epoch);
    let buses = BusOwner::new(shell.session(), Some(fake.clone()));
    buses
        .set_recording_root(Some(std::env::temp_dir().join("pecan-quit-synthetic-root")))
        .unwrap();
    shell.attach_test_buses(buses.clone());
    (shell, buses, fake, d)
}
fn pump_until(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < end);
        while gtk::glib::MainContext::default().pending() {
            gtk::glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[gtk::test]
fn common_quit_waits_for_diagnostics_and_keeps_ingress_failure_distinct_from_successful_join() {
    let (shell, owner, fake, d) = shell();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 11);
    owner.poll();
    shell.request_quit();
    assert!(owner.session.admission_closed());
    assert!(owner.start(&owner.origin().unwrap(), None).is_err());
    assert_eq!(d.shutdown_requests(), 1);
    let take = fake.takes.borrow()[0].clone();
    let status = take.0.borrow_mut().take().unwrap();
    d.complete_shutdown(pecan_pipewire::shutdown::ShutdownReport {
        writers_finalized: 1,
        ..Default::default()
    });
    shell.tick();
    assert_eq!(shell.exit_code(), None);
    *take.0.borrow_mut() = Some(status);
    fake.joined(0, Some(RecordingError::Ingress(WriterSendError::Full)));
    shell.tick();
    assert!(matches!(
        shell.phase(),
        crate::lifetime::QuitPhase::Failed { .. }
    ));
    assert_eq!(shell.exit_code(), None);
    let report = owner.recording_status().pop().unwrap();
    assert_eq!(report.writer, Some(WriterPhase::Joined));
    assert_eq!(report.writer_outcome, Some(WriterOutcome::Finalized));
    assert!(report.failure.is_some());
    button(shell.window(), "Exit after finalization error").emit_clicked();
    assert_eq!(shell.exit_code(), Some(1));
}
#[gtk::test]
fn successful_common_quit_reuses_one_shutdown_and_never_starts_independent_finalization() {
    let (shell, owner, fake, d) = shell();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 11);
    owner.poll();
    shell.request_quit();
    for _ in 0..5 {
        shell.request_quit();
        shell.tick();
    }
    assert_eq!(d.shutdown_requests(), 1);
    assert!(fake.stops.borrow().is_empty());
    assert_eq!(shell.exit_code(), None);
    fake.joined(0, None);
    d.complete_shutdown(Default::default());
    pump_until(|| {
        shell.tick();
        shell.exit_code() == Some(0)
    });
}
#[gtk::test]
fn known_joined_writer_failure_can_exit_explicitly_when_its_diagnostics_are_unavailable() {
    let (shell, owner, fake, d) = shell();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 11);
    owner.poll();
    fake.takes.borrow()[0].0.borrow_mut().take();
    fake.busy.set(true);
    shell.request_quit();
    d.complete_shutdown(pecan_pipewire::shutdown::ShutdownReport {
        writers_failed: 1,
        writer_errors: vec!["Synthetic writer failure".into()],
        ..Default::default()
    });
    shell.tick();
    assert!(matches!(
        shell.phase(),
        crate::lifetime::QuitPhase::Failed { .. }
    ));
    assert_ne!(
        owner.recording_status()[0].writer,
        Some(WriterPhase::Joined)
    );
    button(shell.window(), "Exit after finalization error").emit_clicked();
    assert_eq!(shell.exit_code(), Some(1));
}
