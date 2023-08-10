pub use gtk;

use gtk::glib;

#[cfg(not(feature = "bindings"))]
mod pure {
    use super::glib::{self, flags, translate::IntoGlib, Enum, StaticType};

    #[derive(Debug, Eq, PartialEq, Clone, Copy, Enum)]
    #[enum_type(name = "RdwScroll")]
    #[repr(C)]
    pub enum Scroll {
        Up,
        Down,
        Left,
        Right,
    }

    pub type RdwScroll = <Scroll as IntoGlib>::GlibType;

    pub const RDW_SCROLL_UP: RdwScroll = Scroll::Up as i32;
    pub const RDW_SCROLL_DOWN: RdwScroll = Scroll::Down as i32;
    pub const RDW_SCROLL_LEFT: RdwScroll = Scroll::Left as i32;
    pub const RDW_SCROLL_RIGHT: RdwScroll = Scroll::Right as i32;

    #[no_mangle]
    pub unsafe extern "C" fn rdw_scroll_get_type() -> glib::ffi::GType {
        Scroll::static_type().into_glib()
    }

    #[flags(name = "RdwKeyEvent")]
    #[repr(C)] // See https://github.com/bitflags/bitflags/pull/187
    pub enum KeyEvent {
        PRESS = 0b0000_0001,
        RELEASE = 0b0000_0010,
    }

    pub type RdwKeyEvent = <KeyEvent as IntoGlib>::GlibType;

    pub const RDW_KEY_EVENT_PRESS: RdwKeyEvent = KeyEvent::PRESS.bits();
    pub const RDW_KEY_EVENT_RELEASE: RdwKeyEvent = KeyEvent::RELEASE.bits();

    #[no_mangle]
    pub unsafe extern "C" fn rdw_key_event_get_type() -> glib::ffi::GType {
        KeyEvent::static_type().into_glib()
    }

    #[flags(name = "RdwGrab")]
    #[repr(C)]
    pub enum Grab {
        MOUSE = 0b0000_0001,
        KEYBOARD = 0b0000_0010,
    }

    pub type RdwGrab = <Grab as IntoGlib>::GlibType;

    pub const RDW_GRAB_MOUSE: RdwGrab = Grab::MOUSE.bits();
    pub const RDW_GRAB_KEYBOARD: RdwGrab = Grab::KEYBOARD.bits();

    #[no_mangle]
    pub unsafe extern "C" fn rdw_grab_get_type() -> glib::ffi::GType {
        Grab::static_type().into_glib()
    }
}

#[cfg(not(feature = "bindings"))]
pub use pure::*;

/// cbindgen:ignore
#[cfg(feature = "bindings")]
mod ffi {
    use super::glib;
    use std::os::raw::{c_int, c_uint};

    pub type RdwScroll = c_int;

    pub const RDW_SCROLL_UP: RdwScroll = 0;
    pub const RDW_SCROLL_DOWN: RdwScroll = 1;
    pub const RDW_SCROLL_LEFT: RdwScroll = 2;
    pub const RDW_SCROLL_RIGHT: RdwScroll = 3;

    extern "C" {
        pub fn rdw_scroll_get_type() -> glib::ffi::GType;
    }

    pub type RdwKeyEvent = c_uint;

    pub const RDW_KEY_EVENT_PRESS: RdwKeyEvent = 0b0000_0001;
    pub const RDW_KEY_EVENT_RELEASE: RdwKeyEvent = 0b0000_0010;

    extern "C" {
        pub fn rdw_key_event_get_type() -> glib::ffi::GType;
    }

    pub type RdwGrab = c_uint;

    pub const RDW_GRAB_MOUSE: RdwGrab = 0b0000_0001;
    pub const RDW_GRAB_KEYBOARD: RdwGrab = 0b0000_0010;

    extern "C" {
        pub fn rdw_grab_get_type() -> glib::ffi::GType;
    }
}

#[cfg(feature = "bindings")]
pub use ffi::*;
