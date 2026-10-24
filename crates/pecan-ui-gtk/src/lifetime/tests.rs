use super::*;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Default)]
struct Probe(Rc<RefCell<Option<ShutdownReport>>>);
impl Completion for Probe {
    fn try_complete(&self) -> Option<ShutdownReport> {
        self.0.borrow().clone()
    }
}
fn policy(hidden: bool, backend: bool, settings: bool) -> Lifetime<Probe> {
    Lifetime::start(hidden, Resources { backend, settings }, Duration::ZERO).0
}
fn backend_id(effects: &[Effect]) -> RequestId {
    effects
        .iter()
        .find_map(|e| {
            if let Effect::RequestBackendShutdown { request } = e {
                Some(*request)
            } else {
                None
            }
        })
        .unwrap()
}
fn settings_id(effects: &[Effect]) -> RequestId {
    effects
        .iter()
        .find_map(|e| {
            if let Effect::DrainSettings { request } = e {
                Some(*request)
            } else {
                None
            }
        })
        .unwrap()
}
fn t(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}
#[test]
fn visible_hidden_host_absence_grace_and_loss_never_leave_an_unreachable_window() {
    let (visible, effects) = Lifetime::<Probe>::start(
        false,
        Resources {
            backend: true,
            settings: false,
        },
        t(0),
    );
    assert_eq!(visible.visibility(), Visibility::Visible);
    assert_eq!(effects, vec![Effect::HoldApplication, Effect::Present]);
    for available in [false, true] {
        let mut p = policy(true, true, false);
        assert_eq!(p.visibility(), Visibility::AwaitingTray);
        assert!(p.apply(Event::Tick, Duration::from_millis(999)).is_empty());
        if available {
            assert_eq!(
                p.apply(Event::Tray(TrayAvailability::Usable), t(0)),
                vec![Effect::Hide]
            );
            assert_eq!(p.apply(Event::Close, t(0)), vec![Effect::Hide]);
            assert_eq!(
                p.apply(Event::Tray(TrayAvailability::Unavailable), t(1)),
                vec![Effect::Present]
            );
        } else {
            assert_eq!(p.apply(Event::Tick, t(1)), vec![Effect::Present]);
        }
        assert_eq!(p.visibility(), Visibility::Visible);
        assert!(
            p.apply(Event::Tray(TrayAvailability::Usable), t(2))
                .is_empty(),
            "host recovery cannot unexpectedly hide a visible fallback"
        );
    }
    let mut p = policy(true, true, false);
    assert_eq!(
        p.apply(Event::Tray(TrayAvailability::Unavailable), t(0)),
        vec![Effect::Present]
    );
}
#[test]
fn explicit_activation_overrides_hidden_startup_and_never_requests_another_backend() {
    let mut p = policy(true, true, false);
    for _ in 0..10_000 {
        assert_eq!(p.apply(Event::Activate, t(0)), vec![Effect::Present]);
    }
    p.apply(Event::Tray(TrayAvailability::Usable), t(0));
    assert_eq!(p.visibility(), Visibility::Visible);
    assert_eq!(p.apply(Event::Toggle, t(0)), vec![Effect::Hide]);
    assert_eq!(p.apply(Event::Activate, t(0)), vec![Effect::Present]);
}
#[test]
fn close_hides_only_with_usable_tray_and_explicit_quit_always_finalizes() {
    for availability in [
        TrayAvailability::Unknown,
        TrayAvailability::Unavailable,
        TrayAvailability::Usable,
    ] {
        let mut p = policy(false, true, false);
        p.apply(Event::Tray(availability), t(0));
        let close = p.apply(Event::Close, t(0));
        if availability == TrayAvailability::Usable {
            assert_eq!(close, vec![Effect::Hide]);
            assert!(p.mutations_allowed());
        } else {
            assert!(close
                .iter()
                .any(|e| matches!(e, Effect::RequestBackendShutdown { .. })));
            assert!(!p.mutations_allowed());
        }
    }
    let mut p = policy(true, true, false);
    p.apply(Event::Tray(TrayAvailability::Usable), t(0));
    let effects = p.apply(Event::Quit, t(0));
    assert!(effects.contains(&Effect::Present));
    backend_id(&effects);
    assert_eq!(p.visibility(), Visibility::Visible);
}
#[test]
fn repeated_quit_is_single_attempt_and_timeout_retry_retains_completion() {
    let mut p = policy(false, true, false);
    let id = backend_id(&p.apply(Event::Quit, t(0)));
    let probe = Probe::default();
    p.backend_requested(id, Ok(probe.clone()), t(0));
    for _ in 0..10_000 {
        assert!(p.apply(Event::Quit, t(1)).is_empty());
    }
    assert_eq!(p.apply(Event::Close, t(1)), vec![Effect::Present]);
    assert!(p.apply(Event::Tick, t(5)).is_empty());
    assert_eq!(
        p.phase(),
        &QuitPhase::Pending {
            backend: true,
            settings: false
        }
    );
    *probe.0.borrow_mut() = Some(ShutdownReport::default());
    assert!(
        p.apply(Event::Tick, t(100)).is_empty(),
        "timeout alone never grants success"
    );
    let effects = p.apply(Event::RetryQuit, t(101));
    assert_eq!(effects, vec![Effect::Present, Effect::Exit { code: 0 }]);
    assert!(!effects
        .iter()
        .any(|e| matches!(e, Effect::RequestBackendShutdown { .. })));
    assert!(p.apply(Event::Quit, t(102)).is_empty());
}
#[test]
fn late_handle_is_retained_after_timeout_without_reissuing_shutdown() {
    let mut p = policy(false, true, false);
    let request = backend_id(&p.apply(Event::Quit, t(0)));
    p.apply(Event::Tick, t(5));
    let probe = Probe::default();
    *probe.0.borrow_mut() = Some(ShutdownReport::default());
    assert!(p.backend_requested(request, Ok(probe), t(6)).is_empty());
    assert!(matches!(p.phase(), QuitPhase::Pending { .. }));
    assert_eq!(
        p.apply(Event::RetryQuit, t(7)),
        vec![Effect::Present, Effect::Exit { code: 0 }]
    );
}
#[test]
fn settings_drain_is_independent_and_no_early_exit_is_permitted() {
    let mut p = policy(false, true, true);
    let effects = p.apply(Event::Quit, t(0));
    let backend = backend_id(&effects);
    let settings = settings_id(&effects);
    let probe = Probe::default();
    *probe.0.borrow_mut() = Some(ShutdownReport {
        writers_failed: 1,
        worker_panicked: true,
        ..Default::default()
    });
    assert!(p.backend_requested(backend, Ok(probe), t(0)).is_empty());
    p.apply(Event::Tick, t(5));
    assert_eq!(
        p.phase(),
        &QuitPhase::Pending {
            backend: false,
            settings: true
        }
    );
    assert!(p
        .apply(
            Event::ExitAfterError {
                acknowledgement: RequestId(1)
            },
            t(5)
        )
        .is_empty());
    assert!(p.settings_drained(RequestId(999), Ok(()), t(5)).is_empty());
    assert!(p
        .settings_drained(
            settings,
            Err("Synthetic settings write error; worker joined".into()),
            t(6)
        )
        .is_empty());
    p.apply(Event::RetryQuit, t(7));
    let QuitPhase::Failed {
        acknowledgement,
        errors,
    } = p.phase().clone()
    else {
        panic!("both joined resources failed")
    };
    assert_eq!(errors.len(), 3);
    for event in [
        Event::Quit,
        Event::RetryQuit,
        Event::ExitAfterError {
            acknowledgement: RequestId(999),
        },
    ] {
        assert!(p.apply(event, t(7)).is_empty());
    }
    assert_eq!(
        p.apply(Event::ExitAfterError { acknowledgement }, t(7)),
        vec![Effect::Exit { code: 1 }]
    );
}
#[test]
fn backend_request_failure_retry_preserves_settings_drain_and_rejects_stale_callbacks() {
    let mut p = policy(false, true, true);
    let effects = p.apply(Event::Quit, t(0));
    let first = backend_id(&effects);
    let settings = settings_id(&effects);
    p.backend_requested(first, Err("Synthetic owner not ready".into()), t(0));
    assert!(matches!(p.phase(), QuitPhase::Blocked(_)));
    let retry = p.apply(Event::RetryQuit, t(1));
    let second = backend_id(&retry);
    assert_ne!(first, second);
    assert!(!retry
        .iter()
        .any(|e| matches!(e, Effect::DrainSettings { .. })));
    p.settings_drained(settings, Ok(()), t(1));
    let complete = Probe::default();
    *complete.0.borrow_mut() = Some(ShutdownReport::default());
    assert!(p
        .backend_requested(first, Ok(complete.clone()), t(1))
        .is_empty());
    assert_eq!(p.phase(), &QuitPhase::Waiting);
    assert_eq!(
        p.backend_requested(second, Ok(complete), t(1)),
        vec![Effect::Exit { code: 0 }]
    );
}
#[test]
fn successful_backend_waits_for_explicit_settings_completion_and_plain_shell_can_exit() {
    let mut p = policy(false, true, true);
    let effects = p.apply(Event::Quit, t(0));
    let backend = backend_id(&effects);
    let settings = settings_id(&effects);
    let probe = Probe::default();
    *probe.0.borrow_mut() = Some(ShutdownReport::default());
    assert!(p.backend_requested(backend, Ok(probe), t(0)).is_empty());
    assert_eq!(
        p.settings_drained(settings, Ok(()), t(1)),
        vec![Effect::Exit { code: 0 }]
    );
    assert!(p
        .settings_drained(settings, Err("late".into()), t(1))
        .is_empty());
    let mut empty = policy(false, false, false);
    assert_eq!(
        empty.apply(Event::Close, t(0)),
        vec![
            Effect::DisableMutations,
            Effect::Present,
            Effect::Exit { code: 0 }
        ]
    );
}

#[test]
fn expired_tray_evidence_restores_hidden_window_even_if_adapter_worker_is_stalled() {
    let mut p = policy(true, true, false);
    p.apply(Event::Tray(TrayAvailability::Usable), t(0));
    assert_eq!(p.visibility(), Visibility::Hidden);
    p.apply(Event::Tray(TrayAvailability::Usable), t(4));
    assert!(p.apply(Event::Tick, t(8)).is_empty());
    assert_eq!(p.apply(Event::Tick, t(9)), vec![Effect::Present]);
    assert_eq!(p.tray(), TrayAvailability::Unknown);
    assert!(
        p.apply(Event::Tray(TrayAvailability::Usable), t(10))
            .is_empty(),
        "fresh evidence cannot re-hide a recovered window"
    );
}

#[test]
fn delayed_or_future_tray_evidence_cannot_renew_a_hidden_lease() {
    let mut p = policy(true, true, false);
    assert_eq!(
        p.apply_tray_evidence(TrayAvailability::Usable, t(0), t(6)),
        vec![]
    );
    assert_eq!(p.tray(), TrayAvailability::Unknown);
    assert_eq!(p.apply(Event::Tick, t(6)), vec![Effect::Present]);
    assert!(p
        .apply_tray_evidence(TrayAvailability::Usable, t(20), t(7))
        .is_empty());
    assert_eq!(p.tray(), TrayAvailability::Unknown);
}

#[test]
fn close_cannot_hide_on_expired_evidence_even_before_the_next_tick() {
    let mut p = policy(false, true, false);
    p.apply(Event::Tray(TrayAvailability::Usable), t(0));
    let effects = p.apply(Event::Close, t(6));
    assert!(!effects.contains(&Effect::Hide));
    backend_id(&effects);
}
