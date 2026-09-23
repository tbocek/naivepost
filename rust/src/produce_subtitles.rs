//! F5.4 Subtitles — `spec/08-produce.md` §F5.4, steps S1–S3. What cues a clip contributes (S1), how they
//! land on the produced clock (S2), and how a track is translated one batch of numbered lines at a time (S3).
//!
//! The clip's own speech comes from Prepare's aligned words respelled by the fixed transcript — never from a
//! second transcription, and never from reading the finished video back. That is why this module takes words
//! as input rather than producing them: `transcribe`/`align` own the words, F5.2 S4 owns putting the clips on
//! the produced clock, and what is decided here is where a cue starts and ends, what is on it, and what a
//! translation may do to it.
//!
//! **Nothing is executed or written here.** The LLM call §3.10 describes belongs to the caller; this holds
//! the message, the tool contract and the merge, so the round-trip's rules are testable without a server.

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

/// One word with its seconds **on its own clip's clock** — recording seconds, before the rate and before the
/// clip's place in the video. `text` empty is a word whose spelling was folded into the one in front of it:
/// it holds its seconds and says nothing of its own.
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
    /// An inserted sound or picture, a freeze, or a clip with no video: what it says is not in the words of
    /// the session, so there is nothing to caption. Narration lines still win over all three (S1's first
    /// branch asks about lines before it asks what the clip is).
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
            Source::Insert => Some("an inserted sound or picture says nothing the words of the session said"),
            Source::Freeze => Some("a freeze holds one frame and no speech"),
            Source::NoVideo => Some("a clip with no video has no picture to caption"),
        }
    }
}

/// S1: the cues a clip contributes — its narration lines if it has any, else (footage only) its own speech.
/// `rate` is the clip's playback rate, needed because a cue's seconds on screen are the produced ones: at 2×
/// six seconds of words are up for three.
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
    // No lines: the clip's own speech, but only where its speech is in the video at all.
    if source == Source::Footage {
        word_cues(words, rate)
    } else {
        Vec::new()
    }
}

/// S1: the words of one footage clip, grouped into cues. A cue closes when the gap before the next word is a
/// breath ([`SUBBREAK_SECONDS`]), when adding that word would pass two rows' worth of characters, or when the
/// cue has been up for [`MAX_SECONDS`] **of produced time** — hence `rate`, which is why the prototype divided
/// by `speed` here.
pub fn word_cues(words: &[Word], rate: f64) -> Vec<Cue> {
    let produced_rate = if rate.abs() < f64::EPSILON { 1.0 } else { rate };
    // The three bounds are all measured on the PRODUCED clock, so they are applied to a copy scaled onto it
    // and the cues that come back are scaled back to the clip's own seconds — which is what `on_clock` and
    // every later step of §F5.2 work in. Scaling only the ceiling instead left a cue up for twice its limit
    // at 2×, because the words were still being counted in recording seconds.
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

/// The grouping itself, on a clock where one second is one second: a cue closes at a breath of
/// [`SUBBREAK_SECONDS`], rather than pass two rows of [`ROW_CHARS`], or once it has been up for
/// [`MAX_SECONDS`].
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

/// The words gathered so far become one cue: joined by spaces, empty ones holding their seconds and saying
/// nothing (the folded-word case), from the first word's start to the last word's end.
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

/// S1 (`an empty cue extends the previous`): a line deleted on purpose, or a word folded into the one in
/// front, holds seconds and says nothing — so it lengthens the caption already up rather than becoming a cue
/// nobody can read. Capped at [`MAX_SECONDS`] like every other cue: an endless silence must not buy an
/// endless caption. A leading empty cue has nothing to extend and is dropped.
pub fn extend_previous(cues: &mut Vec<Cue>) {
    let mut kept: Vec<Cue> = Vec::with_capacity(cues.len());
    for cue in cues.drain(..) {
        if !cue.text.trim().is_empty() {
            kept.push(cue);
            continue;
        }
        match kept.last_mut() {
            Some(previous) => {
                let longest = (cue.e - previous.s).min(MAX_SECONDS);
                previous.e = previous.s + longest.max(0.0);
            }
            None => {}
        }
    }
    *cues = kept;
}

// ---- S2: the produced clock -------------------------------------------------------

/// S2: the clip's cues on the video's own timeline — `produced_start` is where the clip begins in the
/// finished file and every second inside the clip shrinks by its rate. [`crate::produce_render::produced_clocks`]
/// already answers where each clip starts, so a caller building a whole track walks that list and calls this
/// once per clip rather than re-deriving the offsets here.
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

/// S2 in the order the spec gives it: no two cues on screen at once, a breath held rather than blank, and a
/// cue too short to read folded into the one after. A cue whose end fell before its start is clamped first —
/// everything below compares against a span that has to be sane.
pub fn tidy(cues: &[Cue]) -> Vec<Cue> {
    let mut out: Vec<Cue> = Vec::with_capacity(cues.len());
    for mut cue in cues.iter().cloned() {
        if cue.e < cue.s {
            cue.e = cue.s;
        }
        if let Some(previous) = out.last_mut() {
            if cue.s < previous.e {
                // No two on screen at once, whatever the clocks said.
                previous.e = cue.s;
            }
            if cue.s - previous.e < HOLD_SECONDS {
                // The breath between two lines is not a blank screen.
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

/// S2: wrap into at most two rows of [`ROW_CHARS`] where that is possible, breaking between words. Text that
/// already holds a newline is left exactly as it is — a cue the model wrapped is already wrapped, and
/// re-breaking it contradicts what §3.10 asks the model to do. When every word fits on one row but two rows
/// of 42 cannot hold them all, the words are balanced over two rows instead: dropping text off the screen is
/// worse than a long row, and cutting mid-word is worse than both — so no break is ever taken inside a word,
/// and a single word longer than a row simply gets its own row.
pub fn wrap(text: &str) -> String {
    if text.contains('\n') {
        return text.to_string();
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    // Greedy first: fill a row to the width, start the next.
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

/// S2: the placement tag a cue carries. The accepted spellings are [`crate::narrate_pass`]'s, so a line whose
/// `pos` came from a model's answer and one a person chose agree here. `None` is the bottom, which needs no
/// tag because that is where a player puts a caption anyway.
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

/// P.machine.translateBatch (§10: 150): lines per request. New — the prototype sent a whole track in one
/// message and then repaired whatever it dropped, which costs a second call for every line that fell off the
/// end of a long answer. A batch is small enough that the model numbers all of it.
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

/// The numbered list under that sentence: `n\ttext`, one entry per line. A cue's row break becomes ` / ` so
/// one cue is one numbered entry — a raw newline would end the line and the reader of the answer could not
/// tell a wrapped cue from a missing number.
pub fn numbered(original: &[(usize, String)]) -> String {
    original
        .iter()
        .map(|(number, text)| format!("{number}\t{}", sent_line(text)))
        .collect::<Vec<String>>()
        .join("\n")
}

/// What goes into the message for one cue: its text with the row breaks folded, and **no placement tag**.
/// The prototype sent `{\an8}` into the translation as a word and never checked it came back, so a model that
/// dropped it silently lost that caption's placement (§3.10's first must-fix). The tag is not the model's
/// business: the app knows where it put the cue and puts it back on return ([`keep_placement`]).
pub fn sent_text(cue: &Cue) -> String {
    sent_line(&cue.text)
}

fn sent_line(text: &str) -> String {
    // The row breaks are folded to ` / ` so one cue is one numbered entry, and the tag never travels (see
    // [`sent_text`]). A tab in somebody's text would end the numbered line early, so it goes too.
    strip_tag(text).replace('\n', " / ").replace('\t', " ")
}

/// A batch's answer book. `first`/`last` are half-open indices into the whole track, so a number arriving on
/// `translate_line` is checked against the range this request was about — and the re-ask can name the numbers
/// the model was originally given rather than renumbering them (S3: "re-asked once with original numbers").
#[derive(Debug, Clone)]
pub struct Batch {
    pub first: usize,
    pub last: usize,
    got: BTreeMap<usize, String>,
}

impl Batch {
    /// The batch for one `[first, last)` range of the track.
    pub fn new(range: (usize, usize)) -> Self {
        Batch { first: range.0, last: range.1, got: BTreeMap::new() }
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
        (self.first + 1..=self.last).filter(|n| !self.got.contains_key(n)).collect()
    }

    /// Whether every line of this batch came back — the only answer worth caching ([`cached`]).
    pub fn complete(&self) -> bool {
        self.missing().is_empty()
    }

    /// The one re-ask, naming ONLY the lines still missing **with the numbers they were given the first
    /// time**. Renumbering them 1..n is what made the prototype's repair calls land on the wrong lines: a
    /// model told "answer with 2 lines" and given numbers 40 and 97 answers 40 and 97, and so must be asked
    /// in those terms.
    pub fn ask_again_text(&self, original: &[(usize, String)]) -> String {
        let want = self.missing();
        let wanted: Vec<(usize, String)> = original
            .iter()
            .filter(|(number, _)| want.contains(number))
            .cloned()
            .collect();
        let n = wanted.len();
        format!(
            "TRANSLATE THESE {n} LINES. They are lines of a longer track, so their numbers do not start at \
             1: answer with {n} lines, each beginning with the number printed in front of it here and a tab.\
             \n\n{}",
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

/// S3 (`still missing → the original text, with a warning`): a track is never dropped for one bad line. The
/// numbers that stayed in the source language ship as they came, and the log names them so nobody reads a
/// German track and wonders why line 7 is English.
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
    let warning = (!still_missing.is_empty()).then(|| {
        format!(
            "!!! subtitles: {language}: line(s) {} stayed in the original",
            still_missing
                .iter()
                .map(|number| number.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        )
    });
    Merge { lines, still_missing, warning }
}

/// S3 (`cached only when complete`): a partial answer cached is a partial answer forever — the next run sees
/// a cache hit and never asks for the line that went missing. Only a full batch earns a place in `cache/llm`.
pub fn cached(complete: bool) -> bool {
    complete
}

/// S3 (`the session's own language = track 0, never translated`): dropping it from the ticked list whatever a
/// tick says. Translating a language into itself costs a minute and answers with what it was given; its track
/// is written anyway, beside the video and unnamed by a code. Order is preserved so the tracks arrive in the
/// order the menu listed them.
pub fn track_languages(session: &str, ticked: &[String]) -> Vec<String> {
    ticked
        .iter()
        .filter(|code| code.as_str() != session)
        .cloned()
        .collect()
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

/// §3.10's first must-fix, the other half: the ORIGINAL cue decides where its caption sits, whatever came
/// back. Returns the text with the placement put back and `true` when the model returned a tag as a word —
/// which is stripped rather than trusted, because a model told to translate is not a model told to lay out.
pub fn keep_placement(original: &Cue, translated: &str) -> (String, bool) {
    let carried = translated.trim_start() != strip_tag(translated).trim_start();
    let mut text = strip_tag(translated);
    if let Some(tag) = placement(&original.pos) {
        if !text.starts_with(tag) {
            text = format!("{tag}{text}");
        }
    }
    (text, carried)
}

/// §3.10's second must-fix: the model's own line breaks are kept. The prompt asks for them (§F5.4 sends a cue
/// as one numbered line and the wrapping is what the cue needs), and the prototype re-wrapped every answer at
/// P.policy.subtitleRowChars on return, contradicting its own prompt and moving a break the model chose —
/// usually into the middle of a phrase it had deliberately split. Used on the return path so the choice is
/// visible in the code rather than implied by a missing call.
pub fn keep_breaks(text: &str) -> &str {
    text
}
