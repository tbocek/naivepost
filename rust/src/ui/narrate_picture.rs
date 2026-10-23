//! §07-narrate#1-screen, number **2**: the Narrate page's preview picture.
//!
//! The frame is the CUT page's frame, painted by the CUT page's painter: [`crate::ui::cut_preview_frame`]'s
//! `Frame`/`Observed`/`paint` are one shared path, and this module only supplies the Narrate page's own
//! answers — which second of the session is on screen, and that a Narrate preview never holds an effect
//! in hand or draws a zoom box. That is what makes it "the finished frame (crop, zoom, stops, titles)"
//! rather than a second, drifting copy of the Cut page's picture.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::cut_preview_frame::{self as picture, Frame};

/// Install the painter on `narrate-picture`. Called from the page's `wire`, once.
pub fn install_draw(window: &adw::ApplicationWindow) {
    let Some(area) = picture_of(window) else {
        return;
    };
    let win = window.clone();
    area.set_draw_func(move |_area, cr, width, height| {
        paint(&win, cr, width.max(1) as f64, height.max(1) as f64);
    });
}

/// Ask `narrate-picture` to paint again: from `refresh`, and from the 100 ms tick so the frame the
/// person sees walks with ▶ instead of freezing at the second the press made.
pub fn repaint(window: &adw::ApplicationWindow) {
    if let Some(area) = picture_of(window) {
        area.queue_draw();
    }
}

/// The frame for one paint: the page's published second, the page's own cut, and no held effect — the
/// Narrate page edits words, not the picture (§A.3: "No effect editing here"). The fitted box comes
/// from [`cut_preview_frame::fit_size_of`], which is the one place the agreed source shape is read.
fn frame_of(fit: (f64, f64)) -> Frame {
    let state = crate::ui::narrate_page::read_state();
    let cut = current_cut();
    Frame {
        cut,
        t: state.session,
        held: None,
        drawing_box: false,
        box_px: None,
        playing: state.playing,
        // The camera stays UP here: no zoom is being armed on this page, so nothing needs the whole
        // source showing behind a box.
        camera_down: false,
        fit,
    }
}

/// The cut the frame shows: the session's `cut.json`, which is what the Narrate page is keyed to (the
/// Cut page's in-progress review cut is not this page's business).
fn current_cut() -> crate::cut::Cut {
    crate::ui::window::narrate_session_tree()
        .map(|tree| crate::cut::load(&tree).unwrap_or_default())
        .unwrap_or_default()
}

/// The panel's pixel box, from the window's own preview declaration — the Cut page's helper, so both
/// pages measure the camera against one shape.
fn fit_size(window: &adw::ApplicationWindow, width: f64, height: f64) -> (f64, f64) {
    picture::fit_size_of(window, width, height)
}

/// Paint one frame with the shared painter: mask for what the camera hides, the stop's still, and the
/// burned text/svg overlays — the same calls the Cut page's surface makes.
fn paint(window: &adw::ApplicationWindow, cr: &cairo::Context, width: f64, height: f64) {
    let fit = fit_size(window, width, height);
    let frame = frame_of(fit);
    let seen = picture::observe(&frame, None);
    // The plate the frame sits on: same dark ground as the Cut page's surface, so a headless snapshot
    // and a live window both show a picture rather than the window's grey.
    cr.set_source_rgb(0.06, 0.07, 0.09);
    cr.rectangle(0.0, 0.0, width, height);
    let _ = cr.fill();
    picture::paint(cr, &frame, &seen);
}

/// This window's `narrate-picture`.
fn picture_of(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(&content, "narrate-picture")?
        .downcast()
        .ok()
}

/// Which line's take the preview will play when it reaches it, from `from` down the cut: the FIRST
/// second at or after `from`, not the line the press is standing inside. The running tick starts a
/// line's wav when the picture ENTERS it, and a line that is already under way at `from` was entered
/// before ▶ was pressed — so it is not one the preview queued (its own start is already behind us).
/// `None` for every clip whose line has no wav: a silent line, or one never synthesized.
pub fn queued_line_for_test(tree: &crate::layout::Tree, from: f64) -> Option<usize> {
    let entries = crate::ui::narrate_page::read_state().entries;
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.s + entry.at > from)
        .map(|(index, _)| index)
        .find(|&index| crate::narrate_preview_leg::line_take(tree, &entries, index).is_some())
}

/// The take wav the preview played for one line, or `None` when it had nothing to play. A door for the
/// wire test, which has to check that a real press reached a REAL file rather than a sentence.
pub fn line_take_for_test(tree: &crate::layout::Tree, line: usize) -> Option<std::path::PathBuf> {
    crate::narrate_preview_leg::line_take(
        tree,
        &crate::ui::narrate_page::read_state().entries,
        line,
    )
}
