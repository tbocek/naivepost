//! F1.13 The session's word list (spec/04-prepare.md) — the one list retakes, joins, `final.txt` and
//! subtitles all read.

use crate::prepare::base_name;
use crate::tools::mm_ss;

/// A word whose loudest moment is under 1/N of its recording's median word was put on no sound.
/// Measured: one such word in 18 155 across three lectures. P.machine.strayWordRatio
pub const STRAY_RATIO: u32 = 10;

/// …and it must start this long after the word before it — a soft word right after another is ordinary
/// speech, and so is a loud one after a pause. P.machine.strayWordGapSeconds
pub const STRAY_GAP_SECONDS: f64 = 1.0;

pub const DRESS_REACH_WORDS: usize = 8;

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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
            if text.is_empty() {
                boundary = true;
            } else if !boundary_before && !out.is_empty() {
                if let Some(last) = out.last_mut() {
                    last.written.push_str(text);
                }
            }
            continue;
        }
        let starts_word =
            !pieces || token.word.starts_with(' ') || boundary_before || out.is_empty();
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
        let mine: Vec<usize> = (0..words.len())
            .filter(|&i| words[i].source == source)
            .collect();
        let loud: Vec<Option<i32>> = mine
            .iter()
            .map(|&i| peak(source.as_str(), words[i].start, words[i].end))
            .collect();
        // The median is over the words that have a peak: one unmeasured word neither lifts it nor drags
        // it down. A recording with nothing measured has no median and no strays.
        let measured: Vec<i32> = loud.iter().flatten().copied().collect();
        let Some(median) = median(&measured) else {
            continue;
        };
        // The first word has no word before it to be timed against; stamped on a breath ahead of itself, it
        // moves onto the sound it is, or the cut starts on the breath and the subtitles run ahead of the voice.
        let spans: Vec<(f64, f64)> = mine
            .iter()
            .map(|&i| (words[i].start, words[i].end))
            .collect();
        if let Some((from, to)) =
            crate::edges::misplaced_first_word(&spans, |a, b| peak(source.as_str(), a, b))
        {
            let first = mine[0];
            logs.push(format!(
                ">>> words: \u{201c}{}\u{201d} was placed ahead of its sound -- moved to where the sound starts ({})",
                words[first].written,
                mm_ss(from)
            ));
            words[first].start = from;
            words[first].end = to;
        }
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
        !word.written.is_empty()
            && (joins_token(&word.written, printed) || joins_token(&word.match_word, printed))
    })
}

/// The whole of F1.13 for one session: tokens in, the list and its log out.
pub const WORDS_TASK: &str = "naivepost:words";

/// The walk's row for this step, or `None` when the policy asks for no marking pass at all.
pub fn row(project: &crate::project::Project) -> Option<crate::run_stages::Task> {
    // No sources means no words to list, and the fix stage is not planned for them either — a row of its
    // own would be the only thing in an otherwise empty plan.
    if project.sources.is_empty() {
        return None;
    }
    if crate::policy::marking_pass_of(&project.policy) == crate::project::MarkingPass::None {
        return None;
    }
    Some(crate::run_stages::Task {
        label: "building the word list".to_string(),
        program: WORDS_TASK.to_string(),
        args: Vec::new(),
        already_done: false,
        session: None,
    })
}

/// The row [`crate::run_stages::drive_walk`] hands here: build the list from the session on disk.
pub fn run_task(_task: &crate::run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let session = crate::startup::open_dir();
    let tree = crate::layout::Tree::new(&session).map_err(|_| {
        format!(
            "{}: not a project folder to write a word list into",
            session.display()
        )
    })?;
    let project = crate::project::load(tree.dir()).unwrap_or_default();
    for line in press_word_list(
        &tree,
        &project,
        crate::policy::marking_pass_of(&project.policy),
    ) {
        log(&line);
    }
    Ok(())
}

pub fn list(
    sources: &[(String, Vec<Token>)],
    mut envelopes: impl FnMut(&str, f64, f64) -> Option<i32>,
    mut transcripts: impl FnMut(&str) -> Option<String>,
    mut fixed: impl FnMut(&str, f64, f64) -> Option<(f64, Vec<String>)>,
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
            let middle = (mine[at].start + mine[at].end) / 2.0;
            let Some((end, line)) = fixed(source, middle, middle) else {
                at += 1;
                continue;
            };
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

/// The whole session list, built and saved. `list` composed with [`save`], so a caller that has the
/// sources in hand makes one call and every later reader finds the same words.
pub fn build_session(
    sources: &[(String, Vec<Token>)],
    envelopes: impl FnMut(&str, f64, f64) -> Option<i32>,
    transcripts: impl FnMut(&str) -> Option<String>,
    fixed: impl FnMut(&str, f64, f64) -> Option<(f64, Vec<String>)>,
    tree: &crate::layout::Tree,
) -> Result<(Vec<Word>, Vec<String>), String> {
    let (words, logs) = list(sources, envelopes, transcripts, fixed);
    save(tree, &words)?;
    Ok((words, logs))
}

/// Write the list to `prepare/word_list.json`.
pub fn save(tree: &crate::layout::Tree, words: &[Word]) -> Result<(), String> {
    let text = serde_json::to_string(words).map_err(|err| format!("word list: {err}"))?;
    let path = tree.session_word_list_json();
    let rel = path
        .strip_prefix(tree.dir())
        .unwrap_or(path.as_path())
        .to_path_buf();
    tree.write_file(&rel, text.as_bytes())
}

/// Read the saved list: `None` when no list was ever written, which is the state before the fix pass
/// finished and NOT an error. Callers fall back to their own ASR reading in that case rather than
/// failing a run over a step that has not happened yet.
pub fn load(tree: &crate::layout::Tree) -> Result<Option<Vec<Word>>, String> {
    let path = tree.session_word_list_json();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// The one call the ▶ handler makes so the session's word list exists before retakes and joins read it.
pub fn press_word_list(
    tree: &crate::layout::Tree,
    project: &crate::project::Project,
    pass: crate::project::MarkingPass,
) -> Vec<String> {
    if pass == crate::project::MarkingPass::None {
        return Vec::new();
    }
    match resume_line(tree) {
        Ok(Some(line)) => return vec![line],
        Ok(None) => (),
        Err(error) => {
            return vec![format!(
                "!!! words: could not read the saved list -- {error}"
            )]
        }
    }
    let sources = match gather(tree, project) {
        Ok(sources) => sources,
        Err(error) => {
            return vec![format!(
                "!!! words: could not build the session word list -- {error}"
            )]
        }
    };
    if sources.iter().all(|(_, tokens)| tokens.is_empty()) {
        return vec![no_tokens_log()];
    }
    // One stat per lane up front: the closure runs once per word and must not touch the disk each time.
    let lanes = lane_stats(tree, project);
    let offsets = lane_offsets(project);
    let built = build_session(
        &sources,
        |lane, start, end| peak_in(&lanes, lane, start, end),
        |lane| raw_transcript(tree, lane),
        // `transcript.fixed.tsv` is on the recording's own clock and the words are on the session's: asked
        // in the recording's seconds, the line's end handed back in the session's.
        |lane, start, end| {
            let off = offsets.get(lane).copied().unwrap_or(0.0);
            fixed_spelling(tree, lane, start - off, end - off)
                .map(|(line_end, words)| (line_end + off, words))
        },
        tree,
    );
    let (words, mut logs) = match built {
        Ok(pair) => pair,
        Err(error) => {
            return vec![format!(
                "!!! words: could not build the session word list -- {error}"
            )]
        }
    };
    let retimed = words.iter().filter(|word| word.stray).count();
    logs.push(summary_log(words.len(), retimed));
    logs
}

/// The resume answer: one line when the list is already on disk, `None` when it has to be built.
fn resume_line(tree: &crate::layout::Tree) -> Result<Option<String>, String> {
    Ok(load(tree)?.map(|words| already_built_log(words.len())))
}

/// The line said when the list was already there, so a re-press reads as a resume and not a redo.
pub fn already_built_log(count: usize) -> String {
    format!(">>> words: session list already built -- {count} words, kept as it is")
}

/// The line said when there were no aligner or ASR tokens to glue: named as a refusal because a silent
/// success here would claim a list the file does not hold.
pub fn no_tokens_log() -> String {
    "!!! words: nothing transcribed yet -- no list written".to_string()
}

/// The line said after a build: how many words the session has and how many S2 moved.
pub fn summary_log(count: usize, retimed: usize) -> String {
    format!(">>> words: {count} words in the session list, {retimed} retimed onto the word before")
}

/// Every source's tokens, keyed the way Prepare keys its files.
fn gather(
    tree: &crate::layout::Tree,
    project: &crate::project::Project,
) -> Result<Vec<(String, Vec<Token>)>, String> {
    let hz = crate::transcribe::SAMPLE_RATE as f64;
    let mut out: Vec<(String, Vec<Token>)> = Vec::new();
    let offsets = lane_offsets(project);
    // On the session clock: a seam is a change of recording in THIS order, and BEFORE has to be the earlier.
    for source in crate::fix_session::in_clock_order(project) {
        let lane = lane_of(source);
        let off = offsets.get(&lane).copied().unwrap_or(0.0);
        let timed = match crate::requests::read_aligned(tree, &lane)? {
            Some(doc) if !doc.words.is_empty() => doc.words,
            _ => crate::requests::read_words(tree, &lane)?
                .map(|doc| doc.words)
                .unwrap_or_default(),
        };
        if timed.is_empty() {
            // Nothing transcribed for this source: it contributes no words, which is a real answer
            // rather than a failure — a video with no speech still leaves the other lanes intact.
            continue;
        }
        let tokens = timed
            .into_iter()
            .map(|word| Token {
                word: word.word,
                start: off + word.start_sample as f64 / hz,
                end: off + word.end_sample as f64 / hz,
            })
            .collect();
        out.push((lane, tokens));
    }
    Ok(out)
}

/// Where each lane's recording starts on the session clock, by lane: the placement every stage shares.
fn lane_offsets(project: &crate::project::Project) -> std::collections::HashMap<String, f64> {
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| base_name(&source.path))
        .collect();
    project
        .sources
        .iter()
        .map(lane_of)
        .zip(crate::fix_transcripts::placement(&names))
        .collect()
}

/// One lane's cached envelope plus where its recording sits on the session clock.
struct LaneWave {
    lane: String,
    /// The session second this recording's first sample lands on, from [`crate::fix_transcripts::placement`].
    off: f64,
    wave: Option<crate::wave::Wave>,
}

/// Read every lane's wave ONCE. Per-word reads would be thousands of opens per press for one number.
fn lane_stats(tree: &crate::layout::Tree, project: &crate::project::Project) -> Vec<LaneWave> {
    // The WHOLE session's stamped file names go into the clock together: leaving one out moves zero
    // and slides every other recording to a different second, which would put each envelope at the
    // wrong place on the timeline.
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| base_name(&source.path))
        .collect();
    let offsets = crate::fix_transcripts::placement(&names);
    project
        .sources
        .iter()
        .zip(offsets)
        .map(|(source, off)| {
            let lane = lane_of(source);
            // size/mtime come from the RECORDING, never from the cache file: reading them back out of
            // the file being checked makes wave::read's staleness test unable to fail.
            let wave = match stat_recording(tree, &source.path) {
                Some((size, mtime)) => crate::wave::read(tree, &lane, size, mtime).ok().flatten(),
                None => None,
            };
            LaneWave { lane, off, wave }
        })
        .collect()
}

/// The loudest bucket whose centre falls inside a word's seconds: S2's "how much sound was this word
/// put on". `None` when the lane has no usable cache or the span holds no bucket — absence of evidence,
/// which `retimed` treats as unmeasurable rather than as silence.
fn peak_in(lanes: &[LaneWave], lane: &str, start: f64, end: f64) -> Option<i32> {
    let held = lanes.iter().find(|held| held.lane == lane)?;
    let peaks = held.wave.as_ref()?.chans.first()?;
    let lo = ((start - held.off) * held.wave.as_ref()?.hz)
        .floor()
        .max(0.0) as usize;
    let hi = ((end - held.off) * held.wave.as_ref()?.hz).ceil().max(0.0) as usize;
    let slice = peaks.get(lo..hi.min(peaks.len()))?;
    slice.iter().map(|peak| i32::from(*peak)).max()
}

/// The raw transcript dressing reads (S3): the text the ASR was handed, absent means undressed.
fn raw_transcript(tree: &crate::layout::Tree, lane: &str) -> Option<String> {
    std::fs::read_to_string(tree.transcript_txt(lane)).ok()
}

/// The fix pass's spelling covering these seconds (S4), from either fixed file.
fn fixed_spelling(
    tree: &crate::layout::Tree,
    lane: &str,
    start: f64,
    end: f64,
) -> Option<(f64, Vec<String>)> {
    let paths = [
        tree.transcript_fixed_tsv(lane),
        tree.commentary_fixed_tsv(lane),
    ];
    for path in paths {
        let Ok(lines) = crate::textfmt::read_lines(&path) else {
            continue;
        };
        if let Some(line) = lines
            .into_iter()
            .find(|line| line.start <= start + 0.001 && line.end >= end - 0.001)
        {
            return Some((
                line.end,
                line.text.split_whitespace().map(str::to_string).collect(),
            ));
        }
    }
    None
}

/// The lane a source's files live under: base name minus extension. Same reduction `retakes::lane_of`,
/// `joins::lane_of` and `prepare_run::lane` use — all private, hence this copy — and the two have to
/// agree or this reads a `words.json` Prepare never wrote.
fn lane_of(source: &crate::project::Source) -> String {
    let name = base_name(&source.path);
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[..dot].to_string(),
        _ => name.to_string(),
    }
}

/// The recording's own size and mtime, off the stored path (`project:` resolves inside the project).
fn stat_recording(tree: &crate::layout::Tree, stored: &str) -> Option<(i64, i64)> {
    let path = match stored.strip_prefix("project:") {
        Some(rel) => tree.dir().join(rel),
        None => std::path::PathBuf::from(stored),
    };
    let meta = std::fs::metadata(&path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some((meta.len() as i64, mtime))
}
