use super::*;
use crate::lifetime::{Resources, Visibility};
use pecan_pipewire::shutdown::Shutdown;
#[test]
fn saturated_wake_and_visibility_storms_cannot_reject_quit_or_grow_backlog() {
    let (mailbox, wake) = ActionMailbox::new();
    for _ in 0..100_000 {
        mailbox.present().unwrap();
        mailbox.toggle().unwrap();
        mailbox.quit().unwrap();
    }
    assert_eq!(wake.len(), 1);
    assert_eq!(wake.capacity(), Some(1));
    let batch = mailbox.drain();
    assert!(batch.quit);
    assert_eq!(batch.visibility, VisibilityAction::Hide);
    let (mut policy, _) = Lifetime::<Shutdown>::start(
        false,
        Resources {
            backend: true,
            settings: false,
        },
        Duration::ZERO,
    );
    policy.apply(Event::Tray(TrayAvailability::Usable), Duration::ZERO);
    let effects = batch.apply(&mut policy, Duration::ZERO);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::RequestBackendShutdown { .. })));
    assert_eq!(policy.visibility(), Visibility::Visible);
    assert!(!mailbox.drain().quit);
}
#[test]
fn composition_preserves_order_and_toggle_parity_without_queued_clicks() {
    let (mailbox, _wake) = ActionMailbox::new();
    mailbox.toggle().unwrap();
    mailbox.toggle().unwrap();
    assert_eq!(mailbox.drain().visibility, VisibilityAction::None);
    mailbox.present().unwrap();
    mailbox.toggle().unwrap();
    assert_eq!(mailbox.drain().visibility, VisibilityAction::Hide);
    mailbox.toggle().unwrap();
    mailbox.present().unwrap();
    assert_eq!(mailbox.drain().visibility, VisibilityAction::Present);
}
#[test]
fn stale_service_and_late_registration_success_cannot_override_host_loss() {
    let (mailbox, _wake) = ActionMailbox::new();
    let first = mailbox.begin_generation().unwrap();
    mailbox.registered(first, mailbox.now());
    assert_eq!(
        mailbox.drain().tray.map(|e| e.availability),
        Some(TrayAvailability::Usable)
    );
    mailbox.availability(first, TrayAvailability::Unavailable);
    mailbox.registered(first, mailbox.now());
    assert_eq!(
        mailbox.drain().tray.map(|e| e.availability),
        Some(TrayAvailability::Unavailable)
    );
    let second = mailbox.begin_generation().unwrap();
    mailbox.registered(second, mailbox.now());
    mailbox.availability(first, TrayAvailability::Unavailable);
    assert_eq!(
        mailbox.drain().tray.map(|e| e.availability),
        Some(TrayAvailability::Usable)
    );
}
#[test]
fn stopped_consumer_is_explicit_and_unknown_host_prevents_hide() {
    let (mailbox, receiver) = ActionMailbox::new();
    drop(receiver);
    assert_eq!(mailbox.quit(), Err(ConsumerStopped));
    let (mailbox, _wake) = ActionMailbox::new();
    mailbox.present().unwrap();
    mailbox.toggle().unwrap();
    let (mut policy, _) = Lifetime::<Shutdown>::start(
        false,
        Resources {
            backend: false,
            settings: false,
        },
        Duration::ZERO,
    );
    assert_eq!(
        mailbox.drain().apply(&mut policy, Duration::ZERO),
        vec![Effect::Present]
    );
    assert_eq!(policy.visibility(), Visibility::Visible);
}

#[test]
fn concurrent_producers_keep_toggle_parity_and_reserved_quit() {
    let (mailbox, wake) = ActionMailbox::new();
    let mut threads = vec![];
    for _ in 0..4 {
        let sender = mailbox.clone();
        threads.push(std::thread::spawn(move || {
            for _ in 0..10000 {
                sender.toggle().unwrap();
                sender.quit().unwrap();
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(wake.len(), 1);
    let batch = mailbox.drain();
    assert!(batch.quit);
    assert_eq!(batch.visibility, VisibilityAction::None);
}

#[test]
fn desktop_action_slot_is_bounded_correlated_and_cannot_starve_reserved_quit() {
    let (mailbox, _wake) = ActionMailbox::new();
    mailbox.quit().unwrap();
    for source in 0..10000 {
        assert!(mailbox.autostart(source, source % 2 == 0));
    }
    assert!(!mailbox.autostart(u64::MAX, true));
    let batch = mailbox.drain();
    assert!(batch.quit);
    assert_eq!(
        batch.autostart,
        Some(AutostartAction {
            source: 9999,
            enabled: false
        })
    );
    assert!(mailbox.drain().autostart.is_none());
}
