use super::*;
use gtk::graphene;
use std::cell::{Cell, RefCell};

#[cfg(windows)]
use std::cell::OnceCell;

#[cfg(windows)]
mod win32;

#[derive(Debug, Default)]
pub struct Paintable {
    buffer: RefCell<Vec<u8>>,
    width: Cell<i32>,
    height: Cell<i32>,
    pixel_format: Cell<PixelFormat>,
    texture: RefCell<Option<gdk::Texture>>,
    y0_top: Cell<Option<bool>>,

    #[cfg(windows)]
    ctxt: OnceCell<gdk::GLContext>,
    #[cfg(windows)]
    texture_id: Cell<Option<gl::types::GLuint>>,
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
        #[cfg(windows)]
        if let Some(tex_id) = self.texture_id.take() {
            unsafe {
                gl::DeleteTextures(1, &tex_id);
            }
        }
    }
}

impl PaintableImpl for Paintable {
    fn intrinsic_width(&self) -> i32 {
        self.width.get()
    }

    fn intrinsic_height(&self) -> i32 {
        self.height.get()
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
        (self.width.get(), self.height.get())
    }

    fn memory_format(format: PixelFormat) -> gdk::MemoryFormat {
        match format {
            PixelFormat::Bgra => gdk::MemoryFormat::B8g8r8a8Premultiplied,
            PixelFormat::Rgba => gdk::MemoryFormat::R8g8b8a8Premultiplied,
            PixelFormat::Bgrx => gdk::MemoryFormat::B8g8r8x8,
            other => {
                log::warn!("Unrecognized pixel format {other:?}, falling back to B8G8R8A8");
                gdk::MemoryFormat::B8g8r8a8Premultiplied
            }
        }
    }

    pub(crate) fn set_pixel_format(&self, format: PixelFormat) -> Result<(), glib::error::Error> {
        if self.pixel_format() == format {
            return Ok(());
        }
        self.pixel_format.set(format);

        #[cfg(windows)]
        {
            let (w, h) = self.size();
            if w > 0 && h > 0 {
                self.recreate_gl_texture((w, h), format)?;
            }
        }

        Ok(())
    }

    pub(crate) fn set_size(&self, w: usize, h: usize) -> Result<(), glib::error::Error> {
        if self.size() == (w as _, h as _) {
            return Ok(());
        }
        let (w, h) = (w as i32, h as i32);
        self.width.set(w);
        self.height.set(h);
        self.buffer
            .borrow_mut()
            .resize((w as usize) * (h as usize) * 4, 0);
        self.texture.replace(None);

        #[cfg(windows)]
        self.recreate_gl_texture((w, h), self.pixel_format.get())?;

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
        let (max_w, max_h) = self.size();
        let x = x.clamp(0, max_w);
        let y = y.clamp(0, max_h);
        let w = w.clamp(0, max_w - x);
        let h = h.clamp(0, max_h - y);

        if let Some(data) = data {
            #[cfg(windows)]
            unsafe {
                self.win32.import_d3d11_texture2d_scanout(self, None)?
            };

            let buf_stride = max_w as usize * 4;
            let mut buffer = self.buffer.borrow_mut();
            for row in 0..h as usize {
                let src_start = row * stride as usize;
                let src_end = src_start + w as usize * 4;
                let dst_start = (y as usize + row) * buf_stride + x as usize * 4;
                let dst_end = dst_start + w as usize * 4;
                if src_end <= data.len() && dst_end <= buffer.len() {
                    buffer[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
                }
            }

            let bytes = glib::Bytes::from(&*buffer);
            let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(x, y, w, h));
            let texture = gdk::MemoryTextureBuilder::new()
                .set_bytes(Some(&bytes))
                .set_format(Self::memory_format(self.pixel_format.get()))
                .set_width(max_w)
                .set_height(max_h)
                .set_stride(buf_stride)
                .set_update_texture(self.texture.borrow().as_ref())
                .set_update_region(Some(&region))
                .build();
            self.texture.replace(Some(texture));
            self.obj().invalidate_contents();
        } else {
            #[cfg(windows)]
            if self.win32.has_texture() {
                self.win32.update_texture(self, x, y, w, h)?;
                let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(x, y, w, h));
                self.update_gl_texture(None, Some(&region))?;
            }
            #[cfg(not(windows))]
            log::warn!("update_area called with no data on non-Windows platform");
        }

        Ok(())
    }

    #[cfg(unix)]
    pub(crate) unsafe fn import_dmabuf(
        &self,
        s: &crate::RdwDmabufScanout,
    ) -> Result<(), glib::error::Error> {
        use std::os::unix::io::RawFd;

        let display = gdk::Display::default()
            .ok_or(glib::Error::new(crate::Error::GL, "No default display"))?;
        let num_planes = s.num_planes as usize;

        let mut dup_fds: Vec<RawFd> = Vec::with_capacity(num_planes);
        for plane in 0..num_planes {
            let fd = libc::dup(s.fd[plane]);
            if fd < 0 {
                for &duped in &dup_fds {
                    libc::close(duped);
                }
                return Err(glib::Error::new(
                    crate::Error::GL,
                    "Failed to dup dmabuf fd",
                ));
            }
            dup_fds.push(fd);
        }

        let mut builder = gdk::DmabufTextureBuilder::new()
            .set_display(&display)
            .set_fourcc(s.fourcc)
            .set_modifier(s.modifier)
            .set_width(s.width)
            .set_height(s.height)
            .set_n_planes(s.num_planes);

        for (plane, &fd) in dup_fds.iter().enumerate() {
            builder = builder.set_fd(plane as u32, fd);
            builder = builder.set_offset(plane as u32, s.offset[plane]);
            builder = builder.set_stride(plane as u32, s.stride[plane]);
        }

        let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(
            0,
            0,
            s.width as _,
            s.height as _,
        ));

        let texture = builder
            .set_update_texture(self.texture.borrow().as_ref())
            .set_update_region(Some(&region))
            .build_with_release_func(move || {
                for fd in dup_fds {
                    libc::close(fd);
                }
            })
            .map_err(|e| {
                glib::Error::new(
                    crate::Error::GL,
                    &format!("Failed to build dmabuf texture: {e}"),
                )
            })?;

        self.width.set(s.width as i32);
        self.height.set(s.height as i32);
        self.y0_top.set(Some(s.y0_top));
        self.texture.replace(Some(texture));
        self.obj().invalidate_size();
        self.obj().invalidate_contents();
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn gl_context(&self) -> Result<&gdk::GLContext, glib::error::Error> {
        let ctxt = if let Some(ctxt) = self.ctxt.get() {
            ctxt
        } else {
            let ctxt = gdk::Display::default().unwrap().create_gl_context()?;
            self.ctxt.set(ctxt).unwrap();
            self.ctxt.get().unwrap()
        };
        Ok(ctxt)
    }

    #[cfg(windows)]
    pub(crate) fn texture_id(&self) -> Result<gl::types::GLuint, glib::error::Error> {
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

    #[cfg(windows)]
    fn recreate_gl_texture(
        &self,
        size: (i32, i32),
        format: PixelFormat,
    ) -> Result<(), glib::error::Error> {
        let (w, h) = size;
        let ctxt = self.gl_context()?;
        ctxt.make_current();

        unsafe {
            assert_eq!(gl::GetError(), gl::NO_ERROR);
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
            }
            assert_eq!(gl::GetError(), gl::NO_ERROR);
        }

        self.update_gl_texture(Some((w, h)), None)?;
        Ok(())
    }

    #[cfg(windows)]
    fn update_gl_texture(
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
                .set_format(if self.pixel_format.get().has_alpha() {
                    gdk::MemoryFormat::R8g8b8a8
                } else {
                    gdk::MemoryFormat::R8g8b8x8
                })
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
}
