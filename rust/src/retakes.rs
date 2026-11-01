//! Mark retakes — spec/04-prepare.md F1.9: finds which stretches of a session were said again and dropped.
//!
//! * [`crate::tools::retakes`] — the §3.4 TOOL half (`Marks`, `mark_abandoned`, `unmark`, `finish`, the `P.machine.retake*` constants).
//! * [`crate::edges`] — F1.11: where a mark's two edges land on the audio.
//! * this module — the FLOW: S1 gate, S2 brief, S3 keying and pooling, S4 words check, S5 merging, S6 ceiling, S7 file and log.
//!
//! No model call happens here: runs arrive as answers handed in by the caller, so it is testable (spec/00-principles.md §5).

use crate::describe::trim_seconds;
use crate::edges::{self, AlignedWord};
use crate::layout::Tree;
use crate::project::{MarkingPass, Project, Source};
use crate::roles::RETAKE_RUNS_POOLED;
use crate::textfmt::{self, Retake};
use crate::tools::retakes::{
    pause_before, Line, Mark, Marks, RETAKE_CEIL, RETAKE_FRAGMENT_SECONDS, RETAKE_MIN_SECONDS,
    RETAKE_PAUSE_SECONDS,
};

pub const STEP: &str = "retakes";

/// Identical calls pooled into one marking pass. P.machine.retakeRuns
pub const RUNS: usize = RETAKE_RUNS_POOLED as usize;

/// Fewer spoken lines than this and there is no "said twice" to look for. S1 states a count, not a ratio.
pub const MIN_LINES: usize = 4;

// ---- S1: is there anything to mark -----------------------------------------

/// S1: fewer than [`MIN_LINES`] spoken lines means nothing was said twice worth marking.
pub fn too_few(lines: &[Line]) -> bool {
    lines.len() < MIN_LINES
}

/// S1: the sentence printed when the session is too short to hold a retake (not the model answering "nothing", `tools::retakes::no_retakes_log()`).
pub fn empty_marks_log() -> String {
    ">>> retakes: nothing to mark -- the session has fewer than 4 spoken lines".to_string()
}

// ---- S2: the brief --------------------------------------------------------

/// S2: the brief the model is given: every line numbered, a pause drawn where it is long enough to be a restart, and the source change spelled out.
///
/// Only gaps of at least [`RETAKE_PAUSE_SECONDS`] draw a marker (measured: 139 of 141 gaps in the reference session sit between 0.7 and 1.1 s);
/// a shorter gap draws NOTHING, because the absence tells the model where to look.
///
/// `source_changes` holds the line numbers that begin a new recording; the separator goes in front of each.
pub fn brief(lines: &[Line], source_changes: &[u32]) -> String {
    let mut out = String::new();
    for line in lines {
        if source_changes.contains(&line.n) {
            out.push_str(SOURCE_CHANGE_LINE);
            out.push('\n');
        }
        let pause = crate::tools::retakes::pause_before(lines, line.n);
        if pause >= RETAKE_PAUSE_SECONDS {
            out.push_str(&format!("{} ({}s pause)\n", line.n, trim_seconds(pause)));
        } else {
            out.push_str(&format!("{}\n", line.n));
        }
    }
    out
}

/// S2: the separator where one recording ends and the next begins, a sentence the model reads as prose.
pub const SOURCE_CHANGE_LINE: &str = "--- the recording stops here; the next one begins ---";

// ---- S3: the pooled runs --------------------------------------------------

/// S3: pool several runs' marks into one set.
///
/// Deduped on the `from`/`to` pair: two runs pointing at the same stretch must not double-count against the ceiling.
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

/// S3: the shape the prototype demanded back, as documented for readers of the cache:
///
/// ```json
/// {"abandoned": [{"from": 7, "to": 12, "again": 15}]}
/// ```
///
/// Line numbers are 1-based, matching the brief.
pub fn parse_answer(json: &str) -> Vec<(u32, u32, u32)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        // Not JSON at all is no answer, not a crash: the run is set aside (`run_set_aside_log`).
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
/// `repeated_tail` is where the later take starts repeating the attempt ([`repeated_tail_match`]); `None` means no repeat was found.
/// `whole_take` says the mark covers the entire recording, the ONLY case where `again = 0` is honest: a partial mark with no replacement
/// is usually the model missing the retake, and cutting on that guess loses good material.
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

    // Rephrase: the attempt broke off mid-sentence; only a fragment-sized tail goes, not a thought abandoned on purpose.
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

    // A repeat trims the mark to the repeated tail, the only part safe to lose.
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
/// Loose in three ways, tight elsewhere, because ASR of the same sentence twice rarely agrees letter-for-letter:
///
/// * a word counts as matched if it is **equal**, shares a **prefix of at least 3 bytes** (so
///   "lecture"/"lecturer" match — the same stem heard differently), or is **one edit** away
///   ([`one_edit_apart`]);
/// * at most **3 skips** are allowed inside the run;
/// * at least **70 %** of the attempt's words must match for the run to count at all.
///
/// Returns the index in `b` where the repeat begins; below 70 % it returns `None`, since a partial echo of a common phrase is not a retake.
///
/// A skip spends two words: skipping in `b` alone could not recover from a misheard word, and the lists would stay apart.
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

/// S4: do two transcribed words count as the same? Equal, a shared prefix of at least 3 bytes, or one edit apart; nothing looser.
pub fn word_matches(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if prefix_shared(a, b, 3) {
        return true;
    }
    one_edit_apart(a, b)
}

/// Do two strings share at least `bytes` bytes at the front? Bytes, not characters: the spec counts bytes.
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

/// S4: a mark refused for lack of a repeat is not thrown away: both attempts are re-transcribed and the mark asked about again.
pub fn reheat(from: u32, to: u32) -> Outcome {
    Outcome::Rehear { from, to }
}

// ---- S5: merge, with edges placed by F1.11 -------------------------------

/// S5: merge marks whose removals overlap or touch into one span.
///
/// Adjacent marks merge too: leaving them separate makes the stretch count lie about what the cut does.
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
/// No edge arithmetic here: `edges::place_edges` fences each edge on the words and lets the envelope choose inside the fence.
/// `edge_of` is the caller's lookup of which recording's envelope answers for a second.
pub fn place_edges<'a>(
    marks: Vec<Retake>,
    words: &[AlignedWord],
    edge_of: impl FnMut(f64) -> Option<&'a edges::Edges>,
    spoken: &[(f64, f64)],
) -> (Vec<Retake>, Vec<String>) {
    edges::place_edges(marks, words, edge_of, spoken)
}

// ---- S6: the ceiling -----------------------------------------------------

/// S6: strictly more than [`RETAKE_CEIL`] of the speech called abandoned; exactly at the ceiling is NOT over it ("more than 40 %").
pub fn over_ceiling(marked_seconds: f64, speech_seconds: f64) -> bool {
    if speech_seconds <= 0.0 {
        return false;
    }
    marked_seconds / speech_seconds > RETAKE_CEIL
}

/// S6: the refusal, in §F1.9's exact words, naming both numbers.
pub fn ceiling_refusal(marked_seconds: f64, speech_seconds: f64) -> String {
    format!(
        "!!! retakes: {} of {} of speech called abandoned -- refused, nothing is marked",
        m_ss(marked_seconds),
        m_ss(speech_seconds)
    )
}

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
/// S7: write `retakes.tsv` through `textfmt::write_retakes`, so the format is the one Cut reads.
///
/// `again_seconds` carries, per mark in order, the session second the kept attempt begins (0.0 for a dropped take); the caller resolves it from the transcript.
///
/// An empty list writes an EMPTY FILE: its presence tells the next step the pass ran and found nothing (§F1.9).
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
            // Where the cut stops removing: already resolved by F1.11's edge placement.
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
/// Gathers the session's spoken lines and returns this pass's log lines; silent when the policy names a different pass (F1.10 owns its own).
///
/// With four or more spoken lines the WHOLE pass runs through [`crate::retakes_ask::run_pass`]. Settled here: S1's too-few gate and the
/// empty `retakes.tsv` it owes Cut, plus the two ways there can be no address to dial. The file's presence tells Cut the pass ran, so even a failure writes it.
pub fn press_marking(tree: &Tree, project: &Project, pass: MarkingPass) -> Vec<String> {
    if pass != MarkingPass::Retakes {
        return Vec::new();
    }
    let lines = spoken_lines(tree, project);
    let speech = speech_seconds(&lines);
    if too_few(&lines) {
        // S1: nothing to mark, but the file is still owed.
        if let Err(error) = write_marks(tree, &[], &[], &[]).map(|_| ()) {
            return vec![format!(
                "!!! retakes: could not write the marks file -- {error}"
            )];
        }
        return vec![empty_marks_log()];
    }
    // Enough lines to hold a retake. With no settings folder nobody was contacted, so only S1's answer and the file owed Cut are settled.
    let paths = match crate::settings::from_environment() {
        Some(paths) => paths,
        None => return vec![brief_log(&lines)],
    };
    let conf = match crate::settings::read(&paths) {
        Ok(conf) => conf,
        Err(reason) => {
            // An unreadable settings file is reported, not guessed past; the empty file is still owed (§Failure-is-specific).
            let mut out = vec![format!(
                "!!! retakes: the settings could not be read -- {reason}"
            )];
            if let Err(error) = write_marks(tree, &[], &[], &[]) {
                out.push(format!(
                    "!!! retakes: could not write the marks file -- {error}"
                ));
            }
            return out;
        }
    };
    // The whole pass runs here, brief to file; every line it speaks is returned in order.
    let mut out: Vec<String> = Vec::new();
    let _ = speech;
    if let Err(reason) = crate::retakes_ask::run_pass(tree, project, &paths, &conf, &mut |line| {
        out.push(line.to_string())
    }) {
        // The flow's own arithmetic failed on disk: say which, and still owe the file.
        out.push(format!("!!! retakes: {reason}"));
        if let Err(error) = write_marks(tree, &[], &[], &[]) {
            out.push(format!(
                "!!! retakes: could not write the marks file -- {error}"
            ));
        }
    }
    out
}

/// S2 as a log line: how many lines the brief numbers and how many pauses worth drawing it found.
#[allow(
    dead_code,
    reason = "the live leg says this from `retakes_ask::briefing_log`, which also knows the seams"
)]
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
/// Numbering runs over the whole session, not per source: an attempt at the end of one file and its retake at the start of the next is the case the pass exists for.
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

/// The seconds actually spoken: the sum of the lines' spans, not wall clock (silence cannot be abandoned).
pub fn speech_seconds(lines: &[Line]) -> f64 {
    lines
        .iter()
        .map(|line| (line.end - line.start).max(0.0))
        .sum()
}

fn lane_of(source: &Source) -> String {
    let name = source.path.rsplit('/').next().unwrap_or(&source.path);
    match name.split_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name.to_string(),
    }
}

/// Returns this pass's log lines, and nothing when the policy's marking pass is not retakes (F1.10 owns those lines).
///
/// `marks` are the pooled runs' marks, already verified against the words by the caller (S4 needs the transcript); `marked_seconds` and
/// `speech_seconds` are measured by the caller so the ceiling test and the log line see the same numbers.
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
