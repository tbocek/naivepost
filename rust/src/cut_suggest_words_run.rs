//! F2.14 §S2a — the WORDS cut: `P.policy.cutMode = words`, no model asked.
//!
//! [`crate::suggest::words_cut`] already does the arithmetic and already answers the page's two
//! sentences; what was missing is the reading side — nobody handed it the runs, the marks or the
//! silences, so the words path could only be called from a test. This module is that reading side
//! and the landing, split out of `ui::window` because `window.rs` is past its size budget.
//!
//! Landings go through [`crate::ui::cut_suggest_land::land_spans`], which keeps hand-placed inserts
//! exactly as the model path's `suggested_cut` does: a words cut proposes scenes, it is not anybody's
//! claim on a card a person placed.

use crate::cut::Cut;
use crate::layout::Tree;
use crate::suggest::SpokenRun;

/// The three arguments [`crate::suggest::words_cut`] takes, read out of the project.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inputs {
    pub runs: Vec<SpokenRun>,
    pub marks: Vec<(f64, f64)>,
    pub silences: Vec<(f64, f64)>,
}

/// Where one word starts and ends, in its recording's own seconds: the aligner's times when it answered and
/// otherwise the ASR's, which is the same preference [`crate::word_list`] uses to build its list —
/// one preference, spelled once, or the two would place cuts on different seconds.
fn word_spans(tree: &Tree, source: &str) -> Vec<(f64, f64)> {
    let hz = crate::transcribe::SAMPLE_RATE as f64;
    let timed = match crate::requests::read_aligned(tree, source) {
        Ok(Some(doc)) if !doc.words.is_empty() => doc.words,
        _ => crate::requests::read_words(tree, source)
            .ok()
            .flatten()
            .map(|doc| doc.words)
            .unwrap_or_default(),
    };
    timed
        .iter()
        .map(|word| (word.start_sample as f64 / hz, word.end_sample as f64 / hz))
        .collect()
}

/// §S2a's runs: every filmed run of the cut on screen, each carrying the seconds of the words its own
/// recording holds, MOVED ONTO THE SESSION CLOCK (§04 F1.9 keeps a tape's seconds in its own file).
/// A run whose recording has no words keeps its own span — nothing was said in it, so there is nothing
/// to trim to contributes NO run at all — §S2a builds the cut from what was said, so silent footage is
/// simply not part of it.
///
/// The spans are the page's TAPE, every filmed recording under its own name and on the session clock
/// (`cut_folds::page_recordings`), because §S2a builds the cut from "every filmed run". The first rewrite took
/// them from the cut on screen instead, named `cam<N>`: a project with no cut yet had no runs, and a recording's
/// words, filed under its own name, were never found under `cam<N>`, so every words cut came out "0 segments"
/// (07-news, 2026-10-26). The cut's own spans stand in only where no tape is published (a page not built yet).
/// A suggestion is refused while hand edits stand (§F2.14 S1), so a run dropped by hand cannot come back here.
pub fn runs_of(tree: &Tree, cut: &Cut) -> Vec<SpokenRun> {
    let tape = crate::ui::page_recordings();
    let recordings = if tape.is_empty() {
        crate::timeline::kept_footage_recordings(cut)
    } else {
        tape
    };
    // The envelopes the edges are placed on, as the marks' are (F1.11). A recording with none takes the word pad.
    let sources: Vec<String> = crate::project::load(tree.dir())
        .map(|project| project.sources.iter().map(|source| source.path.clone()).collect())
        .unwrap_or_default();
    let held = crate::edges::load(tree, &crate::hand_edit::session_lanes(tree, &sources));
    let edge_of = crate::edges::finder(&held);
    let mut runs = Vec::new();
    for span in crate::timeline::filmed_runs(&recordings) {
        let Some(rec) = recordings
            .iter()
            .find(|rec| rec.start <= span.0 && span.0 < rec.end)
            .cloned()
        else {
            continue;
        };
        // §04 F1.9's preference, spelled once: the aligner's times when it answered, the ASR's otherwise.
        let spoken = word_spans(tree, &rec.base);
        // A tape's own second onto the session clock: `sessionAt(local)` for this recording, exactly as
        // §B defines it, so a word 5 s into a tape that opened at 0:20 is a word at 0:25 of the session.
        let on_session = |local: f64| {
            crate::timeline::session_at(
                local,
                rec.start,
                0.0,
                crate::timeline::shift_of(cut, &rec.base),
            )
        };
        // Only words the run actually shows count; a recording that ran twice in the session keeps each
        // stretch to its own words.
        let mut inside: Vec<(f64, f64)> = spoken
            .iter()
            .map(|(from, to)| (on_session(*from), on_session(*to)))
            .filter(|(from, _)| *from >= span.0 && *from < span.1)
            .collect();
        // The same correction the word list makes: a first word stamped ahead of its sound starts on the sound.
        if let Some(sound) = inside.first().and_then(|first| edge_of(first.0)) {
            if let Some(moved) =
                crate::edges::misplaced_first_word(&inside, |a, b| sound.peak_between(a, b))
            {
                inside[0] = moved;
            }
        }
        let earliest = inside.iter().map(|word| word.0).reduce(f64::min);
        let latest = inside.iter().map(|word| word.1).reduce(f64::max);
        let (Some(earliest), Some(latest)) = (earliest, latest) else {
            // Nothing was said while this footage is on screen. No run, rather than a run whose
            // "words" are the footage itself: §S2a builds the cut from what was said, and a silent
            // stretch answers "nothing", not "keep all of it".
            continue;
        };
        // Both edges placed by the sound, inside the reach of their words: the first word's onset and the last
        // word's own tail, the same fenced placement a mark's edges get.
        let word = |s: f64, e: f64| crate::edges::AlignedWord {
            word: String::new(),
            s,
            e,
        };
        let first = inside.iter().find(|w| w.0 == earliest).copied();
        let last = inside.iter().find(|w| w.1 == latest).copied();
        let placed = match (edge_of(earliest), edge_of(latest), first, last) {
            (Some(opening), Some(closing), Some(first), Some(last)) => Some((
                opening.start_before(
                    &word(first.0, first.1),
                    (earliest - crate::suggest::TRIM_REACH_SECONDS).max(span.0),
                ),
                closing.end_after(
                    &word(last.0, last.1),
                    (latest + crate::suggest::TRIM_REACH_SECONDS).min(span.1),
                ),
            )),
            _ => None,
        };
        runs.push(SpokenRun {
            span,
            first_spoken: earliest,
            last_spoken: latest,
            placed,
            words: inside,
        });
    }
    runs
}

/// §S2a's marks: the retake marks, as the session spans `words_cut` subtracts. A mark whose `to` is
/// not `e` removes everything up to the kept attempt, which is exactly the span written by
/// [`crate::hand_edit::write_marks`]; nothing about the file format is invented here.
pub fn marks_of(tree: &Tree) -> Vec<(f64, f64)> {
    match crate::textfmt::read_retakes(&tree.retakes_tsv()) {
        Ok(marks) => marks
            .iter()
            .map(|mark| (mark.s, if mark.to > mark.s { mark.to } else { mark.e }))
            .collect(),
        // An unreadable marks file is no marks: §S2a cuts the stretches it can see, and saying so is
        // the hand-edit flow's business (F1.12), not this cut's.
        Err(_) => Vec::new(),
    }
}

/// §S2a's silences: the dead air to take out of a kept clip, from each footage row's cached wave.
///
/// NO ENVELOPE IS NOT AN EMPTY SESSION: a row whose `cache/waves` file is absent, stale or unreadable
/// contributes nothing, so `remove_dead_air` simply finds nothing long enough in it. Guessing a
/// silence would cut the middle of a sentence, which is the one thing §S2a's dead-air step may not do.
pub fn silences_of(tree: &Tree, project: &crate::project::Project) -> Vec<(f64, f64)> {
    crate::cut_wave_edges::read_lanes(tree, &project.sources)
        .iter()
        .flat_map(crate::cut_wave_edges::silences_of)
        .collect()
}

/// All three inputs, read together.
pub fn inputs(tree: &Tree, project: &crate::project::Project, cut: &Cut) -> Inputs {
    Inputs {
        runs: runs_of(tree, cut),
        marks: marks_of(tree),
        silences: silences_of(tree, project),
    }
}

/// What the words cut left behind, in the shape both a page and a test can read.
#[derive(Debug, Clone, PartialEq)]
pub struct Landed {
    /// The sentence for the status line — `cut by the words: N segments`, `suggest::WordsCut`'s own.
    pub status: String,
    /// The line for `cut.log.json`, `>>> cut by the words: …`, also the module's own wording.
    pub log: String,
    /// Whether anything was written: false when the cut came out empty, because a page that saved an
    /// empty cut would have thrown away the cut it opened with for a result of nothing.
    pub wrote: bool,
    /// The cut now on the page.
    pub cut: Cut,
}

/// `run_words` with the page's own state in hand — the door `ui::cut_suggest_land::press_words` calls.
pub fn press_words(window: &adw::ApplicationWindow) -> String {
    let Some(Some(tree)) = crate::ui::cut_suggest_land::session_tree() else {
        return String::new();
    };
    let project = crate::ui::window::session_handle(window).borrow().clone();
    let made = run_words(window, &tree, &project);
    crate::ui::window::log_line(&made.log);
    made.status
}

/// §S2a: build the cut from the words and land it. No server is asked, so no cache is written and no
/// exchange page is opened — this is the whole job.
///
/// Takes the tree and the project it reads, rather than resolving them itself: the refusal door already
/// answered "is there a session" and pointed at one, and asking twice would let the refusal and the
/// work land on different projects.
pub fn run_words(
    window: &adw::ApplicationWindow,
    tree: &Tree,
    project: &crate::project::Project,
) -> Landed {
    let cut = crate::ui::window::newest_review_cut();
    let read = inputs(tree, project, &cut);
    let made = crate::suggest::words_cut(&read.runs, &read.marks, &read.silences);
    // §12 §4's fourth bullet: the walk-back runs after EVERY reply, "model- or hand-made", not only after
    // a model's. `words_cut` trims to words, takes marks and dead air out and coalesces — it does NOT close
    // holes or snap edges, because on this path there is no stamped answer to walk back. So the spans it
    // settled on go through the same walk here that `suggest::apply` runs for the model arm, which is the
    // only way hole closing and edge snapping ever happen on a words-mode press.
    let walked = crate::suggest::walk_back_spans(
        &made.segs,
        &crate::ui::cut_suggest_model_run::walk_of(&read),
    );
    // The walk removes the marks again over what it closed, so the wordless pieces come back and go again here.
    let segs = crate::suggest::drop_wordless(&walked.segs, &read.runs);
    let changed = segs != made.segs;
    if segs.is_empty() {
        // Nothing was kept. `suggest` has no sentence for this and inventing one is not this module's
        // job, so the page hears the same status it would have heard and the cut on screen stands.
        return Landed {
            status: made.status,
            log: made.log,
            wrote: false,
            cut,
        };
    }
    let landed = crate::ui::cut_suggest_land::land_spans(window, &segs);
    let mut log = made.log;
    if changed {
        // "The page shows what it did": the walk says its own account, appended to the words cut's line so
        // one log entry tells both halves of what the machine did to these seconds.
        log.push_str(&format!(
            "\n>>> {} ({} mark(s), {} of silence taken out)",
            crate::suggest::walk_report(walked.closed_len, walked.moved),
            walked.marks_out,
            crate::tools::mm_ss(walked.silence_out)
        ));
    }
    Landed {
        status: made.status,
        log,
        wrote: true,
        cut: landed,
    }
}
