use super::*;
use gtk::graphene;
use std::cell::{Cell, OnceCell, RefCell};

#[cfg(windows)]
mod win32;

#[derive(Debug, Default)]
pub struct Paintable {
    ctxt: OnceCell<gdk::GLContext>,
    texture: RefCell<Option<gdk::Texture>>,
    texture_id: Cell<Option<gl::types::GLuint>>,
    pixel_format: Cell<PixelFormat>,
    use_rgb_fallback: Cell<bool>,
    y0_top: Cell<Option<bool>>,

    #[cfg(windows)]
    pub(crate) win32: win32::Helper,
}

/// cbindgen:ignore
#[glib::object_subclass]
impl ObjectSubclass for Paintable {
    const NAME: &'static str = "RdwPaintable";
    type Type = super::Paintable;
    type ParentType = glib::Object;
    type Interfaces = (gdk::Paintable,);

    fn class_init(_klass: &mut Self::Class) {}
}

impl ObjectImpl for Paintable {
    fn constructed(&self) {}

    fn dispose(&self) {
        if let Some(tex_id) = self.texture_id.take() {
            unsafe {
                gl::DeleteTextures(1, &tex_id);
            }
        }
    }
}

impl PaintableImpl for Paintable {
    fn intrinsic_width(&self) -> i32 {
        self.size().0
    }

    fn intrinsic_height(&self) -> i32 {
        self.size().1
    }

    fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
        if let Some(texture) = self.texture.borrow().as_ref() {
            let flip = self.y0_top.get().unwrap_or_default();
            if flip {
                snapshot.save();
                snapshot.translate(&graphene::Point::new(0.0, height as _));
                snapshot.scale(1.0, -1.0);
                snapshot.append_texture(
                    texture,
                    &graphene::Rect::new(0.0, 0.0, width as _, height as _),
                );
                snapshot.restore();
            } else {
                snapshot.append_texture(
                    texture,
                    &graphene::Rect::new(0.0, 0.0, width as _, height as _),
                );
            }
        }
    }
}

impl Paintable {
    pub(crate) fn pixel_format(&self) -> PixelFormat {
        self.pixel_format.get()
    }

    pub(crate) fn size(&self) -> (i32, i32) {
        self.texture
            .borrow()
            .as_ref()
            .map_or((0, 0), |t| (t.width(), t.height()))
    }

    fn texture_id(&self) -> Result<gl::types::GLuint, glib::error::Error> {
        match self.texture_id.get() {
            None => {
                let mut tex_id = 0;
                let ctxt = self.gl_context()?;
                ctxt.make_current();
                unsafe {
                    gl::GenTextures(1, &mut tex_id);
                    gl::BindTexture(gl::TEXTURE_2D, tex_id);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                    assert_eq!(gl::GetError(), gl::NO_ERROR);
                    self.texture_id.set(Some(tex_id));
                }
                Ok(tex_id)
            }
            Some(id) => Ok(id),
        }
    }

    fn gl_context(&self) -> Result<&gdk::GLContext, glib::error::Error> {
        let ctxt = if let Some(ctxt) = self.ctxt.get() {
            ctxt
        } else {
            let ctxt = gdk::Display::default().unwrap().create_gl_context()?;
            self.ctxt.set(ctxt).unwrap();
            self.ctxt.get().unwrap()
        };
        Ok(ctxt)
    }

    fn update_texture(
        &self,
        size: Option<(i32, i32)>,
        region: Option<&cairo::Region>,
    ) -> Result<(), glib::error::Error> {
        let ctxt = self.gl_context()?;
        ctxt.make_current();

        let (w, h) = size.unwrap_or_else(|| self.size());
        let texture = unsafe {
            let sync = gl::FenceSync(gl::SYNC_GPU_COMMANDS_COMPLETE, 0);
            let builder = gdk::GLTexture::builder()
                .set_context(Some(ctxt))
                .set_id(self.texture_id()?)
                .set_width(w)
                .set_height(h)
                .set_format(gdk::MemoryFormat::R8g8b8a8)
                .set_update_region(region)
                .set_update_texture(self.texture.take().as_ref());
            let builder = if sync.is_null() {
                builder
            } else {
                builder.set_sync(Some(sync as _))
            };
            builder.build()
        };
        self.texture.replace(Some(texture));
        if region.is_some() {
            self.obj().invalidate_contents();
        }

        Ok(())
    }

    fn recreate_texture(
        &self,
        size: (i32, i32),
        format: PixelFormat,
    ) -> Result<(), glib::error::Error> {
        let (w, h) = size;
        let ctxt = self.gl_context()?;
        ctxt.make_current();

        unsafe {
            assert_eq!(gl::GetError(), gl::NO_ERROR);
            self.pixel_format.set(format);
            self.use_rgb_fallback.set(false);
            gl::BindTexture(gl::TEXTURE_2D, self.texture_id()?);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as _,
                w as _,
                h as _,
                0,
                format.as_opengl(),
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            // Fallback for failing BGRA rendering (try to use RGB instead)
            if gl::GetError() != gl::NO_ERROR {
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGB as _,
                    w as _,
                    h as _,
                    0,
                    gl::RGB,
                    gl::UNSIGNED_BYTE,
                    std::ptr::null(),
                );
                self.use_rgb_fallback.set(true);
            }
            assert_eq!(gl::GetError(), gl::NO_ERROR);
        }

        self.update_texture(Some((w as _, h as _)), None)?;
        self.obj().invalidate_size();
        Ok(())
    }

    pub(crate) fn set_pixel_format(&self, format: PixelFormat) -> Result<(), glib::error::Error> {
        if self.pixel_format() == format {
            return Ok(());
        }
        self.recreate_texture(self.size(), format)
    }

    pub(crate) fn set_size(&self, w: usize, h: usize) -> Result<(), glib::error::Error> {
        if self.size() == (w as _, h as _) {
            return Ok(());
        }
        self.recreate_texture((w as _, h as _), self.pixel_format.get())
    }

    pub(crate) fn update_area(
        &self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        stride: i32,
        data: Option<&[u8]>,
    ) -> Result<(), glib::error::Error> {
        let ctxt = self.gl_context()?;
        ctxt.make_current();
        unsafe { gl::GetError() };

        let (max_w, max_h) = self.size();
        let x = x.clamp(0, max_w);
        let y = y.clamp(0, max_h);
        let w = w.clamp(0, max_w - x);
        let h = h.clamp(0, max_h - y);

        // TODO: check data boundaries
        if let Some(data) = data {
            #[cfg(windows)]
            unsafe {
                self.win32.import_d3d11_texture2d_scanout(self, None)?
            };

            unsafe {
                gl::BindTexture(gl::TEXTURE_2D, self.texture_id()?);
                gl::PixelStorei(gl::UNPACK_ROW_LENGTH, stride / 4);
                if self.use_rgb_fallback.get() {
                    // RGB rendering fallback
                    let mut rgb = Vec::with_capacity(data.len());
                    let (ridx, gidx, bidx) = match self.pixel_format.get() {
                        PixelFormat::Rgba => (0, 1, 2),
                        PixelFormat::Bgra | _ => (2, 1, 0),
                    };
                    for pix in data.chunks(4) {
                        rgb.push(pix[ridx]);
                        rgb.push(pix[gidx]);
                        rgb.push(pix[bidx]);
                    }
                    gl::TexSubImage2D(
                        gl::TEXTURE_2D,
                        0,
                        x,
                        y,
                        w,
                        h,
                        gl::RGB,
                        gl::UNSIGNED_BYTE,
                        rgb.as_ptr() as _,
                    );
                } else {
                    gl::TexSubImage2D(
                        gl::TEXTURE_2D,
                        0,
                        x,
                        y,
                        w,
                        h,
                        self.pixel_format.get().as_opengl(),
                        gl::UNSIGNED_BYTE,
                        data.as_ptr() as _,
                    );
                }
                assert_eq!(gl::GetError(), gl::NO_ERROR);
            }
        } else {
            #[cfg(windows)]
            if self.win32.has_texture() {
                self.win32.update_texture(self, x, y, w, h)?;
            }
        }

        let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(x, y, w, h));
        self.update_texture(None, Some(&region))?;
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) unsafe fn import_dmabuf(
        &self,
        s: &crate::RdwDmabufScanout,
    ) -> Result<(), glib::error::Error> {
        use crate::egl;

        let ctxt = self.gl_context()?;
        ctxt.make_current();
        let egl_dpy = egl::display(ctxt).ok_or(glib::Error::new(
            crate::Error::GL,
            "Failed to get EGL display",
        ))?;

        let egl = egl::egl();
        let egl_image_target = egl::image_target_texture_2d_oes().ok_or(glib::Error::new(
            crate::Error::GL,
            "ImageTargetTexture2DOES support missing",
        ))?;

        const PLANE_FD_ATTRS: [i32; 4] = [
            egl::DMA_BUF_PLANE0_FD_EXT,
            egl::DMA_BUF_PLANE1_FD_EXT,
            egl::DMA_BUF_PLANE2_FD_EXT,
            egl::DMA_BUF_PLANE3_FD_EXT,
        ];
        const PLANE_PITCH_ATTRS: [i32; 4] = [
            egl::DMA_BUF_PLANE0_PITCH_EXT,
            egl::DMA_BUF_PLANE1_PITCH_EXT,
            egl::DMA_BUF_PLANE2_PITCH_EXT,
            egl::DMA_BUF_PLANE3_PITCH_EXT,
        ];
        const PLANE_OFFSET_ATTRS: [i32; 4] = [
            egl::DMA_BUF_PLANE0_OFFSET_EXT,
            egl::DMA_BUF_PLANE1_OFFSET_EXT,
            egl::DMA_BUF_PLANE2_OFFSET_EXT,
            egl::DMA_BUF_PLANE3_OFFSET_EXT,
        ];
        const PLANE_MODIFIER_LO_ATTRS: [i32; 4] = [
            egl::DMA_BUF_PLANE0_MODIFIER_LO_EXT,
            egl::DMA_BUF_PLANE1_MODIFIER_LO_EXT,
            egl::DMA_BUF_PLANE2_MODIFIER_LO_EXT,
            egl::DMA_BUF_PLANE3_MODIFIER_LO_EXT,
        ];
        const PLANE_MODIFIER_HI_ATTRS: [i32; 4] = [
            egl::DMA_BUF_PLANE0_MODIFIER_HI_EXT,
            egl::DMA_BUF_PLANE1_MODIFIER_HI_EXT,
            egl::DMA_BUF_PLANE2_MODIFIER_HI_EXT,
            egl::DMA_BUF_PLANE3_MODIFIER_HI_EXT,
        ];

        let num_planes = s.num_planes as usize;
        let mut attribs = Vec::<usize>::with_capacity(8 + num_planes * 10);

        attribs.push(egl::WIDTH as _);
        attribs.push(s.width as _);
        attribs.push(egl::HEIGHT as _);
        attribs.push(s.height as _);
        attribs.push(egl::LINUX_DRM_FOURCC_EXT as _);
        attribs.push(s.fourcc as _);

        for plane in 0..num_planes {
            attribs.push(PLANE_FD_ATTRS[plane] as _);
            let fd_plane = if s.fd[plane] >= 0 { plane } else { 0 };
            attribs.push(s.fd[fd_plane] as _);
            attribs.push(PLANE_PITCH_ATTRS[plane] as _);
            attribs.push(s.stride[plane] as _);
            attribs.push(PLANE_OFFSET_ATTRS[plane] as _);
            attribs.push(s.offset[plane] as _);
            if s.modifier != 0 {
                attribs.push(PLANE_MODIFIER_LO_ATTRS[plane] as _);
                attribs.push((s.modifier & 0xffffffff) as _);
                attribs.push(PLANE_MODIFIER_HI_ATTRS[plane] as _);
                attribs.push((s.modifier >> 32 & 0xffffffff) as _);
            }
        }
        attribs.push(egl::NONE as _);

        let img = egl
            .create_image(
                egl_dpy,
                egl::no_context(),
                egl::LINUX_DMA_BUF_EXT,
                egl::no_client_buffer(),
                &attribs,
            )
            .map_err(|e| {
                glib::Error::new(crate::Error::GL, &format!("eglCreateImage() failed: {e}"))
            })?;

        gl::BindTexture(gl::TEXTURE_2D, self.texture_id()?);
        egl_image_target(gl::TEXTURE_2D, img.as_ptr() as gl::types::GLeglImageOES);

        egl.destroy_image(egl_dpy, img).map_err(|e| {
            glib::Error::new(crate::Error::GL, &format!("eglDestroyImage() failed: {e}"))
        })?;

        let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(
            0, 0, s.width as _, s.height as _,
        ));

        self.y0_top.set(Some(s.y0_top));
        self.update_texture(Some((s.width as _, s.height as _)), Some(&region))?;
        Ok(())
    }
}
