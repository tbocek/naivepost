//! Forced alignment (spec/04-prepare.md F1.5): which second each word was actually said in.
//!
//! The ASR's own stamps run half a second to a second behind the sound, which is fine for reading a
//! transcript and useless for cutting on a word edge. An aligner fixes that, and this module decides
//! which one, what each request covers, and what `words.aligned.json` ends up holding. As in
//! [`crate::asr`], no process is spawned and no socket opened: the silences arrive as arguments, every
//! cut is returned as ffmpeg's arguments for the caller to run, and each answer arrives through
//! [`run`]'s closure — a step that decided inside a server call could never be tested (spec/00-principles.md §5).

use serde_json::{json, Value};

use crate::asr::{self, Silence};
use crate::layout::Tree;
use crate::requests::{self, Chunk, Word};
use crate::services::{AudioModel, ALIGN_PREFERENCE};
use crate::transcribe::SAMPLE_RATE;

/// S2: how much audio goes into one align request — the same number the ASR chunks by
/// ([`asr::CHUNK_QWEN`]), so the pieces line up and each carries its own words (gui/align.go:656).
pub const WINDOW: f64 = asr::CHUNK_QWEN;
/// S3: below this a piece cannot be made smaller, for either reason (gui/align.go:660 `alignChunkMin`).
pub const MIN_PIECE: f64 = 15.0;
/// S3: how much of the quiet either side of a window's speech goes with it — a word fades out below
/// the silence threshold before it is over (gui/align.go:674 `alignSoundPad`).
pub const PAD: f64 = 0.25;
/// S6: bare talking worth saying out loud, in seconds and as a share of the talking. Both, because ten
/// bare seconds of a four-hour session is rounding and ten bare seconds of a twenty-second clip is the
/// whole clip (gui/align.go:679-680).
pub const BARE_WARN: f64 = 10.0;
pub const BARE_SHARE: f64 = 0.08;
/// S4: comfortably past the longest clip this sends ([`WINDOW`]) — a "second" beyond it did not mean
/// seconds, it meant samples that forgot to say so (gui/align.go:230 `alignClipMax`).
pub const PLAIN_AS_SAMPLES: f64 = 120.0;

/// S1: the aligners to try, best first — a list and not a choice, because a catalog entry is a claim
/// rather than a working model: a server lists an aligner whose family its engine was not built with
/// and answers every request for it with an error. [`crate::services::pick_aligner`] is the settings
/// dialog's single-choice view of the same catalog; this flow needs the whole walk-on-failure list.
///
/// A name in the box is the whole list when the server declares it for `align`: a box exists to be
/// obeyed, and quietly using another model is worse than doing nothing — so the refusal below is not
/// replaced by one of the models that *is* offered.
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
    // The preference first, the rest by name: two aligners sorted plainly put the weaker one first
    // because its name is shorter (gui/align.go:81-98).
    ids.sort_by_key(|id| (id != ALIGN_PREFERENCE, id.clone()));
    Ok(ids)
}

/// S1: what to say when the configured id is not served for alignment. Cut points come off the
/// waveform either way, so this is a log line and not a failure.
pub fn not_served(url: &str, id: &str) -> String {
    format!("!!! align: {url} does not serve {id:?} for alignment -- cut points come off the waveform")
}

/// S1: what to say when an aligner further down the list answered: the first that answers serves the
/// rest of the run, so this is logged once and never again.
pub fn picked_log(chosen: &str, failed: &[String]) -> String {
    format!(
        ">>> align: {chosen} answers where {} does not -- using it for the rest of the run",
        failed.join(", ")
    )
}

/// S1: which aligner answered earlier in this run. `None` means nothing has answered yet, so every
/// model is still worth trying; once one has, the others are never asked again.
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

/// S2: whether the ASR's chunks can be reused as this flow's pieces — every chunk holds exactly as
/// many word times as its text has words.
///
/// The estimate belongs here and nowhere else: a timing-less ASR has no times to select by, so where
/// the counts disagree [S2]'s voiced-seconds share is the only way to divide the text, and it is an
/// estimate. Where they agree the split is kept rather than re-derived — same seconds, same text.
pub fn counts_agree(chunks: &[Chunk], words: &[Word]) -> bool {
    if chunks.is_empty() {
        return false;
    }
    let count = |text: &str| text.split_whitespace().count();
    // The totals first: a chunk whose times ran past the end of the recording would otherwise make
    // the last piece look short and every earlier one right.
    let total = chunks.iter().map(|chunk| count(&chunk.text) as u64).sum::<u64>();
    total == words.len() as u64 && chunks.iter().all(|chunk| {
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

/// The recording's end, which is the last chunk's — the pieces tile the recording with no gap.
fn last_edge(chunks: &[Chunk]) -> f64 {
    chunks.last().map(|chunk| chunk.e).unwrap_or_default()
}

/// S2: the pieces to ask about. The ASR's own chunks when their word counts agree; otherwise windows
/// of [`WINDOW`] cut at silences, which is [`asr::cut_points`] doing what it already does for F1.4 —
/// even pieces, each inside the ceiling, cuts moved to a silence midpoint rather than into a word.
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
    let edges = asr::cut_points(duration, silences, window, asr::SEEK_MAX);
    edges
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

/// S2: the text each piece was asked to align, its share of the transcript by how much of it has
/// sound in it — a stretch of silence holds no words however long it is. Proportional shares are
/// rounded to word boundaries so the pieces together are the whole transcript with nothing lost or
/// duplicated, which is what makes this an estimate that still cannot eat text.
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
    // Nothing has sound in it: fall back to the wall-clock length rather than dividing by zero. A
    // silent recording gets an empty transcript anyway, so what it is asked with does not matter.
    let weights: Vec<f64> = if total > 0.0 {
        voiced
    } else {
        pieces.iter().map(|(from, to)| (to - from).max(0.0)).collect()
    };
    let weight_total: f64 = weights.iter().sum::<f64>().max(1.0);

    let mut out = Vec::with_capacity(pieces.len());
    let mut at = 0usize;
    for (index, weight) in weights.iter().enumerate() {
        // The last piece takes everything left over: rounding each share separately would otherwise
        // strand a word or two off the end of the recording.
        let take = if index + 1 == weights.len() {
            words.len() - at
        } else {
            ((words.len() as f64 * weight / weight_total).floor() as usize)
                .min(words.len() - at)
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
/// Built by subtracting the silences rather than by asking whether one reaches the window's edge,
/// because it does not quite — silencedetect measures a file's end a hair short (gui/align.go:622).
pub fn trim(span: (f64, f64), silences: &[Silence]) -> Option<(f64, f64)> {
    let (t0, t1) = span;
    // The sounding stretches inside the window, in order.
    let mut parts: Vec<(f64, f64)> = Vec::new();
    let mut at = t0;
    for silence in silences {
        if silence.end <= t0 || silence.start >= t1 {
            continue;
        }
        // The stretch before this silence is sound, however short — a hair of quiet left at the end
        // of a file by silencedetect still counts as sounding.
        if silence.start > at {
            parts.push((at, silence.start.min(t1)));
        }
        at = at.max(silence.end);
    }
    if at < t1 {
        parts.push((at, t1));
    }
    let (first, last) = (parts.first()?.0, parts.last()?.1);
    Some((
        (first - PAD).max(t0),
        (last + PAD).min(t1),
    ))
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
/// Candidates are kept inside the middle three quarters: a cut near an edge makes one half no smaller
/// than the whole, which is a recursion that does not end (gui/align.go:543).
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
        ((words.len() as f64 * share).round() as usize).clamp(1, words.len() - 1)
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

/// S3: the window after one halving, and what to say about it. `None` in the log means "this was as
/// small as it goes", which is what ends the retry loop rather than leaving it to spin — the same
/// shape [`asr::halve`] has, so both out-of-memory loops read alike.
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

/// S4: what the aligner is asked with. The clip's file name, the text to place over it, and the
/// language — the same three the ASR gets, so one upload helper serves both.
pub fn body(clip: &str, text: &str, language: &str) -> Value {
    json!({ "audio": clip, "text": text, "language": language })
}

/// S4: ffmpeg's arguments for one window, `-ss` before the caller's own `-i` as in [`asr::cut_plan`] —
/// on the pcm this stage wrote that seek is exact rather than a keyframe away.
pub fn clip_plan(from: f64, to: f64) -> Vec<String> {
    vec![
        "-ss".to_string(),
        trim_seconds(from),
        "-t".to_string(),
        trim_seconds(to - from),
    ]
}

/// Seconds without a tail of zeroes: `213` rather than `213.000`, so the plan in the scratch reads as
/// the numbers the pieces were cut at.
fn trim_seconds(secs: f64) -> String {
    if (secs - secs.round()).abs() < 1e-9 {
        format!("{}", secs.round() as i64)
    } else {
        format!("{secs:.3}")
    }
}

/// S3/S4: the scratch's file name for one window, its start in milliseconds — the window is what a
/// retry re-asks, so the name has to say which one it was.
pub fn clip(from: f64) -> String {
    format!("w{:09}.wav", (from * 1000.0).round() as i64)
}

/// S4: the words out of an answer, however the server spelled it.
///
/// The first non-empty list of `words | alignment | segments | result.words | result.alignment` wins;
/// each time is read as samples, then milliseconds, then seconds, then a bare number that means
/// samples once it is past [`PLAIN_AS_SAMPLES`]. Words are stored lower-cased because every later
/// comparison (F1.3's segments, F1.13's word list) is case-blind; the written form comes back through
/// [`restore`]. A body with no words at all is an error naming what came back rather than a guess.
pub fn parse_words(body: &Value) -> Result<Vec<Word>, String> {
    let lists = ["words", "alignment", "segments"]
        .iter()
        .map(|key| body.get(*key))
        .chain(["words", "alignment"].iter().map(|key| body.pointer(&format!("/result/{key}"))));
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

/// S4: one time, read the four ways a server writes it. `kind` is "start" or "end"; the suffixed keys
/// are tried before the bare one, and the bare one last because its unit is a guess.
fn time(entry: &Value, kind: &str) -> Option<f64> {
    let get = |suffix: &str| entry.get(format!("{kind}_{suffix}")).and_then(Value::as_f64);
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

/// S4: what came back that nobody expected, for the log line — first 300 characters with the
/// whitespace collapsed, since the answer's shape is the one thing this cannot know until a server
/// answers.
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
/// Times never change — only the written form. This is a plain walk over the transcript's own word
/// list; [F1.13]'s session word list (retakes, `final.txt`, subtitles) is what gives it the spellings
/// worth restoring and will replace this lookup when its round comes. A word the transcript has no
/// counterpart for keeps what the aligner heard.
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
        // In order where the counts allow it; a lookup by position is what keeps a repeated word's
        // punctuation attached to the right occurrence.
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

/// S6: how many of the recording's talking seconds no word covers. Words count as covering a hair
/// more than they say ([`PAD`]), because a word's last moments are quieter than the threshold.
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
    // Before the first word and after the last, exactly as much of a recording is bare as the words
    // leave out — an aligner that placed everything in one second would otherwise measure clean.
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

/// S6: both tests, because ten bare seconds of a four-hour session is rounding and ten bare seconds of
/// a twenty-second clip is the whole clip.
pub fn bare_warns(bare: f64, spoken: f64) -> bool {
    spoken > 0.0 && bare >= BARE_WARN && bare >= BARE_SHARE * spoken
}

/// S6: this failure is silent by nature — words in the wrong second are still words, and the only
/// visible sign is voiced audio with no word over it, which the cut reads as footage where nothing was
/// said and deletes. So it gets said out loud.
pub fn bare_warning(bare: f64, spoken: f64) -> String {
    format!(
        "!!! align: {bare:.0} s of the {spoken:.0} s spoken has no word over it -- the times are wrong, and the cut will drop that footage"
    )
}

/// S1: an aligner that refused this window — named, because which one failed is what the next try is
/// about.
pub fn refused_log(model: &str, err: &str) -> String {
    format!("!!! align: {model}: {err}")
}

/// S1: the aligners to ask for one window — the one that already answered, or the whole list until
/// one does. Public because it is what the caller can see of [`Picker`]'s rule; `run` uses it directly.
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

/// What one pass produced. `skipped` is S1's "alignment skipped for the run": no aligner served, so
/// nothing was asked and cut points will come off the waveform.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub logs: Vec<String>,
    /// What the progress bar was told — F0.5 owns the bar, as [`crate::asr`] records its own for.
    pub steps: Vec<String>,
    /// Every word, in order, with times in the recording's own seconds.
    pub words: Vec<Word>,
    pub skipped: bool,
    /// S1: the aligner that served the run.
    pub model: String,
}

/// S1-S6 for one source.
///
/// `ask` is handed the model, the clip's file name and the text to place, and answers with the
/// server's body; it is called once per window per try, in order. The real ▶ hands over ffmpeg's cut,
/// the upload and the request; a test hands over a closure.
///
/// `aligners` is [`aligners`]' answer: empty means nothing is served for alignment, which is S1's skip
/// rather than a failure — so what to log about it is the caller's line, not a second one from here.
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

    // S2: the ASR's own pieces where their word counts agree, windows otherwise, and the text shared
    // out by voiced seconds for whichever of the two this is.
    let spans = pieces(chunks, words, duration, silences, WINDOW);
    let texts = share_text(&spans, transcript, silences);
    let mut picker = Picker::default();
    let mut placed: Vec<Placed> = Vec::new();

    for (index, (from, to)) in spans.iter().enumerate() {
        out.steps.push(progress(index, spans.len()));
        let text = texts.get(index).cloned().unwrap_or_default();
        let windows = align_windows(
            tree, &mut picker, aligners, source, *from, *to, &text, language, silences, &mut ask, &mut out,
        )?;
        placed.extend(windows);
    }

    // S5: written whole, once, after the last window succeeded. Not per window: `words.aligned.json`
    // is what F1.3 reads as "the times are the aligner's", and a half-written file would be that
    // marker for work that was never done.
    let all: Vec<Word> = placed.iter().flat_map(|piece| piece.words.clone()).collect();
    requests::write_aligned(tree, source, &requests::AlignedDoc { words: all.clone() })?;

    out.model = picker.chosen.clone().unwrap_or_default();
    // S4: the written form comes back from the transcript; the times are the aligner's.
    out.words = restore(&all, transcript);
    // S6: only worth measuring when something was placed — an answer with no words at all is reported
    // by the refusal that produced it.
    let spoken = voiced(silences, 0.0, duration);
    if !all.is_empty() {
        let bare = bare_voiced(&all, duration, silences);
        if bare_warns(bare, spoken) {
            out.logs.push(bare_warning(bare, spoken));
        }
    }
    Ok(out)
}

/// S1: what the progress bar says while a window is up there. Recorded rather than drawn.
pub fn progress(index: usize, total: usize) -> String {
    format!("aligning speech {}/{}", index + 1, total)
}

/// S3: one window asked about, halved and split until an answer came back.
///
/// The order is the spec's: trim to sound, split while over the limit and longer than [`MIN_PIECE`],
/// and on an out-of-memory answer halve the limit — never under `MIN_PIECE`, at which point the piece
/// goes up as it stands or the refusal reaches the caller.
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
    let mut limit = WINDOW;
    let mut done = Vec::new();

    while let Some(span) = queue.pop() {
        let window = match trim((span.from, span.to), silences) {
            Some(window) => window,
            // Nothing in it has sound: nothing to place, and no server asked.
            None => continue,
        };
        if window.1 - window.0 > limit && window.1 - window.0 > MIN_PIECE {
            let (left, right) = split_at(&span, silences);
            queue.push(right);
            queue.push(left);
            continue;
        }
        // S3: the clip is the trimmed window — the least audio that still holds all this text's
        // speech — so its seconds are what the retry of a smaller piece is compared on.
        let asked = window.1 - window.0;
        match ask_span(
            tree,
            picker,
            all,
            source,
            &window,
            &span.text,
            language,
            ask,
            out,
        ) {
            Ok(words) => done.push(Placed {
                from: span.from,
                to: span.to,
                words,
            }),
            Err(err) if asr::retryable(&err) => {
                // The ceiling is what a *clip* may be, so the piece is only too big while the audio
                // actually sent still exceeds it. Under that, the refusal stands: halving further
                // would change nothing about what goes up.
                if asked > limit || asked <= MIN_PIECE {
                    return Err(err);
                }
                let Halved { window: next, log } = halve(source, limit);
                match log {
                    Some(log) => {
                        out.logs.push(log);
                        limit = next;
                        // Re-ask this span at the smaller size: pushing it back keeps the order and
                        // the words it was shared.
                        queue.push(span);
                    }
                    None => return Err(err),
                }
            }
            Err(err) => return Err(err),
        }
    }
    done.sort_by(|a, b| a.from.partial_cmp(&b.from).unwrap_or(std::cmp::Ordering::Equal));
    Ok(done)
}

/// S1/S4: ask the aligners about one window until one answers, remembering which one did — a catalog
/// entry is a claim rather than a promise, so the first that answers serves the rest of the run.
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
    let mut last = String::from("no aligner served");
    let candidates = picker.ask(all);
    for model in &candidates {
        // S3/S4: the scratch is this stage's own mess, kept when a window fails so what was asked for
        // it — ffmpeg's cut and the request body — is recoverable without asking again (as in `asr`).
        make_scratch(tree, source, &clip, &plan, &body(&clip, text, language))?;
        match ask(model, &clip, text) {
            Ok(answer) => {
                let words = match parse_words(&answer) {
                    Ok(words) => words,
                    Err(err) => {
                        // The shape is the one thing this cannot be sure of until a server answers, so
                        // what came back is written down rather than guessed at.
                        out.logs.push(unreadable_log(&err, &answer.to_string()));
                        last = err;
                        continue;
                    }
                };
                // S4: the answer timed the clip, so its seconds belong to the recording from here.
                let offset = to_sample(window.0);
                let words = words
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
