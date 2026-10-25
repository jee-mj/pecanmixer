//! Synthetic local filter/configuration probes. No server or audio session.
use super::*;
use pecan_core::domain::buses::{BusConfig, BusState};
use std::cell::{Cell, RefCell};

struct Target {
    ports: Vec<usize>,
    gain: Vec<f32>,
    options: Option<pecan_core::graph::Opties>,
    recording: bool,
    routed: bool,
    fail_add: Option<usize>,
    fail_remove: Option<usize>,
    apply_count: usize,
    ignore_options: bool,
}
impl Default for Target {
    fn default() -> Self {
        Self {
            ports: vec![0, 1],
            gain: vec![1., 1.],
            options: None,
            recording: false,
            routed: false,
            fail_add: None,
            fail_remove: None,
            apply_count: 0,
            ignore_options: false,
        }
    }
}
impl ConfigTarget for Target {
    fn readback(&self) -> Option<BusReadback> {
        Some(BusReadback {
            node: Some(900),
            channels: self.ports.len(),
            gains: self.gain.clone(),
        })
    }
    fn recording(&self) -> bool {
        self.recording
    }
    fn routed(&self) -> bool {
        self.routed
    }
    fn resize(&mut self, requested: usize) -> ChannelResizeResult {
        let result = resize_ports(
            &mut self.ports,
            requested,
            |i| (self.fail_add != Some(i)).then_some(i),
            |p| if self.fail_remove == Some(*p) { -5 } else { 0 },
        );
        self.gain.resize(self.ports.len(), 1.);
        result
    }
    fn gains(&mut self, gains: &[f32]) {
        self.gain = gains.to_vec();
        self.apply_count += 1;
    }
    fn options(&mut self, options: pecan_core::graph::Opties) {
        if !self.ignore_options {
            self.options = Some(options);
        }
    }
    fn options_match(&self, options: &pecan_core::graph::Opties) -> bool {
        self.options.as_ref() == Some(options)
    }
}
fn setup() -> (Commands, Target) {
    let commands = Commands::default();
    commands.publish(Momentopname {
        verbonden: true,
        ..Default::default()
    });
    let target = Target::default();
    commands.refresh_config(target.readback(), false, false);
    (commands, target)
}
fn value() -> IdleBusConfig {
    let mut buses = BusConfig::default();
    buses.midi = false;
    IdleBusConfig {
        buses,
        clock: None,
        start_stop: false,
    }
}
fn request(commands: &Commands, configuration: IdleBusConfig) -> IdleConfigIntent {
    IdleConfigIntent {
        ticket: 1,
        expected: commands.config_snapshot().unwrap().revision,
        configuration,
    }
}
fn drain(commands: &Commands, target: &mut impl ConfigTarget) {
    assert!(commands
        .drain_with_config(
            |_| true,
            |_| false,
            |_| LocalExecution::Unreported,
            |work| commands.apply_config(work, target)
        )
        .is_empty());
}
fn result(receipt: &ConfigReceipt) -> ConfigCompletion {
    match receipt.try_report().unwrap().progress {
        ConfigProgress::Completed(c) => c,
        other => panic!("not completed: {other:?}"),
    }
}

#[test]
fn admission_is_not_local_completion_and_config_readback_includes_gain_mute_and_arming() {
    let (commands, mut target) = setup();
    let mut config = value();
    config.buses.widths = vec![1, 1];
    config.buses.states.clear();
    config.buses.states.insert(
        "rec-1".into(),
        BusState {
            db: -6.,
            mute: false,
            armed: true,
        },
    );
    config.buses.states.insert(
        "rec-2".into(),
        BusState {
            db: 3.,
            mute: true,
            armed: false,
        },
    );
    let old = commands.config_snapshot().unwrap();
    let receipt = commands
        .enqueue_config(request(&commands, config.clone()))
        .unwrap();
    assert_eq!(
        receipt.try_report().unwrap().progress,
        ConfigProgress::Queued
    );
    assert_eq!(commands.config_snapshot().unwrap(), old);
    drain(&commands, &mut target);
    let applied = result(&receipt);
    assert_eq!(applied.outcome, LocalConfigOutcome::Applied);
    assert_eq!(applied.snapshot.configuration, Some(config.clone()));
    assert_eq!(applied.snapshot.local, target.readback());
    assert_eq!(applied.snapshot.transport, target.options);
    let options = target.options.unwrap();
    assert_eq!(options.bussen.len(), 1);
    assert_eq!(options.bussen[0].kanalen, [0]);
    assert_eq!(target.gain[1], 0.);
    assert!((target.gain[0] - 10_f32.powf(-6. / 20.)).abs() < 0.00001);
    assert_eq!(
        applied.snapshot.revision.revision,
        old.revision.revision + 1
    );
    assert_eq!(
        commands.current().stamp.revision,
        1,
        "local apply does not manufacture graph observation"
    );
}

#[test]
fn invalid_configurations_and_disabled_midi_never_consume_capacity_or_mutate() {
    let (commands, target) = setup();
    let mut bads = Vec::new();
    let mut bad = value();
    bad.buses.widths = vec![];
    bads.push(bad);
    let mut bad = value();
    bad.buses.widths = vec![3];
    bads.push(bad);
    let mut bad = value();
    bad.buses.widths = vec![2; 33];
    bads.push(bad);
    let mut bad = value();
    bad.buses.states.get_mut("rec-1-2").unwrap().db = f64::NAN;
    bads.push(bad);
    let mut bad = value();
    bad.buses.states.get_mut("rec-1-2").unwrap().db = 100.;
    bads.push(bad);
    let mut bad = value();
    bad.buses
        .states
        .insert("invalid\0key".into(), BusState::default());
    bads.push(bad);
    for bad in bads {
        assert_eq!(
            commands
                .enqueue_config(request(&commands, bad))
                .unwrap_err(),
            ConfigError::InvalidInput
        );
    }
    for mode in 0..3 {
        let mut bad = value();
        match mode {
            0 => bad.buses.midi = true,
            1 => bad.clock = Some(1234),
            _ => bad.start_stop = true,
        };
        assert_eq!(
            commands
                .enqueue_config(request(&commands, bad))
                .unwrap_err(),
            ConfigError::UnsupportedMidi
        );
    }
    assert!(commands.0.lock().unwrap().pending.is_empty());
    assert_eq!(target.apply_count, 0);
}

#[test]
fn competing_requests_compare_revisions_at_execution_and_retry_is_a_truthful_noop() {
    let (commands, mut target) = setup();
    let intent = request(&commands, value());
    let first = commands.enqueue_config(intent.clone()).unwrap();
    let second = commands.enqueue_config(intent.clone()).unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&first).outcome, LocalConfigOutcome::Applied);
    assert_eq!(
        result(&second).outcome,
        LocalConfigOutcome::Rejected(ConfigError::StaleRevision)
    );
    assert_eq!(
        commands.enqueue_config(intent).unwrap_err(),
        ConfigError::StaleRevision
    );
    let revision = commands.config_snapshot().unwrap().revision;
    let retry = commands
        .enqueue_config(request(&commands, value()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&retry).outcome, LocalConfigOutcome::NoOp);
    assert_eq!(result(&retry).snapshot.revision, revision);
    assert_eq!(target.apply_count, 1);
}

#[test]
fn partial_channel_allocation_is_retained_and_retry_uses_actual_capacity_and_new_revision() {
    let (commands, mut target) = setup();
    target.fail_add = Some(3);
    let mut config = value();
    config.buses.widths = vec![2, 2];
    let receipt = commands
        .enqueue_config(request(&commands, config.clone()))
        .unwrap();
    drain(&commands, &mut target);
    let partial = result(&receipt);
    assert_eq!(
        partial.outcome,
        LocalConfigOutcome::PartiallyApplied(ConfigError::ResizeFailed)
    );
    let resize = partial.resize.unwrap();
    assert_eq!((resize.before, resize.requested, resize.actual), (2, 4, 3));
    assert_eq!(
        resize.failure,
        Some(ResizeFailure::AddFailed { channel: 3 })
    );
    assert!(partial.snapshot.configuration.is_none());
    assert!(partial.snapshot.transport.is_none());
    assert!(target.options.is_none());
    assert_eq!(target.apply_count, 0);
    target.fail_add = None;
    let retry = commands.enqueue_config(request(&commands, config)).unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&retry).outcome, LocalConfigOutcome::Applied);
    assert_eq!(target.ports.len(), 4);
}

#[test]
fn removal_failure_retains_the_failed_port_for_retry_and_invalid_sizes_are_typed() {
    let mut ports = vec![0, 1, 2, 3];
    let failed = resize_ports(&mut ports, 1, Some, |p| if *p == 2 { -12 } else { 0 });
    assert_eq!(ports, [0, 1, 2]);
    assert_eq!(
        failed.failure,
        Some(ResizeFailure::RemoveFailed {
            channel: 2,
            code: -12
        })
    );
    assert_eq!(failed.actual, 3);
    for count in [0, 65, usize::MAX] {
        let invalid = resize_ports(&mut ports, count, Some, |_| 0);
        assert_eq!(invalid.failure, Some(ResizeFailure::InvalidCount));
        assert_eq!(ports, [0, 1, 2]);
    }
    assert!(resize_ports(&mut ports, 1, Some, |_| 0).is_complete());
}

#[test]
fn recording_and_routed_layout_changes_are_rejected_but_idle_gain_and_arm_work_with_routes() {
    let (commands, mut target) = setup();
    target.recording = true;
    let active = commands
        .enqueue_config(request(&commands, value()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(
        result(&active).outcome,
        LocalConfigOutcome::Rejected(ConfigError::RecordingActive)
    );
    target.recording = false;
    target.routed = true;
    let initial = commands
        .enqueue_config(request(&commands, value()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&initial).outcome, LocalConfigOutcome::Applied);
    let mut split = value();
    split.buses.widths = vec![1, 1];
    let migration = commands.enqueue_config(request(&commands, split)).unwrap();
    drain(&commands, &mut target);
    assert_eq!(
        result(&migration).outcome,
        LocalConfigOutcome::Rejected(ConfigError::LayoutMigrationRequired)
    );
    let mut gain = value();
    gain.buses.states.get_mut("rec-1-2").unwrap().mute = true;
    gain.buses.states.get_mut("rec-1-2").unwrap().armed = false;
    let idle = commands.enqueue_config(request(&commands, gain)).unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&idle).outcome, LocalConfigOutcome::Applied);
    assert!(target.options.unwrap().bussen.is_empty());
    assert_eq!(target.gain, [0., 0.]);
}

#[test]
fn config_limits_share_fifo_budget_and_keep_stop_all_reserved() {
    let (commands, mut target) = setup();
    let mut receipts = Vec::new();
    for _ in 0..MAX_CONFIG_PENDING {
        receipts.push(
            commands
                .enqueue_config(request(&commands, value()))
                .unwrap(),
        );
    }
    assert_eq!(
        commands
            .enqueue_config(request(&commands, value()))
            .unwrap_err(),
        ConfigError::QueueFull
    );
    for _ in MAX_CONFIG_PENDING..NORMAL_CAPACITY {
        commands.legacy(Opdracht::Start).unwrap();
    }
    commands.legacy(Opdracht::Stop).unwrap();
    assert_eq!(commands.0.lock().unwrap().pending.len(), CAPACITY);
    let executions = Cell::new(0);
    commands.drain_with_config(
        |_| true,
        |_| false,
        |_| {
            executions.set(executions.get() + 1);
            LocalExecution::Unreported
        },
        |work| commands.apply_config(work, &mut target),
    );
    assert_eq!(
        commands.0.lock().unwrap().pending.len(),
        CAPACITY - PER_TICK
    );
    assert_eq!(executions.get(), PER_TICK - MAX_CONFIG_PENDING);
    assert_eq!(result(&receipts[0]).outcome, LocalConfigOutcome::Applied);
    assert!(receipts[1..]
        .iter()
        .all(|r| result(r).outcome == LocalConfigOutcome::Rejected(ConfigError::StaleRevision)));
}

#[test]
fn disconnect_and_stop_finish_queued_receipts_and_reclaim_capacity_without_replay() {
    for stop in [false, true] {
        let (commands, mut target) = setup();
        let old = request(&commands, value());
        let receipt = commands.enqueue_config(old.clone()).unwrap();
        if stop {
            commands.stop();
        } else {
            commands.publish(Momentopname::default());
        }
        assert_eq!(
            result(&receipt).outcome,
            LocalConfigOutcome::Rejected(if stop {
                ConfigError::Stopped
            } else {
                ConfigError::Disconnected
            })
        );
        assert!(commands.0.lock().unwrap().pending.is_empty());
        drain(&commands, &mut target);
        assert_eq!(target.apply_count, 0);
        if !stop {
            commands.publish(Momentopname {
                verbonden: true,
                ..Default::default()
            });
            commands.refresh_config(target.readback(), false, false);
            assert_eq!(
                commands.enqueue_config(old).unwrap_err(),
                ConfigError::StaleRevision
            );
            let fresh = commands
                .enqueue_config(request(&commands, value()))
                .unwrap();
            drain(&commands, &mut target);
            assert_eq!(result(&fresh).outcome, LocalConfigOutcome::Applied);
        }
    }
}

#[test]
fn preceding_legacy_configuration_invalidates_native_cas_without_reordering() {
    let (commands, target) = setup();
    let target = RefCell::new(target);
    commands
        .legacy(Opdracht::ZetBus {
            kanalen: 2,
            gain: vec![0.25, 0.25],
        })
        .unwrap();
    let receipt = commands
        .enqueue_config(request(&commands, value()))
        .unwrap();
    commands.drain_with_config(
        |_| true,
        |_| false,
        |op| {
            let Opdracht::ZetBus { gain, .. } = op else {
                panic!()
            };
            target.borrow_mut().gains(&gain);
            commands.refresh_config(target.borrow().readback(), false, true);
            LocalExecution::Unreported
        },
        |work| commands.apply_config(work, &mut *target.borrow_mut()),
    );
    assert_eq!(
        result(&receipt).outcome,
        LocalConfigOutcome::Rejected(ConfigError::StaleRevision)
    );
    assert_eq!(target.borrow().gain, [0.25, 0.25]);
    assert!(commands.config_snapshot().unwrap().configuration.is_none());
}

#[test]
fn configuration_authority_is_bound_to_backend_owner_not_just_epoch_and_revision() {
    let (first, _) = setup();
    let (second, _) = setup();
    let old = request(&first, value());
    let current = request(&second, value());
    assert_eq!(old.expected.epoch, current.expected.epoch);
    assert_eq!(old.expected.revision, current.expected.revision);
    assert_ne!(old.expected.owner, current.expected.owner);
    assert_eq!(
        second.enqueue_config(old).unwrap_err(),
        ConfigError::StaleRevision
    );
    assert!(second.0.lock().unwrap().pending.is_empty());
}

#[test]
fn no_allocation_failure_leaves_revision_usable_but_readback_mismatch_is_partial() {
    let (commands, mut target) = setup();
    let mut larger = value();
    larger.buses.widths = vec![2, 2];
    target.fail_add = Some(2);
    let before = commands.config_snapshot().unwrap();
    let failed = commands.enqueue_config(request(&commands, larger)).unwrap();
    drain(&commands, &mut target);
    assert_eq!(
        result(&failed).outcome,
        LocalConfigOutcome::Rejected(ConfigError::ResizeFailed)
    );
    assert_eq!(result(&failed).snapshot, before);
    target.fail_add = None;
    target.ignore_options = true;
    let mismatch = commands
        .enqueue_config(request(&commands, value()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(
        result(&mismatch).outcome,
        LocalConfigOutcome::PartiallyApplied(ConfigError::ReadbackMismatch)
    );
    assert!(result(&mismatch).snapshot.configuration.is_none());
    assert!(result(&mismatch).snapshot.transport.is_none());
    assert_ne!(result(&mismatch).snapshot.revision, before.revision);
    target.ignore_options = false;
    let retry = commands
        .enqueue_config(request(&commands, value()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&retry).outcome, LocalConfigOutcome::Applied);
}

#[test]
fn native_shrink_is_explicitly_deferred_even_without_observed_routes() {
    let (commands, mut target) = setup();
    let mut smaller = value();
    smaller.buses.widths = vec![1];
    let receipt = commands
        .enqueue_config(request(&commands, smaller))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(
        result(&receipt).outcome,
        LocalConfigOutcome::Rejected(ConfigError::LayoutMigrationRequired)
    );
    assert_eq!(target.ports, [0, 1]);
    assert!(target.options.is_none());
}

#[test]
fn native_polling_and_admission_are_nonblocking_and_unavailable_is_explicit() {
    let (commands, target) = setup();
    let intent = request(&commands, value());
    let lock = commands.0.lock().unwrap();
    assert!(commands.config_snapshot().is_none());
    assert_eq!(
        commands.enqueue_config(intent.clone()).unwrap_err(),
        ConfigError::Busy
    );
    drop(lock);
    commands.refresh_config(None, false, false);
    let mut unavailable = intent;
    unavailable.expected = commands.config_snapshot().unwrap().revision;
    assert_eq!(
        commands.enqueue_config(unavailable).unwrap_err(),
        ConfigError::Unavailable
    );
    commands.refresh_config(target.readback(), false, false);
    commands.0.lock().unwrap().config.exhausted = true;
    assert_eq!(
        commands
            .enqueue_config(request(&commands, value()))
            .unwrap_err(),
        ConfigError::RevisionExhausted
    );
    {
        let mut state = commands.0.lock().unwrap();
        state.config.exhausted = false;
        state.config.snapshot.revision.revision = u64::MAX;
    }
    assert_eq!(
        commands
            .enqueue_config(request(&commands, value()))
            .unwrap_err(),
        ConfigError::RevisionExhausted
    );
}

#[test]
fn a_claimed_apply_can_finish_after_stop_but_reports_cancellation_and_local_result() {
    let (commands, mut target) = setup();
    let receipt = commands
        .enqueue_config(request(&commands, value()))
        .unwrap();
    commands.drain_with_config(
        |_| true,
        |_| false,
        |_| LocalExecution::Unreported,
        |work| {
            struct Stopping<'a> {
                target: &'a mut Target,
                commands: &'a Commands,
                receipt: &'a ConfigReceipt,
            }
            impl ConfigTarget for Stopping<'_> {
                fn readback(&self) -> Option<BusReadback> {
                    self.target.readback()
                }
                fn recording(&self) -> bool {
                    self.target.recording()
                }
                fn routed(&self) -> bool {
                    self.target.routed()
                }
                fn resize(&mut self, n: usize) -> ChannelResizeResult {
                    assert_eq!(
                        self.receipt.try_report().unwrap().progress,
                        ConfigProgress::Applying
                    );
                    self.commands.stop();
                    self.target.resize(n)
                }
                fn gains(&mut self, g: &[f32]) {
                    self.target.gains(g);
                }
                fn options(&mut self, o: pecan_core::graph::Opties) {
                    self.target.options(o);
                }
                fn options_match(&self, o: &pecan_core::graph::Opties) -> bool {
                    self.target.options_match(o)
                }
            }
            commands.apply_config(
                work,
                &mut Stopping {
                    target: &mut target,
                    commands: &commands,
                    receipt: &receipt,
                },
            );
        },
    );
    let completion = result(&receipt);
    assert_eq!(completion.outcome, LocalConfigOutcome::Applied);
    assert!(completion.cancellation_requested);
    assert!(!completion.snapshot.connected);
    assert_eq!(target.apply_count, 1);
}

#[test]
fn stable_invalid_legacy_readback_does_not_churn_revisions_and_can_be_repaired() {
    let (commands, mut target) = setup();
    target.gain[0] = f32::NAN;
    commands.refresh_config(target.readback(), false, true);
    let before = commands.config_snapshot().unwrap();
    commands.refresh_config(target.readback(), false, false);
    assert_eq!(commands.config_snapshot().unwrap(), before);
    let receipt = commands
        .enqueue_config(request(&commands, value()))
        .unwrap();
    drain(&commands, &mut target);
    assert_eq!(result(&receipt).outcome, LocalConfigOutcome::Applied);
    assert_eq!(target.gain, [1., 1.]);
}

#[path = "config_origin_tests.rs"]
mod origin;
