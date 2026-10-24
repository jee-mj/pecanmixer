use super::*;
use crate::test_support::{assert_wav, TestDir};
use std::sync::mpsc;

#[test]
fn backend_shutdown_cancels_admission_stops_active_take_and_acknowledges_finalized_bytes() {
    let dir = TestDir::new("backend-shutdown");
    let backend = Graaf {
        opdrachten: Arc::default(),
        stop: Arc::default(),
        completion: Shutdown::new(),
    };
    let (tx, writer) = opname::schrijver(
        &dir.0,
        vec![Spoor {
            naam: "Stereo".into(),
            kanalen: 2,
        }],
        48_000,
    )
    .unwrap();
    tx.send(Bericht::Audio(0, vec![0.25, -0.25, 0.5, -0.5]))
        .unwrap();
    backend.completion.writer(writer);
    let worker_stop = Arc::clone(&backend.stop);
    let (ready, waiting) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let recorder: Opnemer = Rc::new(RefCell::new(vec![Actief {
            tx,
            t0: std::time::Instant::now(),
            bussen: vec![(0, vec![0, 1], "Stereo".into())],
            midi: BTreeMap::new(),
            midi_namen: vec![],
            midi_events: vec![],
            pulsen: vec![],
        }]));
        ready.send(()).unwrap();
        worker_stop.retry_wait();
        assert!(worker_stop.requested());
        // Exercise the actual production recorder stop path, without opening
        // a PipeWire connection or running a process callback.
        eindig(&recorder, &Klok::default());
        assert!(recorder.borrow().is_empty());
    });
    backend.completion.worker(worker);
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let completion = backend.request_shutdown();
    assert_eq!(
        backend.dispatch(BackendCommand {
            epoch: 0,
            command: Opdracht::Start
        }),
        Err(DispatchError::Stopped)
    );
    drop(backend); // Completion handle must remain useful after owner drop.
    let report = completion.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(report.is_success(), "{report:?}");
    assert_eq!(report.writers_finalized, 1);
    assert_wav(
        &dir.0.join("Stereo.wav"),
        2,
        48_000,
        &[0.25, -0.25, 0.5, -0.5],
    );
}

#[test]
fn session_transport_drop_keeps_unfinished_writer_results_across_reconnect() {
    let completion = Shutdown::new();
    let (release, gate) = mpsc::channel();
    {
        let first_session = Transport::new(completion.clone());
        first_session.schrijvers.writer(std::thread::spawn(move || {
            gate.recv_timeout(Duration::from_secs(2))
                .map_err(|e| e.to_string())
        }));
    }
    let next_session = Transport::new(completion.clone());
    completion.worker(std::thread::spawn(move || drop(next_session)));
    assert!(
        completion.wait_timeout(Duration::ZERO).is_err(),
        "old session writer must stay tracked"
    );
    release.send(()).unwrap();
    let report = completion.wait_timeout(Duration::from_secs(2)).unwrap();
    assert!(report.is_success());
    assert_eq!(report.writers_finalized, 1);
}
