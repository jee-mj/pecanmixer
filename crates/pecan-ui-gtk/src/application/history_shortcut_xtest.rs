//! Reuse the existing private-display history fixture; no audio or real desktop.
use super::*;
const Z: c_ulong = 0x7a;
const Y: c_ulong = 0x79;
const CTRL: c_ulong = 0xffe3;
const SHIFT: c_ulong = 0xffe1;
const SUPER: c_ulong = 0xffeb;

fn focus(f: &Fixture) {
    f.view.widget().set_focusable(true);
    assert!(f.view.widget().grab_focus());
    settle(25);
}
fn assert_request(f: &Fixture, node: u32, undo: bool) {
    let requests = f.d.take_requests();
    assert_eq!(requests.len(), 1, "exactly one fresh history request");
    assert_eq!(
        requests[0].intent.commands,
        vec![Opdracht::ZetMute {
            knoop: node,
            mute: !undo
        }]
    );
}
#[gtk::test]
fn xtest_application_history_original_bindings_and_plain_key_control() {
    for (modifier, key, shift, undo) in [
        (CTRL, Z, false, true),
        (CTRL, Z, true, false),
        (CTRL, Y, false, false),
        (CTRL, Y, true, false),
        (SUPER, Z, false, true),
        (SUPER, Z, true, false),
        (SUPER, Y, false, false),
    ] {
        let f = fixture(undo);
        focus(&f);
        f.input.key(key, true);
        f.input.key(key, false);
        assert!(
            f.d.take_requests().is_empty(),
            "plain letters are not history shortcuts"
        );
        f.input.key(modifier, true);
        if shift {
            f.input.key(SHIFT, true);
        }
        f.input.key(key, true);
        assert_request(&f, if undo { 20 } else { 10 }, undo);
        f.input.key(key, false);
        if shift {
            f.input.key(SHIFT, false);
        }
        f.input.key(modifier, false);
        f.window.close();
    }
}
#[gtk::test]
fn xtest_application_history_standard_repeat_after_acknowledgement_captures_current_action() {
    for undo in [true, false] {
        let mut f = fixture(undo);
        focus(&f);
        let key = if undo { Z } else { Y };
        f.input.key(CTRL, true);
        f.input.key(key, true);
        f.origin = complete(&f.s, &f.d, &f.origin, &f.gens);
        f.view.poll();
        assert_eq!(f.s.history_lengths(), (1, 1));
        // Standard GTK/JS keydown semantics: the next delivered repeat may act
        // on the current guarded history. Further repeats hit the busy guard.
        settle(750);
        assert!(f.s.busy());
        assert_request(&f, if undo { 10 } else { 20 }, undo);
        f.input.key(key, false);
        f.input.key(CTRL, false);
        f.window.close();
    }
}
#[gtk::test]
fn xtest_application_history_busy_activation_is_dropped_without_queued_retry() {
    let mut f = fixture(true);
    focus(&f);
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    f.input.key(Z, false);
    assert!(f.s.busy());
    f.input.key(Y, true); // rejected while Undo is pending
    f.input.key(Y, false);
    f.origin = complete(&f.s, &f.d, &f.origin, &f.gens);
    f.view.poll();
    assert!(f.d.take_requests().is_empty());
    // A later delivered activation is a new attempt, not a retry of that event.
    f.input.key(Y, true);
    assert_request(&f, 20, false);
    f.input.key(Y, false);
    f.input.key(CTRL, false);
    f.window.close();
}
#[gtk::test]
fn xtest_application_history_editors_keep_native_undo_redo() {
    let f = fixture(true);
    f.outside.set_enable_undo(true);
    assert!(f.outside.grab_focus());
    f.input.key(0x61, true); // actual text input, not programmatic replacement
    f.input.key(0x61, false);
    assert_eq!(f.outside.text(), "a");
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    assert_eq!(
        f.outside.text(),
        "",
        "native text Undo must receive the event"
    );
    f.input.key(Z, false);
    f.input.key(SHIFT, true);
    f.input.key(Z, true);
    f.input.key(Z, false);
    f.input.key(SHIFT, false);
    assert_eq!(
        f.outside.text(),
        "a",
        "native text Redo must receive the event"
    );
    assert!(f.d.take_requests().is_empty());
    focus(&f);
    f.input.key(Z, true);
    assert_request(&f, 20, true);
    f.input.key(Z, false);
    f.input.key(CTRL, false);
    f.window.close();
}
#[gtk::test]
fn xtest_application_history_stale_presentation_is_rejected_without_retry() {
    let mut f = fixture(true);
    focus(&f);
    f.gens.insert(20, 100_020);
    f.origin = f.d.publish((*f.origin.value).clone(), f.gens.clone());
    f.s.poll(); // Deliberately accept new authority without refreshing old buttons.
    assert_eq!(f.s.history_lengths(), (1, 0));
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    assert!(
        f.d.take_requests().is_empty(),
        "the delivered activation cannot rebind a stale button context"
    );
    // Refresh during rejection is not permission to retry that activation.
    // A later keydown may synchronously capture the newly rendered authority.
    f.input.key(Z, false);
    f.input.key(Z, true);
    assert_request(&f, 10, true);
    f.input.key(Z, false);
    f.input.key(CTRL, false);
    f.window.close();
}
#[gtk::test]
fn xtest_application_history_pending_pointer_and_closed_admission_cannot_be_bypassed() {
    let f = fixture(true);
    let current = button(&f.view, true);
    f.input.point(&current, &f.window);
    f.input.button(true);
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    assert!(
        f.d.take_requests().is_empty(),
        "do not overlap held history button input"
    );
    f.input.key(Z, false);
    f.input.key(CTRL, false);
    f.input.key(ESCAPE, true);
    f.input.key(ESCAPE, false);
    f.input.button(false);
    f.s.close_admission();
    f.view.poll();
    focus(&f);
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    f.input.key(Z, false);
    f.input.key(CTRL, false);
    assert!(f.d.take_requests().is_empty());
    f.window.close();
}

fn off_window_return(release: bool) {
    let mut f = fixture(true);
    focus(&f);
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    f.origin = complete(&f.s, &f.d, &f.origin, &f.gens);
    f.view.poll();
    let other = gtk::Window::builder()
        .child(&gtk::Button::with_label("Synthetic off-window input"))
        .build();
    other.present();
    let other_input = Input::new(&other);
    if release {
        f.window.set_visible(false);
        other_input.key(Z, false);
        other_input.key(CTRL, false);
        other_input.key(CTRL, true);
        other_input.key(Z, true);
        other_input.key(Z, false);
        other_input.key(CTRL, false);
    }
    assert!(
        f.d.take_requests().is_empty(),
        "another window has no workspace shortcut controller"
    );
    f.window.present();
    f.input.focus();
    focus(&f);
    if release {
        f.input.key(CTRL, true);
        f.input.key(Z, true);
    } else {
        // No physical release is required by standard GTK activation semantics.
        // This delivered repeat captures current authority after focus return.
        settle(750);
    }
    assert_request(&f, 10, true);
    f.input.key(Z, false);
    f.input.key(CTRL, false);
    drop(other_input);
    other.close();
    f.window.close();
}
#[gtk::test]
fn xtest_application_history_off_window_release_then_fresh_shortcut() {
    off_window_return(true);
}
#[gtk::test]
fn xtest_application_history_held_return_may_deliver_a_new_standard_activation() {
    off_window_return(false);
}
#[gtk::test]
fn xtest_application_history_text_view_retains_native_undo_redo() {
    let f = fixture(true);
    let text = gtk::TextView::new();
    text.buffer().set_enable_undo(true);
    f.outside
        .parent()
        .unwrap()
        .downcast::<gtk::Box>()
        .unwrap()
        .append(&text);
    assert!(text.grab_focus());
    f.input.key(0x61, true);
    f.input.key(0x61, false);
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    f.input.key(Z, false);
    let buffer = text.buffer();
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        ""
    );
    f.input.key(SHIFT, true);
    f.input.key(Z, true);
    f.input.key(Z, false);
    f.input.key(SHIFT, false);
    f.input.key(CTRL, false);
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        "a"
    );
    assert!(f.d.take_requests().is_empty());
    f.window.close();
}
