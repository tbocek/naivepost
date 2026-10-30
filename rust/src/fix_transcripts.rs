//! Fix the transcripts (spec/04-prepare.md F1.8): what happens between a source's ASR rows and the
//! one timeline the cut model reads.
//!
//! Every source is placed on the session clock, its transcript goes out to the fixer in blocks of
//! [`prepare::FIX_BLOCK_LINES`] lines with five seconds of every other recording around them as
//! grounding, the answers are written back per source, and everything is merged into `session.tsv`
//! and `session.txt`. As in [`crate::asr`], [`crate::align`] and [`crate::frames`], no process is
//! spawned and no socket opened: the fixer's reply arrives through [`run`]'s closure and so does the
//! marking pass's, because a step that decided inside a server call could never be tested
//! (spec/00-principles.md §5).
//!
//! The tool surface itself is [`crate::tools::fix`] — this module *calls* it rather than copying it,
//! which is the whole difference from the prototype: a bad row there was a discarded block of 25 lines
//! and a silent re-ask, and here it is one refusal naming the row.

use std::path::PathBuf;

use crate::clock;
use crate::layout::Tree;
use crate::prepare::FIX_BLOCK_LINES;
use crate::project::MarkingPass;
use crate::textfmt::{self, FrameEvent, Line, SessionLine};
use crate::tools::fix::{Block, FIX_CONTEXT_SECONDS};

/// The cache/step key for these replies, so a re-run reads its own answers back.
pub const FIX_STEP: &str = "fix";

/// Tries per block before the ASR original is kept. `P.machine.fixTries` (default 2), the count the
/// prototype wrote as `try < 2`. F1.8 keeps those two — the difference is not the count but that the
/// second try is told what was wrong, because [`Block::fix_line`] refuses by name rather than voiding
/// the block.
///
/// §10's row for this id is deliberately absent from [`crate::params::prepare`]: §04#4 does not name
/// it among the parameters it lists, and `rust/tests/suite/prepare_parameters.rs` pins that list by ORDER,
/// so a row inserted here would break that assertion. The number lives with the rule that reads it, as
/// every `P.*` value must; [`crate::params::family`] still answers `Family::Machine` from the prefix.
pub const BLOCK_TRIES: u32 = 2;

/// How many asks one block gets in all — the first plus the retries. `P.machine.fixTries`
pub fn attempts() -> u32 {
    BLOCK_TRIES
}

/// Whether another ask is still owed after `asked` have been made. This is the prototype's
/// `try < 2` condition lifted out of the loop so the bound can be read and tested on its own.
pub fn more_tries_left(asked: u32) -> bool {
    asked < BLOCK_TRIES
}

/// What a block that ran out of tries says. Naming the refusal is the point: a reader who sees only
/// "failed" looks for a crash, and there was none — the answer would not validate twice, so this
/// block keeps the ASR's own lines and every other block still gets fixed.
pub fn give_up_log(base: &str, index: usize, total: usize) -> String {
    format!(
        "!!! {} block {}/{} kept its ASR text -- the fixer's answer was refused \
         {BLOCK_TRIES} times",
        base,
        index + 1,
        total
    )
}

/// One source as this step sees it: where it sits on the session clock and what kind of recording it
/// is, since a video gets subtitles and a recorder gets `commentary.fixed.tsv`.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The name every file and log line uses.
    pub base: String,
    /// The stamped file name the session clock reads — not [`Source::base`], which is usually its stem.
    pub file_name: String,
    /// Its first second on the session clock, from [`placement`].
    pub start: f64,
    pub duration: f64,
    pub is_video: bool,
    pub is_commentary: bool,
}

impl Source {
    /// The last second of this recording on the session clock.
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
}

/// S1: every source's first second on the session clock.
///
/// [`clock::clock`] makes the earliest stamped name second nought, so this has to be handed the WHOLE
/// session: leaving one file out moves zero and slides every other file to a different second, which
/// is why it takes names rather than a source at a time. An unstamped name is placed at the start, as
/// §8 spells it.
pub fn placement(names: &[&str]) -> Vec<f64> {
    let clock = clock::clock(names);
    (0..names.len()).map(|index| clock.offset(index)).collect()
}

/// S1: `offsets.tsv`'s rows — each video against each audio recording, and how far apart their starts
/// are. The prototype pairs them this way because the offset is what lines a separate microphone up
/// with the camera it was recording beside; an all-audio session has nothing to pair.
pub fn offsets_rows(sources: &[Source]) -> Vec<(String, String, f64)> {
    let mut rows = Vec::new();
    for video in sources.iter().filter(|source| source.is_video) {
        for audio in sources.iter().filter(|source| !source.is_video) {
            rows.push((
                video.base.clone(),
                audio.base.clone(),
                video.start - audio.start,
            ));
        }
    }
    rows
}

/// S1: what one pairing says — which recording starts how far into which, the sentence a reader of
/// `offsets.tsv` needs beside the numbers.
pub fn offsets_log(video: &str, audio: &str, offset: f64) -> String {
    format!(
        ">>> offset: {video} starts {} s into {audio}",
        fix_seconds(offset)
    )
}

/// Seconds as §6 spells them in this log: whole numbers lose the `.0`, so a pairing that lands exactly
/// on a second reads "starts 0 s into" rather than "0.00 s".
fn fix_seconds(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    let mut text = format!("{value:.2}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text.trim_end_matches('.').to_string()
}

/// S1: write `offsets.tsv`. Rows go out with two decimals like every other time in §6.
pub fn write_offsets(tree: &Tree, sources: &[Source]) -> Result<usize, String> {
    let rows = offsets_rows(sources);
    let mut out = String::new();
    for (video, audio, offset) in &rows {
        out.push_str(&format!("{video}\t{audio}\t{offset:.2}\n"));
    }
    textfmt::write_rows_or_none(&tree.offsets_tsv(), &out)?;
    Ok(rows.len())
}

/// S1: a source's own transcript, as its folder holds it. Absent means nothing was heard in it, which
/// is a recording with no speech rather than an error.
pub fn load_transcript(tree: &Tree, source: &str) -> Result<Vec<Line>, String> {
    textfmt::read_lines(&tree.transcript_tsv(source))
}

/// S1: a footage source's event log — what happened on screen, which is context for the words beside
/// it. A source with no frames has none, and that is normal.
pub fn load_events(tree: &Tree, source: &str) -> Vec<FrameEvent> {
    textfmt::read_events(&tree.events_tsv(source)).unwrap_or_default()
}

/// Is this a tool refusal? [`crate::tools::error`] serialises `{"error": …}`, so that is the test —
/// and a refusal is an answer the model can use, not the end of the flow.
pub(crate) fn refused(answer: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .is_some()
}

/// The reason a refusal gave, for putting in front of the model on the next try.
fn refusal_reason(answer: &str) -> String {
    serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| answer.to_string())
}

/// S2: the blocks one transcript goes out in. `P.machine.fixBlockLines` lines each; a short last block
/// stays short, because the model is asked for exactly its own count and padding it with lines from
/// nowhere would be asking for fixes to lines that do not exist.
pub fn blocks(lines: &[Line]) -> Vec<Vec<Line>> {
    lines
        .chunks(FIX_BLOCK_LINES)
        .map(|block| block.to_vec())
        .collect()
}

/// S2: the grounding window around a block — five seconds either side ([`FIX_CONTEXT_SECONDS`]).
pub fn context_window(start: f64, end: f64) -> (f64, f64) {
    (start - FIX_CONTEXT_SECONDS, end + FIX_CONTEXT_SECONDS)
}

/// S2: which blocks a source's rows go out in.
///
/// The prototype walked the transcript once per source with no window at all, so every ask carried only
/// its own 25 lines; here each block is asked *with* the seconds around it, and §1 of this spec item
/// says what those seconds are — five either side ([`FIX_CONTEXT_SECONDS`]), from every other recording
/// and from this source's own events. A block whose first row begins within five seconds of the
/// session's start simply has less on one side: the window is clipped at the timeline, not padded with
/// silence.
pub fn asks_for(blocks: &[Vec<Line>], from: f64, to: f64) -> Vec<(usize, f64, f64)> {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let (first, last) = (block.first()?, block.last()?);
            // The window reaches five seconds past the block at both ends; it is clipped at the
            // session's own edges, since nothing outside the timeline can be grounding.
            let (open, close) = context_window(first.start, last.end);
            Some((index + 1, open.max(from), close.min(to)))
        })
        .collect()
}

/// S2: the grounding itself.
///
/// Every *other* source's lines and events overlapping the window go in, plus this source's own
/// events: its own transcript lines are what is being cleaned, so showing them as context would be
/// showing the model the thing it is meant to judge. An empty string is normal — a solo recording has
/// nothing beside it — and the prompt says so, because a model that reads silence as a problem starts
/// inventing context.
pub fn context_text(
    from: f64,
    to: f64,
    own_base: &str,
    sources: &[Source],
    lines: &[(String, Line)],
    events: &[(String, FrameEvent)],
) -> String {
    let mut out = String::new();
    for (base, line) in lines {
        if base == own_base {
            continue;
        }
        // Overlap, not containment: a sentence that started before the window and runs into it is
        // exactly the grounding a respelling needs.
        if line.end <= from || line.start >= to {
            continue;
        }
        out.push_str(&format!(
            "{} ({base}): {}\n",
            label(base, &line.speaker, ""),
            // One line per row: the block underneath is already a wall of lines and a wrapped row
            // reads as two.
            line.text.split_whitespace().collect::<Vec<_>>().join(" ")
        ));
    }
    for (base, event) in events {
        if event.end <= from || event.start >= to {
            continue;
        }
        // Own events included: a slide changing is why the words suddenly do not make sense. The
        // source list stays in the signature so rows are handed over placed on one clock.
        debug_assert!(sources.iter().any(|source| source.base == *base) || sources.is_empty());
        out.push_str(&format!("EVENT ({base}): {}\n", event.text));
    }
    out
}

/// One line of the ask: numbered as the model will send it back, with its times and speaker. The text
/// keeps its own tabs — §6's rows are split on the first three, so a tab inside a word is part of it.
pub fn ask_text(context: &str, lines: &[Line]) -> String {
    // The prototype's rows: start, end, speaker, text and nothing else. The job says "no line numbers", and the
    // first rewrite put one in front of every row anyway.
    let mut rows = String::new();
    for line in lines {
        rows.push_str(&format!(
            "{start:.2}\t{end:.2}\t{speaker}\t{text}\n",
            start = line.start,
            end = line.end,
            speaker = line.speaker,
            text = line.text.replace('\t', " ")
        ));
    }
    let count = lines.len();
    format!(
        "Context around these lines:\n{context}\nTranscript lines to clean ({count} lines, return \
         exactly {count}):\n{rows}"
    )
}

/// One row of a fixer's answer as it came back, before it is held against the block it answers.
#[derive(Debug, Clone, PartialEq)]
pub struct AnsweredRow {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text: String,
}

/// The answer's rows, read the way the prototype read them: every line with at least four tab-separated fields
/// is a row (start, end, speaker, then the text, whose own tabs are joined back with spaces); anything else (a
/// fence, a blank line, a sentence around the answer) is not a row. Whether the rows ARE the block is
/// [`rows_to_fixes`]'s question.
pub fn answered_rows(answer: &str) -> Vec<AnsweredRow> {
    answer
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 4 {
                return None;
            }
            Some(AnsweredRow {
                start: fields[0].trim().parse().ok()?,
                end: fields[1].trim().parse().ok()?,
                speaker: fields[2].trim().to_string(),
                text: fields[3..].join(" ").trim().to_string(),
            })
        })
        .collect()
}

/// The prototype's strict check of an answer against its block: exactly as many rows, each with the block's own
/// start, end and speaker, and none emptied. A row that passes is a fix of that line; anything else refuses the
/// whole answer with the reason the model is shown on its next try.
pub fn rows_to_fixes(lines: &[Line], rows: &[AnsweredRow]) -> Result<Vec<Reply>, String> {
    if rows.len() != lines.len() {
        return Err(format!(
            "{} line(s) came back for the {} asked -- answer exactly {} lines, one per line asked",
            rows.len(),
            lines.len(),
            lines.len()
        ));
    }
    let mut fixes = Vec::with_capacity(rows.len());
    for (index, (row, line)) in rows.iter().zip(lines).enumerate() {
        let same = |a: f64, b: f64| (a - b).abs() < 0.006;
        if !same(row.start, line.start) || !same(row.end, line.end) || row.speaker != line.speaker {
            return Err(format!(
                "line {} must keep its start, end and speaker exactly as given ({:.2}\t{:.2}\t{})",
                index + 1,
                line.start,
                line.end,
                line.speaker
            ));
        }
        if row.text.is_empty() {
            return Err(format!(
                "line {} came back empty -- a line you cannot make sense of keeps its original text",
                index + 1
            ));
        }
        fixes.push(Reply::Fix {
            n: index as u32 + 1,
            text: row.text.clone(),
        });
    }
    Ok(fixes)
}

/// S2: the checkpoint before a block goes out. Recorded rather than drawn — F0.5 owns the bar.
pub fn checkpoint(block: usize, blocks: usize, source: &str) -> String {
    format!(">>> fixing {source}: block {block}/{blocks}")
}

/// What one block came back as.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockOutcome {
    /// The block with the fixes applied: every time and speaker is the one that was there.
    pub lines: Vec<Line>,
    pub fixed: usize,
    /// What `flag_line` recorded — a wrong speaker, or a row nobody can make out. Not this step's call:
    /// the fixer raises these through its own tool, and they are what S6 is handed.
    pub flags: Vec<(u32, String)>,
    /// False only when the tries ran out, in which case the caller keeps the ASR original.
    pub valid: bool,
    /// Which try answered — 1 for the first ask, 2 once a refusal has been shown back to the model.
    pub attempt: u32,
}

/// A row the fixer could not read. `flag_line`'s reasons name what it saw, so this is a type and not an
/// enum: the reason is for a person, and §3 says a flag never decides anything on its own.
pub const GARBAGE: &str = "cannot be made out";

/// S3: one block through the fixer's tools.
///
/// `ask` is handed [`ask_text`]'s message — with the previous refusal appended when there was one —
/// and answers with the [`Reply`] calls the model made: `fix_line`, `flag_line`, a `get_lines` read-back,
/// `finish`. Every one goes through [`apply_reply`], so a number outside the block or a text carrying a
/// tab comes back as an error naming that line and is put in front of the model on the next try; the
/// other calls of the reply still land, which is what the prototype's all-or-nothing discard could not
/// do. A block whose tries run out is `valid: false`, and its original stands.
pub fn fix_block<F>(lines: &[Line], context: &str, mut ask: F) -> Result<BlockOutcome, String>
where
    F: FnMut(&str) -> Result<Vec<Reply>, String>,
{
    // The model is shown the message built from these rows and the grounding around them (`context_text`), so a
    // refusal, which is about that message, is appended to it here. The first rewrite built it with the context
    // and threw it away, and asked with "Context around these lines:" empty.
    let message = ask_text(context, lines);
    let mut block = Block::new(
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| crate::tools::fix::Line {
                n: index as u32 + 1,
                start: line.start,
                end: line.end,
                speaker: line.speaker.clone(),
                text: line.text.clone(),
            })
            .collect(),
    );
    let mut refusals: Vec<String> = Vec::new();

    for try_number in 1..=BLOCK_TRIES {
        let ask_with = match refusals.last() {
            // The model is told what it got wrong. Re-asking the identical question, as the prototype
            // did, only earns the same answer twice.
            Some(err) => format!("{message}\nThe previous answer was refused: {err}"),
            None => message.clone(),
        };
        let mut reply = ask(&ask_with)?;
        // A wire answer is the block's rows as the model wrote them; held against the block here, where the
        // block is known. A count, a time or a speaker that moved refuses the answer, as the prototype did.
        let rows: Vec<AnsweredRow> = reply
            .iter()
            .filter_map(|call| match call {
                Reply::Row(row) => Some(row.clone()),
                _ => None,
            })
            .collect();
        if let Some(Reply::Unreadable(why)) = reply
            .iter()
            .find(|call| matches!(call, Reply::Unreadable(_)))
        {
            refusals.push(why.clone());
            continue;
        }
        if !rows.is_empty() {
            match rows_to_fixes(lines, &rows) {
                Ok(fixes) => reply = fixes,
                Err(why) => {
                    refusals.push(why);
                    continue;
                }
            }
        }
        // Every call of the reply goes through the tool surface, and every answer comes back — a
        // `flag_line` note or a `get_lines` read-back is an answer, not a refusal. The first refusal is
        // what ends this try, since that is the one thing the model has to hear about.
        let answers = apply_reply(&mut block, &reply);
        match answers.iter().find(|answer| refused(answer)) {
            Some(answer) => refusals.push(refusal_reason(answer)),
            None => {
                return Ok(BlockOutcome {
                    lines: apply(&block, lines),
                    fixed: block.fixes().len(),
                    flags: block.flags().to_vec(),
                    valid: true,
                    attempt: try_number,
                });
            }
        }
    }
    // Out of tries: the originals stand, and only this block's are kept — which is why a block, and
    // not a source or a session, is the unit of failure here.
    Ok(BlockOutcome {
        lines: lines.to_vec(),
        fixed: 0,
        flags: block.flags().to_vec(),
        valid: false,
        attempt: BLOCK_TRIES,
    })
}

/// The block's lines with the fixes applied, back in `textfmt`'s shape.
fn apply(block: &Block, lines: &[Line]) -> Vec<Line> {
    block
        .apply()
        .iter()
        .zip(lines)
        .map(|(fixed, original)| Line {
            start: original.start,
            end: original.end,
            speaker: original.speaker.clone(),
            text: fixed.text.clone(),
        })
        .collect()
}

/// S4: what a block that ran out of tries says. The originals standing is the outcome worth logging —
/// a reader who sees only "failed" looks for a crash, and there was none: the answer would not validate
/// twice, so this block keeps the ASR's own lines and every other block still gets fixed.
pub fn block_failed_log(base: &str, index: usize, total: usize) -> String {
    format!(
        ">>> [{base}] block {}/{total} failed validation, keeping original lines",
        index + 1
    )
}

/// S3: how many of this source's blocks were read back from `cache/llm/fix` rather than asked again.
/// Worth the line because a re-run that asks everything twice looks, from the outside, like a run that
/// had forgotten it had already answered.
pub fn cached_blocks_log(base: &str, cached: usize) -> String {
    format!(">>> [{base}] {cached} block(s) answered from the cache")
}

/// S3: the cache key for a block — §6's digest over its own rows, times and text included, and over `prompt`,
/// the wording and User Context the block is asked under ([`crate::fix_call::cache_prompt`]). Two runs of the
/// same transcript under the same prompt ask the identical question, so they read the same answer; one
/// changed line, or a changed prompt or context, asks a different one. The first rewrite keyed on the rows
/// alone, so a rule added to the profile's `## fix` section never reached a block already answered.
///
/// The part list is `llm_cache`'s, so this file names its entries the same sha256 hex every other
/// caching job does rather than a hash of its own. The `user` part is the block spelled exactly as
/// [`ask_text`] spells it when there is no surrounding context — the rows are what decide the answer,
/// and the grounding window only repeats them with their neighbours.
pub fn block_key(prompt: &str, lines: &[Line]) -> String {
    let request = crate::llm_cache::Request {
        system: prompt,
        user: &ask_text("", lines),
        state: "",
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: None,
        // The fixer's ask goes to whatever model Settings names, but the caller here has no settings in
        // hand: the block's rows decide the respelling, and the model that spelled it is unknown at this
        // height. Spelled empty rather than guessed at, so it cannot collide with a real model id.
        model: "",
        thinking: false,
    };
    crate::llm_cache::key(&crate::llm_cache::parts(&request)).expect("a block of rows always keys")
}

/// S3: a block's stored reply, if it has one. Only valid blocks are written here, so a re-run re-asks
/// a block that failed rather than replaying its failure.
pub fn cached_block(tree: &Tree, prompt: &str, lines: &[Line]) -> Option<Vec<Reply>> {
    let text = crate::llm_cache::read_at(tree, FIX_STEP, &block_key(prompt, lines))?;
    // §09 §10: the block's stored reply writes its own `cache` line, with no time on the wire, so the
    // file shows what the re-run saved. The model column stays empty for the same reason `block_key`
    // leaves it empty: at this height nothing knows which model spelled the answer.
    crate::request_timing::record_cached(
        tree,
        crate::server_leg::base_row(
            crate::services::Server::Llm,
            "",
            "POST",
            "",
            FIX_STEP,
            FIX_STEP,
        ),
    );
    let mut out = Vec::new();
    // The rows kept are the ones whose text differs from what the ASR heard — exactly the set of
    // `fix_line` calls to replay.
    for (index, (stored, original)) in text.lines().zip(lines).enumerate() {
        if stored != original.text {
            out.push(Reply::Fix {
                n: index as u32 + 1,
                text: stored.to_string(),
            });
        }
    }
    // An answer that changed nothing is still an answer: a re-run of a clean transcript reads it back
    // rather than asking about the same rows again.
    Some(out)
}

/// S3: store a block's outcome, one line per row in the block's own order, under the block's input: the
/// fixed lines whichever try earned them, or the ASR lines when every try was refused. The same input reads it
/// back and asks nothing, and it is validated again on every replay as a live answer is ([`cached_block`] goes
/// through [`fix_block`]). The first rewrite filed only a first-try answer (§6's old "second irregularity"),
/// so a block answered right only on its retry, or refused on every try, was asked again on every run although
/// nothing about it had changed (08-dex 18-41-59: a text-only first answer, then one speaker label miscopied).
pub fn store_block(tree: &Tree, prompt: &str, lines: &[Line], fixed: &[Line]) -> Result<(), String> {
    let text: String = fixed
        .iter()
        .map(|line| format!("{}\n", line.text))
        .collect();
    // Through `llm_cache` so §6's store rules hold here too: an empty answer is never filed, and a
    // cache that will not write comes back as a sentence for the log instead of failing the fix.
    match crate::llm_cache::store_at(tree, FIX_STEP, &block_key(prompt, lines), &text) {
        crate::llm_cache::Stored::Failed(reason) => Err(reason),
        _ => Ok(()),
    }
}

/// S3: the tool calls one ask is allowed to make.
///
/// A reply is a list of `fix_line(n, text)` calls and nothing else — §3's rule that the model never
/// rewrites the block wholesale. So an answer arriving as anything but a fix (`flag_line`'s note, or
/// `finish`) goes back through [`Block`] and comes back as its own answer; only a fix changes a row.
pub fn apply_reply(block: &mut Block, reply: &[Reply]) -> Vec<String> {
    reply
        .iter()
        .map(|call| match call {
            Reply::Fix { n, text } => block.fix_line(*n, text),
            // `flag_line` records a note and `get_lines`/`finish` answer questions: neither is refused,
            // and neither ends the flow.
            Reply::Flag { n, why } => block.flag_line(*n, why),
            Reply::Done => crate::tools::ok(&serde_json::json!({ "finished": true })),
            // Rows are turned into fixes by `fix_block` before a reply is applied; one reaching here says so.
            Reply::Row(_) => {
                crate::tools::error("an answered row was not checked against its block")
            }
            Reply::Unreadable(why) => crate::tools::error(why),
        })
        .collect()
}

/// One call in a reply, as §3's tool surface spells it.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Fix {
        n: u32,
        text: String,
    },
    Flag {
        n: u32,
        why: String,
    },
    Done,
    /// One row of a wire answer, held against the block by [`fix_block`] before it becomes a fix.
    Row(AnsweredRow),
    /// An answer with no rows in it at all: refused, with the reason the model hears next.
    Unreadable(String),
}

/// S4: the source's own fixed files. A recorder writes `commentary.fixed.tsv` because its words are
/// the narration rather than content, and only a video gets `subtitles.srt` — there is nothing to
/// caption over an audio file. The subtitles show the *fixed* text: they are for watching, not for
/// auditing what the ASR heard.
pub fn write_fixed(
    tree: &Tree,
    source: &str,
    is_video: bool,
    is_commentary: bool,
    lines: &[Line],
) -> Result<Vec<PathBuf>, String> {
    let tsv = match is_commentary {
        true => tree.commentary_fixed_tsv(source),
        false => tree.transcript_fixed_tsv(source),
    };
    textfmt::write_lines(lines, &tsv)?;
    let mut written = vec![tsv];
    if is_video {
        // The same writer the transcript step used, so both files carry §6's 0644 and one shape.
        textfmt::write_rows_or_none(
            &tree.subtitles_srt(source),
            &crate::transcribe::srt_text(lines),
        )?;
        written.push(tree.subtitles_srt(source));
    }
    // Nothing heard, nothing written: neither file exists (§6), and the caller lists none.
    written.retain(|file| file.exists());
    Ok(written)
}

/// S5: everything on one timeline — each source's fixed rows plus the things that happened on screen,
/// which are rows too because the cut has to know where the slide changed. Sorted by start with a
/// stable sort, so two rows in the same second keep the order they were handed over in rather than
/// shuffling between runs.
pub fn merge_session(
    lines_by_source: &[(String, Vec<Line>)],
    events_by_source: &[(String, Vec<FrameEvent>)],
) -> Vec<SessionLine> {
    let mut rows: Vec<SessionLine> = Vec::new();
    for (base, lines) in lines_by_source {
        for line in lines {
            rows.push(SessionLine {
                start: line.start,
                end: line.end,
                source: base.clone(),
                who: line.speaker.clone(),
                text: line.text.clone(),
            });
        }
    }
    for (base, events) in events_by_source {
        for event in events {
            rows.push(SessionLine {
                start: event.start,
                end: event.end,
                source: base.clone(),
                who: "EVENT".to_string(),
                text: event.text.clone(),
            });
        }
    }
    rows.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

/// S5: what the merge says. Sources are counted once each, events included, so a screen recording with
/// no speech at all still shows up in the total.
pub fn timeline_log(rows: usize, sources: usize) -> String {
    format!(">>> session timeline: {rows} rows across {sources} source(s)")
}

/// A stretch the marking pass marked — an abandoned take, or a join to read straight past. F1.9 and
/// F1.10 own what produces one; this step only folds them into the text the cut reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub start: f64,
    pub end: f64,
    /// Where it was said again, when there is a take to read instead.
    pub again: Option<f64>,
}

/// S6: run the marking pass `P.policy.markingPass` names — retakes ([`crate::tools`]' F1.9 pass) or
/// joins (F1.10's). F1.9 and F1.10 own what produces a mark; this step only calls for one and folds the
/// answer in.
///
/// `MarkingPass::None` asks for nothing — that is a policy, not an omission. A failure is not this
/// step's failure either: the timeline is written and usable, so the caller logs [`marking_log`] and
/// continues unmarked rather than losing the fixes it just paid for.
pub fn marks_for<F>(pass: MarkingPass, mut mark: F) -> Result<Vec<Mark>, String>
where
    F: FnMut() -> Result<Vec<Mark>, String>,
{
    match pass {
        MarkingPass::None => Ok(Vec::new()),
        _ => mark(),
    }
}

/// S6/S7: fold the marks into the timeline's text, one line apiece.
///
/// It says ALREADY REMOVED in plain words because that is what the cut does with them, on the word:
/// told only that a stretch was "not kept", a model ends its segment in front of the mark instead — and
/// its own boundary is coarser than the mark, so every marker would cost a word off the sentence before
/// it. Rows inside a mark contribute nothing of their own, however many there are.
pub fn fold_marks(rows: &[SessionLine], marks: &[Mark]) -> String {
    // No narrator here: this is the shape of the fold, and a caller that has one calls [`labelled`].
    labelled(rows, "", marks)
}

/// [`fold_marks`] with the narrator's row named: what the user opens and what the cut model reads are
/// the same text, which is why `session.txt` is written from here rather than rendered later.
pub fn labelled(rows: &[SessionLine], narrator: &str, marks: &[Mark]) -> String {
    let mut out = String::new();
    // One line per mark, however many rows fall inside it.
    let mut folded: Vec<bool> = vec![false; marks.len()];
    for row in rows {
        match marks.iter().position(|mark| covers(mark, row)) {
            Some(index) => {
                if folded[index] {
                    continue;
                }
                folded[index] = true;
                let mark = &marks[index];
                let again = match mark.again {
                    Some(again) => format!(", said again at {}", stamp(again)),
                    None => String::new(),
                };
                out.push_str(&format!(
                    "{} (abandoned attempt to {}{again} -- already removed, read straight past it)\n",
                    stamp(mark.start),
                    stamp(mark.end)
                ));
            }
            None => out.push_str(&format!(
                "{}\n",
                session_line(
                    row.start,
                    row.end,
                    &label(&row.source, &row.who, narrator),
                    &row.text
                )
            )),
        }
    }
    out
}

/// S6: what a marking pass that could not run says. Named for the pass, because "retakes" and "joins"
/// are different things to go looking for.
pub fn marking_log(err: &str, pass: MarkingPass) -> String {
    let name = match pass {
        MarkingPass::Joins => "joins",
        _ => "retakes",
    };
    format!("!!! {name}: {err} -- the timeline stands unmarked")
}

/// S7: who a row is shown as. The narrator's own microphone is labelled by its role rather than by a
/// diariser that has never heard anyone else, an empty speaker says what it means, and an event is not
/// a person at all.
pub fn label(source_base: &str, speaker: &str, narrator: &str) -> String {
    if speaker == "EVENT" {
        return "EVENT".to_string();
    }
    if !narrator.is_empty() && source_base == narrator {
        return "NARRATOR".to_string();
    }
    if speaker.trim().is_empty() {
        return "SPEAKER".to_string();
    }
    speaker.to_string()
}

/// S7: a moment on the session clock, as the cut model is shown it. `mm:ss` because a bare second count
/// has to be divided by hand before it can be compared with anything ([`crate::tools::mm_ss`]).
pub fn stamp(seconds: f64) -> String {
    crate::tools::mm_ss(seconds)
}

/// S7: a stretch, `mm:ss-mm:ss`.
/// S7: one line of `session.txt` — `[<start>s-<end>s | mm:ss] LABEL: text`. The seconds are on the
/// left because the model that reads this file answers in them (every cut point it proposes is a number
/// of seconds); the `mm:ss` beside them is what a person scanning the same line for a moment recognises.
pub fn stamp_span(start: f64, end: f64) -> String {
    format!("{}-{}", stamp(start), stamp(end))
}

/// S7: the whole of one `session.txt` line, both clocks in front of the label.
pub fn session_line(start: f64, end: f64, label: &str, text: &str) -> String {
    format!(
        "[{}s-{}s | {}] {}: {}",
        stamp_seconds(start),
        stamp_seconds(end),
        stamp(start),
        label,
        flatten(text)
    )
}

/// A second as `session.txt` spells it: whole numbers lose the `.0`, fractions keep what they need.
fn stamp_seconds(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    let mut text = format!("{value:.2}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text.trim_end_matches('.').to_string()
}

/// The text of one row on one line: the block underneath this list is already a wall of lines, and a
/// wrapped row reads as two rows.
fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One line of the grounding text: `mm:ss-mm:ss label: text`, with the text on one line because the
/// block underneath it is already a wall of lines and a wrapped row reads as two.
#[allow(dead_code)]
fn one_line(base: &str, speaker: &str, narrator: &str, text: &str) -> String {
    format!("{}: {}", label(base, speaker, narrator), flatten(text))
}

/// S7: the timeline as the cut model reads it — one line each, stamped, marked stretches folded into a
/// single line apiece by [`fold_marks`].
pub fn session_text(rows: &[SessionLine], narrator: &str, marks: &[Mark]) -> String {
    labelled(rows, narrator, marks)
}

/// Does this mark cover this row? A row starting exactly on a mark's end is outside it — that is where
/// the take that replaced the abandoned one begins.
fn covers(mark: &Mark, row: &SessionLine) -> bool {
    row.start >= mark.start && row.start < mark.end
}

/// S7: write `session.txt`.
pub fn write_session_text(tree: &Tree, text: &str) -> Result<(), String> {
    textfmt::write_rows_or_none(&tree.session_txt(), text)
}

/// What the whole step produced. `marked` is false when the policy said none or the pass failed — in
/// which case the timeline still stands, unmarked.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub logs: Vec<String>,
    /// What the progress bar was told; F0.5 owns the bar.
    pub steps: Vec<String>,
    pub fixed_lines: usize,
    pub blocks: usize,
    pub rows: usize,
    pub sources: usize,
    pub marks: usize,
    pub marked: bool,
    pub files: Vec<PathBuf>,
}

/// Does this source still owe the fixer anything? Its fixed file is what the per-source fix rows leave
/// behind, so a source that has one was walked by them this run and owes nothing more; a source without one
/// (a resumed run, a fix row that failed or was skipped) still owes its blocks.
///
/// Counted on the FILE rather than on `cache/llm/fix` on purpose: an incomplete answer is deliberately not
/// cached (§S3, so a failed block is re-asked next run), and a source whose blocks all failed is exactly
/// the case the session walk exists to try again with every recording's grounding around it.
pub fn owes_the_fixer(tree: &Tree, source: &Source) -> bool {
    let path = match source.is_commentary {
        true => tree.commentary_fixed_tsv(&source.base),
        false => tree.transcript_fixed_tsv(&source.base),
    };
    !path.exists()
}

/// S1-S7 for one session.
///
/// `ask` is the fixer: handed [`ask_text`]'s message, it answers with the `fix_line` calls the model
/// made for that block. `mark` is F1.9's or F1.10's pass, called once and only when
/// [`MarkingPass`] asks for it. Times arrive on the session clock: a source's `start` is where its own
/// second nought sits, and rows are placed by adding it — the same rule [`crate::clock`] keeps
/// everywhere else, which is what lets a lane, a transcript and a cut agree.
pub fn run<F, M>(
    tree: &Tree,
    sources: &[Source],
    narrator: &str,
    pass: MarkingPass,
    prompt: &str,
    mut ask: F,
    mut mark: M,
) -> Result<Outcome, String>
where
    F: FnMut(&str) -> Result<Vec<Reply>, String>,
    M: FnMut() -> Result<Vec<Mark>, String>,
{
    let mut out = Outcome::default();

    // S1: the clock first, and `offsets.tsv` with it — before any block goes out, so the seconds a
    // block's context names are ones the timeline on disk already agrees with.
    let names: Vec<&str> = sources
        .iter()
        .map(|source| source.file_name.as_str())
        .collect();
    let starts = placement(&names);
    let placed: Vec<Source> = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let mut source = source.clone();
            source.start = starts[index];
            source
        })
        .collect();
    write_offsets(tree, &placed)?;

    // S1: load everything, so a block's context can name another source's rows without re-reading.
    let mut lines_by_source: Vec<(String, Vec<Line>)> = Vec::new();
    let mut events_by_source: Vec<(String, Vec<FrameEvent>)> = Vec::new();
    for source in &placed {
        let at = source.start;
        let mut lines = load_transcript(tree, &source.base)?;
        for line in lines.iter_mut() {
            line.start += at;
            line.end += at;
        }
        let mut events = load_events(tree, &source.base);
        for event in events.iter_mut() {
            event.start += at;
            event.end += at;
        }
        lines_by_source.push((source.base.clone(), lines));
        events_by_source.push((source.base.clone(), events));
    }

    // S2-S4: block by block, source by source.
    let mut fixed_by_source: Vec<(String, Vec<Line>)> = Vec::new();
    for source in &placed {
        let own = lines_by_source
            .iter()
            .find(|(base, _)| base == &source.base)
            .map(|(_, lines)| lines.clone())
            .unwrap_or_default();
        let every_line: Vec<(String, Line)> = lines_by_source
            .iter()
            .flat_map(|(base, lines)| lines.iter().map(move |line| (base.clone(), line.clone())))
            .collect();
        let every_event: Vec<(String, FrameEvent)> = events_by_source
            .iter()
            .flat_map(|(base, events)| {
                events
                    .iter()
                    .map(move |event| (base.clone(), event.clone()))
            })
            .collect();

        let blocks = blocks(&own);
        let total = blocks.len();
        let mut fixed_lines: Vec<Line> = Vec::new();
        for (index, block) in blocks.into_iter().enumerate() {
            out.steps.push(checkpoint(index + 1, total, &source.base));
            let (from, to) = context_window(
                block.first().map(|line| line.start).unwrap_or(0.0),
                block.last().map(|line| line.end).unwrap_or(0.0),
            );
            let context = context_text(from, to, &source.base, &placed, &every_line, &every_event);
            // S3: the cache is keyed on the block's own rows, so a re-run of an unchanged transcript
            // reads its answer back instead of paying for the same question twice. `fix_block` builds
            // the message it asks with: [`ask_text`] over the grounding from `context_text`.
            let cached = cached_block(tree, prompt, &block);
            let from_cache = cached.is_some();
            let outcome = match cached {
                Some(reply) => fix_block(&block, &context, |_| Ok(reply.clone()))?,
                None => fix_block(&block, &context, &mut ask)?,
            };
            out.blocks += 1;
            out.fixed_lines += outcome.fixed;
            for (n, why) in outcome.flags {
                out.logs
                    .push(format!(">>> {} line {n}: {why}", source.base));
            }
            if !outcome.valid {
                out.logs.push(give_up_log(&source.base, index, total));
            }
            if !from_cache {
                // [`store_block`]'s rule: the outcome is filed under the block's input, whichever try earned it
                // and also when every try was refused (the ASR lines kept), and an answer just read back is not
                // written again.
                if let Err(err) = store_block(tree, prompt, &block, &outcome.lines) {
                    // A cache that will not write is not a reason to lose the fix.
                    out.logs.push(format!("!!! could not keep the fix: {err}"));
                }
            }
            fixed_lines.extend(outcome.lines);
        }
        out.files.extend(write_fixed(
            tree,
            &source.base,
            source.is_video,
            source.is_commentary,
            &fixed_lines,
        )?);
        fixed_by_source.push((source.base.clone(), fixed_lines));
    }

    // S5: the merged timeline.
    let rows = merge_session(&fixed_by_source, &events_by_source);
    textfmt::write_session(&rows, &tree.session_tsv())?;
    out.rows = rows.len();
    out.sources = fixed_by_source
        .iter()
        .map(|(base, _)| base.clone())
        .chain(events_by_source.iter().map(|(base, _)| base.clone()))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    out.logs.push(timeline_log(out.rows, out.sources));

    // S6: the marking pass. Its failure is a log line, not this step's end.
    let marks = match marks_for(pass, &mut mark) {
        Ok(marks) => {
            out.marked = !marks.is_empty() && pass != MarkingPass::None;
            marks
        }
        Err(err) => {
            out.logs.push(marking_log(&err, pass));
            Vec::new()
        }
    };
    out.marks = marks.len();

    // S7: the text the cut reads, which is also the text the user opens.
    let text = session_text(&rows, narrator, &marks);
    write_session_text(tree, &text)?;
    if tree.session_txt().exists() {
        out.files.push(tree.session_txt());
    }
    Ok(out)
}
