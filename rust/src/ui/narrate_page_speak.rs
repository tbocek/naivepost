//! F4.4 for one row: speak a line and let it be HEARD. [`crate::narrate_screen::audition`] picks which ▶ answer applies and hands off here; the take is filed in `narrate/tts/` through [`crate::speak_leg`] (same door as ▶ Write and speak) and played from the lead-in at the render's level. No widgets drawn.

use crate::narrate_preview_leg::{self};
use crate::narrate_preview_media;
use crate::narration::Entry;
use crate::ui::narrate_page::{self, NarrateState};

/// F4.4 for one row over the real wire: the take's key and seed come from the record, server settings are re-read from Settings per call ([`crate::speak_leg`]), the language from the flow's published state. Each step's sentence reaches the status line verbatim.
/// The two volumes come from the page's sliders so an audition matches the preview.
/// A take on disk is heard at once; one to synthesize is dialled on a worker (§03-shell §1) with the page `busy` until it lands, and the audition runs only if ⏹ or a newer press has not moved the audition turn.
pub(crate) fn speak_and_audition(
    window: &adw::ApplicationWindow,
    s: &NarrateState,
    entry: &Entry,
    // The audition start: `narrate_screen::audition`'s lead-in.
    from: f64,
    game_volume: f64,
    preview_volume: f64,
) -> String {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return narrate_page::say(
            window,
            "no project folder is open \u{2014} nowhere to keep the take",
        );
    };
    let key = crate::narration::key_for(entry, Some(&s.voice));
    let seed = crate::narration::tts_seed(&key);
    // F4.5 S2: the preview keys takes on this row's voice, and this press retries and clears any earlier failure of THIS wav before dialling.
    narrate_preview_leg::retry_wav(&key);
    // `start_running` re-reads both player programs itself.
    narrate_preview_leg::set_voice(&s.voice);
    // P.policy.ttsLanguage is the PROJECT's language, read off the project here so a flow that never published one does not speak English over a Polish recording. Unreadable leaves the state value; `narrate_tts::language` falls back for empty alone.
    let language = crate::project::load(tree.dir())
        .map(|project| project.policy.language.value)
        .unwrap_or_else(|_| s.language.clone());
    // A take on disk is replayed, not synthesized again (the cache is the cost model).
    let cached = crate::narrate_tts::take_path(&tree, &key);
    if cached.is_file() {
        let outcome = crate::narrate_tts::Outcome::Take(cached);
        return hear(
            window,
            s,
            entry,
            from,
            game_volume,
            preview_volume,
            &tree,
            &key,
            outcome,
        );
    }
    narrate_page::mutate(|state| state.busy = true);
    let turn = narrate_preview_leg::next_audition();
    let (text, emotion) = (entry.text.clone(), entry.emotion.clone());
    let (worker_tree, worker_key) = (tree.clone(), key.clone());
    let held = window.clone();
    let line_no = s
        .entries
        .iter()
        .position(|row| row == entry)
        .map_or(1, |index| index + 1);
    let (s, entry) = (s.clone(), entry.clone());
    crate::ui::background::run(
        "speak a line",
        move || {
            crate::speak_leg::speak(&worker_tree, &text, &emotion, seed, &worker_key, &language)
        },
        move |outcome| {
            // ⏹ or a newer press gave up on it: the take is filed but nothing plays.
            if !narrate_preview_leg::audition_is_current(turn) {
                return;
            }
            narrate_page::mutate(|state| state.busy = false);
            hear(
                &held,
                &s,
                &entry,
                from,
                game_volume,
                preview_volume,
                &tree,
                &key,
                outcome,
            );
        },
    );
    narrate_page::say(
        window,
        &crate::narrate_details::speaking_line_status(line_no),
    )
}

/// The take is in hand (or refused): file the refusal, or play it from the lead-in with the bed under it. On the window thread.
#[allow(clippy::too_many_arguments)] // the row's whole audition: page, state, line, start, two volumes, folder, key, take
fn hear(
    window: &adw::ApplicationWindow,
    s: &NarrateState,
    entry: &Entry,
    from: f64,
    game_volume: f64,
    preview_volume: f64,
    tree: &crate::layout::Tree,
    key: &str,
    outcome: crate::narrate_tts::Outcome,
) -> String {
    if outcome.refused().is_some() {
        // Sticky per wav: a later lap runs mute and says so rather than re-dialling a refused take.
        narrate_preview_leg::note_failed_wav(key);
        return narrate_page::say(window, &crate::narrate_tts::outcome_said(&outcome));
    }
    let wav = match &outcome {
        crate::narrate_tts::Outcome::Take(wav) => wav.clone(),
        crate::narrate_tts::Outcome::Refused(_) => unreachable!("a refusal returned above"),
    };
    // The take exists, so the press must be HEARD (inventory C.4): audition from the lead-in down the cut, clip ducked by the game volume while the line speaks.
    let gain = narrate_preview_leg::voice_gain(game_volume);
    if narrate_page::line_covered(s, entry.s + entry.at) {
        // Bed first: the cut's own players from the lead-in through the picture click's door. `start_running` cues the PICTURE only; the take plays on its own player below because the tick advances the head only on a skip.
        let cut = crate::cut::load(tree).unwrap_or_default();
        match narrate_preview_leg::start_running(
            tree,
            &cut,
            &s.entries,
            from,
            game_volume,
            preview_volume,
            &mut narrate_preview_leg::spawn_player,
        ) {
            Ok(started) => {
                narrate_page::mutate(|state| {
                    state.playing = true;
                    state.started = true;
                    // The head must agree with what the picture was cued at.
                    state.session = from;
                    state.cut_at = from;
                });
                crate::ui::window::log_line(&format!("audition: {started}"));
                // §5: the bar reads this page's transport.
                crate::ui::narrate_page_transport::publish_transport();
            }
            Err(why) => crate::ui::window::log_line(&format!("!!! audition: {why}")),
        }
    }
    // The take on the voice player at the ducked level (spec/07-narrate.md S3, inventory C.4).
    // With no recording under the clip the frame is still shown: §6 (`Preview`) speaks against a still frame, cued at the line's second and frozen ([`narrate_preview_leg::still_frame`]). The built-in picture needs no frozen process.
    if !narrate_page::line_covered(s, entry.s + entry.at)
        && !narrate_preview_leg::picture_is_built_in()
    {
        let cut = crate::cut::load(tree).unwrap_or_default();
        match narrate_preview_leg::still_frame(
            tree,
            &cut,
            from,
            narrate_preview_media::bed_gain(&cut, from, true, game_volume, preview_volume),
            &mut narrate_preview_leg::spawn_player,
        ) {
            Ok((child, cue)) => {
                narrate_page::mutate(|state| {
                    state.session = from;
                    state.cut_at = from;
                });
                crate::ui::window::log_line(&format!(
                    "audition: still frame held at {} (pid {}, {})",
                    crate::narrate_screen::time_field(from),
                    child.id(),
                    cue.args.join(" ")
                ));
            }
            Err(why) => crate::ui::window::log_line(&format!("!!! audition: {why}")),
        }
    }
    let wait = (entry.s + entry.at - from).max(0.0);
    let line = s.entries.iter().position(|row| row == entry);
    let file = wav.display().to_string();
    let turn = narrate_preview_leg::next_audition();
    glib::timeout_add_local_once(std::time::Duration::from_secs_f64(wait), move || {
        if !narrate_preview_leg::audition_is_current(turn) {
            return;
        }
        match narrate_preview_leg::spawn_player(
            &narrate_preview_leg::voice_program(),
            &narrate_preview_leg::voice_args(&file, 0.0, gain),
        ) {
            Ok(child) => {
                crate::ui::window::log_line(&format!(
                    "audition: take plays at gain {gain:.3} (pid {})",
                    child.id()
                ));
                narrate_preview_leg::hold_audition(child, line);
            }
            Err(why) => crate::ui::window::log_line(&format!("!!! audition: {why}")),
        }
    });
    narrate_page::say(window, &crate::narrate_tts::outcome_said(&outcome))
}
