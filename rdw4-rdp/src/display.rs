use glib::{clone, translate::*, SignalHandlerId};
use gtk::{glib, prelude::*};

use rdw::{gtk, DisplayExt};

#[repr(C)]
pub struct RdwRdpDisplay {
    parent: rdw::RdwDisplay,
}

#[repr(C)]
pub struct RdwRdpDisplayClass {
    pub parent_class: rdw::RdwDisplayClass,
}

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use gtk::subclass::prelude::*;
    use rdw::gtk::{gdk, glib::MainContext};
    use std::{cell::Cell, sync::OnceLock};

    unsafe impl ClassStruct for RdwRdpDisplayClass {
        type Type = Display;
    }

    impl std::fmt::Debug for RdwRdpDisplay {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_struct("RdwRdpDisplay")
                .field("parent", &self.parent)
                .finish()
        }
    }

    unsafe impl InstanceStruct for RdwRdpDisplay {
        type Type = Display;
    }

    #[derive(Debug, glib::Properties)]
    #[properties(wrapper_type = super::Display)]
    pub struct Display {
        last_mouse: Cell<(f64, f64)>,
        keymap: Cell<Option<&'static [u16]>>,
        #[property(
            get,
            name = "rdp-connected",
            nick = "RDP connected",
            blurb = "Whether the RDP connection is up and running"
        )]
        connected: Cell<bool>,
    }

    impl Default for Display {
        fn default() -> Self {
            Self {
                last_mouse: Cell::new((0.0, 0.0)),
                keymap: Default::default(),
                connected: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Display {
        const NAME: &'static str = "RdwRdpDisplay";
        type Type = super::Display;
        type ParentType = rdw::Display;
        type Class = RdwRdpDisplayClass;
        type Instance = RdwRdpDisplay;
    }

    impl ObjectImpl for Display {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }

        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS
                .get_or_init(|| {
                    vec![Signal::builder("rdp-authenticate")
                        .return_type_from(<bool>::static_type())
                        .build()]
                })
                .as_ref()
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.obj().set_mouse_absolute(true);

            self.obj().connect_key_event(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, keyval, keycode, event| {
                    log::debug!("key-event: {:?}", (keyval, keycode, event));
                    if keyval == gdk::Key::Pause.into_glib() {
                        unimplemented!()
                    }
                    if let Some(&xt) = this.keymap.get().and_then(|m| m.get(keycode as usize)) {}
                }
            ));

            self.obj().connect_motion(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, x, y| {
                    log::debug!("motion: {:?}", (x, y));
                    MainContext::default().spawn_local(glib::clone!(
                        #[weak]
                        this,
                        async move {
                            this.last_mouse.set((x, y));
                        }
                    ));
                }
            ));

            self.obj().connect_motion_relative(clone!(
                #[weak(rename_to = _this)]
                self,
                move |_, dx, dy| {
                    log::debug!("motion-relative: {:?}", (dx, dy));
                }
            ));

            self.obj().connect_mouse_press(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, button| {
                    log::debug!("mouse-press: {:?}", button);
                    MainContext::default().spawn_local(glib::clone!(
                        #[weak]
                        this,
                        async move { todo!() }
                    ));
                }
            ));

            self.obj().connect_mouse_release(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, button| {
                    log::debug!("mouse-release: {:?}", button);
                    MainContext::default().spawn_local(glib::clone!(
                        #[weak]
                        this,
                        async move { todo!() }
                    ));
                }
            ));

            self.obj().connect_resize_request(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, width, height, wmm, hmm| {
                    let scale_factor = this.obj().scale_factor() * 100;
                    log::debug!(
                        "resize-request: {:?}",
                        (width, height, wmm, hmm, scale_factor)
                    );
                    MainContext::default().spawn_local(glib::clone!(
                        #[weak]
                        this,
                        async move { todo!() }
                    ));
                }
            ));

            let ec = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
            ec.connect_scroll(clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or_panic]
                move |_, dx, dy| {
                    MainContext::default().spawn_local(glib::clone!(
                        #[weak]
                        this,
                        async move { todo!() }
                    ));
                    glib::Propagation::Proceed
                }
            ));
            self.obj().add_controller(ec);

            let cb = gdk::prelude::DisplayExt::clipboard(&self.obj().display());
            cb.connect_changed(clone!(
                #[weak(rename_to = this)]
                self,
                move |clipboard| {
                    let is_local = clipboard.is_local();
                    if let (false, formats) = (is_local, clipboard.formats()) {
                        todo!()
                    }
                }
            ));
        }
    }

    impl WidgetImpl for Display {
        fn realize(&self) {
            self.parent_realize();

            self.keymap.set(rdw::keymap_xtkbd());
        }
    }

    impl rdw::DisplayImpl for Display {}

    impl Display {}
}

glib::wrapper! {
    pub struct Display(ObjectSubclass<imp::Display>) @extends rdw::Display, gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Display {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn connect_rdp_authenticate<F: Fn(&Self) -> bool + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        unsafe extern "C" fn connect_trampoline<P, F: Fn(&P) -> bool + 'static>(
            this: *mut RdwRdpDisplay,
            f: glib::ffi::gpointer,
        ) -> bool
        where
            P: IsA<Display>,
        {
            let f = &*(f as *const F);
            f(Display::from_glib_borrow(this).unsafe_cast_ref::<P>())
        }
        unsafe {
            let f: Box<F> = Box::new(f);
            glib::signal::connect_raw(
                self.as_ptr() as *mut glib::gobject_ffi::GObject,
                b"rdp-authenticate\0".as_ptr() as *const _,
                Some(std::mem::transmute(connect_trampoline::<Self, F> as usize)),
                Box::into_raw(f),
            )
        }
    }

    pub async fn rdp_connect(&self) -> Result<(), String> {
        Ok(())
    }
    pub async fn rdp_disconnect(&self) -> Result<(), String> {
        Ok(())
    }
}

impl Default for Display {
    fn default() -> Self {
        Self::new()
    }
}
