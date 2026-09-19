//! Which job asks which server, spec/02-services.md §2.
//!
//! The table of fourteen jobs: who answers each one, whether the model is asked to think first,
//! whether it gets the web, and what its answer is allowed to change. Everything here is a
//! lookup or a small rule — the calls themselves belong to §6's client and the prompts to §3, so
//! this module holds only the decisions a request is built from, in one place a test can pin.
//!
//! Nothing here reads a socket or a file: [`Job`] plus the settings read by
//! [`crate::services::endpoint_for`] is enough for a caller to build a request.

use crate::project::{CutMode, MarkingPass};
use crate::services::{AudioTask, Server};

/// ASR chunk for the qwen3 family. P.machine.asrChunkQwenSeconds
pub const ASR_CHUNK_QWEN_SECONDS: f64 = 60.0;

/// ASR chunk for every other model. P.machine.asrChunkMaxSeconds
pub const ASR_CHUNK_MAX_SECONDS: f64 = 300.0;

/// Shortest ASR chunk: an out-of-memory answer halves the chunk down to here and no further.
/// Halving from 300 reaches it (300 → 150 → 75 → 37.5 → 20), so the floor is a real chunk size
/// rather than a guard against an infinite loop. P.machine.asrChunkMinSeconds
pub const ASR_CHUNK_MIN_SECONDS: f64 = 20.0;

/// The diarization window ladder, biggest first (measured: 90 passes, 150 fails).
/// P.machine.diarWindowsSeconds
pub const DIAR_WINDOWS_SECONDS: [f64; 3] = [90.0, 45.0, 25.0];

/// Frames per vision request — "four frames a call". P.machine.describeFramesPerReq
pub const VISION_FRAMES_PER_CALL: u32 = 4;

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
    /// The fourteen jobs, in the spec's order — so a fifteenth row has to be added here and the
    /// table test fails until it is described there too.
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

/// Which of the four servers answers this job (§1's three plus sd.cpp; ffmpeg is a subprocess and
/// asks nobody).
pub fn server(job: Job) -> Server {
    match job {
        Job::Asr | Job::Align | Job::Diarize | Job::Separate => Server::Audio,
        Job::Thumbnail => Server::Image,
        _ => Server::Llm,
    }
}

/// Whether this job asks the model to think first.
///
/// The four audio jobs and the thumbnail carry `–` in §2's thinking column rather than `off`.
/// Read as "the question does not apply": an audio.cpp task call and an sd.cpp job have no
/// thinking token to set, so this returns false for them — a caller that sent one anyway would be
/// inventing a field the server never reads.
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

/// Which audio model answers, when the job is one of the four that asks for one. The rest are a
/// server-side LLM or sd.cpp call with no audio model id to send.
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
/// Narrate is the only row with that arrow. The thumbnail is the only job whose whole answer is
/// an image, and it is asked only under [`thumbnail_wanted`] — but sd.cpp draws it, so nothing
/// follows it.
pub fn answers_after_tts(job: Job) -> bool {
    matches!(job, Job::Narrate)
}

/// The family string that means the short ASR chunk: the qwen3 family caps a chunk at 60 s where
/// everything else may run to 300. Matched case-insensitively as a substring because the value is
/// whatever the server declared in its own model list (§1's `AudioModel::family`), so a build that
/// names it `Qwen3-ASR` or `qwen3-asr-1.7b` still gets the short chunk.
pub fn asr_chunk_is_qwen_family(family: &str) -> bool {
    family.to_ascii_lowercase().contains("qwen3")
}

/// How long an ASR chunk may be after `oom_retries` out-of-memory answers.
///
/// Each one halves the chunk and the floor is 20 s: a server that cannot answer at 20 s has no
/// chunk size left to be told about, and cutting further would produce chunks shorter than a
/// sentence, which transcribe worse than they would have failed.
pub fn asr_chunk_seconds(family: &str, oom_retries: u32) -> f64 {
    let start = if asr_chunk_is_qwen_family(family) {
        ASR_CHUNK_QWEN_SECONDS
    } else {
        ASR_CHUNK_MAX_SECONDS
    };
    // Halved by repeated doubling of the divisor rather than a loop: same result, and no way to
    // spin on a huge retry count.
    (start / (1u32 << oom_retries.min(16)) as f64).max(ASR_CHUNK_MIN_SECONDS)
}

/// The diarization windows, biggest first.
pub fn diar_ladder() -> [f64; 3] {
    DIAR_WINDOWS_SECONDS
}

/// The window for attempt `attempt`, counting from 0.
///
/// Past the end of the ladder the smallest window repeats: the window only ever shrinks, so a
/// fourth attempt re-tries the one that fits rather than inventing a size nobody measured.
pub fn diar_window(attempt: usize) -> f64 {
    DIAR_WINDOWS_SECONDS
        .get(attempt)
        .copied()
        .unwrap_or(DIAR_WINDOWS_SECONDS[DIAR_WINDOWS_SECONDS.len() - 1])
}

/// How many vision calls `frames` frames take: four a call.
pub fn vision_batches(frames: usize) -> usize {
    let per_call = VISION_FRAMES_PER_CALL as usize;
    // Integer ceil without the +per-1 trick reading like an off-by-one at a glance.
    frames / per_call + usize::from(frames % per_call != 0)
}

/// Whether this job runs under this project's policy (§5, `P.policy.markingPass` and
/// `P.policy.cutMode`).
///
/// Three jobs are one pass or mode: joins only runs when the marking pass is joins, retakes only
/// when it is retakes, and the model cut only when the mode is model — under words the timeline
/// comes from the words alone and nobody is asked. `MarkingPass::None` therefore asks for neither
/// marking job, which is what "no marking pass" has to mean.
pub fn job_applies(job: Job, marking_pass: MarkingPass, cut_mode: CutMode) -> bool {
    match job {
        Job::Joins => marking_pass == MarkingPass::Joins,
        Job::Retakes => marking_pass == MarkingPass::Retakes,
        Job::ModelCut => cut_mode == CutMode::Model,
        _ => true,
    }
}

/// One attempt at the model cut.
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
/// A model that filled the reasoning block and produced no answer is not going to do better by
/// reasoning again into the same void — it is stuck producing thinking with no content, so the
/// retry asks for the answer alone. Any other failure (bad seconds, a rejected segment) keeps
/// thinking on: that answer was written, it was just wrong.
pub fn cut_retry_thinking(previous_reasoned: bool, previous_answer_empty: bool) -> bool {
    !(previous_reasoned && previous_answer_empty)
}

/// How the model-cut attempt number `attempt` (counting from 1) is sent.
///
/// Web tools are withdrawn after the first rejected one — they cost a round each and the second
/// attempt is asked to copy segments off the timeline it was already given, not to go looking.
pub fn cut_attempt(attempt: u32) -> CutAttempt {
    CutAttempt {
        attempt,
        web_tools: attempt == 1,
        // Attempt 1 always thinks; a later one only if the last answer was not an empty one with
        // reasoning attached — which is what `cut_attempt` cannot see, so callers pass that to
        // `cut_retry_thinking` and set this from it.
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
/// Pooling is about repeats, not volume: three identical calls become one, and N different ones
/// stay N. So the count of calls this layer has to make equals the count of distinct stretches —
/// which is what a caller deduplicates against before it reaches here.
pub fn retake_batches(calls: usize) -> usize {
    calls
}

/// Whether sd.cpp is asked for a thumbnail.
///
/// §2: "only when an instruction exists and no frame was named". A named frame needs no server —
/// it is cut out of the source — so an instruction beside a frame is an instruction nobody needs,
/// and drawing one would spend a model call to produce something the user already rejected by
/// naming a frame instead.
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
    /// What the translation call is asked for. The name, because that is what a prompt reads as a
    /// language; the code and tag are for filenames (`<stem>.de.srt`, 08 §7).
    pub fn ask_for(&self) -> &str {
        &self.name
    }
}

/// The settings list of subtitle languages, in the order written.
///
/// Entries are separated by commas and their fields by colons — the shape §3 of 03-shell.md gives
/// `SUBTITLE_LANGUAGES` ("code:tag:name list"). A field is trimmed, a shorter entry fills what it
/// left out from the code (a bare `de` names German nowhere, so the code stands in rather than the
/// call asking for ""), and an entry with no code at all is dropped: there would be nothing to
/// name the file or the language by.
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

/// How many translation calls `languages` takes: one per language (§2).
pub fn subtitle_calls(languages: &[&str]) -> usize {
    languages.len()
}

/// The respelled transcript, with the times and speakers of the original.
///
/// §2: "the same lines respelled; times and speakers unchanged, enforced". Enforced means this
/// function never reads a time or a speaker out of `respelled`, so a reply that renumbered or
/// shifted them cannot get through. It also refuses a reply whose line count differs — a
/// respelling that dropped a line or invented one would otherwise have its words paired with the
/// times of different lines, which is worse than no cleanup at all because it reads like a success.
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
