//! Which job asks which server, spec/02-services.md §2.
//!
//! The table of fourteen jobs: who answers each, whether the model thinks first, whether it gets the web, and what its answer may change. Lookups and small rules;
//! no socket or file is read here (calls belong to §6's client, prompts to §3).

use crate::project::{CutMode, MarkingPass};
use crate::services::{AudioTask, Server};

/// ASR chunk for the qwen3 family. P.machine.asrChunkQwenSeconds
pub const ASR_CHUNK_QWEN_SECONDS: f64 = 60.0;

/// ASR chunk for every other model. P.machine.asrChunkMaxSeconds
pub const ASR_CHUNK_MAX_SECONDS: f64 = 300.0;

/// Shortest ASR chunk: an out-of-memory answer halves the chunk down to here (300 → 150 → 75 → 37.5 → 20). P.machine.asrChunkMinSeconds
pub const ASR_CHUNK_MIN_SECONDS: f64 = 20.0;

/// The diarization window ladder, biggest first (measured: 90 passes, 150 fails).
/// P.machine.diarWindowsSeconds
pub const DIAR_WINDOWS_SECONDS: [f64; 3] = [90.0, 45.0, 25.0];

/// Frames per vision request — "four frames a call". P.machine.describeFramesPerReq
pub const VISION_FRAMES_PER_CALL: u32 = 4;

/// Pass 1's stride as a share of the diarization window: windows overlap by a third. P.machine.diarHopShare
pub const DIAR_HOP_SHARE: (u32, u32) = (2, 3);

/// Seconds of each voice in the anchor at the biggest window; the lower rungs of
/// [`DIAR_WINDOWS_SECONDS`] take a quarter of their own window instead. P.machine.anchorPerSeconds
pub const ANCHOR_PER_SECONDS: f64 = 12.0;

/// Identical retake calls pooled. P.machine.retakeRuns
pub const RETAKE_RUNS_POOLED: u32 = 3;

/// Thinking for the join pass (measured: 15/29 → 20/29 joins right). P.machine.textEditThinking
pub const TEXT_EDIT_THINKING: bool = true;

/// Validation attempts per job (cut, narrate, upload text). P.eng.llmAttempts
pub const LLM_ATTEMPTS: u32 = 3;

/// Tool rounds per call; beyond it the step gets no answer from the model (09 §2).
/// P.eng.llmToolRounds
pub const TOOL_ROUNDS: u32 = 8;

/// One row of §2's table, in the order the spec lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    /// ASR on audio.cpp: text per chunk.
    Asr,
    /// The aligner on audio.cpp: start/end per word, the chunk's own words in hand.
    Align,
    /// Diarization on audio.cpp: speaker turns per window.
    Diarize,
    /// Separation on audio.cpp: voice and rest stems.
    Separate,
    /// What is on screen: one EVENT per frame plus a running STATE.
    Describe,
    /// The same lines respelled; times and speakers unchanged, enforced.
    CleanTranscript,
    /// markingPass joins: the two takes run on as one, only deletions accepted.
    Joins,
    /// markingPass retakes: line numbers of abandoned stretches.
    Retakes,
    /// cutMode model: segments in session seconds copied off the timeline.
    ModelCut,
    /// Captions, speed and effects: per clip, in the clip's own seconds.
    ClipRules,
    /// A line per clip with an emotion, then spoken in the cloned voice.
    Narrate,
    /// Title, thumbnail (frame or instruction), description.
    UploadText,
    /// An image edited from real frames.
    Thumbnail,
    /// Numbered lines translated, one call per language.
    Subtitles,
}

impl Job {
    /// The fourteen jobs in the spec's order; a fifteenth row must be added here and described in the table test.
    pub fn all() -> [Job; 14] {
        [
            Job::Asr,
            Job::Align,
            Job::Diarize,
            Job::Separate,
            Job::Describe,
            Job::CleanTranscript,
            Job::Joins,
            Job::Retakes,
            Job::ModelCut,
            Job::ClipRules,
            Job::Narrate,
            Job::UploadText,
            Job::Thumbnail,
            Job::Subtitles,
        ]
    }
}

/// Which of the four servers answers this job (ffmpeg is a subprocess and asks nobody).
pub fn server(job: Job) -> Server {
    match job {
        Job::Asr | Job::Align | Job::Diarize | Job::Separate => Server::Audio,
        Job::Thumbnail => Server::Image,
        _ => Server::Llm,
    }
}

/// Whether this job asks the model to think first.
///
/// The four audio jobs and the thumbnail carry `–` in §2's thinking column: not applicable (no thinking token to set), so false.
pub fn thinking(job: Job) -> bool {
    match job {
        // P.machine.textEditThinking: joins are the measured case where reasoning pays.
        Job::Joins => TEXT_EDIT_THINKING,
        Job::ModelCut | Job::Narrate | Job::UploadText => true,
        _ => false,
    }
}

/// Whether this job gets the web tools — the three rows §2 marks `(+ web tools)`.
pub fn web_tools(job: Job) -> bool {
    matches!(job, Job::ModelCut | Job::Narrate | Job::UploadText)
}

/// Which audio model answers, for the four jobs that ask for one.
pub fn audio_task(job: Job) -> Option<AudioTask> {
    match job {
        Job::Asr => Some(AudioTask::Asr),
        Job::Align => Some(AudioTask::Align),
        Job::Diarize => Some(AudioTask::Diar),
        Job::Separate => Some(AudioTask::Sep),
        _ => None,
    }
}

/// Whether this job's answer is spoken after the model gives it (§2's `→ TTS` arrow).
///
/// Narrate is the only row with the arrow. The thumbnail is an image drawn by sd.cpp, so nothing follows it.
pub fn answers_after_tts(job: Job) -> bool {
    matches!(job, Job::Narrate)
}

/// The family string that means the short ASR chunk (qwen3: 60 s, else 300). Matched case-insensitively as a substring, since it is whatever the server declared (§1's `AudioModel::family`).
pub fn asr_chunk_is_qwen_family(family: &str) -> bool {
    family.to_ascii_lowercase().contains("qwen3")
}

/// How long an ASR chunk may be after `oom_retries` out-of-memory answers.
///
/// Each halves the chunk down to the 20 s floor: shorter than a sentence transcribes worse than it would have failed.
pub fn asr_chunk_seconds(family: &str, oom_retries: u32) -> f64 {
    let start = if asr_chunk_is_qwen_family(family) {
        ASR_CHUNK_QWEN_SECONDS
    } else {
        ASR_CHUNK_MAX_SECONDS
    };
    // Halved by doubling the divisor, not a loop: no way to spin on a huge retry count.
    (start / (1u32 << oom_retries.min(16)) as f64).max(ASR_CHUNK_MIN_SECONDS)
}

pub fn diar_ladder() -> [f64; 3] {
    DIAR_WINDOWS_SECONDS
}

/// The window for attempt `attempt`, counting from 0.
///
/// Past the end of the ladder the smallest window repeats: the window only shrinks.
pub fn diar_window(attempt: usize) -> f64 {
    DIAR_WINDOWS_SECONDS
        .get(attempt)
        .copied()
        .unwrap_or(DIAR_WINDOWS_SECONDS[DIAR_WINDOWS_SECONDS.len() - 1])
}

/// How many vision calls `frames` frames take: four a call.
pub fn vision_batches(frames: usize) -> usize {
    let per_call = VISION_FRAMES_PER_CALL as usize;
    frames / per_call + usize::from(!frames.is_multiple_of(per_call))
}

/// Whether this job runs under this project's policy (§5, `P.policy.markingPass` and
/// `P.policy.cutMode`).
///
/// joins only runs under the joins marking pass, retakes under retakes, the model cut under the model mode; `MarkingPass::None` asks for neither marking job.
pub fn job_applies(job: Job, marking_pass: MarkingPass, cut_mode: CutMode) -> bool {
    match job {
        Job::Joins => marking_pass == MarkingPass::Joins,
        Job::Retakes => marking_pass == MarkingPass::Retakes,
        Job::ModelCut => cut_mode == CutMode::Model,
        _ => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutAttempt {
    /// 1, 2 or 3 — §2's "three attempts".
    pub attempt: u32,
    /// Web tools ride the first attempt only.
    pub web_tools: bool,
    /// Thinking, off for a retry that reasoned and wrote nothing.
    pub thinking: bool,
}

/// Whether thinking stays on for the next cut attempt.
///
/// A model that filled the reasoning block and produced no answer will not do better reasoning again, so the retry asks for the answer alone.
/// Any other failure keeps thinking on: that answer was written, it was just wrong.
pub fn cut_retry_thinking(previous_reasoned: bool, previous_answer_empty: bool) -> bool {
    !(previous_reasoned && previous_answer_empty)
}

/// How the model-cut attempt number `attempt` (counting from 1) is sent.
///
/// Web tools are withdrawn after the first rejected attempt: they cost a round each, and later attempts copy segments off the timeline already given.
pub fn cut_attempt(attempt: u32) -> CutAttempt {
    CutAttempt {
        attempt,
        web_tools: attempt == 1,
        // Attempt 1 always thinks; a later one follows `cut_retry_thinking`, which callers set this from.
        thinking: true,
    }
}

/// Whether attempt `attempt` was the last one allowed.
pub fn cut_attempts_exhausted(attempt: u32) -> bool {
    attempt >= LLM_ATTEMPTS
}

/// How many identical retake calls are pooled into one (§2's "three runs pooled").
pub fn retake_runs_pooled() -> u32 {
    RETAKE_RUNS_POOLED
}

/// Calls needed for `calls` distinct stretches.
///
/// Pooling is about repeats, not volume: three identical calls become one, N different stay N, so this equals the count of distinct stretches.
pub fn retake_batches(calls: usize) -> usize {
    calls
}

/// Whether sd.cpp is asked for a thumbnail.
///
/// §2: "only when an instruction exists and no frame was named": a named frame is cut out of the source and needs no server.
pub fn thumbnail_wanted(instruction: &str, frame_named: bool) -> bool {
    !instruction.trim().is_empty() && !frame_named
}

/// One language from `SUBTITLE_LANGUAGES`: `code`, ISO-639-2 `tag` and `name`
/// (00-principles.md §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub code: String,
    pub tag: String,
    pub name: String,
}

impl Language {
    /// What the translation call is asked for: the name (what a prompt reads as a language); code and tag are for filenames (`<stem>.de.srt`, 08 §7).
    pub fn ask_for(&self) -> &str {
        &self.name
    }
}

/// The settings list of subtitle languages, in the order written.
///
/// Entries are comma-separated, fields colon-separated (03-shell.md §3, "code:tag:name list"). A field is trimmed, a shorter entry fills what it
/// left out from the code, and an entry with no code is dropped.
pub fn subtitle_languages(raw: &str) -> Vec<Language> {
    let mut out = Vec::new();
    for entry in raw.split(',') {
        let fields: Vec<&str> = entry.split(':').map(str::trim).collect();
        let code = fields.first().copied().unwrap_or_default();
        if code.is_empty() {
            continue;
        }
        let field = |i: usize| {
            fields
                .get(i)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
                .unwrap_or_else(|| code.to_string())
        };
        out.push(Language {
            code: code.to_string(),
            tag: field(1),
            name: field(2),
        });
    }
    out
}

/// The languages known with no `SUBTITLE_LANGUAGES`: the prototype's three (`subLangs`) as ISO-639-1 code, ISO-639-2 tag and name.
pub const BUILT_IN_LANGUAGES: [(&str, &str, &str); 3] = [
    ("en", "eng", "English"),
    ("de", "deu", "German"),
    ("fr", "fra", "French"),
];

/// The settings list, or the built-in three when it names none: an empty `SUBTITLE_LANGUAGES` is an unfilled file, not a wish for untagged tracks.
pub fn languages_or_built_in(raw: &str) -> Vec<Language> {
    let listed = subtitle_languages(raw);
    if !listed.is_empty() {
        return listed;
    }
    BUILT_IN_LANGUAGES
        .iter()
        .map(|(code, tag, name)| Language {
            code: code.to_string(),
            tag: tag.to_string(),
            name: name.to_string(),
        })
        .collect()
}

/// The code a language goes by, given its code or name (`German` and `de` both answer `de`): the page ticks by name, the policy holds codes; an unknown language is kept as given.
pub fn language_code(given: &str) -> String {
    let given = given.trim();
    BUILT_IN_LANGUAGES
        .iter()
        .find(|(code, _, name)| {
            code.eq_ignore_ascii_case(given) || name.eq_ignore_ascii_case(given)
        })
        .map_or_else(|| given.to_string(), |(code, _, _)| code.to_string())
}

/// The name a language is asked for and said by (`de` answers `German`); an unknown one keeps its code.
pub fn language_name(code: &str) -> String {
    BUILT_IN_LANGUAGES
        .iter()
        .find(|(known, _, _)| known.eq_ignore_ascii_case(code.trim()))
        .map_or_else(|| code.trim().to_string(), |(_, _, name)| name.to_string())
}

/// How many translation calls `languages` takes: one per language (§2).
pub fn subtitle_calls(languages: &[&str]) -> usize {
    languages.len()
}

/// The respelled transcript, with the times and speakers of the original.
///
/// §2: "times and speakers unchanged, enforced": this never reads a time or speaker out of `respelled`. It also refuses a reply whose line count differs,
/// since pairing words with other lines' times reads like a success.
pub fn clean_transcript<T: Clone>(
    originals: &[(f64, f64, String, T)],
    respelled: &[String],
) -> Option<Vec<(f64, f64, String, String)>> {
    if respelled.len() != originals.len() {
        return None;
    }
    Some(
        originals
            .iter()
            .zip(respelled)
            .map(|((start, end, speaker, _), text)| (*start, *end, speaker.clone(), text.clone()))
            .collect(),
    )
}
