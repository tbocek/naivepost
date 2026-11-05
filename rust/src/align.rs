//! Forced alignment (spec/04-prepare.md F1.5): which second each word was actually said in.
//!
//! The ASR's own stamps run a half to a full second behind the sound, useless for cutting on a word edge. This module decides which aligner, what each
//! request covers and what `words.aligned.json` holds. As in [`crate::asr`], no process is spawned: silences arrive as arguments, cuts are returned as ffmpeg arguments,
//! and answers arrive through [`run`]'s closure (spec/00-principles.md §5).

use serde_json::{json, Value};

use crate::asr::{self, Silence};
use crate::layout::Tree;
use crate::requests::{self, Chunk, Word};
use crate::services::{AudioModel, ALIGN_PREFERENCE};
use crate::transcribe::SAMPLE_RATE;

/// S2: audio per align request, the same as the ASR's chunks ([`asr::CHUNK_QWEN`]) so pieces line up (go/align.go:656).
pub const WINDOW: f64 = asr::CHUNK_QWEN;
/// S3: the longest window asked about whole, the ASR's own chunks included (older projects carry chunks up to 300 s).
/// qwen3-aligner timed whole windows of 61-107 s with no word lost and none squeezed, and refused 118 s and more
/// ("audio encoder token count exceeds max_source_positions"); a longer window is split, and its text then shared
/// by voiced seconds. `P.eng.alignWindowWholeSeconds` (100); `params::prepare()` rows it from this constant.
pub const WINDOW_WHOLE: f64 = 100.0;
/// S3: below this a piece cannot be made smaller, for either reason (go/align.go:660 `alignChunkMin`).
pub const MIN_PIECE: f64 = 15.0;
/// S2: how far a cut between align windows may slide to land in a silence (go/align.go:670 `alignCutSeek`). Far less than F1.4's
/// [`asr::SEEK_MAX`]: an alignment cut moves the whole window words are timed against, so the nominal edge stands unless a silence is close.
/// Small enough to keep sliding when the window halves down to [`MIN_PIECE`]. `P.eng.alignCutSeekSeconds` (4); `params::prepare()` rows it from here.
pub const CUT_SEEK: f64 = 4.0;
/// S3: quiet kept either side of a window's speech, since a word fades out below the silence threshold (go/align.go:674 `alignSoundPad`).
/// `P.eng.alignSoundPadSeconds` (0.25); `params::prepare()` rows it from this constant.
pub const PAD: f64 = 0.25;
/// S6: bare talking worth saying out loud, in seconds and as a share (go/align.go:679-680): ten bare seconds is rounding in a four-hour session,
/// the whole clip in a twenty-second one. `P.eng.alignBareWarnSeconds` (10); `params::prepare()` rows it from this constant.
pub const BARE_WARN: f64 = 10.0;
/// S6: the relative companion to [`BARE_WARN`]. `P.eng.alignBareShare` (0.08); `params::prepare()` rows it from this constant.
pub const BARE_SHARE: f64 = 0.08;
/// S4: sanity ceiling past the longest clip sent ([`WINDOW`]): a larger "second" is samples (go/align.go:230 `alignClipMax`).
/// `P.eng.alignClipMaxSeconds` (120); `params::prepare()` rows it from this constant.
pub const PLAIN_AS_SAMPLES: f64 = 120.0;

/// S1: the aligners to try, best first: a catalog entry is a claim, not a working model, so this is a walk-on-failure list.
/// [`crate::services::pick_aligner`] is the settings dialog's single-choice view of the same catalog.
///
/// A name in the box is the whole list when the server declares it for `align`: a box exists to be obeyed, so the refusal below is not replaced by another model.
pub fn aligners(configured: &str, url: &str, models: &[AudioModel]) -> Result<Vec<String>, String> {
    let wanted = configured.trim();
    let declared = |id: &str| models.iter().any(|m| m.id == id && m.task == "align");
    if !wanted.is_empty() {
        return match declared(wanted) {
            true => Ok(vec![wanted.to_string()]),
            false => Err(not_served(url, wanted)),
        };
    }
    let mut ids: Vec<String> = models
        .iter()
        .filter(|m| m.task == "align")
        .map(|m| m.id.clone())
        .collect();
    // The preference first, then by name: plain sorting puts the weaker one first (go/align.go:81-98).
    ids.sort_by_key(|id| (id != ALIGN_PREFERENCE, id.clone()));
    Ok(ids)
}

/// S1: what to say when the configured id is not served for alignment; cut points come off the waveform, so a log line, not a failure.
pub fn not_served(url: &str, id: &str) -> String {
    format!(
        "!!! align: {url} does not serve {id:?} for alignment -- cut points come off the waveform"
    )
}

/// S1: what to say when an aligner further down the list answered; the first to answer serves the rest of the run, so logged once.
pub fn picked_log(chosen: &str, failed: &[String]) -> String {
    format!(
        ">>> align: {chosen} answers where {} does not -- using it for the rest of the run",
        failed.join(", ")
    )
}

/// S1: which aligner answered earlier in this run; `None` means every model is still worth trying.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Picker {
    pub chosen: Option<String>,
}

impl Picker {
    /// The models to ask for this window: the one that answered, or the whole list.
    pub fn ask(&self, all: &[String]) -> Vec<String> {
        to_ask(&self.chosen, all)
    }

    /// Remember which one answered. `Some(log)` when it is not the first tried — that is worth knowing.
    pub fn chose(&mut self, model: &str, tried: &[String]) -> Option<String> {
        let index = tried.iter().position(|m| m == model).unwrap_or(0);
        self.chosen = Some(model.to_string());
        match index {
            0 => None,
            _ => Some(picked_log(model, &tried[..index])),
        }
    }
}

/// S2: whether the ASR's chunks can be reused as this flow's pieces: every chunk holds as many word times as its text has words.
///
/// Where the counts disagree (a timing-less ASR), `S2`'s voiced-seconds share is an estimate; where they agree the split is kept.
pub fn counts_agree(chunks: &[Chunk], words: &[Word]) -> bool {
    if chunks.is_empty() {
        return false;
    }
    let count = |text: &str| text.split_whitespace().count();
    // Totals first: a chunk whose times ran past the recording's end would make the last piece look short.
    let total = chunks
        .iter()
        .map(|chunk| count(&chunk.text) as u64)
        .sum::<u64>();
    total == words.len() as u64
        && chunks.iter().all(|chunk| {
            let inside = words
                .iter()
                // The last chunk owns the recording's end: a word said exactly at `duration` is in it.
                .filter(|word| {
                    let at = crate::transcribe::seconds(word.start_sample);
                    at >= chunk.s && (at < chunk.e || chunk.e == last_edge(chunks))
                })
                .count() as u64;
            count(&chunk.text) as u64 == inside
        })
}

fn last_edge(chunks: &[Chunk]) -> f64 {
    chunks.last().map(|chunk| chunk.e).unwrap_or_default()
}

/// S2: the pieces to ask about: the ASR's chunks when word counts agree, else windows of [`WINDOW`] cut at silences by [`asr::cut_points`].
pub fn pieces(
    chunks: &[Chunk],
    words: &[Word],
    duration: f64,
    silences: &[Silence],
    window: f64,
) -> Vec<(f64, f64)> {
    if counts_agree(chunks, words) {
        return chunks.iter().map(|chunk| (chunk.s, chunk.e)).collect();
    }
    let edges = asr::cut_points(duration, silences, window, CUT_SEEK);
    edges.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

/// S2: the ASR's chunks as the pieces, each with the text the ASR heard in it, when their texts joined are the
/// transcript word for word. The text is then known per piece, not estimated: a voiced-seconds share hands a piece
/// words said in its neighbour, and the aligner crams them into the piece's last moments at one frame (80 ms) each.
/// [`counts_agree`] never held in practice: qwen3-asr writes no word times unless its server was started with a forced
/// aligner, and nemotron-asr times sub-word pieces ("w", "ie"), never one per written word; so every recording took
/// the estimate (first rewrite; 84 of the 824 recordings in /mnt/rec carry such chains).
pub fn own_pieces(chunks: &[Chunk], transcript: &str) -> Option<Vec<((f64, f64), String)>> {
    if chunks.is_empty() {
        return None;
    }
    let joined: Vec<&str> = chunks.iter().flat_map(|c| c.text.split_whitespace()).collect();
    let said: Vec<&str> = transcript.split_whitespace().collect();
    if joined != said || chunks.windows(2).any(|pair| pair[1].s < pair[0].e - 1e-6) {
        return None;
    }
    Some(
        chunks
            .iter()
            .map(|c| ((c.s, c.e), c.text.split_whitespace().collect::<Vec<_>>().join(" ")))
            .collect(),
    )
}

/// S2: the text each piece was asked to align, shared by voiced seconds (a silence holds no words), rounded to word boundaries
/// so the pieces together are the whole transcript with nothing lost or duplicated.
pub fn share_text(pieces: &[(f64, f64)], text: &str, silences: &[Silence]) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if pieces.is_empty() || words.is_empty() {
        return pieces.iter().map(|_| String::new()).collect();
    }
    let voiced: Vec<f64> = pieces
        .iter()
        .map(|(from, to)| voiced(silences, *from, *to))
        .collect();
    let total: f64 = voiced.iter().sum();
    // Nothing has sound: fall back to wall-clock length rather than divide by zero.
    let weights: Vec<f64> = if total > 0.0 {
        voiced
    } else {
        pieces
            .iter()
            .map(|(from, to)| (to - from).max(0.0))
            .collect()
    };
    let weight_total: f64 = weights.iter().sum::<f64>().max(1.0);

    let mut out = Vec::with_capacity(pieces.len());
    let mut at = 0usize;
    for (index, weight) in weights.iter().enumerate() {
        // The last piece takes the remainder: separate rounding would strand words off the end.
        let take = if index + 1 == weights.len() {
            words.len() - at
        } else {
            ((words.len() as f64 * weight / weight_total).floor() as usize).min(words.len() - at)
        };
        out.push(words[at..at + take].join(" "));
        at += take;
    }
    out
}

/// S3: how much of `t0..t1` has sound in it — the measure everything here is shared out by.
pub fn voiced(silences: &[Silence], t0: f64, t1: f64) -> f64 {
    let mut at = t0;
    let mut total = 0.0;
    for silence in silences {
        if silence.end <= at || silence.start >= t1 {
            continue;
        }
        if silence.start > at {
            total += silence.start - at;
        }
        at = at.max(silence.end);
        if at >= t1 {
            return total;
        }
    }
    if t1 > at {
        total += t1 - at;
    }
    total
}

/// S3: the part of `t0..t1` that has sound in it, padded, or `None` when all of it is quiet.
///
/// Built by subtracting silences, not by testing whether one reaches the window's edge: silencedetect measures a file's end a hair short (go/align.go:622).
pub fn trim(span: (f64, f64), silences: &[Silence]) -> Option<(f64, f64)> {
    let (t0, t1) = span;
    let mut parts: Vec<(f64, f64)> = Vec::new();
    let mut at = t0;
    for silence in silences {
        if silence.end <= t0 || silence.start >= t1 {
            continue;
        }
        // The stretch before this silence is sound however short (silencedetect leaves a hair of quiet at a file's end).
        if silence.start > at {
            parts.push((at, silence.start.min(t1)));
        }
        at = at.max(silence.end);
    }
    if at < t1 {
        parts.push((at, t1));
    }
    let (first, last) = (parts.first()?.0, parts.last()?.1);
    Some(((first - PAD).max(t0), (last + PAD).min(t1)))
}

/// S3: one window's seconds and the text to align over them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    pub from: f64,
    pub to: f64,
    pub text: String,
}

/// S3: cut a stretch at the quietest moment nearest its middle and divide its words by how much
/// talking falls each side of the cut.
///
/// Candidates stay inside the middle three quarters: a cut near an edge would make no progress (go/align.go:543).
pub fn split_at(span: &Span, silences: &[Silence]) -> (Span, Span) {
    let (mid, quarter) = ((span.from + span.to) / 2.0, (span.to - span.from) / 4.0);
    let mut cut = mid;
    let mut best = f64::INFINITY;
    for silence in silences {
        let at = (silence.start + silence.end) / 2.0;
        if at <= span.from || at >= span.to || (at - mid).abs() > quarter {
            continue;
        }
        // "Quietest" is the longest silence, and a tie goes to the one nearer the middle.
        let score = -(silence.end - silence.start) + (at - mid).abs() * 1e-6;
        if score < best {
            best = score;
            cut = at;
        }
    }
    let left_voiced = voiced(silences, span.from, cut);
    let right_voiced = voiced(silences, cut, span.to);
    let mut words: Vec<&str> = span.text.split_whitespace().collect();
    // No word to divide is still two pieces of audio; the text goes with neither.
    let take = if words.is_empty() {
        0
    } else {
        let share = left_voiced / (left_voiced + right_voiced).max(f64::EPSILON);
        match words.len() {
            // One word cannot be divided: it goes with the half that has more sound (first rewrite: `clamp(1, 0)`
            // panicked on a long window holding a single word).
            1 => usize::from(share >= 0.5),
            n => ((n as f64 * share).round() as usize).clamp(1, n - 1),
        }
    };
    let right = words.split_off(take);
    (
        Span {
            from: span.from,
            to: cut,
            text: words.join(" "),
        },
        Span {
            from: cut,
            to: span.to,
            text: right.join(" "),
        },
    )
}

/// S3: the window after one halving. `None` in the log means "as small as it goes", which ends the retry loop (same shape as [`asr::halve`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Halved {
    pub window: f64,
    pub log: Option<String>,
}

/// S3: halve the window, never under [`MIN_PIECE`].
pub fn halve(base: &str, window: f64) -> Halved {
    if window <= MIN_PIECE {
        return Halved { window, log: None };
    }
    let next = (window / 2.0).floor().max(MIN_PIECE);
    Halved {
        window: next,
        log: Some(format!(
            "!!! [{base}] align: no room for that much audio -- trying {next:.0} s at a time"
        )),
    }
}

/// S4: what the aligner is asked with: clip file name, text and language, as the ASR gets.
pub fn body(clip: &str, text: &str, language: &str) -> Value {
    json!({ "audio": clip, "text": text, "language": language })
}

/// S4: ffmpeg's arguments for one window, `-ss` before `-i` as in [`asr::cut_plan`]; on this stage's pcm the seek is exact.
pub fn clip_plan(from: f64, to: f64) -> Vec<String> {
    vec![
        "-ss".to_string(),
        trim_seconds(from),
        "-t".to_string(),
        trim_seconds(to - from),
    ]
}

/// Seconds without a tail of zeroes: `213`, not `213.000`.
fn trim_seconds(secs: f64) -> String {
    if (secs - secs.round()).abs() < 1e-9 {
        format!("{}", secs.round() as i64)
    } else {
        format!("{secs:.3}")
    }
}

/// S3/S4: the scratch's file name for one window, its start in milliseconds (a retry re-asks a window).
pub fn clip(from: f64) -> String {
    format!("w{:09}.wav", (from * 1000.0).round() as i64)
}

/// S4: the words out of an answer, however the server spelled it.
///
/// The first non-empty list of `words | alignment | segments | result.words | result.alignment` wins; each time is read as samples, then ms,
/// then seconds, then a bare number that means samples past [`PLAIN_AS_SAMPLES`]. Words are stored lower-cased (later comparisons are case-blind);
/// [`restore`] brings the written form back. No words at all is an error naming what came back.
pub fn parse_words(body: &Value) -> Result<Vec<Word>, String> {
    let lists = ["words", "alignment", "segments"]
        .iter()
        .map(|key| body.get(*key))
        .chain(
            ["words", "alignment"]
                .iter()
                .map(|key| body.pointer(&format!("/result/{key}"))),
        );
    for list in lists.flatten() {
        let entries = match list.as_array() {
            Some(entries) if !entries.is_empty() => entries,
            _ => continue,
        };
        let mut out = Vec::new();
        for entry in entries {
            let text = ["word", "text", "label"]
                .iter()
                .filter_map(|key| entry.get(*key).and_then(Value::as_str))
                .find(|s| !s.trim().is_empty())
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            if text.is_empty() {
                continue;
            }
            let (Some(start), Some(end)) = (time(entry, "start"), time(entry, "end")) else {
                continue;
            };
            out.push(Word {
                word: text,
                start_sample: to_sample(start),
                end_sample: to_sample(end),
            });
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    Err("no words in the answer".to_string())
}

/// S4: one time (`kind` is "start" or "end"), read the four ways a server writes it; the bare key goes last because its unit is a guess.
fn time(entry: &Value, kind: &str) -> Option<f64> {
    let get = |suffix: &str| {
        entry
            .get(format!("{kind}_{suffix}"))
            .and_then(Value::as_f64)
    };
    if let Some(secs) = get("sample").map(|s| s / SAMPLE_RATE as f64) {
        return Some(secs);
    }
    if let Some(secs) = get("ms").map(|ms| ms / 1000.0) {
        return Some(secs);
    }
    if let Some(secs) = get("time") {
        return Some(secs);
    }
    let plain = entry.get(kind).and_then(Value::as_f64)?;
    match plain > PLAIN_AS_SAMPLES {
        true => Some(plain / SAMPLE_RATE as f64),
        false => Some(plain),
    }
}

fn to_sample(secs: f64) -> u64 {
    (secs * SAMPLE_RATE as f64).round().max(0.0) as u64
}

/// S4: what came back that nobody expected, for the log: first 300 characters, whitespace collapsed.
pub fn head(body: &str) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.len() > 300 {
        true => format!("{}...", &flat[..300]),
        false => flat,
    }
}

/// S4: an answer with no words in it, said out loud with the shape of what did come back.
pub fn unreadable_log(err: &str, body: &str) -> String {
    format!("!!! align: {err} -- the answer began: {}", head(body))
}

/// S4: put the transcript's case and punctuation back on words the aligner stored lower-cased.
///
/// Times never change, only the written form. [F1.13]'s session word list will replace this lookup; a word with no counterpart keeps what the aligner heard.
pub fn restore(words: &[Word], transcript: &str) -> Vec<Word> {
    let written: Vec<&str> = transcript.split_whitespace().collect();
    let matches = |written: &str, spoken: &str| {
        written
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
            == spoken
    };
    let mut out = Vec::with_capacity(words.len());
    for (index, word) in words.iter().enumerate() {
        // By position where counts allow: keeps a repeated word's punctuation on the right occurrence.
        let dressed: Option<&str> = written
            .get(index)
            .copied()
            .filter(|written| matches(written, &word.word))
            .or_else(|| written.iter().copied().find(|w| matches(w, &word.word)));
        out.push(Word {
            word: dressed.unwrap_or(&word.word).to_string(),
            start_sample: word.start_sample,
            end_sample: word.end_sample,
        });
    }
    out
}

/// S6: how many of the recording's talking seconds no word covers; words count as a hair more than they say ([`PAD`]).
pub fn bare_voiced(words: &[Word], duration: f64, silences: &[Silence]) -> f64 {
    let mut covered: Vec<(f64, f64)> = words
        .iter()
        .map(|word| {
            (
                crate::transcribe::seconds(word.start_sample) - PAD,
                crate::transcribe::seconds(word.end_sample) + PAD,
            )
        })
        .collect();
    covered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut bare = 0.0;
    // Before the first word and after the last, so an aligner that placed everything in one second does not measure clean.
    let mut at = 0.0;
    for (start, end) in covered {
        if start > at {
            bare += voiced(silences, at, start.min(duration));
        }
        at = at.max(end);
    }
    if at < duration {
        bare += voiced(silences, at, duration);
    }
    bare
}

/// S6: both tests, seconds and share (see [`BARE_WARN`]).
pub fn bare_warns(bare: f64, spoken: f64) -> bool {
    spoken > 0.0 && bare >= BARE_WARN && bare >= BARE_SHARE * spoken
}

/// S6: this failure is silent by nature: words in the wrong second look fine, and voiced audio with no word is deleted by the cut. So it is said out loud.
pub fn bare_warning(bare: f64, spoken: f64) -> String {
    format!(
        "!!! align: {bare:.0} s of the {spoken:.0} s spoken has no word over it -- the times are wrong, and the cut will drop that footage"
    )
}

/// S1: an aligner that refused this window, named.
pub fn refused_log(model: &str, err: &str) -> String {
    format!("!!! align: {model}: {err}")
}

/// S1: the aligners to ask for one window: the one that already answered, or the whole list. Public as the caller's view of [`Picker`]'s rule.
pub fn to_ask(chosen: &Option<String>, all: &[String]) -> Vec<String> {
    match chosen {
        Some(chosen) => vec![chosen.clone()],
        None => all.to_vec(),
    }
}

/// One window aligned: its seconds in the recording and the words placed inside them.
#[derive(Debug, Clone, Default)]
pub struct Placed {
    pub from: f64,
    pub to: f64,
    pub words: Vec<Word>,
}

/// What one pass produced. `skipped` is S1's "alignment skipped for the run": cut points come off the waveform.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub logs: Vec<String>,
    /// What the progress bar was told (F0.5 owns the bar).
    pub steps: Vec<String>,
    /// Every word, in order, with times in the recording's own seconds.
    pub words: Vec<Word>,
    pub skipped: bool,
    /// S1: the aligner that served the run.
    pub model: String,
}

/// S1-S6 for one source.
///
/// `ask` is handed the model, the clip's file name and the text, and answers with the server's body; once per window per try, in order.
///
/// `aligners` is [`aligners`]' answer: empty is S1's skip, not a failure, and the caller logs it.
pub fn run<F>(
    tree: &Tree,
    source: &str,
    duration: f64,
    language: &str,
    silences: &[Silence],
    chunks: &[Chunk],
    words: &[Word],
    transcript: &str,
    aligners: &[String],
    mut ask: F,
) -> Result<Outcome, String>
where
    F: FnMut(&str, &str, &str) -> Result<Value, String>,
{
    let mut out = Outcome::default();
    // S1: nothing served for alignment is not a failure — the waveform places the cut points.
    if aligners.is_empty() {
        out.skipped = true;
        return Ok(out);
    }

    // S2: the ASR's own chunks with the text it heard in each, whenever those texts make up the transcript; else
    // windows cut at silences with the text shared out by voiced seconds, an estimate.
    let (spans, texts) = match own_pieces(chunks, transcript) {
        Some(own) => own.into_iter().unzip(),
        None => {
            let spans = pieces(chunks, words, duration, silences, WINDOW);
            let texts = share_text(&spans, transcript, silences);
            (spans, texts)
        }
    };
    let mut picker = Picker::default();
    let mut placed: Vec<Placed> = Vec::new();

    for (index, (from, to)) in spans.iter().enumerate() {
        out.steps.push(progress(index, spans.len()));
        let text = texts.get(index).cloned().unwrap_or_default();
        // A piece the ASR heard nothing in has no word to place: no server is asked about it.
        if text.trim().is_empty() {
            continue;
        }
        let windows = align_windows(
            tree,
            &mut picker,
            aligners,
            source,
            *from,
            *to,
            &text,
            language,
            silences,
            &mut ask,
            &mut out,
        )?;
        placed.extend(windows);
    }

    // S5: written whole, once, after the last window succeeded: a half-written `words.aligned.json` would read as "the times are the aligner's" (F1.3).
    let all: Vec<Word> = placed
        .iter()
        .flat_map(|piece| piece.words.clone())
        .collect();
    requests::write_aligned(tree, source, &requests::AlignedDoc { words: all.clone() })?;
    // §5 bullet 1: the scratch is gone once its answer is on disk; a failed window returns earlier, so `align/wNNNN.wav.plan` survives the failure it explains (as `asr/`).
    let _ = std::fs::remove_dir_all(tree.align_scratch(source));

    out.model = picker.chosen.clone().unwrap_or_default();
    // S4: the written form comes back from the transcript; the times are the aligner's.
    out.words = restore(&all, transcript);
    // S6: only worth measuring when something was placed.
    let spoken = voiced(silences, 0.0, duration);
    if !all.is_empty() {
        let bare = bare_voiced(&all, duration, silences);
        if bare_warns(bare, spoken) {
            out.logs.push(bare_warning(bare, spoken));
        }
    }
    Ok(out)
}

/// S1: what the progress bar says while a window is up there.
pub fn progress(index: usize, total: usize) -> String {
    format!("aligning speech {}/{}", index + 1, total)
}

/// S3: one window asked about, halved and split until an answer came back.
///
/// The order is the spec's: trim to sound, split while over the limit and longer than [`MIN_PIECE`], and on an out-of-memory answer halve the limit,
/// never under `MIN_PIECE`, where the piece goes up as it stands or the refusal reaches the caller.
fn align_windows<F>(
    tree: &Tree,
    picker: &mut Picker,
    all: &[String],
    source: &str,
    from: f64,
    to: f64,
    text: &str,
    language: &str,
    silences: &[Silence],
    ask: &mut F,
    out: &mut Outcome,
) -> Result<Vec<Placed>, String>
where
    F: FnMut(&str, &str, &str) -> Result<Value, String>,
{
    let mut queue = vec![Span {
        from,
        to,
        text: text.to_string(),
    }];
    let mut limit = WINDOW_WHOLE;
    let mut done = Vec::new();

    while let Some(span) = queue.pop() {
        let window = match trim((span.from, span.to), silences) {
            Some(window) => window,
            // Nothing in it has sound: nothing to place, and no server asked. Words a split shared out to it were
            // said in the next piece, so they go there; a chunk with no sound at all holds only what the ASR made up.
            None => {
                carry(&mut queue, &span.text);
                continue;
            }
        };
        if window.1 - window.0 > limit && window.1 - window.0 > MIN_PIECE {
            let (left, right) = split_at(&span, silences);
            queue.push(right);
            queue.push(left);
            continue;
        }
        // S3: the clip is the trimmed window, so its seconds are what a smaller retry is compared on.
        let asked = window.1 - window.0;
        match ask_span(
            tree, picker, all, source, &window, &span.text, language, ask, out,
        ) {
            Ok(mut words) => {
                // S3: words timed from past the clip's end were not heard in it. After a split that is the share
                // being off, and they are said in the next piece: their text goes to the front of it (first rewrite:
                // kept with made-up times, out of order; 07-webarch 20-16-01 "andere sprachen haben ihre mvc ...").
                let end = to_sample(window.1);
                let tokens: Vec<&str> = span.text.split_whitespace().collect();
                let over = words.iter().rev().take_while(|w| w.start_sample >= end).count();
                if over > 0 && !queue.is_empty() && words.len() == tokens.len() {
                    carry(&mut queue, &tokens[tokens.len() - over..].join(" "));
                    words.truncate(words.len() - over);
                }
                done.push(Placed {
                    from: span.from,
                    to: span.to,
                    words: within_clip(words, end),
                })
            }
            Err(err) if asr::retryable(&err) || too_long(&err) => {
                // The ceiling is what a *clip* may be: a piece is only too big while the audio actually sent exceeds it.
                if asked > limit || asked <= MIN_PIECE {
                    return Err(err);
                }
                let Halved { window: next, log } = halve(source, limit);
                match log {
                    Some(log) => {
                        out.logs.push(log);
                        limit = next;
                        // Re-ask this span smaller: pushing it back keeps the order and its shared words.
                        queue.push(span);
                    }
                    None => return Err(err),
                }
            }
            Err(err) => return Err(err),
        }
    }
    done.sort_by(|a, b| {
        a.from
            .partial_cmp(&b.from)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(done)
}

/// S1/S4: ask the aligners about one window until one answers; the first that answers serves the rest of the run.
fn ask_span<F>(
    tree: &Tree,
    picker: &mut Picker,
    all: &[String],
    source: &str,
    window: &(f64, f64),
    text: &str,
    language: &str,
    ask: &mut F,
    out: &mut Outcome,
) -> Result<Vec<Word>, String>
where
    F: FnMut(&str, &str, &str) -> Result<Value, String>,
{
    let clip = clip(window.0);
    let plan = clip_plan(window.0, window.1);
    // S4: digits have no sound the aligner knows; it is asked with them said as words, and the answer folded back.
    let (spoken, tokens) = crate::spoken_numbers::spell(text, language);
    let text = spoken.as_str();
    let mut last = String::from("no aligner served");
    let candidates = picker.ask(all);
    for model in &candidates {
        // S3/S4: the scratch is kept when a window fails, so what was asked is recoverable (as in `asr`).
        make_scratch(tree, source, &clip, &plan, &body(&clip, text, language))?;
        let mut answer = ask(model, &clip, text);
        // A server that fell over on this window is asked once more before the window counts as failed: the
        // same request answers on a second try (05-tokens 16-56-25, "negative timestamp id", 1 window in 824
        // recordings, fine on every re-ask).
        if let Err(err) = &answer {
            if transient(err) {
                out.logs.push(transient_log(source, model, err));
                answer = ask(model, &clip, text);
            }
        }
        match answer {
            Ok(answer) => {
                let words = match parse_words(&answer) {
                    Ok(words) => words,
                    Err(err) => {
                        // The shape is unknown until a server answers, so what came back is written down.
                        out.logs.push(unreadable_log(&err, &answer.to_string()));
                        last = err;
                        continue;
                    }
                };
                // S4: the answer timed the clip, so its seconds belong to the recording from here.
                let offset = to_sample(window.0);
                let words = crate::spoken_numbers::merge(words, &tokens)
                    .into_iter()
                    .map(|word| Word {
                        word: word.word,
                        start_sample: word.start_sample + offset,
                        end_sample: word.end_sample + offset,
                    })
                    .collect();
                if let Some(log) = picker.chose(model, &candidates) {
                    out.logs.push(log);
                }
                return Ok(words);
            }
            Err(err) => {
                out.logs.push(refused_log(model, &err));
                last = err;
            }
        }
    }
    Err(last)
}

/// S3: words carried to the next piece of a split, at the front of its text.
fn carry(queue: &mut [Span], text: &str) {
    if text.trim().is_empty() {
        return;
    }
    if let Some(next) = queue.last_mut() {
        next.text = format!("{text} {}", next.text).trim().to_string();
    }
}

/// S4: what is left past the clip's end, once nothing more can be carried, is cut to it and kept: the aligner heard
/// no sound for it (the ASR's stock phrases over silence, or speech too quiet to count as sound), so it cannot say
/// when it was said, and no word of the transcript is lost for that (first rewrite: such words sat past their
/// recording's end, 20 of them in the lecture projects).
pub fn within_clip(words: Vec<Word>, end_sample: u64) -> Vec<Word> {
    words
        .into_iter()
        .map(|word| Word {
            start_sample: word.start_sample.min(end_sample),
            end_sample: word.end_sample.min(end_sample),
            ..word
        })
        .collect()
}

/// S4: an answer worth one more ask: the server erred (HTTP 500) on its own, not for want of memory ([`asr::retryable`]
/// has its own way down) and not because it serves no such model.
pub fn transient(err: &str) -> bool {
    !asr::retryable(err) && !too_long(err) && (err.contains("answered 500") || err.contains("server_error"))
}

/// S3: the aligner's own word for a window longer than its model takes; answered like no room, by halving.
pub fn too_long(err: &str) -> bool {
    err.contains("max_source_positions")
}

/// S4: the log line for the second ask.
pub fn transient_log(source: &str, model: &str, err: &str) -> String {
    format!("!!! [{source}] align: {model} failed on a window ({}) -- asking once more", head(err))
}

/// Write one window's plan into `align/` beside the source's other files.
fn make_scratch(
    tree: &Tree,
    source: &str,
    file: &str,
    plan: &[String],
    body: &Value,
) -> Result<(), String> {
    let dir = tree.align_scratch(source);
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let text = format!("{}\n{}\n", plan.join(" "), body);
    std::fs::write(dir.join(format!("{file}.plan")), text)
        .map_err(|err| format!("{}: {err}", dir.join(file).display()))
}
