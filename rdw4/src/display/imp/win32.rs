use std::cell::Cell;

use glib::clone;
use gtk::{glib, prelude::*, subclass::prelude::*};
use windows::Win32::{
    Devices::HumanInterfaceDevice::{HID_USAGE_GENERIC_MOUSE, HID_USAGE_PAGE_GENERIC},
    UI::{
        Input::{
            GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
            RAWINPUTHEADER, RIDEV_INPUTSINK, RID_INPUT, RIM_TYPEMOUSE,
        },
        WindowsAndMessaging::{HHOOK, WM_INPUT},
    },
};

use crate::win32;

#[derive(Default)]
pub(crate) struct Helper {
    mouse: Cell<[isize; 3]>,
    mouse_speed: Cell<isize>,
    filter: Cell<Option<gdk_win32::Win32DisplayFilterHandle>>,
    hook: Cell<Option<HHOOK>>,
    mouse_hook: Cell<Option<HHOOK>>,
}

impl Helper {
    pub(crate) fn realize(&self, display: &crate::Display, dpy: &gdk_win32::Win32Display) {
        let Some(hwnd) = display.imp().win32_handle() else {
            log::warn!("Failed to get windows handle");
            return;
        };
        let rid = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };
        if let Err(e) =
            unsafe { RegisterRawInputDevices(&[rid], std::mem::size_of_val(&rid) as _).ok() }
        {
            log::warn!("Failed to RegisterRawInputDevices: {e}");
            return;
        }

        let filter = dpy.add_filter(clone!(
            #[weak]
            display,
            #[upgrade_or_panic]
            move |_, msg, _rv| {
                if !display.imp().grabbed.get().contains(crate::Grab::MOUSE)
                    || msg.message != WM_INPUT
                {
                    return gdk_win32::Win32MessageFilterReturn::Continue;
                }

                let mut input = RAWINPUT::default();
                let mut pcbsize = std::mem::size_of_val(&input) as u32;
                unsafe {
                    let res = GetRawInputData(
                        HRAWINPUT(msg.lParam.0),
                        RID_INPUT,
                        Some(&mut input as *mut _ as *mut _),
                        &mut pcbsize as *mut _,
                        std::mem::size_of::<RAWINPUTHEADER>() as _,
                    );
                    if res == u32::MAX {
                        log::warn!("Failed to GetRawInputData");
                    }
                    if input.header.dwType == RIM_TYPEMOUSE.0 {
                        let (dx, dy) = (input.data.mouse.lLastX, input.data.mouse.lLastY);
                        let scale = display.scale_factor() as f64;
                        let (dx, dy) = (dx as f64 / scale, dy as f64 / scale);
                        display.imp().do_motion_relative(dx, dy);
                    }
                }

                gdk_win32::Win32MessageFilterReturn::Continue
            }
        ));

        self.filter.set(Some(filter));
    }

    pub(crate) fn try_grab_device(
        &self,
        display: &crate::Display,
        _device: gtk::gdk::Device,
    ) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetWindowRect};

        let Some(h) = display.imp().win32_handle() else {
            return false;
        };
        let mut win_rect = unsafe { std::mem::zeroed() };
        if let Err(e) = unsafe { GetWindowRect(h, &mut win_rect).ok() } {
            log::warn!("Failed to GetWindowRect: {e}");
            return false;
        }

        // a very small clip, hopefully in the center of our widget.
        // FIXME: find real coordinates of our own widget instead
        win_rect.left = (win_rect.left + win_rect.right) / 2;
        win_rect.right = win_rect.left + 1;
        win_rect.top = (win_rect.top + win_rect.bottom) / 2;
        win_rect.bottom = win_rect.top + 1;

        if let Err(e) = unsafe { ClipCursor(Some(&win_rect)).ok() } {
            log::warn!("Failed to ClipCursor: {e}");
            return false;
        }

        match win32::hook_mouse() {
            Ok(h) => self.mouse_hook.set(Some(h)),
            Err(e) => log::warn!("Failed to set mouse hook: {}", e),
        }

        true
    }

    pub(crate) fn save_accel_mouse(&self) {
        match win32::spi_get_mouse() {
            Ok(mouse) => self.mouse.set(mouse),
            Err(e) => log::warn!("Failed to spi_get_mouse: {e}"),
        }
        match win32::spi_get_mouse_speed() {
            Ok(speed) => self.mouse_speed.set(speed),
            Err(e) => log::warn!("Failed to spi_get_mouse: {e}"),
        }

        let mouse: [isize; 3] = Default::default();
        if let Err(e) = win32::spi_set_mouse(mouse) {
            log::warn!("Failed to spi_set_mouse: {e}");
        }
        if let Err(e) = win32::spi_set_mouse_speed(10) {
            log::warn!("Failed to spi_set_mouse_speed: {e}");
        }
    }

    pub(crate) fn restore_accel_mouse(&self) {
        if let Err(e) = win32::spi_set_mouse(self.mouse.get()) {
            log::warn!("Failed to spi_set_mouse: {e}");
        }
        if let Err(e) = win32::spi_set_mouse_speed(self.mouse_speed.get()) {
            log::warn!("Failed to spi_set_mouse_speed: {e}");
        }
    }

    pub(crate) fn unrealize(&self) {}

    pub(crate) fn grab_keyboard(&self) {
        match win32::hook_keyboard() {
            Ok(h) => self.hook.set(Some(h)),
            Err(e) => log::warn!("Failed to set keyboard hook: {}", e),
        }
    }

    pub(crate) fn ungrab_keyboard(&self) {
        if let Some(h) = self.hook.take() {
            let _ = win32::unhook(h);
        }
    }

    pub(crate) fn ungrab_mouse(&self) {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::ClipCursor(None);
            if let Some(h) = self.mouse_hook.take() {
                let _ = win32::unhook(h);
            }
        }
    }
}
