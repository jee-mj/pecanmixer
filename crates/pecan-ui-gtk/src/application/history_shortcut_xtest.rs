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
fn xtest_application_history_repeats_do_not_retarget_after_completion_or_modifier_change() {
    for undo in [true, false] {
        let mut f = fixture(undo);
        focus(&f);
        let key = if undo { Z } else { Y };
        f.input.key(CTRL, true);
        f.input.key(key, true);
        f.origin = complete(&f.s, &f.d, &f.origin, &f.gens);
        f.view.poll();
        assert_eq!(f.s.history_lengths(), (1, 1));
        settle(750); // Let the real X server autorepeat while no receipt is pending.
        assert!(f.d.take_requests().is_empty());
        // Acknowledgement/refresh, modifier drift, and moving through a text
        // editor cannot turn this same physical activation into another action.
        f.input.key(key, true);
        f.input.key(SHIFT, true);
        f.input.key(key, true);
        f.input.key(SHIFT, false);
        assert!(f.outside.grab_focus());
        f.input.key(key, true);
        focus(&f);
        f.input.key(key, true);
        assert!(f.d.take_requests().is_empty());
        f.input.key(key, false);
        f.input.key(key, true);
        assert_request(&f, if undo { 10 } else { 20 }, undo);
        f.input.key(key, false);
        f.input.key(CTRL, false);
        f.window.close();
    }
}
#[gtk::test]
fn xtest_application_history_busy_attempt_cannot_wake_on_receipt() {
    let mut f = fixture(true);
    focus(&f);
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    f.input.key(Z, false);
    assert!(f.s.busy());
    f.input.key(Y, true); // rejected while Undo is pending
    f.origin = complete(&f.s, &f.d, &f.origin, &f.gens);
    f.view.poll();
    f.input.key(Y, true);
    assert!(f.d.take_requests().is_empty());
    f.input.key(Y, false);
    f.input.key(Y, true);
    assert_request(&f, 20, false);
    f.input.key(Y, false);
    f.input.key(CTRL, false);
    f.window.close();
}
#[gtk::test]
fn xtest_application_history_editors_keep_native_undo_and_require_fresh_global_press() {
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
    f.input.key(Z, true);
    focus(&f);
    f.input.key(Z, true);
    assert!(
        f.d.take_requests().is_empty(),
        "an editor-held key must not adopt graph history"
    );
    f.input.key(Z, false);
    f.input.key(Z, true);
    assert_request(&f, 20, true);
    f.input.key(Z, false);
    f.input.key(CTRL, false);
    f.window.close();
}
#[gtk::test]
fn xtest_application_history_replacement_focus_loss_and_unmap_preserve_release_boundary() {
    let mut f = fixture(true);
    focus(&f);
    f.input.key(Z, true); // no modifier: no action, but it is already held
    prune(&mut f, true);
    f.input.key(CTRL, true);
    f.input.key(Z, true);
    assert!(f.d.take_requests().is_empty());
    let other = gtk::Window::builder()
        .child(&gtk::Button::with_label("Synthetic other window"))
        .build();
    other.present();
    let other_input = Input::new(&other);
    f.input.focus();
    focus(&f);
    f.input.key(Z, true);
    f.window.set_visible(false);
    settle(30);
    f.window.present();
    f.input.focus();
    focus(&f);
    f.input.key(Z, true);
    assert!(
        f.d.take_requests().is_empty(),
        "focus/unmap must not clear a held-key latch"
    );
    f.input.key(Z, false);
    f.input.key(Z, true);
    assert_request(&f, 10, true);
    f.input.key(Z, false);
    f.input.key(CTRL, false);
    drop(other_input);
    other.close();
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
