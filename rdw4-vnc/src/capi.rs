use rdw::gtk::glib::{self, subclass::prelude::*, translate::*};

type RdwVncDisplay = <crate::imp::Display as ObjectSubclass>::Instance;

#[no_mangle]
pub extern "C" fn rdw_vnc_display_get_type() -> glib::ffi::GType {
    <crate::Display as glib::types::StaticType>::static_type().into_glib()
}

#[no_mangle]
pub extern "C" fn rdw_vnc_display_new() -> *mut RdwVncDisplay {
    <crate::Display>::new().to_glib_full()
}
