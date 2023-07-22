use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene};

use crate::paintable::Paintable;

mod imp;

glib::wrapper! {
    pub struct Picture(ObjectSubclass<imp::Picture>)
        @extends gtk::Widget,
        @implements gtk::Accessible;
}

impl Default for Picture {
    fn default() -> Self {
        Self::new()
    }
}

impl Picture {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn paintable(&self) -> &Paintable {
        &self.imp().paintable
    }
}
