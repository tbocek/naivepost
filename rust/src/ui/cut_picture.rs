//! §05-cut#1-screen badge **1** — the preview's picture. `spec/inventory/cut.md` §A asks for "preview
//! (GtkPicture contain-fit, min height 160, click toggles play)". The panel already drew the camera rect,
//! the dim and the overlays, but nothing behind them: no frame of the session was ever shown, and a click on
//! the preview did nothing. This module owns the frame and the press; every number about WHICH second and
//! WHICH recording comes from the doors the page already keeps (`line_position`, `watched_row`,
//! `page_recordings`) and from Prepare's own extracted frames through `cut_tracks::frame_paths`/`frame_near`.
//!
//! Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout;
use crate::timeline;
use crate::ui::{self, cut_tracks};

/// §A's own number: the picture never shrinks below this, however narrow the column gets.
pub const PICTURE_MIN_HEIGHT: i32 = 160;

/// §A's sentence for the click, spelled once here because it is this widget's tooltip.
pub const CLICK_TIP: &str = "click toggles play";

/// The clickable host around the picture.
///
/// DECISION: §A names a `GtkPicture` and says the preview is clicked, and is silent on which widget takes
/// the press. A `gtk::Picture` ignores button events, so the press goes to a flat `gtk::Button` that wraps
/// it — the picture stays the thing that shows the frame, the button is only how a click arrives, and a test
/// can fire it the toolkit's way (`emit_by_name("clicked")`) instead of synthesising coordinates.
pub fn build() -> gtk::Button {
    let picture = gtk::Picture::new();
    picture.set_widget_name("preview-picture");
    // Contain-fit (§A), so a 16:9 frame letterboxes inside the panel instead of squashing.
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(-1, PICTURE_MIN_HEIGHT);
    picture.set_can_shrink(true);
    // The picture is the panel's content at whatever size the panel got (§A's paned gives it far more than
    // `PREVIEW_PANEL_W` now), so it fills that box and contain-fits the frame inside it -- a picture that
    // only asked for its minimum drew as a small plate in the middle of the panel.
    picture.set_hexpand(true);
    picture.set_vexpand(true);

    let button = gtk::Button::new();
    button.set_widget_name("preview-picture-button");
    button.set_tooltip_text(Some(CLICK_TIP));
    // Same rule one level up: the button is only how a click reaches the picture, so it takes the box.
    button.set_hexpand(true);
    button.set_vexpand(true);
    button.set_child(Some(&picture));
    button
}

/// The picture widget of THIS window, newest window winning like every other per-window reader here.
fn picture(window: &adw::ApplicationWindow) -> Option<gtk::Picture> {
    find(window.content()?.upcast_ref(), "preview-picture")?
        .downcast()
        .ok()
}

fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Some(found) = find(&widget, name) {
            return Some(found);
        }
    }
    None
}

/// Put one extracted frame on the panel. Returns whether anything is showing: a path that does not decode
/// (or none at all) clears the picture rather than leaving a stale frame up, so the plate reads as empty
/// tape instead of a broken image. The load is gdk-pixbuf's real decoder over a real file — the same
/// library GTK itself uses for a `Picture`, and already in the lock through the `gtk` crate.
///
/// The request is remembered beside the frame ([`preview_asked_for_test`]) so a check can prove WHICH frame
/// the panel was given: a headless window cannot read a paintable back off a `Picture`.
pub fn show_frame(window: &adw::ApplicationWindow, path: Option<&std::path::Path>) -> bool {
    let Some(picture) = picture(window) else {
        return false;
    };
    let shown = match path {
        Some(file) => gtk::gdk_pixbuf::Pixbuf::from_file(file)
            .map(|pixbuf| gtk::gdk::Texture::for_pixbuf(&pixbuf))
            .ok(),
        None => None,
    };
    match shown {
        Some(texture) => {
            picture.set_paintable(Some(&texture));
            FRAME_SHOWN.with(|slot| *slot.borrow_mut() = path.map(|p| p.to_path_buf()));
            true
        }
        None => {
            picture.set_paintable(None::<&gtk::gdk::Paintable>);
            FRAME_SHOWN.with(|slot| *slot.borrow_mut() = None);
            false
        }
    }
}

// The frame the panel was last given, or `None` when it was cleared. `preview_asked_for_test` reports what
// the panel ASKED for its own footage; this reports what actually went up, which is the half a card pass has
// to answer for because a card's frame is not one of the session's files. It is ALSO what a stop's still is
// painted from (§06-effects#2: "a stop's still — rendered from the scene's own camera, fitted on the footage's
// transform"): the preview never extracts a second frame, it re-uses the one already on the panel and crops it
// into the box the borrowed camera says.
thread_local! {
    /// The frame last put on the panel by [`show_frame`]. Read back by [`frame_shown`] (the stop's still) and by
    /// [`frame_shown_for_test`].
    static FRAME_SHOWN: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
    /// The last frame request `refresh` made: which recording the panel asked, at which FILE second. See
    /// [`preview_asked_for_test`] — the program never reads this back.
    static PREVIEW_ASKED: std::cell::RefCell<Option<(String, f64)>> = const { std::cell::RefCell::new(None) };
    /// The recording a snapshot named for the preview's frame search. See [`preview_recording_base`].
    static PREVIEW_BASE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    /// The folder a snapshot named for that search. See [`snapshot_tree`].
    static SNAPSHOT_DIR: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// The frame the panel is showing, as the file it came from — the program's own door. A `gtk::Picture` gives
/// nothing back (a paintable cannot be compared to a path), so this is how anything that needs the frame the user
/// is looking at asks for it: today, the stop's still in `cut_preview_frame`, which paints the held frame out of
/// exactly this file rather than asking ffmpeg for another one.
pub fn frame_shown() -> Option<std::path::PathBuf> {
    FRAME_SHOWN.with(|slot| slot.borrow().clone())
}

/// The frame the panel is showing, as the file it came from. A `gtk::Picture` gives nothing back — a
/// paintable cannot be compared to a path — so this is the door a check reads, and `show_frame` is its only
/// writer. Named for the same convention as [`preview_asked_for_test`]: the program never reads it back.
pub fn frame_shown_for_test() -> Option<std::path::PathBuf> {
    FRAME_SHOWN.with(|slot| slot.borrow().clone())
}

/// The recording whose frames the preview may show, when someone has named one (the snapshot bench renders a
/// fixture folder that carries no extracted frames of its own). `None` means answer from the session's
/// recordings, which is what a live window does.
pub fn preview_recording_base() -> Option<String> {
    PREVIEW_BASE.with(|slot| slot.borrow().clone())
}

/// Name the recording for the preview's frame search. Only the snapshot bench calls this.
pub fn set_preview_camera_base(base: &str) {
    PREVIEW_BASE.with(|slot| *slot.borrow_mut() = Some(base.to_string()));
}

/// What the preview last asked frames for — `(recording base, the second of THAT FILE)` — or `None` on a
/// page that has never refreshed with a recording to ask about. TEST DOOR ONLY: nothing in the program reads
/// it back, only F2.10 S3's widget test does. A watch is otherwise invisible from outside — the row list
/// repaints its outline and the status prints a sentence — and whether a real frame landed on the panel would
/// need Prepare's extraction, so in a headless container this ask is the observable half that exists.
pub fn preview_asked_for_test() -> Option<(String, f64)> {
    PREVIEW_ASKED.with(|slot| slot.borrow().clone())
}

/// Which frame belongs on the panel right now: the recording of the row the preview is watching (falling
/// back to the first recording of the session), at the second the red line stands on. Both halves are asked
/// of the doors that hold them, so moving the line or clicking another row changes the picture without this
/// module remembering anything.
pub fn refresh(window: &adw::ApplicationWindow) -> bool {
    // F2.12 S7 first: a card is not the session's own picture, so a line standing on one shows the card's
    // rendered frame and nothing else. Answers `None` for footage, which is every second there is a frame to
    // search for below.
    if let Some((frame, held)) = crate::ui::cut_card_preview::card_frame(
        window,
        &ui::window::newest_review_cut(),
        // The FRAME's second, not the clock's: a line standing on a stop keeps showing that stop's first frame
        // (§F3.3 S4), and a card cannot be re-timed anyway — this answers `t` unchanged when no stop covers it.
        crate::cut_speed_play::picture_second(
            &ui::window::newest_review_cut().fx,
            ui::line_position(window).t,
        ),
    ) {
        // The held sentence belongs to the frame it explains: say it while the card is on the panel, because
        // everywhere else on this page the picture moves when the clock does.
        if let (Some(line), Some(status)) = (held, ui::find_status(window.upcast_ref())) {
            status.set_text(&line);
        }
        return show_frame(window, Some(&frame));
    }
    // The same spans the ROW LIST colours (`row_spans`), so a watched row number means one thing on the page.
    // Asked of that list first and falling back to the session's own tape: a cut with no footage scene yet
    // colours nothing, and a declared take must still be the picture then (F2.10 S3 says nothing about
    // dropping it). One span per row, in row order, or the outline and the frame disagree about row 2.
    let colouring = crate::ui::cut_cameras::row_spans(&ui::window::newest_review_cut());
    let recordings = if colouring.is_empty() {
        ui::page_recordings()
    } else {
        colouring
    };
    if recordings.is_empty() {
        return show_frame(window, None);
    }
    // The watched row indexes the scene's rows; `scene_from` lays them in the recording order, so the row
    // number picks the recording rather than needing a second mapping.
    let row = ui::preview_row(window);
    // A snapshot names the session it is rendering (`snapshot.rs`), because its folder is the fixture and
    // not the working copy a real session writes frames into. Empty in every other case, so a live window
    // still answers from its own recordings.
    let named = preview_recording_base();
    let chosen: Option<&timeline::Recording> = match named {
        // A snapshot names the session by base alone, so find that base among the recordings for its start.
        Some(base) => recordings.iter().find(|recording| recording.base == base),
        None => recordings.get(row).or(recordings.first()),
    };
    let Some(recording) = chosen else {
        return show_frame(window, None);
    };
    let base = recording.base.clone();
    let Some(tree) = session_tree(window) else {
        return show_frame(window, None);
    };
    let paths = cut_tracks::frame_paths(&tree, &base);
    // The frames on disk carry the FILE's own stamps, so the line's session second has to be converted
    // before it can pick one — otherwise a recording that does not start at 0:00, or one the hand slid,
    // shows a frame from the wrong part of the file (§B's `at(t)`). The SAME number is what
    // `preview_asked_for_test` reports: which recording the panel asked, at which second of THAT file — the
    // ask a watch provokes and the frame looked up cannot drift apart, because they are computed in one place.
    let at = timeline::local_at(
        // Same rule one door down: under a stop the panel shows the held frame, so that is the second asked for.
        // With no stop covering the line `picture_second` answers `t` unchanged, which is what
        // `preview_asked_for_test` has always reported.
        crate::cut_speed_play::picture_second(
            &ui::window::newest_review_cut().fx,
            ui::line_position(window).t,
        ),
        recording.start,
        0.0,
        timeline::shift_of(&ui::window::newest_review_cut(), &base),
    );
    PREVIEW_ASKED.with(|slot| *slot.borrow_mut() = Some((base.clone(), at)));
    let found = cut_tracks::frame_near(&paths, at);
    // §9 (`With cameras of different shapes, both MUST use the first footage clip's frame`):
    // the panel used to measure its plate against whichever camera was playing while the
    // render measured against the first footage clip for the whole video. `source_frame` is
    // that one rule — the same call `produce_render::plan_clips` makes through `frame_box` —
    // and the shape it picks is what the plate and every overlay on it are now fitted to.
    let agreed = crate::effect_overlap::source_frame(
        first_footage_shape(window),
        shown_shape(found.as_deref()),
    );
    set_plate_shape(window, agreed);
    // §8 (`unrenderable still ">>> the stop frame at m:ss cannot be shown in the preview:
    // <err>"`): a stop holds a frame, and when the search above finds none the preview owes
    // that sentence — said, never enforced: the stop stays on the lane and the render may well
    // manage what this one-shot file search could not.
    crate::ui::cut_stop_frame::say_missing_still(
        window,
        &ui::window::newest_review_cut().fx,
        ui::line_position(window).t,
        found.is_some(),
    );
    show_frame(window, found.as_deref())
}

/// The folder this window is working on. `line_root` is the page's own per-window slot for its folder (the
/// red line's file lives beside it), which is the same folder the band lists its frames from — so there is
/// exactly one answer to "which project" in a window, and no second copy of that decision here.
/// A snapshot names its own folder: the bench renders a fixture, while `line_root` follows the working copy
/// the app opens, and those are different directories by design.
fn session_tree(window: &adw::ApplicationWindow) -> Option<layout::Tree> {
    if let Some(dir) = snapshot_tree() {
        return layout::Tree::new(&dir).ok();
    }
    let dir = ui::line_root(window)?;
    layout::Tree::new(&dir).ok()
}

/// Set by the snapshot bench only: the folder whose frames the preview may show. See
/// [`set_preview_camera_base`], which names the recording inside it.
fn snapshot_tree() -> Option<std::path::PathBuf> {
    SNAPSHOT_DIR.with(|slot| slot.borrow().clone())
}

/// Name the folder the preview searches for frames. Only the snapshot bench calls this.
pub fn set_snapshot_dir(dir: &std::path::Path) {
    SNAPSHOT_DIR.with(|slot| *slot.borrow_mut() = Some(dir.to_path_buf()));
}
/// Move THIS window's red line and repaint the frame that goes with it. `window.rs` owns where the line is
/// stored; this is the one line its owner has to add, so a placed line always changes what the preview shows
/// without the widget layer deciding anything about seconds.
pub fn note_line_moved(window: &adw::ApplicationWindow, at: f64) {
    ui::set_line_position(window, crate::cut_line::LinePos { t: at });
    refresh(window);
}

/// The click (§A: "click toggles play"). Forwards to the transport's OWN door with the session's filmed
/// stretches, which is what makes a preview with nothing filmed refuse for the reason F2.1 gives rather
/// than starting a blank player. Then the frame follows the line the press left the preview on.
/// §9's two MUST-agree seams, measured here rather than assumed:
/// the shape of the frame the panel is showing, and the shape of the session's FIRST
/// footage recording. Both sides of `source_frame` come from real files: a decoded pixbuf
/// for what is on screen, and Prepare's own extracted frames for the first footage row.
/// The render agrees through `produce_render::plan_clips`, which fits every clip to the
/// footage row's shape under aspect "source".
///
/// A frame file's own shape, or `None` when there is no frame or it will not decode.
fn shown_shape(path: Option<&std::path::Path>) -> Option<(i32, i32)> {
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file(path?).ok()?;
    let (w, h) = (pixbuf.width(), pixbuf.height());
    (w > 0 && h > 0).then_some((w, h))
}

/// The first footage recording's shape, read from the frames Prepare already extracted for
/// it — the same door `refresh` picks its picture through, so the two cannot disagree about
/// which recording is first. `None` with no tape or no frames yet, which leaves the playing
/// camera's shape standing ([`crate::effect_overlap::source_frame`]'s fallback).
fn first_footage_shape(window: &adw::ApplicationWindow) -> Option<(i32, i32)> {
    let _ = window;
    let recordings = ui::page_recordings();
    let first = recordings.first()?;
    let tree = session_tree(window)?;
    shown_shape(
        cut_tracks::frame_paths(&tree, &first.base)
            .first()
            .map(|path| path.as_path()),
    )
}

/// What the plate and every overlay on it are fitted to, as §9 says both sides must be.
/// Read by [`crate::ui::cut_preview_frame::fit_size_of`], the one place the preview turns
/// fractions into pixels, so a caption, a drawing and a zoom box all agree with the render.
pub fn plate_shape(window: &adw::ApplicationWindow) -> (i32, i32) {
    let _ = window;
    PLATE_SHAPE
        .with(|slot| *slot.borrow())
        .unwrap_or(FALLBACK_SHAPE)
}

/// The shape a preview panel falls back to with nothing decoded yet: the shipped declaration
/// the panel pins itself to, so an empty plate is still measured like a picture.
const FALLBACK_SHAPE: (i32, i32) = (
    crate::ui::window::PREVIEW_PANEL_W as i32,
    crate::ui::window::PREVIEW_PANEL_H as i32,
);

thread_local! {
    static PLATE_SHAPE: std::cell::RefCell<Option<(i32, i32)>> = const { std::cell::RefCell::new(None) };
}

/// Write what the plate is fitted to, from [`crate::effect_overlap::source_frame`].
fn set_plate_shape(window: &adw::ApplicationWindow, shape: (i32, i32)) {
    note_plate_shape(window, shape)
}

/// The same note for a caller that knows the plate's shape from a file rather than from the page's own search:
/// a snapshot staging its own frame (`snapshot_preview`) has the JPEG on disk and can say what it measures, instead
/// of leaving the plate on `FALLBACK_SHAPE` and letting one set of fractions mean two different boxes in px.
pub fn note_plate_shape(window: &adw::ApplicationWindow, shape: (i32, i32)) {
    let _ = window;
    PLATE_SHAPE.with(|slot| *slot.borrow_mut() = Some(shape));
}

pub fn wire_click(window: &adw::ApplicationWindow) {
    let Some(root) = window.content() else { return };
    let Some(button) = find(root.upcast_ref(), "preview-picture-button")
        .and_then(|widget| widget.downcast::<gtk::Button>().ok())
    else {
        return;
    };
    let win = window.clone();
    button.connect_clicked(move |_| {
        let runs = timeline::filmed_runs(&ui::page_recordings())
            .iter()
            .map(|(start, end)| (*start, *end))
            .collect::<Vec<_>>();
        let _said = ui::press_play_recording(&win, &runs);
        refresh(&win);
    });
}
