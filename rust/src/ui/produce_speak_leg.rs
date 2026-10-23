//! §08-produce#1-screen — the render's **speak** leg, over the real wire.
//!
//! §F5.1 S4 hands the run two halves and §F5.2 S2 makes the render's first job "speak every line
//! without a wav". Deciding WHICH lines those are is `produce_exec::speech_plan`'s (through
//! [`crate::narrate_off::lines_to_speak`] and [`crate::produce_render::speak_job`]); asking audio.cpp
//! for the take is this file's. It used to answer a hardcoded refusal, which let a run report
//! "0 lines spoken, 0 speech failures" as a success while nothing had been synthesized — the exact
//! thing spec rule 5 forbids.
//!
//! The work is the Narrate page's own door, reused rather than rewritten:
//! [`crate::ui::narrate_run_speak::dial_line`] derives the cache key, the seed and the project language
//! from the line exactly as a row's ▶ does, and hands them to [`crate::speak_leg::speak`], which reads
//! the Settings file per call, dials the audio address through `server_leg` (so every trip leaves its
//! timed `requests.tsv` row) and files the reply at the path `narrate_tts` names. That path is the one
//! `produce_stamp::wav_of` answers for the same entry, voice and roll, so a take earned here is the take
//! the encoder muxes and the stamp weighs — the line is never spoken twice in a later run.
//!
//! The voice and language a produce run speaks with are read from the opened project rather than from a
//! second copy of them, so a render and the Narrate page cannot disagree about whose voice a line has.

use crate::layout::Tree;
use crate::narration::Entry;

/// One line the render asked for, spoken for real: Ok holds the take's file, Err is the refusal's own
/// sentence, which names the step and the reason (a server that is not there, a voice with no reference,
/// a reply too small to be audio) rather than a generic string.
///
/// `voice` is the run's selected voice, `""` meaning the session's own — the same value
/// `produce_snapshot::render_half` passes to `wav_of`, so both doors key the take identically.
pub fn speak_one(
    tree: &Tree,
    line: &crate::produce_exec::ToSpeak,
    voice: &str,
) -> Result<std::path::PathBuf, String> {
    let language = project_language(tree);
    crate::ui::narrate_run_speak::dial_line(tree, voice, &language, &spoken_entry(tree, line))
}

/// The line as the speak door wants it: the plan hands over text and target path only, so the entry is
/// rebuilt around them. Bounds, `at` and roll take the neutral form a take does not depend on — the key
/// is the text, the emotion and the voice (§4's key body), and bounds are not part of it.
///
/// The emotion comes from the session's own narration record when it still holds this line's words: the
/// run plans from the same `narrate/narration.json`, so a weighted tag rides to the engine here exactly
/// as it does on the row's ▶, and the take the render gets is the take that line already owns. An entry
/// the record no longer holds (or a line with no tag) keeps the flat delivery, which is what an untagged
/// line sounds like on the Narrate page too.
fn spoken_entry(tree: &Tree, line: &crate::produce_exec::ToSpeak) -> Entry {
    let emotion = crate::narration::load(tree)
        .map(|record| {
            record
                .entries
                .iter()
                .find(|held| held.text == line.text)
                .map(|held| held.emotion.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    Entry {
        s: 0.0,
        e: 0.0,
        at: 0.0,
        text: line.text.clone(),
        emotion,
        pos: String::new(),
        roll: 0,
    }
}

/// The project's own language (P.policy.ttsLanguage), read from the session folder the run works in. An
/// unreadable or blank one falls back inside [`crate::narrate_tts::language`], which keeps `"en"` only
/// for a session that truly names nothing.
fn project_language(tree: &Tree) -> String {
    crate::project::load(tree.dir())
        .map(|opened| opened.language)
        .unwrap_or_default()
}
