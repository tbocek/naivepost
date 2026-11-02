//! Hand-edit the text (spec/04-prepare.md F1.12): the user deletes words in `prepare/transcript/final.txt` in any editor and the next Cut ▶ remakes the marks from it without a model.
//! The match runs backwards (the tails line up, so a middle deletion cannot shift the rest). A word nothing spoken matches was never said and is left out; an edit removing more than the retake ceiling is refused.

use std::time::SystemTime;

use crate::edges::{self, Edges};
use crate::layout::Tree;
use crate::textfmt::{self, read_final, Retake};
use crate::tools::mm_ss;
use crate::tools::retakes::RETAKE_CEIL;

/// How many words back the match may look for the text's next word; past that the word was not said (a typing or a rewrite).
pub const KEEP_REACH: usize = 200;

/// What the run says when it takes the edit over; the model is not asked, or it could undo the user's edit.
pub const EDITED_NOTE: &str =
    ">>> final.txt was edited after Prepare -- the marks are remade from it, no model asked";

/// S2: is the text newer than the marks? A missing `final.txt` is nothing to read; a missing `retakes.tsv` means an edit is waiting. Equal times are not newer.
pub fn edited(final_mtime: Option<SystemTime>, marks_mtime: Option<SystemTime>) -> bool {
    match final_mtime {
        Some(text) => marks_mtime.is_none_or(|marks| text > marks),
        None => false,
    }
}

/// The words of the edited text, with join marks ([`read_final`]) and pause markers dropped. A pipe never occurs in a transcript, so a mangled mark goes rather than becoming a word.
fn text_words(text: &str) -> Vec<String> {
    let bare = PAUSE_MARK.replace_all(text, " ");
    read_final(bare.as_ref())
        .0
        .into_iter()
        .filter(|word| word != "SEAM")
        .collect()
}

static PAUSE_MARK: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"\(pause [0-9.]+s\)").unwrap());

/// S2: which spoken words the edited text still holds, and how many of its words were never said. Walked from the end in both, since the edit is a deletion; matching forwards would let one mismatch misalign every word after it.
pub fn keep_mask(words: &[String], out: &[String]) -> (Vec<bool>, usize) {
    let mut kept = vec![false; words.len()];
    let mut extra = 0usize;
    let mut cursor = words.len();
    for wanted in out.iter().rev() {
        // Search back from the cursor within the reach; the window is `KEEP_REACH` candidates counted off the cursor, independent of how much was deleted above.
        let found = (1..=cursor.min(KEEP_REACH)).find(|&back| words[cursor - back] == *wanted);
        match found {
            Some(back) => {
                let k = cursor - back;
                kept[k] = true;
                cursor = k;
            }
            None => extra += 1,
        }
    }
    (kept, extra)
}

/// S2: every maximal run of dropped words becomes one mark. Seconds come in beside the words so a test can hand over a synthetic session. No second take exists, so `again` stays nought and `to` equals the run's end.
pub fn marks_from(times: &[(f64, f64)], kept: &[bool]) -> Vec<Retake> {
    let mut marks = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    for (index, keep) in kept.iter().enumerate().take(times.len()) {
        match (keep, run) {
            (false, None) => run = Some((index, index)),
            (false, Some((first, _))) => run = Some((first, index)),
            (true, Some((first, last))) => {
                marks.push(mark(times, first, last));
                run = None;
            }
            (true, None) => {}
        }
    }
    if let Some((first, last)) = run {
        marks.push(mark(times, first, last));
    }
    marks
}

fn mark(times: &[(f64, f64)], first: usize, last: usize) -> Retake {
    Retake {
        s: times[first].0,
        e: times[last].1,
        again: 0.0,
        to: times[last].1,
        text: String::new(),
        whole: String::new(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub marks: Vec<Retake>,
    pub logs: Vec<String>,
    pub dropped: usize,
    pub extra: usize,
    pub refused: bool,
}

/// S2: the marks remade from the edited text, no model asked. `edge_of` gives the envelope of the recording a session second belongs to ([`crate::edges`]); `None` leaves edges on the word times.
/// `dedupeJoins` and join repair belong to F1.9/F1.10; [`textfmt::read_final`] tolerates join marks staying or going.
pub fn remake<'a>(
    text: &str,
    words: &[String],
    times: &[(f64, f64)],
    mut edge_of: impl FnMut(f64) -> Option<&'a Edges>,
) -> Outcome {
    let (kept, extra) = keep_mask(words, &text_words(text));
    let mut out = Outcome {
        extra,
        ..Default::default()
    };
    out.logs.push(EDITED_NOTE.to_string());
    if extra > 0 {
        out.logs.push(format!(
            "!!! text edit: {extra} word(s) in the text were never said -- left out, there is no sound for them"
        ));
    }

    out.dropped = kept.iter().filter(|keep| !**keep).count();
    if times.is_empty() {
        return out;
    }
    if out.dropped as f64 > RETAKE_CEIL * times.len() as f64 {
        out.logs.push(format!(
            "!!! text edit: {} of {} words removed -- refused, that is not an edit, nothing is marked",
            out.dropped,
            times.len()
        ));
        out.refused = true;
        return out;
    }

    let mut marks = marks_from(times, &kept);
    // The text names the words, so the marks can (what a person reads in `retakes.tsv`); named off the spoken list and clipped to the run, so the name is the same whether or not an envelope moved the edges.
    for mark in marks.iter_mut() {
        mark.text = words
            .iter()
            .zip(times)
            .filter(|(_, (start, _))| *start >= mark.s && *start < mark.to)
            .map(|(word, _)| word.as_str())
            .collect::<Vec<_>>()
            .join(" ");
    }

    let (marks, notes) = edges::place_edges(marks, &[], &mut edge_of, &[]);
    out.logs.extend(notes);
    for mark in &marks {
        out.logs.push(format!(
            ">>> text edit: {}-{} goes ({:?})",
            mm_ss(mark.s),
            mm_ss(mark.to),
            mark.text
        ));
    }
    out.logs.push(format!(
        ">>> text edit: {} of {} words removed in {} stretch(es)",
        out.dropped,
        times.len(),
        marks.len()
    ));
    out.marks = marks;
    out
}

/// Write the text where Prepare keeps it, via the same 0644 writer.
pub fn write_final(tree: &Tree, text: &str) -> Result<(), String> {
    let path = tree.final_txt();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    std::fs::write(&path, text).map_err(|err| format!("{}: {err}", path.display()))
}

/// Write the remade marks where Cut reads them. Their fresh mtime stops the next Cut ▶ remaking the same edit.
pub fn write_marks(tree: &Tree, marks: &[Retake]) -> Result<(), String> {
    textfmt::write_retakes(marks, &tree.retakes_tsv())
}

/// The per-source folder key: the base name without extension, from any stored form (`project:sources/lecture.mkv`, `sources/lecture.mkv`, `lecture.mkv`); must agree with [`crate::prepare_data`]'s row name.
pub(crate) fn source_key(stored: &str) -> String {
    let name = stored.rsplit('/').next().unwrap_or(stored);
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[..dot].to_string(),
        _ => name.to_string(),
    }
}

/// The session's lanes for F1.11's edge placement: keyed as Prepare keys them, placed by their OWN file name, stamped with size/mtime so a cache for a replaced file misses.
/// [`crate::fix_transcripts::placement`] MUST get the WHOLE session (leaving a take out moves second nought and slides every lane). A source that cannot be stat-ed contributes no lane.
pub(crate) fn session_lanes(tree: &Tree, sources: &[String]) -> Vec<edges::Lane> {
    // The clock is keyed on the file NAME but the stat needs the WHOLE stored path (`project:sources/lecture.mkv` lives inside the project); both travel together.
    let named: Vec<(&str, &str)> = sources
        .iter()
        .map(|stored| (stored.rsplit('/').next().unwrap_or(stored), stored.as_str()))
        .collect();
    let names: Vec<&str> = named.iter().map(|(name, _)| *name).collect();
    let offsets = crate::fix_transcripts::placement(&names);
    named
        .iter()
        .zip(offsets)
        .filter_map(|((name, stored), off)| {
            let (size, mtime) = stat_recording(tree, stored)?;
            Some(edges::Lane {
                key: source_key(name),
                off,
                size,
                mtime,
            })
        })
        .collect()
}

/// The recording's own size and mtime from the stored path (`project:` resolves inside the project). These are what [`crate::wave::read`] checks the cache header against, so never take them from the cache file itself.
fn stat_recording(tree: &Tree, stored: &str) -> Option<(i64, i64)> {
    let path = match stored.strip_prefix("project:") {
        Some(rel) => tree.dir().join(rel),
        None => std::path::PathBuf::from(stored),
    };
    let meta = std::fs::metadata(&path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some((meta.len() as i64, mtime))
}

/// A file's modification time, or `None`; read off the disk so an edit by any program counts.
fn mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

/// The session's spoken words and seconds from each source's `words.json`, in source order (the order `final.txt` was written from, so word *n* matches).
///
/// `start_sample`/`end_sample` are 16 kHz samples off the source start (§04 F1.3), converted via [`crate::transcribe::SAMPLE_RATE`]; the session offset is not added since the marks are read against the same list.
/// A source with no `words.json` contributes nothing (never transcribed). Stored paths become the folder key Prepare writes under (`project:sources/lecture.mkv` → `lecture`); the bare form reduces to the same key.
pub fn session_words(tree: &Tree, sources: &[String]) -> (Vec<String>, Vec<(f64, f64)>) {
    let hz = crate::transcribe::SAMPLE_RATE as f64;
    let mut words = Vec::new();
    let mut times = Vec::new();
    for source in sources {
        let Ok(Some(doc)) = crate::requests::read_words(tree, &source_key(source)) else {
            continue;
        };
        for word in doc.words {
            if word.word.trim().is_empty() {
                continue;
            }
            words.push(word.word.clone());
            times.push((word.start_sample as f64 / hz, word.end_sample as f64 / hz));
        }
    }
    (words, times)
}

/// S2: the step the next Cut ▶ takes when the text was edited since the marks were written. `None` when the text is not newer than `retakes.tsv` (§F1.12 `N -- no`). Otherwise the lines and marks; marks are already written unless the edit crossed the ceiling (`P.machine.retakeCeil`, [`RETAKE_CEIL`]), when nothing was written.
///
/// Edges: each lane's `cache/waves/<key>.wave` is read and, if stamped against this recording, its envelope moves the mark onto the sound; without one the marks sit on word times (normal, nothing in `src/` writes that cache yet). The words fence the edge either way.
pub fn before_cut(tree: &Tree, sources: &[String]) -> Option<Outcome> {
    let final_mtime = mtime(&tree.final_txt());
    let marks_mtime = mtime(&tree.retakes_tsv());
    if !edited(final_mtime, marks_mtime) {
        return None;
    }
    let Ok(text) = std::fs::read_to_string(tree.final_txt()) else {
        // Readable a moment ago and not now: treat as no edit, the user may still be saving.
        return None;
    };
    let (words, times) = session_words(tree, sources);
    // Nothing was ever spoken: nothing to remake or refuse, and no marks file written over a good one.
    if times.is_empty() {
        return Some(Outcome {
            logs: vec![EDITED_NOTE.to_string()],
            ..Default::default()
        });
    }
    let held = edges::load(tree, &session_lanes(tree, sources));
    let outcome = remake(&text, &words, &times, edges::finder(&held));
    if !outcome.refused {
        // Writing the marks ends the edit: the new mtime answers S2 with "no", so the remake happens once.
        if let Err(reason) = write_marks(tree, &outcome.marks) {
            let mut outcome = outcome;
            outcome.logs.push(format!("!!! text edit: {reason}"));
            outcome.refused = true;
            return Some(outcome);
        }
    }
    Some(outcome)
}
