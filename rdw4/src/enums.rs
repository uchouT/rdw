use bitflags::bitflags;

use gtk::glib::{
    self,
    translate::{from_glib, FromGlib, IntoGlib, ToGlibPtr, ToGlibPtrMut},
    value::*,
    StaticType, Type,
};

use crate::ffi;

/// cbindgen:ignore
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
#[non_exhaustive]
#[repr(C)]
pub enum Scroll {
    Up,
    Down,
    Left,
    Right,
    __Unknown(i32),
}

impl IntoGlib for Scroll {
    type GlibType = ffi::RdwScroll;

    fn into_glib(self) -> ffi::RdwScroll {
        match self {
            Scroll::Up => ffi::RDW_SCROLL_UP,
            Scroll::Down => ffi::RDW_SCROLL_DOWN,
            Scroll::Left => ffi::RDW_SCROLL_LEFT,
            Scroll::Right => ffi::RDW_SCROLL_RIGHT,
            Scroll::__Unknown(v) => v,
        }
    }
}

impl FromGlib<ffi::RdwScroll> for Scroll {
    unsafe fn from_glib(value: ffi::RdwScroll) -> Self {
        match value {
            ffi::RDW_SCROLL_UP => Self::Up,
            ffi::RDW_SCROLL_DOWN => Self::Down,
            ffi::RDW_SCROLL_LEFT => Self::Left,
            ffi::RDW_SCROLL_RIGHT => Self::Right,
            value => Self::__Unknown(value),
        }
    }
}

impl StaticType for Scroll {
    fn static_type() -> Type {
        unsafe { from_glib(ffi::rdw_scroll_get_type()) }
    }
}

impl ValueType for Scroll {
    type Type = Self;
}

unsafe impl<'a> FromValue<'a> for Scroll {
    type Checker = GenericValueTypeChecker<Self>;

    unsafe fn from_value(value: &'a Value) -> Self {
        from_glib(glib::gobject_ffi::g_value_get_enum(
            ToGlibPtr::to_glib_none(value).0,
        ))
    }
}

impl ToValue for Scroll {
    fn to_value(&self) -> Value {
        let mut value = Value::for_value_type::<Self>();
        unsafe {
            glib::gobject_ffi::g_value_set_enum(
                ToGlibPtrMut::to_glib_none_mut(&mut value).0,
                IntoGlib::into_glib(*self),
            )
        }
        value
    }

    fn value_type(&self) -> Type {
        <Self as StaticType>::static_type()
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct Grab: u32 {
        const MOUSE = ffi::RDW_GRAB_MOUSE;
        const KEYBOARD = ffi::RDW_GRAB_KEYBOARD;
    }
}

impl IntoGlib for Grab {
    type GlibType = ffi::RdwGrab;

    fn into_glib(self) -> ffi::RdwGrab {
        self.bits()
    }
}

impl FromGlib<ffi::RdwGrab> for Grab {
    unsafe fn from_glib(value: ffi::RdwGrab) -> Self {
        Grab::from_bits_truncate(value)
    }
}

impl StaticType for Grab {
    fn static_type() -> Type {
        unsafe { from_glib(ffi::rdw_grab_get_type()) }
    }
}

impl ValueType for Grab {
    type Type = Self;
}

unsafe impl<'a> FromValue<'a> for Grab {
    type Checker = GenericValueTypeChecker<Self>;

    unsafe fn from_value(value: &'a Value) -> Self {
        from_glib(glib::gobject_ffi::g_value_get_flags(
            ToGlibPtr::to_glib_none(value).0,
        ))
    }
}

impl ToValue for Grab {
    fn to_value(&self) -> Value {
        let mut value = Value::for_value_type::<Self>();
        unsafe {
            glib::gobject_ffi::g_value_set_flags(
                ToGlibPtrMut::to_glib_none_mut(&mut value).0,
                IntoGlib::into_glib(*self),
            )
        }
        value
    }

    fn value_type(&self) -> Type {
        <Self as StaticType>::static_type()
    }
}

impl std::default::Default for Grab {
    fn default() -> Self {
        Self::empty()
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct KeyEvent: u32 {
        const PRESS = ffi::RDW_KEY_EVENT_PRESS;
        const RELEASE = ffi::RDW_KEY_EVENT_RELEASE;
    }
}

impl IntoGlib for KeyEvent {
    type GlibType = ffi::RdwKeyEvent;

    fn into_glib(self) -> ffi::RdwKeyEvent {
        self.bits()
    }
}

impl FromGlib<ffi::RdwKeyEvent> for KeyEvent {
    unsafe fn from_glib(value: ffi::RdwKeyEvent) -> Self {
        KeyEvent::from_bits_truncate(value)
    }
}

impl StaticType for KeyEvent {
    fn static_type() -> Type {
        unsafe { from_glib(ffi::rdw_key_event_get_type()) }
    }
}

impl ValueType for KeyEvent {
    type Type = Self;
}

unsafe impl<'a> FromValue<'a> for KeyEvent {
    type Checker = GenericValueTypeChecker<Self>;

    unsafe fn from_value(value: &'a Value) -> Self {
        from_glib(glib::gobject_ffi::g_value_get_flags(
            ToGlibPtr::to_glib_none(value).0,
        ))
    }
}

impl ToValue for KeyEvent {
    fn to_value(&self) -> Value {
        let mut value = Value::for_value_type::<Self>();
        unsafe {
            glib::gobject_ffi::g_value_set_flags(
                ToGlibPtrMut::to_glib_none_mut(&mut value).0,
                IntoGlib::into_glib(*self),
            )
        }
        value
    }

    fn value_type(&self) -> Type {
        <Self as StaticType>::static_type()
    }
}
