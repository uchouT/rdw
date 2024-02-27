use super::*;

extern "C" {
    pub fn rdw_display_get_type() -> glib::ffi::GType;

    pub fn rdw_display_send_keys(dpy: *mut RdwDisplay, keys: *mut u32, nkeys: usize);

    pub fn rdw_display_get_display_size(
        dpy: *mut RdwDisplay,
        width: *mut usize,
        height: *mut usize,
    ) -> bool;

    pub fn rdw_display_set_display_size(dpy: *mut RdwDisplay, width: usize, height: usize);

    pub fn rdw_display_define_cursor(dpy: *mut RdwDisplay, cursor: *const gdk::ffi::GdkCursor);

    pub fn rdw_display_set_cursor_position(dpy: *mut RdwDisplay, enabled: bool, x: i32, y: i32);

    pub fn rdw_display_update_area(
        dpy: *mut RdwDisplay,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        stride: i32,
        data: *const u8,
    );

    #[cfg(unix)]
    pub fn rdw_display_set_dmabuf_scanout(dpy: *mut RdwDisplay, dmabuf: *const RdwDmabufScanout);

    #[cfg(windows)]
    pub fn rdw_display_set_d3d11_texture2d_scanout(
        dpy: *mut RdwDisplay,
        s: *const RdwD3d11Texture2dScanout,
    );

    #[cfg(windows)]
    pub fn rdw_display_set_d3d11_texture2d_can_acquire(dpy: *mut RdwDisplay, can_acquire: bool);
}
