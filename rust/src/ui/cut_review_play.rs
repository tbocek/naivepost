//! F2.3's page half: ▶✂✂'s button face, its grey, and the tick that makes a review ADVANCE.
//!
//! [`crate::cut_review`] holds every rule — which joins there are, how far each is heard, where a press at
//! the red line starts, what one tick of the line means (`Move::{Stay, Seek, Done, Lost}`) and the exact
//! sentence for each. None of that belongs here. What was missing, and what this file supplies, is the hand
//! that feeds the line back into `cut_review::tick` ten times a second: without it the rules were complete
//! but unreachable — the status printed "reviewing cut 1 of 2" and then nothing played, no join was ever
//! sought, and neither `reviewed_all` nor `LINE_MOVED` could be reached by any user action.
//!
//! The clock arithmetic is not written twice either: the head advances through [`crate::cut_play_leg::tick`],
//! the same function ▶✂ uses, so removed stretches are skipped identically under both buttons ("play as the
//! cut", S3). This module only sequences: move the line, ask `cut_review` what that means, do what it says.

use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;

use crate::{cut, cut_play_leg, cut_review, cut_screen, preview, ui};
use crate::ui::window::find_status;/// §A group 1's shipped label, spelled from the toolbar row so the two cannot drift.
pub const PLAY_LABEL: &str = "\u{25b6}\u{2702}\u{2702} Review every cut";
/// §F2.3's one rule for the three buttons: "each wears ⏸ only while its own thing runs … exactly one is
/// lit". So the pause face appears here ONLY while a review is the running preview, and ▶/▶✂ give theirs
/// back at the same moment (each module reads the player rather than remembering).
pub const PAUSE_LABEL: &str = "\u{23f8} Pause every cut";

/// The review's own state: which join is being heard and whether the line has been seen inside its window.
/// One slot per process because one preview plays at a time — the same reason [`crate::cut_play_back`]'s
/// player slots are process-wide. A test drives it through `begin`, exactly as a press does.
fn held() -> &'static std::sync::Mutex<Option<cut_review::Review>> {
    static HELD: std::sync::OnceLock<std::sync::Mutex<Option<cut_review::Review>>> =
        std::sync::OnceLock::new();
    HELD.get_or_init(|| std::sync::Mutex::new(None))
}

/// Start owning a review. Called by the press with what [`cut_review::start`] decided about the red line.
pub fn begin(review: cut_review::Review) {
    *held().lock().expect("the review slot") = Some(review);
}

/// Stop owning one (S4's end, S5's hand on the line, S5's own pause).
pub fn clear() {
    *held().lock().expect("the review slot") = None;
}

/// Whether a review is being owned. A TEST's look at the slot — the program reads it only inside
/// [`tick_once`], which consumes the state itself.
pub fn armed_for_test() -> bool {
    held().lock().expect("the review slot").is_some()
}

/// What the press did to the page: every arm of [`cut_review::pressed`] answered in one place, so the shell
/// forwards and this module owns the reaction (§5). Lives here rather than in `window.rs`, which is past its
/// size budget.
pub fn answer_press(
    window: &adw::ApplicationWindow,
    status_line: &gtk4::Label,
    pressed: &cut_review::Pressed,
    cut_: &cut::Cut,
) {
    match pressed {
        // S4: the status names which join out of how many is being heard. The review IS the cut preview with
        // one more hand on the line, so its press starts the player and hands the line to the review's tick,
        // which cues each later run-up as it seeks (S3).
        cut_review::Pressed::Started { status, seek_to, .. } => {
            // The footage is the session's own files; a test names them through `cue_files` instead.
            let files = ui::cut_record_play::files_for(window, &ui::cut_record_play::runs_now());
            crate::ui::cut_cut_play::cue_files(&files, seek_to.unwrap_or_else(|| playhead_of(window)));
            begin(cut_review::start(cut_, playhead_of(window)));
            status_line.set_text(status)
        }
        // S5: pausing ends the review, and says so with the module's own short line.
        cut_review::Pressed::PausedAndEnded { status } => {
            clear();
            status_line.set_text(status)
        }
        // S1: fewer than two clips, in `refused`'s own words.
        cut_review::Pressed::Refused(reason) => status_line.set_text(reason),
    }
    paint(window);
}

/// This window's red line, or 0 for a page that has never been clicked.
fn playhead_of(window: &adw::ApplicationWindow) -> f64 {
    ui::preview_player(window).playhead.unwrap_or(0.0)
}

/// Paint the button from the page's state, reading rather than remembering (§5: the UI renders state).
///
/// Two things the spec puts ON this widget: S1's grey ("greyed with fewer than two clips") and the ⏸ face.
/// The press-time refusal stays as well — a greyed button and a named refusal answer two different moments.
pub fn paint(window: &adw::ApplicationWindow) {
    let Some(button) = ui::review_cuts_button(window) else {
        return;
    };
    button.set_sensitive(cut_screen::can_review(&ui::review_cut_of(window)));
    let player = ui::preview_player(window);
    // §2's "one lit" from [`crate::tab_states::preview_lits`], the same decision ▶ and ▶✂ read; the ⏸
    // face additionally needs the transport to be playing, since a parked review owes a ▶.
    let [_recording_lit, _cut_lit, ours_lit] =
        crate::tab_states::preview_lits(&crate::cut_screen::Preview {
            cut_only: player.cut_only,
            reviewing: player.reviewing,
        });
    button.set_label(if ours_lit && player.transport.playing { PAUSE_LABEL } else { PLAY_LABEL });
}

/// What one review tick did, so a test can assert the outcome without a window or a clock.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Still hearing this join, or the review's own seek has not landed yet.
    Stay,
    /// S3: go to the next join's run-up. `to` is where the line goes, in session seconds.
    SeekTo { to: f64, status: String },
    /// S4: the last join has been heard — pause and say so.
    Finished { status: String },
    /// S5: somebody moved the line.
    LineMoved { status: &'static str },
    /// No review is being owned: this tick owes nothing.
    NotReviewing,
}

/// One tick's whole decision, over plain numbers instead of a window: advance the head the way ▶✂ does,
/// then ask [`cut_review::tick`] what the new line means. `tick_once` is the thin window wrapper around it;
/// keeping them apart is what lets S3/S4/S5 be tested without waiting on a timer.
pub fn step(cut_: &cut::Cut, review: &mut cut_review::Review, from: f64) -> Outcome {
    // Same arithmetic as the plain cut preview, so a removed stretch is skipped identically under both.
    let head = cut_play_leg::tick(cut_, from, preview::TICK_MS).head;
    match cut_review::tick(cut_, review, head) {
        cut_review::Move::Stay => Outcome::Stay,
        cut_review::Move::Seek => Outcome::SeekTo {
            // The run-up of the join `cut_review::tick` just advanced to — clamped to the clips, since what
            // lies before an earlier clip is a removed stretch rather than the cut.
            to: cut_review::window(&cut_.segs, review.join).from,
            status: cut_review::reviewing_status(cut_, review.join),
        },
        cut_review::Move::Done => Outcome::Finished {
            status: cut_review::reviewed_all(cut_),
        },
        cut_review::Move::Lost => Outcome::LineMoved {
            status: cut_review::LINE_MOVED,
        },
    }
}

/// Write a sentence to the page's status row (the Cut page has no review-specific readout; the status line
/// is where every preview sentence goes, §D).
fn say(window: &adw::ApplicationWindow, line: &str) {
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(line);
    }
}

/// One tick of the review against THIS window: move the line, then act on what the rules answered.
/// Returns early unless a review is running, so the loop costs nothing while ▶ or ▶✂ owns the preview.
pub fn tick_once(window: &adw::ApplicationWindow) {
    let player = ui::preview_player(window);
    if !player.reviewing || !player.transport.playing {
        return;
    }
    let Some(mut review) = held().lock().expect("the review slot").take() else {
        return;
    };
    let cut_ = ui::review_cut_of(window);
    let from = player.playhead.unwrap_or(0.0);
    match step(&cut_, &mut review, from) {
        Outcome::Stay => {
            // Head may have moved even when the join did not; keep it and the review.
            let head = cut_play_leg::tick(&cut_, from, preview::TICK_MS).head;
            ui::set_playhead(window, head);
            *held().lock().expect("the review slot") = Some(review);
        }
        Outcome::SeekTo { to, status } => {
            // The real player jumps to the run-up: with a subprocess media leg a seek IS a re-cue at that
            // second, and that is what "plays 10 s before and after each join" means here.
            ui::set_playhead(window, to);
            crate::ui::cut_cut_play::cue_running(window, to);
            say(window, &status);
            *held().lock().expect("the review slot") = Some(review);
        }
        Outcome::Finished { status } => {
            // S4: pause, and the processes go with the pause (F2.2 S4's door releases player and spare).
            ui::set_preview_state(
                window,
                true,
                false,
                crate::run::Transport { playing: false, started: true },
            );
            crate::ui::cut_cut_play::pause_media();
            say(window, &status);
            paint(window);
        }
        Outcome::LineMoved { status } => {
            // S5's hand on the line ENDS the review. DECISION (the spec names the sentence, not the
            // transport): the cut preview keeps playing, because the same chapter says pressing another of
            // the three switches the preview WITHOUT stopping — a person who moved the line wants to watch
            // where they moved it to, not a paused picture. The review simply stops owning the line.
            ui::set_preview_state(
                window,
                true,
                false,
                crate::run::Transport { playing: true, started: true },
            );
            say(window, status);
            paint(window);
        }
        Outcome::NotReviewing => {}
    }
}

/// Own the review loop for this window, on the preview's one cadence.
///
/// It is a separate loop from [`crate::cut_play_leg::start_cut_preview_tick`] because that one answers for
/// ▶✂'s own needs (spare arms, stills, rates) and returns unless the preview is the PLAIN cut; a review is
/// `cut_only` AND `reviewing`, and folding a second gate into that loop would decide the review's flow from
/// inside the cut preview's. Both loops run at `preview::TICK_MS` and each no-ops for the other's mode.
pub fn start(window: Rc<adw::ApplicationWindow>) {
    glib::timeout_add_local(Duration::from_millis(preview::TICK_MS), move || {
        tick_once(&window);
        glib::ControlFlow::Continue
    });
}
