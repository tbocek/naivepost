//! §F4.6 — the take band and the sample plate: what ＋, －, ▶ on a take, and ▶ / ⏹ / ⟳ on the sample
//! sentence do. Split out of `narrate_page.rs`, which is past its size budget; these doors are
//! re-exported through it, so no caller's path changed. All state still lives in `NarrateState` behind
//! `read_state` / `mutate` — this module keeps none of its own.

use adw::prelude::*;
use gtk4 as gtk;

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

/// The take walk a ▶ started: the stretches it plays in order, which one sounds, when that one began, and the
/// recording they are cut from.
struct Walk {
    spans: Vec<(f64, f64)>,
    at: usize,
    began: std::time::Instant,
    file: std::path::PathBuf,
    /// How long past a span's end the clock waits for the player to end it: a player told the end
    /// (ffplay's `-t`) gets half a second to exit by itself; one that cannot be told (`gst-play-1.0`) is ended
    /// by the clock at the end itself, or it would play on into what is not picked.
    slack: f64,
}

thread_local! {
    static WALK: std::cell::RefCell<Option<Walk>> = const { std::cell::RefCell::new(None) };
}

/// Is a take walk playing?
pub fn walking() -> bool {
    WALK.with(|walk| walk.borrow().is_some())
}

/// The take band's ▶ wears ⏹ while a walk plays (prototype `syncPlayBtn`: a second press can only mean
/// "enough"), sized for both faces so the row never moves.
fn paint_take_play(window: &adw::ApplicationWindow) {
    if let Some(button) = crate::ui::line_step_button(window, "take-play") {
        crate::ui::cut_record_play::set_steady_label(
            &button,
            if walking() { "\u{23f9}" } else { "\u{25b6}" },
            &["\u{25b6}", "\u{23f9}"],
        );
    }
}

/// **8** ▶ plays the takes from the red bar, the seconds themselves in the recording they are cut from (the
/// prototype's `playClicked`; the reference file does not exist until a line was spoken, and is levelled and
/// pitch-shifted besides). The bar inside a take plays from the bar to that take's end, then the next take, and
/// the red bar walks with it, jumping over what is not picked; with no take after the bar the recording plays
/// from the bar ([`narrate_screen::take_queue`]). A press while it walks stops it. The first rewrite played
/// `voice_ref.wav`, so on a project nobody had spoken a line in, ▶ did nothing.
pub fn press_take_play(window: &adw::ApplicationWindow) -> String {
    if walking() {
        WALK.with(|walk| walk.borrow_mut().take());
        narrate_sample::stop();
        crate::ui::narrate_band::set_walk_head(None);
        paint_take_play(window);
        crate::ui::narrate_band::repaint(window);
        return say(window, narrate_details::takes_stopped());
    }
    let s = read_state();
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return say(window, "no project folder is open \u{2014} nothing to play");
    };
    let project = crate::project::load(tree.dir()).unwrap_or_default();
    let Some(file) = crate::narrate_band_face::recording_path(&tree, &project, &s.voice)
        .filter(|path| path.is_file())
    else {
        return say(
            window,
            "this voice has no recording to play \u{2014} tag one on the Prepare step",
        );
    };
    // A recording whose length could not be read plays to its own end.
    let duration = crate::ui::narrate_band::painted_seconds().unwrap_or(f64::INFINITY);
    let from = crate::ui::narrate_band::red_bar_or(s.session);
    let (spans, takes) = narrate_screen::take_queue(&s.takes, from, duration);
    let Some(&(first, _)) = spans.first() else {
        return say(
            window,
            "the red bar is at the end of the recording \u{2014} click the band where ▶ should start",
        );
    };
    // The player is the sample's too: a sample still sounding would play over the takes.
    narrate_sample::stop();
    if let Err(why) = narrate_sample::play_span(&file, spans[0].0, spans[0].1) {
        return say(window, &why);
    }
    let total: f64 = spans.iter().map(|(a, b)| b - a).sum();
    WALK.with(|walk| {
        *walk.borrow_mut() = Some(Walk {
            spans,
            at: 0,
            began: std::time::Instant::now(),
            file,
            slack: if crate::narrate_preview_leg::voice_player_bounds_spans() {
                0.5
            } else {
                0.0
            },
        })
    });
    // The red bar stays where it was put; the walk's own playhead shows where the sound is.
    crate::ui::narrate_band::set_walk_head(Some(first));
    paint_take_play(window);
    walk_on(window);
    let count = if takes {
        WALK.with(|walk| walk.borrow().as_ref().map_or(0, |w| w.spans.len()))
    } else {
        0
    };
    say(window, &narrate_details::playing_takes(count, total, first))
}

/// The walk's beat: the red bar follows the sounding stretch, and at its end the next one starts where it is
/// picked, the bar jumping there; after the last, "takes played".
fn walk_on(window: &adw::ApplicationWindow) {
    let window = window.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        enum Beat {
            Gone,
            At(f64),
            Next(std::path::PathBuf, f64, f64),
            Done,
        }
        let beat = WALK.with(|walk| {
            let mut walk = walk.borrow_mut();
            let Some(held) = walk.as_mut() else {
                return Beat::Gone;
            };
            let (start, end) = held.spans[held.at];
            let at = start + held.began.elapsed().as_secs_f64();
            // The player's own exit is the end; the clock is the fallback for a player that lingers.
            if (narrate_sample::all_finished() && at > start + 0.2) || at >= end + held.slack {
                if held.at + 1 >= held.spans.len() {
                    *walk = None;
                    return Beat::Done;
                }
                held.at += 1;
                held.began = std::time::Instant::now();
                let (next_start, next_end) = held.spans[held.at];
                return Beat::Next(held.file.clone(), next_start, next_end);
            }
            Beat::At(at.min(end))
        });
        match beat {
            Beat::Gone => return glib::ControlFlow::Break,
            Beat::At(at) => crate::ui::narrate_band::set_walk_head(Some(at)),
            Beat::Next(file, start, end) => {
                narrate_sample::stop();
                crate::ui::narrate_band::set_walk_head(Some(start));
                if let Err(why) = narrate_sample::play_span(&file, start, end) {
                    WALK.with(|walk| walk.borrow_mut().take());
                    crate::ui::narrate_band::set_walk_head(None);
                    paint_take_play(&window);
                    say(&window, &why);
                    crate::ui::narrate_band::repaint(&window);
                    return glib::ControlFlow::Break;
                }
            }
            Beat::Done => {
                narrate_sample::stop();
                crate::ui::narrate_band::set_walk_head(None);
                paint_take_play(&window);
                say(&window, narrate_details::takes_played());
                crate::ui::narrate_band::repaint(&window);
                return glib::ControlFlow::Break;
            }
        }
        crate::ui::narrate_band::repaint(&window);
        glib::ControlFlow::Continue
    });
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
    } // Said BEFORE the leg starts: §6 names this sentence precisely so the bar never reads as done while
      // the synthesis is out on the wire. The flag goes with it, for the next press.
    say(window, said);
    run_sample_leg(window, 0);
    said.to_string()
}

/// The cached look-aside, with the page's own state read here so the module below stays free of it.
fn sample_heard_if_any(text: &str) -> Option<String> {
    let voice = read_state().voice;
    let tree = crate::ui::window::narrate_session_tree()?;
    narrate_sample::already_heard(&tree, &voice, text)
}

/// Read the plate's own words and hand them to the speak. Split out so both ▶ and ⟳ pass the same text
/// they show rather than each re-reading the entry and risking disagreeing with it.
///
/// The synthesis runs on a worker (§03-shell §1, "The window thread never waits"); the page is `busy` until
/// it lands, which is what makes another ▶ or ⟳ say "still synthesizing". Sample ⏹ clears `busy`, so a take
/// landing after it is stopped at once rather than heard.
fn run_sample_leg(window: &adw::ApplicationWindow, roll: usize) {
    let s = read_state();
    let text = widget_in(window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|e| e.text().to_string())
        .unwrap_or_default();
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        say(
            window,
            "no project folder is open \u{2014} nowhere to keep the sample",
        );
        return;
    };
    mutate(|state| state.busy = true);
    let held = window.clone();
    crate::ui::background::run(
        "sample",
        move || narrate_sample::speak(&tree, &s.voice, s.pitch, &text, roll),
        move |outcome| {
            if !read_state().busy {
                // ⏹ gave up on it while it was out: what the speak started is stopped.
                narrate_sample::stop();
                return;
            }
            mutate(|state| state.busy = false);
            match outcome {
                Ok(status) => say(&held, &status),
                Err(why) => say(&held, &why),
            };
        },
    );
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
            &format!(
                "{} \u{2014} {} player(s) stopped",
                narrate_details::sample_stopped_status(),
                killed
            ),
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
    run_sample_leg(window, 1);
    said.to_string()
}
