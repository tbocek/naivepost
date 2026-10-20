//! F2.2's page half for ▶✂: the two states the spec puts ON the button, and the clock's own tooltip.
//!
//! The rules live in [`crate::cut_play`] (what counts as no clips, what a press means) and
//! [`crate::cut_play_leg`] (the tick). This file only reads the page and paints — no rule is decided here,
//! per spec/00-principles.md §5. Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;

use crate::ui::window::{live_player, log_line};
use crate::{cut_play, tools, ui};

/// The label the toolbar ships (§1's transport table): ▶✂ while nothing of ours runs.
pub const PLAY_LABEL: &str = crate::cut_screen::PLAY_GLYPH_CUT;
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
    // §2's "one lit" from [`crate::tab_states::preview_lits`], the same decision the other two buttons
    // read; ⏸ only while this mode is the lit one AND the transport is actually playing.
    let [_recording_lit, ours_lit, _review_lit] =
        crate::tab_states::preview_lits(&crate::cut_screen::Preview {
            cut_only: player.cut_only,
            reviewing: player.reviewing,
        });
    button.set_label(if ours_lit && player.transport.playing { PAUSE_LABEL } else { PLAY_LABEL });
}

/// Paint BOTH skipping buttons — ▶✂ and ▶✂✂ — from this window's cut. They read the same cut slot, and the
/// slot is only filled after the page's first refresh, so the pair is repainted together from one door; the
/// review's own rules stay in [`crate::ui::cut_review_play`].
pub fn paint_both_skipping_buttons(window: &adw::ApplicationWindow) {
    paint(window);
    crate::ui::cut_review_play::paint(window);
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
///
/// F3.6 S5: and it carries the number to the process that makes sound. [`crate::play_back::set_volume`] asks
/// playbin's volume PROPERTY at `at_secs` — a seek, never a stop — which is what lets a gain follow an effect
/// without tearing the stream down; when nothing is running or the answer has not changed it does nothing.
pub fn hand_gain(window: &adw::ApplicationWindow, gain: f64, at_secs: f64) {
    if let Some(player) = live_player(window) {
        player.borrow_mut().volume = Some(gain);
    }
    // Only a RUNNING process may be asked, and only one THIS leg started: `play_back::cue` is ▶'s door (F2.1 —
    // the press is what starts this process), so a gain arriving while nothing plays must not cue a player nobody
    // asked for. It would also re-cue at the second the gain was named in rather than at the second ▶ chose,
    // which is how a parked line once moved the red line's `-s` by a tenth of a second.
    if crate::play_back::playing_pid().is_some() {
        crate::play_back::set_volume(gain, at_secs);
    }
}

/// Start (or resume) the cut preview's RUNNING player at `at`, replacing whatever was playing.
///
/// The press is the only door that starts this process: [`crate::cut_play_back::preroll`] makes the SPARE and
/// [`crate::cut_play_back::set_rate`] only ever replaces a player that already exists, so without this call ▶✂
/// runs its clock arithmetic and arms a spare while nothing plays the first clip. It cues at the session second
/// the press landed on — the snapped line `cut_play::pressed` just wrote — for the same reason `set_rate`
/// re-cues at the head: a player left sitting on an earlier stretch would make sound from seconds the cut took
/// out. A player that will not start is logged and answered `false`; the tick's arithmetic and every other
/// preview rule are unaffected, which is the "carry on elsewhere" half of §0 (F2.1's cue door says the same).
pub fn cue_running(window: &adw::ApplicationWindow, at: f64) -> bool {
    let cued = cue_files(&ui::cut_record_play::files_for(window, &ui::cut_record_play::runs_now()), at);
    // §05-cut#8 bullet 2: the log carries the reason (`cue_files` already wrote it) and the status row says
    // where to find it. Both halves, because a status that reads "playback failed" without a reason and a
    // reason nobody was told about are the same failure twice over.
    if !cued {
        if let Some(status) = ui::find_status(window.upcast_ref()) {
            status.set_text(&crate::preview::playback_status("Cut"));
        }
    }
    cued
}

/// The same cue with the footage already named. Split out because a test can name two files it wrote itself,
/// while `files_for` needs a real session folder under the process cwd — and this binary builds its window
/// from the fixture elsewhere, so the run list resolves to nothing there.
pub fn cue_files(files: &[String], at: f64) -> bool {
    match crate::cut_play_back::cue_clips(files, at, &|line| log_line(line)) {
        Ok(_) => true,
        Err(why) => {
            log_line(&crate::preview::playback_failure("Cut", &why));
            false
        }
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
