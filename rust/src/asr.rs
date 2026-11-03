//! ASR in chunks (spec/04-prepare.md F1.4): what happens between `voice16k.wav` and `words.json`.

use crate::layout::Tree;
use crate::requests::{self, Chunk, Word, WordsDoc};

pub const CHUNK_MAX: f64 = 300.0;

pub const CHUNK_QWEN: f64 = 60.0;

pub const CHUNK_MIN: f64 = 20.0;

pub const SEEK_MAX: f64 = 20.0;

pub const QUIET_DB: f64 = -35.0;
pub const QUIET_MIN: f64 = 0.4;

/// One quiet stretch of the recording, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Silence {
    pub start: f64,
    pub end: f64,
}

impl Silence {
    pub fn midpoint(&self) -> f64 {
        (self.start + self.end) / 2.0
    }
}

/// S1: how much audio one request may carry, which is a property of the model rather than of the
/// machine.
pub fn chunk_limit(model_family: &str) -> f64 {
    if model_family.contains("qwen3") {
        CHUNK_QWEN
    } else {
        CHUNK_MAX
    }
}

/// S1: the limit after one halving, and what to say about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Halved {
    pub limit: f64,
    pub log: Option<String>,
}

/// S1: halve the limit, never under [`CHUNK_MIN`].
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

pub fn retryable(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("out of memory")
        || lower.contains("no room")
        || lower.contains("oom")
        || lower.contains("failed to allocate")
}

pub fn fits(duration: f64, limit: f64) -> bool {
    duration <= limit
}

/// S2: the edges between the pieces, `0.0` and `duration` included so `n` pieces are `n + 1` edges.
/// The count comes from `limit - 2 * seek` because each cut may slide that far and the piece
/// between two of them would grow by both.
pub fn cut_points(duration: f64, silences: &[Silence], limit: f64, seek: f64) -> Vec<f64> {
    if fits(duration, limit) || limit <= 0.0 {
        return vec![0.0, duration];
    }
    let mut seek = if seek < 0.0 || 2.0 * seek >= limit {
        0.0
    } else {
        seek
    };
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

pub fn part(index: usize) -> String {
    format!("c{index:02}.wav")
}

pub fn progress(index: usize, total: usize) -> String {
    format!("recognising speech {}/{}", index + 1, total)
}

/// S3: ffmpeg's arguments for one chunk, `-ss` before `-i` because on the pcm this stage wrote that
/// seek is exact rather than a keyframe away.
pub fn cut_plan(from: f64, to: f64) -> Vec<String> {
    vec![
        "-ss".into(),
        trim_number(from),
        "-t".into(),
        trim_number(to - from),
    ]
}

/// A number without a tail of `.0`, which is what makes the logged command line readable and keeps
/// ffmpeg's own parsing out of it.
pub(crate) fn trim_number(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    format!("{value}")
}

pub fn body(audio: &str, language: &str) -> serde_json::Value {
    serde_json::json!({ "audio": audio, "language": language })
}

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

pub fn stitch(pieces: &[Piece]) -> (String, Vec<Word>) {
    let texts: Vec<&str> = pieces
        .iter()
        .map(|piece| piece.text.as_str())
        .filter(|text| !text.trim().is_empty())
        .collect();
    let words = pieces
        .iter()
        .flat_map(|piece| piece.words.clone())
        .collect();
    (texts.join(" "), words)
}

pub fn no_speech_log(base: &str) -> String {
    format!(">>> [{base}] no speech found -- an empty transcript")
}

#[derive(Debug, Clone, Default)]
pub struct Answer {
    pub text: String,
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, Default)]
pub struct Piece {
    pub from: f64,
    pub to: f64,
    pub text: String,
    pub words: Vec<Word>,
}

/// What one pass produced: what to log, what the progress bar was told (`steps`, since F0.5 owns
/// the bar), the rows of `asrchunks.json`, and the joined answer.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub logs: Vec<String>,
    pub steps: Vec<String>,
    pub chunks: Vec<Chunk>,
    pub text: String,
    pub words: Vec<Word>,
}

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
    let mut limit = chunk_limit(model_family);

    loop {
        let attempt = run_at(tree, source, duration, language, silences, limit, &mut ask);
        match attempt {
            Ok(mut done) => {
                let mut merged = std::mem::take(&mut logs);
                merged.extend(done.logs);
                done.logs = merged;
                let _ = std::fs::remove_dir_all(tree.asr_scratch(source));
                return Ok(done);
            }
            Err(err) if retryable(&err) && limit > CHUNK_MIN => {
                let Halved { limit: next, log } = halve(source, limit);
                if log.is_none() {
                    return Err(err);
                }
                logs.extend(log);
                limit = next;
            }
            Err(err) => return Err(err),
        }
    }
}

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
        let file = part(index);
        let (plan, asked_with) = (cut_plan(from, to), body(&file, language));
        make_scratch(tree, source, &file, &plan, &asked_with)?;
        let answer = ask(&file)?;
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
/// arguments and the request body beside the audio that was uploaded, which is how `asr/c03.wav`
/// gets compared against its answer without asking the server again.
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
