use gtk::subclass::prelude::*;
use once_cell::sync::OnceCell;

use super::*;

#[repr(C)]
pub struct RdwContentProviderClass {
    pub parent_class: gdk::ffi::GdkContentProviderClass,
}

unsafe impl ClassStruct for RdwContentProviderClass {
    type Type = ContentProvider;
}

#[repr(C)]
pub struct RdwContentProvider {
    parent: gdk::ffi::GdkContentProvider,
}

impl std::fmt::Debug for RdwContentProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("RdwContentProvider")
            .field("parent", &self.parent)
            .finish()
    }
}

unsafe impl InstanceStruct for RdwContentProvider {
    type Type = ContentProvider;
}

#[derive(Default)]
pub struct ContentProvider {
    pub(crate) formats: OnceCell<gdk::ContentFormats>,
    pub(crate) write_future: OnceCell<Box<WriteFunc>>,
}

#[glib::object_subclass]
impl ObjectSubclass for ContentProvider {
    const NAME: &'static str = "RdwContentProvider";
    type Type = super::ContentProvider;
    type ParentType = gdk::ContentProvider;
    type Class = RdwContentProviderClass;
    type Instance = RdwContentProvider;
}

impl ObjectImpl for ContentProvider {}

impl ContentProviderImpl for ContentProvider {
    fn formats(&self) -> gdk::ContentFormats {
        self.formats.get().unwrap().clone()
    }

    fn write_mime_type_future(
        &self,
        mime_type: &str,
        stream: &gio::OutputStream,
        io_priority: glib::Priority,
    ) -> Pin<Box<dyn Future<Output = Result<(), glib::Error>> + 'static>> {
        let future = self.write_future.get().unwrap()(mime_type, stream, io_priority);
        future.unwrap_or_else(|| {
            Box::pin(async move {
                Err(glib::Error::new(
                    gio::IOErrorEnum::Failed,
                    "write_mime failed!",
                ))
            })
        })
    }
}
