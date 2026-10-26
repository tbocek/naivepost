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
/// tape instead of a broken image. The decode is gdk-pixbuf's real decoder over a real file, run on a worker
/// ([`crate::ui::cut_picture_cache::picture`], §03-shell §1): a frame not decoded yet keeps the picture that
/// is up, and goes up itself when it lands ([`repaint_landed`]). So a line move never waits on a decode.
///
/// The request is remembered beside the frame ([`preview_asked_for_test`]) so a check can prove WHICH frame
/// the panel was given: a headless window cannot read a paintable back off a `Picture`.
pub fn show_frame(window: &adw::ApplicationWindow, path: Option<&std::path::Path>) -> bool {
    let Some(picture) = picture(window) else {
        return false;
    };
    let shown = match path {
        Some(file) => match crate::ui::cut_picture_cache::picture(file) {
            crate::ui::cut_picture_cache::Decoded::Ready(held) => Some(held.texture),
            crate::ui::cut_picture_cache::Decoded::Coming => {
                // Owed until it lands, so a snapshot of the page waits for the picture.
                owe_picture(&picture);
                return true;
            }
            crate::ui::cut_picture_cache::Decoded::Missing => None,
        },
        None => None,
    };
    PICTURE_OWED.with(|owed| owed.borrow_mut().take());
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

thread_local! {
    /// The picture owed while a listing or a decode is on a worker.
    static PICTURE_OWED: std::cell::RefCell<Option<crate::ui::settle::Owed>> =
        const { std::cell::RefCell::new(None) };
}

fn owe_picture(picture: &gtk::Picture) {
    PICTURE_OWED.with(|owed| {
        let mut owed = owed.borrow_mut();
        if owed.is_none() {
            *owed = Some(crate::ui::settle::owe("the Cut preview's picture", picture));
        }
    });
}

/// A listing, a decode or a card render landed (`cut_picture_cache`): put the wanted frame up. The built-in
/// player's picture is the player's own, so nothing is asked while it shows.
pub fn repaint_landed() {
    let Some(window) = crate::ui::window::built_window() else {
        return;
    };
    if video_shown() {
        return;
    }
    refresh(&window);
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
        release_video(window);
        // An empty path is a card whose frames a worker is still rendering: what is up stays up.
        if frame.as_os_str().is_empty() {
            return true;
        }
        return show_frame(window, Some(&frame));
    }
    // F2.1: once ▶ has played through the built-in player, the picture IS that player's (the prototype's way):
    // the line it stands on is sought in it, and the extracted frames come back only when it cannot follow.
    if video_shown()
        && crate::ui::cut_record_play::follow_in_video(window, ui::line_position(window).t)
    {
        return true;
    }
    release_video(window);
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
    // The preview's row (the watched one, or the one the scene's lens picks) is a ROW of the colouring, and a
    // row holds every file one camera filmed one after another. So the row narrows the recordings to that
    // camera's, and the line's second picks the one it falls in. Indexing the recording list by the row
    // number showed the first file of the session wherever the line stood.
    let row = ui::preview_row(window);
    let t = crate::cut_speed_play::picture_second(
        &ui::window::newest_review_cut().fx,
        ui::line_position(window).t,
    );
    // A snapshot names the session it is rendering (`snapshot.rs`), because its folder is the fixture and
    // not the working copy a real session writes frames into. Empty in every other case, so a live window
    // still answers from its own recordings.
    let named = preview_recording_base();
    let chosen: Option<&timeline::Recording> = match named {
        // A snapshot names the session by base alone, so find that base among the recordings for its start.
        Some(base) => recordings.iter().find(|recording| recording.base == base),
        None => recording_at(&recordings, row, t),
    };
    let Some(recording) = chosen else {
        return show_frame(window, None);
    };
    let base = recording.base.clone();
    let Some(tree) = session_tree(window) else {
        return show_frame(window, None);
    };
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
    // Listed on a worker (`cut_picture_cache`): until the first listing lands, the picture that is up stays up.
    let Some(paths) = crate::ui::cut_picture_cache::frames_or_coming(&tree, &base) else {
        if let Some(picture) = picture(window) {
            owe_picture(&picture);
        }
        return true;
    };
    // Frames are named on the file's own clock: its start instant plus the frame's second in the file
    // (`frames::frame_file(start, at)`), and the first frame of every folder is second 0. So that frame's name
    // IS the start, and `at` is added to it before the names are compared. Compared bare, a second like 115
    // named a 1970 instant before every frame, and the panel showed the file's first frame wherever the line was.
    let zero = paths
        .first()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .and_then(crate::clock::read_frame)
        .unwrap_or(0.0);
    let found = cut_tracks::frame_near(&paths, zero + at);
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

thread_local! {
    /// The preview's picture shows the built-in player's paintable (F2.1), not an extracted frame.
    static VIDEO_SHOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Put the built-in player's frames on this window's picture: from here on the picture is the player's, until
/// [`release_video`] hands it back to the extracted frames.
pub fn show_video(window: &adw::ApplicationWindow) {
    let (Some(picture), Some(paintable)) = (picture(window), crate::play_built_in::paintable())
    else {
        return;
    };
    if let Ok(paintable) = paintable.downcast::<gtk::gdk::Paintable>() {
        picture.set_paintable(Some(&paintable));
        VIDEO_SHOWN.with(|cell| cell.set(true));
    }
}

/// Is the picture the built-in player's right now?
pub fn video_shown() -> bool {
    VIDEO_SHOWN.with(|cell| cell.get())
}

/// Give the picture back to the extracted frames and let the player's file go.
fn release_video(window: &adw::ApplicationWindow) {
    let _ = window;
    if VIDEO_SHOWN.with(|cell| cell.replace(false)) {
        crate::play_built_in::stop();
    }
}

/// The recording the picture shows at session second `t` (the preview's row, as [`refresh`] chooses it) and that
/// second on the recording's own clock.
pub fn picture_source(
    window: &adw::ApplicationWindow,
    t: f64,
) -> Option<(timeline::Recording, f64)> {
    let cut_ = ui::window::newest_review_cut();
    let colouring = crate::ui::cut_cameras::row_spans(&cut_);
    let recordings = if colouring.is_empty() {
        ui::page_recordings()
    } else {
        colouring
    };
    let recording = recording_at(&recordings, ui::preview_row(window), t)?.clone();
    let local = timeline::local_at(
        t,
        recording.start,
        0.0,
        timeline::shift_of(&cut_, &recording.base),
    );
    Some((recording, local))
}

/// The recording the preview shows at session second `t` on `row`: among the recordings the colouring puts on
/// that row (all of them when the row holds none), the one whose span holds `t`, else the nearest one.
pub fn recording_at(
    recordings: &[timeline::Recording],
    row: usize,
    t: f64,
) -> Option<&timeline::Recording> {
    let rows = timeline::rows_for(recordings, &ui::window::newest_review_cut());
    let on_row: Vec<&timeline::Recording> = recordings
        .iter()
        .zip(rows)
        .filter(|(_, at)| *at == row)
        .map(|(recording, _)| recording)
        .collect();
    let pool: Vec<&timeline::Recording> = if on_row.is_empty() {
        recordings.iter().collect()
    } else {
        on_row
    };
    let distance = |r: &timeline::Recording| {
        if t < r.start {
            r.start - t
        } else if t > r.end {
            t - r.end
        } else {
            0.0
        }
    };
    pool.into_iter().min_by(|a, b| {
        distance(a)
            .partial_cmp(&distance(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
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

/// The folder whose extracted frames THIS window's band should list, if the bench has named one. Same door as
/// [`set_snapshot_dir`]'s slot, read out: a live page lists frames from the project it opened (`session_reads`),
/// but a headless shot renders a FIXTURE in another directory, and without this the picture row would be asked
/// about the working copy rather than the shot's own — i.e. no thumbnails at all.
///
/// The folder the shot was handed IS a project folder (`.naivepost`), and its extracted frames live under its own
/// `prepare/inputs/frames/…`, so that is tried first; a caller that handed over the inner `session.naivepost`
/// instead still gets its frames through the fallback.
pub fn snapshot_frame_tree() -> Option<layout::Tree> {
    snapshot_tree().and_then(|dir| {
        layout::Tree::new(&dir)
            .ok()
            .or_else(|| layout::Tree::new(crate::startup::session_dir(&dir)).ok())
    })
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
///
/// Read off the decoded frame the cache holds, else off the file's header (a few kilobytes, never a decode),
/// remembered per file: a line move asks this every time.
fn shown_shape(path: Option<&std::path::Path>) -> Option<(i32, i32)> {
    let path = path?;
    if let Some(shape) = crate::ui::cut_picture_cache::held_shape(path) {
        return Some(shape);
    }
    if let Some(shape) = SHAPES.with(|shapes| shapes.borrow().get(path).copied()) {
        return shape;
    }
    let shape = gtk::gdk_pixbuf::Pixbuf::file_info(path)
        .map(|(_, w, h)| (w, h))
        .filter(|(w, h)| *w > 0 && *h > 0);
    SHAPES.with(|shapes| shapes.borrow_mut().insert(path.to_path_buf(), shape));
    shape
}

thread_local! {
    /// The header shapes [`shown_shape`] read, per file.
    static SHAPES: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, Option<(i32, i32)>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
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
        crate::ui::cut_picture_cache::frames_of(&tree, &first.base)
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
    // What the picture cache lands (a listing, a decode, a card's frames) goes up through this page's repaint.
    crate::ui::cut_picture_cache::on_landed(repaint_landed);
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
