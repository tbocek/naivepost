//! §F2.8 — trim and move on the track strip, the copy/paste hand, and the lane press that places a line.
//! Split out of `window.rs`, which is past its size budget; its doors are re-exported through `window`.
//!
//! The rules are `cut_trim`'s, `cut_copy`'s and `cut_line`'s; what is here is the per-window state a drag
//! leaves behind, the strip it repaints, and the sentence it prints.

use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_copy;
use crate::cut_line;
use crate::cut_play_leg;
use crate::cut_screen;
use crate::cut_select;
use crate::cut_trim;
use crate::ui::cut_colours::{
    GROUND, INSERT_VIOLET, KEPT_BORDER, KEPT_TINT, KEPT_TINT_ALPHA, RULER_INK,
};
use crate::ui::window as ui;

/// The trim drag in flight, as [`wire_track_strip`] holds it between the press and the release.
struct TrimDrag {
    /// The segment whose border is being dragged — the index into this window's cut.
    index: usize,
    /// Which of its two borders the press took.
    border: cut_trim::Border,
    /// When the last scrub happened, for `cut_trim::scrubs`' throttle.
    scrubbed_ms: Option<u64>,
    /// Whether THIS hold has spent its one Undo step yet (§7: one Undo per drag). False at `drag-begin`, true
    /// from the first move that pushed; every later update of the same hold writes state without pushing.
    pushed: bool,
}

/// This window's ⌦ controller, so a test can see it is attached. `None` on a page that drew none.
pub fn delete_key_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    DELETE_KEYS.with(|cell| cell.borrow().clone())
}
// --- F2.8: trim and move on the track strip -------------------------------------------------------------

/// Pixels-per-second for the placeholder strip — the same zoom `select-surface` runs at, so a pixel the
/// hand travels means the same number of seconds on both strips.
pub const TRACK_STRIP_PPS: f64 = cut_screen::ZOOM_AT_OPEN;

pub use crate::ui::cut_view::strip_pps;
/// This window's `track-strip` (F2.8), by its stable name.
pub fn track_strip(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    ui::find_widget_by_name(window.upcast_ref(), "track-strip")?
        .downcast()
        .ok()
}
/// The left-button drag that trims a border, so a test can fire it the way GTK does.
pub fn trim_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    TRIM_GESTURES.with(|cell| cell.borrow().clone())
}
/// The right-button drag that moves. Same reason for existing as [`trim_gesture`].
pub fn move_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    MOVE_GESTURES.with(|cell| cell.borrow().clone())
}
/// One window's move-drag press point, newest window first (`MOVE_PRESSED` below).
type MovePressed = std::cell::RefCell<Vec<Rc<std::cell::RefCell<(f64, f64)>>>>;

thread_local! {
    /// The trim drag in flight: which clip's which border the press took, and when it last scrubbed.
    /// `None` means the press fell where no border was in reach, so the drag does nothing at all.
    static TRIM_DRAGS: std::cell::RefCell<Option<TrimDrag>> =
        const { std::cell::RefCell::new(None) };
    static TRIM_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
    static MOVE_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
    /// F2.8 S2: where the move drag was PRESSED, (x, y) in strip px, newest window first. The scope has to
    /// be fixed at the press — `wave_strip` and `on_border` are read from this, not re-derived on each
    /// update, so a pointer wandering off the band mid-drag cannot change what was picked up.
    static MOVE_PRESSED: MovePressed =
        const { std::cell::RefCell::new(Vec::new()) };
    // F2.8 S2: the folds opened for the drag in flight live in `cut_trim_press` now — that module opens
    // them for the drag and refolds them on release, so nothing is held here.
}
/// What the right button was pressed on, where the page knows it: a recording on the recorders' band, or
/// the scene under the pointer. Both are seams standing in until the real track rounds (F2.10/F2.11) set
/// them from an actual press position, exactly as F2.7's held-effect/held-clip seams do.
pub fn set_press_band(recording: Option<String>) {
    PRESS_BAND.with(|cell| *cell.borrow_mut() = recording);
}
/// Read side of [`set_press_band`].
pub fn press_band() -> Option<String> {
    PRESS_BAND.with(|cell| cell.borrow().clone())
}
/// The scene index a press fell on, if it fell on one.
pub fn set_press_scene(scene: Option<usize>) {
    PRESS_SCENE.with(|cell| *cell.borrow_mut() = scene);
}
/// Read side of [`set_press_scene`].
pub fn press_scene() -> Option<usize> {
    PRESS_SCENE.with(|cell| *cell.borrow())
}
thread_local! {
    static PRESS_BAND: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
    static PRESS_SCENE: std::cell::RefCell<Option<usize>> =
        const { std::cell::RefCell::new(None) };
}
/// F2.8 S1: drag one clip's border to `to` (session seconds). [`cut_trim::clamp_edge`] decides where it
/// may go — a scene's worth of length, the next clip, the recording's end — this writes that answer into
/// the window's cut and prints the sentence it earned. No rule lives here; the write itself is
/// [`cut_trim_press::press_border_move`], which is also what the strip's own drag calls with the hold's
/// "is this the first move" flag beside it.
pub fn press_trim_border(
    window: &adw::ApplicationWindow,
    index: usize,
    border: cut_trim::Border,
    to: f64,
) -> f64 {
    // A call that stands on its own is a whole gesture, so it spends the step it earned.
    crate::ui::cut_trim_press::press_border_move(window, index, border, to, false)
}
/// track: the real bounds come with the tracks (F2.10/F2.11), and until then a trim needs *some* wall to
/// stop at rather than none, which would let a clip be dragged off the end of a session.
pub const TRACK_REC_END: f64 = 3600.0;

/// F2.8 S1: where a clip at `s..e` may reach, `(start, end)`: the recording it stands in, once the session's takes
/// are known. Before they are, the wall is the tape's end, and a clip already at or past that has none: held at [`TRACK_REC_END`] alone, every clip past the first hour had its end clamped to the 1 s minimum the moment
/// a border was taken, whichever way the hand moved.
pub fn clip_wall(s: f64, e: f64) -> (f64, f64) {
    let recordings = crate::ui::cut_folds::page_recordings();
    let middle = (s + e) / 2.0;
    if crate::ui::cut_folds::session_declared() {
        if let Some(take) = recordings
            .iter()
            .find(|take| take.start <= middle && middle <= take.end)
        {
            return (take.start, take.end);
        }
    }
    let wall = session_wall();
    (0.0, if e < wall { wall } else { f64::MAX })
}

/// F2.8 S2: where a slide may reach: the tape's end, or [`TRACK_REC_END`] on a shorter tape. A slide can carry a
/// selection across recordings, so no one recording fences it.
pub fn session_wall() -> f64 {
    crate::ui::cut_folds::page_recordings()
        .iter()
        .map(|take| take.end)
        .fold(TRACK_REC_END, f64::max)
}

/// F2.8 S2/S3: a right-drag moved. [`cut_trim::right_gesture`] answers the whole gesture; this applies
/// the answer — the shift map, the row change, the folds to open — and prints the sentence.
///
/// S3 holds here absolutely: nothing on this path touches the red line. `cut_trim::right_press_moves_line()`
/// is `false`, and there is deliberately no `set_playhead` / `ui::move_line_and_save` call anywhere in this
/// function or in the button-3 gesture that reaches it, so the page cannot drift into moving the line
/// while the right hand is doing arithmetic on the clock.
/// F2.8 S2: the boxes `track-strip` paints for this window's cut, in the same px space a press arrives in.
/// Read from the newest review cut so a press and a paint never disagree about where a clip is.
pub(crate) fn trim_boxes_for_strip() -> Vec<cut_trim::Box_> {
    let cut_ = ui::newest_review_cut();
    // Deliberately UNFOLDED: `clip_boxes` multiplies the scene's own seconds by the zoom and never reads
    // `cut.folds`, and that is the rule, not an oversight (§B: "a fold is a view, never used to measure a
    // drag"). A gesture measured through a seam would move a border to a second the person never touched,
    // because the pixels they grabbed are on the far side of a stretch that is only hidden. The band PAINTS
    // folded (`cut_tracks::band_x`); the hand measures unfolded.
    cut_trim::clip_boxes(&cut_.segs, strip_pps())
}
/// F2.8 S1/S2: what a right-press at `press_x` takes. A border grabbed within [`cut_trim::EDGE_GRAB_PX`]
/// of a DRAWN box belongs to S1 ("either button drags that border") and must be trimmed, not slid — but
/// only once the hand has actually travelled: an unmoved press stays a click (S3), which trims nothing.
/// Anything else falls through to the move rule.
fn right_press_takes_border(press_x: f64, travel_px: f64) -> Option<(usize, cut_trim::Border)> {
    if travel_px.abs() < cut_select::DRAG_SLOP_PX {
        return None;
    }
    cut_trim::border_at(&trim_boxes_for_strip(), press_x)
}
/// F2.8 S2: where this window's move press landed in x, in strip px. The widget path stores it at
/// `drag-begin`; until F2.10/F2.11 own the real rows a caller may also prime it directly (the same kind of
/// seam `set_press_scene` is), which is how a test places a press mid-scene rather than on a border.
pub fn set_press_x(x: f64) {
    // Read the old pair first and drop that borrow before writing: holding it across the second
    // `borrow_mut` on the same RefCell panics with "RefCell already borrowed".
    let old = MOVE_PRESSED.with(|cell| cell.borrow().last().map(|s| *s.borrow()));
    if let Some((_, y)) = old {
        MOVE_PRESSED.with(|cell| {
            if let Some(slot) = cell.borrow().last() {
                *slot.borrow_mut() = (x, y);
            }
        });
    }
}
/// Read side of [`set_press_x`] — the stored press position for the newest window.
pub fn press_position() -> (f64, f64) {
    MOVE_PRESSED.with(|cell| {
        cell.borrow()
            .last()
            .map(|s| *s.borrow())
            .unwrap_or((0.0, 0.0))
    })
}
/// F2.8 S2/S3: the older door, seam-only. Kept for callers (and tests) that supply no press position: it
/// answers with whatever `set_press_band` primed and nothing else — no band, no border — so a call here can
/// still never move the line without travel. The widget path goes through [`press_move_at`] instead.
pub fn press_move(
    window: &adw::ApplicationWindow,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
) -> cut_trim::Gesture {
    crate::ui::cut_trim_press::press_move_with_override(
        window,
        (
            press_band(),
            None,
            false,
            None,
            press_position().0 / strip_pps(),
        ),
        travel_px,
        row_change,
        d_seconds,
    )
}
/// F2.8 S2: what a right-press at (`press_x`, `press_y`) took, exactly as the widget computes it — the
/// same door, with the answer returned instead of applied. Kept separate from [`press_move_at`] so a test
/// can assert WHICH branch the wire chose without trusting the status sentence alone.
///
/// The recording NAME comes from `set_press_band` (until F2.10/F2.11 own the lane names); WHICH band was
/// hit comes from the press y through `cut_trim::press_targets` over `cut_trim::PLACEHOLDER_STRIP_BANDS`.
/// That is what makes the flowchart's wave-strip branch reachable from a real right-press with nothing
/// primed but the name.
pub fn right_press_took(press_x: f64, press_y: f64) -> (Option<String>, Option<String>, bool) {
    let boxes = trim_boxes_for_strip();
    let seam = press_band();
    crate::ui::cut_trim_press::press_targets_at(
        &ui::newest_review_cut(),
        press_x,
        press_y,
        &boxes,
        strip_pps(),
        seam.as_deref(),
    )
}
/// F2.8 S2: the move door that knows WHERE the press landed. Same rule as [`press_move`], but its
/// `recorders_band` / `wave_strip` / `on_border` come from the press position through
/// [`right_press_took`], not from literals. `press_scene`/`ui::press_row` remain overrides for the scene/row
/// questions; those seams are all that is left standing in, because the page has no real rows to read them
/// off yet.
pub fn press_move_at(
    window: &adw::ApplicationWindow,
    press_x: f64,
    press_y: f64,
    travel_px: f64,
    d_seconds: f64,
) -> cut_trim::Gesture {
    let (recorders_band, wave_strip, on_border) = right_press_took(press_x, press_y);
    let gesture = crate::ui::cut_trim_press::press_move_with_override(
        window,
        (
            recorders_band,
            wave_strip,
            on_border,
            None,
            press_x / strip_pps(),
        ),
        travel_px,
        false,
        d_seconds,
    );
    // S3 lives HERE, at the press: a right press of this widget that never travelled is a click, and a
    // click's only answer is the status line saying `this one`.
    if gesture == cut_trim::Gesture::Click && travel_px.abs() < cut_trim::ROW_TRAVEL_PX {
        crate::ui::cut_trim_press::press_unmoved(window);
    }
    gesture
}
/// The row a right-press would land on, and the row a zoom records itself as framed on (§1's new `cam` column).
/// The page's own answer is F2.4's watched row — the one a picture-band click left behind — because that is the
/// only place the person ever says "this camera"; with nothing watched the strip sits on row 0, and a row number
/// past the rows this cut has costs nothing but a reframe nobody can see, so it is clamped back.
pub fn press_row(_window: &adw::ApplicationWindow) -> usize {
    let Some(row) = ui::watched_row() else {
        return 0;
    };
    let rows = crate::timeline::rows_for(
        &crate::timeline::kept_footage_recordings(&ui::newest_review_cut()),
        &ui::newest_review_cut(),
    )
    .into_iter()
    .max()
    .map_or(1, |last| last + 1);
    row.min(rows - 1)
}
/// The folds this window's drag in flight asked to open, newest write last. Read by the fold surface when
/// it exists; exposed now so a test can assert the rule's answer reached the page.
pub fn open_folds(_window: &adw::ApplicationWindow) -> Vec<usize> {
    crate::ui::cut_trim_press::open_folds()
}
thread_local! {
    /// The copy in hand (F2.9 S1), newest window first's rule not needed: one copy app-wide, because a
    /// second one would leave the person unsure which ⧉ Paste answers.
    static COPY_HAND: std::cell::RefCell<Option<cut_copy::Hand>> =
        const { std::cell::RefCell::new(None) };
    static COPY_ESC_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}
/// What is in hand right now — the read side of [`press_copy`], so a test can assert the hand without
/// reaching into thread-locals.
pub fn copy_hand() -> Option<cut_copy::Hand> {
    COPY_HAND.with(|cell| cell.borrow().clone())
}
/// This window's Esc controller (F2.9 S2), so a test can see it is attached. `None` on a page that drew none.
pub fn copy_esc_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    COPY_ESC_KEYS.with(|cell| cell.borrow().clone())
}
/// F2.9 S1: ⧉ Copy. [`cut_copy::copy`] decides everything — whether there is a band, whether it is a
/// second long, what it is a stretch of; this only reads the band, keeps the answer if it took one, and
/// hands back the sentence to print. A refusal leaves no hand, so ⧉ Paste stays greyed.
pub fn press_copy(window: &adw::ApplicationWindow) -> String {
    // Split-then-take-the-second-half in one gesture (F2.7 S2): a | Split at the red line put that half in
    // the verb hand precisely because selecting it again would be a second gesture for something the split
    // already defined. With a band up, ⧉ Copy answers the band as always — the split's half is only asked
    // for when there is nothing to copy, so no existing Copy behaviour changes.
    if ui::selection(window).is_none() {
        if let Some(half) = crate::ui::cut_verb_press::verb_hand() {
            let taken = cut_copy::copy(Some(&half));
            if let cut_copy::Take::Taken(hand) = &taken {
                COPY_HAND.with(|cell| *cell.borrow_mut() = Some(hand.clone()));
                // The hand is spent: taking it twice would be two pastes of one split.
                crate::ui::cut_verb_press::take_into_hand(None);
                return cut_copy::copied_status(hand);
            }
        }
    }
    let taken = cut_copy::copy(ui::selection(window).as_ref());
    match &taken {
        cut_copy::Take::Taken(hand) => {
            COPY_HAND.with(|cell| *cell.borrow_mut() = Some(hand.clone()));
            cut_copy::copied_status(hand)
        }
        cut_copy::Take::TooShort(say) => say.clone(),
        cut_copy::Take::NothingSelected(say) => (*say).to_string(),
    }
}
pub fn paste_line(window: &adw::ApplicationWindow) -> f64 {
    crate::ui::cut_lane::paste_line(window)
}
/// F2.9 S2: ⧉ Paste at the red line. [`cut_copy::paste`] decides footage-vs-sound, splices or lays, and
/// refuses with its own sentence; this supplies the three things only the page knows — the line, the
/// copied recording's project path, and the file second the span starts at — then writes the mutated cut
/// back and mirrors the hand so a refusal keeps the copy.
pub fn press_paste(window: &adw::ApplicationWindow) -> String {
    let at = paste_line(window);
    let mut hand = copy_hand();
    // The copied recording's path: a sound names itself by base, and until F2.10/F2.11 own the row/path
    // mapping the base IS the project-relative path the cut uses, so an unmatched name passes through
    // unchanged rather than inventing one.
    let sources = ui::session_sources(window);
    let path = match hand.as_ref().and_then(|h| h.recording_public()) {
        Some(recording) => sources
            .iter()
            .find(|source| crate::ui::cut_lane::source_base(source) == recording)
            .cloned()
            .unwrap_or_else(|| recording.to_string()),
        None => String::new(),
    };
    // Session→file seconds for the copied span — see [`cut_copy::file_seconds`].
    let file_seconds = hand
        .as_ref()
        .map(|h| {
            cut_copy::file_seconds(
                &ui::newest_review_cut(),
                &crate::ui::cut_folds::page_recordings(),
                h,
            )
        })
        .unwrap_or(0.0);
    let mut cut_ = ui::newest_review_cut();
    let outcome = cut_copy::paste(
        &mut cut_,
        &mut hand,
        Some(at),
        &path,
        file_seconds,
        &sources,
    );
    // Only a paste that landed writes: a refusal left the cut untouched.
    if outcome.is_ok() {
        ui::save_insert_cut(&cut_);
    }
    ui::record_edit(window, &cut_);
    COPY_HAND.with(|cell| *cell.borrow_mut() = hand);
    match outcome {
        Ok(status) => status,
        Err(refusal) => refusal,
    }
}
/// F2.9 S3: ⇲ Lane. [`cut_copy::lane_start`] says whether the line may open a row there and
/// [`cut_copy::lane`] builds it; this names it and files it. NOTHING is cut to the new row — no `Seg` is
/// added here, because a lane that arrived already green would be a cut nobody made; ＋ Add lays scenes on
/// it later. Name: `Copied`, then `Copied-2`, `Copied-3`… past every row already taken, since
/// `Lane::name` keys `Cut::rows` and every scene's `quiet` list.
/// The ⇲ button's door: [`press_lane`], except that with nothing in hand the footage file is probed on a worker
/// (§03-shell §1: ffprobe can take half a second) and the row is filed when the length lands.
pub fn press_lane_from_button(window: &adw::ApplicationWindow) -> String {
    if copy_hand().is_some() {
        return press_lane(window);
    }
    crate::ui::cut_lane::lane_for_footage_file_on_worker(window)
}

pub fn press_lane(window: &adw::ApplicationWindow) -> String {
    let Some(hand) = copy_hand() else {
        // §B's other half: nothing in hand, so a footage FILE gets the row — see [`crate::ui::cut_lane`].
        return crate::ui::cut_lane::lane_for_footage_file(window);
    };
    let at = paste_line(window);
    let mut cut_ = ui::newest_review_cut();
    // The session's FILMED runs — what `lane_start`'s "nothing is rolling at m:ss" asks. Where no session was
    // resolved the page's list already IS the kept clips' spans (`session_or_kept`), so this union only ever
    // adds a real project's tape: a take that keeps running past the last kept clip leaves a line out there
    // still rolling, which is the answer §B wants.
    let mut spans = crate::ui::cut_folds::page_recordings();
    spans.extend(crate::timeline::kept_footage_recordings(&cut_));
    let filmed: Vec<(f64, f64)> = crate::timeline::filmed_runs(&spans);
    match cut_copy::lane_start(Some(at), &hand, &filmed) {
        cut_copy::LaneStart::Refusal(say) => say,
        cut_copy::LaneStart::Start(at) => {
            let taken: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
            let name = cut_copy::lane_name("Copied", &taken);
            let source = hand
                .source_public()
                .map(|src| src.to_string())
                .unwrap_or_else(|| {
                    ui::session_sources(window)
                        .first()
                        .cloned()
                        .unwrap_or_default()
                });
            // The FILE second the row windows from — the same conversion the paste asks for.
            let file_seconds =
                cut_copy::file_seconds(&cut_, &crate::ui::cut_folds::page_recordings(), &hand);
            let Some(lane) = cut_copy::lane(&hand, &source, file_seconds, at, name.clone()) else {
                return cut_copy::too_short().to_string();
            };
            cut_.lanes.push(lane);
            // A lane is a fact about the project, so it goes in the file (with its `off`).
            ui::save_insert_cut(&cut_);
            ui::record_edit(window, &cut_);
            cut_copy::lane_status(hand.length, hand.from, &name, at)
        }
    }
}
/// Record whether a second has been placed. `false` means "no target": it clears the flag AND drops the
/// line to zero, because a page that says it has no target while still showing one would contradict itself.
/// `true` only claims the target — where it points is whatever `ui::set_line_position` last stored.
pub fn note_place(placed: bool) {
    ui::INSERT_PLACE_KNOWN.with(|cell| *cell.borrow_mut() = Some(placed));
    if !placed {
        ui::LINE_STATES.with(|slots| {
            if let Some(slot) = slots.borrow().last() {
                slot.borrow_mut().0 = cut_line::LinePos { t: 0.0 };
            }
        });
    }
}
/// S2: Esc drops the copy. Claims ONLY Escape — every other key returns `Proceed`, so typing in an entry/// field is untouched, exactly as the ⌦ controller does for its two keys.
pub(crate) fn wire_copy_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key == gtk::gdk::Key::Escape {
            COPY_HAND.with(|cell| *cell.borrow_mut() = None);
            if let Some(status_line) = ui::find_status(win.upcast_ref()) {
                status_line.set_text(cut_copy::DROPPED);
            }
            refresh_copy_buttons(&win);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller.clone());
    COPY_ESC_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}
/// Whether another Esc owner has something to drop: a copy, a held effect, an arm or an open form. Those
/// controllers answer Esc themselves; the hand's layer stands back for them, whichever order GTK asks the
/// window's controllers in.
fn esc_belongs_elsewhere(window: &adw::ApplicationWindow) -> bool {
    copy_hand().is_some()
        || ui::held_effect().is_some()
        || ui::zoom_armed()
        || ui::zoom_form_open().is_some()
        || crate::ui::cut_text_hand::text_armed()
        || crate::ui::cut_text_hand::text_form_open().is_some()
        || crate::ui::cut_svg::svg_armed()
        || crate::ui::cut_svg::svg_form_open().is_some()
        || crate::ui::cut_speed_hand::speed_form_open().is_some()
        || crate::ui::cut_volume_hand::volume_form_open().is_some()
        || crate::ui::cut_label_hand::label_form_open().is_some()
        || crate::ui::window_insert::insert_open(window).is_some()
}

/// The last Esc layer: with no copy, effect, arm or form left to drop, Esc puts down the clip or edge in
/// hand, and with nothing in hand it clears the selection; one layer per press, so a person who held a clip
/// inside a selection gets the selection back on the second Esc rather than losing both to one. Proceeds with
/// nothing to drop, and while another Esc owner has something ([`esc_belongs_elsewhere`]).
pub(crate) fn wire_hand_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape || esc_belongs_elsewhere(&win) {
            return glib::Propagation::Proceed;
        }
        let said = match ui::held_now() {
            Some(cut_line::Held::Edge) => HAND_EDGE_DOWN,
            Some(cut_line::Held::Clip) => HAND_CLIP_DOWN,
            _ if ui::clear_selection(&win) => SELECTION_DROPPED,
            _ => return glib::Propagation::Proceed,
        };
        if said != SELECTION_DROPPED {
            crate::ui::cut_hand::clear_all();
        }
        if let Some(status_line) = ui::find_status(win.upcast_ref()) {
            status_line.set_text(said);
        }
        glib::Propagation::Stop
    });
    window.add_controller(controller);
}
/// What Esc says when it puts down a held edge.
pub const HAND_EDGE_DOWN: &str = "the edge in hand is put down";
/// What Esc says when it puts down a held clip.
pub const HAND_CLIP_DOWN: &str = "the clip in hand is put down";
/// What Esc says when it clears the selection.
pub const SELECTION_DROPPED: &str = "the selection is cleared";
/// F2.9: set the three copy buttons from live ui::state. Copy is live with a band a second or longer; Paste
/// and Lane only while a hand is held. Runs from `refresh_selection_readout`, which every draw, nudge and
/// clear already calls, so a greyed button is always today's answer.
pub(crate) fn refresh_copy_buttons(window: &adw::ApplicationWindow) {
    let copy_live = ui::selection(window)
        .map(|band| band.length() >= cut_select::MIN_SCENE_SECONDS)
        .unwrap_or(false);
    let held = copy_hand().is_some();
    if let Some(button) = ui::line_step_button(window, "copy-button") {
        button.set_sensitive(copy_live);
    }
    // Paste needs a hand (F2.9 S3); Lane also lives with footage to put on a row of its own (§B).
    for name in ["paste-button", "lane-button"] {
        let live = held
            || (name == "lane-button"
                && crate::ui::cut_lane::footage_source_path(window).is_some());
        if let Some(button) = ui::line_step_button(window, name) {
            button.set_sensitive(live);
        }
    }
}
/// Paint `track-strip` from this window's own cut. The callback reads ui::state and draws it: every mark comes
/// from `cut_trim::ruler_ticks`, `cut_trim::clip_boxes` and `cut_trim::border_positions` at
/// [`TRACK_STRIP_PPS`], so the border the eye sees is the same pixel the hand grabs. An empty cut draws
/// the frame plus [`cut_trim::STRIP_EMPTY_HINT`] rather than a blank box, which would read as broken.
pub fn paint_track_strip(cr: &cairo::Context, width: i32, height: i32) {
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;
    let cut_ = ui::newest_review_cut();

    // Ground first, so every mark after it sits on something.
    cr.set_source_rgb(GROUND.0, GROUND.1, GROUND.2);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    // The ruler row: whole-second ticks with their labels, then the kept bar under them. §7's translate: every
    // tape x slides by `cut_strip_view::scroll_px`, so scrolling moves the ruler with the clips
    // instead of leaving it pinned to the widget.
    let ticks = cut_trim::ruler_ticks(TRACK_REC_END, strip_pps(), ui::RULER_EVERY_SECONDS);
    let _ = cr.save();
    cr.translate(-crate::ui::cut_strip_view::scroll_px(), 0.0);
    cr.set_source_rgb(RULER_INK.0, RULER_INK.1, RULER_INK.2);
    for (x, _) in &ticks {
        cr.rectangle(*x, 0.0, 1.0, cut_trim::RULER_H);
        let _ = cr.fill();
    }
    draw_strip_labels(cr, &ticks, w);
    let _ = cr.restore();

    let boxes = cut_trim::clip_boxes(&cut_.segs, strip_pps());
    let top = cut_trim::RULER_H;
    // F2.2 S6: the removed stretches inside filmed material are painted dim BEFORE the kept boxes go
    // on top, so a clip always wins over the dim and only the gaps show it. Dimmed whether or not ▶✂
    // is running: the kept bar already shows only kept material, so the gap IS the removed stretch.
    // §7's translate again: the dim spans are tape seconds too, so they slide with everything else — painted
    // through cairo's own translate rather than by editing a shared painter that the PAINTED band (which
    // translates its whole context already) also calls.
    let _ = cr.save();
    cr.translate(-crate::ui::cut_strip_view::scroll_px(), 0.0);
    cut_play_leg::paint_dimmed(
        cr,
        &cut_play_leg::dimmed(
            &cut_,
            &crate::timeline::filmed_runs(crate::ui::cut_folds::page_recordings().as_slice()),
        ),
        cut_trim::RULER_H,
        cut_trim::BAR_H,
        strip_pps(),
    );
    let _ = cr.restore();
    if boxes.is_empty() {
        cr.select_font_face(
            "Sans",
            cairo::FontSlant::Normal,
            gtk::cairo::FontWeight::Normal,
        );
        cr.set_font_size(11.0);
        cr.set_source_rgb(0.45, 0.45, 0.45);
        cr.move_to(6.0, cut_trim::RULER_H + cut_trim::BAR_H * 0.6);
        let _ = cr.show_text(cut_trim::STRIP_EMPTY_HINT);
        crate::ui::cut_trim_press::paint_bands(
            cr,
            w,
            &cut_,
            &crate::ui::cut_folds::page_recordings(),
            strip_pps(),
        );
        crate::ui::cut_trim_press::paint_laid_pieces(cr, w, &cut_, strip_pps());
        return;
    }

    // §7's translate again: a box fully scrolled past either edge draws nothing, one running off the right
    // edge draws its visible part (`visible` clips), which is what makes the strip a window rather than a
    // picture that gets cut off.
    let _ = cr.save();
    cr.translate(-crate::ui::cut_strip_view::scroll_px(), 0.0);
    for b in &boxes {
        let x = b.x;
        // Violet for an insert, the kept green for footage — the two colours the inventory lists, told
        // apart because only one of them may be trimmed.
        // Violet for an insert, the kept green for footage — the two tints `cut_colours` holds and
        // `params::cut()` catalogues, told apart because only one of them may be trimmed.
        if b.insert {
            cr.set_source_rgba(INSERT_VIOLET.0, INSERT_VIOLET.1, INSERT_VIOLET.2, 0.55);
        } else {
            cr.set_source_rgba(KEPT_TINT.0, KEPT_TINT.1, KEPT_TINT.2, KEPT_TINT_ALPHA);
        }
        cr.rectangle(x, top, b.w.max(1.0), cut_trim::BAR_H);
        let _ = cr.fill();
    }
    // Borders last and darker, so they read on top of the fill: these are the pixels a press takes.
    cr.set_source_rgb(KEPT_BORDER.0, KEPT_BORDER.1, KEPT_BORDER.2);
    for (x, _, _) in cut_trim::border_positions(&boxes) {
        cr.rectangle(x - 0.5, top, 1.5, cut_trim::BAR_H);
        let _ = cr.fill();
    }
    let _ = cr.restore();
    // F2.8 S2: the two rows under the kept bar — the recorders' band and the wave strip — drawn from the
    // SAME bands a press resolves against, so a plate the eye saw is the plate the hand picked. §7's translate
    // covers them too: both painters take tape pixels, so cairo slides them as one block (the lane plates,
    // the wave strips and the laid-piece bars all move with the clips above them).
    let _ = cr.save();
    cr.translate(-crate::ui::cut_strip_view::scroll_px(), 0.0);
    crate::ui::cut_trim_press::paint_bands(
        cr,
        w,
        &cut_,
        &crate::ui::cut_folds::page_recordings(),
        strip_pps(),
    );
    crate::ui::cut_trim_press::paint_laid_pieces(cr, w, &cut_, strip_pps());
    let _ = cr.restore();
}
/// Draw the ruler's labels onto the strip's cairo context. Cairo text is best-effort in the headless
/// container (no font config beyond what GTK brings up), so a failure here must never take the strip down
/// with it: every call's result is ignored and the tick marks stay visible regardless.
pub(crate) fn draw_strip_labels(cr: &cairo::Context, ticks: &[(f64, String)], w: f64) {
    cr.set_source_rgb(0.35, 0.35, 0.35);
    for (x, label) in ticks {
        if *x <= w {
            cr.move_to(x + 2.0, 11.0);
            let _ = cr.show_text(label);
        }
    }
}
/// Milliseconds since an arbitrary origin, for the scrub throttle. Monotonic enough for a 90 ms gate; the
/// app never compares these across a restart.
pub(crate) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
/// Attach the two gestures to `track-strip`: the left button trims a border, the right button moves.
/// Registered after `set_content` like every other control here, and kept in thread-locals so a test can
/// fire them the way GTK does.
pub(crate) fn wire_track_strip(window: &adw::ApplicationWindow) {
    let Some(strip) = track_strip(window) else {
        return;
    };

    let trimmer = gtk::GestureDrag::new();
    // §7: `A border belongs to both buttons; moving is the right button's verb`. The mover below says
    // `set_button(3)`; this one now says its own half of that sentence instead of leaving it to GTK's default.
    trimmer.set_button(1);
    // The border a drag chases, seeded from the press position over the drawn layout. Until F2.10/F2.11
    // own the real rows, this strip's own geometry (`cut_trim::clip_boxes` at `TRACK_STRIP_PPS`) is what
    // the pointer is pointing at — so the hand grabs exactly the pixel the eye saw, and no rule about
    // where a border may go lives here: `press_trim_border` asks `cut_trim::clamp_edge` for that.
    trimmer.connect_drag_begin(move |_g, px, _py| {
        let boxes = cut_trim::clip_boxes(&ui::newest_review_cut().segs, strip_pps());
        // The strip's own zoom converts this pixel; §7's translate is not added here because the strip widget
        // has no scroll of its own yet (`cut_strip_view::scroll_px` is zero and documented as such).
        let taken = cut_trim::border_at(&boxes, px);
        TRIM_DRAGS.with(|cell| {
            *cell.borrow_mut() = taken.map(|(index, border)| TrimDrag {
                index,
                border,
                scrubbed_ms: None,
                pushed: false,
            })
        });
    });
    let win_upd = window.clone();
    let strip_upd = strip.clone();
    let strip_end = strip.clone();
    trimmer.connect_drag_update(move |_g, x, _y| {
        TRIM_DRAGS.with(|cell| {
            let Some(mut drag) = cell.borrow_mut().take() else {
                return;
            };
            // §7's `one Undo per drag` is decided HERE, before the scrub gate: the hold's first move spends the
            // single step and every later tick of the same hold writes the edge in place. Read ahead of the
            // gate on purpose — a tick the 90 ms gate swallowed never wrote anything, so it must not be the tick
            // that spends the step either, and a drag whose first few ticks were swallowed still gets exactly
            // one step from the first tick that did write. `first` is read before the gate below and applied
            // only on the ticks that pass it, which is what makes the step the hold's first WRITE rather than
            // its first event.
            let first = !drag.pushed;
            // S1: the picture scrubs live, but at most every `SCRUB_MS` — an accurate seek per mouse-move
            // event is a pipeline that never stops flushing. `// preview.scrubMs`
            let now = now_millis();
            if !cut_trim::scrubs(drag.scrubbed_ms, now) {
                *cell.borrow_mut() = Some(drag);
                return;
            }
            drag.scrubbed_ms = Some(now);
            drag.pushed = true;
            crate::ui::cut_trim_press::press_border_move(
                &win_upd,
                drag.index,
                drag.border,
                x / strip_pps(),
                first,
            );
            // S1: the picture follows the border the hand is dragging, at the second the cut now holds.
            let landed = crate::ui::cut_trim_press::edge_second(drag.index, drag.border);
            crate::ui::cut_trim_press::scrub_picture(&win_upd, landed);
            *cell.borrow_mut() = Some(drag);
        });
        // The border moved, so the strip has to be repainted or the hand drags something invisible.
        strip_upd.queue_draw();
    });
    let win_end = window.clone();
    trimmer.connect_drag_end(move |_g, x, _y| {
        // Release lands the final edge and prints the sentence it earned (joined-into-one-scene when a
        // neighbour came within a frame), through the same seam as every tick above.
        let done = TRIM_DRAGS.with(|cell| cell.borrow_mut().take());
        if let Some(drag) = done {
            press_trim_border(&win_end, drag.index, drag.border, x / strip_pps());
            // The LAST edge always reaches the picture, even when the throttle swallowed the updates that
            // came before it — "the picture lands on the edge".
            let landed = crate::ui::cut_trim_press::edge_second(drag.index, drag.border);
            crate::ui::cut_trim_press::scrub_picture(&win_end, landed);
        } else {
            // A release with no drag in hand is S3's unmoved right press over the pictures: no edge moved,
            // so the picture goes to the second it belongs on — the released pixel, as the band press uses it.
            crate::ui::cut_trim_press::scrub_picture(&win_end, x / strip_pps());
        }
        TRIM_DRAGS.with(|cell| *cell.borrow_mut() = None);
        strip_end.queue_draw();
    });
    strip.add_controller(trimmer.clone());
    TRIM_GESTURES.with(|cell| *cell.borrow_mut() = Some(trimmer));

    // Button 3 is GTK's secondary button — the right hand, whose drags move and never trim.
    let mover = gtk::GestureDrag::new();
    mover.set_button(3);
    // F2.8 S2: the press fixes what the drag takes, so its x AND y are remembered here — `wave_strip` and
    // `on_border` have to come from where the pointer actually landed, not from a seam primed beforehand.
    // Newest-slot rule like `SELECT_SURFACES`, so one window's press cannot answer another's drag.
    let pressed_begin = Rc::new(std::cell::RefCell::new((0.0f64, 0.0f64)));
    MOVE_PRESSED.with(|cell| cell.borrow_mut().push(pressed_begin.clone()));
    let pressed_upd = pressed_begin.clone();
    // A `drag-begin` carries the ABSOLUTE press position; a `drag-update`/`drag-end` carry the OFFSET from
    // it. So the begin stores the absolute pair, and an update's own numbers are added to it to get where
    // the pointer is now. A caller that primed the x seam directly (`set_press_x`) already supplied the
    // absolute press, so its stored 0.0 begin adds nothing and the seam answer stands.
    mover.connect_drag_begin(move |_g, x, y| {
        // A `drag-begin` carries the ABSOLUTE press position; updates carry offsets from it. Each axis is
        // taken from the begin UNLESS a caller primed it directly (`set_press_x`, F2.10/F2.11's seam), in
        // which case the primed value stands -- hence the per-axis 0.0 "not primed" test rather than a
        // whole-pair one: a test can prime x and still let the real y through.
        let (px, py) = *pressed_begin.borrow();
        *pressed_begin.borrow_mut() = (
            if px == 0.0 { x } else { px },
            if py == 0.0 { y } else { py },
        );
        // F2.8 S2: a new hold starts with no Undo step spent, so the first real move is the one that pushes.
        crate::ui::cut_trim_press::begin_move();
    });
    let win = window.clone();
    mover.connect_drag_update(move |_g, x, _y| {
        // The press position was stored at the begin above; the update carries only the offset.
        // The stored pair is the press itself when a caller primed it; add nothing to it. Reading it here
        // rather than recomputing keeps the scope fixed at the press even if the pointer wanders.
        let (press_x, press_y) = *pressed_upd.borrow();
        // S1's "either button drags that border": if the press landed within EDGE_GRAB_PX of a drawn
        // clip border, the RIGHT button trims it rather than starting a slide. That is why S2 reads
        // "...the scene slides ... unless on a border" — the border question is answered first, by the
        // same `border_at` the left button uses, so the two sentences cannot contradict each other.
        if let Some((index, border)) = right_press_takes_border(press_x, x) {
            press_trim_border(&win, index, border, (press_x + x) / strip_pps());
            if let Some(strip) = track_strip(&win) {
                strip.queue_draw();
            }
            return;
        }
        // Otherwise the right hand moves: seconds travelled since the press, at the strip's zoom.
        press_move_at(&win, press_x, press_y, x, x / strip_pps());
    });
    // F2.8 S2 ("folds near the grab open for the drag, refold after"): the release puts back exactly the
    // folds this drag opened and forgets the hold, so one gesture is one Undo step and the fold list the
    // page was saved with comes back.
    let win_released = window.clone();
    mover.connect_drag_end(move |_g, _x, _y| {
        crate::ui::cut_trim_press::end_move(&win_released);
        // §7 bullet 7: the release is where a trim reaches `cut.json` — the hold only updates ui::state.
        crate::ui::cut_trim_press::end_release();
    });
    strip.add_controller(mover.clone());
    MOVE_GESTURES.with(|cell| *cell.borrow_mut() = Some(mover));
}
thread_local! {
    static DELETE_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}
/// Add the ⌦ / Delete / BackSpace controller to the window. It claims ONLY those two keys: every other
/// keypress is passed on untouched, so this cannot swallow typing in an entry field.
pub(crate) fn wire_delete_keys(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if matches!(key, gtk::gdk::Key::Delete | gtk::gdk::Key::BackSpace) {
            // F3.8 S3: `⌦ with an effect held removes it`. The EFFECT comes first because the hand last
            // touched it — the same priority `cut_verbs::delete_verb` already gives a held effect over a
            // ui::selection. With nothing in hand the key keeps its old job and still deletes the ui::selection.
            if ui::held_effect().is_some() {
                let said = ui::remove_held_effect(&win);
                if let Some(status_line) = ui::find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
                ui::log_line(&said);
            } else {
                crate::ui::cut_verb_press::press_delete_key(&win);
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller.clone());
    DELETE_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}
