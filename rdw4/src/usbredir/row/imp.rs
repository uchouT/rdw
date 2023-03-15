use glib::{clone, ParamSpec};
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};
use once_cell::sync::OnceCell;

#[derive(Debug, Default, glib::Properties, CompositeTemplate)]
#[template(file = "row.ui")]
#[properties(wrapper_type = super::Row)]
pub struct Row {
    #[template_child]
    pub label: TemplateChild<gtk::Label>,
    #[template_child]
    pub switch: TemplateChild<gtk::Switch>,

    #[property(
        get,
        set,
        construct_only,
        nick = "Device",
        blurb = "The associated device"
    )]
    pub device: OnceCell<super::Device>,
}

#[glib::object_subclass]
impl ObjectSubclass for Row {
    const NAME: &'static str = "RdwUsbRow";
    type Type = super::Row;
    type ParentType = gtk::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Row {
    fn properties() -> &'static [ParamSpec] {
        Self::derived_properties()
    }

    fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        self.derived_set_property(id, value, pspec)
    }

    fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        self.derived_property(id, pspec)
    }

    fn constructed(&self) {
        self.parent_constructed();

        let device = self.obj().device();
        device
            .bind_property("name", &*self.label, "label")
            .sync_create()
            .build();
        device
            .bind_property("active", &*self.switch, "active")
            .sync_create()
            .build();
        // because we are waiting for state changes
        device
            .bind_property("active", &*self.switch, "state")
            .sync_create()
            .build();

        self.switch.connect_state_set(
            clone!(@weak self as this => @default-panic, move |_s, state| {
                let device = this.obj().device();
                device.emit_by_name::<()>("state-set", &[&state]);
                gtk::Inhibit(true)
            }),
        );
    }

    // Needed for direct subclasses of GtkWidget;
    // Here you need to unparent all direct children
    // of your template.
    fn dispose(&self) {
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for Row {}
