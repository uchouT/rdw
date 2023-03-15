mod imp;

use super::Device;
use gtk::{glib, subclass::prelude::*};

glib::wrapper! {
    pub struct Row(ObjectSubclass<imp::Row>) @extends gtk::Widget;
}

impl Row {
    pub(crate) fn new(device: &Device) -> Self {
        glib::Object::builder().property("device", device).build()
    }

    pub(crate) fn switch(&self) -> &gtk::Switch {
        &self.imp().switch
    }
}
