//! §08-produce#1-screen item **7** — the thumbnail's text-overlay editor: the ✎ chips and their dashed
//! boxes below/over the picture, and the drag on the plate that draws a new box or moves and resizes one. Split out of
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
    crate::ui::produce_paint::clear_overlay_boxes();
    // The title is a box like the others while the picture prints it, and its ✎ edits the picture's line only,
    // never the video's title (the prototype's `editTitle`).
    if s.title_seeded && !s.thumb_title.trim().is_empty() {
        let chip = gtk::Button::with_label(&format!("\u{270e} {}", s.thumb_title));
        chip.set_widget_name("words-chip-title");
        chip.set_tooltip_text(Some(
            "reword or remove the title printed on the picture (the video's title stays as it is)",
        ));
        chip.set_halign(gtk::Align::Start);
        let w = window.clone();
        let text = s.thumb_title.clone();
        chip.connect_clicked(move |_| crate::ui::produce_words::open_title(&w, &text));
        holder.append(&chip);
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

/// Which box a drag on the picture holds: the title band or one marked text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hold {
    Title,
    Mark(usize),
}

/// A drag that took hold of a box: which box, by what, where the hand pressed and the box as it was, all in
/// the picture's own pixels.
#[derive(Clone, Copy)]
struct Grab {
    hold: Hold,
    grip: screen::Grip,
    press: (f64, f64),
    was: (f64, f64, f64, f64),
}

/// The boxes on the picture in the picture's pixels, bottom first, as the plate paints them: the title band
/// while the picture prints the title, then every marked text.
fn boxes_on_picture(s: &ProduceState, wide: f64, high: f64) -> Vec<(Hold, (f64, f64, f64, f64))> {
    let painted = crate::ui::produce_paint::layout_marks(
        &s.texts,
        &s.thumb_title,
        s.title_seeded,
        &crate::ui::produce_paint::title_band(),
        wide,
        high,
    );
    let title = painted.len() > s.texts.len();
    painted
        .into_iter()
        .enumerate()
        .map(|(at, (x, y, w, h, _))| {
            let hold = match (title, at) {
                (true, 0) => Hold::Title,
                (true, n) => Hold::Mark(n - 1),
                (false, n) => Hold::Mark(n),
            };
            (hold, (x, y, w, h))
        })
        .collect()
}

/// The box a press at `(x, y)` (picture px) takes hold of, and by what: the topmost whose border or middle is
/// under it, because the one painted on top is the one the eye is pointing at.
fn grab_at(s: &ProduceState, x: f64, y: f64, wide: f64, high: f64) -> Option<Grab> {
    boxes_on_picture(s, wide, high)
        .into_iter()
        .rev()
        .find_map(|(hold, rect)| {
            let grip = screen::grip_at(x, y, rect);
            grip.holds().then_some(Grab {
                hold,
                grip,
                press: (x, y),
                was: rect,
            })
        })
}

/// What a held box lands on: the picture's edges and middles and every OTHER box's edges and middle.
fn targets_beside(s: &ProduceState, held: Hold, wide: f64, high: f64) -> (Vec<f64>, Vec<f64>) {
    let mut xs = vec![0.0, wide / 2.0, wide];
    let mut ys = vec![0.0, high / 2.0, high];
    for (hold, (x, y, w, h)) in boxes_on_picture(s, wide, high) {
        if hold != held {
            xs.extend([x, x + w / 2.0, x + w]);
            ys.extend([y, y + h / 2.0, y + h]);
        }
    }
    (xs, ys)
}

/// Where a held box is after the hand moved `(dx, dy)` from the press: a border drag resizes (the edge under
/// the hand snaps), a drag from the middle slides the whole box (any of its lines snaps).
fn moved(grab: &Grab, dx: f64, dy: f64, xs: &[f64], ys: &[f64]) -> (f64, f64, f64, f64) {
    let (x0, y0, w0, h0) = grab.was;
    if grab.grip.horiz || grab.grip.vert {
        let x = screen::snap(grab.press.0 + dx, xs);
        let y = screen::snap(grab.press.1 + dy, ys);
        return screen::resize_box(x, y, grab.was, grab.grip);
    }
    (
        screen::snap_slide(x0 + dx, w0, xs),
        screen::snap_slide(y0 + dy, h0, ys),
        w0,
        h0,
    )
}

/// The lines a drag lands on: none while Shift is held, so the box follows the hand smoothly.
fn unless_shift(gesture: &gtk::GestureDrag, targets: (Vec<f64>, Vec<f64>)) -> (Vec<f64>, Vec<f64>) {
    let shift = gesture
        .current_event_state()
        .contains(gtk::gdk::ModifierType::SHIFT_MASK);
    if shift {
        (Vec::new(), Vec::new())
    } else {
        targets
    }
}

/// The marked text's own dashed widget, hidden while the painted box follows the hand, so the box is not seen
/// twice, once where it was and once where it goes.
fn show_widget_of(hold: Hold, shown: bool) {
    if let Hold::Mark(index) = hold {
        if let Some(widget) = widget_in_root(&format!("words-box-{index}")) {
            widget.set_opacity(if shown { 1.0 } else { 0.0 });
        }
    }
}

/// The drag on the picture (§A, the prototype's `textOverlay`). A press on a box's border resizes it, a press
/// in its middle moves it, the title band included; the box follows the hand while the drag lasts and is kept
/// when it ends. A press clear of every box draws a new one. Every box is kept as FRACTIONS of the picture,
/// not pixels, so a saved mark survives being shown at another size; edges snap to
/// [`screen::overlay_snap_targets`]' lines, the picture's edges and middles plus every other box (§A: "boxes
/// snap to the picture's and each other's edges and middles"), and a new box that ends smaller than §A's 28 px
/// floor is a look, not a box. The pointer says what a press would do before it is pressed.
pub fn install_words_gesture(window: &adw::ApplicationWindow) {
    // The gesture goes on the PLATE (the `DrawingArea`), which is what the drag measures; the overlay that
    // floats the word boxes over it is its parent and takes no pointer of its own.
    let Some(picture) = widget_in(window, "thumbnail-picture") else {
        return;
    };
    // Measured on the PICTURE where it sits on the plate, not on the plate: a mark is a fraction of the picture
    // (§A), and a wide thumbnail letterboxed in a taller plate put every box below where it was drawn when the
    // plate was the unit.
    let on_picture = {
        let plate = picture.clone();
        move || {
            let (left, top, wide, high) = crate::ui::produce_paint::picture_on_plate(
                f64::from(plate.width()).max(1.0),
                f64::from(plate.height()).max(1.0),
            );
            (left, top, wide.max(1.0), high.max(1.0))
        }
    };
    let motion = gtk::EventControllerMotion::new();
    let plate = picture.clone();
    let where_ = on_picture.clone();
    motion.connect_motion(move |_, x, y| {
        if dragging().is_some() {
            return;
        }
        let (left, top, wide, high) = where_();
        let name = grab_at(&read_state(), x - left, y - top, wide, high)
            .map_or("default", |grab| grab.grip.cursor());
        plate.set_cursor_from_name(Some(name));
    });
    picture.add_controller(motion);

    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    // `drag-begin` carries the ABSOLUTE press position and `drag-update`/`drag-end` only the OFFSET from it
    // (the F2.8 lesson), so the press is kept across the signals, with the box it took if it took one.
    let start = std::rc::Rc::new(std::cell::Cell::new((0.0f64, 0.0f64)));
    let held = std::rc::Rc::new(std::cell::Cell::new(None::<Grab>));
    let (start_begin, held_begin, where_) = (start.clone(), held.clone(), on_picture.clone());
    gesture.connect_drag_begin(move |_g, x, y| {
        start_begin.set((x, y));
        let (left, top, wide, high) = where_();
        held_begin.set(grab_at(&read_state(), x - left, y - top, wide, high));
    });
    let (held_update, where_, plate) = (held.clone(), on_picture.clone(), picture.clone());
    gesture.connect_drag_update(move |g, dx, dy| {
        let Some(grab) = held_update.get() else {
            return;
        };
        let (_, _, wide, high) = where_();
        let (xs, ys) = unless_shift(g, targets_beside(&read_state(), grab.hold, wide, high));
        let rect = moved(&grab, dx, dy, &xs, &ys);
        DRAGGING.with(|now| {
            *now.borrow_mut() = Some((grab.hold, screen::box_fractions(rect, wide, high)))
        });
        show_widget_of(grab.hold, false);
        plate.queue_draw();
    });
    let w = window.clone();
    gesture.connect_drag_end(move |g, offset_x, offset_y| {
        let (left, top, wide, high) = on_picture();
        if let Some(grab) = held.take() {
            DRAGGING.with(|now| *now.borrow_mut() = None);
            show_widget_of(grab.hold, true);
            // A press that never travelled is not an edit of the box it landed on: its ✎ is the chip below.
            if offset_x.abs() < 2.0 && offset_y.abs() < 2.0 {
                crate::ui::produce_paint::repaint_plates();
                return;
            }
            let (xs, ys) = unless_shift(g, targets_beside(&read_state(), grab.hold, wide, high));
            let rect = moved(&grab, offset_x, offset_y, &xs, &ys);
            crate::ui::produce_page::press_words_move(
                &w,
                grab.hold,
                screen::box_fractions(rect, wide, high),
            );
            return;
        }
        let (start_x, start_y) = start.get();
        let (start_x, start_y) = (start_x - left, start_y - top);
        // The targets are read when the drag ENDS, not when it was wired: whatever boxes the page holds at
        // that moment are the ones this box may line up with.
        let (xs, ys) = unless_shift(
            g,
            screen::overlay_snap_targets(wide, high, &read_state().texts),
        );
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

/// The box being dragged and where the hand has it now, in fractions of the picture, or `None` between drags.
/// The plate paints a held box from here, so the words follow the hand before anything is kept.
pub fn dragging() -> Option<(Hold, crate::project::TitleBox)> {
    DRAGGING.with(|now| *now.borrow())
}

thread_local! {
    /// [`dragging`]'s answer: set on every step of a drag that holds a box, cleared when it ends.
    static DRAGGING: std::cell::RefCell<Option<(Hold, crate::project::TitleBox)>> =
        const { std::cell::RefCell::new(None) };
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
