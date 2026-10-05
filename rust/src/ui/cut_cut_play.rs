//! F2.2's page half for ▶✂: the two states the spec puts ON the button, and the clock's own tooltip.
//!
//! The rules live in [`crate::cut_play`] (what counts as no clips, what a press means) and
//! [`crate::cut_play_leg`] (the tick). This file only reads the page and paints — no rule is decided here,
//! per spec/00-principles.md §5. Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;

use crate::ui::window::{live_player, log_line};
use crate::{cut_play, tools, ui};

/// The label the toolbar ships (§1's transport table): ▶✂ while nothing of ours runs.
pub const PLAY_LABEL: &str = "\u{25b6}\u{2702} Play the cut";
/// Inventory §D's three-▶ rule: "each wears ⏸ only while its thing runs". The run bar's ▶ takes its icon
/// from `run::controls`; this button belongs to the cut preview, so its face changes here and nowhere else.
pub const PAUSE_LABEL: &str = "\u{23f8} Pause the cut";

/// Paint the button from the page's state: S1's grey and the ⏸ face, both read rather than remembered.
///
/// S1 says "greyed with no clips", so the button itself tells the truth before any press. The press-time
/// refusal stays as the belt for the ways in that cannot switch the preview ([`crate::cut_play::EMPTY_CUT_REFUSAL`],
/// S2's sentence) — a greyed button and a named refusal are two different answers to two different moments,
/// and the spec asks for both.
pub fn paint(window: &adw::ApplicationWindow) {
    let Some(button) = ui::play_cut_button(window) else { return };
    button.set_sensitive(!cut_play::empty(&ui::review_cut_of(window)));
    let player = ui::preview_player(window);
    let ours_running = player.cut_only && !player.reviewing && player.transport.playing;
    button.set_label(if ours_running { PAUSE_LABEL } else { PLAY_LABEL });
}

/// Preroll the spare at `at`.
///
/// S5's page doors, as a group: each hands a number to [`crate::cut_play_back`], which owns the processes,
/// and nothing here decides anything (spec/00-principles.md §5). They live in this file rather than in
/// `window.rs`'s `CutPreviewSlot` impl because that file is past its size budget — the impl forwards. The
/// page names the footage (only it can reach the session's source list — F2.1's `files_for`), the leg owns
/// the process; a refusal is logged where the press's other lines go and answers false, leaving the tick's
/// arithmetic untouched, because no spare means every join is a reload, as it was. `freeze`/`release_spare`
/// take no window: the leg's slots are process-wide, for `play_back`'s reason — a child outlives the call
/// that started it.
pub fn preroll_spare(window: &adw::ApplicationWindow, at: f64) -> bool {
    let files = ui::cut_record_play::files_for(window, &ui::cut_record_play::runs_now());
    crate::cut_play_back::preroll(&files, at, &|line| log_line(line)).is_ok()
}

/// The join arrived and the spare was sitting on it: swap it in. Answers whether one was there to swap.
pub fn swap_spare(window: &adw::ApplicationWindow) -> bool {
    let _ = window;
    crate::cut_play_back::swap_spare_in(&|line| log_line(line)).is_some()
}

/// Release the spare: pausing, or the cut changing under it, takes the waiting process back with it.
pub fn release_spare() {
    crate::cut_play_back::drop_spare();
}

/// S5 "stops show their still": freeze the running player's frame, or release it.
pub fn freeze(hold: bool) -> bool {
    crate::cut_play_back::hold_still(hold)
}

/// S4's pause, from the ▶✂ press: stop the cut's player AND release its spare. A paused preview that keeps
/// a prerolled pipeline is a process holding a file for a jump that will now never happen.
pub fn pause_media() {
    crate::cut_play_back::halt();
}

/// S5 "speed effects at their flat rate", handed to the player at the head. What the player's argv can and
/// cannot do about a rate is [`crate::cut_play_back::set_rate`]'s honest answer, not this door's.
pub fn hand_rate(window: &adw::ApplicationWindow, rate: f64, at: f64) -> bool {
    let files = ui::cut_record_play::files_for(window, &ui::cut_record_play::runs_now());
    crate::cut_play_back::set_rate(&files, rate, at, &|line| log_line(line))
}

/// S5 "volume effects apply": the player's own loudness for these seconds. `window::set_preview_volume` is
/// the slider's door and would move the SLIDER, which owns the room's loudness — `cut_play_leg::route`
/// already multiplied it in, so writing a gain back as a percent would move the slider out from under the
/// hand that set it. This writes the player's own `volume` property instead (the muted-by-property shape F2.5
/// S6 established) and leaves the scale alone.
pub fn hand_gain(window: &adw::ApplicationWindow, gain: f64) {
    if let Some(player) = live_player(window) {
        player.borrow_mut().volume = Some(gain);
    }
}

/// §D's tooltip for the ✂ clock, verbatim in shape: which second of the FINISHED video the line is on,
/// how long that video is, and the session second it corresponds to — because under ▶✂ the face stops
/// reading the session and a person needs to be able to translate back.
pub fn clock_tooltip(into_cut: f64, of_cut: f64, session_t: f64) -> String {
    format!(
        "{} into the cut, of {} \u{2014} the finished video's own clock (the \u{25b6}\u{2702} preview), \
         the speed effects included. Session time here is {}.",
        tools::tenths(into_cut),
        tools::tenths(of_cut),
        tools::tenths(session_t)
    )
}

/// Put that sentence on the button's tooltip — the ✂ clock has no widget of its own on this page today
/// (see [`crate::cut_play_leg::start_cut_preview_tick`]'s note), and the transport button whose press set
/// the clock reading is where the explanation belongs.
pub fn set_clock_tooltip(window: &adw::ApplicationWindow, into_cut: f64, of_cut: f64, session_t: f64) {
    if let Some(button) = ui::play_cut_button(window) {
        button.set_tooltip_text(Some(&clock_tooltip(into_cut, of_cut, session_t)));
    }
}
