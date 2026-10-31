//! F2.3's page half: ▶✂✂'s button face, its grey, and the tick that makes a review ADVANCE.

use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;

use crate::ui::window::find_status;
use crate::{cut, cut_play_leg, cut_review, cut_screen, preview, ui};
/// §A group 1's shipped label, spelled from the toolbar row so the two cannot drift.
pub const PLAY_LABEL: &str = crate::cut_screen::PLAY_GLYPH_REVIEW;
pub const PAUSE_LABEL: &str = "\u{23f8}\u{2702}\u{2702}";

/// The review's own state: which join is being heard and whether the line has been seen inside its window.
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

pub fn answer_press(
    window: &adw::ApplicationWindow,
    status_line: &gtk4::Label,
    pressed: &cut_review::Pressed,
    cut_: &cut::Cut,
) {
    match pressed {
        cut_review::Pressed::Started {
            status, seek_to, ..
        } => {
            crate::ui::cut_cut_play::cue_running(window, seek_to.unwrap_or_else(|| playhead_of()));
            begin(cut_review::start(cut_, playhead_of()));
            status_line.set_text(status)
        }
        cut_review::Pressed::PausedAndEnded { status } => {
            clear();
            crate::ui::cut_cut_play::pause_media();
            status_line.set_text(status)
        }
        cut_review::Pressed::Refused(reason) => status_line.set_text(reason),
    }
    crate::ui::cut_record_play::paint_transport(window);
}

/// This window's red line, or 0 for a page that has never been clicked.
fn playhead_of() -> f64 {
    ui::preview_player().playhead.unwrap_or(0.0)
}

/// Paint the button from the page's state, reading rather than remembering (§5: the UI renders state).
pub fn paint(window: &adw::ApplicationWindow) {
    let Some(button) = ui::review_cuts_button(window) else {
        return;
    };
    button.set_sensitive(cut_screen::can_review(
        &ui::window_preview::newest_review_cut(),
    ));
    let player = ui::preview_player();
    // §2's "one lit" from [`crate::tab_states::preview_lits`], the same decision ▶ and ▶✂ read; the ⏸
    // face additionally needs the transport to be playing, since a parked review owes a ▶.
    let [_recording_lit, _cut_lit, ours_lit] =
        crate::tab_states::preview_lits(&crate::cut_screen::Preview {
            cut_only: player.cut_only,
            reviewing: player.reviewing,
        });
    crate::ui::cut_record_play::set_steady_label(
        &button,
        if ours_lit && player.transport.playing {
            PAUSE_LABEL
        } else {
            PLAY_LABEL
        },
        &[PLAY_LABEL, PAUSE_LABEL],
    );
    crate::ui::cut_record_play::light_lit(window);
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

pub fn step(cut_: &cut::Cut, review: &mut cut_review::Review, from: f64) -> Outcome {
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
pub fn tick_once(window: &adw::ApplicationWindow) {
    let player = ui::preview_player();
    if !player.reviewing || !player.transport.playing {
        return;
    }
    let Some(mut review) = held().lock().expect("the review slot").take() else {
        return;
    };
    let cut_ = ui::window_preview::newest_review_cut();
    // The built-in player says where it is (F2.1 S4's "the line follows the player"); otherwise the line's own.
    let from = crate::ui::cut_record_play::playing_second()
        .or(player.playhead)
        .unwrap_or(0.0);
    match step(&cut_, &mut review, from) {
        Outcome::Stay => {
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
            ui::set_preview_state(
                true,
                false,
                crate::run::Transport {
                    playing: false,
                    started: true,
                },
            );
            crate::ui::cut_cut_play::pause_media();
            say(window, &status);
            crate::ui::cut_record_play::paint_transport(window);
        }
        Outcome::LineMoved { status } => {
            ui::set_preview_state(
                true,
                false,
                crate::run::Transport {
                    playing: true,
                    started: true,
                },
            );
            say(window, status);
            crate::ui::cut_record_play::paint_transport(window);
        }
        Outcome::NotReviewing => {}
    }
}

/// Own the review loop for this window, on the preview's one cadence.
pub fn start(window: Rc<adw::ApplicationWindow>) {
    glib::timeout_add_local(Duration::from_millis(preview::TICK_MS), move || {
        tick_once(&window);
        glib::ControlFlow::Continue
    });
}
