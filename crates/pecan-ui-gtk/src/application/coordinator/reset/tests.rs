use super::*;
use crate::application::{tests::large_reset::scene, ApplicationOwner};

#[test]
fn large_reset_total_deadline_stops_queued_child_without_fabricating_receipt_or_future_batch() {
    let owner = ApplicationOwner::fixture();
    let session = owner.activate();
    let driver = owner.fixture_driver().unwrap();
    let g = scene(25);
    let ids = g
        .knopen
        .iter()
        .map(|n| (n.id, u64::from(n.id)))
        .chain(g.poorten.iter().map(|p| (p.id, u64::from(p.id))))
        .collect();
    let graph = driver.publish(g, ids);
    session.poll();
    let parent = session
        .execute(&graph, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let Receipt::Sequence(sequence) = &parent.1 else {
        panic!()
    };
    let deadline = sequence.borrow().started + TOTAL_LIMIT;
    session.drive_reset_at(deadline);
    assert!(
        matches!(sequence.borrow().progress,SequenceProgress::Failed {ref message} if message.contains("timed out"))
    );
    assert_eq!(sequence.borrow().report().children.len(), 1);
    assert_eq!(
        sequence.borrow().report().children[0].progress,
        NativeProgress::Queued
    );
    assert_eq!(driver.take_requests().len(), 1);
    for _ in 0..10 {
        session.poll();
        assert!(driver.take_requests().is_empty());
    }
    assert_eq!(session.history_lengths(), (0, 0));
    assert!(session.busy());
    assert!(session.recovery_ready(&parent, &graph).is_err());
}

#[test]
fn large_reset_zero_command_progress_still_rechecks_previous_effects_before_dispatch() {
    let owner = ApplicationOwner::fixture();
    let session = owner.activate();
    let driver = owner.fixture_driver().unwrap();
    let g = scene(60);
    let ids = g
        .knopen
        .iter()
        .map(|n| (n.id, u64::from(n.id)))
        .chain(g.poorten.iter().map(|p| (p.id, u64::from(p.id))))
        .collect();
    let origin = driver.publish(g, ids);
    session.poll();
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let Receipt::Sequence(sequence) = &parent.1 else {
        panic!()
    };
    let first = driver.take_requests().pop().unwrap();
    let applied = crate::application::tests::large_reset::apply(&driver, &session, &first, false);
    let old = first
        .intent
        .commands
        .iter()
        .find_map(|c| match c {
            Opdracht::Ontkoppel { kabel } => origin.value.kabels.iter().find(|l| l.id == *kabel),
            _ => None,
        })
        .unwrap()
        .clone();
    // An external observer already removed the next 32 planned links. They
    // progress without another RPC or a synthesized child receipt.
    let removed: BTreeSet<_> = sequence.borrow().units[32..64]
        .iter()
        .filter_map(|u| match u.command {
            Opdracht::Ontkoppel { kabel } => Some(kabel),
            _ => None,
        })
        .collect();
    assert_eq!(removed.len(), 32);
    let mut g = (*applied.value).clone();
    g.kabels.retain(|l| !removed.contains(&l.id));
    let ids = g
        .knopen
        .iter()
        .map(|n| (n.id, u64::from(n.id)))
        .chain(g.poorten.iter().map(|p| (p.id, u64::from(p.id))))
        .collect();
    driver.publish(g.clone(), ids);
    session.poll();
    assert!(driver.take_requests().is_empty());
    assert_eq!(sequence.borrow().cursor, 64);
    let mut link = old;
    link.id = 900_000;
    g.kabels.push(link);
    let ids = g
        .knopen
        .iter()
        .map(|n| (n.id, u64::from(n.id)))
        .chain(g.poorten.iter().map(|p| (p.id, u64::from(p.id))))
        .collect();
    driver.publish(g, ids);
    session.poll();
    assert!(driver.take_requests().is_empty());
    assert!(matches!(
        sequence.borrow().progress,
        SequenceProgress::Failed { .. }
    ));
    assert_eq!(sequence.borrow().report().children.len(), 1);
    assert_eq!(session.history_lengths(), (0, 0));
}
