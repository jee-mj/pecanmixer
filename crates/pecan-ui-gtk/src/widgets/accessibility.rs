//! Minimal GTK 4.0 label binding. gtk4-rs 0.11 gates its Accessible wrapper on
//! 4.10 (when the interface became public to implement), although this function
//! is marked GDK_AVAILABLE_IN_ALL in gtkaccessible.h. Keep the preview's default
//! API floor while using GTK's native accessible names. This can disappear when
//! the supported distribution matrix selects GTK >= 4.10.
use gtk::{glib::translate::*, prelude::*};

unsafe extern "C" {
    fn gtk_accessible_update_property_value(
        accessible: *mut std::ffi::c_void,
        n_properties: std::ffi::c_int,
        properties: *mut gtk::ffi::GtkAccessibleProperty,
        values: *const gtk::glib::gobject_ffi::GValue,
    );
}

pub(crate) fn set_name(widget: &impl IsA<gtk::Widget>, name: &str) {
    let mut property = gtk::AccessibleProperty::Label.into_glib();
    let value = name.to_value();
    // SAFETY: every GtkWidget implements GtkAccessible, including on GTK 4.0.
    // The borrowed widget and single property/value are live for this call; GTK
    // copies the value. No pointers or GTK objects leave the main thread.
    unsafe {
        gtk_accessible_update_property_value(
            widget.as_ref().as_ptr().cast(),
            1,
            &mut property,
            value.to_glib_none().0,
        );
    }
}
