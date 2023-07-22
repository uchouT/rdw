#[cfg(all(unix, not(feature = "bindings")))]
use gdk_wl::wayland_client::{self, protocol::wl_registry};
#[cfg(all(unix, not(feature = "bindings")))]
use wayland_protocols::wp::{
    pointer_constraints::zv1::client::{
        zwp_locked_pointer_v1::ZwpLockedPointerV1,
        zwp_pointer_constraints_v1::ZwpPointerConstraintsV1,
    },
    relative_pointer::zv1::client::{
        zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
        zwp_relative_pointer_v1::{Event as RelEvent, ZwpRelativePointerV1},
    },
};

use gtk::{prelude::*, subclass::prelude::*};

use crate::Display;

impl wayland_client::Dispatch<wl_registry::WlRegistry, wayland_client::globals::GlobalListContents>
    for Display
{
    fn event(
        _state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &wayland_client::globals::GlobalListContents,
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
        log::trace!("{event:?}");
    }
}

impl wayland_client::Dispatch<ZwpRelativePointerManagerV1, ()> for Display {
    fn event(
        _state: &mut Self,
        _: &ZwpRelativePointerManagerV1,
        event: wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
        log::trace!("{event:?}");
    }
}

impl wayland_client::Dispatch<ZwpRelativePointerV1, ()> for Display {
    fn event(
        obj: &mut Self,
        _: &ZwpRelativePointerV1,
        event: wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
        if let RelEvent::RelativeMotion {
            dx_unaccel,
            dy_unaccel,
            ..
        } = event
        {
            let scale = obj.scale_factor() as f64;
            let (dx, dy) = (dx_unaccel / scale, dy_unaccel / scale);
            obj.imp().do_motion_relative(dx, dy)
        }
    }
}

impl wayland_client::Dispatch<ZwpPointerConstraintsV1, ()> for Display {
    fn event(
        _state: &mut Self,
        _: &ZwpPointerConstraintsV1,
        event: wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
        log::trace!("{event:?}");
    }
}

impl wayland_client::Dispatch<ZwpLockedPointerV1, ()> for Display {
    fn event(
        _state: &mut Self,
        _: &ZwpLockedPointerV1,
        event: wayland_protocols::wp::pointer_constraints::zv1::client::zwp_locked_pointer_v1::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
        log::trace!("{event:?}");
    }
}
