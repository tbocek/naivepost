//! §F2.8 — trim and move on the track strip, the copy/paste hand, and the lane press that places a line.

use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_copy;
use crate::cut_line;
use crate::cut_screen;
use crate::cut_select;
use crate::cut_trim;
use crate::ui::window as ui;
pub(crate) use crate::ui::window_line_step::now_ms as now_millis;

pub(crate) struct TrimDrag {
    index: usize,
    border: cut_trim::Border,
    scrubbed_ms: Option<u64>,
    /// Whether THIS hold has spent its one Undo step (§7: one Undo per drag).
    pushed: bool,
}

pub fn delete_key_controller() -> Option<gtk::EventControllerKey> {
    WINDOW_PAGE.with(
        |WindowPage {
             delete_keys: cell, ..
         }| cell.borrow().clone(),
    )
}

/// Pixels-per-second for the strip: the same zoom `select-surface` runs at.
pub const TRACK_STRIP_PPS: f64 = cut_screen::ZOOM_AT_OPEN;

pub use crate::ui::cut_view::strip_pps;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};
pub fn track_strip(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    ui::find_widget_by_name(window.upcast_ref(), "track-strip")?
        .downcast()
        .ok()
}
pub fn trim_gesture() -> Option<gtk::GestureDrag> {
    WINDOW_PAGE.with(
        |WindowPage {
             trim_gestures: cell,
             ..
         }| cell.borrow().clone(),
    )
}
pub fn move_gesture() -> Option<gtk::GestureDrag> {
    WINDOW_PAGE.with(
        |WindowPage {
             move_gestures: cell,
             ..
         }| cell.borrow().clone(),
    )
}
pub(crate) type MovePressed = std::cell::RefCell<Vec<Rc<std::cell::RefCell<(f64, f64)>>>>;

/// What the right button was pressed on: a recording on the recorders' band, or the scene under the pointer.
/// Seams standing in until the real track rounds (F2.10/F2.11) set them from a press position.
pub fn set_press_band(recording: Option<String>) {
    WINDOW_PAGE.with(
        |WindowPage {
             press_band: cell, ..
         }| *cell.borrow_mut() = recording,
    );
}
pub fn press_band() -> Option<String> {
    WINDOW_PAGE.with(
        |WindowPage {
             press_band: cell, ..
         }| cell.borrow().clone(),
    )
}
pub fn set_press_scene(scene: Option<usize>) {
    WINDOW_PAGE.with(
        |WindowPage {
             press_scene: cell, ..
         }| *cell.borrow_mut() = scene,
    );
}
pub fn press_scene() -> Option<usize> {
    WINDOW_PAGE.with(
        |WindowPage {
             press_scene: cell, ..
         }| *cell.borrow(),
    )
}
/// F2.8 S1: drag one clip's border to `to` (session seconds); [`cut_trim::clamp_edge`] decides where it may
/// go, the write is `cut_trim_press::press_border_move`.
pub fn press_trim_border(
    window: &adw::ApplicationWindow,
    index: usize,
    border: cut_trim::Border,
    to: f64,
) -> f64 {
    crate::ui::cut_trim_press::press_border_move(window, index, border, to, false)
}
/// track: until the real bounds come with the tracks (F2.10/F2.11) a trim needs some wall to stop at.
pub const TRACK_REC_END: f64 = 3600.0;

/// F2.8 S1: where a clip at `s..e` may reach, `(start, end)`: the recording it stands in, once the session's
/// takes are known. Before that the wall is the tape's end; a clip already past [`TRACK_REC_END`] alone had
/// its end clamped to the 1 s minimum the moment a border was taken.
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

/// F2.8 S2: where a slide may reach: the tape's end, or [`TRACK_REC_END`] on a shorter tape.
/// A slide can carry a selection across recordings, so no one recording fences it.
pub fn session_wall() -> f64 {
    crate::ui::cut_folds::page_recordings()
        .iter()
        .map(|take| take.end)
        .fold(TRACK_REC_END, f64::max)
}

/// F2.8 S2: the boxes `track-strip` paints for this window's cut, in the px space a press arrives in.
/// Read from the newest review cut so a press and a paint never disagree.
pub(crate) fn trim_boxes_for_strip() -> Vec<cut_trim::Box_> {
    let cut_ = ui::newest_review_cut();
    cut_trim::clip_boxes(&cut_.segs, strip_pps())
}
/// F2.8 S1/S2: what a right-press at `press_x` takes. A border within [`cut_trim::EDGE_GRAB_PX`] of a
/// DRAWN box belongs to S1 and is trimmed, not slid, but only once the hand has travelled: an unmoved press
/// stays a click (S3).
fn right_press_takes_border(press_x: f64, travel_px: f64) -> Option<(usize, cut_trim::Border)> {
    if travel_px.abs() < cut_select::DRAG_SLOP_PX {
        return None;
    }
    cut_trim::border_at(&trim_boxes_for_strip(), press_x)
}
/// F2.8 S2: where this window's move press landed in x, in strip px. Stored at `drag-begin`; a caller may
/// also prime it directly (as `set_press_scene`), which is how a test places a press mid-scene.
pub fn set_press_x(x: f64) {
    // Drop the read's borrow before writing: a second `borrow_mut` on the same RefCell panics.
    let old = WINDOW_PAGE.with(
        |WindowPage {
             move_pressed: cell, ..
         }| cell.borrow().last().map(|s| *s.borrow()),
    );
    if let Some((_, y)) = old {
        WINDOW_PAGE.with(
            |WindowPage {
                 move_pressed: cell, ..
             }| {
                if let Some(slot) = cell.borrow().last() {
                    *slot.borrow_mut() = (x, y);
                }
            },
        );
    }
}
pub fn press_position() -> (f64, f64) {
    WINDOW_PAGE.with(
        |WindowPage {
             move_pressed: cell, ..
         }| {
            cell.borrow()
                .last()
                .map(|s| *s.borrow())
                .unwrap_or((0.0, 0.0))
        },
    )
}
/// F2.8 S2/S3: the older door, seam-only, for callers that supply no press position: answers with whatever
/// `set_press_band` primed, so it can never move the line without travel. The widget path uses [`press_move_at`].
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
/// F2.8 S2: what a right-press at (`press_x`, `press_y`) took, as the widget computes it, returned instead
/// of applied so a test can assert WHICH branch the wire chose.
///
/// The recording NAME comes from `set_press_band`; WHICH band was hit comes from the press y through
/// `cut_trim::press_targets` over `cut_trim::PLACEHOLDER_STRIP_BANDS`.
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
/// F2.8 S2: the move door that knows WHERE the press landed: like [`press_move`], but `recorders_band` /
/// `wave_strip` / `on_border` come from the press position through [`right_press_took`].
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
    // S3 lives HERE, at the press: a right press that never travelled is a click, answered by the status line.
    if gesture == cut_trim::Gesture::Click && travel_px.abs() < cut_trim::ROW_TRAVEL_PX {
        crate::ui::cut_trim_press::press_unmoved(window);
    }
    gesture
}
/// The row a right-press lands on, and the row a zoom records itself as framed on (§1's `cam` column):
/// F2.4's watched row, else row 0; clamped to the rows this cut has.
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
/// The folds this window's drag in flight asked to open, newest write last.
pub fn open_folds(_window: &adw::ApplicationWindow) -> Vec<usize> {
    crate::ui::cut_trim_press::open_folds()
}
/// What is in hand right now, the read side of [`press_copy`].
pub fn copy_hand() -> Option<cut_copy::Hand> {
    WINDOW_PAGE.with(
        |WindowPage {
             copy_hand: cell, ..
         }| cell.borrow().clone(),
    )
}
pub fn copy_esc_controller() -> Option<gtk::EventControllerKey> {
    WINDOW_PAGE.with(
        |WindowPage {
             copy_esc_keys: cell,
             ..
         }| cell.borrow().clone(),
    )
}
/// F2.9 S1: ⧉ Copy. [`cut_copy::copy`] decides everything; this keeps the answer if it took one and returns
/// the sentence. A refusal leaves no hand, so ⧉ Paste stays greyed.
pub fn press_copy(_window: &adw::ApplicationWindow) -> String {
    // F2.7 S2: with no band up, ⧉ Copy takes the half a | Split at the red line put in the verb hand.
    if ui::selection().is_none() {
        if let Some(half) = crate::ui::cut_verb_press::verb_hand() {
            let taken = cut_copy::copy(Some(&half));
            if let cut_copy::Take::Taken(hand) = &taken {
                WINDOW_PAGE.with(
                    |WindowPage {
                         copy_hand: cell, ..
                     }| *cell.borrow_mut() = Some(hand.clone()),
                );
                // The hand is spent: taking it twice would be two pastes of one split.
                crate::ui::cut_verb_press::take_into_hand(None);
                return cut_copy::copied_status(hand);
            }
        }
    }
    let taken = cut_copy::copy(ui::selection().as_ref());
    match &taken {
        cut_copy::Take::Taken(hand) => {
            WINDOW_PAGE.with(
                |WindowPage {
                     copy_hand: cell, ..
                 }| *cell.borrow_mut() = Some(hand.clone()),
            );
            cut_copy::copied_status(hand)
        }
        cut_copy::Take::TooShort(say) => say.clone(),
        cut_copy::Take::NothingSelected(say) => (*say).to_string(),
    }
}
fn hand_file(hand: &cut_copy::Hand, sources: &[String]) -> Option<String> {
    let stem_of = |path: &str| crate::narrate_data::take_base(std::path::Path::new(path));
    let named = |name: &str| {
        sources
            .iter()
            .find(|source| stem_of(source) == stem_of(name))
            .cloned()
    };
    if let Some(recording) = hand.recording() {
        let lane = ui::newest_review_cut()
            .lanes
            .iter()
            .find(|lane| lane.name == recording)
            .map(|lane| lane.src.clone());
        if let Some(file) = lane.or_else(|| named(recording)) {
            return Some(file);
        }
    }
    crate::ui::cut_folds::page_recordings()
        .iter()
        .find(|rec| rec.start <= hand.from && hand.from < rec.end)
        .and_then(|rec| named(&rec.base))
}

/// F2.9 S2: ⧉ Paste at the red line. [`cut_copy::paste`] decides and refuses with its own sentence; this
/// supplies the line, the copied recording's project path and the file second, writes the cut back and
/// mirrors the hand so a refusal keeps the copy.
pub fn press_paste(window: &adw::ApplicationWindow) -> String {
    let mut hand = copy_hand();
    let sources = ui::session_sources();
    let path = hand
        .as_ref()
        .filter(|h| h.recording().is_some())
        .and_then(|h| hand_file(h, &sources))
        .unwrap_or_default();
    let session: Vec<String> = hand
        .as_ref()
        .and_then(|h| h.recording())
        .filter(|_| !path.is_empty())
        .map(|recording| vec![recording.to_string()])
        .unwrap_or_else(|| sources.clone());
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
        crate::ui::cut_lane::paste_line_opt(),
        &path,
        file_seconds,
        &session,
    );
    // Only a paste that landed writes.
    if outcome.is_ok() {
        ui::save_insert_cut(&cut_);
    }
    ui::record_edit(window, &cut_);
    WINDOW_PAGE.with(
        |WindowPage {
             copy_hand: cell, ..
         }| *cell.borrow_mut() = hand,
    );
    match outcome {
        Ok(status) => status,
        Err(refusal) => refusal,
    }
}
/// F2.9 S3: ⇲ Lane. [`cut_copy::lane_start`] says whether the line may open a row and [`cut_copy::lane`]
/// builds it. NOTHING is cut to the new row (a lane that arrived already green would be a cut nobody made).
/// Name: `Copied`, then `Copied-2`, `Copied-3`… since `Lane::name` keys `Cut::rows` and every `quiet` list.
/// The ⇲ button's door: [`press_lane`], except with nothing in hand the footage file is probed on a worker
/// (§03-shell §1: ffprobe can take half a second) and the row is filed when the length lands.
pub fn press_lane_from_button(window: &adw::ApplicationWindow) -> String {
    if copy_hand().is_some() {
        return press_lane(window);
    }
    crate::ui::cut_lane::lane_for_footage_file_on_worker(window)
}

pub fn press_lane(window: &adw::ApplicationWindow) -> String {
    let Some(hand) = copy_hand() else {
        // §B's other half: nothing in hand, so a footage FILE gets the row.
        return crate::ui::cut_lane::lane_for_footage_file(window);
    };
    let at = crate::ui::cut_lane::paste_line();
    let mut cut_ = ui::newest_review_cut();
    // The session's FILMED runs, as `lane_start`'s "nothing is rolling at m:ss" asks. Where no session was
    // resolved the page's list is the kept clips' spans, so this only adds a real project's tape.
    let mut spans = crate::ui::cut_folds::page_recordings();
    spans.extend(crate::timeline::kept_footage_recordings(&cut_));
    let filmed: Vec<(f64, f64)> = crate::timeline::filmed_runs(&spans);
    match cut_copy::lane_start(Some(at), &hand, &filmed) {
        cut_copy::LaneStart::Refusal(say) => say,
        cut_copy::LaneStart::Start(at) => {
            let taken: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
            let name = cut_copy::lane_name("Copied", &taken);
            let source = hand_file(&hand, &ui::session_sources())
                .or_else(|| hand.recording().map(str::to_string))
                .unwrap_or_default();
            let file_seconds =
                cut_copy::file_seconds(&cut_, &crate::ui::cut_folds::page_recordings(), &hand);
            let Some(lane) = cut_copy::lane(&hand, &source, file_seconds, at, name.clone()) else {
                return cut_copy::too_short().to_string();
            };
            cut_.lanes.push(lane);
            ui::save_insert_cut(&cut_);
            ui::record_edit(window, &cut_);
            cut_copy::lane_status(hand.length, hand.from, &name, at)
        }
    }
}
/// Record whether a second has been placed. `false` clears the flag AND drops the line to zero, so the page
/// never says it has no target while showing one. `true` only claims the target.
pub fn note_place(placed: bool) {
    WINDOW_PAGE.with(
        |WindowPage {
             window_insert_place_known: cell,
             ..
         }| *cell.borrow_mut() = Some(placed),
    );
    if !placed {
        WINDOW_PAGE.with(
            |WindowPage {
                 line_states: slots, ..
             }| {
                if let Some(slot) = slots.borrow().last() {
                    slot.borrow_mut().0 = cut_line::LinePos { t: 0.0 };
                }
            },
        );
    }
}
/// S2: Esc drops the copy. Claims ONLY Escape, so typing in an entry field is untouched.
pub(crate) fn wire_copy_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key == gtk::gdk::Key::Escape
            && WINDOW_PAGE.with(
                |WindowPage {
                     copy_hand: cell, ..
                 }| cell.borrow().is_some(),
            )
        {
            WINDOW_PAGE.with(
                |WindowPage {
                     copy_hand: cell, ..
                 }| *cell.borrow_mut() = None,
            );
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
    WINDOW_PAGE.with(
        |WindowPage {
             copy_esc_keys: cell,
             ..
         }| *cell.borrow_mut() = Some(controller),
    );
}
/// Whether another Esc owner has something to drop: a copy, a held effect, an arm or an open form.
/// The hand's layer stands back for them, whichever order GTK asks the controllers in.
fn esc_belongs_elsewhere() -> bool {
    copy_hand().is_some()
        || crate::ui::cut_hand::held_effect().is_some()
        || ui::zoom_armed()
        || ui::zoom_form_open().is_some()
        || crate::ui::cut_text_hand::text_armed()
        || crate::ui::cut_text_hand::text_form_open().is_some()
        || crate::ui::cut_svg::svg_armed()
        || crate::ui::cut_svg::svg_form_open().is_some()
        || crate::ui::cut_speed_hand::speed_form_open().is_some()
        || crate::ui::cut_volume_hand::volume_form_open().is_some()
        || crate::ui::cut_label_hand::label_form_open().is_some()
        || crate::ui::window_insert::insert_open().is_some()
}

/// The last Esc layer: with nothing else to drop, Esc puts down the clip or edge in hand, and with
/// nothing in hand clears the selection; one layer per press. Proceeds while another Esc owner has
/// something ([`esc_belongs_elsewhere`]).
pub(crate) fn wire_hand_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape || esc_belongs_elsewhere() {
            return glib::Propagation::Proceed;
        }
        let said = match crate::ui::cut_hand::held_kind() {
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
pub const HAND_EDGE_DOWN: &str = "the edge in hand is put down";
pub const HAND_CLIP_DOWN: &str = "the clip in hand is put down";
pub const SELECTION_DROPPED: &str = "the selection is cleared";
/// F2.9: set the three copy buttons from live ui::state. Copy is live with a band a second or longer; Paste
/// and Lane only while a hand is held.
pub(crate) fn refresh_copy_buttons(window: &adw::ApplicationWindow) {
    let copy_live = ui::selection()
        .map(|band| band.length() >= cut_select::MIN_SCENE_SECONDS)
        .unwrap_or(false);
    let held = copy_hand().is_some();
    if let Some(button) = ui::line_step_button(window, "copy-button") {
        button.set_sensitive(copy_live);
    }
    for name in ["paste-button", "lane-button"] {
        let live =
            held || (name == "lane-button" && crate::ui::cut_lane::footage_source_path().is_some());
        if let Some(button) = ui::line_step_button(window, name) {
            button.set_sensitive(live);
        }
    }
}
/// Attach the two gestures to `track-strip`: left button trims a border, right button moves.
/// Kept in thread-locals so a test can fire them the way GTK does.
pub(crate) fn wire_track_strip(window: &adw::ApplicationWindow) {
    let Some(strip) = track_strip(window) else {
        return;
    };

    let trimmer = gtk::GestureDrag::new();
    // §7: `A border belongs to both buttons; moving is the right button's verb`.
    trimmer.set_button(1);
    // The border a drag chases, seeded from the press position over the drawn layout; no rule about where a
    // border may go lives here (`press_trim_border` asks `cut_trim::clamp_edge`).
    trimmer.connect_drag_begin(move |_g, px, _py| {
        let boxes = cut_trim::clip_boxes(&ui::newest_review_cut().segs, strip_pps());
        // The strip has no scroll of its own yet (`cut_strip_view::scroll_px` is zero), so §7's translate is not added.
        let taken = cut_trim::border_at(&boxes, px);
        WINDOW_PAGE.with(
            |WindowPage {
                 trim_drags: cell, ..
             }| {
                *cell.borrow_mut() = taken.map(|(index, border)| TrimDrag {
                    index,
                    border,
                    scrubbed_ms: None,
                    pushed: false,
                })
            },
        );
    });
    let win_upd = window.clone();
    let strip_upd = strip.clone();
    let strip_end = strip.clone();
    trimmer.connect_drag_update(move |_g, x, _y| {
        WINDOW_PAGE.with(
            |WindowPage {
                 trim_drags: cell, ..
             }| {
                let Some(mut drag) = cell.borrow_mut().take() else {
                    return;
                };
                // §7's `one Undo per drag`, decided HERE before the scrub gate: the step is the hold's first WRITE, not
                // its first event, so a tick the 90 ms gate swallowed must not spend it.
                let first = !drag.pushed;
                // S1: the picture scrubs live, at most every `SCRUB_MS` (an accurate seek per event never stops flushing).
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
                let landed = crate::ui::cut_trim_press::edge_second(drag.index, drag.border);
                crate::ui::cut_trim_press::scrub_picture(landed);
                *cell.borrow_mut() = Some(drag);
            },
        );
        strip_upd.queue_draw();
    });
    let win_end = window.clone();
    trimmer.connect_drag_end(move |_g, x, _y| {
        // Release lands the final edge and prints the sentence it earned.
        let done = WINDOW_PAGE.with(
            |WindowPage {
                 trim_drags: cell, ..
             }| cell.borrow_mut().take(),
        );
        if let Some(drag) = done {
            press_trim_border(&win_end, drag.index, drag.border, x / strip_pps());
            // The LAST edge always reaches the picture, even when the throttle swallowed the updates before it.
            let landed = crate::ui::cut_trim_press::edge_second(drag.index, drag.border);
            crate::ui::cut_trim_press::scrub_picture(landed);
        } else {
            // A release with no drag in hand is S3's unmoved right press: the picture goes to the released pixel.
            crate::ui::cut_trim_press::scrub_picture(x / strip_pps());
        }
        WINDOW_PAGE.with(
            |WindowPage {
                 trim_drags: cell, ..
             }| *cell.borrow_mut() = None,
        );
        strip_end.queue_draw();
    });
    strip.add_controller(trimmer.clone());
    WINDOW_PAGE.with(
        |WindowPage {
             trim_gestures: cell,
             ..
         }| *cell.borrow_mut() = Some(trimmer),
    );

    // Button 3 is GTK's secondary button: the right hand, whose drags move and never trim.
    let mover = gtk::GestureDrag::new();
    mover.set_button(3);
    // F2.8 S2: the press fixes what the drag takes, so x AND y are remembered: `wave_strip` and `on_border`
    // must come from where the pointer landed, not from a seam primed beforehand. Newest-slot like `SELECT_SURFACES`.
    let pressed_begin = Rc::new(std::cell::RefCell::new((0.0f64, 0.0f64)));
    WINDOW_PAGE.with(
        |WindowPage {
             move_pressed: cell, ..
         }| cell.borrow_mut().push(pressed_begin.clone()),
    );
    let pressed_upd = pressed_begin.clone();
    // `drag-begin` carries the ABSOLUTE press position, `drag-update`/`drag-end` the OFFSET from it, so
    // the begin stores the pair. A caller that primed x directly (`set_press_x`) stored 0.0 and the seam stands.
    mover.connect_drag_begin(move |_g, x, y| {
        // Each axis is taken from the begin UNLESS primed directly (`set_press_x`): a per-axis 0.0 "not primed"
        // test, so a test can prime x and still let the real y through.
        let (px, py) = *pressed_begin.borrow();
        *pressed_begin.borrow_mut() = (
            if px == 0.0 { x } else { px },
            if py == 0.0 { y } else { py },
        );
        crate::ui::cut_trim_press::begin_move();
    });
    let win = window.clone();
    mover.connect_drag_update(move |_g, x, _y| {
        // The press position was stored at the begin; the update carries only the offset.
        let (press_x, press_y) = *pressed_upd.borrow();
        // S1's "either button drags that border": a press within EDGE_GRAB_PX of a drawn clip border makes the
        // RIGHT button trim it rather than slide, answered by the same `border_at` the left button uses.
        if let Some((index, border)) = right_press_takes_border(press_x, x) {
            press_trim_border(&win, index, border, (press_x + x) / strip_pps());
            if let Some(strip) = track_strip(&win) {
                strip.queue_draw();
            }
            return;
        }
        press_move_at(&win, press_x, press_y, x, x / strip_pps());
    });
    // F2.8 S2: the release puts back exactly the folds this drag opened, so one gesture is one Undo step.
    let win_released = window.clone();
    mover.connect_drag_end(move |_g, _x, _y| {
        crate::ui::cut_trim_press::end_move(&win_released);
        // §7 bullet 7: the release is where a trim reaches `cut.json`.
        crate::ui::cut_trim_press::end_release();
    });
    strip.add_controller(mover.clone());
    WINDOW_PAGE.with(
        |WindowPage {
             move_gestures: cell,
             ..
         }| *cell.borrow_mut() = Some(mover),
    );
}
/// Add the ⌦ / Delete / BackSpace controller. It claims ONLY those two keys, so typing in an entry is safe.
pub(crate) fn wire_delete_keys(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if matches!(key, gtk::gdk::Key::Delete | gtk::gdk::Key::BackSpace) {
            // F3.8 S3: `⌦ with an effect held removes it`; the effect comes first, as `cut_verbs::delete_verb` does.
            if crate::ui::cut_hand::held_effect().is_some() {
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
    WINDOW_PAGE.with(
        |WindowPage {
             delete_keys: cell, ..
         }| *cell.borrow_mut() = Some(controller),
    );
}
