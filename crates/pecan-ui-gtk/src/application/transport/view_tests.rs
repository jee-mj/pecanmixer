use super::super::tests::{snapshot, Temp};
use super::*;
fn pump() {
    let end = std::time::Instant::now() + Duration::from_millis(40);
    let context = glib::MainContext::default();
    while std::time::Instant::now() < end {
        while context.pending() && std::time::Instant::now() < end {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
struct Window(gtk::Window);
impl Drop for Window {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

#[gtk::test]
fn folder_widget_retains_pending_across_publications_and_tick_storms_do_not_rebuild_controls() {
    let temp = Temp::new();
    let view = TransportControls::new();
    view.observe(snapshot(&temp.0, Duration::from_secs(1)))
        .unwrap();
    assert!(!view.0.folder.borrow().as_ref().unwrap().is_sensitive());
    let requests = Rc::new(RefCell::new(Vec::new()));
    let seen = requests.clone();
    view.bind_folder(move |request| {
        seen.borrow_mut().push(request);
        Ok(())
    });
    assert_eq!(view.0.status.text(), "Ready");
    let window = Window(
        gtk::Window::builder()
            .child(view.widget())
            .default_width(360)
            .default_height(280)
            .build(),
    );
    window.0.present();
    pump();
    let old = view.0.folder.borrow().as_ref().unwrap().clone();
    let before = view.0.renders.get();
    for n in 0..1000 {
        view.tick(Duration::from_millis(1000 + n));
    }
    for n in 0..1000 {
        let mut s = snapshot(&temp.0, Duration::from_millis(1000 + n));
        s.takes[0].tracks[0].elapsed = Some(Duration::from_millis(5000 + n));
        view.update_elapsed(s.activity, &s.takes, s.observed_at)
            .unwrap();
    }
    assert_eq!(view.0.renders.get(), before);
    assert_eq!(view.0.folder.borrow().as_ref(), Some(&old));
    assert_eq!(
        view.offer(snapshot(&temp.0, Duration::from_millis(1500))),
        Err(Error::OldSnapshot)
    );
    assert!(old.is_sensitive());
    let long_root = temp.0.join("synthetic-long-label".repeat(100));
    for n in 0..1000 {
        view.offer(snapshot(&long_root, Duration::from_millis(2000 + n)))
            .unwrap();
    }
    assert!(!old.is_sensitive());
    old.emit_clicked();
    assert!(requests.borrow().is_empty());
    assert_eq!(view.0.renders.get(), before);
    pump();
    assert_eq!(view.0.renders.get(), before + 1);
    old.emit_clicked();
    assert!(requests.borrow().is_empty());
    {
        let button = view.0.folder.borrow().as_ref().unwrap().clone();
        button.emit_clicked();
    }
    let request = requests.borrow_mut().pop().unwrap();
    assert_eq!(request.paths.root(), long_root);
    assert_eq!(view.pending(), Some(request.token.clone()));
    view.observe(snapshot(&temp.0, Duration::from_secs(5)))
        .unwrap();
    assert!(!view.0.folder.borrow().as_ref().unwrap().is_sensitive());
    assert!(view.complete(
        &request.token,
        Completion::Failed("synthetic launch rejection".into())
    ));
    assert!(view.0.status.text().contains("synthetic launch rejection"));
    assert!(view.0.folder.borrow().as_ref().unwrap().is_sensitive());
    let detached = view.0.folder.borrow().as_ref().unwrap().clone();
    view.observe(snapshot(&long_root, Duration::from_secs(5)))
        .unwrap();
    detached.emit_clicked();
    assert!(
        requests.borrow().is_empty(),
        "a detached callback cannot use a new root"
    );
    let count = view.0.renders.get();
    let elapsed = view.0.elapsed.text();
    window.0.set_visible(false);
    pump();
    view.tick(Duration::from_secs(100));
    assert_eq!(view.0.renders.get(), count);
    assert_eq!(view.0.elapsed.text(), elapsed);
    window.0.present();
    pump();
    view.tick(Duration::from_secs(101));
    assert_eq!(view.0.elapsed.text(), "1:41.0");
}

#[gtk::test]
fn reentrant_folder_completion_does_not_fabricate_recording_or_clobber_results() {
    let temp = Temp::new();
    let view = TransportControls::new();
    view.observe(snapshot(&temp.0, Duration::ZERO)).unwrap();
    let plan = FolderPaths::new(temp.0.clone(), Some(temp.0.join("Take 1")))
        .unwrap()
        .prepare()
        .unwrap();
    let weak = Rc::downgrade(&view.0);
    view.bind_folder(move |request| {
        TransportControls(weak.upgrade().unwrap())
            .complete(&request.token, Completion::Submitted(plan.clone()));
        Err("error after already reported handoff".into())
    });
    {
        let button = view.0.folder.borrow().as_ref().unwrap().clone();
        button.emit_clicked();
    }
    assert!(view.pending().is_none());
    assert!(view.0.status.text().contains("Folder request submitted"));
    assert!(!view.0.status.text().contains("error after"));
    let mut s = snapshot(&temp.0, Duration::from_secs(1));
    s.closing = true;
    view.observe(s).unwrap();
    assert!(!view.0.folder.borrow().as_ref().unwrap().is_sensitive());
}
