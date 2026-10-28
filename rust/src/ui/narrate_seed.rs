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
use crate::narrate_data;
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
    note_takes(&tree, &crate::ui::narrate_page::read_state().voice, takes)
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

/// The whole page as the opened project holds it (§07-narrate#1-screen): the cut's clips and the finished video's
/// length, the lines in `narration.json`, the narrator slots Prepare tagged, the voices folder's recordings, the
/// project's language and narration switch, what the recordings cover, the voice/pitch/takes off disk, and the
/// tally the Inputs row reads. `line` is the window's red line; the page stands on it when it lies on a clip,
/// else on the first clip.
///
/// The first port only ever filled this state from tests and the snapshot bench, so a live window's Narrate page
/// showed "00:00/00:00", no voices and "the playhead is between clips" over a project with a cut.
pub fn load_page(tree: &Tree, line: f64) -> NarrateState {
    let project = crate::project::load(tree.dir()).ok();
    let cut = crate::cut::load(tree).unwrap_or_default();
    let narration = crate::narration::load(tree).unwrap_or_default();
    let clips: Vec<crate::cut::Seg> = cut
        .segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .cloned()
        .collect();
    let session = if clips.iter().any(|seg| seg.s <= line && line < seg.e) {
        line
    } else {
        clips.first().map(|seg| seg.s).unwrap_or(0.0)
    };
    let mut state = NarrateState {
        entries: narration.entries.clone(),
        segs: clips.clone(),
        language: project
            .as_ref()
            .map(|project| project.policy.language.value.clone())
            .unwrap_or_default(),
        session,
        cut_at: crate::cut_play::cut_time(&clips, session).unwrap_or(0.0),
        length: crate::shell::cut_seconds(&cut),
        clips: clips.len(),
        has_cut: !clips.is_empty(),
        narrators: project
            .as_ref()
            .map(|project| {
                project
                    .sources
                    .iter()
                    .map(|source| source.narrator as usize)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0),
        voice_files: crate::voices::list_wavs(&crate::ui::narrate_voices::voices_folder()),
        voice: project_voice(tree),
        covered_spans: crate::timeline::filmed_runs(&crate::ui::cut_folds::page_recordings()),
        // Whether a line's clip is filmed is asked per line of `covered_spans` (`narrate_page::line_covered`);
        // this flag only turns that answer off.
        recording_covers_clip: true,
        ..NarrateState::default()
    };
    state = seed_from_disk(tree, state);
    crate::ui::narrate_inputs_tally::apply(&mut state, Some(tree));
    state
}

/// The project's voice as both pages open on it: `voice.txt`; with none, the project's own narrator when
/// Prepare tagged one (the prototype's `voiceID`), else no voice at all: a default nobody stored is not a choice
/// to refuse, and Produce's stamp weighs the same answer Narrate shows.
pub fn project_voice(tree: &Tree) -> String {
    if tree.voice_txt().is_file() || take_recording_key(tree).is_some() {
        narrate_data::read_voice(tree)
    } else {
        String::new()
    }
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
        state.takes = stored_takes(tree, &state.voice);
    }
    state
}

/// What the page shows when it opens: the takes this project left in `narrate/takes.json`, each under the
/// recording it was cut from, as the band shows them, in seconds of the voice's tape
/// ([`crate::narrate_tape::Tape::takes_on`]). Empty for a voice with no recording, or when nothing was picked.
pub fn stored_takes(tree: &Tree, voice: &str) -> Vec<(f64, f64)> {
    let Ok(project) = crate::project::load(tree.dir()) else {
        return Vec::new();
    };
    let tape = crate::narrate_band_face::tape(tree, &project, voice);
    tape.takes_on(&narrate_data::load_takes(tree).unwrap_or_default())
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

/// `narrate/takes.json` after the band's takes became `takes` (band seconds): each recording on the voice's tape
/// stores its own part of them in its own seconds, beside what it keeps outside the tape
/// ([`crate::narrate_tape::Tape::store`]). An emptied list is what deletes a key, which is
/// [`narrate_data::save_takes`]'s own rule, so "removed the last take" and "never had one" end up as the same file.
pub fn note_takes(tree: &Tree, voice: &str, takes: &[(f64, f64)]) -> Option<String> {
    let project = crate::project::load(tree.dir()).ok()?;
    let tape = crate::narrate_band_face::tape(tree, &project, voice);
    // No recording behind the voice: there is nothing to file the picks under, so nothing is written.
    if tape.is_empty() {
        return None;
    }
    let stored = match narrate_data::load_takes(tree) {
        Ok(stored) => stored,
        Err(why) => return Some(why),
    };
    for (recording, picked) in tape.store(takes, &stored) {
        if let Err(why) = narrate_data::save_takes(tree, &recording, &picked) {
            return Some(why);
        }
    }
    None
}
