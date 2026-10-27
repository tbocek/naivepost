//! §F2.1–§F2.3 / §F2.5 S6 / §F2.8–§F2.9 — the preview player and the review cut: the named widget
//! doors, the reads a test asserts against, and the three presses ▶, ▶✂ and ▶✂✂ forward through.
//! Split out of `window.rs`, which is past its size budget; its doors are re-exported through
//! `window`, so no caller's path changed. The stable widget names (`play-recording-button`,
//! `play-cut-button`, `review-cuts-button`, `preview-volume`) come along unchanged because tests find
//! those widgets by name.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{self};
use crate::cut_play;
use crate::cut_review;
use crate::preview::{self, Player, Press};
use crate::run;
use crate::ui::cut_cameras::hand_preview_back;
use crate::ui::window::{find_status, find_widget_by_name, NO_RECORDING_STATUS, REVIEW_CUTS};
use crate::ui::window_selection::preview_playhead;

thread_local! {
    /// One window's preview state, pushed beside `SESSION` so each window owns its own; newest slot
    /// wins — the same arrangement `session_sources` reads.
    static PREVIEW_PLAYERS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Player>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub fn play_recording_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-recording-button")?
        .downcast()
        .ok()
}

/// The Cut page's ▶✂ (F2.2), found by name the way [`play_recording_button`] finds ▶.
pub fn play_cut_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-cut-button")?
        .downcast()
        .ok()
}

/// The Cut page's ▶✂✂ (F2.3), found by name the way [`play_cut_button`] finds ▶✂.
pub fn review_cuts_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "review-cuts-button")?
        .downcast()
        .ok()
}

/// F2.8 S1: the strip's own cut, read back — the visible state a trim writes and the painter reads.
pub fn review_cut_segs(window: &adw::ApplicationWindow) -> Vec<cut::Seg> {
    let _ = window;
    newest_review_cut().segs
}

/// How many segments this window's cut holds right now — the read side of `seed_review_cut`, so a test can
/// see whether a gesture changed the list.
pub fn review_cut_segs_count(window: &adw::ApplicationWindow) -> usize {
    let _ = window;
    newest_review_cut().segs.len()
}

/// This window's shift correction (F2.8 S2), read straight off the cut the gesture wrote. A slide of a
/// recording or a row lands here, so a test can assert WHICH sources moved and by how much instead of
/// trusting the status sentence alone.
pub fn review_cut_shift(
    window: &adw::ApplicationWindow,
) -> std::collections::BTreeMap<String, f64> {
    let _ = window;
    newest_review_cut().shift.clone()
}

/// This window's lanes (F2.9 S3), by their stable read: ⇲ Lane adds one and cuts nothing, so a test
/// asserts the lane arrived while `review_cut_segs_count` stayed put.
pub fn review_lanes(window: &adw::ApplicationWindow) -> Vec<cut::Lane> {
    let _ = window;
    newest_review_cut().lanes
}

/// F2.13 S3: the whole cut this window holds, as a read seam. Clear takes the scenes and the effects off
/// and keeps everything recording-side, so a check has to read `fx`, `rows` and `nrows` too — the three
/// the narrower accessors above do not cover. Reading only: no rule lives here.
pub fn review_cut_of(window: &adw::ApplicationWindow) -> cut::Cut {
    let _ = window;
    newest_review_cut()
}

/// This window's preview as it currently stands — for a test that fired the real button and wants to
/// check the same state the logic test checks rather than a painted pixel.
pub fn preview_player(window: &adw::ApplicationWindow) -> Player {
    let _ = window;
    PREVIEW_PLAYERS.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|player| *player.borrow())
            .unwrap_or_default()
    })
}

/// This window's live preview handle — the newest registered player, which is the one this window's
/// button moves. Shared by every accessor below so they all read and write the same object.
pub(crate) fn live_player(window: &adw::ApplicationWindow) -> Option<Rc<RefCell<Player>>> {
    let _ = window;
    PREVIEW_PLAYERS.with(|slots| slots.borrow().last().cloned())
}

/// Give this window its own preview slot. The Cut page gets one with its ▶ (F2.1); the Narrate page
/// registers the same slot from `narrate_page::wire`, because §5 makes the run bar read THAT page's
/// transport once its preview has started, and a bar can only paint a slot that exists.
pub fn register_preview_player_slot() {
    PREVIEW_PLAYERS.with(|slots| {
        slots
            .borrow_mut()
            .push(Rc::new(std::cell::RefCell::new(Player::default())))
    });
}

/// Publish what the run bar should draw for this window's preview: [`crate::run::controls`] reads the
/// transport off the player, so the page that owns the playing state writes it here rather than the
/// bar guessing. Silently does nothing with no slot — a page that never registered one has no preview.
pub fn publish_preview_transport(window: &adw::ApplicationWindow, transport: run::Transport) {
    if let Some(player) = live_player(window) {
        player.borrow_mut().transport = transport;
    }
}

/// Put the red line somewhere before a press, which is what placing the cursor on the timeline does in
/// the finished page. F2.1 S2 plays from here, so a test seeds it rather than depending on a click
/// landing on a track that this round does not draw.
pub fn set_playhead(window: &adw::ApplicationWindow, at: f64) {
    if let Some(player) = live_player(window) {
        player.borrow_mut().playhead = Some(at);
    }
    // §1 has ONE red line. The page keeps the line's second in two slots (the player's, which ▶ and the band
    // read, and the line's, which the picture, the steps and the save read), so a move through either door
    // writes both: a tick that moved only this one left the picture standing while the line ran.
    crate::ui::window_line_step::hold_line_at(at);
    show_playhead_reading(window, at);
    crate::ui::cut_view::follow_playhead(window, at);
    // The picture at the line follows it, ten times a second while ▶ runs (F2.1 S4).
    crate::ui::cut_picture::refresh(window);
    // The tick writes the playhead; the playing half reads the smoothed clock at paint time, so this is what
    // keeps the black mask and the camera layer from drifting off the picture between ticks.
    crate::ui::cut_preview_frame::repaint(window);
}

/// The form column's Playhead reading (§1 badge 4): the red line's second on the preview's clock face, written by
/// every door that moves the line.
pub(crate) fn show_playhead_reading(window: &adw::ApplicationWindow, at: f64) {
    let name = crate::cut_screen::readout_widget("Playhead");
    if let Some(label) = crate::ui::window::find_widget_by_name(window.upcast_ref(), &name)
        .and_then(|widget| widget.downcast::<gtk::Label>().ok())
    {
        label.set_text(&crate::preview::clock(Some(at)));
    }
}

/// Stand the preview in one of its three modes (recording / ✂ cut / ▶✂✂ review), playing or not,
/// before a press — which is what the ✂ toggles and the transport itself do on the finished page.
pub fn set_preview_state(
    window: &adw::ApplicationWindow,
    cut_only: bool,
    reviewing: bool,
    transport: run::Transport,
) {
    if let Some(player) = live_player(window) {
        let mut held = player.borrow_mut();
        held.cut_only = cut_only;
        held.reviewing = reviewing;
        held.transport = transport;
    }
}

/// The seam F2.1's button calls: [`preview::press_recording`] decides which of switch-to-the-
/// recording, pause or play-from-the-red-line the press means; this forwards it and paints only what
/// the answer says.
///
/// `runs` are the filmed stretches ([`crate::timeline::filmed_runs`]) — needed because a session
/// with nothing filmed has nothing to start at, and starting at 0 would pretend otherwise.
///
/// The player's own `Transport` is deliberately NOT pushed into the shell's [`run::RunBar`]: the bar
/// tracks a *run* (`Run { step, paused, .. }`), not a preview transport, and folding one into the
/// other would mean changing its API for a state it has no rule about. The two stay separate until a
/// round owns both.
pub fn press_play_recording(window: &adw::ApplicationWindow, runs: &[(f64, f64)]) -> Press {
    let Some(status) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = PREVIEW_PLAYERS.with(|slots| slots.borrow().last().cloned()) else {
        panic!("the window has no preview player");
    };
    let pressed = preview::press_recording(&mut player.borrow_mut(), runs);
    match pressed {
        // S1: the clock just changed meaning, so the sentence that says so goes where a press's
        // answer is read.
        Press::SwitchedToRecording { .. } => status.set_text(preview::RECORDING_STATUS),
        // S2: playing and pausing need no sentence — the bar's own progress owns the line while a
        // preview runs, and a pause leaves the previous one standing.
        Press::Playing { .. } | Press::Paused => {}
        Press::NoFootage => status.set_text(NO_RECORDING_STATUS),
    }
    pressed
}

/// This window's preview-volume slider, by its stable name (F2.5 S6). The seam a test fires and the
/// page reads; `None` on a page that drew none.
pub fn preview_volume_scale(window: &adw::ApplicationWindow) -> Option<gtk::Scale> {
    find_widget_by_name(window.upcast_ref(), "preview-volume")?
        .downcast()
        .ok()
}

/// The seam F2.2's ▶✂ calls: [`cut_play::pressed`] decides whether the press switches to the cut,
/// ends a review or toggles play/pause; this forwards it and paints only what the answer says.
///
/// `segs` are the cut's own segments ([`crate::cut::Cut::segs`]). Like ▶'s `runs`, they are not yet
/// reachable from the window — the Cut page holds no live cut model until its own round — so an empty
/// list is what the button knows today, which is exactly the S1 case: nothing to skip to, refused.
pub fn press_play_cut(window: &adw::ApplicationWindow, cut: &cut::Cut) -> cut_play::Pressed {
    let Some(status) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player(window) else {
        panic!("the window has no preview player");
    };
    let pressed = cut_play::pressed(&mut player.borrow_mut(), cut);
    // F2.10 S3: ▶ is what takes the preview back from a watched row. The release runs FIRST and its
    // sentence wins over `cut_play`'s own, because "▶ plays the cut — <row> no longer watched" answers
    // the click the user just made; a press that released nothing leaves the status as `cut_play`
    // answered it. (It used to run before the match below and be overwritten by it.)
    let released = hand_preview_back(window);
    if let Some(back) = &released {
        status.set_text(back);
    }
    // The outer `status` is the status Label, so each arm binds its own name and paints from it. A row
    // release already printed above keeps the line; otherwise this press's own sentence goes there.
    match &pressed {
        // S2/S3: both change what the clock means or what is playing, so both get their sentence. Both also
        // need a process making sound from the line the press just snapped to: ▶✂ owns that player, so the
        // press starts it (`cut_play_back::cue_clips` has no other program door).
        // F2.2 S2: the switch plays: carried on when it was playing, started from the line when it was paused.
        cut_play::Pressed::SwitchedToCut { status: line, .. } => {
            crate::ui::cut_cut_play::cue_running(window, preview_playhead(window));
            if released.is_none() {
                status.set_text(line)
            }
        }
        cut_play::Pressed::ReviewEnded { status: ended } => {
            crate::ui::cut_cut_play::cue_running(window, preview_playhead(window));
            if released.is_none() {
                status.set_text(ended)
            }
        }
        cut_play::Pressed::Refused(line) => status.set_text(line),
        // S4: the toggle needs no sentence — the button's own face carries it. A pause takes the cut's
        // processes back with it (the player and the prerolled spare); a resume re-cues at the second the
        // press snapped to, which `Toggle::Playing` names rather than leaving this seam to guess.
        cut_play::Pressed::Toggled(cut_play::Toggle::Paused) => {
            crate::ui::cut_cut_play::pause_media()
        }
        cut_play::Pressed::Toggled(cut_play::Toggle::Playing { from }) => {
            crate::ui::cut_cut_play::cue_running(window, *from);
        }
    }
    pressed
}

/// Put this window's cut where ▶✂✂ can read it — the seam the cut-model round plugs into. Until then
/// `build_window` seeds an empty one and a press answers S1's refusal.
pub fn seed_review_cut(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    let _ = window;
    REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = cut_.clone();
        }
    });
    // F2.10/F2.11: the row list, the fold badges and ⇲ Lane's "is anything rolling at m:ss" all read this
    // page's session spans, so a cut seeded after the window opened moves them — until someone declares the
    // real takes, what the page holds is only what the cut it OPENED implied, and the newer fact is this one.
    // A session DECLARED through `set_session_recordings` is never rewritten: its heads, tails and holes are
    // what fold badges and row-emptiness are asked against, and no kept clip can imply them.
    if !crate::ui::cut_folds::session_declared() {
        crate::ui::cut_folds::set_session_recordings(&crate::ui::cut_session::session_or_kept(
            cut_,
        ));
    }
}

/// The newest cut this window holds, or an empty one when nothing has been seeded.
pub(crate) fn newest_review_cut() -> cut::Cut {
    REVIEW_CUTS
        .with(|slots| slots.borrow().last().map(|slot| slot.borrow().clone()))
        .unwrap_or_default()
}

/// The seam F2.3's ▶✂✂ calls: [`cut_review::pressed`] decides whether the press starts the review from
/// the red line, refuses for want of a join, or pauses-and-ends a running one; this forwards it and hands
/// the answer to [`cut_review_play::answer_press`], which owns what each arm does to the page. Every
/// sentence comes from `cut_review` itself.
pub fn press_review_cuts(window: &adw::ApplicationWindow, cut_: &cut::Cut) -> cut_review::Pressed {
    let Some(status_line) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player(window) else {
        panic!("the window has no preview player");
    };
    let pressed = cut_review::pressed(&mut player.borrow_mut(), cut_);
    crate::ui::cut_review_play::answer_press(window, &status_line, &pressed, cut_);
    pressed
}
