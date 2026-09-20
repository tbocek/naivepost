//! ASR in chunks (spec/04-prepare.md F1.4): what happens between `voice16k.wav` and `words.json`.
//!
//! The audio server is given a slice of the recording, not the recording: an answer comes back as
//! text plus word times relative to what it heard, so a whole lecture in one request would be both
//! too big for the box and too long for a model that loses its place. What leaves here is the three
//! files F1.3 reads — `transcript.txt`, `asrchunks.json`, `words.json` — and nothing about chunks
//! reaches further than that. As in [`crate::separate`] and [`crate::transcribe`], no process is
//! spawned and no socket opened: ffmpeg's cut, the box's answer and where the recording goes quiet
//! arrive as arguments (spec/00-principles.md §5).

use crate::layout::Tree;
use crate::requests::{self, Chunk, Word, WordsDoc};

/// The longest audio in one request (`asrChunkMax`). A chunk edge is a place the decoder loses its
/// context, so the text at a join is the worst text in the file — which is why this starts at what
/// the model allows and shortens only when told to, rather than being set small to be safe.
pub const CHUNK_MAX: f64 = 300.0;

/// The same ceiling for the qwen3 family (`asrChunkQwen`), which cuts off earlier on a long request:
/// a fifth of the audio per ask, five times the requests, and that trade is this family's own.
pub const CHUNK_QWEN: f64 = 60.0;

/// The shortest chunk worth asking for (`asrChunkMin`). Under this the edges get so close together
/// that words land on both sides of one, and the halving loop has to stop somewhere anyway.
pub const CHUNK_MIN: f64 = 20.0;

/// How far from an even cut a silence is worth taking (`asrCutSeek`) — before it is trimmed to what
/// the pieces can afford, which is [`cut_points`]'s job.
pub const SEEK_MAX: f64 = 20.0;

/// What counts as quiet enough to cut in (`asrQuietDB`, `asrQuietMin`): -35 dBFS for 0.4 s or longer.
/// ffmpeg's `silencedetect` finds these and the caller hands them over; finding none is not an error,
/// it only means the cuts land on the clock.
pub const QUIET_DB: f64 = -35.0;
pub const QUIET_MIN: f64 = 0.4;

/// One quiet stretch of the recording, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Silence {
    pub start: f64,
    pub end: f64,
}

impl Silence {
    /// The middle of a silence — where a cut belongs, since both edges are places sound was absent
    /// for a moment and the middle is furthest from either.
    pub fn midpoint(&self) -> f64 {
        (self.start + self.end) / 2.0
    }
}

/// S1: how much audio one request may carry, which is a property of the model rather than of the
/// machine. The family comes from the audio box's catalogue; anything unknown gets the general
/// ceiling, since guessing "qwen3" at an unfamiliar model would shorten every request for nothing.
pub fn chunk_limit(model_family: &str) -> f64 {
    if model_family.contains("qwen3") {
        CHUNK_QWEN
    } else {
        CHUNK_MAX
    }
}

/// S1: the limit after one halving, and what to say about it. `None` in the log means "this was as
/// far as it could go", which is what ends the retry loop rather than leaving it to spin.
#[derive(Debug, Clone, PartialEq)]
pub struct Halved {
    pub limit: f64,
    pub log: Option<String>,
}

/// S1: halve the limit, never under [`CHUNK_MIN`]. Better to say out loud that the machine is out of
/// room than to fail quietly — the same reasoning F0.7 gives for a degraded mode.
pub fn halve(base: &str, limit: f64) -> Halved {
    if limit <= CHUNK_MIN {
        return Halved { limit, log: None };
    }
    let next = (limit / 2.0).floor().max(CHUNK_MIN);
    Halved {
        limit: next,
        log: Some(format!(
            "!!! [{base}] ASR: no room for that much audio -- trying {next:.0} s at a time"
        )),
    }
}

/// S1: whether an error is the box saying it had no room, which is the only kind worth retrying. The
/// server's wording is not ours to fix, so this looks for the three shapes it arrives as rather than
/// one exact sentence; anything else — a model that is not loaded, a bad upload — is a refusal.
pub fn retryable(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("out of memory") || lower.contains("no room") || lower.contains("oom")
}

/// S2: whether the whole recording goes up in one request. Equal to the limit is one request — the
/// ceiling is what a request may be, not what it must be under.
pub fn fits(duration: f64, limit: f64) -> bool {
    duration <= limit
}

/// S2: the edges between the pieces, `0.0` and `duration` included so `n` pieces are `n + 1` edges.
///
/// Even pieces, not full ones with a runt: a 40 s tail is a whole extra request whose text is the
/// worst in the file. The count comes from `limit - 2 * seek` because each cut may slide that far and
/// the piece between two of them would grow by both. Then the reach itself is trimmed twice — to a
/// sixth of the ceiling, so every piece keeps at least two thirds of what the model allows, and to a
/// third of the step, so cuts stay in order and each inside its own piece. A generous reach against a
/// short ceiling was once enough to make 19 s pieces out of a 60 s limit: three times the requests
/// and three times the joins.
pub fn cut_points(duration: f64, silences: &[Silence], limit: f64, seek: f64) -> Vec<f64> {
    if fits(duration, limit) || limit <= 0.0 {
        return vec![0.0, duration];
    }
    // A reach of half the ceiling or more would let two cuts walk past each other.
    let mut seek = if seek < 0.0 || 2.0 * seek >= limit { 0.0 } else { seek };
    seek = seek.min(limit / 6.0);
    let pieces = (duration / (limit - 2.0 * seek)).ceil() as usize;
    let step = duration / pieces as f64;
    seek = seek.min(step / 3.0);

    let mut edges = vec![0.0];
    let mut previous = 0.0;
    for piece in 1..pieces {
        let want = step * piece as f64;
        let at = silences
            .iter()
            .map(Silence::midpoint)
            .filter(|mid| (mid - want).abs() < seek)
            .min_by(|a, b| {
                (a - want)
                    .abs()
                    .partial_cmp(&(b - want).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            // An edge that would swallow the piece before it, or make this one longer than the
            // ceiling, is worse than a cut in mid-sentence: the nominal time stands.
            .filter(|mid| *mid > previous && *mid - previous <= limit)
            .unwrap_or(want);
        edges.push(at);
        previous = at;
    }
    edges.push(duration);
    edges
}

/// S2/S3: the file one chunk is cut into, under `tree.asr_scratch(source)`. Numbered so a kept
/// scratch folder reads as the sequence it was.
pub fn part(index: usize) -> String {
    format!("c{index:02}.wav")
}

/// S3: what the progress bar says while a chunk is up there. Recorded rather than drawn — F0.5 owns
/// the bar, as [`crate::add_sources`] records its byte fraction for the same reason.
pub fn progress(index: usize, total: usize) -> String {
    format!("recognising speech {}/{}", index + 1, total)
}

/// S3: ffmpeg's arguments for one chunk, `-ss` before `-i` because on the pcm this stage wrote that
/// seek is exact rather than a keyframe away. The caller appends its own `-i` and output path.
pub fn cut_plan(from: f64, to: f64) -> Vec<String> {
    vec!["-ss".into(), trim_number(from), "-t".into(), trim_number(to - from)]
}

/// A number without a tail of `.0`, which is what makes the logged command line readable and keeps
/// ffmpeg's own parsing out of it.
fn trim_number(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    format!("{value}")
}

/// S3: the body the audio box is asked with — the chunk's file and the ASR code, both of which the
/// server needs (the language is what tells it what to expect; an empty one is left to the server).
pub fn body(audio: &str, language: &str) -> serde_json::Value {
    serde_json::json!({ "audio": audio, "language": language })
}

/// S4: one chunk's words moved to where the chunk was in the recording. A word's times are relative
/// to the file the server heard, which started at the chunk's edge; unshifted, every chunk's words
/// would land at the start of the recording and the segments would be nonsense.
pub fn shift(words: &[Word], offset_samples: u64) -> Vec<Word> {
    words
        .iter()
        .map(|word| Word {
            word: word.word.clone(),
            start_sample: word.start_sample + offset_samples,
            end_sample: word.end_sample + offset_samples,
        })
        .collect()
}

/// S4: the pieces put back as one answer. Text is joined with a single space — the join is already
/// the worst text in the file and a paragraph break would claim more than it says — and an empty
/// chunk contributes nothing rather than a gap.
pub fn stitch(pieces: &[Piece]) -> (String, Vec<Word>) {
    let texts: Vec<&str> = pieces
        .iter()
        .map(|piece| piece.text.as_str())
        .filter(|text| !text.trim().is_empty())
        .collect();
    let words = pieces.iter().flat_map(|piece| piece.words.clone()).collect();
    (texts.join(" "), words)
}

/// S4: nothing was heard. A real case rather than an error — a screen capture with no microphone
/// behind it is an ordinary thing to import.
pub fn no_speech_log(base: &str) -> String {
    format!(">>> [{base}] no speech found -- an empty transcript")
}

/// What the box answered for one chunk. `out_of_memory` is set from [`retryable`] on the error, so a
/// caller that gets `Err` can still tell whether it was worth asking again.
#[derive(Debug, Clone, Default)]
pub struct Answer {
    pub text: String,
    pub words: Vec<Word>,
}

/// One request's slice of the recording: where it started and ended, what came back as text, and its
/// word times still relative to itself. Named so [`stitch`] reads as what it is — the pieces joined
/// in order — rather than a tuple a reader has to decode.
#[derive(Debug, Clone, Default)]
pub struct Piece {
    pub from: f64,
    pub to: f64,
    pub text: String,
    pub words: Vec<Word>,
}

/// What one pass produced: what to log, what the progress bar was told (`steps`, since F0.5 owns the
/// bar), the rows of `asrchunks.json`, and the joined answer.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub logs: Vec<String>,
    pub steps: Vec<String>,
    pub chunks: Vec<Chunk>,
    pub text: String,
    pub words: Vec<Word>,
}

/// S1-S4 for one source. `ask` is handed the chunk's file name and answers with that chunk's text
/// and word times; it is called once per chunk, in order, and its answer is what gets shifted. The
/// real ▶ hands over ffmpeg's cut, the upload and the ASR request; a test hands over a closure.
///
/// The three files are written in the spec's order and `words.json` last: it is F1.3's resume marker,
/// so nothing may exist after it that the marker does not stand for.
pub fn run<F>(
    tree: &Tree,
    source: &str,
    duration: f64,
    model_family: &str,
    language: &str,
    silences: &[Silence],
    mut ask: F,
) -> Result<Outcome, String>
where
    F: FnMut(&str) -> Result<Answer, String>,
{
    let mut logs = Vec::new();
    // S1: the ceiling is the model's, and it comes down only when the box says it has no room.
    let mut limit = chunk_limit(model_family);

    loop {
        let attempt = run_at(
            tree,
            source,
            duration,
            language,
            silences,
            limit,
            &mut ask,
        );
        match attempt {
            Ok(mut done) => {
                // `done.steps` stand as they are: the progress strings of an attempt that failed
                // belonged to a cut being thrown away with it.
                // The halving lines came before this attempt's, so they go in front of it.
                let mut merged = std::mem::take(&mut logs);
                merged.extend(done.logs);
                done.logs = merged;
                // S2: the scratch is this stage's own mess — gone once the answer is on disk, kept
                // when it is not, because `asr/c03.wav` is how a failed chunk is compared against its
                // audio without asking the server again.
                let _ = std::fs::remove_dir_all(tree.asr_scratch(source));
                return Ok(done);
            }
            Err(err) if retryable(&err) && limit > CHUNK_MIN => {
                // Halve, say so, and start the recording over at the smaller size: the pieces change,
                // so there is no half-finished chunk to carry across.
                let Halved { limit: next, log } = halve(source, limit);
                if log.is_none() {
                    return Err(err);
                }
                logs.extend(log);
                limit = next;
            }
            // At the floor and still no room, or a refusal that halving could not help: out it goes.
            Err(err) => return Err(err),
        }
    }
}

/// One pass at one ceiling. Split out so the halving loop above only decides whether to try again.
fn run_at<F>(
    tree: &Tree,
    source: &str,
    duration: f64,
    language: &str,
    silences: &[Silence],
    limit: f64,
    ask: &mut F,
) -> Result<Outcome, String>
where
    F: FnMut(&str) -> Result<Answer, String>,
{
    let mut logs = Vec::new();
    let mut steps = Vec::new();

    // S2: one request when the recording fits, however the ceiling was arrived at.
    let edges = if fits(duration, limit) {
        vec![0.0, duration]
    } else {
        cut_points(duration, silences, limit, SEEK_MAX)
    };
    let chunks = edges.len() - 1;

    let mut pieces: Vec<Piece> = Vec::with_capacity(chunks);
    for index in 0..chunks {
        let (from, to) = (edges[index], edges[index + 1]);
        steps.push(progress(index, chunks));
        // S3: the cut is ffmpeg's — `cut_plan`'s arguments against `voice16k.wav`, written to
        // `asr/cNN.wav` — and what the box is asked with is `body`. Both are this module's decision
        // and neither is executed by it; the caller runs them and answers with what came back.
        let file = part(index);
        let (plan, asked_with) = (cut_plan(from, to), body(&file, language));
        make_scratch(tree, source, &file, &plan, &asked_with)?;
        let answer = ask(&file)?;
        // S4: the chunk's times are relative to the chunk, so they move by where it started.
        let offset = (from * crate::transcribe::SAMPLE_RATE as f64) as u64;
        pieces.push(Piece {
            from,
            to,
            text: answer.text,
            words: shift(&answer.words, offset),
        });
    }

    // S4: text and words joined, then the three files in the order that makes a resume honest.
    let (text, words) = stitch(&pieces);
    if text.trim().is_empty() {
        logs.push(no_speech_log(source));
    }
    write_transcript_txt(tree, source, &text)?;
    let rows: Vec<Chunk> = pieces
        .iter()
        .map(|piece| Chunk {
            s: piece.from,
            e: piece.to,
            text: piece.text.clone(),
        })
        .collect();
    requests::write_chunks(tree, source, &rows)?;
    requests::write_words(
        tree,
        source,
        &WordsDoc {
            text: text.clone(),
            words: words.clone(),
        },
    )?;

    Ok(Outcome {
        logs,
        steps,
        chunks: rows,
        text,
        words,
    })
}

/// `transcript.txt`: the text and one trailing newline. The nothing-heard case is a lone newline,
/// which is how [`requests::write_silence`] spells it — so an empty transcript keeps that shape
/// rather than becoming a zero-byte file, which every reader here treats as "not written yet".
fn write_transcript_txt(tree: &Tree, source: &str, text: &str) -> Result<(), String> {
    let path = tree.transcript_txt(source);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let body = format!("{}\n", text.trim_end_matches('\n'));
    std::fs::write(&path, body).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(())
}

/// The scratch file for one chunk, written so a failed pass leaves something to look at: the cut's
/// arguments and the request body beside the audio that was uploaded, which is how `asr/c03.wav` gets
/// compared against its answer without asking the server again. Named after the chunk's wav with a
/// `.plan` suffix so it cannot be mistaken for audio.
fn make_scratch(
    tree: &Tree,
    source: &str,
    file: &str,
    plan: &[String],
    body: &serde_json::Value,
) -> Result<(), String> {
    let dir = tree.asr_scratch(source);
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let text = format!("{}\n{}\n", plan.join(" "), body);
    std::fs::write(dir.join(format!("{file}.plan")), text)
        .map_err(|err| format!("{}: {err}", dir.join(file).display()))?;
    Ok(())
}
