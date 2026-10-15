//! F4.4 for one row: the leg that speaks a line and then lets it be HEARD.
//!
//! [`crate::narrate_screen::audition`] decides which of the row's ▶ answers applies (a pause, a blank
//! clip on its own audio, a caption's moment, a synthesis already in flight) and, when the answer is
//! "speak it", hands off here. This module owns what happens next: the take is filed in `narrate/tts/`
//! through [`crate::speak_leg`] — the same door ▶ Write and speak uses, so a run and a single row cannot
//! disagree about what a take costs — and then it plays, from the line's lead-in, at the level the render
//! would give it. Living beside the page rather than inside it keeps the page's surface and wires in one
//! file and this rule in another; nothing here draws a widget.

use crate::narrate_preview_leg::{self};
use crate::ui::narrate_page::{self, NarrateState};
use crate::narration::Entry;

/// F4.4 for one row, over the real wire: the take's key and seed come from the record itself, the
/// address, the key, the model id and the server's health off the Settings file as it stands NOW
/// ([`crate::speak_leg`], which re-reads per call), and the project's language off the state the
/// flow published. Every step's own sentence reaches the status line verbatim: the module names the
/// reason where it failed and the page adds nothing to that diagnosis.
///
/// The two volumes arrive from the page because they are read off its own sliders: a second rule for
/// either would let an audition sound different from the preview it belongs to.
pub(crate) fn speak_and_audition(
    window: &adw::ApplicationWindow,
    s: &NarrateState,
    entry: &Entry,
    // The second the audition starts from — `narrate_screen::audition`'s lead-in, which decides whether
    // the three seconds ahead are this line's own or still belong to the line above it.
    from: f64,
    game_volume: f64,
    preview_volume: f64,
) -> String {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return narrate_page::say(window, "no project folder is open \u{2014} nowhere to keep the take");
    };
    let key = crate::narration::key_for(entry, Some(&s.voice));
    let seed = crate::narration::tts_seed(&key);
    // F4.5 S2: the preview keys its takes on the same voice this row speaks with, and this press is
    // the retry that clears any earlier failure of THIS wav before dialling, so a second failure reads
    // as a second failure rather than as the first one still standing.
    narrate_preview_leg::retry_wav(&key);
    // The voice the preview keys its takes on is this row's, so a lap of the cut that reaches this line
    // looks for the wav this press just filed (F4.5 S2). `start_running` re-reads both player programs
    // itself, which is why nothing here has to set them.
    narrate_preview_leg::set_voice(&s.voice);
    // P.policy.ttsLanguage is the PROJECT's language, so it is read off the project here rather
    // than trusted to what was last published: a flow that never published one must not silently
    // speak English over a Polish recording. An unreadable project leaves the state's value (or
    // empty), and `narrate_tts::language` answers its fallback for empty alone.
    let language = crate::project::load(tree.dir())
        .map(|project| project.language)
        .unwrap_or_else(|_| s.language.clone());
    // A take already on disk is REPLAYED, not synthesized again: the cache is the app's cost model, and a
    // second dial for bytes already filed would pay audio.cpp twice for one line.
    let cached = crate::narrate_tts::take_path(&tree, &key);
    let outcome = if cached.is_file() {
        crate::narrate_tts::Outcome::Take(cached)
    } else {
        crate::speak_leg::speak(&tree, &entry.text, &entry.emotion, seed, &key, &language)
    };
    if outcome.refused().is_some() {
        // Sticky per wav: a lap of the cut that reaches this line must run mute and say so again rather
        // than dial the server for a take that was already refused.
        narrate_preview_leg::note_failed_wav(&key);
        return narrate_page::say(window, &crate::narrate_tts::outcome_said(&outcome));
    }
    let wav = match &outcome {
        crate::narrate_tts::Outcome::Take(wav) => wav.clone(),
        crate::narrate_tts::Outcome::Refused(_) => unreachable!("a refusal returned above"),
    };
    // The take exists, so the press has to be HEARD: inventory C.4 gives the row's ▶ an audition that
    // starts from its lead-in, carries on down the cut, and has sound equal to the render's — the clip
    // ducked by the game volume while the line speaks.
    let gain = narrate_preview_leg::voice_gain(game_volume);
    if s.recording_covers_clip {
        // Bed first: the cut's OWN players from the lead-in, through the picture click's door, so the bed,
        // the hushes and the seek rate cannot disagree with a real preview (inventory C.4: the audition
        // carries on down the cut). `start_running` cues the PICTURE only — the take goes on its own player
        // below, because the page's tick never advances the head except on a skip, so waiting for it to
        // walk into the line would leave the audition silent.
        let cut = crate::cut::load(&tree).unwrap_or_default();
        match narrate_preview_leg::start_running(
            &tree,
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
                    // §5: an audition that cued players starts the preview for the bar's sake too.
                    state.started = true;
                    // The head has to agree with what the picture was cued at, or the next beat describes a
                    // different second than the player is playing.
                    state.session = from;
                    state.cut_at = from;
                });
                crate::ui::window::log_line(&format!("audition: {started}"));
                // §5: the bar reads this page's transport, so an audition that cued players says so.
                crate::ui::narrate_page_transport::publish_transport();
            }
            Err(why) => crate::ui::window::log_line(&format!("!!! audition: {why}")),
        }
    }
    // The take itself, on the voice player at the ducked level: spec/07-narrate.md S3 "whole clip ducked by
    // the game volume while a line speaks", inventory C.4 "sound equals the render's". With no recording
    // under the clip this is the whole soundtrack (an audition over a clip with no footage speaks against a
    // still frame), so the bed above is simply absent.
    match narrate_preview_leg::spawn_player(
        &narrate_preview_leg::voice_program(),
        &narrate_preview_leg::voice_args(&wav.display().to_string(), 0.0, gain),
    ) {
        Ok(child) => crate::ui::window::log_line(&format!(
            "audition: take plays at gain {gain:.3} (pid {})",
            child.id()
        )),
        Err(why) => crate::ui::window::log_line(&format!("!!! audition: {why}")),
    }
    narrate_page::say(window, &crate::narrate_tts::outcome_said(&outcome))
}
