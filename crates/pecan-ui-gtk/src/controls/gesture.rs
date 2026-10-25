//! Per-control input lifetime. A graph refresh never substitutes the origin of
//! an already-started input sequence, including after an acknowledgement.
use super::{Adapter, Binding, ControlError};
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Input {
    Mouse(u32),
    Touch(gtk::gdk::EventSequence),
    Key(u32),
    Scroll,
}
pub(super) struct GestureOrigin<A: Adapter> {
    presented: Binding<A::Snapshot>,
    captured: Option<Binding<A::Snapshot>>,
    inputs: Vec<Input>,
    cancelled: bool,
    retired: bool,
}
impl<A: Adapter> GestureOrigin<A> {
    pub(super) fn new(presented: Binding<A::Snapshot>) -> Self {
        Self {
            presented,
            captured: None,
            inputs: vec![],
            cancelled: false,
            retired: false,
        }
    }
    pub(super) fn present(&mut self, binding: Binding<A::Snapshot>) {
        if self.captured.as_ref().is_some_and(|captured| {
            let a = A::graph(&captured.snapshot);
            let b = A::graph(&binding.snapshot);
            captured.key != binding.key || a.stamp != b.stamp || !Arc::ptr_eq(&a.value, &b.value)
        }) {
            self.cancelled = true;
        }
        self.presented = binding;
    }
    pub(super) fn begin(&mut self, input: Input) {
        if self.retired || self.inputs.contains(&input) {
            return;
        }
        if self.inputs.is_empty() {
            self.captured = Some(self.presented.clone());
            self.cancelled = false;
        } else {
            // Overlapping input devices must not rebind each other's gesture.
            self.cancelled = true;
        }
        self.inputs.push(input);
    }
    pub(super) fn end(&mut self, input: &Input) {
        self.inputs.retain(|active| active != input);
        // Keep the captured/cancelled binding through release-time callbacks.
        // Only a fresh input begin may rearm against the latest presentation.
    }
    pub(super) fn cancel(&mut self) {
        self.cancelled = true;
    }
    pub(super) fn focus_enter(&mut self) {
        // GTK may focus a control after capture-phase button press. Preserve
        // that press; focus must never rearm an already-cancelled held input.
        if self.inputs.is_empty() {
            self.cancel();
        }
    }
    pub(super) fn cancel_pointer(&mut self) {
        self.inputs
            .retain(|input| !matches!(input, Input::Mouse(_) | Input::Touch(_)));
        self.cancel();
    }
    pub(super) fn retire(&mut self) {
        self.cancel();
        self.retired = true;
    }
    pub(super) fn binding(&self) -> Result<Binding<A::Snapshot>, ControlError> {
        if self.cancelled || self.retired {
            return Err(ControlError::StaleView);
        }
        Ok(self.captured.as_ref().unwrap_or(&self.presented).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::tests::{app_binding, setup, Fake};
    #[test]
    fn refresh_cancels_held_inputs_and_release_does_not_adopt_latest_origin() {
        for input in [Input::Mouse(1), Input::Key(42), Input::Scroll] {
            let (mut controller, log, mut graph) = setup();
            let mut origin = GestureOrigin::<Fake>::new(app_binding(&controller));
            origin.begin(input.clone());
            graph.graph.stamp.revision += 1;
            controller.observe(graph);
            origin.present(app_binding(&controller));
            origin.begin(input.clone()); // key repeat / continuing smooth scroll
            assert!(matches!(origin.binding(), Err(ControlError::StaleView)));
            origin.end(&input);
            assert!(matches!(origin.binding(), Err(ControlError::StaleView)));
            assert!(log.borrow().calls.is_empty());
            origin.begin(input);
            controller
                .set_volume(&origin.binding().unwrap(), -6.0)
                .unwrap();
            assert_eq!(log.borrow().calls.len(), 1);
            assert_eq!(log.borrow().calls[0].1.revision, 2);
        }
    }
    #[test]
    fn focus_after_button_capture_preserves_origin_and_never_rearms_cancelled_input() {
        let (mut controller, _, mut graph) = setup();
        let mut origin = GestureOrigin::<Fake>::new(app_binding(&controller));
        origin.begin(Input::Mouse(1));
        origin.focus_enter();
        assert!(origin.binding().is_ok());
        graph.graph.stamp.revision += 1;
        controller.observe(graph);
        origin.present(app_binding(&controller));
        origin.focus_enter();
        origin.begin(Input::Mouse(1));
        assert!(matches!(origin.binding(), Err(ControlError::StaleView)));
        origin.cancel_pointer();
        assert!(matches!(origin.binding(), Err(ControlError::StaleView)));
        origin.begin(Input::Mouse(1));
        assert!(origin.binding().is_ok());
    }
    #[test]
    fn overlapping_inputs_and_retired_rows_cannot_rearm_an_old_callback() {
        let (controller, _, _) = setup();
        let mut origin = GestureOrigin::<Fake>::new(app_binding(&controller));
        origin.begin(Input::Mouse(1));
        origin.begin(Input::Key(42));
        assert!(matches!(origin.binding(), Err(ControlError::StaleView)));
        origin.end(&Input::Mouse(1));
        origin.begin(Input::Key(42));
        assert!(matches!(origin.binding(), Err(ControlError::StaleView)));
        origin.retire();
        origin.focus_enter();
        origin.begin(Input::Mouse(1));
        assert!(matches!(origin.binding(), Err(ControlError::StaleView)));
    }
}
