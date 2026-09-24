//! One source read: audio → text → word times → speakers → segments
//! (spec/04-prepare.md F1.3).
//!
//! What leaves here is two ordinary files — `transcript.tsv` and `transcript.srt` — and the sidecars
//! that let a re-run resume, so nothing downstream learns that an ASR, an aligner and a diarizer ever
//! existed. Like [`crate::separate`], this module spawns no process and opens no socket: the decode's
//! length (S1), the ASR answer (S3), whether an aligner is served (S4) and the turns (S5) arrive as
//! arguments, because a step that decides inside a server call cannot be tested at all
//! (spec/00-principles.md §5).

use crate::layout::Tree;
use crate::requests::{self, Turn, Word, WordsDoc};
use crate::textfmt::{self, Line};

/// S1: the sample rate `voice16k.wav` is written at, and therefore the rate every sidecar's samples
/// are counted in. One constant owns the conversion so a word's time cannot be read at two rates.
pub const SAMPLE_RATE: u64 = 16_000;

/// S6: a silence this long ends a segment (`mergeGap`). Subtitles the viewer reads come out of it.
pub const MERGE_GAP: f64 = 0.7;

/// S6: the hard cap on one segment (`mergeMaxLen`), however unbroken the speech is.
pub const MERGE_MAX_LEN: f64 = 12.0;

/// S6: how long a word may be (`mergeMaxWord`). Parakeet stretches a word's end across the silence
/// after it, and an unstretched "the" nine seconds wide would hold its segment open forever.
pub const MERGE_MAX_WORD: f64 = 2.0;

/// S6: how far away a turn may be and still own a word (`mergeNear`) — diarization's edges are not
/// exact, so a word just outside every turn belongs to the nearest one rather than to nobody.
pub const MERGE_NEAR: f64 = 1.0;

/// S5/S6: same-speaker turns closer than this are one turn (`diarTurnGap`). Gluing them first is what
/// stops a speaker who paused from being two people for one word each.
///
/// §10 files it as `P.eng.turnGapSeconds` (0.5, "gap that ends a turn"; prototype `diarTurnGap`,
/// gui/pipeline.go), and `params::prepare()` rows it from this constant. The bound is INCLUSIVE
/// (`<=`): exactly this much of a gap still leaves one turn, so a breath of precisely 0.5 s does not
/// end it. Two further things the comparison in `glue_turns` assumes: it looks only at the LAST kept
/// turn, so the diarizer's answer must arrive in time order — which it does, and is why gluing is
/// idempotent over what was already written; and a later turn ending before the kept one does not
/// shorten it (`end > previous.1`), so a turn keeps the widest span it has been shown.
pub const TURN_GLUE: f64 = 0.5;

/// S6: a word no turn covers and none is near. Kept as the running speaker while a segment is open,
/// so it never reaches the timeline as somebody's name.
pub const UNKNOWN: &str = "?";

/// S6: the one voice of a recording diarization found no voices in — `turns.json` says `[]`, yet the
/// ASR heard words. Named the way a diarized session names its first speaker, because "?" would reach
/// the timeline as a name.
pub const FIRST_SPEAKER: &str = "SPEAKER_00";

/// A sample of `voice16k.wav` as seconds.
pub fn seconds(sample: u64) -> f64 {
    sample as f64 / SAMPLE_RATE as f64
}

/// S1: the audio is on disk and how long it is, said before anything is asked of a server.
pub fn audio_log(base: &str, secs: f64) -> String {
    format!(">>> [{base}] {secs:.1} s of audio")
}

/// S2 (`P.policy.minTakeSeconds`): the shorter thing is a start/stop rather than a take, and three
/// servers are not worth asking about it.
// P.policy.minTakeSeconds
pub fn short_take_log(base: &str, secs: f64) -> String {
    format!(">>> [{base}] {secs:.1} s long -- a start/stop, not a take: written up as silence")
}

/// S3: the ASR is being asked. Named with the model, because which one answered changes what came
/// back and the log is where that is recoverable from.
pub fn asr_log(base: &str, model: &str) -> String {
    format!(">>> [{base}] ASR ({model})")
}

/// S3: `words.json` is this stage's resume marker, so its presence means the answer is on disk.
pub fn asr_done_log(base: &str) -> String {
    format!(">>> [{base}] ASR already done")
}

/// S4: an aligner that failed while the ASR timed its own words is a loss of precision, not of the
/// recording — so it is a warning and the run carries on.
pub fn align_warning(base: &str, err: &str) -> String {
    format!("!!! [{base}] align: {err} -- the ASR's own times stand")
}

/// S4: an aligner that failed with nothing else to time the words by is a dead end, and it is named
/// by the model that could not do it — "a missing aligner" would send the user to the wrong setting.
pub fn align_failure(model: &str, err: &str) -> String {
    format!(
        "align: {err}\n\n{model} answers with no word times of its own, so the aligner is the only \
         thing that can time them and this recording cannot be transcribed without it"
    )
}

/// S6: what the two files hold.
pub fn segments_log(base: &str, count: usize) -> String {
    format!(">>> [{base}] {count} segments")
}

/// S4: how an alignment attempt ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Alignment {
    /// The aligner's times are on disk and are the ones used.
    Aligned,
    /// Nothing was aligned and nothing was lost: either `words.aligned.json` was already there — the
    /// resume case, which is why it is not an error path — or there were no words to align at all.
    Skipped,
    /// It failed, and the ASR's own times stand: a warning, then on with the run.
    Stood,
    /// It failed and nothing else can time the words. The string is the sentence to show.
    Fatal(String),
}

/// S4: what an alignment failure means, which depends entirely on whether anything else can time the
/// words. `err` is `None` when the aligner answered; `asr_has_word_times` is whether the ASR document
/// carries times of its own; `transcript` is `transcript.txt` as it stands.
pub fn align_outcome(err: Option<&str>, asr_has_word_times: bool, transcript: &str) -> Alignment {
    let Some(err) = err else { return Alignment::Aligned };
    if asr_has_word_times {
        return Alignment::Stood;
    }
    if transcript.trim().is_empty() {
        // Nothing to align: a recording with no words in it is not an aligner's failure. Said nowhere
        // — the silence case has its own line, and two lines about one nothing would read as trouble.
        return Alignment::Skipped;
    }
    Alignment::Fatal(err.to_string())
}

/// S5/S6: the turns as segments want them — seconds, and speakers named the way a diarized session
/// names them (`SPEAKER_00`, `SPEAKER_01`, …). Same-speaker turns closer than [`TURN_GLUE`] are one
/// turn: the diarizer splits where a speaker breathed, and a word between two halves of one sentence
/// would otherwise be given to two people. Gluing is idempotent, so it is done over what the diarizer
/// wrote whether or not that was this run.
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

/// S6: who said a word. The turn it shares the most time with wins — that is what diarization is
/// for. Failing that, the nearest turn within [`MERGE_NEAR`], because diarization's edges are not
/// exact and a word just outside every turn still belongs to somebody. With no turns at all the
/// recording has one voice ([`FIRST_SPEAKER`]); `UNKNOWN` is for a word in the space between turns,
/// which is a gap in what was measured rather than an unperson.
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

/// S6: words and turns become the segments the two transcript files hold. A segment ends at a speaker
/// change, at a silence over [`MERGE_GAP`], or when it would pass [`MERGE_MAX_LEN`] — those are what
/// make subtitles readable rather than one long line. Word ends come in clamped to
/// [`MERGE_MAX_WORD`] before anything else is decided, so the stretch an ASR puts across a pause can
/// neither hold a segment open nor invent a gap.
pub fn segments(words: &[Word], turns: &[(f64, f64, String)]) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut previous_end = 0.0;
    for word in words {
        let start = seconds(word.start_sample);
        let end = seconds(word.end_sample).min(start + MERGE_MAX_WORD);
        let mut speaker = speaker_of(start, end, turns);

        // A word nobody could place keeps the running speaker: it is the same voice that was speaking
        // a moment ago, and "?" would be shown as somebody's name. Borrowed while the open segment is
        // not, since taking it from there and then writing to it is one borrow too many.
        let unnamed = out.last().is_some_and(|current| current.speaker == UNKNOWN);
        if speaker == UNKNOWN && !unnamed {
            if let Some(current) = out.last() {
                speaker = current.speaker.clone();
            }
        }
        let open = out.last_mut();
        let breaks = open.as_ref().is_some_and(|current| {
            speaker != current.speaker || start - previous_end > MERGE_GAP || end - current.start > MERGE_MAX_LEN
        });
        if breaks {
            out.push(Line {
                start,
                end,
                speaker: speaker.clone(),
                text: word.word.clone(),
            });
        } else if let Some(current) = open {
            // A segment that opened on an unplaceable word is named by the first word that could be
            // placed — that is who was speaking when it started.
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

/// S6: `transcript.srt`. One block per segment — number, times, then the text with the speaker in
// brackets in front of it, which is how a viewer tells who is talking. An unnamed speaker gets no
/// tag rather than a bracketed question mark.
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

/// A subtitle timecode: `HH:MM:SS,mmm`. Negative times clamp to zero — a word whose end was clamped
/// below its start is corrupt input, and a negative timecode would not parse downstream.
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

/// S6: both transcript files, over the same segments. The tsv is what Cut reads and the srt is what a
/// person reads; writing them from one list is what keeps them agreeing.
pub fn write_transcript(tree: &Tree, source: &str, lines: &[Line]) -> Result<(), String> {
    textfmt::write_lines(lines, &tree.transcript_tsv(source))?;
    // The same writer as the tsv, so both files carry §6's 0644 and neither is half a file.
    textfmt::write_rows(&tree.transcript_srt(source), &srt_text(lines))
}

/// S6: the words an answer sent back that cannot be placed on the clock. A word with no usable
/// `start_sample`/`end_sample` is not a word with an unknown time — it is the answer having changed
/// shape, and every cut point downstream is put on a word edge, so guessing here would move the video.
pub fn unusable_words(words: &[Word]) -> usize {
    words
        .iter()
        .filter(|word| word.end_sample <= word.start_sample)
        .count()
}

/// S6: refusing a transcript whose words have no usable times, naming how many — "some" would leave a
/// reader counting the file by hand.
pub fn unusable_words_error(count: usize) -> String {
    format!("{count} words carry no usable start_sample/end_sample -- the answer changed shape")
}

/// S4/S6: the recording was transcribed but nothing in it has a time, and there is no aligner to give
/// them. Named by the model that answered, because the fix is a setting: register an aligner, or use an
/// ASR that answers with word timings. The variant is for when an aligner *was* asked and left no times
/// either — why is what the warning logged directly above this line says, so this one points at it
/// instead of repeating it.
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

/// What one source's pass wrote down, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Passage {
    pub logs: Vec<String>,
    pub segments: Vec<Line>,
}

/// What the aligner did, as far as this pass is concerned. Spelled out rather than passed as an
/// `Option<Result>` because "no aligner served" and "nothing to align" both answer `None` and are not
/// the same decision: one asks for nothing, the other asks and gives up quietly.
pub enum Align {
    /// No aligner is served — [`crate::services::pick_aligner`] found none on the audio box.
    None,
    /// It answered and `words.aligned.json` is on disk now.
    Done,
    /// It failed, with the server's own sentence.
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

/// The outcome of the alignment step (S4) as the caller has to see it.
pub enum AlignStep {
    /// Nothing was aligned and nothing was lost — no aligner served, or its answer already on disk.
    Quiet,
    /// The aligner answered and `words.aligned.json` is on disk.
    Aligned,
    /// It failed and the ASR's own times stand: the warning to log, then on with the run.
    Warns(String),
    /// It failed and nothing else can time the words: the sentence to show, and the pass stops.
    Stops(String),
}

/// S1-S6 for one source. `duration` is the length of `voice16k.wav`, which the caller decoded (S1) —
/// this module reads files and decides, it does not run ffmpeg. `asr` is asked only when `words.json`
/// is absent and `diarize` only when `turns.json` is; `align` carries S4's answer, `None` when no
/// aligner is served at all (what [`crate::services::pick_aligner`] decides from the model list). The
/// real ▶ hands over ffmpeg, the ASR box and the diarizer; a test hands over closures, which is the
/// whole reason these steps can be tested rather than exercised.
pub fn run<F, D>(
    tree: &Tree,
    source: &str,
    duration: f64,
    model: &str,
    align: Align,
    asr: F,
    diarize: D,
) -> Result<Passage, String>
where
    F: FnOnce() -> Result<WordsDoc, String>,
    D: FnOnce() -> Result<Vec<Turn>, String>,
{
    let mut logs = vec![audio_log(source, duration)];

    // S2: a take shorter than `P.policy.minTakeSeconds` is written up as silence and nothing is asked
    // of any server — the same files an empty recording gets, so every later step resumes rather than
    // re-asking about a third of a second of nothing.
    let short = is_short_take(duration);
    if short {
        logs.push(short_take_log(source, duration));
        requests::write_silence(tree, source)?;
    }

    // S3: `words.json` is this stage's resume marker — the file's existence, not its contents. An
    // ASR that heard nothing writes a document with empty text and no words, and reading that as
    // "not yet done" would ask the server again on every run forever.
    let mut words = requests::read_words(tree, source)?;
    match words {
        Some(_) => logs.push(asr_done_log(source)),
        None if short => {}
        None => {
            logs.push(asr_log(source, model));
            words = Some(asr()?);
            // Written last and whole: it is this stage's resume marker, so nothing may be missing
            // once it exists.
            requests::write_words(tree, source, &words.clone().unwrap_or_default())?;
        }
    }
    let words = words.unwrap_or_default();

    // S4: alignment, when an aligner is served and its answer is not already on disk.
    match align_step(source, align, &words, tree.words_aligned_json(source).exists()) {
        AlignStep::Warns(warning) => logs.push(warning),
        AlignStep::Stops(err) => return Err(align_failure(model, &err)),
        AlignStep::Quiet | AlignStep::Aligned => {}
    }

    // S5: diarization, unless its answer is already there — again the file's existence, since `[]` is
    // what "nobody spoke" is written as. Its failure is fatal on its own: a session with no turns has
    // to be an empty one, not a server that gave up (the window ladder that gets it there is F1.5's).
    let mut turns = requests::read_turns(tree, source)?;
    if !tree.turns_json(source).exists() && !short {
        turns = diarize()?;
        write_turns(tree, source, &turns)?;
    }

    // S6: the segments, from the aligner's times when they exist and the ASR's otherwise.
    let aligned = requests::read_aligned(tree, source)?.unwrap_or_default();
    let timed: &[Word] = if aligned.words.is_empty() { &words.words } else { &aligned.words };
    let segments = segments(timed, &glue_turns(&turns));
    write_transcript(tree, source, &segments)?;
    logs.push(segments_log(source, segments.len()));
    Ok(Passage { logs, segments })
}

/// S4 as one decision, so its four branches are in one place and their wording is not spread across
/// the driver. `source` only appears in the warning's line.
pub fn align_step(source: &str, align: Align, words: &WordsDoc, already: bool) -> AlignStep {
    // Its answer on disk is this pass's starting point, whatever the aligner would have said: asking
    // again would be re-doing work whose result is already in the file that makes a run resumable.
    if already {
        return AlignStep::Quiet;
    }
    let outcome = align_outcome(align.err(), !words.words.is_empty(), &words.text);
    match outcome {
        Alignment::Aligned => AlignStep::Aligned,
        Alignment::Skipped => AlignStep::Quiet,
        Alignment::Stood => AlignStep::Warns(align_warning(source, align.err().unwrap_or_default())),
        Alignment::Fatal(err) => AlignStep::Stops(err),
    }
}

/// Write `turns.json` the way the diarizer leaves it.
fn write_turns(tree: &Tree, source: &str, turns: &[Turn]) -> Result<(), String> {
    let path = tree.turns_json(source);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let body = serde_json::to_string(turns).map_err(|err| err.to_string())?;
    std::fs::write(&path, body).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(())
}

/// S2's question, spelled out so the parameter is named where it is used.
// P.policy.minTakeSeconds
pub fn is_short_take(duration: f64) -> bool {
    requests::is_short(duration, requests::MIN_TAKE_SECONDS)
}

/// Whether this source's audio has been decoded yet (S1). The decode itself is ffmpeg's, at mono
/// 16 kHz — `voice16k.wav` is what every sidecar's samples are counted in.
pub fn has_audio(tree: &Tree, source: &str) -> bool {
    tree.voice16k_wav(source).exists()
}

/// A word's span as the segments use it: its end clamped to [`MERGE_MAX_WORD`] past its start. Public
/// because anything that shows words has to show the same span Cut reads, or the two disagree on
/// screen.
pub fn word_span(word: &Word) -> (f64, f64) {
    let start = seconds(word.start_sample);
    (start, seconds(word.end_sample).min(start + MERGE_MAX_WORD))
}
