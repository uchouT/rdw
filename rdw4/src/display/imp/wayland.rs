use std::{
    cell::{Cell, RefCell},
    os::unix::io::AsRawFd,
};

use gdk_wl::wayland_client::{
    self, backend::Backend, globals::registry_queue_init, protocol::wl_registry, Connection,
};
use glib::translate::ToGlibPtr;
use gtk::{gdk, glib, prelude::*, subclass::prelude::*};
use wayland_protocols::wp::{
    pointer_constraints::zv1::client::{
        zwp_locked_pointer_v1::ZwpLockedPointerV1,
        zwp_pointer_constraints_v1::{self, ZwpPointerConstraintsV1},
    },
    relative_pointer::zv1::client::{
        zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
        zwp_relative_pointer_v1::{Event, ZwpRelativePointerV1},
    },
};

use crate::Display;

#[derive(Default)]
pub(crate) struct Helper {
    queue: RefCell<Option<gdk_wl::wayland_client::QueueHandle<Display>>>,
    source: Cell<Option<glib::SourceId>>,
    rel_manager: RefCell<Option<ZwpRelativePointerManagerV1>>,
    rel_pointer: RefCell<Option<ZwpRelativePointerV1>>,
    pointer_constraints: RefCell<Option<ZwpPointerConstraintsV1>>,
    lock_pointer: RefCell<Option<ZwpLockedPointerV1>>,
}

impl Helper {
    pub(crate) fn dispose(&self) {
        if let Some(source) = self.source.take() {
            source.remove();
        }
    }

    pub(crate) fn realize(&self, display: &Display, dpy: &gdk_wl::WaylandDisplay) {
        let wl_display =
            unsafe { gdk_wl::ffi::gdk_wayland_display_get_wl_display(dpy.to_glib_none().0) };
        let connection = Connection::from_backend(unsafe {
            Backend::from_foreign_display(wl_display as *mut _)
        });
        let (globals, mut queue) = registry_queue_init::<Display>(&connection).unwrap();

        let rel_manager = globals.bind(&queue.handle(), 1..=1, ()).unwrap();
        self.rel_manager.replace(Some(rel_manager));
        let pointer_constraints = globals.bind(&queue.handle(), 1..=1, ()).unwrap();
        self.pointer_constraints.replace(Some(pointer_constraints));

        let fd = connection
            .prepare_read()
            .unwrap()
            .connection_fd()
            .as_raw_fd();
        let source = glib::source::unix_fd_add_local(fd, glib::IOCondition::IN, move |_, _| {
            connection.prepare_read().unwrap().read().unwrap();
            glib::ControlFlow::Continue
        });

        self.queue.replace(Some(queue.handle()));
        glib::MainContext::default().spawn_local(
            glib::clone!(@weak display => @default-panic, async move {
                let mut obj = display.clone();
                std::future::poll_fn(|cx| queue.poll_dispatch_pending(cx, &mut obj)).await.unwrap();
            }),
        );
        self.source.set(Some(source))
    }

    pub(crate) fn unrealize(&self) {
        self.rel_manager.take();
        self.pointer_constraints.take();
        self.queue.take();
        self.source.set(None);
    }

    pub(crate) fn ungrab_mouse(&self) {
        if let Some(lock) = self.lock_pointer.take() {
            lock.destroy();
        }
        if let Some(rel_pointer) = self.rel_pointer.take() {
            rel_pointer.destroy();
        }
    }

    pub(crate) fn try_grab_device(&self, display: &Display, device: gdk::Device) -> bool {
        let device = match device.downcast::<gdk_wl::WaylandDevice>() {
            Ok(device) => device,
            _ => return false,
        };
        let pointer = device.wl_pointer().unwrap();
        let queue = self.queue.borrow();
        let handle = queue.as_ref().unwrap();

        if self.lock_pointer.borrow().is_none() {
            if let Some(constraints) = &*self.pointer_constraints.borrow() {
                if let Some(surf) = display.imp().wl_surface() {
                    let lock = constraints.lock_pointer(
                        &surf,
                        &pointer,
                        None,
                        zwp_pointer_constraints_v1::Lifetime::Persistent as _,
                        handle,
                        (),
                    );
                    self.lock_pointer.replace(Some(lock));
                }
            }
        }

        if self.rel_pointer.borrow().is_none() {
            if let Some(rel_manager) = &*self.rel_manager.borrow() {
                let rel_pointer = rel_manager.get_relative_pointer(&pointer, handle, ());
                self.rel_pointer.replace(Some(rel_pointer));
            }
        }

        true
    }
}

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
        if let Event::RelativeMotion {
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
