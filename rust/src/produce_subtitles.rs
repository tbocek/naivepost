//! F5.4 Subtitles — `spec/08-produce.md` §F5.4, steps S1–S3. What cues a clip contributes (S1), how they
//! land on the produced clock (S2), and how a track is translated one batch of numbered lines at a time (S3).

use std::collections::BTreeMap;

use crate::narration::{self, Entry};
use crate::transcribe;

// ---- the five numbers of §10 that build a cue ------------------------------------

/// P.policy.subtitleBreakSeconds (§10: 0.6): a pause this long is a breath, and a breath ends a cue — the
/// reader gets a new caption where the speaker stopped rather than one running across the silence.
pub const SUBBREAK_SECONDS: f64 = 0.6;

/// P.policy.subtitleRowChars (§10: 42): characters per row. Two rows is what a player will show, so two
/// rows' worth of characters is what one cue may carry before it is broken.
pub const ROW_CHARS: usize = 42;

/// P.policy.subtitleMaxSeconds (§10: 6): the longest a cue stays up, however little was said on it. A player
/// wraps what it is given at its own font size, so a cue longer than this is one nobody finishes reading.
pub const MAX_SECONDS: f64 = 6.0;

/// P.policy.subtitleHoldSeconds (§10: 1.2): a gap shorter than this between two cues is not a blank screen —
/// the caption already up is held across it, which is what an editor would do by hand.
pub const HOLD_SECONDS: f64 = 1.2;

/// P.policy.subtitleMinSeconds (§10: 0.8): the shortest a cue may be on screen. Under this a reader sees a
/// flash rather than words, so its text is folded into the cue that follows instead.
pub const MIN_SECONDS: f64 = 0.8;

// ---- S1: the cues one clip contributes -------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Word {
    pub text: String,
    pub s: f64,
    pub e: f64,
}

/// One caption: the seconds it is up (recording-relative until [`on_clock`] moves them), its text with the
/// rows joined by `\n`, and the placement the line asked for (`""` = the bottom).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cue {
    pub s: f64,
    pub e: f64,
    pub text: String,
    pub pos: String,
}

/// What the clip is, which decides whether it contributes cues at all when nobody wrote a line over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Footage: its own speech is worth reading.
    Footage,
    Insert,
    Freeze,
    NoVideo,
}

impl Source {
    /// Why this clip contributes nothing, in the words a log would use. `None` for footage, which always has
    /// its speech to offer.
    pub fn no_cues_reason(self) -> Option<&'static str> {
        match self {
            Source::Footage => None,
            Source::Insert => {
                Some("an inserted sound or picture says nothing the words of the session said")
            }
            Source::Freeze => Some("a freeze holds one frame and no speech"),
            Source::NoVideo => Some("a clip with no video has no picture to caption"),
        }
    }
}

/// S1: the cues a clip contributes — its narration lines if it has any, else (footage only) its own speech.
pub fn cues(source: Source, lines: &[Entry], words: &[Word], rate: f64) -> Vec<Cue> {
    let spoken: Vec<Cue> = lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .map(|line| Cue {
            s: line.s,
            e: line.e,
            text: line.text.clone(),
            pos: line.pos.clone(),
        })
        .collect();
    if !spoken.is_empty() {
        return spoken;
    }
    if source == Source::Footage {
        word_cues(words, rate)
    } else {
        Vec::new()
    }
}

pub fn word_cues(words: &[Word], rate: f64) -> Vec<Cue> {
    let produced_rate = if rate.abs() < f64::EPSILON { 1.0 } else { rate };
    let scaled: Vec<Word> = words
        .iter()
        .map(|word| Word {
            text: word.text.clone(),
            s: word.s / produced_rate,
            e: word.e / produced_rate,
        })
        .collect();
    let mut out = group(&scaled);
    for cue in out.iter_mut() {
        cue.s *= produced_rate;
        cue.e *= produced_rate;
    }
    out
}

fn group(words: &[Word]) -> Vec<Cue> {
    let mut out: Vec<Cue> = Vec::new();
    let mut current: Vec<&Word> = Vec::new();
    let mut chars = 0usize;
    for word in words {
        if !current.is_empty() {
            let last = current[current.len() - 1];
            // Two rows hold 84 characters of text; the count this adds a joining space to is the one §10's
            // row describes, so 84 fits and 85 closes the cue.
            let long = chars + word.text.chars().count() > 2 * ROW_CHARS;
            // Measured to where this word would END: a cue may be up for six seconds, never six plus one
            // more word's length.
            let over = word.e - current[0].s > MAX_SECONDS;
            if word.s - last.e >= SUBBREAK_SECONDS || long || over {
                flush(&mut out, &mut current, &mut chars);
            }
        }
        chars += word.text.chars().count() + 1;
        current.push(word);
    }
    flush(&mut out, &mut current, &mut chars);
    extend_previous(&mut out);
    out
}

fn flush(out: &mut Vec<Cue>, current: &mut Vec<&Word>, chars: &mut usize) {
    if current.is_empty() {
        return;
    }
    let text = current
        .iter()
        .map(|word| word.text.as_str())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<&str>>()
        .join(" ");
    out.push(Cue {
        s: current[0].s,
        e: current[current.len() - 1].e,
        text,
        pos: String::new(),
    });
    current.clear();
    *chars = 0;
}

pub fn extend_previous(cues: &mut Vec<Cue>) {
    let mut kept: Vec<Cue> = Vec::with_capacity(cues.len());
    for cue in cues.drain(..) {
        if !cue.text.trim().is_empty() {
            kept.push(cue);
            continue;
        }
        if let Some(previous) = kept.last_mut() {
            let longest = (cue.e - previous.s).min(MAX_SECONDS);
            previous.e = previous.s + longest.max(0.0);
        }
    }
    *cues = kept;
}

// ---- S2: the produced clock -------------------------------------------------------

pub fn on_clock(cues: &[Cue], produced_start: f64, rate: f64) -> Vec<Cue> {
    cues.iter()
        .map(|cue| Cue {
            s: produced_start + narration::output_seconds(cue.s, rate),
            e: produced_start + narration::output_seconds(cue.e, rate),
            text: cue.text.clone(),
            pos: cue.pos.clone(),
        })
        .collect()
}

pub fn tidy(cues: &[Cue]) -> Vec<Cue> {
    let mut out: Vec<Cue> = Vec::with_capacity(cues.len());
    for mut cue in cues.iter().cloned() {
        if cue.e < cue.s {
            cue.e = cue.s;
        }
        if let Some(previous) = out.last_mut() {
            if cue.s < previous.e {
                previous.e = cue.s;
            }
            if cue.s - previous.e < HOLD_SECONDS {
                previous.e = cue.s;
            }
            if previous.e - previous.s < MIN_SECONDS {
                // Too short to read even after the hold: its words go with what follows rather than flashing.
                cue.s = previous.s;
                cue.text = format!("{}\n{}", previous.text, cue.text);
                out.pop();
            }
        }
        out.push(cue);
    }
    // The last cue has nothing after it to hold against, so it is given the time a reader needs.
    if let Some(last) = out.last_mut() {
        if last.e - last.s < MIN_SECONDS {
            last.e = last.s + MIN_SECONDS;
        }
    }
    out
}

pub fn wrap(text: &str) -> String {
    if text.contains('\n') {
        return text.to_string();
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let mut rows: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in &words {
        if current.is_empty() {
            current = word.to_string();
        } else if current.chars().count() + 1 + word.chars().count() <= ROW_CHARS {
            current.push(' ');
            current.push_str(word);
        } else {
            rows.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    if rows.len() > 2 {
        // The greedy pass needed three rows or more, so two of the width cannot hold this text. Balance it
        // over two instead — split at the space nearest the middle, every break still a word boundary. (A
        // cue whose own words fit two rows never gets here, so no row that could have been 42 wide is longer.)
        let joined = words.join(" ");
        let half = joined.chars().count() / 2;
        let head: String = joined.chars().take(half).collect();
        match head.rfind(' ') {
            Some(at) => rows = vec![joined[..at].to_string(), joined[at + 1..].to_string()],
            None => rows = vec![joined],
        }
    }
    rows.join("\n")
}

pub fn placement(pos: &str) -> Option<&'static str> {
    match pos.trim().to_ascii_lowercase().as_str() {
        "top" => Some(r"{\an8}"),
        "center" | "centre" | "middle" => Some(r"{\an5}"),
        _ => None,
    }
}

/// S2/S9: the cue sheet in SubRip. The timecode is [`transcribe`]'s — one formatter for every `.srt` the app
/// writes rather than one per module.
pub fn srt_text(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (index, cue) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            index + 1,
            transcribe::timecode(cue.s),
            transcribe::timecode(cue.e),
            cue_line(cue)
        ));
    }
    out
}

/// The same cues as WebVTT — the format a browser reads and the only one it will show beside a `<video>`,
/// which is why §1 writes it on every render. Its timecode separates milliseconds with a dot.
pub fn vtt_text(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            vtt_timecode(cue.s),
            vtt_timecode(cue.e),
            cue_line(cue)
        ));
    }
    out
}

/// A cue's text line: the placement tag when it has one, then the wrapped words.
fn cue_line(cue: &Cue) -> String {
    match placement(&cue.pos) {
        Some(tag) => format!("{tag}{}", wrap(&cue.text)),
        None => wrap(&cue.text),
    }
}

fn vtt_timecode(secs: f64) -> String {
    transcribe::timecode(secs).replace(',', ".")
}

// ---- S3: translation, one batch of numbered lines at a time -----------------------

pub const BATCH: usize = 150;

/// §3.10's thinking-off flag: a translation is arithmetic in somebody else's language, and a model that
/// reasons out loud over 150 lines answers slowly and charges for the reasoning.
pub const THINKING_OFF: bool = true;

/// §3.10's two tools. One call per line rather than one answer to be parsed: a line the model cannot place is
/// then a refused call naming its number, not a line silently lost in prose.
pub const TOOLS: [&str; 2] = ["translate_line", "finish"];

/// S3: the cues split into requests of [`BATCH`] lines, half-open `[first, last)` ranges numbered from 1 — so
/// `(0, 150)` holds lines 1 to 150. Empty input costs no request.
pub fn batches(count: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut first = 0;
    while first < count {
        let last = (first + BATCH).min(count);
        out.push((first, last));
        first = last;
    }
    out
}

/// S3's message, §F5.4's sentence verbatim. The numbered lines follow the colon: the numbers are the whole
/// contract, which is why they are the track's own numbers rather than 1..n of what was sent.
pub fn message(n: usize, language: &str) -> String {
    format!("TRANSLATE THESE {n} LINES INTO {language}. Answer with {n} lines, numbered as they are here:")
}

pub const REPAIR_MARK: &str = "their numbers do not start at";

pub fn is_repair_request(message: &str) -> bool {
    message.contains(REPAIR_MARK)
}

pub fn numbered(original: &[(usize, String)]) -> String {
    original
        .iter()
        .map(|(number, text)| format!("{number}\t{}", sent_line(text)))
        .collect::<Vec<String>>()
        .join("\n")
}

/// What goes into the message for one cue: its text with the row breaks folded, and **no placement tag**.
pub fn sent_text(cue: &Cue) -> String {
    sent_line(&cue.text)
}

fn sent_line(text: &str) -> String {
    // The row breaks are folded to ` / ` so one cue is one numbered entry, and the tag never travels (see
    // [`sent_text`]). A tab in somebody's text would end the numbered line early, so it goes too.
    strip_tag(text).replace('\n', " / ").replace('\t', " ")
}

pub fn numbered_one(number: usize, text: &str) -> String {
    format!("{number}\t{}", sent_line(text))
}

#[derive(Debug, Clone)]
pub struct Batch {
    pub first: usize,
    pub last: usize,
    got: BTreeMap<usize, String>,
}

impl Batch {
    /// The batch for one `[first, last)` range of the track.
    pub fn new(range: (usize, usize)) -> Self {
        Batch {
            first: range.0,
            last: range.1,
            got: BTreeMap::new(),
        }
    }

    /// §3.10's `translate_line(n, text)`: ok with the line as it will sit in its cue, or an error naming what
    /// was wrong — a number this batch never asked for (the caller keeps that line's original), or no text.
    pub fn translate_line(&mut self, n: usize, text: &str) -> Result<(), String> {
        if n < self.first + 1 || n > self.last {
            return Err(format!("no such line number in this batch: {n}"));
        }
        if text.trim().is_empty() {
            return Err(format!("line {n} came back empty"));
        }
        self.got.insert(n, keep_breaks(text).to_string());
        Ok(())
    }

    /// §3.10's `finish`: the numbers still without an answer, in order.
    pub fn missing(&self) -> Vec<usize> {
        (self.first + 1..=self.last)
            .filter(|n| !self.got.contains_key(n))
            .collect()
    }

    /// Whether every line of this batch came back — the only answer worth caching ([`cached`]).
    pub fn complete(&self) -> bool {
        self.missing().is_empty()
    }

    pub fn ask_again_text(&self, original: &[(usize, String)]) -> String {
        let want = self.missing();
        let wanted: Vec<(usize, String)> = original
            .iter()
            .filter(|(number, _)| want.contains(number))
            .cloned()
            .collect();
        let n = wanted.len();
        format!(
            "TRANSLATE THESE {n} LINES. They are lines of a longer track, so {REPAIR_MARK} 1: answer \
             with {n} lines, each beginning with the number printed in front of it here and a tab.\n\n{}",
            numbered(&wanted)
        )
    }

    /// What this batch has collected, for [`merge`].
    pub fn answers(&self) -> &BTreeMap<usize, String> {
        &self.got
    }
}

/// One language's finished track: the lines in order, which numbers never arrived, and what to say about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Merge {
    pub lines: Vec<String>,
    pub still_missing: Vec<usize>,
    pub warning: Option<String>,
}

pub fn merge(language: &str, original: &[(usize, String)], got: &BTreeMap<usize, String>) -> Merge {
    let mut lines = Vec::with_capacity(original.len());
    let mut still_missing = Vec::new();
    for (number, text) in original {
        match got.get(number) {
            Some(translated) => lines.push(translated.clone()),
            None => {
                lines.push(sent_line(text).replace(" / ", "\n"));
                still_missing.push(*number);
            }
        }
    }
    let warning =
        (!still_missing.is_empty()).then(|| left_in_the_original(language, &still_missing));
    Merge {
        lines,
        still_missing,
        warning,
    }
}

pub fn left_in_the_original(language: &str, numbers: &[usize]) -> String {
    format!(
        "!!! subtitles: {language}: {} line(s) left in the original ({}) -- the rest of the track is good",
        numbers.len(),
        numbered_inline(numbers)
    )
}

/// §08 §6's re-ask line: `>>> subtitles: <lang> came back missing line 7, 64, 92 -- asking again for N
/// line(s)`. Said BEFORE the second call, so a slow second round is explained while it is running rather than after.
pub fn came_back_missing(language: &str, numbers: &[usize]) -> String {
    format!(
        ">>> subtitles: {language} came back missing line {} -- asking again for {} line(s)",
        numbered_inline(numbers),
        numbers.len()
    )
}

/// §08 §6's cache line, said when a complete cached track answered instead of the model: the person learns the
/// minute they expected to wait did not happen, and why.
pub fn from_cache_log(language: &str) -> String {
    format!(
        ">>> subtitles: {language} came from the cache -- the same lines were translated before"
    )
}

/// A list of line numbers as §6 writes them inside parentheses: `line 7, 92`.
fn numbered_inline(numbers: &[usize]) -> String {
    numbers
        .iter()
        .map(usize::to_string)
        .collect::<Vec<String>>()
        .join(", ")
}

/// S3 (`cached only when complete`): a partial answer cached is a partial answer forever — the next run sees
/// a cache hit and never asks for the line that went missing. Only a full batch earns a place in `cache/llm`.
pub fn cached(complete: bool) -> bool {
    complete
}

pub fn track_languages(session: &str, ticked: &[String]) -> Vec<String> {
    let own = crate::roles::language_code(session);
    let mut out: Vec<String> = Vec::new();
    for tick in ticked {
        let code = crate::roles::language_code(tick);
        if code.is_empty() || code == own || out.contains(&code) {
            continue;
        }
        out.push(code);
    }
    out
}

// ---- §3.10's two must-fix behaviours ---------------------------------------------

/// A leading placement tag, as the model may hand one back: `{\an8}` or `{\an5}`, with or without braces left
/// on. Only a leading one — a brace in somebody's sentence is their punctuation.
fn strip_tag(text: &str) -> String {
    let trimmed = text.trim_start();
    for tag in [r"{\an8}", r"{\an5}"] {
        if let Some(rest) = trimmed.strip_prefix(tag) {
            return rest.trim_start().to_string();
        }
    }
    text.to_string()
}

pub fn keep_placement(original: &Cue, translated: &str) -> (String, bool) {
    let carried = translated.trim_start() != strip_tag(translated).trim_start();
    // Peel until nothing more comes off: one pass left ` {\an8}` standing inside the caption, because the
    // space hid the tag from `strip_tag` and the app then put ITS tag in front of both.
    let mut rest = strip_tag(translated);
    loop {
        let trimmed = rest.trim_start();
        if trimmed == rest {
            break;
        }
        let stripped = strip_tag(trimmed);
        if stripped == rest {
            break;
        }
        rest = stripped;
    }
    let text = match placement(&original.pos) {
        Some(tag) => format!("{tag}{rest}"),
        None => rest,
    };
    (text, carried)
}

pub fn keep_breaks(text: &str) -> &str {
    text
}
