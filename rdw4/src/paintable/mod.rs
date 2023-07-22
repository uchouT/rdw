use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{cairo, gdk, glib, graphene};

mod imp;

glib::wrapper! {
    pub struct Paintable(ObjectSubclass<imp::Paintable>)
        @implements gdk::Paintable;
}

impl Default for Paintable {
    fn default() -> Self {
        Self::new()
    }
}

impl Paintable {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_size(&self, w: usize, h: usize) -> Result<(), glib::error::Error> {
        self.imp().set_size(w, h)
    }

    pub fn size(&self) -> (i32, i32) {
        self.imp().size()
    }

    pub fn update_area(
        &self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        stride: i32,
        data: Option<&[u8]>,
    ) -> Result<(), glib::error::Error> {
        self.imp().update_area(x, y, w, h, stride, data)
    }

    pub fn import_dmabuf(&self, s: &crate::RdwDmabufScanout) -> Result<(), glib::error::Error> {
        unsafe { self.imp().import_dmabuf(s) }
    }
}
