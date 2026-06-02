use rdw::gtk::glib;

#[cfg(not(feature = "bindings"))]
#[allow(dead_code)]
mod pure {
    use super::glib::{self, flags, translate::IntoGlib, types::StaticType};

    #[flags(name = "RdwRdpPerformanceFlags")]
    #[repr(C)]
    pub enum RdpPerformanceFlags {
        DisableWallpaper = 0x0000_0001,
        DisableFullwindowDrag = 0x0000_0002,
        DisableMenuAnimations = 0x0000_0004,
        DaibleTheming = 0x0000_0008,
        Reserved1 = 0x0000_0010,
        DisableCursorShadow = 0x0000_0020,
        DisableCursorSettings = 0x0000_0040,
        EnableFontSmoothing = 0x0000_0080,
        EnableDesktopComposition = 0x0000_0100,
        Reserved2 = 0x8000_0000,
    }

    pub type RdwRdpPerformanceFlags = <RdpPerformanceFlags as IntoGlib>::GlibType;

    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_WALLPAPER: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::DisableWallpaper.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_FULLWINDOWDRAG: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::DisableFullwindowDrag.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_MENUANIMATIONS: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::DisableMenuAnimations.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_THEMING: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::DaibleTheming.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_RESERVED1: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::Reserved1.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_CURSOR_SHADOW: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::DisableCursorShadow.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_CURSORSETTINGS: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::DisableCursorSettings.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_ENABLE_FONT_SMOOTHING: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::EnableFontSmoothing.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_ENABLE_DESKTOP_COMPOSITION: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::EnableDesktopComposition.bits();
    pub const RDW_RDP_PERFORMANCE_FLAGS_RESERVED2: RdwRdpPerformanceFlags =
        RdpPerformanceFlags::Reserved2.bits();

    #[no_mangle]
    pub unsafe extern "C" fn rdw_rdp_performance_flags_get_type() -> glib::ffi::GType {
        RdwRdpPerformanceFlags::static_type().into_glib()
    }
}

#[cfg(not(feature = "bindings"))]
#[allow(unused_imports)]
pub use pure::*;

/// cbindgen:ignore
#[cfg(feature = "bindings")]
mod ffi {
    use super::glib;
    use std::os::raw::c_uint;

    pub type RdwRdpPerformanceFlags = c_uint;

    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_WALLPAPER: RdwRdpPerformanceFlags = 0x0000_0001;
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_FULLWINDOWDRAG: RdwRdpPerformanceFlags =
        0x0000_0002;
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_MENUANIMATIONS: RdwRdpPerformanceFlags =
        0x0000_0004;
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_THEMING: RdwRdpPerformanceFlags = 0x0000_0008;
    pub const RDW_RDP_PERFORMANCE_FLAGS_RESERVED1: RdwRdpPerformanceFlags = 0x0000_0010;
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_CURSOR_SHADOW: RdwRdpPerformanceFlags = 0x0000_0020;
    pub const RDW_RDP_PERFORMANCE_FLAGS_DISABLE_CURSORSETTINGS: RdwRdpPerformanceFlags =
        0x0000_0040;
    pub const RDW_RDP_PERFORMANCE_FLAGS_ENABLE_FONT_SMOOTHING: RdwRdpPerformanceFlags = 0x0000_0080;
    pub const RDW_RDP_PERFORMANCE_FLAGS_ENABLE_DESKTOP_COMPOSITION: RdwRdpPerformanceFlags =
        0x0000_0100;
    pub const RDW_RDP_PERFORMANCE_FLAGS_RESERVED2: RdwRdpPerformanceFlags = 0x8000_0000;

    extern "C" {
        pub fn rdw_rdp_performance_flags_get_type() -> glib::ffi::GType;
    }
}

#[cfg(feature = "bindings")]
pub use ffi::*;
