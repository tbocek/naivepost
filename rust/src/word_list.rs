//! F1.13 The session's word list (spec/04-prepare.md) — the one list retakes, joins, `final.txt` and
//! subtitles all read.
//!
//! Aligner (or ASR) tokens come in, words go out: glued on the session clock, with a word the aligner
//! put on almost no sound moved onto the end of the word before it, then dressed with the raw
//! transcript's case and punctuation and re-dressed with the fix pass's spelling. **Times never change —
//! only the written form**, which is what makes the list safe for every reader: a cut measured against it
//! is unaffected by a respelling, and a caption shows what the transcript pass settled on rather than
//! what one recogniser happened to hear.
//!
//! Two spellings per word, deliberately: `match_word` is what every pass matches against and never moves
//! once glued, `written` is what a human or the join model is shown and the only field dressing rewrites.
//! "Only .raw changes — the word used for MATCHING stays the recogniser's, because every pass that reads
//! the words reads that one" (prototype `gui/subwords.go`).

use crate::tools::mm_ss;

/// A word whose loudest moment is under 1/N of its recording's median word was put on no sound.
/// Measured: one such word in 18 155 across three lectures. P.machine.strayWordRatio
pub const STRAY_RATIO: u32 = 10;

/// …and it must start this long after the word before it — a soft word right after another is ordinary
/// speech, and so is a loud one after a pause. P.machine.strayWordGapSeconds
pub const STRAY_GAP_SECONDS: f64 = 1.0;

/// How far ahead in the transcript a word is looked for when dressing: the ASR may have heard a word the
/// transcript has twice, or missed one, and eight fields covers both without walking into the next
/// sentence. P.eng.dressReachWords
pub const DRESS_REACH_WORDS: usize = 8;

/// How far apart the two spellings of one stretch may be before the re-dressing walk gives up on it:
/// "two hundred and sixty" against "RSA-260" is four words against one, and six covers every rewrite of a
/// number, a name or a company seen so far. Past it the walk has lost its place, and dressing the rest of
/// the line from the wrong offset would put the right words on the wrong seconds.
/// P.eng.respellRunReachWords
pub const RESPPELL_REACH_WORDS: usize = 6;

/// One aligner (or ASR) token, already in session seconds. The caller converts samples with
/// [`crate::transcribe::seconds`] and adds the recording's offset from `prepare/transcript/offsets.tsv`,
/// so nothing here does clock arithmetic.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub word: String,
    pub start: f64,
    pub end: f64,
}

/// One word of the session list.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    /// Which recording it came off — the key every per-recording rule (median, cursor) groups by.
    pub source: String,
    /// The bare lowercase form every pass matches against. Never rewritten after gluing.
    pub match_word: String,
    /// What a reader or the join model is shown: the token's own text until dressing says otherwise.
    pub written: String,
    pub start: f64,
    pub end: f64,
    /// Whether this word was placed at the end of the word before it (S2), kept so a cut can tell a
    /// retimed word from one the aligner really heard there.
    pub stray: bool,
}

impl Word {
    fn new(source: &str, written: String, start: f64, end: f64) -> Word {
        Word {
            source: source.to_string(),
            match_word: bare(&written),
            written,
            start,
            end,
            stray: false,
        }
    }
}

/// A token's bare matching form: no surrounding punctuation, no case. Punctuation-only text is empty,
/// which is exactly what makes it a comma rather than a word.
pub fn bare(word: &str) -> String {
    word.trim()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// S1: glue tokens into words on the session clock.
///
/// A leading space anywhere means the server answered in pieces — " wa", "lle", "t" — and a token of
/// nothing but space marks where a word begins; with no leading space anywhere every token is a word, and
/// the same rule would otherwise weld a whole take into one word (the prototype measured "1 word" twice
/// before it found this out). Punctuation carries no sound of its own, so it joins the word in front
/// without moving that word's end: left in, a comma's stamp lands in the silence after the sentence and
/// makes every last word of that sentence look a second longer than it is. A token with neither sound nor
/// a word before it belongs to nothing and is dropped.
pub fn glued(source: &str, tokens: &[Token]) -> Vec<Word> {
    let pieces = tokens.iter().any(|token| token.word.starts_with(' '));
    let mut out: Vec<Word> = Vec::with_capacity(tokens.len());
    // Set by a token of nothing but space: the next word starts a word even in mid-piece.
    let mut boundary = true;
    for token in tokens {
        let text = token.word.trim();
        // A space is a boundary before the next word; anything else measured against the word in front.
        let boundary_before = boundary;
        if bare(text).is_empty() {
            // Text and not sound: it belongs to the word in front, which is what the subtitles read back.
            // A token of nothing but space marks where a word begins — remembered rather than dropped, or
            // the next piece welds onto the word before (the prototype measured "1 word" that way twice).
            if text.is_empty() {
                boundary = true;
            } else if !boundary_before && !out.is_empty() {
                if let Some(last) = out.last_mut() {
                    last.written.push_str(text);
                }
            }
            continue;
        }
        let starts_word = !pieces || token.word.starts_with(' ') || boundary_before || out.is_empty();
        if starts_word {
            out.push(Word::new(source, text.to_string(), token.start, token.end));
        } else if let Some(last) = out.last_mut() {
            // A piece of the word being built: its own seconds extend that word's, and its letters join
            // the matching form too — " wa" + "lle" is one word to match as well as one word to read.
            last.written.push_str(text);
            last.match_word.push_str(&bare(text));
            last.end = token.end;
        }
        // Only a real boundary is kept: glued from there on, a leading space is what starts a word.
        boundary = false;
    }
    out
}

/// S2: a word placed on almost no sound, well after the word before it, is timed at that word's end.
///
/// The aligner matches text to sound, and a last word said quickly after the one before can be matched to
/// the next thing that makes any noise at all — on one lecture "Geld." landed on a breath 3.4 s late and
/// the clip ending on it carried three seconds of silence. `peak` is the caller's envelope lookup
/// (`None` = no envelope for this recording, in which case nothing can be called a stray); both the
/// median and the gap are per recording. Returns one log line per retimed word.
pub fn retimed(
    words: &mut [Word],
    mut peak: impl FnMut(&str, f64, f64) -> Option<i32>,
) -> Vec<String> {
    let mut logs = Vec::new();
    let mut sources: Vec<String> = words.iter().map(|word| word.source.clone()).collect();
    sources.dedup();
    for source in sources {
        // Every word of this recording, in order — the neighbour a word is timed against is the word
        // before it, whether or not there was sound enough to measure either of them.
        let mine: Vec<usize> = (0..words.len()).filter(|&i| words[i].source == source).collect();
        let loud: Vec<Option<i32>> = mine
            .iter()
            .map(|&i| peak(source.as_str(), words[i].start, words[i].end))
            .collect();
        // The median is over the words that have a peak: one unmeasured word neither lifts it nor drags
        // it down. A recording with nothing measured has no median and no strays.
        let measured: Vec<i32> = loud.iter().flatten().copied().collect();
        let Some(median) = median(&measured) else { continue };
        for row in 1..mine.len() {
            let this = mine[row];
            // No envelope under this word's seconds: it is not evidence of anything.
            let Some(loud) = loud[row] else { continue };
            let quiet = loud as i64 * i64::from(STRAY_RATIO) < i64::from(median);
            let late = words[this].start - words[mine[row - 1]].end > STRAY_GAP_SECONDS;
            if !quiet || !late {
                continue;
            }
            let at = words[mine[row - 1]].end;
            logs.push(stray_log(&words[this].written, at));
            words[this].start = at;
            words[this].end = at;
            words[this].stray = true;
        }
    }
    logs
}

/// S2's line, said once per retimed word.
fn stray_log(word: &str, at: f64) -> String {
    format!(
        ">>> words: \u{201c}{word}\u{201d} was placed on almost no sound, well after the word before it \
         -- timed at the end of that word instead ({})",
        mm_ss(at)
    )
}

/// The middle of a sorted list — the value half the recording's words reach or beat.
fn median(values: &[i32]) -> Option<i32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Some(sorted[sorted.len() / 2])
}

/// S3: the raw transcript's case and punctuation, put back on words the aligner answered bare.
///
/// The aligner hands back what it matched rather than what it was given, and a subtitle is read. Each
/// recording walks its own part of the list with its own cursor, both in time order; a word the
/// transcript has no counterpart for keeps what was heard, since a caption missing a capital is a smaller
/// fault than one missing a word.
pub fn dressed(words: &mut [Word], transcript: &str) {
    let fields: Vec<&str> = transcript.split_whitespace().collect();
    // One cursor per recording: the words and the transcript are both in time order, so each recording
    // picks up where its own last word left off.
    let mut cursors: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for word in words.iter_mut() {
        let cursor = cursors.get(&word.source).copied().unwrap_or(0);
        // Look ahead rather than give up: the ASR may have heard a word twice or skipped one, and the
        // reach is bounded so a walk that has truly lost its place cannot wander into the next sentence.
        let found = (cursor..(cursor + DRESS_REACH_WORDS).min(fields.len()))
            .find(|&k| bare(fields[k]) == word.match_word);
        if let Some(k) = found {
            word.written = fields[k].to_string();
            cursors.insert(word.source.clone(), k + 1);
        }
    }
}

/// S4: the fix pass's spelling over the same seconds — a re-dressing and never a rewrite.
///
/// `fixed` is one session line's words as the transcript pass wrote them. Where the two spell a stretch
/// differently the whole fixed spelling goes on the FIRST word of the stretch and the rest are emptied:
/// every word keeps its own seconds, so a cut through the middle of such a stretch loses the words rather
/// than showing words the video does not play. Returns whether anything was touched — a line sharing no
/// word with these is left alone entirely, because rewriting from a text that shares nothing with it puts
/// the wrong words on these seconds.
pub fn respelled(words: &mut [Word], fixed: &[&str]) -> bool {
    if words.is_empty() || fixed.is_empty() || !any_shared(words, fixed) {
        return false;
    }
    // Fixed words with no time of their own, waiting for a word that has one.
    let mut pending = String::new();
    // Rust will not let a closure hold `&mut pending` while the loop mutates `words`, so the two are
    // passed to this together: what is put on a word is whatever was waiting, then nothing is.
    fn put(word: &mut Word, text: &str, pending: &mut String) {
        word.written = format!("{pending}{text}").trim().to_string();
        pending.clear();
    }
    let (mut i, mut j) = (0usize, 0usize);
    while i < words.len() && j < fixed.len() {
        if bare(fixed[j]) == words[i].match_word {
            put(&mut words[i], fixed[j], &mut pending);
            i += 1;
            j += 1;
            continue;
        }
        match resync(words, fixed, i, j) {
            // Nothing within reach spells the same: take them as the same word anyway — it is the same
            // position in a line that matched either side of here — and let the transcript win. "RSA-1024"
            // against a heard "rsa" resyncs on neither and is the right word for that second.
            None => {
                put(&mut words[i], fixed[j], &mut pending);
                i += 1;
                j += 1;
            }
            // The transcript has words the recogniser never heard: they have no seconds, so they ride on
            // the next word that does.
            Some((0, dj)) => {
                pending.push_str(&fixed[j..j + dj].join(" "));
                pending.push(' ');
                j += dj;
            }
            Some((di, dj)) => {
                let joined = fixed[j..j + dj].join(" ");
                put(&mut words[i], &joined, &mut pending);
                for word in &mut words[i + 1..i + di] {
                    word.written.clear();
                }
                i += di;
                j += dj;
            }
        }
    }
    // Whatever the line ends with and the words do not: onto the last word that has a time.
    if j < fixed.len() && !words.is_empty() {
        let last = i.min(words.len()) - 1;
        let tail = fixed[j..].join(" ");
        words[last].written = format!("{} {tail}", words[last].written).trim().to_string();
    }
    // …and the other way round: words left over when the line ran out are the tail of a stretch folded
    // into fewer words, and the fold is already printed on their first — kept, they would be said twice.
    let rest = i.min(words.len());
    for word in &mut words[rest..] {
        word.written.clear();
    }
    true
}

/// Whether the two spellings of a line have a single word in common — what makes the walk a re-dressing
/// rather than a guess.
fn any_shared(words: &[Word], fixed: &[&str]) -> bool {
    fixed
        .iter()
        .any(|field| words.iter().any(|word| bare(field) == word.match_word))
}

/// The shortest pair of runs that puts the walk back in step: how many words of the recogniser's and how
/// many of the transcript's to skip before the two spell the same word again. Shortest by total length,
/// so one word against four is found before two against five.
fn resync(words: &[Word], fixed: &[&str], i: usize, j: usize) -> Option<(usize, usize)> {
    for total in 1..=2 * RESPPELL_REACH_WORDS {
        for di in 0..=total {
            let dj = total - di;
            if di > RESPPELL_REACH_WORDS || dj > RESPPELL_REACH_WORDS {
                continue;
            }
            if i + di >= words.len() || j + dj >= fixed.len() {
                continue;
            }
            if bare(fixed[j + dj]) == words[i + di].match_word {
                return Some((di, dj));
            }
        }
    }
    None
}

/// The list as its readers see it. Words emptied by a fold stay in the list — they hold seconds a cut may
/// land on — so a reader that wants text filters them out.
pub fn written(words: &[Word]) -> Vec<String> {
    words.iter().map(|word| word.written.clone()).collect()
}

/// Whether the join pass's answer names this word.
///
/// [`crate::tools::textedit::Join`] is shown the `written` forms and matches what the model sends back
/// against exactly these tokens: a word printed as two owns both pieces, so a piece survives if either
/// half of it does — "RSA-260" against a heard "rsa" is one word said once. A word's own punctuation is
/// dropped first (a comma the model did not repeat is no obstacle), but a hyphen or a space is kept as the
/// seam between its pieces, so only a piece taken at that seam matches: an unrelated word never does, and
/// neither does a fragment from the middle of one.
pub fn joins_token(word: &str, printed: &str) -> bool {
    let (word, printed) = (named_forms(word), named_forms(printed));
    if word[0].is_empty() || printed[0].is_empty() {
        return false;
    }
    // forms[0] is the whole token and the rest are its pieces at a seam: a whole matching a piece of the
    // other side is the seam rule, and two wholes equal is what lets a plain heard word match a plain
    // printed one. Two pieces never match each other — that would let any folded word name any other.
    word.iter().any(|one| printed[1..].contains(one))
        || printed.iter().any(|one| word[1..].contains(one))
        || word[0] == printed[0]
}

/// Every form a token can be named by: its whole shape first, then the shape of each piece at a hyphen or
/// space seam — "RSA-260" is named both by "rsa260" and by "rsa". [`bare`] alone would weld the pieces into
/// one string and lose the seam, so this splits first and only then strips punctuation.
fn named_forms(token: &str) -> Vec<String> {
    let whole = bare(token);
    let mut forms: Vec<String> = token
        .split(['-', ' '])
        .map(bare)
        .filter(|form| !form.is_empty() && *form != whole)
        .collect();
    // The whole shape leads, but only when it carries sound: a bare "" must never match a piece.
    forms.insert(0, whole);
    forms
}

/// Which word of the list a printed piece belongs to: matched against the bare form every pass uses and,
/// since a fold empties the words behind it, against what is written as well.
pub fn owns(words: &[Word], printed: &str) -> Option<usize> {
    words.iter().position(|word| {
        !word.written.is_empty() && (joins_token(&word.written, printed) || joins_token(&word.match_word, printed))
    })
}

/// The whole of F1.13 for one session: tokens in, the list and its log out.
///
/// `envelopes` is each recording's loudness lookup (S2), `transcripts` its raw transcript as the aligner
/// was handed it (S3), and `fixed` the words of each fixed session line that covers these seconds (S4).
/// Who reads `session.tsv` is not this module's question — the caller passes the lines through
/// [`crate::textfmt::read_session`], skipping EVENT rows and lines with no text, which say nothing about
/// spelling. Lines are asked for per recording in time order and each gets its own cursor over the words,
/// whose middle falls inside it. A line is asked for at most once, which is what stops a caller that
/// answers from a clock rather than from its arguments from being looped over forever.
pub fn list(
    sources: &[(String, Vec<Token>)],
    mut envelopes: impl FnMut(&str, f64, f64) -> Option<i32>,
    mut transcripts: impl FnMut(&str) -> Option<String>,
    mut fixed: impl FnMut(&str, f64, f64) -> Option<Vec<String>>,
) -> (Vec<Word>, Vec<String>) {
    let mut all: Vec<Word> = Vec::new();
    let mut logs = Vec::new();
    for (source, tokens) in sources {
        let mut mine = glued(source, tokens);
        logs.extend(retimed(&mut mine, &mut envelopes));
        if let Some(text) = transcripts(source) {
            dressed(&mut mine, &text);
        }
        // One pass over the recording's words: each window is the words whose middle sits in the line
        // asked for at its first word, so a line never covers the same word twice.
        let mut at = 0usize;
        while at < mine.len() {
            let Some(line) = fixed(source, mine[at].start, mine[at].end) else { break };
            let end = mine[at].end;
            let span = mine[at..]
                .iter()
                .position(|word| (word.start + word.end) / 2.0 > end + 0.01)
                .unwrap_or(mine.len() - at);
            let words = &mut mine[at..at + span];
            respelled(words, &line.iter().map(String::as_str).collect::<Vec<_>>());
            at += span.max(1);
        }
        all.extend(mine);
    }
    (all, logs)
}
