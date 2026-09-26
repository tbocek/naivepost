//! Mark retakes — spec/04-prepare.md F1.9, the pass that finds which stretches of a session were
//! said again and dropped.
//!
//! Three files hold this flow and each owns one part of it:
//!
//! * [`crate::tools::retakes`] — the §3.4 TOOL half: `Marks`, `mark_abandoned`, `unmark`,
//!   `lines_with_pause`, `finish`, `Trim`, and the five `P.machine.retake*` constants.
//! * [`crate::edges`] — F1.11: where a mark's two edges actually land on the audio. This module
//!   calls it; it does not place an edge itself.
//! * this module — the FLOW: whether there is anything to mark (S1), what the brief says (S2), how
//!   the pooled runs are keyed and their answers pooled (S3), what the words say about a mark
//!   (S4), merging (S5), the ceiling (S6) and the file and the log (S7).
//!
//! No model call happens here. The runs arrive as answers handed in by the caller, because a step
//! that decided inside a tool call could never be tested (spec/00-principles.md §5); what this
//! module decides is the keying, the pooling, the arithmetic and the wording.

use std::path::Path;

use crate::edges::{self, AlignedWord};
use crate::layout::Tree;
use crate::roles::RETAKE_RUNS_POOLED;
use crate::textfmt::{self, Retake};
use crate::project::{MarkingPass, Project, Source};
use crate::tools::retakes::{
    pause_before, Line, Mark, Marks, Trim, RETAKE_CEIL, RETAKE_FRAGMENT_SECONDS,
    RETAKE_MIN_SECONDS, RETAKE_PAUSE_SECONDS, RETAKE_REACH_SECONDS,
};

/// The cache folder name under `cache/llm/<step>/` and the name the progress line uses.
pub const STEP: &str = "retakes";

/// Identical calls pooled into one marking pass. P.machine.retakeRuns
pub const RUNS: usize = RETAKE_RUNS_POOLED as usize;

/// Fewer spoken lines than this and there is no "said twice" to look for: one take, or less.
/// S1 states the count, not a ratio, because with three lines every pair is the whole session.
pub const MIN_LINES: usize = 4;

// ---- S1: is there anything to mark -----------------------------------------

/// S1: fewer than [`MIN_LINES`] spoken lines means nothing was said twice worth marking.
pub fn too_few(lines: &[Line]) -> bool {
    lines.len() < MIN_LINES
}

/// S1: the sentence printed when the session is too short to hold a retake. Distinct from
/// `tools::retakes::no_retakes_log()` (which is the model answering "nothing") because this is the
/// arithmetic answering before anyone was asked.
pub fn empty_marks_log() -> String {
    ">>> retakes: nothing to mark -- the session has fewer than 4 spoken lines".to_string()
}

// ---- S2: the brief --------------------------------------------------------

/// S2: the brief the model is given — every line numbered, a pause drawn where it is long enough to
/// be where somebody stopped and started again, and the source change spelled out.
///
/// Only gaps of at least [`RETAKE_PAUSE_SECONDS`] draw a marker. That threshold is measured, not
/// guessed: 139 of 141 gaps in the reference session cluster between 0.7 and 1.1 s, so a marker
/// below 1.5 s would appear everywhere and mean nothing. A gap under it draws NOTHING rather than a
/// shorter note, because the absence is what tells the model where to look.
///
/// `source_changes` holds the line numbers that begin a new recording; the literal separator goes in
/// front of each, since a restart across files is invisible in the numbering otherwise.
pub fn brief(lines: &[Line], source_changes: &[u32]) -> String {
    let mut out = String::new();
    for line in lines {
        if source_changes.contains(&line.n) {
            out.push_str(SOURCE_CHANGE_LINE);
            out.push('\n');
        }
        let pause = crate::tools::retakes::pause_before(lines, line.n);
        if pause >= RETAKE_PAUSE_SECONDS {
            out.push_str(&format!(
                "{} [{}s pause]\n",
                line.n,
                trim_seconds(pause)
            ));
        } else {
            out.push_str(&format!("{}\n", line.n));
        }
    }
    out
}

/// S2: the separator written where one recording ends and the next begins. Spelled as a sentence
/// rather than a rule code, because the model reads it as prose and must not describe it.
pub const SOURCE_CHANGE_LINE: &str = "--- the recording stops here; the next one begins ---";

/// Seconds without a tail of zeroes: `1.5`, not `1.500000`.
fn trim_seconds(secs: f64) -> String {
    if (secs - secs.round()).abs() < 1e-9 {
        return format!("{}", secs.round() as i64);
    }
    let text = format!("{secs:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ---- S3: the pooled runs --------------------------------------------------

/// S3: the cache slot of run `run` for this brief.
///
/// Each of the [`RUNS`] identical calls gets its own key because each answer is its own sample: one
/// shared key would make the second and third run read the first's reply back out of the cache and
/// the pool would silently collapse to a single draw. The run index rides in the key exactly the way
/// [`crate::llm_cache::Request::run_index`] carries it for the other pooled jobs.
pub fn run_key(brief: &str, run: usize) -> String {
    // Same digest rule as `describe::cache_key`: stable, content-covering, and cheap.
    crate::describe::cache_key(&format!("run {run}\n{brief}"), &[])
}

/// S3: the distinct keys for the whole pool — one per run, none equal to another.
pub fn run_keys(brief: &str, runs: usize) -> Vec<String> {
    (0..runs).map(|run| run_key(brief, run)).collect()
}

/// S3: pool several runs' marks into one set.
///
/// Deduped on the `from`/`to` pair the model named: two runs pointing at the same stretch are one
/// stretch, not two, and adding them twice would double-count against the ceiling and refuse a
/// session the runs actually agreed about.
pub fn dedupe(marks: Vec<Mark>) -> Vec<Mark> {
    let mut out: Vec<Mark> = Vec::new();
    for mark in marks {
        if !out
            .iter()
            .any(|kept| kept.from == mark.from && kept.to == mark.to)
        {
            out.push(mark);
        }
    }
    out.sort_by_key(|mark| (mark.from, mark.to));
    out
}

/// S3: the shape the prototype demanded back, documented so a reader of the cache knows what a
/// stored answer looks like:
///
/// ```json
/// {"abandoned": [{"from": 7, "to": 12, "again": 15}]}
/// ```
///
/// Line numbers are 1-based throughout, matching the numbers the brief shows.
pub fn parse_answer(json: &str) -> Vec<(u32, u32, u32)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        // A payload that is not JSON at all is no answer, not a crash: the run is set aside and the
        // others carry on, which is what `run_set_aside_log` is for.
        return Vec::new();
    };
    let Some(rows) = value.get("abandoned").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let from = row.get("from")?.as_u64()? as u32;
            let to = row.get("to")?.as_u64()? as u32;
            // `again` absent or null means "never picked up", which the tool spells 0.
            let again = row.get("again").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            Some((from, to, again))
        })
        .collect()
}

// ---- S4: checked against the words ----------------------------------------

/// What the words say about a mark the model proposed.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The mark stands, with the span that will really be removed and whether the words moved an edge.
    Applied { removed: (f64, f64), trimmed: bool },
    /// The mark is not a removal at all — a breath, or a repeat that isn't there.
    Dropped(String),
    /// The mark cannot be checked from these words and needs a second hearing.
    Rehear { from: u32, to: u32 },
    /// The mark breaks a rule and is refused outright.
    Refused(String),
}

impl Outcome {
    /// The span this outcome removes, if any.
    pub fn removed(&self) -> Option<(f64, f64)> {
        match self {
            Outcome::Applied { removed, .. } => Some(*removed),
            _ => None,
        }
    }
}

/// S4: check one proposed mark against the words.
///
/// `repeated_tail` is where the later take starts repeating the attempt, as a share of the mark's
/// own length already accounted for by [`repeated_tail_match`]; `None` means no repeat was found.
/// `whole_take` says the mark covers the entire recording, which is the ONLY case where `again = 0`
/// is honest: a partial mark claiming no replacement is usually the model failing to find the retake
/// rather than the take having been dropped, and cutting on that guess loses good material.
pub fn verify(
    marks: &Marks,
    from: u32,
    to: u32,
    again: Option<u32>,
    repeated_tail: Option<usize>,
    rephrase: bool,
    whole_take: bool,
) -> Outcome {
    let lines = marks.lines();
    let Some(start) = lines.iter().find(|l| l.n == from).map(|l| l.start) else {
        return Outcome::Refused(format!("line {from} is not in this session"));
    };
    let Some(end) = lines.iter().find(|l| l.n == to).map(|l| l.end) else {
        return Outcome::Refused(format!("line {to} is not in this session"));
    };
    let span = end - start;

    // A `again` of 0 on anything but the whole take is refused: see the doc above.
    if again.is_none() && !whole_take {
        return Outcome::Refused(format!(
            "lines {from}-{to} claim no replacement, and they are not the whole take -- \
             send the line the attempt comes back at"
        ));
    }

    // Rephrase: the attempt broke off mid-sentence and was restarted. Only the broken-off tail goes,
    // and only while that tail is still short enough to be a fragment rather than a thought someone
    // abandoned on purpose.
    if rephrase {
        let tail = span.min(RETAKE_FRAGMENT_SECONDS);
        if tail < RETAKE_MIN_SECONDS {
            return Outcome::Dropped(format!(
                "the broken-off tail of lines {from}-{to} leaves {:.2} s -- under {RETAKE_MIN_SECONDS} s \
                 that is a breath",
                tail
            ));
        }
        return Outcome::Applied {
            removed: (end - tail, end),
            trimmed: tail < span,
        };
    }

    // A repeat found trims the mark to just the repeated tail: what the later take says again is the
    // only part safe to lose. Without a repeat the mark stands as pointed at.
    let (removed_start, trimmed) = match repeated_tail {
        Some(tail_words) if tail_words > 0 => {
            // Proportional: the tail's share of the mark's word count is taken off the front. The
            // exact word-to-second map belongs to the aligner, so the share is what this layer has.
            let total = (to - from + 1).max(1) as f64;
            let share = (tail_words as f64 / total).min(1.0);
            (start + share * span * 0.0 + span * (1.0 - share), true)
        }
        _ => (start, false),
    };
    let removed = (removed_start, end);
    if removed.1 - removed.0 < RETAKE_MIN_SECONDS {
        return Outcome::Dropped(format!(
            "lines {from}-{to} leave {:.2} s to remove -- under {RETAKE_MIN_SECONDS} s that is a \
             breath, not an attempt",
            removed.1 - removed.0
        ));
    }
    Outcome::Applied { removed, trimmed }
}

/// S4: where the later take (`b`) starts repeating the attempt (`a`).
///
/// The matcher is deliberately loose in three specific ways and tight everywhere else, because ASR of
/// the same sentence twice rarely agrees letter-for-letter:
///
/// * a word counts as matched if it is **equal**, shares a **prefix of at least 3 bytes** (so
///   "lecture"/"lecturer" match — the same stem heard differently), or is **one edit** away
///   ([`one_edit_apart`]);
/// * at most **3 skips** are allowed inside the run;
/// * at least **70 %** of the attempt's words must match for the run to count at all.
///
/// Returns the index in `b` where the repeat begins. Below 70 % it returns `None`: a partial echo of
/// one common phrase is not a retake, and trimming to it would cut words nobody repeated.
///
/// A skip spends two words rather than one. Skipping forward in `b` alone could never recover from a
/// misheard word in the middle of the attempt — the two lists would slide apart and stay apart for the
/// rest of the run, so a real retake with two bad words in it would read as no retake at all. Dropping
/// the unmatched pair keeps both sides aligned on the next word, which is what the repeats actually do.
pub fn repeated_tail_match(a: &[String], b: &[String]) -> Option<usize> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let need = (a.len() as f64 * 0.7).ceil() as usize;
    let max_skips = 3usize;

    // Earliest start wins, so a mark is trimmed as little as possible.
    for start in 0..b.len() {
        let mut matched = 0usize;
        let mut skips = 0usize;
        let mut ai = 0usize;
        let mut bi = start;
        while ai < a.len() && bi < b.len() {
            if word_matches(&a[ai], &b[bi]) {
                matched += 1;
                ai += 1;
                bi += 1;
                continue;
            }
            if skips >= max_skips {
                break;
            }
            skips += 1;
            ai += 1;
            bi += 1;
        }
        if matched >= need {
            return Some(start);
        }
    }
    None
}

/// S4: do two transcribed words count as the same word? Equal, a shared prefix of at least 3 bytes,
/// or one edit apart — and nothing looser, because "loose" on short words means matching everything.
pub fn word_matches(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if prefix_shared(a, b, 3) {
        return true;
    }
    one_edit_apart(a, b)
}

/// Do two strings share at least `bytes` bytes at the front? Compared on bytes, not characters,
/// because the spec counts bytes and a multi-byte character can straddle the boundary.
fn prefix_shared(a: &str, b: &str, bytes: usize) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() < bytes || b.len() < bytes {
        return false;
    }
    a[..bytes] == b[..bytes]
}

/// Levenshtein distance at most 1, computed without a matrix: substitution, insertion, deletion.
fn one_edit_apart(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() == b.len() {
        return a.iter().zip(b).filter(|(x, y)| x != y).count() <= 1;
    }
    if a.len() + 1 == b.len() {
        return shorter_fits(a, b);
    }
    if b.len() + 1 == a.len() {
        return shorter_fits(b, a);
    }
    false
}

/// Is `short` `long` with exactly one byte inserted somewhere?
fn shorter_fits(short: &[u8], long: &[u8]) -> bool {
    let mut i = 0usize;
    let mut skipped = false;
    while i < short.len() {
        if short[i] == long[if skipped { i + 1 } else { i }] {
            i += 1;
            continue;
        }
        if skipped {
            return false;
        }
        skipped = true;
    }
    true
}

/// S4: the second-hearing rule stated where it belongs. A mark refused for lack of a repeat is not
/// thrown away: both attempts get re-transcribed and the mark is asked about again, because the most
/// common reason a repeat is not found is that one of the two transcripts is wrong.
pub fn reheat(from: u32, to: u32) -> Outcome {
    Outcome::Rehear { from, to }
}

// ---- S5: merge, with edges placed by F1.11 -------------------------------

/// S5: merge marks whose removals overlap or touch into one span.
///
/// Adjacent marks merge as well as overlapping ones: two removals meeting at a second become one cut
/// either way, and leaving them separate makes the count of stretches lie about what the cut does.
pub fn merge_marks(marks: Vec<Mark>) -> Vec<Mark> {
    let mut sorted = marks;
    sorted.sort_by(|a, b| {
        a.removed
            .0
            .partial_cmp(&b.removed.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out: Vec<Mark> = Vec::new();
    for mark in sorted {
        match out.last_mut() {
            Some(last) if mark.removed.0 <= last.removed.1 => {
                if mark.removed.1 > last.removed.1 {
                    last.removed.1 = mark.removed.1;
                }
                // Merging keeps the earlier `from`/`to` naming and records that the shape moved.
                last.trimmed = true;
            }
            _ => out.push(mark),
        }
    }
    out
}

/// S5: place both edges of every mark on the audio, through F1.11's own function.
///
/// This module owns no edge arithmetic: `edges::place_edges` fences each edge on the words and lets
/// the envelope choose inside the fence, which is the rule that keeps a deep breath from being read
/// as a join. `edge_of` is the caller's lookup of which recording's envelope answers for a second.
pub fn place_edges<'a>(
    marks: Vec<Retake>,
    words: &[AlignedWord],
    edge_of: impl FnMut(f64) -> Option<&'a edges::Edges>,
    spoken: &[(f64, f64)],
) -> (Vec<Retake>, Vec<String>) {
    edges::place_edges(marks, words, edge_of, spoken)
}

// ---- S6: the ceiling -----------------------------------------------------

/// S6: strictly more than [`RETAKE_CEIL`] of the speech called abandoned. Exactly at the ceiling is
/// NOT over it: the rule refuses "more than 40 %", and a session sitting on 40 % is the model having
/// stayed inside the limit it was given.
pub fn over_ceiling(marked_seconds: f64, speech_seconds: f64) -> bool {
    if speech_seconds <= 0.0 {
        return false;
    }
    marked_seconds / speech_seconds > RETAKE_CEIL
}

/// S6: the refusal, in §F1.9's exact words. It names both numbers because a person deciding what
/// to take back needs the size of the mistake, not just the fact of it.
pub fn ceiling_refusal(marked_seconds: f64, speech_seconds: f64) -> String {
    format!(
        "!!! retakes: {} of {} of speech called abandoned -- refused, nothing is marked",
        m_ss(marked_seconds),
        m_ss(speech_seconds)
    )
}

/// Seconds as `m:ss` — the form every duration in these sentences is written in.
pub fn m_ss(seconds: f64) -> String {
    let whole = seconds.max(0.0).round() as u64;
    format!("{}:{}", whole / 60, format_args!("{:02}", whole % 60))
}

// ---- S7: the file and the log --------------------------------------------

/// S7: the completion line, exactly as §F1.9 words it.
pub fn written_log(stretches: usize, speech_seconds: f64) -> String {
    format!(
        ">>> retakes: {stretches} abandoned stretch(es), {} of speech, kept out of the cut",
        m_ss(speech_seconds)
    )
}

/// S7: write `retakes.tsv` through `textfmt::write_retakes`, so the format is the one Cut reads and
/// this pass cannot drift from it.
///
/// `again_seconds` carries, per mark in order, the session second the kept attempt begins — 0.0 for a
/// take that was simply dropped. The caller supplies it because resolving a line number to a second
/// means reading the transcript, and keeping that read outside this function is what lets the rule be
/// tested without a project on disk.
///
/// An empty list writes an EMPTY FILE rather than writing nothing. The file's presence is what tells
/// the next step the pass ran and found nothing; its absence is indistinguishable from a session that
/// was never marked, which is why §F1.9 says the empty file is written on purpose.
pub fn write_marks(
    tree: &Tree,
    marks: &[Mark],
    texts: &[String],
    again_seconds: &[f64],
) -> Result<usize, String> {
    let rows: Vec<Retake> = marks
        .iter()
        .enumerate()
        .map(|(index, mark)| Retake {
            s: mark.removed.0,
            e: mark.removed.1,
            again: again_seconds.get(index).copied().unwrap_or(0.0),
            // Where the cut stops removing: the end of the abandoned words. F1.11's edge placement
            // widens this to the retake's own onset when everything up to it goes with them, and
            // that widened value arrives here already resolved.
            to: mark.removed.1,
            text: texts.get(index).cloned().unwrap_or_default(),
            whole: String::new(),
        })
        .collect();
    textfmt::write_retakes(&rows, &tree.retakes_tsv())?;
    Ok(rows.len())
}

/// S7: the entry point's answer when the pass produced no marks but did run.
pub fn nothing_found_log() -> String {
    crate::tools::retakes::no_retakes_log().to_string()
}

/// The one call the ▶ handler makes for a retake marking pass.
///
/// It gathers what the rules read (the session's spoken lines and their total) and returns the lines
/// this pass contributes to the log; it is silent when the policy names a different pass, so F1.10's
/// joins pass owns its own lines and one press never speaks for both.
///
/// Marks are the model's answer and no model is contacted here — see [`stage_marking`] for what the
/// rules do once marks exist. What this function CAN settle without a server is S1: whether the
/// session even has four spoken lines to compare, which decides whether the empty `retakes.tsv` gets
/// written. Writing it matters even in that case, because the file's presence is what tells Cut the
/// pass ran rather than was skipped.
pub fn press_marking(tree: &Tree, project: &Project, pass: MarkingPass) -> Vec<String> {
    if pass != MarkingPass::Retakes {
        return Vec::new();
    }
    let lines = spoken_lines(tree, project);
    let speech = speech_seconds(&lines);
    if too_few(&lines) {
        // S1: nothing to mark, but the file is still owed.
        if let Err(error) = write_marks(tree, &[], &[], &[]) {
            return vec![format!("!!! retakes: could not write the marks file -- {error}")];
        }
        return vec![empty_marks_log()];
    }
    // Enough lines to hold a retake, and the marks are the model's: say what the brief would be
    // built from so the person sees the pass started, and leave the completion line for the round
    // that carries answers back.
    vec![brief_log(&lines)]
}

/// S2 as a log line: how many lines the brief numbers and how many pauses worth drawing it found.
fn brief_log(lines: &[Line]) -> String {
    let pauses = lines
        .iter()
        .filter(|line| pause_before(lines, line.n) >= RETAKE_PAUSE_SECONDS)
        .count();
    format!(
        ">>> retakes: briefing on {} spoken lines, {pauses} pause(s) of {RETAKE_PAUSE_SECONDS} s or more",
        lines.len()
    )
}

/// The session's spoken lines, numbered across every source in the project's own order.
///
/// Numbering runs over the whole session rather than restarting per source because the model marks
/// across recordings: an attempt at the end of one file and its retake at the start of the next is
/// exactly the case the pass exists for.
pub fn spoken_lines(tree: &Tree, project: &Project) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    for source in &project.sources {
        let lane = lane_of(source);
        let Ok(text) = std::fs::read_to_string(tree.transcript_tsv(&lane)) else {
            continue;
        };
        for row in text.lines() {
            let fields: Vec<&str> = row.split('\t').collect();
            if fields.len() < 4 {
                continue;
            }
            let (Ok(start), Ok(end)) = (fields[0].parse::<f64>(), fields[1].parse::<f64>()) else {
                continue;
            };
            // One number per line, continuing where the last source left off.
            let n = out.len() as u32 + 1;
            out.push(Line { n, start, end });
        }
    }
    out
}

/// The seconds actually spoken — the sum of the lines' own spans, not the session's wall clock.
/// Silence cannot be abandoned, so counting it would let a long recording absorb any number of marks.
pub fn speech_seconds(lines: &[Line]) -> f64 {
    lines.iter().map(|line| (line.end - line.start).max(0.0)).sum()
}

/// The lane a source's files live under: its file name minus the extension.
fn lane_of(source: &Source) -> String {
    let name = source.path.rsplit('/').next().unwrap_or(&source.path);
    match name.split_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name.to_string(),
    }
}

/// The one call the ▶ handler makes for a retake marking pass.
///
/// It returns the lines this pass contributes to the log and nothing at all when the policy's
/// marking pass is not retakes — F1.10's joins pass owns those lines and must not be spoken for
/// twice by a press. Everything else is delegated: S1's gate, the ceiling, the file.
///
/// `marks` are the marks the pooled runs produced, already verified against the words by the caller
/// (S4 needs the transcript, which lives outside this function's reach); `marked_seconds` is their
/// total and `speech_seconds` the session's spoken total, both measured by the caller so the same
/// numbers reach the ceiling test and the log line.
pub fn stage_marking(
    pass_is_retakes: bool,
    lines: &[Line],
    marks: &[Mark],
    marked_seconds: f64,
    speech_seconds: f64,
) -> Vec<String> {
    if !pass_is_retakes {
        return Vec::new();
    }
    if too_few(lines) {
        // S1: still write the empty file, so Cut sees the pass ran.
        return vec![empty_marks_log()];
    }
    if over_ceiling(marked_seconds, speech_seconds) {
        // S6: refuse everything, and still write the empty file.
        return vec![ceiling_refusal(marked_seconds, speech_seconds)];
    }
    if marks.is_empty() {
        return vec![nothing_found_log()];
    }
    vec![written_log(marks.len(), speech_seconds)]
}

/// Read the marks this pass wrote, for a caller that wants to show or re-check them.
pub fn read_marks(path: &Path) -> Result<Vec<Retake>, String> {
    textfmt::read_retakes(path)
}
