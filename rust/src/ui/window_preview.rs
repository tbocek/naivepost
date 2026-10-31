//! §F2.1–§F2.3 / §F2.5 S6 / §F2.8–§F2.9: the preview player and review cut: named widget doors, test reads, and the ▶, ▶✂, ▶✂✂ presses.
//! Re-exported through `window`. Widget names (`play-recording-button`, `play-cut-button`, `review-cuts-button`, `preview-volume`) are found by tests.

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
    /// One window's preview state, beside `SESSION`; newest slot wins.
    static PREVIEW_PLAYERS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Player>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub fn play_recording_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-recording-button")?
        .downcast()
        .ok()
}

pub fn play_cut_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-cut-button")?
        .downcast()
        .ok()
}

pub fn review_cuts_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "review-cuts-button")?
        .downcast()
        .ok()
}

pub fn review_cut_segs() -> Vec<cut::Seg> {
    newest_review_cut().segs
}

pub fn review_cut_segs_count() -> usize {
    newest_review_cut().segs.len()
}

/// This window's shift correction (F2.8 S2), so a test can assert WHICH sources moved and by how much.
pub fn review_cut_shift() -> std::collections::BTreeMap<String, f64> {
    newest_review_cut().shift.clone()
}

pub fn review_lanes() -> Vec<cut::Lane> {
    newest_review_cut().lanes
}

pub fn preview_player() -> Player {
    PREVIEW_PLAYERS.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|player| *player.borrow())
            .unwrap_or_default()
    })
}

/// This window's live preview handle, the newest registered player; shared by every accessor below.
pub(crate) fn live_player() -> Option<Rc<RefCell<Player>>> {
    PREVIEW_PLAYERS.with(|slots| slots.borrow().last().cloned())
}

/// Give this window its own preview slot. Narrate registers it from `narrate_page::wire` too, since §5 makes the run bar read that page's transport.
pub fn register_preview_player_slot() {
    PREVIEW_PLAYERS.with(|slots| {
        slots
            .borrow_mut()
            .push(Rc::new(std::cell::RefCell::new(Player::default())))
    });
}

/// Publish what the run bar draws for this window's preview ([`crate::run::controls`] reads the transport off the player). No-op with no slot.
pub fn publish_preview_transport(transport: run::Transport) {
    if let Some(player) = live_player() {
        player.borrow_mut().transport = transport;
    }
}

/// Put the red line somewhere before a press; F2.1 S2 plays from here.
pub fn set_playhead(window: &adw::ApplicationWindow, at: f64) {
    if let Some(player) = live_player() {
        player.borrow_mut().playhead = Some(at);
    }
    // §1 has ONE red line kept in two slots (the player's, read by ▶ and the band; the line's, read by picture, steps and save), so a move writes both.
    crate::ui::window_line_step::hold_line_at(at);
    show_playhead_reading(window, at);
    crate::ui::cut_view::follow_playhead(window, at);
    crate::ui::cut_picture::refresh(window);
    // The tick writes the playhead; the playing half reads the smoothed clock at paint time, which keeps the mask and camera layer on the picture between ticks.
    crate::ui::cut_preview_frame::repaint(window);
}

/// The form column's Playhead reading (§1 badge 4), written by every door that moves the line.
pub(crate) fn show_playhead_reading(window: &adw::ApplicationWindow, at: f64) {
    let name = crate::cut_screen::readout_widget("Playhead");
    if let Some(label) = crate::ui::window::find_widget_by_name(window.upcast_ref(), &name)
        .and_then(|widget| widget.downcast::<gtk::Label>().ok())
    {
        label.set_text(&crate::preview::clock(Some(at)));
    }
}

pub fn set_preview_state(cut_only: bool, reviewing: bool, transport: run::Transport) {
    if let Some(player) = live_player() {
        let mut held = player.borrow_mut();
        held.cut_only = cut_only;
        held.reviewing = reviewing;
        held.transport = transport;
    }
}

/// The seam F2.1's button calls: [`preview::press_recording`] decides what the press means; this forwards it and paints only what the answer says.
/// `runs` are [`crate::timeline::filmed_runs`]; nothing filmed means nothing to start at.
/// The player's `Transport` is deliberately NOT pushed into [`run::RunBar`], which tracks a *run*, not a preview transport.
pub fn press_play_recording(window: &adw::ApplicationWindow, runs: &[(f64, f64)]) -> Press {
    let Some(status) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = PREVIEW_PLAYERS.with(|slots| slots.borrow().last().cloned()) else {
        panic!("the window has no preview player");
    };
    let pressed = preview::press_recording(&mut player.borrow_mut(), runs);
    match pressed {
        // S1: the clock changed meaning, so say so.
        Press::SwitchedToRecording { .. } => status.set_text(preview::RECORDING_STATUS),
        // S2: play/pause need no sentence; the bar's progress owns the line while running.
        Press::Playing { .. } | Press::Paused => {}
        Press::NoFootage => status.set_text(NO_RECORDING_STATUS),
    }
    pressed
}

/// This window's preview-volume slider (F2.5 S6); `None` on a page that drew none.
pub fn preview_volume_scale(window: &adw::ApplicationWindow) -> Option<gtk::Scale> {
    find_widget_by_name(window.upcast_ref(), "preview-volume")?
        .downcast()
        .ok()
}

/// The seam F2.2's ▶✂ calls: [`cut_play::pressed`] decides whether the press switches to the cut, ends a review or toggles; this forwards it and paints only what the answer says.
/// `segs` ([`crate::cut::Cut::segs`]) are not yet reachable from the window, so an empty list is what the button knows: the S1 refusal.
pub fn press_play_cut(window: &adw::ApplicationWindow, cut: &cut::Cut) -> cut_play::Pressed {
    let Some(status) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player() else {
        panic!("the window has no preview player");
    };
    let pressed = cut_play::pressed(&mut player.borrow_mut(), cut);
    let released = hand_preview_back(window);
    if let Some(back) = &released {
        status.set_text(back);
    }
    // A row release already printed above keeps the line; otherwise this press's sentence goes there.
    match &pressed {
        // S2/S3: both change what the clock means or what is playing, so both get a sentence, and ▶✂ owns the sound player, so the press starts it (`cut_play_back::cue_clips` has no other door).
        cut_play::Pressed::SwitchedToCut { status: line, .. } => {
            crate::ui::cut_cut_play::cue_running(window, preview_playhead());
            if released.is_none() {
                status.set_text(line)
            }
        }
        cut_play::Pressed::ReviewEnded { status: ended } => {
            crate::ui::cut_cut_play::cue_running(window, preview_playhead());
            if released.is_none() {
                status.set_text(ended)
            }
        }
        cut_play::Pressed::Refused(line) => status.set_text(line),
        // S4: the toggle needs no sentence. A pause takes the cut's processes with it; a resume re-cues at the second the press snapped to (`Toggle::Playing`).
        cut_play::Pressed::Toggled(cut_play::Toggle::Paused) => {
            crate::ui::cut_cut_play::pause_media()
        }
        cut_play::Pressed::Toggled(cut_play::Toggle::Playing { from }) => {
            crate::ui::cut_cut_play::cue_running(window, *from);
        }
    }
    pressed
}

/// Put this window's cut where ▶✂✂ can read it; until then `build_window` seeds an empty one and a press answers S1's refusal.
pub fn seed_review_cut(cut_: &cut::Cut) {
    REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = cut_.clone();
        }
    });
    // F2.10/F2.11: rows, fold badges and ⇲ Lane's "anything rolling" read this page's session spans, so a cut seeded after open moves them. A session DECLARED through `set_session_recordings` is never rewritten.
    if !crate::ui::cut_folds::session_declared() {
        crate::ui::cut_folds::set_session_recordings(&crate::ui::cut_session::session_or_kept(
            cut_,
        ));
    }
}

pub fn newest_review_cut() -> cut::Cut {
    REVIEW_CUTS
        .with(|slots| slots.borrow().last().map(|slot| slot.borrow().clone()))
        .unwrap_or_default()
}

/// The seam F2.3's ▶✂✂ calls: [`cut_review::pressed`] decides start, refusal or pause-and-end; the answer goes to `cut_review_play::answer_press`. Every sentence comes from `cut_review`.
pub fn press_review_cuts(window: &adw::ApplicationWindow, cut_: &cut::Cut) -> cut_review::Pressed {
    let Some(status_line) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player() else {
        panic!("the window has no preview player");
    };
    let pressed = cut_review::pressed(&mut player.borrow_mut(), cut_);
    crate::ui::cut_review_play::answer_press(window, &status_line, &pressed, cut_);
    pressed
}
