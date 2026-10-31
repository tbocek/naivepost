//! F2.2's page half for ▶✂: the two states the spec puts ON the button, and the clock's own tooltip.

use adw::prelude::*;

use crate::ui::window::{live_player, log_line};
use crate::{cut_play, tools, ui};

/// The label the toolbar ships (§1's transport table): ▶✂ while nothing of ours runs.
pub const PLAY_LABEL: &str = crate::cut_screen::PLAY_GLYPH_CUT;
/// Inventory §D's three-▶ rule: "each wears ⏸ only while its thing runs". The run bar's ▶ takes its icon
/// from `run::controls`; this button belongs to the cut preview, so its face changes here and nowhere else.
pub const PAUSE_LABEL: &str = "\u{23f8}\u{2702}";

/// Paint the button from the page's state: S1's grey and the ⏸ face, both read rather than remembered.
pub fn paint(window: &adw::ApplicationWindow) {
    let Some(button) = ui::play_cut_button(window) else {
        return;
    };
    button.set_sensitive(!cut_play::empty(&ui::review_cut_of(window)));
    let player = ui::preview_player(window);
    // §2's "one lit" from [`crate::tab_states::preview_lits`], the same decision the other two buttons
    // read; ⏸ only while this mode is the lit one AND the transport is actually playing.
    let [_recording_lit, ours_lit, _review_lit] =
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

/// Paint BOTH skipping buttons — ▶✂ and ▶✂✂ — from this window's cut. They read the same cut slot, and the
/// slot is only filled after the page's first refresh, so the pair is repainted together from one door; the
/// review's own rules stay in [`crate::ui::cut_review_play`].
pub fn paint_both_skipping_buttons(window: &adw::ApplicationWindow) {
    paint(window);
    crate::ui::cut_review_play::paint(window);
}

/// Preroll the spare at `at`.
pub fn preroll_spare(window: &adw::ApplicationWindow, at: f64) -> bool {
    // The built-in player seeks within its pipeline instead of holding a second process at the join.
    if crate::play_built_in::in_use() {
        return false;
    }
    let files = ui::cut_record_play::files_for(window, &ui::cut_record_play::runs_now());
    crate::cut_play_back::preroll(&files, at, &|line| log_line(line)).is_ok()
}

/// The join arrived and the spare was sitting on it: swap it in. Answers whether one was there to swap.
pub fn swap_spare(window: &adw::ApplicationWindow) -> bool {
    let _ = window;
    // No spare with the built-in player: the line's jump is followed by a seek (`cut_picture::refresh`).
    if crate::play_built_in::in_use() {
        return false;
    }
    crate::cut_play_back::swap_spare_in(&|line| log_line(line)).is_some()
}

/// Release the spare: pausing, or the cut changing under it, takes the waiting process back with it.
pub fn release_spare() {
    crate::cut_play_back::drop_spare();
}

/// S5 "stops show their still": freeze the running player's frame, or release it.
pub fn freeze(hold: bool) -> bool {
    if crate::play_built_in::in_use() && crate::play_built_in::file().is_some() {
        return if hold {
            crate::play_built_in::pause()
        } else {
            crate::play_built_in::resume()
        };
    }
    crate::cut_play_back::hold_still(hold)
}

/// S4's pause, from the ▶✂ press: stop the cut's player AND release its spare. A paused preview that keeps
/// a prerolled pipeline is a process holding a file for a jump that will now never happen.
pub fn pause_media() {
    crate::cut_play_back::halt();
    crate::play_built_in::pause();
}

/// S5 "speed effects at their flat rate", handed to the player at the head. What the player's argv can and
/// cannot do about a rate is [`crate::cut_play_back::set_rate`]'s honest answer, not this door's.
pub fn hand_rate(window: &adw::ApplicationWindow, rate: f64, at: f64) -> bool {
    // The built-in player takes a rate as it plays (a flushing seek that carries it).
    if crate::play_built_in::in_use() && crate::play_built_in::file().is_some() {
        return crate::play_built_in::set_rate(rate);
    }
    let files = ui::cut_record_play::files_for(window, &ui::cut_record_play::runs_now());
    crate::cut_play_back::set_rate(&files, rate, at, &|line| log_line(line))
}

pub fn hand_gain(window: &adw::ApplicationWindow, gain: f64, at_secs: f64) {
    if let Some(player) = live_player(window) {
        player.borrow_mut().volume = Some(gain);
    }
    if crate::play_back::running() {
        crate::play_back::set_volume(gain, at_secs);
    }
}

/// Start (or resume) the cut preview's RUNNING player at `at`, replacing whatever was playing.
pub fn cue_running(window: &adw::ApplicationWindow, at: f64) -> bool {
    let cued = if crate::play_built_in::in_use() {
        // The built-in player: the scene's recording at `at`, in the preview's own picture.
        crate::cut_play_back::halt();
        ui::cut_record_play::play_in_preview(window, at, true)
    } else {
        cue_files(
            &ui::cut_record_play::files_for(window, &ui::cut_record_play::runs_now()),
            at,
        )
    };
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
pub fn set_clock_tooltip(
    window: &adw::ApplicationWindow,
    into_cut: f64,
    of_cut: f64,
    session_t: f64,
) {
    if let Some(button) = ui::play_cut_button(window) {
        button.set_tooltip_text(Some(&clock_tooltip(into_cut, of_cut, session_t)));
    }
}
