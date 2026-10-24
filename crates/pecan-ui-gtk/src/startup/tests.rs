//! Registered injected entrypoints only. No native constructor, host tray,
//! profile discovery or user settings is used by these tests.
use super::*;
use crate::{application::FixtureDriver, display_settings::DisplayEdit};
use gtk::gio;
use pecan_core::{
    domain::history::Action,
    native_command::{Counts, NativeProgress},
};
use std::{
    cell::Cell,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
static NEXT: AtomicUsize = AtomicUsize::new(1);
fn app() -> gtk::Application {
    let app = gtk::Application::builder()
        .application_id(format!(
            "org.example.PecanStartup.Case{}",
            NEXT.fetch_add(1, Ordering::SeqCst)
        ))
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    app
}
fn graph() -> Momentopname {
    serde_json::from_str(include_str!("../../../../fixtures/graph/audio-basic.json")).unwrap()
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
fn setup(
    options: Options,
    worker: Option<SettingsWorker>,
) -> (Rc<EntryPoint>, FixtureDriver, Rc<Cell<usize>>) {
    let owner = Rc::new(ApplicationOwner::fixture());
    let driver = owner.fixture_driver().unwrap();
    let graph = graph();
    driver.publish(graph.clone(), generations(&graph));
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let entry = EntryPoint::attach(
        &app(),
        options,
        Box::new(move || {
            seen.set(seen.get() + 1);
            assert!(owner.native_handle().is_none());
            Ok(Resources {
                owner,
                settings: worker,
            })
        }),
    );
    entry.app.activate();
    (entry, driver, calls)
}
fn pump_until(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(6);
    while !ready() {
        assert!(Instant::now() < end, "synthetic entrypoint deadline");
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn finish(entry: &EntryPoint, driver: &FixtureDriver) {
    let shell = entry.shell().unwrap();
    shell.request_quit();
    driver.complete_shutdown(Default::default());
    pump_until(|| {
        shell.tick();
        shell.exit_code() == Some(0)
    });
    assert!(entry.app.windows().is_empty());
}
fn parse(args: &[&str]) -> Result<Option<Options>, String> {
    Options::parse(args.iter().map(OsString::from))
}
#[test]
fn options_keep_fixture_settings_and_real_services_disjoint() {
    assert!(parse(&[]).unwrap().unwrap().settings.is_none());
    for args in [
        vec!["--headless"],
        vec!["--smoke"],
        vec![
            "--fixture",
            "synthetic.json",
            "--settings",
            "/tmp/synthetic.json",
        ],
        vec!["--fixture", "synthetic.json", "--tray"],
        vec!["--settings", "relative.json"],
        vec!["--settings", "/tmp/synthetic.json", "--memory-defaults"],
        vec!["--fixture", "a", "--fixture", "b"],
    ] {
        assert!(parse(&args).is_err(), "{args:?}");
    }
    assert!(
        parse(&["--fixture", "synthetic.json", "--smoke", "--patchbay"])
            .unwrap()
            .is_some()
    );
    assert!(parse(&["--help"]).unwrap().is_none());
}
#[gtk::test]
fn failed_factory_never_installs_a_window_or_retries_on_later_activation() {
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let entry = EntryPoint::attach(
        &app(),
        Options::default(),
        Box::new(move || {
            seen.set(seen.get() + 1);
            Err("Synthetic startup failure".into())
        }),
    );
    entry.app.activate();
    entry.app.activate();
    assert_eq!(calls.get(), 1);
    assert!(entry.shell().is_none());
    assert!(entry.app.windows().is_empty());
    assert_eq!(
        entry.failure.borrow().as_deref(),
        Some("Synthetic startup failure")
    );
}
#[gtk::test]
fn actual_activation_consumes_one_injected_factory_and_keeps_owner_until_joined_quit() {
    let (entry, d, calls) = setup(Options::default(), None);
    let shell = entry.shell().unwrap();
    let session = shell.session();
    let window = shell.window().clone();
    for _ in 0..50 {
        entry.app.activate();
        assert_eq!(entry.shell().unwrap().window(), &window);
        assert!(entry.shell().unwrap().session().same_session(&session));
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(entry.app.windows().len(), 1);
    shell.window().close();
    shell.tick();
    assert_eq!(d.shutdown_requests(), 1);
    assert_eq!(shell.exit_code(), None);
    assert!(shell.window().is_visible());
    for _ in 0..3 {
        entry.app.activate();
        shell.request_quit();
        shell.tick();
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(d.shutdown_requests(), 1);
    finish(&entry, &d);
    entry.app.activate();
    assert_eq!(calls.get(), 1);
    assert!(entry.app.windows().is_empty());
}
#[gtk::test]
fn hidden_without_real_host_reopens_and_mailbox_quit_keeps_the_same_owner() {
    let (entry, d, calls) = setup(
        Options {
            hidden: true,
            ..Default::default()
        },
        None,
    );
    let shell = entry.shell().unwrap();
    assert!(!shell.window().is_visible());
    pump_until(|| shell.window().is_visible());
    assert_eq!(calls.get(), 1);
    assert_eq!(d.shutdown_requests(), 0);
    shell.mailbox().quit().unwrap();
    shell.tick();
    assert_eq!(d.shutdown_requests(), 1);
    finish(&entry, &d);
}
#[gtk::test]
fn explicit_settings_initial_load_then_unsent_final_edit_is_saved_before_exit() {
    let root = std::env::temp_dir().join(format!(
        "pecan-startup-synthetic-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("settings.json");
    let worker = SettingsWorker::start(SettingsFile::new(&path).unwrap()).unwrap();
    let (entry, d, calls) = setup(Options::default(), Some(worker));
    let shell = entry.shell().unwrap();
    pump_until(|| {
        shell.tick();
        shell.preferences().editing_allowed()
    });
    shell
        .edit_display(DisplayEdit::DestinationWidth(Some(275.)))
        .unwrap();
    // No intervening tick: quit must admit this coalesced, unsent document.
    finish(&entry, &d);
    let loaded = SettingsFile::new(&path).unwrap().load().unwrap();
    assert_eq!(
        loaded.settings().preferences().destination_width_px,
        Some(275.)
    );
    assert!(!shell.settings_summary().unsaved_preferences);
    assert_eq!(calls.get(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
#[gtk::test]
fn functional_handoff_waits_for_receipt_and_graph_then_history_and_patch_share_session() {
    let (entry, d, _) = setup(
        Options {
            patchbay: true,
            ..Default::default()
        },
        None,
    );
    let shell = entry.shell().unwrap();
    let session = shell.session();
    let origin = session.graph().unwrap();
    session
        .execute(
            &origin,
            "Synthetic mute",
            Action::Mute {
                node: 20,
                muted: true,
            },
        )
        .unwrap();
    let request = d.take_requests().pop().unwrap();
    assert_eq!(session.history_lengths(), (0, 0));
    request.reporter.set(NativeProgress::Observed {
        stamp: pecan_core::backend::Stamp {
            epoch: origin.stamp.epoch,
            revision: origin.stamp.revision + 1,
        },
        counts: Counts::default(),
    });
    shell.tick();
    assert!(!session.enabled());
    assert_eq!(session.history_lengths(), (0, 0));
    let mut next = (*origin.value).clone();
    next.knopen.iter_mut().find(|n| n.id == 20).unwrap().mute = Some(true);
    let current = d.publish(next.clone(), generations(&next));
    shell.tick();
    assert_eq!(session.history_lengths(), (1, 0));
    session.undo(&current).unwrap();
    assert_eq!(d.take_requests().len(), 1);
    finish(&entry, &d);
}
