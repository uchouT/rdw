use gl::types::{GLint, GLuint};
use gtk::glib;
use once_cell::sync::OnceCell;
use std::cell::{Cell, RefCell};
use windows::Win32::{
    Foundation::HANDLE,
    Graphics::{
        Direct3D11::{ID3D11Device1, ID3D11Texture2D},
        Dxgi::IDXGIKeyedMutex,
    },
};

use crate::egl;

#[derive(Debug, Default)]
pub(crate) struct Helper {
    device: OnceCell<ID3D11Device1>,
    texture: RefCell<Option<ID3D11Texture2D>>,
    scanout: RefCell<Option<crate::RdwD3d11Texture2dScanout>>,
    texture_can_acquire: Cell<bool>,
    texture_id: Cell<Option<GLuint>>,
}

impl Drop for Helper {
    fn drop(&mut self) {
        if let Some(tex_id) = self.texture_id.take() {
            unsafe {
                gl::DeleteTextures(1, &tex_id);
            }
        }
    }
}

pub(crate) struct D3d11TexGuard(IDXGIKeyedMutex);

impl std::fmt::Debug for D3d11TexGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("D3d11TexGuard").finish()
    }
}

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

impl Helper {
    pub(crate) fn has_texture(&self) -> bool {
        self.texture.borrow().is_some()
    }

    pub(crate) unsafe fn device(
        &self,
        p: &super::Paintable,
    ) -> Result<&ID3D11Device1, glib::error::Error> {
        if let Some(d) = self.device.get() {
            return Ok(d);
        }

        let ctxt = p.gl_context()?;
        let egl_dpy = egl::display(ctxt)
            .ok_or("Failed to get EGL display")
            .map_err(|e| glib::Error::new(crate::Error::GL, e))?;
        let query_display = egl::query_display_attrib()
            .ok_or("No eglQueryDisplayAttrib")
            .map_err(|e| glib::Error::new(crate::Error::GL, e))?;
        let query_device = egl::query_device_attrib()
            .ok_or("No eglQueryDeviceAttrib")
            .map_err(|e| glib::Error::new(crate::Error::GL, e))?;
        let mut device: egl::EGLDevice = std::ptr::null_mut();

        if query_display(
            egl_dpy.as_ptr(),
            egl::DEVICE_EXT,
            &mut device as *mut _ as *mut _,
        ) == 0
        {
            return Err("Failed to query EGL display device")
                .map_err(|e| glib::Error::new(crate::Error::GL, e));
        }

        let mut d3d11_device: *mut ID3D11Device1 = std::ptr::null_mut();
        if query_device(
            device,
            egl::D3D11_DEVICE_ANGLE,
            &mut d3d11_device as *mut _ as *mut _,
        ) == 0
        {
            return Err("Failed to query EGL D3D11 device")
                .map_err(|e| glib::Error::new(crate::Error::GL, e));
        }

        // there should be a better way, to get a &ID3D instead
        let d3d11_device = std::ptr::NonNull::new_unchecked(d3d11_device);
        let d3d11_device: ID3D11Device1 = std::mem::transmute(d3d11_device);
        self.device.set(d3d11_device.clone()).unwrap();
        std::mem::forget(d3d11_device);

        Ok(self.device.get().unwrap())
    }

    pub(crate) fn set_d3d11_texture2d_can_acquire(&self, can_acquire: bool) {
        self.texture_can_acquire.set(can_acquire);
    }

    pub(crate) fn d3d11_texture2d_acquire0(&self) -> Result<Option<D3d11TexGuard>, String> {
        use windows::{core::ComInterface, Win32::System::Threading::INFINITE};

        if !self.texture_can_acquire.get() {
            log::debug!("can't acquire texture2d");
            return Ok(None);
        }
        let Some(tex) = &*self.texture.borrow() else {
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

    fn texture_id(&self) -> Result<GLuint, glib::error::Error> {
        match self.texture_id.get() {
            None => {
                let mut tex_id = 0;
                unsafe {
                    gl::GenTextures(1, &mut tex_id);
                    assert_eq!(gl::GetError(), gl::NO_ERROR);
                    self.texture_id.set(Some(tex_id));
                }
                Ok(tex_id)
            }
            Some(id) => Ok(id),
        }
    }

    unsafe fn texture_blit(
        &self,
        from: GLuint,
        to: GLuint,
        x: GLint,
        y: GLint,
        width: GLint,
        height: GLint,
    ) {
        let mut from_fbo = 0;
        let mut to_fbo = 0;

        gl::GenFramebuffers(1, &mut from_fbo);
        gl::BindFramebuffer(gl::READ_FRAMEBUFFER, from_fbo);
        gl::FramebufferTexture2D(
            gl::READ_FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            from,
            0,
        );
        gl::GenFramebuffers(1, &mut to_fbo);
        gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, to_fbo);
        gl::FramebufferTexture2D(
            gl::DRAW_FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            to,
            0,
        );

        gl::BlitFramebuffer(
            x,
            y,
            width,
            height,
            x,
            y,
            width,
            height,
            gl::COLOR_BUFFER_BIT,
            gl::NEAREST,
        );

        gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0);
        gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        gl::DeleteFramebuffers(1, &to_fbo);
        gl::DeleteFramebuffers(1, &from_fbo);
    }

    pub(crate) fn update_texture(
        &self,
        p: &super::Paintable,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Result<(), glib::error::Error> {
        // FIXME: could we avoid the copy?...
        let guard = self.d3d11_texture2d_acquire0();
        unsafe {
            self.texture_blit(self.texture_id()?, p.texture_id()?, x, y, w, h);
        }
        drop(guard);
        Ok(())
    }

    pub(crate) unsafe fn import_d3d11_texture2d_scanout(
        &self,
        p: &super::Paintable,
        s: Option<crate::RdwD3d11Texture2dScanout>,
    ) -> Result<(), glib::error::Error> {
        let Some(s) = s else {
            self.scanout.replace(None);
            self.texture.replace(None);
            return Ok(());
        };

        log::debug!("import texture2d");
        let d3d11_device = self.device(p)?;

        let egl_image_target = egl::image_target_texture_2d_oes()
            .ok_or("ImageTargetTexture2DOES support missing")
            .map_err(|e| glib::Error::new(crate::Error::GL, e))?;

        let ctxt = p.gl_context()?;
        let egl_dpy = egl::display(ctxt).ok_or(glib::Error::new(
            crate::Error::GL,
            "Failed to get EGL display",
        ))?;
        let egl = egl::egl();

        let d3d11_tex: ID3D11Texture2D = unsafe {
            d3d11_device
                .OpenSharedResource1(HANDLE(s.handle as _))
                .map_err(|e| format!("Failed to open shared texture: {}", e))
                .map_err(|e| glib::Error::new(crate::Error::GL, &e))?
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
            .map_err(|e| format!("eglCreateImage() failed: {}", e))
            .map_err(|e| glib::Error::new(crate::Error::GL, &e))?;

        gl::BindTexture(gl::TEXTURE_2D, self.texture_id()?);
        egl_image_target(gl::TEXTURE_2D, img.as_ptr() as gl::types::GLeglImageOES);

        egl.destroy_image(egl_dpy, img)
            .map_err(|e| format!("eglDestroyImage() failed: {}", e))
            .map_err(|e| glib::Error::new(crate::Error::GL, &e))?;

        self.scanout.replace(Some(s));
        self.texture.replace(Some(d3d11_tex));
        log::debug!("import texture2d: ok");
        Ok(())
    }
}
