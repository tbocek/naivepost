//! F5.4 S3: the translation walk, batches in and one language's finished track out.
//! Rules live in `produce_subtitles`; this orders them and talks to a model through an injected [`Ask`]. Text, spans and placement come from the ORIGINAL cue.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::produce_subtitles as subs;

/// The model leg: message and numbers asked for, numbers answered. `Err` means no call happened; the track ships as the original text with a warning (§F5.4 S3).
pub type Ask = dyn Fn(&str, &[usize]) -> Result<BTreeMap<usize, String>, String>;

/// One language's finished track: translated cues on the original spans, the missing numbers, their warning, and whether it earned a cache slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Tracked {
    pub language: String,
    pub cues: Vec<subs::Cue>,
    pub missing: Vec<usize>,
    pub warning: Option<String>,
    /// S3 (`cached only when complete`).
    pub cached: bool,
}

thread_local! {
    /// A scripted spawner so the press can reach S9 with no ffmpeg; see [`crate::ui::produce_page::page_spawn`].
    static SPAWN: RefCell<Option<SpawnSlot>> = const { RefCell::new(None) };
}

/// The scripted-spawner shape; `Arc` and `Send + Sync` because the run is walked on a worker (§03-shell §1).
pub type SpawnSlot =
    std::sync::Arc<dyn Fn(&crate::produce_exec::Command) -> Result<(), String> + Send + Sync>;

pub fn set_spawn_for_test(spawn: SpawnSlot) {
    SPAWN.with(|cell| cell.replace(Some(spawn)));
}

pub fn spawn_for_test() -> Option<SpawnSlot> {
    SPAWN.with(|cell| cell.borrow().clone())
}

/// S3: every ticked language's track, in [`subs::track_languages`] order; the session's own language is dropped there.
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

/// §3.10's sentence PLUS the numbered lines: `subs::message` is only the heading.
fn batch_request(part: &[(usize, String)], language: &str) -> String {
    format!(
        "{}\n{}",
        subs::message(part.len(), &crate::roles::language_name(language)),
        subs::numbered(part)
    )
}

/// Every first ask of every language's track, as [`one_track`] will make it: batches share nothing, so
/// [`crate::produce_translate_ask::warm`] can have them answered side by side before the walk reads them back.
pub fn first_asks(
    cues: &[subs::Cue],
    session_language: &str,
    ticked: &[String],
) -> Vec<(String, Vec<usize>)> {
    let original: Vec<(usize, String)> = (1..=cues.len())
        .map(|number| (number, subs::sent_text(&cues[number - 1])))
        .collect();
    let mut asks = Vec::new();
    for language in subs::track_languages(session_language, ticked) {
        for range in subs::batches(cues.len()) {
            let part = &original[range.0..range.1];
            asks.push((
                batch_request(part, &language),
                part.iter().map(|(number, _)| *number).collect(),
            ));
        }
    }
    asks
}

/// One language: numbered originals, batches, one re-ask, then the merge onto the original cues. Everything said goes through `log`.
fn one_track(cues: &[subs::Cue], language: &str, ask: &Ask, log: &mut impl FnMut(&str)) -> Tracked {
    let said = crate::roles::language_name(language);
    // Numbers are 1-based; `sent_text` strips the placement tag so `{\an8}` never travels as a word (§3.10 first must-fix).
    let original: Vec<(usize, String)> = (1..=cues.len())
        .map(|number| (number, subs::sent_text(&cues[number - 1])))
        .collect();

    let mut got: BTreeMap<usize, String> = BTreeMap::new();
    let mut complete = true;
    for range in subs::batches(cues.len()) {
        let part: Vec<(usize, String)> = original[range.0..range.1].to_vec();
        let numbers: Vec<usize> = part.iter().map(|(number, _)| *number).collect();
        let mut batch = subs::Batch::new(range);
        let request = batch_request(&part, language);
        if let Ok(answers) = ask(&request, &numbers) {
            for (number, text) in answers {
                // A rejected line (unasked number, empty text) stays missing; `missing` decides the re-ask.
                let _ = batch.translate_line(number, &text);
            }
        }
        if !batch.complete() {
            let again: Vec<usize> = batch.missing();
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

    let merged = subs::merge(&said, &original, &got);
    // §6 cache sentence: only a COMPLETE track is cached (`subs::cached`), and the walk cannot tell whether the reply came from `cache/llm`, so it is said for every complete track.
    if subs::cached(got.len() == original.len()) {
        log(&subs::from_cache_log(&said));
    }
    let translated = cues
        .iter()
        .enumerate()
        .map(|(index, cue)| {
            let body = merged.lines.get(index).map(String::as_str).unwrap_or("");
            // Placement comes from the ORIGINAL cue and the model's row breaks are kept (§3.10 must-fixes).
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
