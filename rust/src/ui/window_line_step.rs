//! §F2.4 — the red line and its frame stepping: where the line stands, what a step or an arrow
//! does to it, the four step buttons and their key controller. Split out of `window.rs`, which is
//! past its size budget; its doors are re-exported through `window`.
//!
//! The rules are `cut_line`’s (the position, the throttled writer, what a nudge means); what is
//! here reads this window’s own slots, calls one rule, and paints from the answer.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_delete;
use crate::cut_line;
use crate::cut_play_leg;
use crate::ui::cut_picture;
use crate::ui::window as ui;
use crate::ui::window_insert::place_known;
use crate::ui::window_selection::{clear_selection, nudge_held_effect};
use crate::ui::window_trim_strip::note_place;

/// `line-step-back-five` reads as "five" while saying nothing about which way.
pub(crate) const LINE_STEP_BUTTONS: [(&str, &str, i64); 4] = [
    ("line-step-back-five", "\u{2039}\u{2039} f  step back 5 frames", -5),
    ("line-step-back", "\u{2039} f  step back 1 frame", -1),
    ("line-step-forward", "f \u{203a}  step forward 1 frame", 1),
    ("line-step-forward-five", "f \u{203a}\u{203a}  step forward 5 frames", 5),
];

/// The reason ▶ had nothing to play. Spec silent on a session with no filmed stretch; §0 asks for a
/// named, local reason rather than a silent button.
pub const NO_RECORDING_STATUS: &str = "nothing was filmed \u{2014} there is no recording to play";

type WindowFolders = std::cell::RefCell<Vec<std::path::PathBuf>>;
type WindowTrees = std::cell::RefCell<Vec<Option<crate::layout::Tree>>>;

// The project root this window's line file sits under, newest slot winning like [`REVIEW_CUTS`]. Held
// beside [`ui::LINE_STATES`] because the position without its root is a number with nowhere to be saved:
// `restore` needs it on open and `flush_line_on_close` needs it on close. `SESSION_DIRS` is the same
// shape for the folder §2 S6's close flush writes the session into — `project_autosave::session_dir()`
// asks the process cwd, a different answer once a flow has moved the session or a test has pinned it.
thread_local! {
    pub(crate) static LINE_ROOTS: WindowFolders = const { std::cell::RefCell::new(Vec::new()) };
    pub(crate) static SESSION_DIRS: WindowFolders = const { std::cell::RefCell::new(Vec::new()) };
    // The session folder each window took on, as a `Tree`, newest slot winning like [`SESSION_DIRS`].
    // The Cut page's ▶ resolves its session through this rather than through one shared slot, so a fresh
    // window stops answering with whatever folder an earlier window in this process pointed at.
    pub(crate) static SESSION_TREES: WindowTrees = const { std::cell::RefCell::new(Vec::new()) };
}

/// Publish the session folder THIS window took on, for the Cut page's ▶. One push per window build and
/// per Open, so the newest entry is always the folder the live window holds.
pub fn push_session_tree(tree: Option<crate::layout::Tree>) {
    SESSION_TREES.with(|held| held.borrow_mut().push(tree));
}

/// The newest published session folder, or `None` when no window has taken one on yet — which is how a
/// flow with no shell open (or a test that built none) falls back to its own resolution.
pub(crate) fn last_session_tree() -> Option<Option<crate::layout::Tree>> {
    SESSION_TREES.with(|held| held.borrow().last().cloned())
}

/// §2 S6 as the close hook wants it, resolved through THIS window: write the live session into the
/// folder that window was built for. Returns whether a file was written.
pub(crate) fn flush_session_on_close() -> bool {
    let Some(dir) = SESSION_DIRS.with(|held| held.borrow().last().cloned()) else {
        return false;
    };
    crate::project_autosave::flush_on_close(&ui::live_project(), &dir)
}

/// What a frame step did (F2.4 S2/S3), so the caller paints the right thing and decides nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Something was held: THAT moved by `frames` frames rather than the line.
    Nudged { frames: i64 },
    /// Nothing held: the recording under the line seeked to `t` and paused.
    SteppedTo { t: f64 },
    /// Nothing happened — S3's arrows with nothing held.
    Still,
}

/// The project root this window's line file lives under, or `None` when the page was built outside a
/// project. Newest slot wins like every other per-window accessor here.
pub fn line_root(window: &adw::ApplicationWindow) -> Option<std::path::PathBuf> {
    let _ = window;
    LINE_ROOTS.with(|slots| slots.borrow().last().cloned())
}

/// This window's red line position.
pub fn line_position(window: &adw::ApplicationWindow) -> cut_line::LinePos {
    let _ = window;
    ui::LINE_STATES
        .with(|slots| slots.borrow().last().map(|slot| slot.borrow().0))
        .unwrap_or(cut_line::LinePos { t: 0.0 })
}

/// Set this window's line WITHOUT saving -- the test/setup entry point. Saving is a side effect of a
/// user moving the line, not of being told where it is, and a setup call that consumed the throttle
/// would make the next real move silently unwritable for a second.
pub fn set_line_position(window: &adw::ApplicationWindow, pos: cut_line::LinePos) {
    let _ = window;
    // F2.12 S1: a placed line IS a target for Insert, and this is where the page learns one exists.
    note_place(true);
    ui::LINE_STATES.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            slot.borrow_mut().0 = pos;
        }
    });
    // F3.6 S5 (`applied in the preview even while paused`): a line that LANDED on a second, so that second's
    // loudness goes to sound now — a parked line inside a raised passage must be heard at that loudness or the
    // fades cannot be judged without playing. `note_place(true)` is what says "a person put the line down":
    // `set_line_position` is also the setup door (snapshots and tests park a line before any hand was there), and
    // a gain ask for a second nobody chose would be the app speaking for a user. While TRANSPORT is running the
    // tick hands the same number itself, so this asks only of a still frame and the two never both speak.
    if place_known() && !ui::preview_player(window).transport.playing {
        crate::ui::cut_volume_hear::apply_paused_gain(window, pos.t);
    }
}

/// A user moved the line: store it and let [`cut_line::LineWriter`] decide whether that is worth a
/// save right now (at most one write a second, `cut_line::LINE_WRITE_MS`). Returns whether it wrote.
///
/// No project root reaches the page yet (the cut-model round owns it), so there is nowhere to write;
/// the throttle is still advanced so the rate rule holds the moment a root exists.
pub fn move_line_and_save(window: &adw::ApplicationWindow, pos: cut_line::LinePos, now_ms: u64) -> bool {
    // A root DOES reach the page (`line_root`), so a move writes through the writer, which keeps its own
    // one-second rule. With no root the rate rule still holds via the throttle stamp alone.
    let root = line_root(window);
    ui::LINE_STATES.with(|slots| {
        let Some(slot) = slots.borrow().last().cloned() else {
            return false;
        };
        slot.borrow_mut().0 = pos;
        let Some(root) = root else {
            // No project open: keep the one-second rule on the stamp alone, for when a root exists.
            let owed = cut_line::may_write_line(slot.borrow().1.last_write_ms(), now_ms, false);
            if owed {
                slot.borrow_mut().1.note_throttled(now_ms);
            }
            return owed;
        };
        slot.borrow_mut().1.note_move(pos, &root, now_ms);
        true
    })
}

/// F2.4 S4: the close path. Writes the line through the one-second rate limit — `LineWriter::flush`
/// asks `may_write_line(.., closing: true)`, so a position reached 10 ms after the last save still lands
/// (the prototype's `lineSaveMs` / `cut_line::LINE_WRITE_MS` caps MOVES, not the close). Returns
/// whether bytes went out: `false` when this window has no root or no slot to write from.
pub fn flush_line_on_close(window: &adw::ApplicationWindow) -> bool {
    let Some(root) = line_root(window) else {
        return false;
    };
    ui::LINE_STATES.with(|slots| {
        let Some(slot) = slots.borrow().last().cloned() else {
            return false;
        };
        let pos = line_position(window);
        // Bound to a `let` so the RefMut drops before `slot` does — as a bare tail expression its
        // temporary outlives the binding and the borrow checker refuses it.
        let wrote = slot.borrow_mut().1.flush(pos, &root, now_ms());
        wrote
    })
}

/// F2.4 S1 through the seam, with the row the pointer landed on: a press places the line, clears the
/// selection and watches THAT row exactly as [`cut_line::click_outcome_in_row`] says.
pub fn place_line_press(
    window: &adw::ApplicationWindow,
    hit: cut_line::Hit,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    // F2.2 S2: a click on the picture is another way in that cannot switch the preview, so an empty
    // cut refuses it here rather than placing a line over nothing (nothing moves, clears or watches).
    let refused = hit.on_picture && cut_play_leg::other_way_in(&ui::review_cut_of(window)).is_some();
    let outcome = if refused {
        cut_line::refused_click(hit.at)
    } else {
        cut_line::click_outcome_in_row(hit.at, hit.on_picture, hit.on_scene_picture, hit.playing, hit.sources, hit.gutter, hit.row)
    };
    if outcome.line_moved {
        move_line_and_save(window, cut_line::LinePos { t: outcome.line_at }, now_ms);
    }
    if outcome.clears_selection {
        clear_selection(window);
    }
    ui::set_watched_row(outcome.watches);
    if outcome.takes_scene {
        let under = cut_delete::scene_under(&ui::newest_review_cut().segs, outcome.line_at).cloned();
        ui::set_held_clip(under);
    }
    outcome
}



/// F2.4 S2: ‹f / f› steps one frame (Shift five). With a hold, the HOLD moves; with nothing held the
/// recording under the line seeks and pauses.
pub fn step_the_line(
    window: &adw::ApplicationWindow,
    shift: bool,
    held: Option<cut_line::Held>,
    fps: f64,
    now_ms: u64,
) -> Step {
    let frames = cut_line::step_frames(shift);
    if held.is_some() {
        // Nudging the held edge/clip/effect needs the held item's own geometry, which belongs to the
        // cut-editing rounds; the returned frame count is what the caller acts on once that exists.
        return Step::Nudged { frames };
    }
    let from = line_position(window).t;
    let to = cut_line::step_line(from, frames, fps);
    move_line_and_save(window, cut_line::LinePos { t: to }, now_ms);
    // S2's "then pause": stepping is looking at a frame, not watching a run.
    pause_for_step(window);
    // And the frame follows: the preview shows the new second and the view keeps it on screen.
    cut_picture::note_line_moved(window, to);
    crate::ui::cut_view::follow_playhead(window, to);
    Step::SteppedTo { t: to }
}

/// F2.4 S3: ← / → step only while something is held; with nothing held they do nothing at all.
pub fn arrow_steps(
    window: &adw::ApplicationWindow,
    shift: bool,
    held: Option<cut_line::Held>,
    fps: f64,
    now_ms: u64,
) -> Step {
    if !cut_line::arrow_moves_line(held) {
        return Step::Still;
    }
    step_the_line(window, shift, held, fps, now_ms)
}

/// F2.4 S3: Space toggles the preview unless a text box has the focus.
///
/// F2.2 S2 adds the refusal in front of the toggle: Space is one of the ways in that CANNOT switch the
/// preview (only ▶✂ does that), so on an empty cut it says what to do instead of starting a player
/// with nothing to play. The sentence comes from `cut_play_leg::other_way_in`, i.e. from `cut_play`.
pub fn space_toggles(window: &adw::ApplicationWindow, text_focus: bool) -> bool {
    let toggle = cut_line::space_toggles_preview(text_focus);
    if toggle && !text_focus {
        // S2: an empty cut refuses this way in and leaves the transport where it was.
        if let Some(refusal) = cut_play_leg::other_way_in(&ui::review_cut_of(window)) {
            if let Some(status) = ui::find_status(window.upcast_ref()) {
                status.set_text(refusal);
            }
            return false;
        }
        if let Some(player) = ui::live_player(window) {
            let mut held = player.borrow_mut();
            held.transport.playing = !held.transport.playing;
            held.transport.started = held.transport.playing || held.transport.started;
        }
    }
    toggle
}

/// Find one of the four named line-step buttons.
pub fn line_step_button(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Button> {
    ui::find_widget_by_name(window.upcast_ref(), name)?.downcast().ok()
}

/// This window's key controller for the line (F2.4 S2/S3): ‹‹f ‹f f› ›› step one or five frames,
/// ← / → move only while something is held, Space toggles play.
pub fn line_key_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    LINE_KEYS.with(|cell| cell.borrow().clone())
}

thread_local! {
    static LINE_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// Wire one step button to its signed frame count. A button press holds nothing, so this is always
/// S2's seek-and-pause; the direction lives in `frames`, never inferred from the widget.
pub(crate) fn wire_line_step(button: &gtk::Button, window: &adw::ApplicationWindow, frames: i64) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        let _ = step_frames_of(&window, frames);
    });
}

/// Move the line by a signed number of frames and pause (F2.4 S2). The seam the buttons use; the rate
/// comes from [`crate::cut_fps`] (the real ffprobe over the footage file), not a constant.
pub fn step_frames_of(window: &adw::ApplicationWindow, frames: i64) -> Step {
    let from = line_position(window).t;
    let to = cut_line::step_line(from, frames, crate::cut_fps::session_fps(window));
    move_line_and_save(window, cut_line::LinePos { t: to }, now_ms());
    pause_for_step(window);
    // The frame the user sees after a step is Prepare's JPEG nearest the new second -- the same frame a
    // click cues -- and the view recentres if the step took the line off screen.
    cut_picture::note_line_moved(window, to);
    crate::ui::cut_view::follow_playhead(window, to);
    Step::SteppedTo { t: to }
}

/// S2's "then pause", all three ways something can play: the transport flag, ▶'s process, ▶✂'s pair.
pub(crate) fn pause_for_step(window: &adw::ApplicationWindow) {
    if let Some(player) = ui::live_player(window) {
        player.borrow_mut().transport.playing = false;
    }
    if crate::play_back::playing_pid().is_some() {
        crate::play_back::halt();
    }
    if crate::cut_play_back::playing_pid().is_some() {
        crate::cut_play_back::halt();
    }
}
/// A monotone-ish millisecond clock for the write throttle. Coarse is fine: the rule it feeds only asks
/// whether a second has passed.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Add the keyboard controller to the window. Called after `set_content`, like every other `wire_*`.
pub(crate) fn wire_line_keys(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, mods| {
        let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        // S2/S3: the hold comes from the window's own held state, not a constant. With `None` hardcoded
        // here a real key press could never take S2's nudge branch, however much was in hand.
        let held = ui::held_now();
        // F3.8 S2 (`Frame steps and ←/→ nudge a held effect unsnapped`): with an EFFECT in hand the arrows
        // move that record, not the line. The step is `cut_line::step_frames(shift)` frames converted to
        // seconds at the page's fps, and it lands exactly there: `nudge_held_effect` goes through
        // `effect_rules::nudge`, which never consults snap marks, so a nudged band stays precisely one frame
        // off the segment end it was beside. A clip or edge in hand keeps its existing path untouched.
        if ui::held_effect().is_some() && matches!(key, gtk::gdk::Key::Left | gtk::gdk::Key::Right) {
            let frames = cut_line::step_frames(shift) as f64 / crate::cut_fps::session_fps(&win);
            let delta = if key == gtk::gdk::Key::Left { -frames } else { frames };
            let _ = nudge_held_effect(&win, delta);
            return glib::Propagation::Stop;
        }
        // One read of the session's rate per press: every branch below steps at the same number.
        let fps = crate::cut_fps::session_fps(&win);
        let response = match key {
            gtk::gdk::Key::Left => arrow_steps(&win, shift, held, fps, now_ms()),
            gtk::gdk::Key::Right => arrow_steps(&win, shift, held, fps, now_ms()),
            gtk::gdk::Key::comma => step_the_line(&win, shift, held, fps, now_ms()),
            gtk::gdk::Key::period => step_the_line(&win, shift, held, fps, now_ms()),
            gtk::gdk::Key::space => {
                // Space is not a line step, so it answers the propagation directly rather than with a
                // `Step`; handled means the focused widget never sees it.
                return if space_toggles(&win, false) {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                };
            }
            _ => return glib::Propagation::Proceed,
        };
        // Every handled key stops the default action; `Still` deliberately does not, so an unheld
        // arrow keeps doing whatever the focused widget would have done.
        if matches!(response, Step::Still) { glib::Propagation::Proceed } else { glib::Propagation::Stop }
    });
    window.add_controller(controller.clone());
    LINE_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}
