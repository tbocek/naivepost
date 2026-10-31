//! §08-produce#1-screen — the render's **speak** leg, over the real wire.

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

fn spoken_entry(tree: &Tree, line: &crate::produce_exec::ToSpeak) -> Entry {
    let record = crate::narration::load(tree).unwrap_or_default();
    let held = crate::narration::lines_to_speak(&record.entries);
    let found = held
        .get(line.index)
        .filter(|held| held.text == line.text)
        .or_else(|| held.iter().find(|held| held.text == line.text));
    Entry {
        s: 0.0,
        e: 0.0,
        at: 0.0,
        text: line.text.clone(),
        emotion: found.map(|held| held.emotion.clone()).unwrap_or_default(),
        pos: String::new(),
        roll: found.map_or(0, |held| held.roll),
    }
}

fn project_language(tree: &Tree) -> String {
    crate::project::load(tree.dir())
        .map(|opened| opened.policy.language.value)
        .unwrap_or_default()
}
