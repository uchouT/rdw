use super::*;
use glib::clone;

#[derive(Debug, Default)]
pub struct Picture {
    pub(crate) paintable: Paintable,
    y_inverted: bool,
}

/// cbindgen:ignore
#[glib::object_subclass]
impl ObjectSubclass for Picture {
    const NAME: &'static str = "RdwPicture";
    type Type = super::Picture;
    type ParentType = gtk::Widget;
}

impl ObjectImpl for Picture {
    fn constructed(&self) {
        self.parent_constructed();
        self.obj().set_focusable(true);

        self.paintable
            .connect_invalidate_contents(clone!(@weak self as this => move |_| {
                this.obj().queue_draw();
            }));
        self.paintable
            .connect_invalidate_size(clone!(@weak self as this => move |_| {
                this.obj().queue_resize();
            }));
    }
}

impl WidgetImpl for Picture {
    fn request_mode(&self) -> gtk::SizeRequestMode {
        gtk::SizeRequestMode::HeightForWidth
    }

    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        let (w, h) = (self.obj().width() as _, self.obj().height() as _);

        if !self.y_inverted {
            self.paintable.snapshot(snapshot, w, h);
        } else {
            snapshot.save();
            snapshot.translate(&graphene::Point::new(0.0, h as _));
            snapshot.scale(1.0, -1.0);
            self.paintable.snapshot(snapshot, w, h);
            snapshot.restore();
        }
    }

    fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
        if for_size == 0 {
            return (0, 0, -1, -1);
        }

        let (width, height) = (
            self.paintable.intrinsic_width().max(640),
            self.paintable.intrinsic_height().max(480),
        );

        if orientation == gtk::Orientation::Horizontal {
            let (nat_width, _nat_height) = self.paintable.compute_concrete_size(
                0f64,
                for_size.max(0) as _,
                width as _,
                height as _,
            );
            (0, nat_width.ceil() as _, -1, -1)
        } else {
            let (_nat_width, nat_height) = self.paintable.compute_concrete_size(
                for_size.max(0) as _,
                0f64,
                width as _,
                height as _,
            );
            (0, nat_height.ceil() as _, -1, -1)
        }
    }
}
