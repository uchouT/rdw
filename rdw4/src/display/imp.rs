use super::*;
use crate::picture::Picture;
#[cfg(windows)]
use crate::win32;
use glib::{clone, subclass::Signal, SourceId};
use gtk::{graphene, subclass::prelude::*};
use once_cell::sync::{Lazy, OnceCell};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    time::Duration,
};
#[cfg(windows)]
use windows::Win32::{
    Graphics::Direct3D11::{ID3D11Device1, ID3D11Texture2D},
    Graphics::Dxgi::IDXGIKeyedMutex,
    UI::WindowsAndMessaging::HHOOK,
};

#[cfg(all(unix, not(feature = "bindings")))]
mod wayland;

#[cfg(windows)]
pub(crate) struct D3d11TexGuard(IDXGIKeyedMutex);

#[cfg(windows)]
impl Drop for D3d11TexGuard {
    fn drop(&mut self) {
        if let Err(e) = unsafe {
            self.0
                .ReleaseSync(0)
                .map_err(|e| format!("Failed to release Mutex: {}", e))
        } {
            log::warn!("{:?}", e);
        }
    }
}

unsafe impl ClassStruct for RdwDisplayClass {
    type Type = Display;
}

unsafe impl InstanceStruct for RdwDisplay {
    type Type = Display;
}

#[derive(Default)]
pub struct Display {
    pub(crate) picture: Picture,

    pub(crate) scaling: Cell<bool>,
    pub(crate) show_local_cursor: Cell<bool>,
    pub(crate) read_only: Cell<bool>,
    // The remote display size, ex: 1024x768
    pub(crate) last_resize_request: Cell<Option<(u32, u32, u32, u32)>>,
    pub(crate) resize_timeout_id: Cell<Option<SourceId>>,
    // The currently defined cursor
    pub(crate) cursor: RefCell<Option<gdk::Cursor>>,
    pub(crate) mouse_absolute: Cell<bool>,
    // position of cursor when drawn by client
    pub(crate) cursor_position: Cell<Option<(i32, i32)>>,
    // press-and-release detection time in ms
    pub(crate) synthesize_delay: Cell<u32>,
    pub(crate) last_key_press: Cell<Option<(gdk::Key, u32)>>,
    pub(crate) last_key_press_timeout: Cell<Option<SourceId>>,
    pub(crate) keys_pressed: RefCell<HashSet<(gdk::Key, u32)>>,

    // the shortcut to ungrab key/mouse (to be configurable and extended with ctrl-alt)
    pub(crate) grab_shortcut: OnceCell<gtk::ShortcutTrigger>,
    pub(crate) grabbed: Cell<Grab>,
    pub(crate) shortcuts_inhibited_id: Cell<Option<SignalHandlerId>>,

    #[cfg(unix)]
    pub(crate) dmabuf: RefCell<Option<RdwDmabufScanout>>,

    #[cfg(unix)]
    wayland: wayland::Helper,

    #[cfg(windows)]
    pub(crate) win_mouse: Cell<[isize; 3]>,
    #[cfg(windows)]
    pub(crate) win_mouse_speed: Cell<isize>,
    #[cfg(windows)]
    pub(crate) win_filter: Cell<Option<gdk_win32::Win32DisplayFilterHandle>>,
    #[cfg(windows)]
    pub(crate) win_hook: Cell<Option<HHOOK>>,
    #[cfg(windows)]
    pub(crate) win_mouse_hook: Cell<Option<HHOOK>>,
    #[cfg(windows)]
    pub(crate) d3d11_device: OnceCell<ID3D11Device1>,
    #[cfg(windows)]
    pub(crate) d3d11_texture: RefCell<Option<ID3D11Texture2D>>,
    #[cfg(windows)]
    pub(crate) d3d11_scanout: RefCell<Option<RdwD3d11Texture2dScanout>>,
    #[cfg(windows)]
    pub(crate) d3d11_texture_can_acquire: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for Display {
    const NAME: &'static str = "RdwDisplay";
    type Type = super::Display;
    type ParentType = gtk::Widget;
    type Class = RdwDisplayClass;
    type Instance = RdwDisplay;

    fn class_init(_klass: &mut Self::Class) {
        // Load GL pointers from epoxy (GL context management library used by GTK).
        {
            #[cfg(target_os = "macos")]
            let library =
                unsafe { libloading::os::unix::Library::new("libepoxy.0.dylib") }.unwrap();
            #[cfg(all(unix, not(target_os = "macos")))]
            let library = unsafe { libloading::os::unix::Library::new("libepoxy.so.0") }.unwrap();
            #[cfg(windows)]
            let library = libloading::os::windows::Library::open_already_loaded("libepoxy-0.dll")
                .or_else(|_| libloading::os::windows::Library::open_already_loaded("epoxy-0.dll"))
                .unwrap();

            epoxy::load_with(|name| {
                unsafe { library.get::<_>(name.as_bytes()) }
                    .map(|symbol| *symbol)
                    .unwrap_or(std::ptr::null())
            });
            gl::load_with(epoxy::get_proc_addr);
            assert_eq!(unsafe { gl::GetError() }, gl::NO_ERROR);
        }
    }
}

impl ObjectImpl for Display {
    fn constructed(&self) {
        self.parent_constructed();
        self.obj().set_focusable(true);
        self.picture.set_parent(self.obj().as_ref());

        self.grab_shortcut.get_or_init(|| {
            gtk::ShortcutTrigger::parse_string("<Ctrl>Alt_L|<Alt>Control_L").unwrap()
        });

        let ec = gtk::EventControllerFocus::new();
        ec.connect_leave(clone!(@weak self as this => @default-panic, move |_ec| {
            this.release_keys();
        }));
        self.obj().add_controller(ec);

        let ec = gtk::EventControllerKey::new();
        ec.set_propagation_phase(gtk::PropagationPhase::Capture);
        ec.connect_key_pressed(
            clone!(@weak self as this => @default-panic, move |ec, keyval, keycode, _state| {
                this.key_pressed(ec, keyval, keycode);
                glib::ControlFlow::Break
            }),
        );
        ec.connect_key_released(
            clone!(@weak self as this => @default-panic, move |_ec, keyval, keycode, _state| {
                this.key_released(keyval, keycode);
            }),
        );
        self.obj().add_controller(ec);

        let ec = gtk::EventControllerMotion::new();
        ec.connect_motion(clone!(@weak self as this => move |_, x, y| {
            this.do_motion(x, y)
        }));
        ec.connect_enter(clone!(@weak self as this => move |_, x, y| {
            this.do_motion(x, y)
        }));
        ec.connect_leave(clone!(@weak self as this => move |_| {
            log::debug!("leave -> ungrab");
            this.ungrab();
        }));
        self.picture.add_controller(ec);

        let ec = gtk::GestureClick::new();
        ec.set_button(0);
        ec.connect_pressed(
            clone!(@weak self as this => @default-panic, move |gesture, _n_press, x, y| {
                let grabbed = this.try_grab();

                if grabbed.contains(Grab::MOUSE) {
                    log::debug!("Skipping mouse-press, since we took the grab");
                    return;
                }

                let button = gesture.current_button();
                this.do_motion(x, y);
                this.do_mouse_press(button);
            }),
        );
        ec.connect_released(
            clone!(@weak self as this => move |gesture, _n_press, x, y| {
                let button = gesture.current_button();
                this.do_motion(x, y);
                this.do_mouse_release(button);
            }),
        );
        ec.connect_cancel(clone!(@weak self as this => move |gesture, _| {
            let button = gesture.current_button();
            this.do_mouse_release(button);
        }));
        self.picture.add_controller(ec);

        let ec = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::BOTH_AXES | gtk::EventControllerScrollFlags::DISCRETE,
        );
        ec.connect_scroll(
            clone!(@weak self as this => @default-panic, move |_, dx, dy| {
                if dy >= 1.0 {
                    this.do_scroll_discrete(Scroll::Down);
                } else if dy <= -1.0 {
                    this.do_scroll_discrete(Scroll::Up);
                }
                if dx >= 1.0 {
                    this.do_scroll_discrete(Scroll::Right);
                } else if dx <= -1.0 {
                    this.do_scroll_discrete(Scroll::Left);
                }
                glib::ControlFlow::Continue
            }),
        );
        self.picture.add_controller(ec);
    }

    fn dispose(&self) {
        #[cfg(unix)]
        self.wayland.dispose();

        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }

    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
            vec![
                glib::ParamSpecObject::builder::<gtk::ShortcutTrigger>("grab-shortcut")
                    .nick("Grab shortcut")
                    .blurb("Input devices grab/ungrab shortcut")
                    .build(),
                glib::ParamSpecFlags::builder::<Grab>("grabbed")
                    .nick("grabbed")
                    .blurb("Grabbed")
                    .read_only()
                    .explicit_notify()
                    .default_value(Grab::empty())
                    .build(),
                glib::ParamSpecUInt::builder("synthesize-delay")
                    .nick("Synthesize delay")
                    .blurb("Press-and-release synthesize maximum time in ms")
                    .default_value(100)
                    .construct()
                    .build(),
                glib::ParamSpecBoolean::builder("mouse-absolute")
                    .nick("Mouse absolute")
                    .blurb("Whether the mouse is absolute or relative")
                    .construct()
                    .build(),
                glib::ParamSpecBoolean::builder("read-only")
                    .nick("Read-only")
                    .blurb("Do no send input events")
                    .explicit_notify()
                    .default_value(false)
                    .construct()
                    .build(),
                glib::ParamSpecBoolean::builder("show-local-cursor")
                    .nick("Show local cursor")
                    .blurb("Show local cursor")
                    .explicit_notify()
                    .default_value(false)
                    .construct()
                    .build(),
                glib::ParamSpecBoolean::builder("scaling")
                    .nick("Scaling")
                    .blurb("Scale display")
                    .explicit_notify()
                    .default_value(true)
                    .construct()
                    .build(),
            ]
        });
        PROPERTIES.as_ref()
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        match pspec.name() {
            "grab-shortcut" => {
                let shortcut = value.get().unwrap();
                self.grab_shortcut.set(shortcut).unwrap();
            }
            "synthesize-delay" => {
                let delay = value.get().unwrap();
                self.synthesize_delay.set(delay);
            }
            "mouse-absolute" => {
                let absolute = value.get().unwrap();
                if absolute {
                    self.ungrab_mouse();
                    self.update_cursor();
                }

                self.mouse_absolute.set(absolute);
            }
            "read-only" => {
                let ro = value.get().unwrap();
                self.set_read_only(ro)
            }
            "show-local-cursor" => {
                let val = value.get().unwrap();
                self.set_show_local_cursor(val)
            }
            "scaling" => {
                let val = value.get().unwrap();
                self.set_scaling(val)
            }
            _ => unimplemented!(),
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "grab-shortcut" => self.grab_shortcut.get().to_value(),
            "grabbed" => self.grabbed.get().to_value(),
            "synthesize-delay" => self.synthesize_delay.get().to_value(),
            "mouse-absolute" => self.mouse_absolute().to_value(),
            "read-only" => self.read_only().to_value(),
            "show-local-cursor" => self.show_local_cursor().to_value(),
            "scaling" => self.scaling().to_value(),
            _ => unimplemented!(),
        }
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
            vec![
                Signal::builder("key-event")
                    .param_types([
                        u32::static_type(),
                        u32::static_type(),
                        KeyEvent::static_type(),
                    ])
                    .build(),
                Signal::builder("motion")
                    .param_types([f64::static_type(), f64::static_type()])
                    .build(),
                Signal::builder("motion-relative")
                    .param_types([f64::static_type(), f64::static_type()])
                    .build(),
                Signal::builder("mouse-press")
                    .param_types([u32::static_type()])
                    .build(),
                Signal::builder("mouse-release")
                    .param_types([u32::static_type()])
                    .build(),
                Signal::builder("scroll-discrete")
                    .param_types([Scroll::static_type()])
                    .build(),
                Signal::builder("resize-request")
                    .param_types([
                        u32::static_type(),
                        u32::static_type(),
                        u32::static_type(),
                        u32::static_type(),
                    ])
                    .build(),
            ]
        });
        SIGNALS.as_ref()
    }
}

impl WidgetImpl for Display {
    fn realize(&self) {
        self.parent_realize();

        #[cfg(unix)]
        if let Ok(dpy) = self.obj().display().downcast::<gdk_wl::WaylandDisplay>() {
            self.wayland.realize(&self.obj(), &dpy);
        }

        #[cfg(windows)]
        if let Ok(dpy) = self.obj().display().downcast::<gdk_win32::Win32Display>() {
            self.realize_win32(&dpy);
        }
    }

    fn unrealize(&self) {
        #[cfg(unix)]
        if self
            .obj()
            .display()
            .downcast::<gdk_wl::WaylandDisplay>()
            .is_ok()
        {
            self.wayland.unrealize();
        }

        #[cfg(windows)]
        if self
            .obj()
            .display()
            .downcast::<gdk_win32::Win32Display>()
            .is_ok()
        {
            self.unrealize_win32();
        }

        self.parent_unrealize();
    }

    fn request_mode(&self) -> gtk::SizeRequestMode {
        gtk::SizeRequestMode::HeightForWidth
    }

    fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
        self.picture.measure(orientation, for_size)
    }

    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.parent_size_allocate(width, height, baseline);

        if let Some(timeout_id) = self.resize_timeout_id.take() {
            timeout_id.remove();
        }
        self.resize_timeout_id.set(Some(glib::timeout_add_local(
                Duration::from_millis(500),
                clone!(@weak self as this => @default-return glib::ControlFlow::Break, move || {
                    let sf = this.obj().scale_factor() as u32;
                    let width = width as u32 * sf;
                    let height = height as u32 * sf;
                    let (w_mm, h_mm) = this.surface()
                                   .as_ref()
                                   .and_then(|s| gdk::traits::DisplayExt::monitor_at_surface(&this.obj().display(), s))
                                   .map(|m| {
                                       let (geom, wmm, hmm) = (m.geometry(), m.width_mm() as u32, m.height_mm() as u32);
                                       (wmm * width / (geom.width() as u32), hmm * height / geom.height() as u32)
                                   }).unwrap_or((0u32, 0u32));
                    this.do_resize_request(width, height, w_mm, h_mm);
                    this.resize_timeout_id.set(None);
                    glib::ControlFlow::Break
                }),
            )));

        let (x, y, w, h) = self.paintable_area();
        self.picture
            .size_allocate(&gtk::Allocation::new(x, y, w, h), -1);
    }

    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        self.obj().snapshot_child(&self.picture, snapshot);

        if self.obj().mouse_absolute() {
            return;
        }
        if !self.grabbed.get().contains(Grab::MOUSE) {
            return;
        }
        let Some(pos) = self.cursor_position.get() else {
            return;
        };
        let Some(cursor) = &*self.cursor.borrow() else {
            return;
        };
        let Some(texture) = cursor.texture() else {
            return;
        };
        let (x, y) = self.transform_pos_inv(
            // do not take hotspot into account, at least for qemu.. to check with spice/vnc/rdp..
            (pos.0, pos.1),
        );

        let sf = self.obj().scale_factor();

        snapshot.append_texture(
            &texture,
            &graphene::Rect::new(
                x as f32,
                y as f32,
                (texture.width() / sf) as f32,
                (texture.height() / sf) as f32,
            ),
        );
    }

    fn grab_focus(&self) -> bool {
        self.picture.grab_focus()
    }
}

impl Display {
    fn paintable_area(&self) -> (i32, i32, i32, i32) {
        let (width, height) = (self.obj().width() as f64, self.obj().height() as f64);
        let display_ratio = width / height;
        let ratio = self.picture.paintable().intrinsic_aspect_ratio();

        let (w, h) = if ratio > display_ratio {
            (width, width / ratio)
        } else {
            (height * ratio, height)
        };

        let x = (width - w.ceil()) / 2.0;
        let y = (height - h.ceil()).floor() / 2.0;
        (x as _, y as _, w as _, h as _)
    }

    fn set_scaling(&self, scaling: bool) {
        if scaling == self.scaling() {
            return;
        }
        if scaling {
            self.obj().set_size_request(-1, -1);
        } else if let Some((width, height)) = self.obj().display_size() {
            self.obj().set_size_request(
                width as i32 / self.obj().scale_factor(),
                height as i32 / self.obj().scale_factor(),
            );
        }
        self.scaling.set(scaling);
        self.obj().notify("scaling");
        self.obj().queue_resize();
    }

    fn scaling(&self) -> bool {
        self.scaling.get()
    }

    pub(crate) fn update_cursor(&self) {
        if self.read_only() || self.show_local_cursor() {
            self.picture.set_cursor(None);
        } else if self.mouse_absolute() {
            self.picture.set_cursor(self.cursor.borrow().as_ref());
        } else if self.grabbed.get().contains(Grab::MOUSE) {
            self.picture.set_cursor_from_name(Some("none"));
        } else {
            self.picture.set_cursor(None);
        }
        self.obj().queue_draw();
    }

    fn set_show_local_cursor(&self, show: bool) {
        if show == self.show_local_cursor() {
            return;
        }
        self.show_local_cursor.set(show);
        self.update_cursor();
        self.obj().notify("show-local-cursor");
    }

    fn show_local_cursor(&self) -> bool {
        self.show_local_cursor.get()
    }

    fn set_read_only(&self, ro: bool) {
        if ro == self.read_only() {
            return;
        }
        if ro {
            self.ungrab();
        }
        self.read_only.set(ro);
        self.update_cursor();
        self.obj().notify("read-only");
    }

    fn read_only(&self) -> bool {
        self.read_only.get()
    }

    fn mouse_absolute(&self) -> bool {
        self.mouse_absolute.get()
    }

    fn do_motion(&self, x: f64, y: f64) {
        if self.read_only() {
            return;
        }
        self.obj().emit_by_name::<()>("motion", &[&x, &y]);
    }

    pub(crate) fn do_motion_relative(&self, dx: f64, dy: f64) {
        if self.read_only() {
            return;
        }
        self.obj()
            .emit_by_name::<()>("motion-relative", &[&dx, &dy]);
    }

    fn do_mouse_press(&self, button: u32) {
        if self.read_only() {
            return;
        }
        self.obj().emit_by_name::<()>("mouse-press", &[&button])
    }

    fn do_mouse_release(&self, button: u32) {
        if self.read_only() {
            return;
        }
        self.obj().emit_by_name::<()>("mouse-release", &[&button])
    }

    fn do_scroll_discrete(&self, dir: Scroll) {
        self.obj().emit_by_name::<()>("scroll-discrete", &[&dir])
    }

    pub(crate) fn do_key_press(&self, keyval: gdk::Key, keycode: u32) {
        if self.read_only() {
            return;
        }
        self.obj()
            .emit_by_name::<()>("key-event", &[&keyval, &keycode, &KeyEvent::PRESS])
    }

    pub(crate) fn do_key_release(&self, keyval: gdk::Key, keycode: u32) {
        if self.read_only() {
            return;
        }
        self.obj()
            .emit_by_name::<()>("key-event", &[&keyval, &keycode, &KeyEvent::RELEASE])
    }

    pub(crate) fn do_key_press_and_release(&self, keyval: gdk::Key, keycode: u32) {
        if self.read_only() {
            return;
        }
        self.obj().emit_by_name::<()>(
            "key-event",
            &[&keyval, &keycode, &(KeyEvent::PRESS | KeyEvent::RELEASE)],
        )
    }

    fn do_resize_request(&self, width: u32, height: u32, w_mm: u32, h_mm: u32) {
        if self.read_only() {
            return;
        }

        let req = Some((width, height, w_mm, h_mm));
        if req == self.last_resize_request.get() {
            return;
        }
        self.last_resize_request.set(req);

        self.obj()
            .emit_by_name::<()>("resize-request", &[&width, &height, &w_mm, &h_mm]);
    }

    #[cfg(windows)]
    fn unrealize_win32(&self) {}

    #[cfg(windows)]
    fn realize_win32(&self, dpy: &gdk_win32::Win32Display) {
        use windows::Win32::{
            Devices::HumanInterfaceDevice::{HID_USAGE_GENERIC_MOUSE, HID_USAGE_PAGE_GENERIC},
            UI::Input::{RegisterRawInputDevices, RAWINPUTDEVICE, RIDEV_INPUTSINK},
        };

        let Some(hwnd) = self.win32_handle() else {
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

        let filter = dpy.add_filter(
            clone!(@weak self as this => @default-panic, move |_, msg, _rv| {
                use windows::Win32::UI::Input::{
                    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTHEADER, RID_INPUT, RIM_TYPEMOUSE,
                };
                use windows::Win32::UI::WindowsAndMessaging::WM_INPUT;

                if !this.grabbed.get().contains(Grab::MOUSE) || msg.message != WM_INPUT {
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
                        let scale = this.obj().scale_factor() as f64;
                        let (dx, dy) = (dx as f64 / scale, dy as f64 / scale);
                        this.do_motion_relative(dx, dy);
                    }
                }

                gdk_win32::Win32MessageFilterReturn::Continue
            }),
        );

        self.win_filter.set(Some(filter));

        #[cfg(windows)]
        if let Err(e) = self.realize_gl_win32() {
            log::warn!("{}", e);
        }
    }

    #[cfg(windows)]
    unsafe fn realize_gl_win32(&self) -> Result<(), String> {
        let dpy = self.egl_display().ok_or("No EGL display".to_string())?;
        let query_display =
            egl::query_display_attrib().ok_or("No eglQueryDisplayAttrib".to_string())?;
        let query_device =
            egl::query_device_attrib().ok_or("No eglQueryDeviceAttrib".to_string())?;
        let mut device: egl::EGLDevice = std::ptr::null_mut();

        if query_display(
            dpy.as_ptr(),
            egl::DEVICE_EXT,
            &mut device as *mut _ as *mut _,
        ) == 0
        {
            return Err("Failed to query EGL display device".into());
        }

        let mut d3d11_device: *mut ID3D11Device1 = std::ptr::null_mut();
        if query_device(
            device,
            egl::D3D11_DEVICE_ANGLE,
            &mut d3d11_device as *mut _ as *mut _,
        ) == 0
        {
            return Err("Failed to query EGL D3D11 device".into());
        }

        // there should be a better way, to get a &ID3D instead
        let d3d11_device = std::ptr::NonNull::new_unchecked(d3d11_device);
        let d3d11_device: ID3D11Device1 = std::mem::transmute(d3d11_device);
        self.d3d11_device.set(d3d11_device.clone()).unwrap();
        std::mem::forget(d3d11_device);

        Ok(())
    }

    fn ungrab_keyboard(&self) {
        if !self.grabbed.get().contains(Grab::KEYBOARD) {
            return;
        }

        if let Some(toplevel) = self.toplevel() {
            toplevel.restore_system_shortcuts();
            #[cfg(windows)]
            if let Some(h) = self.win_hook.take() {
                let _ = win32::unhook(h);
            }
            self.grabbed.set(self.grabbed.get() - Grab::KEYBOARD);
            self.obj().notify("grabbed");
        }
    }

    pub(crate) fn ungrab_mouse(&self) {
        if self.grabbed.get().contains(Grab::MOUSE) {
            #[cfg(unix)]
            self.wayland.ungrab_mouse();

            #[cfg(windows)]
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::ClipCursor(None);
                if let Some(h) = self.win_mouse_hook.take() {
                    let _ = win32::unhook(h);
                }
            }
            self.restore_accel_mouse();

            self.grabbed.set(self.grabbed.get() - Grab::MOUSE);
            if !self.obj().mouse_absolute() {
                self.picture.set_cursor(None);
            }
            self.obj().queue_draw(); // update cursor
            self.obj().notify("grabbed");
        }
    }

    fn clear_last_key_press(&self) {
        self.last_key_press.set(None);
        if let Some(timeout_id) = self.last_key_press_timeout.take() {
            timeout_id.remove();
        }
    }

    fn release_keys(&self) {
        self.clear_last_key_press();
        for (keyval, keycode) in self.keys_pressed.take() {
            self.keys_pressed.borrow_mut().remove(&(keyval, keycode));
            self.do_key_release(keyval, keycode)
        }
        self.keys_pressed.borrow_mut().clear();
    }

    fn emit_last_key_press(&self) {
        if let Some((keyval, keycode)) = self.last_key_press.take() {
            self.keys_pressed.borrow_mut().insert((keyval, keycode));
            self.do_key_press(keyval, keycode)
        }

        self.clear_last_key_press();
    }

    fn ungrab(&self) {
        self.ungrab_keyboard();
        self.ungrab_mouse();
    }

    fn key_pressed(&self, ec: &gtk::EventControllerKey, keyval: gdk::Key, keycode: u32) {
        if let Some(ref e) = ec.current_event() {
            if self.grab_shortcut.get().unwrap().trigger(e, false) == gdk::KeyMatch::Exact {
                if self.grabbed.get().is_empty() {
                    self.try_grab();
                } else {
                    self.ungrab();
                }
            }
        }

        // flush pending key event
        self.emit_last_key_press();

        // synthesize press-and-release if within the synthesize-delay boundary, else emit
        self.last_key_press.set(Some((keyval, keycode)));
        self.last_key_press_timeout
                .set(Some(glib::timeout_add_local(
                    Duration::from_millis(self.synthesize_delay.get() as _),
                    glib::clone!(@weak self as this => @default-return glib::ControlFlow::Break, move || {
                        this.emit_last_key_press();
                        glib::ControlFlow::Break
                    }),
                )));
    }

    fn key_released(&self, keyval: gdk::Key, keycode: u32) {
        if let Some((last_keyval, last_keycode)) = self.last_key_press.get() {
            if (last_keyval, last_keycode) == (keyval, keycode) {
                self.clear_last_key_press();
                self.do_key_press_and_release(keyval, keycode);
            }
        }

        // flush pending key event
        self.emit_last_key_press();

        self.keys_pressed.borrow_mut().remove(&(keyval, keycode));
        self.do_key_release(keyval, keycode)
    }

    fn try_grab_keyboard(&self) -> bool {
        if self.grabbed.get().contains(Grab::KEYBOARD) {
            return false;
        }

        let Some(toplevel) = self.toplevel() else {
                return false;
            };

        toplevel.inhibit_system_shortcuts(None::<&gdk::ButtonEvent>);
        // Apparently, inhibit-system is not implemented on win32 yet
        #[cfg(windows)]
        match win32::hook_keyboard() {
            Ok(h) => self.win_hook.set(Some(h)),
            Err(e) => log::warn!("Failed to set keyboard hook: {}", e),
        }

        let id = toplevel.connect_shortcuts_inhibited_notify(
            clone!(@weak self as this => @default-panic, move |toplevel| {
                let inhibited = toplevel.is_shortcuts_inhibited();
                log::debug!("shortcuts-inhibited: {}", inhibited);
                if !inhibited {
                    let id = this.shortcuts_inhibited_id.take();
                    toplevel.disconnect(id.unwrap());
                    this.ungrab_keyboard();
                }
            }),
        );
        self.shortcuts_inhibited_id.set(Some(id));
        true
    }

    #[cfg(unix)]
    fn try_grab_device(&self, device: gdk::Device) -> bool {
        self.wayland.try_grab_device(&self.obj(), device)
    }

    #[cfg(windows)]
    fn try_grab_device(&self, _device: gdk::Device) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetWindowRect};

        let h = match self.win32_handle() {
            Some(h) => h,
            None => return false,
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
            Ok(h) => self.win_mouse_hook.set(Some(h)),
            Err(e) => log::warn!("Failed to set mouse hook: {}", e),
        }

        true
    }

    #[cfg(not(any(unix, windows)))]
    fn try_grab_device(&self, _device: gdk::Device) -> bool {
        false
    }

    fn try_grab_mouse(&self) -> bool {
        if self.obj().mouse_absolute() {
            // we could eventually grab the mouse in client mode, but what's the point?
            return false;
        }
        if self.obj().grabbed().contains(Grab::MOUSE) {
            return false;
        }

        if let Some(default_seat) = gdk::traits::DisplayExt::default_seat(&self.obj().display()) {
            for device in default_seat.devices(gdk::SeatCapabilities::POINTER) {
                if !self.try_grab_device(device) {
                    return false;
                }
            }
        }

        self.save_accel_mouse();

        true
    }

    fn save_accel_mouse(&self) {
        #[cfg(windows)]
        {
            match win32::spi_get_mouse() {
                Ok(mouse) => self.win_mouse.set(mouse),
                Err(e) => log::warn!("Failed to spi_get_mouse: {e}"),
            }
            match win32::spi_get_mouse_speed() {
                Ok(speed) => self.win_mouse_speed.set(speed),
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
        #[cfg(not(windows))]
        {
            // todo
        }
    }

    fn restore_accel_mouse(&self) {
        #[cfg(windows)]
        {
            if let Err(e) = win32::spi_set_mouse(self.win_mouse.get()) {
                log::warn!("Failed to spi_set_mouse: {e}");
            }
            if let Err(e) = win32::spi_set_mouse_speed(self.win_mouse_speed.get()) {
                log::warn!("Failed to spi_set_mouse_speed: {e}");
            }
        }
        #[cfg(not(windows))]
        {
            // todo
        }
    }

    fn try_grab(&self) -> Grab {
        let mut grabbed = Default::default();
        self.picture.grab_focus();
        if self.try_grab_keyboard() {
            grabbed |= Grab::KEYBOARD;
        }
        if self.try_grab_mouse() {
            grabbed |= Grab::MOUSE;
        }
        self.grabbed.set(self.obj().grabbed() | grabbed);
        self.obj().notify("grabbed");
        if grabbed.contains(Grab::MOUSE) {
            self.update_cursor();
        }
        grabbed
    }

    // remote display pos -> widget pos
    fn transform_pos_inv(&self, pos: (i32, i32)) -> (f64, f64) {
        let (px, py, pw, ph) = self.paintable_area();
        let (w, h) = self.picture.paintable().size();
        let x = pos.0 as f64 * (pw as f64 / w as f64) + px as f64;
        let y = pos.1 as f64 * (ph as f64 / h as f64) + py as f64;
        (x, y)
    }

    fn toplevel(&self) -> Option<gdk::Toplevel> {
        let obj = self.obj();
        obj.root()
            .and_then(|r| r.native())
            .map(|n| n.surface())
            .and_then(|s| s.downcast::<gdk::Toplevel>().ok())
    }

    fn surface(&self) -> Option<gdk::Surface> {
        let obj = self.obj();
        obj.native().map(|n| n.surface())
    }

    #[cfg(windows)]
    fn win32_handle(&self) -> Option<gdk_win32::HWND> {
        self.surface()
            .and_then(|s| s.downcast::<gdk_win32::Win32Surface>().ok())
            .map(|w| w.handle())
    }

    #[cfg(unix)]
    fn wl_surface(&self) -> Option<gdk_wl::wayland_client::protocol::wl_surface::WlSurface> {
        self.surface()
            .and_then(|s| s.downcast::<gdk_wl::WaylandSurface>().ok())
            .map(|w| w.wl_surface().unwrap())
    }

    #[cfg(windows)]
    pub(crate) fn egl_display(&self) -> Option<egl::Display> {
        let widget = self.obj();

        #[cfg(unix)]
        if let Ok(dpy) = widget.display().downcast::<gdk_wl::WaylandDisplay>() {
            return dpy.egl_display();
        }

        #[cfg(unix)]
        if let Ok(dpy) = widget.display().downcast::<gdk_x11::X11Display>() {
            return dpy.egl_display();
        };

        #[cfg(windows)]
        if let Ok(dpy) = widget.display().downcast::<gdk_win32::Win32Display>() {
            return dpy.egl_display();
        };

        None
    }

    #[cfg(windows)]
    pub(crate) fn d3d11_texture2d_acquire0(&self) -> Result<Option<D3d11TexGuard>, String> {
        use windows::{core::Interface, Win32::System::WindowsProgramming::INFINITE};

        if !self.d3d11_texture_can_acquire.get() {
            log::debug!("can't acquire texture2d");
            return Ok(None);
        }
        let Some(tex) = &*self.d3d11_texture.borrow() else {
                log::debug!("no texture2d");
                return Ok(None);
            };

        log::trace!("acquire d3d texture, begin");
        let mutex: IDXGIKeyedMutex = tex
            .cast()
            .map_err(|e| format!("Failed to cast to Mutex: {}", e))?;
        unsafe {
            mutex
                .AcquireSync(0, INFINITE)
                .map_err(|e| format!("Failed to acquire Mutex: {}", e))?
        }
        log::trace!("acquire d3d texture, end");

        Ok(Some(D3d11TexGuard(mutex)))
    }

    #[cfg(windows)]
    pub(crate) fn set_d3d11_texture2d_can_acquire(&self, can_acquire: bool) {
        self.d3d11_texture_can_acquire.set(can_acquire);
    }

    #[cfg(windows)]
    pub(crate) fn set_d3d11_texture2d_scanout(
        &self,
        s: Option<RdwD3d11Texture2dScanout>,
    ) -> Result<(), String> {
        use windows::Win32::Foundation::HANDLE;

        let Some(s) = s else {
                self.d3d11_scanout.replace(None);
                self.d3d11_texture.replace(None);
                return Ok(());
            };

        let d3d11_device = self
            .d3d11_device
            .get()
            .ok_or("No d3d11 device initialized")?;
        let egl_image_target =
            egl::image_target_texture_2d_oes().ok_or("ImageTargetTexture2DOES support missing")?;

        let egl_dpy = self
            .egl_display()
            .ok_or("Unsupported display kind (or not egl)")?;
        let egl = egl::egl();

        let d3d11_tex: ID3D11Texture2D = unsafe {
            d3d11_device
                .OpenSharedResource1(HANDLE(s.handle as _))
                .map_err(|e| format!("Failed to open shared texture: {}", e))?
        };

        let tex: std::ptr::NonNull<std::ffi::c_void> =
            unsafe { std::mem::transmute_copy(&d3d11_tex) };
        let img = egl
            .create_image(
                egl_dpy,
                egl::no_context(),
                egl::D3D11_TEXTURE_ANGLE,
                unsafe { egl::ClientBuffer::from_ptr(tex.as_ptr()) },
                &[egl::NONE as _],
            )
            .map_err(|e| format!("eglCreateImage() failed: {}", e))?;

        unsafe { gl::BindTexture(gl::TEXTURE_2D, self.texture_id()) };
        egl_image_target(gl::TEXTURE_2D, img.as_ptr() as gl::types::GLeglImageOES);

        egl.destroy_image(egl_dpy, img)
            .map_err(|e| format!("eglDestroyImage() failed: {}", e))?;

        self.d3d11_scanout.replace(Some(s));
        self.d3d11_texture.replace(Some(d3d11_tex));
        Ok(())
    }
}
