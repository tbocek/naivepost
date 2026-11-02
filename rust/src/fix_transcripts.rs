//! Fix the transcripts (spec/04-prepare.md F1.8): what happens between a source's ASR rows and the
//! one timeline the cut model reads.

use std::path::PathBuf;

use crate::clock;
use crate::describe::flatten_line as flatten;
use crate::layout::Tree;
use crate::prepare::FIX_BLOCK_LINES;
use crate::project::MarkingPass;
use crate::textfmt::{self, FrameEvent, Line, SessionLine};
use crate::tools::fix::{Block, FIX_CONTEXT_SECONDS};

/// The cache/step key for these replies, so a re-run reads its own answers back.
pub const FIX_STEP: &str = "fix";

/// Tries per block before the ASR original is kept.
pub const BLOCK_TRIES: u32 = 2;

/// How many asks one block gets in all — the first plus the retries.
pub fn attempts() -> u32 {
    BLOCK_TRIES
}

/// What a block that ran out of tries says.
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
pub fn placement(names: &[&str]) -> Vec<f64> {
    let clock = clock::clock(names);
    (0..names.len()).map(|index| clock.offset(index)).collect()
}

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

/// S1: write `offsets.tsv`.
pub fn write_offsets(tree: &Tree, sources: &[Source]) -> Result<usize, String> {
    let rows = offsets_rows(sources);
    let mut out = String::new();
    for (video, audio, offset) in &rows {
        out.push_str(&format!("{video}\t{audio}\t{offset:.2}\n"));
    }
    textfmt::write_rows_or_none(&tree.offsets_tsv(), &out)?;
    Ok(rows.len())
}

/// S1: a source's own transcript, as its folder holds it.
pub fn load_transcript(tree: &Tree, source: &str) -> Result<Vec<Line>, String> {
    textfmt::read_lines(&tree.transcript_tsv(source))
}

/// S1: a footage source's event log — what happened on screen, which is context for the words beside
/// it.
pub fn load_events(tree: &Tree, source: &str) -> Vec<FrameEvent> {
    textfmt::read_events(&tree.events_tsv(source)).unwrap_or_default()
}

/// Is this a tool refusal?
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

/// S2: the blocks one transcript goes out in.
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

/// S2: the grounding itself.
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
            line.text.split_whitespace().collect::<Vec<_>>().join(" ")
        ));
    }
    for (base, event) in events {
        if event.end <= from || event.start >= to {
            continue;
        }
        // Own events included: a slide changing is why the words suddenly do not make sense.
        debug_assert!(sources.iter().any(|source| source.base == *base) || sources.is_empty());
        out.push_str(&format!("EVENT ({base}): {}\n", event.text));
    }
    out
}

/// One line of the ask: numbered as the model will send it back, with its times and speaker.
pub fn ask_text(context: &str, lines: &[Line]) -> String {
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

/// S2: the checkpoint before a block goes out.
pub fn checkpoint(block: usize, blocks: usize, source: &str) -> String {
    format!(">>> fixing {source}: block {block}/{blocks}")
}

/// What one block came back as.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockOutcome {
    /// The block with the fixes applied: every time and speaker is the one that was there.
    pub lines: Vec<Line>,
    pub fixed: usize,
    /// What `flag_line` recorded — a wrong speaker, or a row nobody can make out.
    pub flags: Vec<(u32, String)>,
    /// False only when the tries ran out, in which case the caller keeps the ASR original.
    pub valid: bool,
    /// Which try answered — 1 for the first ask, 2 once a refusal has been shown back to the model.
    pub attempt: u32,
}

/// A row the fixer could not read.
pub const GARBAGE: &str = "cannot be made out";

/// What a model is told when its answer held no rows.
pub const NO_LINES: &str =
    "the answer held no lines -- answer with the lines asked, start, end, speaker and text separated by tabs";

/// A one-line block answered with the sentence alone: the time and speaker can only be its own.
fn bare_row(lines: &[Line], reply: &[Reply]) -> Option<AnsweredRow> {
    let [line] = lines else { return None };
    let [Reply::Bare(text)] = reply else {
        return None;
    };
    let words = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let (asked, answered) = (words(&line.text), words(text));
    let shared = asked.iter().filter(|word| answered.contains(word)).count();
    (shared * 2 >= asked.len() && answered.len() <= asked.len() * 2).then(|| AnsweredRow {
        start: line.start,
        end: line.end,
        speaker: line.speaker.clone(),
        text: text.trim().to_string(),
    })
}

/// S3: one block through the fixer's tools.
pub fn fix_block<F>(lines: &[Line], context: &str, ask: F) -> Result<BlockOutcome, String>
where
    F: FnMut(&str) -> Result<Vec<Reply>, String>,
{
    walk_block(lines, context, true, ask)
}

/// A stored answer read back. It went through the checks when it was given, and a block that ran out of tries is stored as its own lines, which must not read as a raw answer to be refused.
pub fn stored_block(
    lines: &[Line],
    context: &str,
    reply: Vec<Reply>,
) -> Result<BlockOutcome, String> {
    walk_block(lines, context, false, |_| Ok(reply.clone()))
}

fn walk_block<F>(
    lines: &[Line],
    context: &str,
    refuse_raw: bool,
    mut ask: F,
) -> Result<BlockOutcome, String>
where
    F: FnMut(&str) -> Result<Vec<Reply>, String>,
{
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
            Some(err) => format!("{message}\nThe previous answer was refused: {err}"),
            None => message.clone(),
        };
        let mut reply = ask(&ask_with)?;
        if let Some(row) = bare_row(lines, &reply) {
            reply = vec![Reply::Row(row)];
        }
        if let Some(why) = reply.iter().find_map(|call| match call {
            Reply::Bare(_) => Some(NO_LINES),
            _ => None,
        }) {
            refusals.push(why.to_string());
            continue;
        }
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
        let answers = apply_reply(&mut block, &reply);
        match answers.iter().find(|answer| refused(answer)) {
            Some(answer) => refusals.push(refusal_reason(answer)),
            None if refuse_raw && left_raw(lines, &apply(&block, lines)) => {
                refusals.push(LEFT_RAW.to_string())
            }
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

/// Fewest words for an unchanged answer to mean anything: "und noch äh" has nothing to clean.
const RAW_MIN_WORDS: usize = 6;

const LEFT_RAW: &str = "the lines came back exactly as asked, with no capitals or punctuation -- write them as clean text";

/// The block came back as it went out and still reads as the recogniser's bare words: an answer that passes every other check and cleans nothing.
fn left_raw(lines: &[Line], answered: &[Line]) -> bool {
    let unchanged = lines.len() == answered.len()
        && lines
            .iter()
            .zip(answered)
            .all(|(line, back)| line.text.trim() == back.text.trim());
    let bare = |line: &Line| {
        !line
            .text
            .chars()
            .any(|c| c.is_uppercase() || ".,:;?!".contains(c))
    };
    let words: usize = lines
        .iter()
        .map(|line| line.text.split_whitespace().count())
        .sum();
    unchanged && words >= RAW_MIN_WORDS && lines.iter().all(bare)
}

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

/// S3: how many of this source's blocks were read back from `cache/llm/fix` rather than asked again.
pub fn cached_blocks_log(base: &str, cached: usize) -> String {
    format!(">>> [{base}] {cached} block(s) answered from the cache")
}

/// S3: the cache key for a block — §6's digest over its own rows, times and text included, and over `prompt`,
/// the wording and User Context the block is asked under ([`crate::fix_call::cache_prompt`]).
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
        // height.
        model: "",
        thinking: false,
    };
    crate::llm_cache::key(&crate::llm_cache::parts(&request)).expect("a block of rows always keys")
}

/// S3: a block's stored reply, if it has one.
pub fn cached_block(tree: &Tree, prompt: &str, lines: &[Line]) -> Option<Vec<Reply>> {
    let text = crate::llm_cache::read_at(tree, FIX_STEP, &block_key(prompt, lines))?;
    // §09 §10: the block's stored reply writes its own `cache` line, with no time on the wire, so the
    // file shows what the re-run saved.
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
    for (index, (stored, original)) in text.lines().zip(lines).enumerate() {
        if stored != original.text {
            out.push(Reply::Fix {
                n: index as u32 + 1,
                text: stored.to_string(),
            });
        }
    }
    Some(out)
}

/// S3: store a block's outcome, one line per row in the block's own order, under the block's input: the
/// fixed lines whichever try earned them, or the ASR lines when every try was refused.
pub fn store_block(
    tree: &Tree,
    prompt: &str,
    lines: &[Line],
    fixed: &[Line],
) -> Result<(), String> {
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
pub fn apply_reply(block: &mut Block, reply: &[Reply]) -> Vec<String> {
    reply
        .iter()
        .map(|call| match call {
            Reply::Fix { n, text } => block.fix_line(*n, text),
            Reply::Flag { n, why } => block.flag_line(*n, why),
            Reply::Done => crate::tools::ok(&serde_json::json!({ "finished": true })),
            // Rows are turned into fixes by `fix_block` before a reply is applied; one reaching here says so.
            Reply::Row(_) => {
                crate::tools::error("an answered row was not checked against its block")
            }
            Reply::Bare(_) => crate::tools::error(NO_LINES),
            Reply::Unreadable(why) => crate::tools::error(why),
        })
        .collect()
}

/// One call in a reply, as §3's tool surface spells it.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Bare(String),
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
    Unreadable(String),
}

/// S4: the source's own fixed files.
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
/// which are rows too because the cut has to know where the slide changed.
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

/// S5: what the merge says.
pub fn timeline_log(rows: usize, sources: usize) -> String {
    format!(">>> session timeline: {rows} rows across {sources} source(s)")
}

/// A stretch the marking pass marked — an abandoned take, or a join to read straight past.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub start: f64,
    pub end: f64,
    /// Where it was said again, when there is a take to read instead.
    pub again: Option<f64>,
}

/// S6: run the marking pass `P.policy.markingPass` names — retakes ([`crate::tools`]' F1.9 pass) or
/// joins (F1.10's).
pub fn marks_for<F>(pass: MarkingPass, mut mark: F) -> Result<Vec<Mark>, String>
where
    F: FnMut() -> Result<Vec<Mark>, String>,
{
    match pass {
        MarkingPass::None => Ok(Vec::new()),
        _ => mark(),
    }
}

/// [`fold_marks`] with the narrator's row named: what the user opens and what the cut model reads are
/// the same text, which is why `session.txt` is written from here rather than rendered later.
pub fn labelled(rows: &[SessionLine], narrator: &str, marks: &[Mark]) -> String {
    let mut out = String::new();
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
                    Some(again) => format!(", said again at {}", crate::tools::mm_ss(again)),
                    None => String::new(),
                };
                out.push_str(&format!(
                    "{} (abandoned attempt to {}{again} -- already removed, read straight past it)\n",
                    crate::tools::mm_ss(mark.start),
                    crate::tools::mm_ss(mark.end)
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

/// S6: what a marking pass that could not run says.
pub fn marking_log(err: &str, pass: MarkingPass) -> String {
    let name = match pass {
        MarkingPass::Joins => "joins",
        _ => "retakes",
    };
    format!("!!! {name}: {err} -- the timeline stands unmarked")
}

/// S7: who a row is shown as.
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

/// S7: a stretch, `mm:ss-mm:ss`.
pub fn stamp_span(start: f64, end: f64) -> String {
    format!(
        "{}-{}",
        crate::tools::mm_ss(start),
        crate::tools::mm_ss(end)
    )
}

/// S7: the whole of one `session.txt` line, both clocks in front of the label.
pub fn session_line(start: f64, end: f64, label: &str, text: &str) -> String {
    format!(
        "[{}s-{}s | {}] {}: {}",
        stamp_seconds(start),
        stamp_seconds(end),
        crate::tools::mm_ss(start),
        label,
        flatten(text)
    )
}

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

/// One line of the grounding text: `mm:ss-mm:ss label: text`, with the text on one line because the
/// block underneath it is already a wall of lines and a wrapped row reads as two.
#[allow(dead_code)]
fn one_line(base: &str, speaker: &str, narrator: &str, text: &str) -> String {
    format!("{}: {}", label(base, speaker, narrator), flatten(text))
}

fn covers(mark: &Mark, row: &SessionLine) -> bool {
    row.start >= mark.start && row.start < mark.end
}

/// S7: write `session.txt`.
pub fn write_session_text(tree: &Tree, text: &str) -> Result<(), String> {
    textfmt::write_rows_or_none(&tree.session_txt(), text)
}

/// What the whole step produced.
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

/// Does this source still owe the fixer anything? A recording nobody spoke in has nothing to fix and never gets a fixed file.
pub fn owes_the_fixer(tree: &Tree, source: &Source) -> bool {
    let path = match source.is_commentary {
        true => tree.commentary_fixed_tsv(&source.base),
        false => tree.transcript_fixed_tsv(&source.base),
    };
    !path.exists() && load_transcript(tree, &source.base).map_or(true, |lines| !lines.is_empty())
}

/// S1-S7 for one session.
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
            let cached = cached_block(tree, prompt, &block);
            let from_cache = cached.is_some();
            let outcome = match cached {
                Some(reply) => stored_block(&block, &context, reply)?,
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
                if let Err(err) = store_block(tree, prompt, &block, &outcome.lines) {
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

    let text = labelled(&rows, narrator, &marks);
    write_session_text(tree, &text)?;
    if tree.session_txt().exists() {
        out.files.push(tree.session_txt());
    }
    Ok(out)
}
