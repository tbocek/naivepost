//! §08-produce#1-screen items **2**/**3** — the Images row's slots: each slot's picture, which one is
//! the base, the crop box the base wears when its shape differs from the video's, and the four buttons.

use adw::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

use crate::produce_screen as screen;
use crate::ui::produce_page::ProduceState;

// One door per button: `(window, slot)`, spelled once because all four share it and clippy calls the
// inline form too complex to read.
type Door = Rc<dyn Fn(&adw::ApplicationWindow, usize)>;

/// The narrowest a slot's plate may be drawn.
const SLOT_PICTURE_MIN_PX: i32 = 64;

const SLOT_PICTURE_H_PX: i32 = 96;

pub struct Doors {
    pub make_base: Door,
    pub set_thumbnail: Door,
    /// Change…: the slot whose picture the image chooser replaces.
    pub change: Door,
    pub remove: Door,
}

/// The per-slot buttons for however many slots the page holds.
pub fn wire_buttons(window: &adw::ApplicationWindow, doors: &Doors) {
    let count = crate::ui::produce_page::read_state().frames.len();
    for index in 0..count {
        for prefix in [
            "image-make-base",
            "image-set-thumbnail",
            "image-change",
            "image-remove",
        ] {
            let name = format!("{prefix}-{index}");
            let Some(button) = crate::ui::line_step_button(window, &name) else {
                continue;
            };
            let w = window.clone();
            let door: Door = match prefix {
                "image-make-base" => Rc::clone(&doors.make_base),
                "image-set-thumbnail" => Rc::clone(&doors.set_thumbnail),
                "image-change" => Rc::clone(&doors.change),
                _ => Rc::clone(&doors.remove),
            };
            button.connect_clicked(move |_| door(&w, index));
        }
    }
}

/// One slot per picture, side by side, rebuilt whole because the base changes when slot 0 is removed.
pub fn draw(
    list: &gtk::Box,
    s: &ProduceState,
    window: Option<&adw::ApplicationWindow>,
    doors: &Doors,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if s.frames.is_empty() {
        // §A's empty state: two sentences where the slots would be, so the page reads as empty rather
        // than broken.
        let note = gtk::Box::new(gtk::Orientation::Vertical, 2);
        note.set_widget_name("publish-images-empty");
        let first = gtk::Label::new(Some(screen::EMPTY_IMAGES));
        first.set_xalign(0.0);
        first.add_css_class("dim-label");
        note.append(&first);
        let second = gtk::Label::new(Some(screen::EMPTY_IMAGES_HINT));
        second.set_xalign(0.0);
        second.add_css_class("dim-label");
        note.append(&second);
        list.append(&note);
        return;
    }
    let count = s.frames.len();
    for (index, path) in s.frames.iter().enumerate() {
        // §1: "up to 8 slots side by side, left to right".
        let slot = gtk::Box::new(gtk::Orientation::Vertical, 4);
        slot.set_widget_name(&format!("image-slot-{index}"));
        slot.set_hexpand(true);
        // §A: every slot shows its picture, 200 px across in a roomy column.
        let plate = gtk::Overlay::new();
        let picture = gtk::DrawingArea::new();
        picture.set_widget_name(&format!("image-picture-{index}"));
        picture.set_size_request(SLOT_PICTURE_MIN_PX, SLOT_PICTURE_H_PX);
        picture.set_hexpand(true);
        picture.add_css_class("produce-plate");
        plate.set_child(Some(&picture));
        // §1: the base wears a crop box when its shape differs from the video's, because the thumbnail
        // keeps only the video's own shape of it.
        let base_shape = if index == 0 { s.base_shape } else { None };
        if screen::shows_crop_box(base_shape, s.video_shape) {
            let crop = gtk::Box::new(gtk::Orientation::Vertical, 0);
            crop.set_widget_name(&format!("image-crop-box-{index}"));
            crop.set_tooltip_text(Some(
                "The part of this frame the thumbnail keeps \u{2014} the video's own shape. Drag it over \
what matters.",
            ));
            plate.add_overlay(&crop);
            plate.set_measure_overlay(&crop, false);
        }
        slot.append(&plate);
        let caption = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let name = gtk::Label::new(Some(path.rsplit('/').next().unwrap_or(path)));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        caption.append(&name);
        // §1: the FIRST image is the base the image model edits and the others are references the
        // instruction can name, so the slot says which it is rather than leaving it to slot order.
        let tag = gtk::Label::new(Some(if index == 0 { "base" } else { "reference" }));
        tag.set_widget_name(&format!("image-base-tag-{index}"));
        tag.add_css_class("dim-label");
        tag.set_tooltip_text(Some(if index == 0 {
            "The picture the image model edits"
        } else {
            "A reference the instruction can name \u{2014} \"the ship from the second image\""
        }));
        caption.append(&tag);
        slot.append(&caption);
        let buttons = gtk::FlowBox::new();
        buttons.set_selection_mode(gtk::SelectionMode::None);
        buttons.set_column_spacing(4);
        buttons.set_row_spacing(4);
        for action in screen::slot_actions(index, count) {
            let (widget_name, label) = match *action {
                "Make base" => (format!("image-make-base-{index}"), "Make base"),
                "Set Thumbnail" => (format!("image-set-thumbnail-{index}"), "Set Thumbnail"),
                "Change\u{2026}" => (format!("image-change-{index}"), "Change\u{2026}"),
                _ => (format!("image-remove-{index}"), "remove"),
            };
            let button = gtk::Button::with_label(label);
            button.set_widget_name(&widget_name);
            buttons.append(&button);
        }
        slot.append(&buttons);
        list.append(&slot);
    }
    // The buttons are found by name from the window root, so they can be wired the moment they exist:
    if let Some(window) = window {
        wire_buttons(window, doors);
    }
}
