//! One source read: audio → text → word times → speakers → segments (spec/04-prepare.md F1.3).
//! Output is `transcript.tsv`, `transcript.srt` and resume sidecars; downstream never learns an ASR, aligner or diarizer existed. Like [`crate::separate`] it spawns nothing: decode length (S1), ASR answer (S3), aligner availability (S4) and turns (S5) arrive as arguments (spec/00-principles.md §5).

use crate::layout::Tree;
use crate::requests::{self, Turn, Word, WordsDoc};
use crate::textfmt::{self, Line};

/// S1: the sample rate `voice16k.wav` is written at and every sidecar's samples are counted in; one constant so a word's time is read at one rate.
pub const SAMPLE_RATE: u64 = 16_000;

pub const MERGE_GAP: f64 = 0.7;

pub const MERGE_MAX_LEN: f64 = 12.0;

/// S6: how long a word may be (`mergeMaxWord`). Parakeet stretches a word's end across the following silence.
pub const MERGE_MAX_WORD: f64 = 2.0;

/// S6: how far away a turn may be and still own a word (`mergeNear`); a word outside every turn goes to the nearest.
pub const MERGE_NEAR: f64 = 1.0;

/// S5/S6: same-speaker turns closer than this are one turn (`diarTurnGap`), so a pause does not make two speakers.
/// §10: `P.eng.turnGapSeconds` (0.5, "gap that ends a turn"); `params::prepare()` rows it from this constant. INCLUSIVE (`<=`). `glue_turns` looks only at the LAST kept turn, so turns must arrive in time order; a later turn ending earlier does not shorten it.
pub const TURN_GLUE: f64 = 0.5;

/// S6: a word no turn covers and none is near; keeps the running speaker so it never shows as a name.
pub const UNKNOWN: &str = "?";

/// S6: the one voice when diarization found none (`turns.json` is `[]`) but the ASR heard words; named like a first speaker, as "?" would show as a name.
pub const FIRST_SPEAKER: &str = "SPEAKER_00";

pub fn seconds(sample: u64) -> f64 {
    sample as f64 / SAMPLE_RATE as f64
}

pub fn audio_log(base: &str, secs: f64) -> String {
    format!(">>> [{base}] {secs:.1} s of audio")
}

/// S2 (`P.policy.minTakeSeconds`): shorter is a start/stop, not worth three servers.
pub fn short_take_log(base: &str, secs: f64) -> String {
    format!(">>> [{base}] {secs:.1} s long -- a start/stop, not a take: written up as silence")
}

/// S3: the ASR is being asked; named with the model, since the log is where which one answered is recoverable.
pub fn asr_log(base: &str, model: &str) -> String {
    format!(">>> [{base}] ASR ({model})")
}

pub fn asr_done_log(base: &str) -> String {
    format!(">>> [{base}] ASR already done")
}

pub fn align_warning(base: &str, err: &str) -> String {
    format!("!!! [{base}] align: {err} -- the ASR's own times stand")
}

/// S4: an aligner that failed with nothing else to time the words is a dead end, named by the model that could not do it.
pub fn align_failure(model: &str, err: &str) -> String {
    format!(
        "align: {err}\n\n{model} answers with no word times of its own, so the aligner is the only \
         thing that can time them and this recording cannot be transcribed without it"
    )
}

pub fn segments_log(base: &str, count: usize) -> String {
    format!(">>> [{base}] {count} segments")
}

#[derive(Debug, Clone, PartialEq)]
pub enum Alignment {
    Aligned,
    /// Nothing aligned and nothing lost: `words.aligned.json` was already there (resume) or there were no words.
    Skipped,
    Stood,
    Fatal(String),
}

/// S4: what an alignment failure means. `err` is `None` when the aligner answered; `asr_has_word_times` is whether the ASR document has times; `transcript` is `transcript.txt`.
pub fn align_outcome(err: Option<&str>, asr_has_word_times: bool, transcript: &str) -> Alignment {
    let Some(err) = err else {
        return Alignment::Aligned;
    };
    if asr_has_word_times {
        return Alignment::Stood;
    }
    if transcript.trim().is_empty() {
        // A recording with no words is not an aligner failure; the silence case has its own line.
        return Alignment::Skipped;
    }
    Alignment::Fatal(err.to_string())
}

/// S5/S6: turns as segments want them: seconds and speakers named as a diarized session (`SPEAKER_00`, …). Same-speaker turns closer than [`TURN_GLUE`] are one turn; gluing is idempotent.
pub fn glue_turns(turns: &[Turn]) -> Vec<(f64, f64, String)> {
    let mut glued: Vec<(f64, f64, String)> = Vec::with_capacity(turns.len());
    for turn in turns {
        let (start, end) = (seconds(turn.start_sample), seconds(turn.end_sample));
        let speaker = format!("SPEAKER_{:02}", turn.speaker_id);
        match glued.last_mut() {
            Some(previous) if previous.2 == speaker && start - previous.1 <= TURN_GLUE => {
                if end > previous.1 {
                    previous.1 = end;
                }
            }
            _ => glued.push((start, end, speaker)),
        }
    }
    glued
}

/// S6: who said a word: the turn sharing the most time wins, else the nearest within [`MERGE_NEAR`]. With no turns the recording has one voice ([`FIRST_SPEAKER`]); `UNKNOWN` is a word between turns.
pub fn speaker_of(word_start: f64, word_end: f64, turns: &[(f64, f64, String)]) -> String {
    let mut best = 0.0;
    let mut found = None;
    for (start, end, speaker) in turns {
        let overlap = word_end.min(*end) - word_start.max(*start);
        if overlap > best {
            best = overlap;
            found = Some(speaker);
        }
    }
    if let Some(speaker) = found {
        return speaker.clone();
    }

    let mut nearest = MERGE_NEAR;
    let mut found = None;
    for (start, end, speaker) in turns {
        let away = if *start > word_end {
            *start - word_end
        } else if *end < word_start {
            word_start - *end
        } else {
            0.0
        };
        if away < nearest {
            nearest = away;
            found = Some(speaker);
        }
    }
    if let Some(speaker) = found {
        return speaker.clone();
    }
    if turns.is_empty() {
        return FIRST_SPEAKER.to_string();
    }
    UNKNOWN.to_string()
}

/// S6: words and turns become the segments of the two transcript files. A segment ends at a speaker change, a silence over [`MERGE_GAP`], or past [`MERGE_MAX_LEN`]. Word ends are clamped to [`MERGE_MAX_WORD`] first.
pub fn segments(words: &[Word], turns: &[(f64, f64, String)]) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut previous_end = 0.0;
    for word in words {
        let start = seconds(word.start_sample);
        let end = seconds(word.end_sample).min(start + MERGE_MAX_WORD);
        let mut speaker = speaker_of(start, end, turns);

        // A word nobody could place keeps the running speaker, borrowed only while the open segment is not (one borrow too many otherwise).
        let unnamed = out.last().is_some_and(|current| current.speaker == UNKNOWN);
        if speaker == UNKNOWN && !unnamed {
            if let Some(current) = out.last() {
                speaker = current.speaker.clone();
            }
        }
        let open = out.last_mut();
        let breaks = open.as_ref().is_some_and(|current| {
            speaker != current.speaker
                || start - previous_end > MERGE_GAP
                || end - current.start > MERGE_MAX_LEN
        });
        if breaks {
            out.push(Line {
                start,
                end,
                speaker: speaker.clone(),
                text: word.word.clone(),
            });
        } else if let Some(current) = open {
            // A segment that opened on an unplaceable word is named by the first word that could be placed.
            if current.speaker == UNKNOWN && speaker != UNKNOWN {
                current.speaker = speaker;
            }
            current.text.push(' ');
            current.text.push_str(&word.word);
            current.end = end;
        } else {
            out.push(Line {
                start,
                end,
                speaker: speaker.clone(),
                text: word.word.clone(),
            });
        }
        previous_end = end;
    }
    out
}

/// S6: `transcript.srt`. One block per segment: number, times, then the text with the speaker in brackets in front; an unnamed speaker gets no tag.
pub fn srt_text(lines: &[Line]) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        let tag = if line.speaker == UNKNOWN {
            String::new()
        } else {
            format!("[{}] ", line.speaker)
        };
        out.push_str(&format!(
            "{}\n{} --> {}\n{}{}\n\n",
            index + 1,
            timecode(line.start),
            timecode(line.end),
            tag,
            line.text
        ));
    }
    out
}

/// A subtitle timecode `HH:MM:SS,mmm`; negative times clamp to zero.
pub fn timecode(secs: f64) -> String {
    let secs = secs.max(0.0);
    let whole = secs as u64;
    let millis = ((secs - secs.floor()) * 1000.0 + 0.5) as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        whole / 3600,
        (whole % 3600) / 60,
        whole % 60,
        millis
    )
}

/// S6: both transcript files from the same segments (tsv is what Cut reads, srt what a person reads).
pub fn write_transcript(tree: &Tree, source: &str, lines: &[Line]) -> Result<(), String> {
    textfmt::write_lines(lines, &tree.transcript_tsv(source))?;
    textfmt::write_rows_or_none(&tree.transcript_srt(source), &srt_text(lines))
}

/// S6: words an answer sent back that cannot be placed on the clock. No usable `start_sample`/`end_sample` means the answer changed shape, and cut points sit on word edges, so guessing would move the video.
pub fn unusable_words(words: &[Word]) -> usize {
    words
        .iter()
        .filter(|word| word.end_sample <= word.start_sample)
        .count()
}

/// S6: refusing a transcript whose words have no usable times, naming how many.
pub fn unusable_words_error(count: usize) -> String {
    format!("{count} words carry no usable start_sample/end_sample -- the answer changed shape")
}

/// S4/S6: transcribed but nothing has a time and no aligner can give them. Named by the model that answered, since the fix is a setting. The variant is for when an aligner was asked and left no times; the warning above says why.
pub fn no_word_times(asr: &str, aligner_left_none: bool) -> String {
    if aligner_left_none {
        return format!(
            "{asr} transcribed this recording but timed no words, and the aligner left no times either, \
             which the line above this one says why"
        );
    }
    format!(
        "{asr} transcribed this recording but timed no words, no aligner is registered to time them -- \
         register one (task \"align\") -- or use an ASR that answers with word timings"
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct Passage {
    pub logs: Vec<String>,
    pub segments: Vec<Line>,
}

/// What the aligner did, as far as this pass is concerned; not an `Option<Result>` because "no aligner served" and "nothing to align" are different decisions.
pub enum Align {
    None,
    Done,
    Failed(String),
}

impl Align {
    fn err(&self) -> Option<&str> {
        match self {
            Align::Failed(err) => Some(err),
            _ => None,
        }
    }
}

pub enum AlignStep {
    Quiet,
    Aligned,
    Warns(String),
    Stops(String),
}

/// S1-S6 for one source. `duration` is the length of `voice16k.wav` (decoded by the caller, S1). `asr` is asked only when `words.json` is absent and `diarize` only when `turns.json` is; `align` carries S4's answer (`None` when [`crate::services::pick_aligner`] finds none) as a closure, since what it places is THIS pass's word list. A test hands closures.
pub fn run<F, D, A>(
    tree: &Tree,
    source: &str,
    duration: f64,
    model: &str,
    align: A,
    asr: F,
    diarize: D,
) -> Result<Passage, String>
where
    F: FnOnce() -> Result<WordsDoc, String>,
    D: FnOnce() -> Result<Vec<Turn>, String>,
    A: FnOnce(&WordsDoc) -> Align,
{
    let mut logs = vec![audio_log(source, duration)];

    // S2: a take shorter than `P.policy.minTakeSeconds` is written up as silence and no server is asked, so later steps resume.
    let short = is_short_take(duration);
    if short {
        logs.push(short_take_log(source, duration));
        requests::write_silence(tree, source)?;
    }

    // S3: `words.json` is the resume marker by existence, not contents; an ASR that heard nothing writes empty text and would otherwise be re-asked forever.
    let mut words = requests::read_words(tree, source)?;
    match words {
        Some(_) => logs.push(asr_done_log(source)),
        None if short => {}
        None => {
            logs.push(asr_log(source, model));
            words = Some(asr()?);
            // Written last and whole: it is the resume marker.
            requests::write_words(tree, source, &words.clone().unwrap_or_default())?;
        }
    }
    let words = words.unwrap_or_default();

    let already = tree.words_aligned_json(source).exists();
    let align = if short || already {
        Align::None
    } else {
        align(&words)
    };
    match align_step(source, align, &words, already) {
        AlignStep::Warns(warning) => logs.push(warning),
        AlignStep::Stops(err) => return Err(align_failure(model, &err)),
        AlignStep::Quiet | AlignStep::Aligned => {}
    }

    // S5: diarization unless its answer is there (existence; `[]` means nobody spoke). Failure is fatal on its own (the window ladder is F1.5's).
    let mut turns = requests::read_turns(tree, source)?;
    if !tree.turns_json(source).exists() && !short {
        turns = diarize()?;
        write_turns(tree, source, &turns)?;
        // §5 bullet 1: the diarizer's scratch goes with its answer; an absent folder is no error.
        let _ = std::fs::remove_dir_all(tree.diar_scratch(source));
    }

    let aligned = requests::read_aligned(tree, source)?.unwrap_or_default();
    let timed: &[Word] = if aligned.words.is_empty() {
        &words.words
    } else {
        &aligned.words
    };
    let segments = segments(timed, &glue_turns(&turns));
    write_transcript(tree, source, &segments)?;
    logs.push(segments_log(source, segments.len()));
    Ok(Passage { logs, segments })
}

/// S4 as one decision, so its four branches and wording are in one place. `source` only appears in the warning.
pub fn align_step(source: &str, align: Align, words: &WordsDoc, already: bool) -> AlignStep {
    // Its answer on disk is the starting point whatever the aligner would say; asking again redoes resumable work.
    if already {
        return AlignStep::Quiet;
    }
    let outcome = align_outcome(align.err(), !words.words.is_empty(), &words.text);
    match outcome {
        Alignment::Aligned => AlignStep::Aligned,
        Alignment::Skipped => AlignStep::Quiet,
        Alignment::Stood => {
            AlignStep::Warns(align_warning(source, align.err().unwrap_or_default()))
        }
        Alignment::Fatal(err) => AlignStep::Stops(err),
    }
}

fn write_turns(tree: &Tree, source: &str, turns: &[Turn]) -> Result<(), String> {
    let path = tree.turns_json(source);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let body = serde_json::to_string(turns).map_err(|err| err.to_string())?;
    std::fs::write(&path, body).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(())
}

pub fn is_short_take(duration: f64) -> bool {
    requests::is_short(duration, requests::MIN_TAKE_SECONDS)
}

/// Whether this source's audio is decoded yet (S1): mono 16 kHz `voice16k.wav`.
pub fn has_audio(tree: &Tree, source: &str) -> bool {
    tree.voice16k_wav(source).exists()
}

/// A word's span as segments use it: end clamped to [`MERGE_MAX_WORD`] past its start. Public so anything showing words shows the span Cut reads.
pub fn word_span(word: &Word) -> (f64, f64) {
    let start = seconds(word.start_sample);
    (start, seconds(word.end_sample).min(start + MERGE_MAX_WORD))
}
