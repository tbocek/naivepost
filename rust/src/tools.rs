//! The tool catalogue, spec/02-services.md §3 (rewrite directive B).
//!
//! Which tools a job is offered, the two shapes every result comes back in, and how long a call
//! may keep asking. What a tool actually does to the model's work — what a segment becomes when it
//! is added, what a line costs when it is written — lives in the submodules named after §3's
//! sections, so each of those can be read against its table.
//!
//! Two rules run through all of it. A problem comes back as one sentence the model can act on and
//! never ends the flow: a tool that refuses is a step in the conversation, not a failed run. And
//! every number in a result is an identifier the request already contained — line, clip and frame
//! numbers, offsets as stamped — because the arithmetic is what the app is for, and a model asked
//! to compute it computes something plausible.
//!
//! Nothing here opens a socket or starts a browser: `web_search`/`web_read` are described by their
//! caps below, and the headless-firefox client that meets them is [09 §2]'s own item.
//!
//! [09 §2]: spec/09-llm-and-tools.md#2-tool-protocol-f61

use serde::Serialize;

use crate::roles::{self, Job};

/// Every tool in §3's tables. The name is what the model sees, so [`Tool::name`] is the spelling
/// the spec writes and this enum is only its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    // 3.1 Shared
    GetContext,
    GetLines,
    GetEvents,
    WebSearch,
    WebRead,
    GetFrames,
    Finish,
    // 3.2 Describe
    RecordEvent,
    SetState,
    SpeechAround,
    // 3.3 Fix
    FixLine,
    FlagLine,
    // 3.4 Retake marking
    MarkAbandoned,
    Unmark,
    // 3.5 Text edit
    DropWords,
    KeepJoin,
    GetWords,
    // 3.6 Cut
    AddSegment,
    RemoveSegment,
    SetSpeed,
    CutStatus,
    FinishCut,
    // 3.7 Captions / speed / decorations
    AddCaption,
    SetClipSpeed,
    AddEffect,
    // 3.8 Narration
    WriteLine,
    LeaveSilent,
    ListEmotions,
    DescribeInsert,
    // 3.9 Upload text and thumbnail
    SetTitle,
    SetDescription,
    PickFrame,
    SetThumbnailInstruction,
    // 3.10 Translate
    TranslateLine,
    // 3.11 Policy derivation
    SetPolicy,
}

impl Tool {
    /// Every tool §3 names, in the order its tables list them — the catalogue a caller can walk to
    /// build a request's `tools` array or to check that nothing was forgotten.
    pub fn all() -> Vec<Tool> {
        [
            Self::GetContext,
            Self::GetLines,
            Self::GetEvents,
            Self::WebSearch,
            Self::WebRead,
            Self::GetFrames,
            Self::Finish,
            Self::RecordEvent,
            Self::SetState,
            Self::SpeechAround,
            Self::FixLine,
            Self::FlagLine,
            Self::MarkAbandoned,
            Self::Unmark,
            Self::DropWords,
            Self::KeepJoin,
            Self::GetWords,
            Self::AddSegment,
            Self::RemoveSegment,
            Self::SetSpeed,
            Self::CutStatus,
            Self::FinishCut,
            Self::AddCaption,
            Self::SetClipSpeed,
            Self::AddEffect,
            Self::WriteLine,
            Self::LeaveSilent,
            Self::ListEmotions,
            Self::DescribeInsert,
            Self::SetTitle,
            Self::SetDescription,
            Self::PickFrame,
            Self::SetThumbnailInstruction,
            Self::TranslateLine,
            Self::SetPolicy,
        ]
        .to_vec()
    }

    /// The name the model is offered this tool under — §3's tables verbatim.
    pub fn name(self) -> &'static str {
        match self {
            Self::GetContext => "get_context",
            Self::GetLines => "get_lines",
            Self::GetEvents => "get_events",
            Self::WebSearch => "web_search",
            Self::WebRead => "web_read",
            Self::GetFrames => "get_frames",
            Self::Finish => "finish",
            Self::RecordEvent => "record_event",
            Self::SetState => "set_state",
            Self::SpeechAround => "speech_around",
            Self::FixLine => "fix_line",
            Self::FlagLine => "flag_line",
            Self::MarkAbandoned => "mark_abandoned",
            Self::Unmark => "unmark",
            Self::DropWords => "drop_words",
            Self::KeepJoin => "keep_join",
            Self::GetWords => "get_words",
            Self::AddSegment => "add_segment",
            Self::RemoveSegment => "remove_segment",
            Self::SetSpeed => "set_speed",
            Self::CutStatus => "cut_status",
            Self::FinishCut => "finish_cut",
            Self::AddCaption => "add_caption",
            Self::SetClipSpeed => "set_clip_speed",
            Self::AddEffect => "add_effect",
            Self::WriteLine => "write_line",
            Self::LeaveSilent => "leave_silent",
            Self::ListEmotions => "list_emotions",
            Self::DescribeInsert => "describe_insert",
            Self::SetTitle => "set_title",
            Self::SetDescription => "set_description",
            Self::PickFrame => "pick_frame",
            Self::SetThumbnailInstruction => "set_thumbnail_instruction",
            Self::TranslateLine => "translate_line",
            Self::SetPolicy => "set_policy",
        }
    }
}

/// The five shared tools of §3.1 that every tool-using job gets — the prefix [`offered`] starts
/// each list with, spelled once so a section's own table is the only thing it has to add.
/// `web_search` and `web_read` are not here because they go only where the prototype offered them:
/// see [`web_search_offered`].
#[allow(dead_code, reason = "the per-job lists spell the five inline; kept as §3.1's own row")]
const SHARED: [Tool; 5] = [
    Tool::GetContext,
    Tool::GetLines,
    Tool::GetEvents,
    Tool::GetFrames,
    Tool::Finish,
];

/// The tools this job is offered (§3: "Tools are offered per job").
///
/// The four audio jobs get nothing at all: they are one audio.cpp task call whose answer is the
/// work, not a model choosing what to look at next. Every other job starts from [`SHARED`] and adds
/// its own section's tools, with two differences the tables force:
///
/// * Describe reads `get_frames`, `get_events`, `get_context` and `speech_around` — §3.2's row is a
///   Reads list, not an addition to the shared set, so `get_lines` is dropped for it: there are no
///   transcript lines in a chunk of frames to read.
/// * The cut ends with `finish_cut`, not `finish`: its check list is about segments and footage,
///   so `Finish` is replaced rather than duplicated (§3.6's `finish_cut` "is the reading finish_cut
///   would give" for this job).
pub fn offered(job: Job) -> Vec<Tool> {
    let mut out: Vec<Tool> = match job {
        // One audio.cpp task call; nothing to offer it.
        Job::Asr | Job::Align | Job::Diarize | Job::Separate => return Vec::new(),
        // §3.2's Reads row instead of the shared set: no transcript lines in a chunk of frames.
        Job::Describe => vec![
            Tool::GetContext,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::RecordEvent,
            Tool::SetState,
            Tool::SpeechAround,
        ],
        Job::CleanTranscript => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::FixLine,
            Tool::FlagLine,
        ],
        Job::Retakes => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::MarkAbandoned,
            Tool::Unmark,
        ],
        Job::Joins => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::DropWords,
            Tool::KeepJoin,
            Tool::GetWords,
        ],
        Job::ModelCut => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::AddSegment,
            Tool::RemoveSegment,
            Tool::SetSpeed,
            Tool::CutStatus,
            Tool::FinishCut,
        ],
        Job::ClipRules => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::AddCaption,
            Tool::SetClipSpeed,
            Tool::AddEffect,
        ],
        Job::Narrate => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::WriteLine,
            Tool::LeaveSilent,
            Tool::ListEmotions,
            Tool::DescribeInsert,
        ],
        // §3.9 is one table for the two jobs that share its state: the text and the picture are
        // decided in one conversation, which is why picking a frame can refuse an instruction.
        Job::UploadText | Job::Thumbnail => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::SetTitle,
            Tool::SetDescription,
            Tool::PickFrame,
            Tool::SetThumbnailInstruction,
        ],
        Job::Subtitles => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::TranslateLine,
        ],
    };
    if web_search_offered(job) {
        out.push(Tool::WebSearch);
        out.push(Tool::WebRead);
    }
    out
}

/// The tools of §3.11, which runs once per project (F0.7) rather than per job — so it belongs to no
/// [`Job`] and could not be reached through [`offered`] even if it wanted to be.
pub fn policy_tools() -> Vec<Tool> {
    vec![Tool::GetContext, Tool::SetPolicy]
}

/// The one shape a problem comes back in: `{}` around a sentence the model can act on.
///
/// Serialised rather than formatted so a quote or a newline inside that sentence cannot produce
/// something the client chokes on — the failure mode would be a tool result that reads as a broken
/// reply, which is a worse version of the problem it reports. Returning this never ends the flow:
/// §3's whole point is that a refusal is an answer the model can use.
pub fn error(reason: &str) -> String {
    #[derive(Serialize)]
    struct Error<'a> {
        error: &'a str,
    }
    serde_json::to_string(&Error { error: reason })
        .expect("one string field always serialises")
}

/// The one shape a success comes back in: a short JSON object saying what the tool did.
///
/// "Short" is a rule about the model's context, not about tidiness — a result that repeats the
/// transcript costs a round to read and gets quoted back at us. Neither this nor [`error`] ends
/// the flow; only `finish` does.
pub fn ok(body: &impl Serialize) -> String {
    serde_json::to_string(body).expect("a caller's own struct always serialises")
}

// How long a call may keep asking: re-exported so §2 and §3 name one number, not two that have to
// be kept equal. P.eng.llmToolRounds
pub use crate::roles::TOOL_ROUNDS;

/// Whether round `round` (counting from 1) is past the budget — beyond it the step gets no answer
/// from the model at all. P.eng.llmToolRounds
pub fn rounds_exhausted(round: u32) -> bool {
    round > TOOL_ROUNDS
}

/// The single nudge of [09 §2]: a round with no tool calls and no `finish` is asked once to call
/// `finish`, and the next one is taken as finished. True for exactly that asking round, so a caller
/// that asks twice has invented a second round the spec does not allow.
pub fn asked_to_finish(silent_rounds: u32) -> bool {
    silent_rounds == 1
}

/// The log line when the budget runs out with the model still calling tools. Names the number so
/// the log reads as a limit rather than as a hang.
pub fn exhausted_log(step: &str) -> String {
    format!(
        "!!! {step}: still calling tools after {TOOL_ROUNDS} rounds -- no answer from it"
    )
}

/// Hits one `web_search` may return, with their snippets.
pub const WEB_SEARCH_MAX_HITS: usize = 8;

/// Page text one `web_read` returns, in bytes — past that the page is a cost, not an answer.
pub const WEB_READ_MAX_BYTES: usize = 6000;

/// Seconds one web tool call gets, browser boot and render wait included.
pub const WEB_CALL_SECONDS: u64 = 45;

/// The search ladder's budget: three queries — broad, medium, narrow — sharing one 45 s, not each
/// getting their own. A ladder that spent 45 s per query would hold a step open for minutes on a
/// search nobody finished.
pub fn web_ladder_budget() -> (u64, u32) {
    (WEB_CALL_SECONDS, 3)
}

/// Whether this job gets the web at all — "only where the prototype offered it: cut, narrate,
/// upload text", which is exactly §2's `(+ web tools)` column. Reused rather than re-derived so the
/// two tables cannot drift apart.
pub fn web_search_offered(job: Job) -> bool {
    roles::web_tools(job)
}

/// Seconds as `mm:ss`, the reading a model recognises as a moment on the timeline.
///
/// Used where §3.6 tells us to quote the model's own number back in the units it meant: "past the
/// end" beside `4271` is a correction the model cannot check, and beside `71:11` it is one it can.
/// Past an hour the minutes keep counting rather than rolling over — a session timestamp is not a
/// clock face, and `62:05` says where the second hour is while `02:05` would not.
pub fn mm_ss(seconds: f64) -> String {
    // A negative number has no reading on a timeline; clamping it beats printing "-00:03", which
    // a model reads as a moment and tries to reason about.
    let whole = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", whole / 60, whole % 60)
}

pub mod describe;
pub mod fix;
pub mod retakes;
// §3.6's model-chosen cut, named after the pass rather than the file it writes.
pub mod cutpass;
pub mod textedit;
pub mod clips;
