use std::cell::{Cell, RefCell};

use glib::{subclass::Signal, ParamSpec};
use gtk::{glib, prelude::*, subclass::prelude::*};
use once_cell::sync::Lazy;
use usbredirhost::rusb;

#[derive(Default, glib::Properties)]
#[properties(wrapper_type = super::Device)]
pub struct Device {
    pub device: RefCell<Option<rusb::Device<rusb::Context>>>,
    #[property(get, set, nick = "Name", blurb = "The device name")]
    pub name: RefCell<String>,
    #[property(get, set, nick = "Active", blurb = "Device is redirected")]
    pub active: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for Device {
    const NAME: &'static str = "RdwUsbDevice";
    type Type = super::Device;
}

impl ObjectImpl for Device {
    fn properties() -> &'static [ParamSpec] {
        Self::derived_properties()
    }

    fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        self.derived_property(id, pspec)
    }

    fn set_property(&self, id: usize, value: &glib::Value, pspec: &ParamSpec) {
        self.derived_set_property(id, value, pspec)
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
            vec![Signal::builder("state-set")
                .param_types([bool::static_type()])
                .build()]
        });
        SIGNALS.as_ref()
    }
}

impl Device {
    pub fn set_device(&self, device: rusb::Device<rusb::Context>) {
        if let Ok((manufacturer, product)) = device_manufacturer_product(&device) {
            self.obj().set_name(format!("{} {}", manufacturer, product));
        } else {
            self.obj().set_name(format!(
                "Bus {:03} Device {:03}",
                device.bus_number(),
                device.address()
            ));
        }
        self.device.replace(Some(device));
    }

    pub fn device(&self) -> Option<rusb::Device<rusb::Context>> {
        self.device.borrow().clone()
    }
}

#[cfg(unix)]
fn read_char_attribute(major: u32, minor: u32, attribute: &str) -> std::io::Result<String> {
    use std::io::prelude::*;

    let mut file = std::fs::File::open(format!("/sys/dev/char/{}:{}/{}", major, minor, attribute))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    loop {
        if !contents.ends_with('\n') {
            break;
        }
        contents.pop();
    }
    Ok(contents)
}

// TODO: non-linux version, with usb-ids?
#[cfg(unix)]
fn device_manufacturer_product(
    device: &rusb::Device<rusb::Context>,
) -> std::io::Result<(String, String)> {
    use std::{fs::*, os::unix::fs::MetadataExt};

    let (bus, addr) = (device.bus_number(), device.address());
    let metadata = metadata(format!("/dev/bus/usb/{:03}/{:03}", bus, addr))?;
    let rdev = metadata.rdev();
    let (major, minor) = unsafe { (libc::major(rdev), libc::minor(rdev)) };
    let manufacturer = read_char_attribute(major, minor, "manufacturer");
    let product = read_char_attribute(major, minor, "product");
    if manufacturer.is_ok() || product.is_ok() {
        Ok((
            manufacturer.unwrap_or_else(|_| "N/A".into()),
            product.unwrap_or_else(|_| "N/A".into()),
        ))
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Unknown device",
        ))
    }
}

#[cfg(not(unix))]
fn device_manufacturer_product(
    device: &rusb::Device<rusb::Context>,
) -> std::io::Result<(String, String)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Unknown device",
    ))
}
