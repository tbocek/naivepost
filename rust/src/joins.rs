//! Repair the joins — spec/04-prepare.md F1.10: the seam where one recording stopped and the next
//! started, and the stumble either side of it that the video has to run over as one piece of speech.
//!
//! Three files hold this flow and each owns one part of it, exactly as F1.9's marking pass is split:
//!
//! * [`crate::tools::textedit`] — the §3.5 TOOL half: `Join`, `drop_words`, `keep_join`,
//!   `get_words`, `one_stretch_at_join`, `seam_snap`, `dedupe_joins`, and the `P.machine.seam*`
//!   constants. It answers a model's call; it knows nothing about sessions or files.
//! * [`crate::word_list`] — F1.13: the session's word list, the spellings a join is shown, and who
//!   a printed piece belongs to (`owns`, `joins_token`). This module reads that list; it does not
//!   build or re-spell one.
//! * [`crate::edges`] — F1.11: where a mark's two edges actually land on the sound. Called here,
//!   never reimplemented.
//! * this module — the FLOW: which seams exist, what each one is asked, whether the answer may be
//!   used, and the two files written together at the end.
//!
//! Why the flow sits apart from the tool: the tool can only ever see ONE join's words, while every
//! question in the flow depends on the others — the window a join is shown must leave out what the
//! joins before it already took out (the 2026-09-18 fix), and `final.txt` cannot be written until
//! all of them have answered. Keeping those rules out of the tool is what lets a test drive fifty-seven
//! joins without a model.
//!
//! What needs a live textedit server is deliberately outside this module: the answers themselves.
//! Everything else — S1's gate, the seam list, the request wording, matching an answer back, the
//! ceilings, the dedupe, the marks and both files — settles with no server, and so is testable.

use std::path::Path;

use crate::edges::{self, AlignedWord};
use crate::hand_edit;
use crate::layout::Tree;
use crate::project::{MarkingPass, Project};
use crate::prompt_assembly;
use crate::prompts;
use crate::seam_retries;
use crate::textfmt::{self, Retake};
use crate::tools::textedit::{
    self, one_stretch_at_join, seam_snap, Side, Join, JOIN_REACH_WORDS, SEAM_CEIL, SEAM_MAX_WORDS,
    SEAM_NOISE_WORDS, SEAM_REACH_WORDS, SEAM_SNAP_WORDS,
};
use crate::word_list::{self, Word};

/// The step name under `cache/llm/<step>/` and in the progress line. Same page F1.7's Describe writes
/// to, because §F1.10 says the join calls land on the run's `…-describe.html`.
pub const STEP: &str = "textedit";

/// Fewer joinable words than this and there is no seam worth asking about: the session is one short
/// take, or less. S1 states the count rather than a ratio for the same reason F1.9 does — with three
/// words any pair is most of the session.
pub const MIN_WORDS: usize = 4;

// ---- S1: the words a join pass may use ------------------------------------

/// S1: the words this pass may work on, out of the whole session list.
///
/// Two exclusions, both load-bearing:
///
/// * the narrator mic. Its words are heard by the app and not played by the video, so a seam into
///   or out of it is not a join in the finished film at all.
/// * the words earlier joins already took out. A window that still shows them shows a repeat that is
///   no longer in the video, and the one-stretch check then counts join *k-1*'s stretch against join
///   *k* and refuses an answer that was right. Measured on the ETH lecture, join 22: leaving them in
///   produced "2 separate stretches left out, not one at the join"; leaving them out leaves exactly
///   the hand cut.
///
/// Words emptied by a fold stay dropped here too — they are gone from the film whatever their seconds
/// say — but keep their place in the caller's list, since a cut may still land on those seconds.
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

/// S1: the sentence printed when there is too little to join. Distinct from the no-seam line below:
/// this is arithmetic answering before anyone was asked. The count is the session's own, so a reader
/// can see how far short of [`MIN_WORDS`] it fell rather than being told a bound with no number.
pub fn too_few_log(words: usize) -> String {
    format!(">>> text edit: nothing to repair -- {words} joinable words, fewer than {MIN_WORDS}")
}

// ---- S2: the seams -------------------------------------------------------

/// S2: the seams of a word list — every place one recording ends and another begins.
///
/// Returned as the index the later recording's first word sits at, ascending. Index rather than
/// seconds because everything downstream cuts between words: the seam IS that boundary, and a
/// second would only have to be looked up again to find it.
///
/// The list must already be S1's: a seam into the narrator mic is not a seam in the film, and one
/// whose earlier half was already taken out is not a seam either.
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

// ---- S3: what one join is asked, and what its answer means ----------------

/// S3: the user message for one join, verbatim as §F1.10 spells it.
///
/// The User Context block is assembled by [`prompt_assembly::user_message`], which already omits it
/// whole when empty — the header never sits over nothing. What this function owns is the material: the
/// join's number, the two sides labelled as the spec labels them, and the shape of the answer.
///
/// BEFORE is the tail of the interrupted take and AFTER the head of the one that follows, capped at
/// [`SEAM_REACH_WORDS`] each. The cap is not decoration: the ceilings in S3's checks count against
/// what the model was SHOWN, so a request built from more words than it can count would refuse an
/// answer for being large when the model was never given the small version.
///
/// Thinking is ON for this job (`P.machine.textEditThinking`, [`prompts::thinking`]) — measured at
/// 15/29 joins right without it against 20/29 with.
pub fn request(context: &str, k: usize, n: usize, before: &[String], after: &[String]) -> String {
    let material = format!(
        "JOIN {k} of {n}.\n\nBEFORE (the end of the take that was interrupted):\n{}\n\n\
         AFTER (the beginning of the take that follows):\n{}\n\n\
         Answer {{\"joined\":\"...\"}} and nothing else: these {b} words and then these {a}, in \
         that order, with the stretch at the join left out.",
        before.join(" "),
        after.join(" "),
        b = before.len(),
        a = after.len(),
    );
    prompt_assembly::user_message(context, "textedit", &material)
}

/// Whether this job thinks. Kept as a named question rather than assumed at the call site: §F1.10's
/// diagram says "thinking ON" per seam, and if `prompts::THINKING_ON` ever drops `textedit` the
/// flow should notice rather than quietly get worse.
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

/// S3: read `{"joined":"..."}` and say what it leaves out.
///
/// The tool's own surface asks for counts (`drop_words(side, count)`); a model answers with text.
/// This is the door between them: the answer is matched backwards against the words the join was
/// shown, preferring the LATER saying of anything repeated, and the counts that fall out are handed
/// to the tool so the tool's ceilings and error sentences are the ones that decide.
///
/// Backwards, and preferring the later saying, because the shared middle of a restart is spoken twice
/// and the answer keeps one copy: matching forwards would attribute the kept copy to BEFORE and charge
/// the removal to AFTER, which is backwards from where the stumble actually is.
///
/// The reply is checked word by word against what was shown, punctuation stripped: the prompt allows
/// no other words, and one invented word anywhere means the answer describes a video this session does
/// not contain. That is a refusal naming the word, not a best-effort fit.
pub fn answer_join(join: &mut Join, reply: &str, shown_before: &[String], shown_after: &[String]) -> Outcome {
    let joined = match parse_joined(reply) {
        Some(text) => text,
        None => return Outcome::Refused("no \"joined\" in the answer".to_string()),
    };
    let words: Vec<String> = joined.split_whitespace().map(|w| w.to_string()).collect();
    if words.is_empty() {
        // An empty string is not "nothing left out" — it is no answer at all. `keep_join` is how a
        // model says the takes need no repair, and it has to say that rather than leave it blank.
        return Outcome::Refused("no words in the answer".to_string());
    }

    // Which words the answer kept, and therefore which it left out. The split between BEFORE's part
    // and AFTER's part is found rather than assumed: a repaired join is ONE run of speech with no
    // marker inside it saying where one take's words end and the next begin.
    let parts = match split_answer(&words, shown_before, shown_after) {
        Some(parts) => parts,
        None => {
            return Outcome::Refused(
                "the answer keeps neither the end of BEFORE nor the start of AFTER".to_string(),
            )
        }
    };
    // What went is measured against what was SHOWN on each side, so a respelled or folded word that
    // matched nothing in the answer still counts as left out rather than silently vanishing.
    let drop_before = shown_before.len() - parts.head.len();
    let drop_after = shown_after.len() - parts.tail.len();
    if drop_before == 0 && drop_after == 0 {
        // Everything shown is in the answer: the model kept both takes whole. That is `keep_join`,
        // said in the only other way it could be said.
        join.keep_join();
        return Outcome::Kept;
    }

    // One stretch at the join: words off the end of BEFORE, or off the start of AFTER, or some of
    // each — but never a run that stops short of the seam on one side while another run sits past it.
    if !(drop_before == 0 || drop_after == 0) {
        // Both sides gave way. Allowed (the spec says "some of each"), and still one stretch because
        // the two halves touch the seam from either side of it. Checked through the tool so the same
        // rule that guards a model's own calls guards this one.
        let runs_before = vec![drop_before];
        let runs_after = vec![drop_after];
        if !one_stretch_at_join(&runs_before, &runs_after) {
            return Outcome::Refused("two stretches left out, not one at the join".to_string());
        }
    }

    // The removal has to REACH the seam on the side it came from: dropping words out of the middle of
    // BEFORE while its last words survive is not a join repair, it is an edit somewhere else in the
    // video. Measured against the unmatched span on each side rather than a word count, because what
    // matters is where the gap sits, not how big it is.
    if drop_before > 0 {
        let (_first, last) = unmatched_span(&parts.head, shown_before);
        if last < shown_before.len() {
            // The kept run resumes before the end of BEFORE: the removal stops short of the seam by
            // however many words sit after it. P.machine.seamSnapWords decides whether that snaps.
            let gap = shown_before.len() - last;
            if let Err(reason) = seam_snap(Side::Before, gap) {
                return Outcome::Refused(reason);
            }
        }
    }
    if drop_after > 0 {
        let (first, _last) = unmatched_span(&parts.tail, shown_after);
        if first > 0 {
            // The kept run starts past the seam: the removal is that many words late.
            if let Err(reason) = seam_snap(Side::After, first) {
                return Outcome::Refused(reason);
            }
        }
    }

    // Hand the counts to the tool and let its ceilings speak. A refusal here arrives as an error JSON
    // carrying the tool's own sentence, which is the sentence the log wants.
    let mut refused: Option<String> = None;
    if drop_before > 0 {
        let verdict = join.drop_words(Side::Before, drop_before);
        if let Some(reason) = tool_error(&verdict) {
            refused = Some(reason);
        }
    }
    if refused.is_none() && drop_after > 0 {
        let verdict = join.drop_words(Side::After, drop_after);
        if let Some(reason) = tool_error(&verdict) {
            refused = Some(reason);
        }
    }
    match refused {
        Some(reason) => Outcome::Refused(reason),
        None => Outcome::Applied {
            before: join.dropped_count(Side::Before),
            after: join.dropped_count(Side::After),
        },
    }
}

/// Read `{"joined":"..."}` out of a reply. Tolerant of surrounding whitespace and of a fence a model
/// added despite being told not to; intolerant of anything that is not that one field, because a
/// report about the join is not an answer about it.
pub fn parse_joined(reply: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(reply.trim()).ok()?;
    value.get("joined")?.as_str().map(|text| text.trim().to_string())
}

/// Split an answer into the two parts it says were kept — the end of BEFORE and the start of AFTER —
/// rather than assuming it keeps each side as a whole block.
///
/// A model that repairs a join returns ONE run of speech, and where that run stops matching BEFORE
/// and starts matching AFTER is not marked. Matching the whole answer against each side separately
/// gets this wrong in the ordinary case: join 23 keeps BEFORE up to "ermöglichen" and then all of
/// AFTER, which shares no contiguous head with either list, so both sides looked dropped and the
/// answer was refused as "two stretches".
///
/// So every split point is tried and the LONGEST total removal wins, because the stumble is what the
/// answer left out and leaving more out than it meant would cut words the speaker kept. Ties go to
/// the split that takes less from AFTER, since the spec has BEFORE give way first.
/// The two runs an answer kept: the tail of BEFORE and the head of AFTER.
#[derive(Debug, Clone)]
pub struct Parts {
    pub head: Vec<String>,
    pub tail: Vec<String>,
}

/// Split the answer into the two parts it kept, scoring every cut by how much it leaves out.
///
/// The cut is FOUND rather than assumed because nothing in a repaired join marks the seam. For each
/// way of cutting the answer, both halves must be subsequences of their own side (an answer may drop a
/// word from what it keeps -- a folded or respelled one -- without that being a second removal), and
/// the reading with the FEWEST words left out wins. Ties go to whichever keeps more of BEFORE, since
/// the spec has BEFORE give way first: what was said last is what the speaker meant to keep.
fn split_answer(
    answer: &[String],
    shown_before: &[String],
    shown_after: &[String],
) -> Option<Parts> {
    let mut best: Option<(usize, Parts)> = None;
    for split in 0..=answer.len() {
        let head = answer[..split].to_vec();
        let tail = answer[split..].to_vec();
        if !is_subsequence(&head, shown_before) || !is_subsequence(&tail, shown_after) {
            continue;
        }
        let removed =
            (shown_before.len() - head.len()) + (shown_after.len() - tail.len());
        match best {
            Some((best_removed, ref best_parts))
                if best_removed < removed
                    || (best_removed == removed && best_parts.head.len() >= head.len()) => {}
            _ => best = Some((removed, Parts { head, tail })),
        }
    }
    best.map(|(_, parts)| parts)
}

/// Does every word of `needles` appear in `hay`, in order? Each matched as spelled-equal, and each
/// later needle only after the position the previous one matched at. An empty `needles` matches.
fn is_subsequence(needles: &[String], hay: &[String]) -> bool {
    let mut cursor = 0usize;
    for word in needles {
        match (cursor..hay.len()).find(|&i| same_spelling(&hay[i], word)) {
            // `find` yields the index itself, not an offset into the range -- advance TO it plus one.
            Some(index) => cursor = index + 1,
            None => return false,
        }
    }
    true
}

/// Where a kept run leaves its gap on this side: the index of the FIRST unmatched word, and the index
/// just past the LAST unmatched one. Used to say which way a stretch missed the seam.
fn unmatched_span(kept: &[String], shown: &[String]) -> (usize, usize) {
    let mut cursor = 0usize;
    let mut first = shown.len();
    let mut last = 0usize;
    for word in kept {
        match (cursor..shown.len()).position(|i| same_spelling(&shown[i], word)) {
            Some(offset) => {
                if offset > 0 {
                    if first > cursor {
                        first = cursor;
                    }
                    last = cursor + offset;
                }
                cursor += offset + 1;
            }
            None => break,
        }
    }
    if cursor < shown.len() {
        if first > cursor {
            first = cursor;
        }
        last = shown.len();
    }
    (first.min(shown.len()), last)
}

/// Two printed words spelling the same thing. Punctuation and case are the transcript's, not the
/// word's: a comma the model did not bother to repeat is no obstacle, and neither is a full stop it
/// added. Compared bare, the same way every other pass in the session compares words.
fn same_spelling(a: &str, b: &str) -> bool {
    word_list::bare(a) == word_list::bare(b)
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

/// S3: the cache slot for one ask about one join. The retry has to key elsewhere than the answer it
/// is retrying, or the refusal comes back from the cache and the retry buys nothing — the whole
/// reason `P.machine.seamRetries` needed a parameter round in the first place.
pub fn ask_slot(base: &str, attempt: u32) -> String {
    seam_retries::ask_slot(base, attempt)
}

// ---- S4: the doubled saying across a cut ---------------------------------

/// S4: at every join still standing, a saying repeated straight across the cut loses its EARLIER
/// copy, up to [`JOIN_REACH_WORDS`] words either side.
///
/// Delegated to [`textedit::dedupe_joins`] rather than rewritten: the reach, the longest-match-first
/// rule and the note's wording are that function's ground. This wrapper exists so the flow names the
/// step and so the notes land in the flow's log rather than vanishing.
pub fn dedupe_across(
    words: &[String],
    times: &[(f64, f64)],
    kept: &mut [bool],
) -> Vec<String> {
    textedit::dedupe_joins(words, times, kept)
}

// ---- S5: marks, edges, merged -------------------------------------------

/// S5: the dropped runs as marks, their edges placed by the sound.
///
/// The runs come out of `hand_edit::marks_from` (a dropped run is a dropped run whatever made it),
/// and every edge then goes through [`edges::place_edges`] — F1.11's rule, which fences each edge
/// with the words on either side and lets the envelope choose inside the fence. This module places no
/// edge itself; a join's word times are a fence, never a cut point.
///
/// `edge_of` answers with the envelope of the recording a session second belongs to, `None` where
/// there is none: a run with no waveform to ask leaves its marks on the word times, which is what
/// `press_joins` does when nothing has been audio-extracted yet.
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
///
/// Flagged, not refused, on purpose: a take that was nothing but a false start SHOULD go whole, and
/// a join that misread its window looks identical at this point. Refusing would throw away the good
/// case to guard the bad one, so the bad one is made visible instead — yellow on the Cut page, with
/// the log naming the recording and its seconds.
///
/// `dropped` indexes the same list as `words`; `recording_of` is the caller's map from a word index
/// to the recording it came off, which the flow holds and this rule does not care how it is built.
pub fn whole_takes(
    words: &[Word],
    dropped: &[bool],
) -> Vec<(String, f64, f64)> {
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
        if !mine.iter().all(|index| dropped.get(*index).copied().unwrap_or(false)) {
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

// ---- S6: the two files, written together --------------------------------

/// S6: write `final.txt` and `retakes.tsv` in ONE step.
///
/// Together, not one then the other, because they are two views of the same decision: the text is
/// what survives and the marks are where it was taken out. Written apart, a crash between them
/// leaves a text whose marks do not exist, and the next Cut rebuilds a different video from the one
/// the person approved — `final.txt` newer ⇒ F1.12 remakes the marks from it, and the remade marks
/// are not the ones the joins chose.
///
/// The text is built by grouping the survivors into per-recording runs and joining consecutive runs
/// with [`textfmt::cut_marker`], because a marker belongs ONLY at a change of recording (spec step
/// 4) while `write_final` places one at every transition it is given an entry for. The marker's own
/// S6: write `final.txt` and `retakes.tsv` in ONE step.
///
/// Together, not one then the other, because they are two views of the same decision: the text is
/// what survives and the marks are where it was taken out. Written apart, a crash between them
/// leaves a text whose marks do not exist, and the next Cut rebuilds a different video from the one
/// the person approved — `final.txt` newer means F1.12 remakes the marks from it, and the remade
/// marks are not the ones the joins chose.
///
/// The text is built by grouping the survivors into per-recording runs and joining consecutive runs
/// with [`textfmt::cut_marker`], because a marker belongs ONLY at a change of recording (spec step
/// 4) while `write_final` places one at every transition it is given an entry for. The marker's own
/// spelling stays single-sourced in `textfmt`; nothing here writes a pipe by hand.
///
/// `whole_bases` names the recordings this pass took out entire; the join on the far side of such a
/// recording carries the longer `whole take` marker so Cut can tint it yellow.
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
    hand_edit::write_final(tree, &text)?;
    hand_edit::write_marks(tree, marks)
}

/// Which recording begins at the `step`-th join. Walks the source list to the `step`-th change and
/// returns the source on the far side of it; empty when there is no such change.
fn joined_recording(sources: &[String], kept: &[bool], step: usize) -> String {
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
///
/// A marker sits where a recording changed. Between two words of the same recording there is nothing
/// to mark: the video plays straight through, and a `|cut|` there would tell the reader a splice
/// exists that the picture never shows. So the count accumulates dropped words and is spent only at
/// a change of source — which makes the result's length the number of joins, not the number of words.
///
/// The last recording's trailing drops are NOT counted here: they belong to no join, and dropping the
/// tail of the final take is the retake pass' ground (F1.9), not this one's.
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

// ---- the entry points ---------------------------------------------------

/// The one call the ▶ handler makes for the joins pass.
///
/// Silent unless the policy names joins, so one press never speaks for both F1.9 and F1.10. With no
/// model server it settles everything that needs no answer: whether there is anything to repair at
/// all, whether there is a seam, and the two empty files the pass owes Cut so the next step can
/// tell "ran, found nothing" from "never ran".
///
/// The requests themselves arrive with the runner rounds, exactly as F1.7's plan lines do: what
/// lands on a press is the flow's own arithmetic, printed.
pub fn press_joins(tree: &Tree, project: &Project, pass: MarkingPass) -> Vec<String> {
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
            return vec![format!("!!! text edit: could not write the text files -- {error}")];
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
            return vec![format!("!!! text edit: could not write the text files -- {error}")];
        }
        return vec![no_seam_log()];
    }
    // Seams exist; the answers come from the textedit model, asked here over the wire. The brief
    // line goes first so the person sees the pass start, then each seam's own line: applied, kept,
    // or refused with its reason.
    let pass = crate::join_ask::run_seams(tree, project, &words, &times);
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
///
/// `session_words` reads each lane's raw `words.json` and so shows a join the words as heard,
/// lower case and unpunctuated. §F1.10 wants each word "printed as the fix pass spelled it", and
/// the word list F1.13 writes is exactly that (with its own raw fallback where the fix pass left a
/// word no spelling of its own). Two reasons to prefer it beyond the wording: it is the list the
/// retakes pass and `final.txt` are built from, so a join that worked on a different list would
/// drop indices that mean something else to the next pass; and it already runs the aligner-over-ASR
/// preference, so a session that was aligned gets the finer times (§F1.13).
///
/// The list is filtered the way S1 asks: the narrator mic out, and, on the live path, the words an
/// earlier pass already dropped stay out because they are not in the surviving list at all.
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
        .find(|source| source.narrator != 0)
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
///
/// Returns the words paired with their times rather than a ready-made [`Word`] list because the list
/// F1.13 builds needs the fixed transcripts and the envelopes to dress its spellings, and a session
/// mid-Prepare has neither. The `written` form falls back to the word as heard — lower case, no
/// punctuation added — which is exactly what §F1.10 says happens where the fix pass left a word no
/// spelling of its own.
fn session_words(tree: &Tree, project: &Project) -> (Vec<Word>, Vec<(f64, f64)>) {
    let hz = crate::transcribe::SAMPLE_RATE as f64;
    let mut words: Vec<Word> = Vec::new();
    let mut times: Vec<(f64, f64)> = Vec::new();
    for source in &project.sources {
        if source.narrator != 0 {
            // The narrator mic is heard and not played: no seam into it is a seam in the film.
            continue;
        }
        let lane = lane_of(source);
        let Ok(Some(doc)) = crate::requests::read_words(tree, &lane) else {
            continue;
        };
        for word in doc.words {
            if word.word.trim().is_empty() {
                continue;
            }
            let start = word.start_sample as f64 / hz;
            let end = word.end_sample as f64 / hz;
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

/// The lane a source's files live under: its file name minus the extension. Same reduction the
/// retake pass uses, so both read the same `words.json`.
fn lane_of(source: &crate::project::Source) -> String {
    let name = source.path.rsplit('/').next().unwrap_or(&source.path);
    match name.split_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name.to_string(),
    }
}

/// Read the marks this pass wrote, for a caller that wants to show or re-check them.
pub fn read_marks(path: &Path) -> Result<Vec<Retake>, String> {
    textfmt::read_retakes(path)
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

/// The noise bound: a stretch of this many words somewhere away from the join is a respelling of the
/// seam rather than a second copy, and does not disqualify the join. `P.machine.seamNoiseWords`.
pub fn noise_words() -> usize {
    SEAM_NOISE_WORDS
}
