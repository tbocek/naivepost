//! Repair the joins — spec/04-prepare.md F1.10: the seam where one recording stopped and the next
//! started, and the stumble either side of it that the video has to run over as one piece of speech.

use crate::edges::{self, AlignedWord};
use crate::fix_session::lane_of;
use crate::hand_edit;
use crate::layout::Tree;
use crate::project::{MarkingPass, Project};
use crate::prompt_assembly;
use crate::prompts;
use crate::seam_retries;
use crate::textfmt::{self, Retake};
use crate::tools::textedit::{
    Join, Side, JOIN_REACH_WORDS, SEAM_CEIL, SEAM_MAX_WORDS, SEAM_REACH_WORDS, SEAM_SNAP_WORDS,
};
use crate::word_list::{self, Word};

/// The step name under `cache/llm/<step>/` and in the progress line. Same page F1.7's Describe writes
/// to, because §F1.10 says the join calls land on the run's `…-describe.html`.
pub const STEP: &str = "textedit";

/// Fewer joinable words than this and there is no seam worth asking about: the session is one short
/// take, or less. S1 states the count rather than a ratio for the same reason F1.9 does — with three
/// words any pair is most of the session.
pub const MIN_WORDS: usize = 4;

/// S1: the words this pass may work on, out of the whole session list.
pub fn joinable_words(words: &[Word], narrator: Option<&str>, dropped: &[bool]) -> Vec<Word> {
    words
        .iter()
        .enumerate()
        .filter(|(_, word)| Some(word.source.as_str()) != narrator)
        .filter(|(index, _)| dropped.get(*index).map(|gone| !*gone).unwrap_or(true))
        .map(|(_, word)| word.clone())
        .collect()
}

/// S1: fewer joinable words than [`MIN_WORDS`] means nothing to repair.
pub fn too_few(words: &[Word]) -> bool {
    words.len() < MIN_WORDS
}

pub fn too_few_log(words: usize) -> String {
    format!(">>> text edit: nothing to repair -- {words} joinable words, fewer than {MIN_WORDS}")
}

/// S2: the seams of a word list — every place one recording ends and another begins.
pub fn seams(words: &[Word]) -> Vec<usize> {
    let mut out = Vec::new();
    for index in 1..words.len() {
        if words[index].source != words[index - 1].source {
            out.push(index);
        }
    }
    out
}

/// S2: one recording, so there is no seam and every word stands. The pass says so rather than
/// printing nothing, because silence here reads as "not run" to whoever is watching the log.
pub fn no_seam_log() -> String {
    ">>> text edit: one recording, no seam to repair -- every word stands".to_string()
}

/// The words either side of [`CUT`] the close-up shows.
pub const CLOSE_WORDS: usize = 15;

/// How many of BEFORE's last words the ask prints as BEFORE: the wide view carries the rest. Measured this way
/// on 07-news (2026-10-26); a quote is still matched against the whole window.
pub const BEFORE_WORDS: usize = 25;

/// Where the join is, in every view the ask shows.
pub const CUT: &str = "[CUT]";

/// Where another join is, in the wide view: marked so the model does not take it for its own.
pub const OTHER_TAKE: &str = "[another take starts]";

/// S3: the user message for one join, verbatim as §F1.10 spells it. `refused` is why the previous answer was refused.
pub fn request(
    context: &str,
    k: usize,
    n: usize,
    wide: &str,
    before: &[String],
    after: &[String],
    refused: Option<&str>,
) -> String {
    // Only words that were said are shown and counted: an entry the fixed transcript left empty showed as blank space
    // and took a place in BEFORE's 25 (09-admin join 4: five empty entries at the seam pushed "Über nächstes Mal" out of
    // BEFORE, and the reworded restart was not seen whole). The quote is still matched against the whole window.
    let said = |words: &[String]| -> Vec<String> {
        words
            .iter()
            .filter(|word| !word_list::bare(word).is_empty())
            .cloned()
            .collect()
    };
    let (before, after) = (said(before), said(after));
    let close = format!(
        "{} {CUT} {}",
        before[before.len().saturating_sub(CLOSE_WORDS)..].join(" "),
        after[..after.len().min(CLOSE_WORDS)].join(" ")
    );
    let mut material = format!(
        "JOIN {k} of {n}.\n\nWIDE VIEW (this join is at {CUT}; every {OTHER_TAKE} is another join, not yours):\n{wide}\n\n\
         CLOSE TO THE CUT:\n{close}\n\n\
         BEFORE (the end of the take that was interrupted):\n...{}\n\n\
         AFTER (the whole take that follows, up to where the next one starts):\n{}\n\n\
         Answer {{\"drop_before\":\"...\",\"drop_after\":\"...\"}} and nothing else: the last words of BEFORE and the \
         first words of AFTER that go, copied exactly, or empty.",
        before[before.len().saturating_sub(BEFORE_WORDS)..].join(" "),
        after.join(" "),
    );
    if let Some(reason) = refused {
        material.push_str(&format!(
            "\n\nYour previous answer at this join was refused: {reason}. Answer again."
        ));
    }
    prompt_assembly::user_message(context, "textedit", &material)
}

/// The wide view of one join: up to `reach` standing words either side of `seam`, with [`CUT`] at it and
/// [`OTHER_TAKE`] at every other seam inside the view.
pub fn wide_view(
    words: &[Word],
    kept: &[bool],
    seam: usize,
    seams: &[usize],
    reach: usize,
) -> String {
    let standing = |index: usize| kept.get(index).copied().unwrap_or(true);
    let cut = seam.min(words.len());
    let before: Vec<usize> = (0..cut).filter(|i| standing(*i)).collect();
    let after: Vec<usize> = (cut..words.len()).filter(|i| standing(*i)).collect();
    let mut shown: Vec<&str> = Vec::new();
    for &index in &before[before.len().saturating_sub(reach)..] {
        if seams.contains(&index) {
            shown.push(OTHER_TAKE);
        }
        shown.push(&words[index].written);
    }
    shown.push(CUT);
    for (at, &index) in after.iter().take(reach).enumerate() {
        if at > 0 && seams.contains(&index) {
            shown.push(OTHER_TAKE);
        }
        shown.push(&words[index].written);
    }
    shown.retain(|word| !word.is_empty());
    shown.join(" ")
}

/// Whether this job thinks. Kept as a named question rather than assumed at the call site: §F1.10's
/// diagram says "thinking ON" per seam, and if `prompts::THINKING_ON` ever drops `textedit` the
/// flow should notice rather than quietly get worse.
/// The join ask's temperature: lower than the 1.0 every other thinking job runs at (§09 §1). A join has one right
/// answer and needs no surprising connection, and at 1.0 the same join came back differently from one run to the next
/// (09-account, 2026-11-05: 2 of 3 runs right on a join the prompt answers right). `P.machine.joinTemperature`.
pub const JOIN_TEMPERATURE: f64 = 0.6;

pub fn thinking_on() -> bool {
    prompts::thinking("textedit")
}

/// What one join's answer came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The join is repaired: the words left out, and how many from each side.
    Applied { before: usize, after: usize },
    /// `keep_join` — the two takes belong together as they stand. A whole answer, not a refusal:
    /// a speaker who stopped to change a slide did not stumble.
    Kept,
    /// The answer breaks a rule. Nothing is removed at this join and the reason says which rule.
    Refused(String),
}

impl Outcome {
    /// How many words this outcome takes out. Zero for a kept join and for a refusal.
    pub fn removed(&self) -> usize {
        match self {
            Outcome::Applied { before, after } => before + after,
            _ => 0,
        }
    }

    /// Whether the answer may be cached. Only a usable one may: caching a refusal turns a join that
    /// could be answered tomorrow into a permanent nothing, and the retry would be served straight
    /// back out of the cache it was supposed to escape.
    pub fn usable(&self) -> bool {
        !matches!(self, Outcome::Refused(_))
    }
}

/// S3: read the words the answer removes and say what that comes to (§02 §3.5).
pub fn answer_join(
    join: &mut Join,
    reply: &str,
    shown_before: &[String],
    shown_after: &[String],
) -> Outcome {
    let Some((quote_before, quote_after)) = parse_quotes(reply) else {
        return Outcome::Refused(
            "no \"drop_before\"/\"drop_after\" in the answer -- answer {\"drop_before\":\"…\",\"drop_after\":\"…\"}"
                .to_string(),
        );
    };
    let Some(drop_before) = quoted_tail(&quote_before, shown_before) else {
        return Outcome::Refused(format!(
            "\"{quote_before}\" is not how BEFORE ends -- quote its last words exactly as shown"
        ));
    };
    let Some(drop_after) = quoted_head(&quote_after, shown_after) else {
        return Outcome::Refused(format!(
            "\"{quote_after}\" is not how AFTER starts -- quote its first words exactly as shown"
        ));
    };
    if drop_before == 0 && drop_after == 0 {
        // Nothing removed: the two takes belong together as they stand, a whole answer.
        join.keep_join();
        return Outcome::Kept;
    }
    // Hand the counts to the tool and let its ceilings speak. A refusal arrives as an error JSON carrying the
    // tool's own sentence, which is the sentence the log and the retry want.
    for (side, count) in [(Side::Before, drop_before), (Side::After, drop_after)] {
        if count > 0 {
            if let Some(reason) = tool_error(&join.drop_words(side, count)) {
                return Outcome::Refused(reason);
            }
        }
    }
    Outcome::Applied {
        before: join.dropped_count(Side::Before),
        after: join.dropped_count(Side::After),
    }
}

/// The two quotes of an answer. A fence or a word around the JSON is tolerated (the object is taken from the
/// first `{` to the last `}`); an object with neither field is no answer.
pub fn parse_quotes(reply: &str) -> Option<(String, String)> {
    let from = reply.find('{')?;
    let to = reply.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(reply.get(from..=to)?).ok()?;
    let field = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let (before, after) = (field("drop_before"), field("drop_after"));
    if before.is_none() && after.is_none() {
        return None;
    }
    Some((before.unwrap_or_default(), after.unwrap_or_default()))
}

/// The bare words of some texts, in order: what a quote and a shown run are compared by.
fn bare_words<'a>(texts: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    texts
        .into_iter()
        .flat_map(|text| text.split_whitespace())
        .map(word_list::bare)
        .filter(|word| !word.is_empty())
        .collect()
}

/// How many of BEFORE's last shown words the quote names, or `None` when it is not how BEFORE ends. The fewest
/// that match: a respelling folded onto one word leaves its followers empty, and those belong to the fold, not
/// to the quote's first word.
fn quoted_tail(quote: &str, shown: &[String]) -> Option<usize> {
    let wanted = bare_words(&[quote.to_string()]);
    if wanted.is_empty() {
        return Some(0);
    }
    (1..=shown.len())
        .find(|n| bare_words(&shown[shown.len() - n..]) == wanted)
        // A quote that starts inside an entry holding several words ("that. The" is one entry, timed as one): the
        // entry goes whole, since it cannot be cut in two. 09-news join 13, "The last article is the following
        // here." was refused twice and the sentence stayed in twice; the hand cut took "that. The" out whole.
        .or_else(|| {
            (1..=shown.len()).find(|n| {
                let run = bare_words(&shown[shown.len() - n..]);
                run.len() > wanted.len() && run.ends_with(&wanted)
            })
        })
}

/// How many of AFTER's first shown words the quote names, or `None` when it is not how AFTER starts. The most
/// that match, so a folded word's empty followers go with it.
fn quoted_head(quote: &str, shown: &[String]) -> Option<usize> {
    let wanted = bare_words(&[quote.to_string()]);
    if wanted.is_empty() {
        return Some(0);
    }
    (1..=shown.len())
        .rev()
        .find(|n| bare_words(&shown[..*n]) == wanted)
        // Ending inside an entry of several words: that entry goes whole, as in [`quoted_tail`].
        .or_else(|| {
            (1..=shown.len()).find(|n| {
                let run = bare_words(&shown[..*n]);
                run.len() > wanted.len() && run.starts_with(&wanted)
            })
        })
}

/// The refusal sentence inside a tool reply, if the reply is one.
fn tool_error(reply: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(reply).ok()?;
    value.get("error")?.as_str().map(str::to_string)
}

/// S3: the refusal line, exactly as §F1.10 words it — the join numbered, the reason, and the tail
/// that says whether this join gets asked again.
pub fn refusal_line(k: usize, reason: &str, tail: &str) -> String {
    format!("!!! text edit: join {k}: {reason} -- {tail}")
}

/// The session's lanes for F1.11's edge placement, from the project's stored source paths: the word list's `source`
/// is a bare recording name, never a path (first rewrite: it was stat-ed as one, found nothing, and no join edge was
/// ever placed by the sound).
pub fn project_lanes(tree: &Tree, project: &crate::project::Project) -> Vec<crate::edges::Lane> {
    let stored: Vec<String> = project
        .sources
        .iter()
        .map(|source| source.path.clone())
        .collect();
    crate::hand_edit::session_lanes(tree, &stored)
}

/// S5: the dropped runs as marks, their edges placed by the sound.
pub fn marks_for<'a>(
    times: &[(f64, f64)],
    kept: &[bool],
    words: &[AlignedWord],
    edge_of: impl FnMut(f64) -> Option<&'a edges::Edges>,
    spoken: &[(f64, f64)],
) -> (Vec<Retake>, Vec<String>) {
    let raw = hand_edit::marks_from(times, kept);
    edges::place_edges(raw, words, edge_of, spoken)
}

/// S5: the whole-take flag. When the words one join takes out are EVERY word of a recording, the
/// mark names that recording and the person is told rather than the join being refused.
pub fn whole_takes(words: &[Word], dropped: &[bool]) -> Vec<(String, f64, f64)> {
    let mut out: Vec<(String, f64, f64)> = Vec::new();
    for source in distinct_sources(words) {
        let mine: Vec<usize> = words
            .iter()
            .enumerate()
            .filter(|(_, word)| word.source == source)
            .map(|(index, _)| index)
            .collect();
        if mine.is_empty() {
            continue;
        }
        // Every word of the recording gone — and gone BY THIS PASS, not folded away by an earlier
        // one. A recording whose words were already absent before the joins ran is not something a
        // join took out, and flagging it would blame the wrong step.
        if !mine
            .iter()
            .all(|index| dropped.get(*index).copied().unwrap_or(false))
        {
            continue;
        }
        let start = words[mine[0]].start;
        let end = words[*mine.last().expect("non-empty above")].end;
        out.push((source, start, end));
    }
    out
}

/// The recordings a word list touches, in the order they appear, each named once.
fn distinct_sources(words: &[Word]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in words {
        if !out.contains(&word.source) {
            out.push(word.source.clone());
        }
    }
    out
}

/// S5: the whole-take sentence, as §F1.10 words it.
pub fn whole_take_log(base: &str, start: f64, end: f64) -> String {
    format!(
        ">>> text edit: all of {base} goes ({start}-{end}) -- marked yellow on Cut, worth a look",
        start = crate::retakes::m_ss(start),
        end = crate::retakes::m_ss(end),
    )
}

/// S6: write `final.txt` and `retakes.tsv` in ONE step.
pub fn write_pair(
    tree: &Tree,
    sources: &[String],
    words: &[String],
    kept: &[bool],
    whole_bases: &[String],
    marks: &[Retake],
) -> Result<(), String> {
    // Survivors grouped by the recording they came off, in order. A run is one stretch of the film
    // that plays straight through, so its words need no markers inside them.
    let mut runs: Vec<(String, Vec<String>)> = Vec::new();
    for index in 0..words.len() {
        if !kept.get(index).copied().unwrap_or(true) {
            continue;
        }
        let source = sources.get(index).cloned().unwrap_or_default();
        match runs.last_mut() {
            Some((last, run)) if *last == source => run.push(words[index].clone()),
            _ => runs.push((source, vec![words[index].clone()])),
        }
    }
    // One marker per join, counting what went between the survivors on either side of it.
    let counts = join_counts_for(sources, kept);
    // A space either side of the marker: `read_final` recognises a marker only at the start of a
    // token run, so gluing it onto the word before it ("two|cut") makes the reader see a word and
    // not a splice.
    let mut text = String::new();
    for (step, (_, run)) in runs.iter().enumerate() {
        if step > 0 {
            let gone = counts.get(step - 1).copied().unwrap_or(0);
            let whole = whole_bases
                .iter()
                .any(|base| *base == joined_recording(sources, kept, step));
            text.push(' ');
            text.push_str(&textfmt::cut_marker(gone, whole));
            text.push(' ');
        }
        text.push_str(&textfmt::write_final(run, &[]));
    }
    // Nothing was ever heard: no text and no marks, rather than two empty files (§3). A pass that cut every
    // word it had is an answer, and its marks are written below like any other.
    if words.is_empty() {
        crate::textfmt::write_rows_or_none(&tree.final_txt(), "")?;
        return crate::textfmt::write_rows_or_none(&tree.retakes_tsv(), "");
    }
    hand_edit::write_final(tree, &text)?;
    hand_edit::write_marks(tree, marks)
}

/// Which recording begins at the `step`-th join. Walks the source list to the `step`-th change and
/// returns the source on the far side of it; empty when there is no such change.
fn joined_recording(sources: &[String], _kept: &[bool], step: usize) -> String {
    let mut changes = 0usize;
    for index in 1..sources.len() {
        if sources[index] != sources[index - 1] {
            changes += 1;
            if changes == step {
                return sources[index].clone();
            }
        }
    }
    String::new()
}

/// S6: the `|cut N|` counts, one per transition where the RECORDING changes.
pub fn join_counts_for(sources: &[String], kept: &[bool]) -> Vec<usize> {
    let mut counts = Vec::new();
    let mut pending = 0usize;
    for index in 1..sources.len() {
        if sources[index] != sources[index - 1] {
            // The join between the previous recording and this one: everything dropped since the
            // last marker went AT this seam.
            counts.push(pending);
            pending = 0;
        } else if !kept.get(index).copied().unwrap_or(true) {
            pending += 1;
        }
    }
    counts
}

/// S6: the completion line, exactly as §F1.10 words it.
pub fn written_log(removed: usize, total: usize, stretches: usize) -> String {
    format!(">>> text edit: {removed} of {total} words removed in {stretches} stretch(es)")
}

/// The one call the ▶ handler makes for the joins pass.
pub fn press_joins(
    tree: &Tree,
    project: &Project,
    pass: MarkingPass,
    reached: &mut dyn FnMut(f64, &str),
) -> Vec<String> {
    if pass != MarkingPass::Joins {
        return Vec::new();
    }
    // The words come off F1.13's saved session list where one exists, so a join is shown the
    // spellings and the aligned times every other pass works from; `session_words` is the raw
    // fallback for a session that never built the list.
    let (words, times) = session_words_with_list(tree, project);
    let sources: Vec<String> = words.iter().map(|word| word.source.clone()).collect();
    if too_few(&words) {
        // S1: nothing to repair, and both files still owed.
        if let Err(error) = write_pair(tree, &[], &[], &[], &[], &[]) {
            return vec![format!(
                "!!! text edit: could not write the text files -- {error}"
            )];
        }
        return vec![too_few_log(words.len())];
    }
    let seams = seams(&words);
    if seams.is_empty() {
        // S2: one recording. Still owe the files: the text is the whole session and there are no
        // marks, which is a real answer and not the same as having skipped.
        let written: Vec<String> = words.iter().map(|word| word.written.clone()).collect();
        let kept: Vec<bool> = vec![true; written.len()];
        if let Err(error) = write_pair(tree, &sources, &written, &kept, &[], &[]) {
            return vec![format!(
                "!!! text edit: could not write the text files -- {error}"
            )];
        }
        return vec![no_seam_log()];
    }
    // Seams exist; the answers come from the textedit model, asked here over the wire. The brief
    // line goes first so the person sees the pass start, then each seam's own line: applied, kept,
    // or refused with its reason.
    let pass = crate::join_ask::run_seams(tree, project, &words, &times, reached);
    let mut lines = vec![brief_log(&words, seams.len())];
    let spoke = !pass.logs.is_empty();
    lines.extend(pass.logs);
    if !spoke {
        // Nothing was applied and nothing was refused — every seam had no words left to ask about.
        // S6's own line says that, so the pass does not end on a silence that reads "not run".
        lines.push(written_log(0, words.len(), 0));
    }
    lines
}

/// Reads the session's joinable words out of F1.13's saved list — the SAME list the retakes pass
/// marks against — falling back to [`session_words`] when it has not been built yet.
fn session_words_with_list(tree: &Tree, project: &Project) -> (Vec<Word>, Vec<(f64, f64)>) {
    if let Ok(Some(list)) = crate::word_list::load(tree) {
        let words = list.to_vec();
        // A session whose every word is joinable gets its own list; one with a narrator mic gets it
        // stripped out through the S1 rule rather than by hand.
        let joinable = match narrator_of(project) {
            Some(mic) => joinable_words(&words, Some(mic.as_str()), &[]),
            None => words,
        };
        if !joinable.is_empty() {
            let times = joinable.iter().map(|word| (word.start, word.end)).collect();
            return (joinable, times);
        }
    }
    session_words(tree, project)
}

/// The narrator mic's source name, if this project has one: its words are heard and not played, so
/// no seam into them is a seam in the film (S1). Owned rather than borrowed, so the caller can
/// hand it to [`joinable_words`] without tying a lifetime to the project.
fn narrator_of(project: &Project) -> Option<String> {
    project
        .sources
        .iter()
        .find(|source| crate::sources::is_narrator_mic(source))
        .map(lane_of)
}

/// S2/S3 as a log line: the size of the job, in the units the pass counts in.
fn brief_log(words: &[Word], seam_count: usize) -> String {
    format!(
        ">>> text edit: briefing on {} joinable words, {seam_count} seam(s) of {SEAM_REACH_WORDS} \
         words each side",
        words.len()
    )
}

/// The session's joinable words, read off disk: every word the aligner heard, on the session clock,
/// the narrator mic left out.
fn session_words(tree: &Tree, project: &Project) -> (Vec<Word>, Vec<(f64, f64)>) {
    let hz = crate::transcribe::SAMPLE_RATE as f64;
    let mut words: Vec<Word> = Vec::new();
    let mut times: Vec<(f64, f64)> = Vec::new();
    // Where each recording starts on the session clock: the marks are session seconds, as the word list's are.
    let placed = crate::fix_session::placed_sources(project);
    // On the session clock, as the session word list is: BEFORE has to be the earlier recording.
    for source in crate::fix_session::in_clock_order(project) {
        if crate::sources::is_narrator_mic(source) {
            // The narrator mic is heard and not played: no seam into it is a seam in the film.
            continue;
        }
        let lane = lane_of(source);
        let off = placed
            .iter()
            .find(|placed| placed.base == lane)
            .map_or(0.0, |placed| placed.start);
        let Ok(Some(doc)) = crate::requests::read_words(tree, &lane) else {
            continue;
        };
        for word in doc.words {
            if word.word.trim().is_empty() {
                continue;
            }
            let start = off + word.start_sample as f64 / hz;
            let end = off + word.end_sample as f64 / hz;
            words.push(Word {
                source: lane.clone(),
                match_word: word_list::bare(&word.word),
                written: word.word.clone(),
                start,
                end,
                stray: false,
            });
            times.push((start, end));
        }
    }
    (words, times)
}

/// The numbers this module's rules count with, exposed so a test can assert the parameter ids rather
/// than retyping them: `P.machine.seamReachWords`, `P.machine.seamSnapWords`, `P.machine.seamMaxWords`,
/// `P.machine.seamCeil`, `P.machine.seamRetries`, and the dedupe reach `P.eng.joinReachWords`.
pub fn bounds() -> (usize, usize, usize, f64, u32, usize) {
    (
        SEAM_REACH_WORDS,
        SEAM_SNAP_WORDS,
        SEAM_MAX_WORDS,
        SEAM_CEIL,
        seam_retries::SEAM_RETRIES,
        JOIN_REACH_WORDS,
    )
}
