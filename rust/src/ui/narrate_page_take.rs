//! §F4.6 — the take band and the sample plate: what ＋, －, ▶ on a take, and ▶ / ⏹ / ⟳ on the sample
//! sentence do. Split out of `narrate_page.rs`, which is past its size budget; these doors are
//! re-exported through it, so no caller's path changed. All state still lives in `NarrateState` behind
//! `read_state` / `mutate` — this module keeps none of its own.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_data;
use crate::narrate_details;
use crate::narrate_sample;
use crate::narrate_screen::{self};
use crate::ui::narrate_page::NarrateState;
use crate::ui::narrate_page::{mutate, read_state, refresh, say, widget_in};

/// **8** ＋ makes the selection a take, refused under P.eng.takeMinSeconds.
///
/// The sentence it says carries the selection's own span first (§F4.6's caption: "take added: 0:00–0:03 —
/// 1 take(s), 2.6 s of reference (14 s is plenty)"), because the seconds someone just dragged are the
/// fact the status has to confirm; the totals after the dash are [`narrate_details::commit_status`]'s.
pub fn press_take_add(window: &adw::ApplicationWindow, selection: (f64, f64)) -> String {
    // §6 (`Take band messages`): with no drag on the band there is nothing to make a take OF, and that is a
    // different sentence from "the selection was too short" — it names the button to use. A zero-width or
    // negative selection is exactly "nothing was dragged", so it is answered before the floor rule.
    if selection.1 <= selection.0 {
        // Nothing has been picked in this session at all: §6 keeps that its own sentence (`no_takes_yet`),
        // distinct from "no takes left", which is what － says after emptying a band that had some.
        return say(window, narrate_details::no_takes_yet());
    }
    if !narrate_screen::can_make_take(selection) {
        let seconds = selection.1 - selection.0;
        return say(window, &narrate_screen::take_too_short(seconds));
    }
    let takes = mutate(|s| {
        s.takes = narrate_screen::add_takes(&s.takes, selection);
        s.takes.clone()
    });
    // S4: the reference was cut from the OLD take set, so it is wrong the moment this one lands; the picks
    // live in `narrate/takes.json`, not in this window (§07-narrate#3-data).
    if let Some(why) = crate::ui::narrate_seed::note_takes_for_session(&takes) {
        say(window, &format!("could not save the takes \u{2014} {why}"));
    }
    refresh(window);
    say(
        window,
        &format!(
            "take added: {}–{} \u{2014} {}",
            crate::tools::mm_ss(selection.0),
            crate::tools::mm_ss(selection.1),
            // §6 spells the mark as `P.machine.refWantSeconds` (14 s), NOT the ＋ press's own floor:
            // handing this `take_min()` read "(0 s is plenty)" beside a 2.6 s take, which is the
            // sentence's whole point — is this enough voice to clone?
            narrate_details::commit_status(
                "takes",
                takes.len(),
                takes.iter().map(|(a, b)| b - a).sum(),
                narrate_details::REFERENCE_WANTED_SECONDS,
            )
        ),
    )
}

/// **8** − takes those seconds back out, splitting takes when they fall inside one.
pub fn press_take_remove(window: &adw::ApplicationWindow, selection: (f64, f64)) -> String {
    let takes = mutate(|s| {
        s.takes = narrate_screen::remove_takes(&s.takes, selection);
        s.takes.clone()
    });
    // S4: same rule as adding — subtracting seconds changes what the base was cut from.
    if let Some(why) = crate::ui::narrate_seed::note_takes_for_session(&takes) {
        return say(window, &format!("could not save the takes \u{2014} {why}"));
    }
    refresh(window);
    if takes.is_empty() {
        return say(window, narrate_details::no_takes_left());
    }
    say(
        window,
        &narrate_details::commit_status(
            "takes",
            takes.len(),
            takes.iter().map(|(a, b)| b - a).sum(),
            // Same mark as the ＋ sentence: §6's "(14 s is plenty)" is `P.machine.refWantSeconds`, the
            // length the reference aims for, not the 0.4 s floor a single take has to clear.
            narrate_details::REFERENCE_WANTED_SECONDS,
        ),
    )
}

/// **8** ▶ plays the takes from the red bar (§1: "the takes, one after another — exactly what the
/// model is cloned from"), and says the raw-recording case when there are none. With takes it is the
/// same reference file the speech server is handed (`narrate/voice_ref.wav`, already cut to the takes
/// in order), so what is heard is what gets cloned; with none it is the uncut base. The seconds named
/// are the band's own red bar, which is where §1 says playback starts.
pub fn press_take_play(window: &adw::ApplicationWindow) -> String {
    // §6 ("takes played" / "stopped playing the takes"): a ▶ over a take walk still running is a
    // STOP, not a second copy — and asking whether one is running is the question the button answers,
    // so ask it before reaching for the players. The band has no ⏹ of its own, so this press is the
    // only hand that can end the walk.
    if narrate_sample::is_playing() {
        narrate_sample::stop();
        return say(window, narrate_details::takes_stopped());
    }
    // The walk ran to the end on its own (`-autoexit` leaves the finished player behind): §6's other
    // sentence, because "playing N take(s)" followed by silence reads as a hang without it.
    if narrate_sample::all_finished() {
        return say(window, narrate_details::takes_played());
    }
    let s = read_state();
    let total: f64 = s.takes.iter().map(|(a, b)| b - a).sum();
    let said = narrate_details::playing_takes(
        s.takes.len(),
        total,
        crate::ui::narrate_band::red_bar_or(s.session),
    );
    // The answer first, so the status line is the sentence §1 fixes; the player is what follows it,
    // and a project with no reference on disk yet still gets the plain answer about its seconds.
    if let Some(tree) = crate::ui::window::narrate_session_tree() {
        let file = if s.takes.is_empty() {
            narrate_data::base_reference(&tree)
        } else {
            narrate_data::served_reference(&tree)
        };
        if file.exists() {
            if let Ok(playing) = narrate_sample::play_file(&file) {
                return say(window, &format!("{said} \u{2014} {playing}"));
            }
        }
    }
    say(window, &said)
}

/// **12** ▶ speaks the sample, or says which click would clear what stops it.
///
/// The refusal ladder runs first because a sample asked of a voice that cannot speak one must not put a
/// request on the wire; past the ladder the sample is really synthesised over the same TTS leg a line
/// uses, then heard from `narrate/samples/` on every later press of the same voice and words (S5).
pub fn press_sample_play(window: &adw::ApplicationWindow) -> String {
    // §6 (`Re-roll / sample refusals`, the three states of a playing sample): a ▶ over a sample that is
    // HOLDING freezes it and says so — "sample paused — ▶ resumes, ⏹ starts over". The next ▶ thaws the same
    // player rather than synthesizing the words again, which is what "resumes" has to mean.
    if narrate_sample::hold_paused(false).is_some() {
        return say(window, narrate_details::sample_playing_status());
    }
    let text = widget_in(window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|e| e.text().to_string())
        .unwrap_or_default();
    // The ladder first: §6 lists busy / no-voice / empty-text as refusals that come BEFORE anything is
    // heard, so a press with a cached take but the page busy still says "still synthesizing" rather
    // than playing over the top of the run.
    let said = sample_ladder(&read_state(), &text);
    if said != narrate_details::sample_status() {
        return say(window, said);
    }
    // A sample is already PLAYING (nothing was held, so the thaw above did not fire): §6's three states make
    // this press a pause, not a second synthesis of the same words.
    if narrate_sample::hold_paused(true).is_some() {
        return say(window, narrate_details::sample_paused_status());
    }
    // Past the ladder: a take already on disk answers at once, and hearing what exists is not a
    // synthesis, so it must not set `busy` — that flag is what makes a later press read as work in    // flight, and a cache hit leaves nothing running.
    if let Some(heard) = sample_heard_if_any(&text) {
        return say(window, &heard);
    }    // Said BEFORE the leg starts: §6 names this sentence precisely so the bar never reads as done while
    // the synthesis is out on the wire. The flag goes with it, for the next press.
    say(window, said);
    mutate(|s| s.busy = true);
    let outcome = run_sample_leg(window, 0);
    mutate(|s| s.busy = false);
    match outcome {
        Ok(status) => say(window, &status),
        Err(why) => say(window, &why),
    }
}

/// The cached look-aside, with the page's own state read here so the module below stays free of it.
fn sample_heard_if_any(text: &str) -> Option<String> {
    let voice = read_state().voice;
    let tree = crate::ui::window::narrate_session_tree()?;
    narrate_sample::already_heard(&tree, &voice, text)
}

/// Read the plate's own words and hand them to the speak. Split out so both ▶ and ⟳ pass the same text
/// they show rather than each re-reading the entry and risking disagreeing with it.
fn run_sample_leg(window: &adw::ApplicationWindow, roll: usize) -> Result<String, String> {
    let s = read_state();
    let text = widget_in(window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|e| e.text().to_string())
        .unwrap_or_default();
    let tree = crate::ui::window::narrate_session_tree()
        .ok_or_else(|| "no project folder is open \u{2014} nowhere to keep the sample".to_string())?;
    narrate_sample::speak(&tree, &s.voice, s.pitch, &text, roll)
}

/// The four ways ▶ Sample refuses, in the order §6 lists them, then the working answer.
fn sample_ladder(s: &NarrateState, text: &str) -> &'static str {
    if s.busy {
        return narrate_details::busy_sample_status();
    }
    if s.voice == narrate_screen::CAPTIONS {
        return narrate_details::no_voice_to_sample_refusal();
    }
    if s.voice.is_empty() {
        return narrate_details::pick_a_voice_refusal();
    }
    if text.trim().is_empty() {
        return narrate_details::empty_sample_refusal();
    }
    narrate_details::sample_status()
}

pub fn press_sample_stop(window: &adw::ApplicationWindow) -> String {
    // ⏹ gives up on the sample, which means both halves: the flag, and the player that may be holding
    // the audio. Clearing only the flag would leave a take playing after the page says it stopped.
    let killed = narrate_sample::stop();
    mutate(|s| s.busy = false);
    if killed > 0 {
        return say(
            window,
            &format!("{} \u{2014} {} player(s) stopped", narrate_details::sample_stopped_status(), killed),
        );
    }
    say(window, narrate_details::sample_stopped_status())
}

pub fn press_sample_reroll(window: &adw::ApplicationWindow) -> String {
    let text = widget_in(window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|e| e.text().to_string())
        .unwrap_or_default();
    let said = sample_ladder(&read_state(), &text);
    if said != narrate_details::sample_status() {
        return say(window, said);
    }
    // ⟳ is a NEW take of the same words, so it must not read from the cache: the speak runs with its
    // roll count, which is what makes the log line say `take 2` and the file land under a fresh key.
    say(window, said);
    mutate(|s| s.busy = true);
    let outcome = run_sample_leg(window, 1);
    mutate(|s| s.busy = false);
    match outcome {
        Ok(status) => say(window, &status),
        Err(why) => say(window, &why),
    }
}
