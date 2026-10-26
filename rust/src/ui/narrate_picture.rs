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
    show_frame(window);
    if let Some(area) = picture_of(window) {
        area.queue_draw();
    }
}

thread_local! {
    /// What `narrate-video` shows now: the built-in player's frames (`None`), or the still last put there.
    static SHOWN: std::cell::RefCell<Option<Option<std::path::PathBuf>>> = const { std::cell::RefCell::new(None) };
    /// A recheck is already set for a listing or a decode a worker has not answered yet.
    static RECHECK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// How soon a picture still coming from a worker is looked for again.
const RECHECK_MS: u64 = 100;

/// Put the right frame on `narrate-video`: the built-in player's own while it is the preview's picture, else the
/// frame Prepare extracted for the recording the cut shows at the page's second (the scene's camera, at that second
/// of its file). Nothing extracted there leaves the dark plate.
fn show_frame(window: &adw::ApplicationWindow) {
    let Some(video) = video_of(window) else {
        return;
    };
    if crate::narrate_preview_leg::picture_built_in() {
        if SHOWN.with(|shown| shown.borrow().as_ref().is_some_and(Option::is_none)) {
            return;
        }
        if let Some(paintable) = crate::play_built_in::paintable()
            .and_then(|object| object.downcast::<gtk::gdk::Paintable>().ok())
        {
            video.set_paintable(Some(&paintable));
            SHOWN.with(|shown| *shown.borrow_mut() = Some(None));
        }
        return;
    }
    // The folder listing and the decode are the Cut page's caches (`cut_picture_cache`): a worker lists and
    // decodes, the picture already up stays up meanwhile, and a recheck shortly after picks the answer up
    // (§03-shell §1, the window thread never waits).
    let Some(still) = still_at(crate::ui::narrate_page::read_state().session) else {
        recheck(window);
        return;
    };
    if SHOWN.with(|shown| shown.borrow().as_ref() == Some(&still)) {
        return;
    }
    let texture = match still.as_ref() {
        None => None,
        Some(path) => match crate::ui::cut_picture_cache::picture(path) {
            crate::ui::cut_picture_cache::Decoded::Ready(held) => Some(held.texture),
            crate::ui::cut_picture_cache::Decoded::Missing => None,
            crate::ui::cut_picture_cache::Decoded::Coming => {
                recheck(window);
                return;
            }
        },
    };
    video.set_paintable(texture.as_ref());
    SHOWN.with(|shown| *shown.borrow_mut() = Some(still));
}

/// Look again in a moment, once: what a worker was asked for will have landed or still be coming.
fn recheck(window: &adw::ApplicationWindow) {
    if RECHECK.with(|set| set.replace(true)) {
        return;
    }
    let window = window.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(RECHECK_MS), move || {
        RECHECK.with(|set| set.set(false));
        show_frame(&window);
    });
}

/// The extracted frame the cut shows at session second `at`: `Some(None)` when Prepare left none there, `None`
/// while a worker is still listing the recording's frames.
fn still_at(at: f64) -> Option<Option<std::path::PathBuf>> {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return Some(None);
    };
    let Some((base, local)) = crate::narrate_preview_media::shown_at(&current_cut(), at) else {
        return Some(None);
    };
    let paths = crate::ui::cut_picture_cache::frames_or_coming(&tree, &base)?;
    let zero = crate::ui::cut_tracks::first_second(&paths);
    Some(crate::ui::cut_tracks::frame_near(&paths, zero + local))
}

/// This window's `narrate-video`.
fn video_of(window: &adw::ApplicationWindow) -> Option<gtk::Picture> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(&content, "narrate-video")?
        .downcast()
        .ok()
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
    // Only what the cut lays over the footage: the footage itself is `narrate-video` under this surface.
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
