use glib::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use std::{future::Future, pin::Pin};

mod imp;

type WriteFunc = dyn Fn(
        &str,
        &gio::OutputStream,
        glib::Priority,
    ) -> Option<Pin<Box<dyn Future<Output = Result<(), glib::Error>> + 'static>>>
    + 'static;

glib::wrapper! {
    pub struct ContentProvider(ObjectSubclass<imp::ContentProvider>) @extends gdk::ContentProvider;
}

impl ContentProvider {
    pub fn new<
        F: Fn(
                &str,
                &gio::OutputStream,
                glib::Priority,
            )
                -> Option<Pin<Box<dyn Future<Output = Result<(), glib::Error>> + 'static>>>
            + 'static,
    >(
        mime_types: &[&str],
        write_future: F,
    ) -> Self {
        let obj = glib::Object::new::<Self>();
        let imp = obj.imp();

        let mut formats = gdk::ContentFormatsBuilder::new();
        for m in mime_types {
            formats = formats.add_mime_type(*m);
        }
        imp.formats.set(formats.build()).unwrap();
        assert!(imp.write_future.set(Box::new(write_future)).is_ok());
        obj
    }
}
