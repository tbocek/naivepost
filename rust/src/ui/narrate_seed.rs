//! §07-narrate#3-data's writes from the Narrate page: the two one-line settings and `takes.json`.
//!
//! The page's doors own what a press MEANS; this module owns what a press LEAVES ON DISK, so the same
//! three files are written the same way whichever control changed them. It lives apart from
//! [`narrate_page`](super::narrate_page) because that file is at its size budget and these are three
//! independent writes, not part of the page's drawing.
//!
//! Nothing here decides rules: [`crate::narrate_data`] owns the file formats, and the callers keep their
//! own sentences. A project with no folder open simply has nothing to write to, so every function answers
//! quietly in that case rather than inventing a path.

use crate::layout::Tree;
use crate::narrate_data::{self, Take};
use crate::ui::narrate_page::NarrateState;

/// The page's own pitch door, whichever control moved it: the slider's debounced wire and the keyboard step
/// both call THIS, so one writer touches `narrate/pitch.txt`, beside S4's invalidation of the shifted copy
/// (the base stays and re-shifts). `None` = no folder open, or it worked.
pub fn note_pitch_for_session(semitones: f64) -> Option<String> {
    let tree = crate::ui::window::narrate_session_tree()?;
    crate::voice_ref::invalidate_for_change(&tree, crate::voice_ref::Changed::Pitch);
    // §07-narrate#3-data: `narrate/pitch.txt` is what makes the shift survive the window. A reason reaches
    // the log rather than the status line: the sentence the press prints is about the VOICE, and losing the
    // shift silently is worse than a second line saying it did not persist.
    narrate_data::write_pitch(&tree, semitones).err()
}

/// `narrate/takes.json` after either take door moved the list, with S4's invalidation beside it (§07's takes
/// row): each press cuts the reference from different seconds. `None` = nothing to write, or it worked.
pub fn note_takes_for_session(takes: &[(f64, f64)]) -> Option<String> {
    let tree = crate::ui::window::narrate_session_tree()?;
    crate::voice_ref::invalidate_for_change(&tree, crate::voice_ref::Changed::Takes);
    note_takes(&tree, takes)
}

/// `narrate/voice.txt` after a voice pick, plus S4's invalidation of the reference cut from the old one
/// (§07-narrate#3-data). `Err` says the id could not be stored.
pub fn note_voice_for_session(id: &str) -> Result<(), String> {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return Ok(());
    };
    narrate_data::write_voice(&tree, id)?;
    crate::voice_ref::invalidate_for_change(&tree, crate::voice_ref::Changed::Voice);
    Ok(())
}

/// What the page shows when it opens, seeded from the project's own files (§07-narrate#3-data): the voice
/// row, the slider's shift and the take list come off disk rather than from defaults. Only a file that EXISTS
/// is copied over, so a project that never touched the slider keeps what the flow published.
///
/// Takes are the exception in the other direction: keyed by the RECORDING, they fill an EMPTY state only — a
/// non-empty one is a flow's own live copy, which a per-tab rebuild must not drop.
pub fn seed_from_disk(tree: &Tree, mut state: NarrateState) -> NarrateState {
    if tree.voice_txt().is_file() {
        state.voice = narrate_data::read_voice(tree);
    }
    if tree.pitch_txt().is_file() {
        state.pitch = narrate_data::read_pitch(tree);
    }
    if state.takes.is_empty() {
        state.takes = stored_takes(tree);
    }
    state
}

/// What the page shows when it opens: the takes this project left in `narrate/takes.json`, under its own
/// recording's key. Empty when no narrator is tagged or nothing was ever picked, which is what leaves the
/// band bare rather than showing someone else's list.
pub fn stored_takes(tree: &Tree) -> Vec<(f64, f64)> {
    let Some(recording) = take_recording_key(tree) else {
        return Vec::new();
    };
    narrate_data::load_takes(tree)
        .ok()
        .and_then(|mut all| all.remove(&recording))
        .unwrap_or_default()
        .iter()
        .map(|take| (take.s, take.e))
        .collect()
}

/// The recording the takes belong to: the project's narrator 1, keyed by its own stem (§07's takes row —
/// keyed by RECORDING, so re-tagging a slot never hands one person's takes to another). Same lookup the
/// speaking leg uses; `None` when the project tagged no narrator.
pub fn take_recording_key(tree: &Tree) -> Option<String> {
    let project = crate::project::load(tree.dir()).ok()?;
    let source = project.sources.iter().find(|source| source.narrator == 1)?;
    let key = narrate_data::take_base(std::path::Path::new(&source.path));
    (!key.is_empty()).then_some(key)
}

/// `narrate/takes.json` for the session's recording, from the page's `(start, end)` spans. An empty list
/// is what deletes the key, which is [`narrate_data::save_takes`]'s own rule, so "removed the last take"
/// and "never had one" end up as the same file.
pub fn note_takes(tree: &Tree, takes: &[(f64, f64)]) -> Option<String> {
    let Some(recording) = take_recording_key(tree) else {
        // No narrator tagged: there is no recording to file the picks under, so nothing is written.
        return None;
    };
    let stored: Vec<Take> = takes.iter().map(|(s, e)| Take { s: *s, e: *e }).collect();
    narrate_data::save_takes(tree, &recording, &stored).err()
}
