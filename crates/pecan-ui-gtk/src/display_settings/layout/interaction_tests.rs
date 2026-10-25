//! Real allocated GTK widgets, synthetic gesture signals; no audio or profile.
use super::*;
use crate::display_settings::{RowGrip, RowResize, SplitResize};
use pecan_core::settings::{RowWeights, SettingsV1};
#[test]
fn resize_math_preserves_pair_sum_and_other_row_with_legacy_minima() {
    let weights = RowWeights {
        apps: 3.,
        hardware: 1.,
        destinations: 2.,
    };
    for grip in [RowGrip::AppsHardware, RowGrip::HardwareDestinations] {
        let r = RowResize::new(grip, weights, 200., 100.).unwrap();
        let first = r.drag(-10_000.).unwrap();
        let last = r.drag(10_000.).unwrap();
        match grip {
            RowGrip::AppsHardware => {
                assert_eq!(first.destinations, 2.);
                assert!((first.apps - 4. * 110. / 300.).abs() < 1e-10);
                assert!((last.hardware - 4. * 110. / 300.).abs() < 1e-10);
            }
            RowGrip::HardwareDestinations => {
                assert_eq!(first.apps, 3.);
                assert!((first.hardware - 3. * 110. / 300.).abs() < 1e-10);
                assert!((last.destinations - 3. * 110. / 300.).abs() < 1e-10);
            }
        }
    }
    let tiny = RowResize::new(RowGrip::AppsHardware, weights, 40., 50.)
        .unwrap()
        .drag(-100.)
        .unwrap();
    assert_eq!(tiny.apps, 2.);
    assert_eq!(tiny.hardware, 2.);
    assert_eq!(tiny.destinations, 2.);
    assert!(RowResize::new(RowGrip::AppsHardware, weights, 0., 0.).is_none());
    assert_eq!(
        SplitResize::new(200., 800., 8.).unwrap().drag(-1000.),
        Some(120.)
    );
    assert_eq!(
        SplitResize::new(200., 800., 8.).unwrap().drag(1000.),
        Some(672.)
    );
    assert_eq!(
        SplitResize::new(200., 200., 8.).unwrap().drag(0.),
        Some(120.)
    );
    assert_eq!(
        SplitResize::new(200., 800., 8.).unwrap().drag(0.5),
        Some(201.)
    );
}
fn settle() {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(100);
    let context = glib::MainContext::default();
    while std::time::Instant::now() < until {
        while context.pending() && std::time::Instant::now() < until {
            context.iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
#[gtk::test]
fn programmatic_application_and_allocation_do_not_emit_saves() {
    let view = PreferenceLayout::new();
    for lane in &view.0.lanes {
        lane.append(&gtk::Label::new(Some("Synthetic lane")));
    }
    view.changed();
    let edits = Rc::new(RefCell::new(Vec::new()));
    let output = edits.clone();
    view.edit_handler(Rc::new(move |edit| output.borrow_mut().push(edit)));
    view.set_editable(true);
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(900)
        .default_height(900)
        .build();
    window.present();
    settle();
    for width in [200., 250., 300.] {
        let mut p = SettingsV1::default().into_preferences();
        p.destination_width_px = Some(width);
        view.set_preferences(DisplayPreferences::from(&SettingsV1::new(p).unwrap()));
        window.set_default_size(700, 700);
        settle();
    }
    assert!(edits.borrow().is_empty());
    window.close();
}
