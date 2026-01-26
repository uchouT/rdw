use glib::{clone, translate::*, SignalHandlerId};
use gtk::{glib, prelude::*};
use ironrdp::{
    connector,
    pdu::{
        gcc::KeyboardType,
        rdp::{
            capability_sets::MajorPlatformType,
            client_info::{PerformanceFlags, TimezoneInfo},
        },
    },
};

use rdw::{gtk, gtk::subclass::prelude::*, DisplayExt};

use crate::{Error, Result};

#[repr(C)]
pub struct RdwRdpDisplay {
    parent: rdw::RdwDisplay,
}

#[repr(C)]
pub struct RdwRdpDisplayClass {
    pub parent_class: rdw::RdwDisplayClass,
}

mod imp {
    use crate::{
        cliprdr::{
            rdp_clipboard_format, rdp_clipboard_formats, rdp_clipboard_mime,
            rdp_clipboard_mime_from_id, PasteProgress,
        },
        rdp::{
            rdp_key_event, rdp_motion, rdp_mouse_press, rdp_mouse_release, rdp_run, rdp_scroll,
            RdpInputEvent, RdpOutputEvent,
        },
        util::utf16_from_utf8,
    };

    use super::*;
    use glib::subclass::Signal;
    use gtk::{gdk::Clipboard, gio};
    use ironrdp::{
        cliprdr::{
            backend::ClipboardMessage,
            pdu::{ClipboardFormatId, OwnedFormatDataResponse},
        },
        connector::Config,
        graphics::image_processing::PixelFormat as IronRdpPixelFormat,
        pdu::geometry::Rectangle,
    };
    use rdw::{gtk::gdk, PixelFormat};
    use std::{
        cell::{Cell, RefCell},
        thread::JoinHandle,
    };
    use tokio::sync::{mpsc, oneshot};
    use tracing::{debug, instrument, trace, warn};

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
        cb_watch_id: RefCell<Option<SignalHandlerId>>,
        last_mouse: Cell<(f64, f64)>,
        keymap: Cell<Option<&'static [u16]>>,
        rdp_thread: RefCell<Option<JoinHandle<()>>>,
        input_event_tx: RefCell<Option<mpsc::UnboundedSender<RdpInputEvent>>>,
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
                cb_watch_id: RefCell::new(None),
                last_mouse: Cell::new((0.0, 0.0)),
                keymap: Default::default(),
                connected: Default::default(),
                rdp_thread: Default::default(),
                input_event_tx: Default::default(),
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
            static SIGNALS: std::sync::OnceLock<Vec<Signal>> = std::sync::OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![Signal::builder("verify-tls-certificate")
                    .param_types([
                        String::static_type(),
                        String::static_type(),
                        String::static_type(),
                    ])
                    .return_type::<bool>()
                    .build()]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.obj().set_mouse_absolute(true);

            self.obj().connect_key_event(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, keyval, keycode, event| {
                    trace!("key-event: {:?}", (keyval, keycode, event));
                    let Some(&xt) = this.keymap.get().and_then(|m| m.get(keycode as usize)) else {
                        return;
                    };
                    let fp = rdp_key_event(xt, event);
                    this.send_event(RdpInputEvent::FastPath(fp));
                }
            ));

            self.obj().connect_motion(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, x, y| {
                    trace!("motion: {:?}", (x, y));
                    let fp = rdp_motion(x as _, y as _);
                    this.send_event(RdpInputEvent::FastPath(fp));
                    this.last_mouse.set((x, y));
                }
            ));

            self.obj().connect_motion_relative(clone!(
                #[weak(rename_to = _this)]
                self,
                move |_, dx, dy| {
                    trace!("motion-relative: {:?}", (dx, dy));
                }
            ));

            self.obj().connect_mouse_press(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, button| {
                    trace!("mouse-press: {:?}", button);
                    let (x_position, y_position) = this.last_mouse.get();
                    let fp = rdp_mouse_press(x_position as _, y_position as _, button);
                    this.send_event(RdpInputEvent::FastPath(fp));
                }
            ));

            self.obj().connect_mouse_release(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, button| {
                    trace!("mouse-release: {:?}", button);
                    let (x_position, y_position) = this.last_mouse.get();
                    let fp = rdp_mouse_release(x_position as _, y_position as _, button);
                    this.send_event(RdpInputEvent::FastPath(fp));
                }
            ));

            self.obj().connect_resize_request(clone!(
                #[weak(rename_to = this)]
                self,
                move |_, width, height, wmm, hmm| {
                    let scale_factor = this.obj().scale_factor() * 100;
                    trace!(
                        "resize-request: {:?}",
                        (width, height, wmm, hmm, scale_factor)
                    );
                    let width = width.try_into().expect("Failed to convert width");
                    let height = height.try_into().expect("Failed to convert height");
                    let scale_factor = scale_factor
                        .try_into()
                        .expect("Failed to convert scale factor");
                    let physical_size = Some((wmm, hmm));
                    this.send_event(RdpInputEvent::Resize {
                        width,
                        height,
                        scale_factor,
                        physical_size,
                    });
                }
            ));

            let ec = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
            ec.connect_scroll(clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or_panic]
                move |_, dx, dy| {
                    trace!(?dx, ?dy, "scroll");
                    let (x_position, y_position) = this.last_mouse.get();
                    let fp = rdp_scroll(dx as _, dy as _, x_position as _, y_position as _);
                    this.send_event(RdpInputEvent::FastPath(fp));
                    glib::Propagation::Proceed
                }
            ));
            self.obj().add_controller(ec);

            let cb = gdk::prelude::DisplayExt::clipboard(&self.obj().display());
            let watch_id = cb.connect_changed(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.clipboard_send_initiate_copy(false);
                }
            ));
            self.cb_watch_id.replace(Some(watch_id));
        }

        fn dispose(&self) {
            if let Some(watch_id) = self.cb_watch_id.take() {
                let cb = gdk::prelude::DisplayExt::clipboard(&self.obj().display());
                cb.disconnect(watch_id);
            }
        }
    }

    impl WidgetImpl for Display {
        fn realize(&self) {
            self.parent_realize();

            self.keymap.set(rdw::keymap_xtkbd());
        }
    }

    impl rdw::DisplayImpl for Display {}

    impl Display {
        #[instrument]
        pub(crate) async fn connect(&self, domain: &str, port: u16, config: Config) -> Result<()> {
            if self.rdp_thread.borrow().is_some() {
                return Err(Error::AlreadyConnected);
            }

            let (input_event_tx, mut input_event_rx) = RdpInputEvent::create_channel();
            let (output_event_tx, mut output_event_rx) = RdpOutputEvent::create_channel();
            let rdp_cb_paste = PasteProgress::new();

            let domain = domain.to_owned();
            let cb_paste = rdp_cb_paste.clone();
            let t = std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                rt.block_on(async {
                    rdp_run(
                        &domain,
                        port,
                        config,
                        &mut input_event_rx,
                        &output_event_tx,
                        cb_paste,
                    )
                    .await;
                });
            });

            self.rdp_thread.borrow_mut().replace(t);
            self.input_event_tx.replace(Some(input_event_tx));
            let clipboard = gdk::prelude::DisplayExt::clipboard(&self.obj().display());

            let (tx, wait_connect_rx) = oneshot::channel();
            let mut tx = Some(tx);

            glib::spawn_future_local(clone!(
                #[weak(rename_to = this)]
                self,
                async move {
                    while let Some(event) = output_event_rx.recv().await {
                        match event {
                            RdpOutputEvent::TlsNeedsCertVerification {
                                subject,
                                issuer,
                                fingerprint,
                            } => {
                                let is_verified = this.obj().emit_by_name::<bool>(
                                    "verify-tls-certificate",
                                    &[&subject, &issuer, &fingerprint],
                                );
                                this.send_event(RdpInputEvent::VerifyTlsCert { is_verified });
                            }
                            RdpOutputEvent::Connected => {
                                if let Some(tx) = tx.take() {
                                    tx.send(Ok(())).unwrap();
                                }
                                this.set_connected(true);
                            }
                            RdpOutputEvent::Terminated(result) => {
                                if let Err(err) = result {
                                    if let Some(tx) = tx.take() {
                                        tx.send(Err(err)).unwrap();
                                    }
                                }
                                this.set_connected(false);
                            }
                            RdpOutputEvent::GraphicsUpdate { image, region } => {
                                trace!("Graphics update received.");
                                let image = image.lock().unwrap();
                                trace!("Pixel format: {:?}", image.pixel_format());
                                let paintable_format = match image.pixel_format() {
                                    // I'm assuming the A and X variants should be treatable the same,
                                    // not sure though.
                                    IronRdpPixelFormat::BgrA32 | IronRdpPixelFormat::BgrX32 => {
                                        PixelFormat::Bgra
                                    }
                                    IronRdpPixelFormat::RgbA32 | IronRdpPixelFormat::RgbX32 => {
                                        PixelFormat::Rgba
                                    }
                                    format => {
                                        warn!("Unsupported RDP pixel format: {format:?}. Trying BGRA anyway.");
                                        PixelFormat::Bgra
                                    }
                                };
                                this.obj().set_display_size(Some((
                                    usize::from(image.width()),
                                    usize::from(image.height()),
                                )));
                                this.obj().set_display_pixel_format(paintable_format);
                                let data = image.data_for_rect(&region);
                                let stride = image.stride() as i32;
                                let (x, y, w, h) =
                                    (region.left, region.top, region.width(), region.height());
                                this.obj().update_area(
                                    x.into(),
                                    y.into(),
                                    w.into(),
                                    h.into(),
                                    stride,
                                    Some(data),
                                );
                            }
                            RdpOutputEvent::PointerBitmap(pointer) => {
                                let cursor = rdw::Display::make_cursor(
                                    &pointer.bitmap_data,
                                    pointer.width.into(),
                                    pointer.height.into(),
                                    pointer.hotspot_x.into(),
                                    pointer.hotspot_y.into(),
                                    1,
                                );
                                this.obj().define_cursor(Some(cursor));
                            }
                            RdpOutputEvent::PointerDefault => {
                                this.obj().define_cursor(None);
                            }
                            RdpOutputEvent::PointerHidden => {
                                let cursor = gdk::Cursor::from_name("none", None);
                                this.obj().define_cursor(cursor);
                            }
                            RdpOutputEvent::PointerPosition { x, y } => {
                                this.obj().set_cursor_position(Some((x.into(), y.into())));
                            }
                            RdpOutputEvent::ClipboardRequestFormatList => {
                                this.clipboard_send_initiate_copy(true);
                            }
                            RdpOutputEvent::Clipboard(message) => {
                                this.rdp_clipboard(&clipboard, message, rdp_cb_paste.clone());
                            }
                        }
                    }
                }
            ));

            wait_connect_rx.await.unwrap()
        }

        fn clipboard_send_initiate_copy(&self, request: bool) {
            let cb = gdk::prelude::DisplayExt::clipboard(&self.obj().display());
            if cb.is_local() && !request {
                trace!("not sending clipboard copy, since it's local");
                return;
            }

            let formats = if cb.is_local() {
                Vec::new()
            } else {
                rdp_clipboard_formats(cb.formats())
            };

            let event = if request {
                RdpInputEvent::ClipboardResponseInitialCopy(formats)
            } else {
                let msg = ClipboardMessage::SendInitiateCopy(formats);
                RdpInputEvent::Clipboard(msg)
            };
            self.send_event(event)
        }

        fn rdp_clipboard(
            &self,
            clipboard: &Clipboard,
            message: ClipboardMessage,
            rdp_cb_paste: PasteProgress,
        ) {
            if self.obj().read_only() {
                trace!("dropping cb event, since we are read only");
                return;
            }

            match message {
                ClipboardMessage::SendInitiateCopy(formats) => {
                    if rdp_cb_paste.is_some() {
                        warn!("Clipboard paste already in progress");
                        return;
                    }
                    let formats: Vec<_> = formats
                        .iter()
                        .filter_map(|f| rdp_clipboard_mime(f))
                        .collect();
                    let provider = rdw::ContentProvider::new(
                        &formats,
                        clone!(
                            #[weak(rename_to = this)]
                            self,
                            #[upgrade_or]
                            None,
                            move |mime, stream, prio| {
                                debug!("content-provider-write: {:?}", (mime, stream));
                                let format = rdp_clipboard_format(mime);
                                Some(Box::pin(clone!(
                                    #[weak]
                                    this,
                                    #[strong]
                                    stream,
                                    #[strong]
                                    rdp_cb_paste,
                                    #[upgrade_or_panic]
                                    async move {
                                        let (tx, rx) = oneshot::channel();
                                        rdp_cb_paste.set(format.id(), tx);
                                        this.send_event(RdpInputEvent::Clipboard(
                                            ClipboardMessage::SendInitiatePaste(format.id()),
                                        ));
                                        if let Ok(bytes) = rx.await {
                                            let bytes = glib::Bytes::from_owned(bytes);
                                            return stream
                                                .write_bytes_future(&bytes, prio)
                                                .await
                                                .map(|_| ());
                                        }

                                        Err(glib::Error::new(
                                            gio::IOErrorEnum::Failed,
                                            "failed to request clipboard data",
                                        ))
                                    }
                                )))
                            }
                        ),
                    );
                    let res = clipboard.set_content(Some(&provider));
                    trace!(?res);
                }
                ClipboardMessage::SendInitiatePaste(format_id) => {
                    glib::MainContext::default().spawn_local(glib::clone!(
                        #[weak(rename_to = this)]
                        self,
                        async move {
                            let mut data = None;

                            if let Some(mime) = rdp_clipboard_mime_from_id(format_id) {
                                let cb = gdk::prelude::DisplayExt::clipboard(&this.obj().display());
                                let res = cb.read_future(&[mime], glib::Priority::default()).await;
                                debug!(?res, "clipboard-read");
                                if let Ok((stream, _)) = res {
                                    let out = gio::MemoryOutputStream::new_resizable();
                                    let res = out
                                        .splice_future(
                                            &stream,
                                            gio::OutputStreamSpliceFlags::CLOSE_SOURCE
                                                | gio::OutputStreamSpliceFlags::CLOSE_TARGET,
                                            glib::Priority::default(),
                                        )
                                        .await;
                                    if res.is_ok() {
                                        let bytes = out.steal_as_bytes();
                                        if format_id == ClipboardFormatId::CF_UNICODETEXT {
                                            data = utf16_from_utf8(bytes.as_ref()).ok();
                                        } else {
                                            data = Some(bytes.to_vec());
                                        }
                                    }
                                }
                            }

                            let resp = if let Some(data) = data {
                                OwnedFormatDataResponse::new_data(data)
                            } else {
                                OwnedFormatDataResponse::new_error()
                            };
                            this.send_event(RdpInputEvent::Clipboard(
                                ClipboardMessage::SendFormatData(resp),
                            ));
                        }
                    ));
                }
                ClipboardMessage::Error(clipboard_error) => {
                    warn!("Clipboard error: {}", clipboard_error);
                }
                ClipboardMessage::SendFormatData(_) => unreachable!(), // handled directly in cliprdr.rs
            }
        }

        fn send_event(&self, event: RdpInputEvent) {
            let tx = self.input_event_tx.borrow();
            let Some(ref tx) = *tx else {
                return;
            };
            let _ = tx.send(event);
        }

        fn set_connected(&self, connected: bool) {
            if self.connected.replace(connected) != connected {
                self.obj().notify("rdp-connected");
            }
        }

        #[instrument]
        pub(crate) async fn disconnect(&self) -> Result<()> {
            let Some(tx) = self.input_event_tx.take() else {
                trace!("disconnect: not connected");
                return Ok(());
            };
            tx.send(RdpInputEvent::Close).unwrap();
            if let Some(rdp_thread) = self.rdp_thread.take() {
                let _ = rdp_thread.join();
            }
            Ok(())
        }
    }
}

glib::wrapper! {
    pub struct Display(ObjectSubclass<imp::Display>) @extends rdw::Display, gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Display {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub async fn rdp_connect(
        &self,
        domain: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let config = connector::Config {
            credentials: connector::Credentials::UsernamePassword {
                username: username.to_owned(),
                password: password.to_owned(),
            },
            domain: None,
            enable_tls: true,
            enable_credssp: true,
            keyboard_type: KeyboardType::IbmEnhanced,
            keyboard_subtype: 0,
            keyboard_layout: 0, // the server SHOULD use the default active input locale identifier
            keyboard_functional_keys_count: 12,
            ime_file_name: "".to_owned(),
            dig_product_id: "".to_owned(),
            desktop_size: connector::DesktopSize {
                width: 1920,
                height: 1080,
            },
            desktop_scale_factor: 0, // Default to 0 per FreeRDP
            bitmap: None,
            client_build: 0,
            client_name: whoami::hostname().unwrap_or_else(|_| "ironrdp".to_owned()),
            // NOTE: hardcode this value like in freerdp
            // https://github.com/FreeRDP/FreeRDP/blob/4e24b966c86fdf494a782f0dfcfc43a057a2ea60/libfreerdp/core/settings.c#LL49C34-L49C70
            client_dir: "C:\\Windows\\System32\\mstscax.dll".to_owned(),
            platform: match whoami::platform() {
                whoami::Platform::Windows => MajorPlatformType::WINDOWS,
                whoami::Platform::Linux => MajorPlatformType::UNIX,
                whoami::Platform::Mac => MajorPlatformType::MACINTOSH,
                whoami::Platform::Ios => MajorPlatformType::IOS,
                whoami::Platform::Android => MajorPlatformType::ANDROID,
                _ => MajorPlatformType::UNSPECIFIED,
            },
            hardware_id: None,
            license_cache: None,
            enable_server_pointer: true,
            autologon: true,
            enable_audio_playback: true,
            request_data: None,
            pointer_software_rendering: false,
            performance_flags: PerformanceFlags::default(),
            timezone_info: TimezoneInfo::default(),
        };

        self.imp().connect(domain, port, config).await
    }

    pub async fn rdp_disconnect(&self) -> Result<()> {
        self.imp().disconnect().await
    }

    pub fn connect_verify_tls_certificate<
        F: Fn(&Self, String, String, String) -> bool + 'static,
    >(
        &self,
        f: F,
    ) -> SignalHandlerId {
        use std::{ffi::CStr, os::raw::c_char};

        unsafe extern "C" fn connect_trampoline<
            P,
            F: Fn(&P, String, String, String) -> bool + 'static,
        >(
            this: *mut RdwRdpDisplay,
            subject: *const c_char,
            issuer: *const c_char,
            fingerprint: *const c_char,
            f: glib::ffi::gpointer,
        ) -> bool
        where
            P: IsA<Display>,
        {
            let f = &*(f as *const F);
            f(
                Display::from_glib_borrow(this).unsafe_cast_ref::<P>(),
                CStr::from_ptr(subject).to_str().unwrap().to_owned(),
                CStr::from_ptr(issuer).to_str().unwrap().to_owned(),
                CStr::from_ptr(fingerprint).to_str().unwrap().to_owned(),
            )
        }
        unsafe {
            let f: Box<F> = Box::new(f);
            glib::signal::connect_raw(
                self.as_ptr() as *mut glib::gobject_ffi::GObject,
                c"verify-tls-certificate".as_ptr() as *const _,
                Some(std::mem::transmute::<*const (), unsafe extern "C" fn()>(
                    connect_trampoline::<Self, F> as *const (),
                )),
                Box::into_raw(f),
            )
        }
    }
}

impl Default for Display {
    fn default() -> Self {
        Self::new()
    }
}
