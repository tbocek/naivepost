//! F5.4 S3 — the translation walk: batches in, one language's finished track out.
//!
//! `produce_subtitles` holds every rule (the batch size, the message wording, what a missing number
//! means); this module is the only thing that PUTS THEM IN ORDER and talks to a model. It talks through
//! an injected [`Ask`] for the same reason `narrate_tts::speak_line` takes both of its network legs as
//! arguments: no test dials a socket, and the page can hand in a scripted answer or a real client
//! without either side knowing which.
//!
//! Nothing here decides a rule. Every string comes from `subs`, and every span and placement comes from
//! the ORIGINAL cue — the translated text never re-derives when a caption appears or where it sits.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::produce_subtitles as subs;

/// The model leg: the message and the numbers asked for, back as the numbers answered. `Err` means the
/// call did not happen at all — which ships that language's track as the original text with a warning
/// (§F5.4 S3), rather than failing the render over one server.
pub type Ask = dyn Fn(&str, &[usize]) -> Result<BTreeMap<usize, String>, String>;

/// One language's finished track: its cues carrying the translated text on the original spans, the
/// numbers that never came back, what to say about them, and whether the whole answer earned a cache slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Tracked {
    pub language: String,
    pub cues: Vec<subs::Cue>,
    /// S3: lines that stayed in the source language.
    pub missing: Vec<usize>,
    /// S3: `subs::merge`'s own sentence about those lines, for the run's log.
    pub warning: Option<String>,
    /// S3 (`cached only when complete`): a partial answer cached is a partial answer forever.
    pub cached: bool,
}

thread_local! {
    /// A scripted spawner, so the press can reach S9 in a container with no ffmpeg
    /// (`command -v ffmpeg` finds nothing). See [`crate::ui::produce_page::page_spawn`].
    static SPAWN: RefCell<Option<SpawnSlot>> = const { RefCell::new(None) };
}

/// The scripted-spawner shape. `Arc` and `Send + Sync` because a run is walked on a worker (§03-shell §1):
/// the press takes the slot loaded on the window thread and loads it again on the run's own thread.
pub type SpawnSlot =
    std::sync::Arc<dyn Fn(&crate::produce_exec::Command) -> Result<(), String> + Send + Sync>;

/// Load a scripted spawner for the next render on this thread.
pub fn set_spawn_for_test(spawn: SpawnSlot) {
    SPAWN.with(|cell| cell.replace(Some(spawn)));
}

/// The scripted spawner, if one is loaded.
pub fn spawn_for_test() -> Option<SpawnSlot> {
    SPAWN.with(|cell| cell.borrow().clone())
}

/// S3: every ticked language's track, in the order [`subs::track_languages`] lists them. The session's
/// own language is dropped there — it is the bare-stem track already written, and translating a language
/// into itself answers with what it was given.
pub fn track(
    cues: &[subs::Cue],
    session_language: &str,
    ticked: &[String],
    ask: &Ask,
    mut log: impl FnMut(&str),
) -> Vec<Tracked> {
    subs::track_languages(session_language, ticked)
        .iter()
        .map(|language| one_track(cues, language, ask, &mut log))
        .collect()
}

/// One language, start to finish: numbered originals, batches, the one re-ask, then the merge put back
/// onto the original cues. Every word it says goes out through `log` — the run's own log line, never a print.
fn one_track(cues: &[subs::Cue], language: &str, ask: &Ask, log: &mut impl FnMut(&str)) -> Tracked {
    // `language` is the track's CODE (it names the file); a person reads the language by its name.
    let said = crate::roles::language_name(language);
    // 1-based numbers (the contract is what the model sees printed), and `sent_text` strips the
    // placement tag so `{\an8}` never travels as a word — §3.10's first must-fix.
    let original: Vec<(usize, String)> = (1..=cues.len())
        .map(|number| (number, subs::sent_text(&cues[number - 1])))
        .collect();

    let mut got: BTreeMap<usize, String> = BTreeMap::new();
    let mut complete = true;
    for range in subs::batches(cues.len()) {
        let part: Vec<(usize, String)> = original[range.0..range.1].to_vec();
        let numbers: Vec<usize> = part.iter().map(|(number, _)| *number).collect();
        let mut batch = subs::Batch::new(range);
        // The request is §3.10's sentence PLUS the numbered lines themselves: `subs::message` is only the
        // heading, so sending it alone asks for a translation of lines the model never got.
        let request = format!(
            "{}\n{}",
            subs::message(part.len(), &crate::roles::language_name(language)),
            subs::numbered(&part)
        );
        if let Ok(answers) = ask(&request, &numbers) {
            for (number, text) in answers {
                // A line the batch rejects (a number it never asked for, empty text) leaves that line
                // missing rather than poisoning the track: `missing` is what decides the re-ask.
                let _ = batch.translate_line(number, &text);
            }
        }
        // S3: ONE re-ask, naming only the lines still missing **with the numbers they were given the
        // first time** — `ask_again_text` says so outright, and renumbering is what made the
        // prototype's repair calls land on the wrong lines.
        if !batch.complete() {
            let again: Vec<usize> = batch.missing();
            // §6's sentence, said before the call so the second round is explained while it runs.
            log(&subs::came_back_missing(&said, &again));
            if let Ok(answers) = ask(&batch.ask_again_text(&part), &again) {
                for (number, text) in answers {
                    let _ = batch.translate_line(number, &text);
                }
            }
        }
        got.extend(batch.answers().clone());
        complete &= batch.complete();
    }

    // The merge owns the per-line decision (translated, or the original with its rows folded back) and
    // the warning that names the lines that stayed behind.
    let merged = subs::merge(&said, &original, &got);
    // §6's cache sentence: only a COMPLETE track is ever cached (`subs::cached`), so a complete answer here is
    // exactly the case where the reply may have come from `cache/llm` rather than from the model. The walk
    // cannot tell which — the LLM cache answers inside the same leg it hands this function — so the line is
    // said for the complete track and the run's own log keeps it next to the ask.
    if subs::cached(got.len() == original.len()) {
        log(&subs::from_cache_log(&said));
    }
    let translated = cues
        .iter()
        .enumerate()
        .map(|(index, cue)| {
            let body = merged.lines.get(index).map(String::as_str).unwrap_or("");
            // Placement comes from the ORIGINAL cue whatever came back (§3.10's first must-fix, other
            // half), and the model's own row breaks are kept rather than re-wrapped at
            // P.policy.subtitleRowChars (§3.10's second).
            let (text, _) = subs::keep_placement(cue, body);
            let text = subs::keep_breaks(&text).to_string();
            subs::Cue {
                s: cue.s,
                e: cue.e,
                text,
                pos: cue.pos.clone(),
            }
        })
        .collect();

    Tracked {
        language: language.to_string(),
        cues: translated,
        missing: merged.still_missing,
        warning: merged.warning,
        cached: subs::cached(complete),
    }
}
