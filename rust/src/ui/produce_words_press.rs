//! §08-produce#1-screen item **7** — the thumbnail's text-overlay editor: the ✎ chips and their dashed
//! boxes below/over the picture, and the drag on the plate that draws a new box. Split out of
//! `produce_page`, which is at its size budget; nothing here owns state — it reads and writes the page's
//! through [`produce_page`]'s doors.

use adw::prelude::*;
use gtk4 as gtk;

use crate::produce_screen as screen;
use crate::ui::produce_page::{read_state, widget_in, widget_in_root, ProduceState};

/// One ✎ chip per word box on the picture (§1 item 7: "✎ to reword"). Rebuilt whole, because a removed
/// mark must not leave its chip behind claiming a box that no longer exists.
pub fn draw_word_chips(window: &adw::ApplicationWindow, s: &ProduceState) {
    let Some(holder) = widget_in_root("thumbnail-word-chips") else {
        return;
    };
    while let Some(child) = holder.first_child() {
        holder.remove(&child);
    }
    // The dashed boxes themselves live over the plate, in their own holder, and are rebuilt by the same
    // door so a removed mark loses its box too.
    if let Some(boxes) = widget_in_root("thumbnail-word-boxes")
        .and_then(|w| w.downcast::<gtk::Box>().ok())
    {
        while let Some(child) = boxes.first_child() {
            boxes.remove(&child);
        }
    }
    for (index, mark) in s.texts.iter().enumerate() {
        // §A: the box itself is visible ON the picture (dashed violet), not only as its chip below. The
        // widget is made here and `produce_paint::place_overlay_box` moves it into the holder floating over
        // the plate, at the same fractions that painter strokes — a box stacked in the column under the
        // picture would be beside the thumbnail, not on it.
        let box_on_picture = gtk::Box::new(gtk::Orientation::Vertical, 0);
        box_on_picture.set_widget_name(&format!("words-box-{index}"));
        box_on_picture.add_css_class("produce-word-box");
        box_on_picture.set_tooltip_text(Some(&mark.text));
        crate::ui::produce_paint::place_overlay_box(box_on_picture, mark);
        let chip = gtk::Button::with_label(&format!("\u{270e} {}", mark.text));
        chip.set_widget_name(&format!("words-chip-{index}"));
        chip.set_tooltip_text(Some("reword or remove this box"));
        chip.set_halign(gtk::Align::Start);
        let w = window.clone();
        let text = mark.text.clone();
        chip.connect_clicked(move |_| crate::ui::produce_words::open_reword(&w, index, &text));
        holder.append(&chip);
    }
}

/// The drag-a-box gesture over the picture. The box is kept as FRACTIONS of the plate, not pixels:
/// `screen::place_words` is resolution-independent, so a saved mark survives being shown at another
/// size. Each corner first goes through [`screen::snap`] against [`screen::overlay_snap_targets`] — the
/// plate's edges and middles plus every box already on it (§A: "boxes snap to the picture's and each
/// other's edges and middles") — and a drag that ends smaller than §A's 28 px floor is a look, not a box.
pub fn install_words_gesture(window: &adw::ApplicationWindow) {
    // The gesture goes on the PLATE (the `DrawingArea`), which is what the drag measures; the overlay that
    // floats the word boxes over it is its parent and takes no pointer of its own.
    let Some(picture) = widget_in(window, "thumbnail-picture") else {
        return;
    };
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    // Grabbed before the closure below takes what it needs: `add_controller` consumes nothing but the
    // handler must own its own copies of `picture` and `window`.
    let plate = picture.clone();
    let w = window.clone();
    // `drag-begin` carries the ABSOLUTE press position and `drag-end` only the OFFSET from it (the F2.8
    // lesson), so both corners are known only across the two signals.
    let start = std::rc::Rc::new(std::cell::Cell::new((0.0f64, 0.0f64)));
    let start_begin = start.clone();
    gesture.connect_drag_begin(move |_g, x, y| {
        start_begin.set((x, y));
    });
    gesture.connect_drag_end(move |_g, offset_x, offset_y| {
        let (start_x, start_y) = start.get();
        let wide = f64::from(plate.width()).max(1.0);
        let high = f64::from(plate.height()).max(1.0);
        // The targets are read when the drag ENDS, not when it was wired: whatever boxes the page holds at
        // that moment are the ones this box may line up with.
        let (xs, ys) = screen::overlay_snap_targets(wide, high, &read_state().texts);
        let near_x = screen::snap(start_x, &xs);
        let near_y = screen::snap(start_y, &ys);
        let far_x = screen::snap(start_x + offset_x, &xs);
        let far_y = screen::snap(start_y + offset_y, &ys);
        let x0 = near_x.min(far_x).max(0.0) / wide;
        let y0 = near_y.min(far_y).max(0.0) / high;
        let wf = (far_x - near_x).abs() / wide;
        let hf = (far_y - near_y).abs() / high;
        if screen::box_is_too_small(wf * wide, hf * high) {
            return;
        }
        crate::ui::produce_words::open_place(
            &w,
            x0 + wf / 2.0,
            y0 + hf / 2.0,
            wf.min(1.0),
            hf.min(1.0),
        );
    });
    picture.add_controller(gesture.clone());
    WORDS_GESTURE.with(|held| *held.borrow_mut() = Some(gesture));
}

thread_local! {
    /// The picture's drag controller, kept so a test can fire a real drag the way `ui::zoom_gesture`
    /// lets `zoom_drag_widgets.rs` fire the Cut panel's. Nothing in the program asks for it — hence
    /// [`words_gesture_for_test`]'s name.
    static WORDS_GESTURE: std::cell::RefCell<Option<gtk::GestureDrag>> = const {
        std::cell::RefCell::new(None)
    };
}

/// The thumbnail picture's drag gesture, for a test that drives the overlay editor through the real
/// widget (`emit_by_name("drag-begin"/"drag-end")`). Program code never calls this.
pub fn words_gesture_for_test(_window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    WORDS_GESTURE.with(|held| held.borrow().clone())
}
