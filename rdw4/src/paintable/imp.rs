use gtk::graphene;
use std::cell::{Cell, OnceCell, RefCell};

use super::*;

#[cfg(windows)]
mod win32;

#[derive(Debug, Default)]
pub struct Paintable {
    ctxt: OnceCell<gdk::GLContext>,
    texture: RefCell<Option<gdk::Texture>>,
    texture_id: Cell<Option<gl::types::GLuint>>,
    use_rgb: Cell<bool>,
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
            let flip = !self.y0_top.get().unwrap_or(true);
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
            gdk::GLTexture::builder()
                .set_context(Some(ctxt))
                .set_id(self.texture_id()?)
                .set_width(w)
                .set_height(h)
                .set_update_region(region)
                .set_update_texture(self.texture.take().as_ref())
                .build()
        };
        self.texture.replace(Some(texture));
        if region.is_some() {
            self.obj().invalidate_contents();
        }

        Ok(())
    }

    pub(crate) fn set_size(&self, w: usize, h: usize) -> Result<(), glib::error::Error> {
        let ctxt = self.gl_context()?;
        ctxt.make_current();

        unsafe {
            assert_eq!(gl::GetError(), gl::NO_ERROR);
            gl::BindTexture(gl::TEXTURE_2D, self.texture_id()?);
            self.use_rgb.set(false);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as _,
                w as _,
                h as _,
                0,
                gl::BGRA,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
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
                self.use_rgb.set(true);
            }
            assert_eq!(gl::GetError(), gl::NO_ERROR);
        }

        self.update_texture(Some((w as _, h as _)), None)?;
        self.obj().invalidate_size();
        Ok(())
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

        // TODO: check data boundaries
        if let Some(data) = data {
            #[cfg(windows)]
            unsafe {
                self.win32.import_d3d11_texture2d_scanout(self, None)?
            };

            unsafe {
                gl::BindTexture(gl::TEXTURE_2D, self.texture_id()?);
                gl::PixelStorei(gl::UNPACK_ROW_LENGTH, stride / 4);
                if self.use_rgb.get() {
                    let mut rgb = Vec::with_capacity(data.len());
                    for pix in data.chunks(4) {
                        rgb.push(pix[2]);
                        rgb.push(pix[1]);
                        rgb.push(pix[0]);
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
                        gl::BGRA,
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
        let egl_dpy = egl::display(ctxt).ok_or(glib::Error::new(
            crate::Error::GL,
            "Failed to get EGL display",
        ))?;

        let egl = egl::egl();
        let egl_image_target = egl::image_target_texture_2d_oes().ok_or(glib::Error::new(
            crate::Error::GL,
            "ImageTargetTexture2DOES support missing",
        ))?;

        let attribs = &[
            egl::WIDTH as _,
            s.width as _,
            egl::HEIGHT as _,
            s.height as _,
            egl::LINUX_DRM_FOURCC_EXT as _,
            s.fourcc as _,
            egl::DMA_BUF_PLANE0_FD_EXT as _,
            s.fd as _,
            egl::DMA_BUF_PLANE0_PITCH_EXT as _,
            s.stride as _,
            egl::DMA_BUF_PLANE0_OFFSET_EXT as _,
            0,
            egl::DMA_BUF_PLANE0_MODIFIER_LO_EXT as _,
            (s.modifier & 0xffffffff) as _,
            egl::DMA_BUF_PLANE0_MODIFIER_HI_EXT as _,
            (s.modifier >> 32 & 0xffffffff) as _,
            egl::NONE as _,
        ];

        let img = egl
            .create_image(
                egl_dpy,
                egl::no_context(),
                egl::LINUX_DMA_BUF_EXT,
                egl::no_client_buffer(),
                attribs,
            )
            .map_err(|e| {
                glib::Error::new(crate::Error::GL, &format!("eglCreateImage() failed: {}", e))
            })?;

        gl::BindTexture(gl::TEXTURE_2D, self.texture_id()?);
        egl_image_target(gl::TEXTURE_2D, img.as_ptr() as gl::types::GLeglImageOES);

        egl.destroy_image(egl_dpy, img).map_err(|e| {
            glib::Error::new(
                crate::Error::GL,
                &format!("eglDestroyImage() failed: {}", e),
            )
        })?;

        let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(
            0,
            0,
            s.width as _,
            s.height as _,
        ));

        self.y0_top.set(Some(s.y0_top));
        self.update_texture(Some((s.width as _, s.height as _)), Some(&region))?;
        Ok(())
    }
}
