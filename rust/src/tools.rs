//! The tool catalogue, spec/02-services.md §3.
//! Which tools a job is offered, the two result shapes, and how long a call may keep asking; what a tool does lives in the submodules named after §3's sections.
//! A problem comes back as one sentence the model can act on and never ends the flow. Every number in a result is an identifier the request already contained (line, clip, frame numbers, offsets as stamped); the app does the arithmetic.
//! Nothing here opens a socket: `web_search`/`web_read` are described by their caps and the client is [09 §2]'s item.
//! [09 §2]: spec/09-llm-and-tools.md#2-tool-protocol-f61

use serde::Serialize;

use crate::roles::{self, Job};

/// Every tool in §3's tables; [`Tool::name`] gives the spec spelling.
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
    // 3.12 Text gate
    JudgeText,
}

impl Tool {
    /// Every tool §3 names, in table order.
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
            Self::JudgeText,
        ]
        .to_vec()
    }

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
            Self::JudgeText => "judge_text",
        }
    }
}

/// The five shared tools of §3.1 that start each tool-using job's list; `web_search`/`web_read` are separate, see [`web_search_offered`].
#[allow(
    dead_code,
    reason = "the per-job lists spell the five inline; kept as §3.1's own row"
)]
const SHARED: [Tool; 5] = [
    Tool::GetContext,
    Tool::GetLines,
    Tool::GetEvents,
    Tool::GetFrames,
    Tool::Finish,
];

/// The tools this job is offered (§3: "Tools are offered per job"). The four audio jobs get none (one audio.cpp call). Every other job starts from [`SHARED`] plus its section's tools, except:
/// * Describe reads `get_frames`, `get_events`, `get_context`, `speech_around` (§3.2) and drops `get_lines`.
/// * The cut ends with `finish_cut` instead of `finish` (§3.6).
pub fn offered(job: Job) -> Vec<Tool> {
    let mut out: Vec<Tool> = match job {
        Job::Asr | Job::Align | Job::Diarize | Job::Separate => return Vec::new(),
        Job::Describe => vec![
            Tool::GetContext,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::RecordEvent,
            Tool::SetState,
            Tool::SpeechAround,
        ],
        Job::CleanTranscript => vec![],
        Job::Retakes => vec![
            Tool::GetContext,
            Tool::GetLines,
            Tool::GetEvents,
            Tool::GetFrames,
            Tool::Finish,
            Tool::MarkAbandoned,
            Tool::Unmark,
        ],
        // §3.5: the text edit offers no tools and answers with the quoted words it removes; with drop_words/keep_join the 27B answered nothing usable at 125 of 127 asks on 07-news (2026-10-26).
        Job::Joins => vec![],
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
        // §3.9 is one table for two jobs sharing state: text and picture are decided in one conversation.
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

/// The tools of §3.11, run once per project (F0.7); in no [`Job`].
pub fn policy_tools() -> Vec<Tool> {
    vec![Tool::GetContext, Tool::SetPolicy]
}

impl Tool {
    /// The tool's arguments as a JSON schema from §3's tables (`name?` is optional). A tool offered by name alone is answered with `{}` (llama.cpp builds its grammar from the schema; probed 2026-10-25).
    pub fn parameters(self) -> serde_json::Value {
        use serde_json::json;
        let number = || json!({"type": "number"});
        let integer = || json!({"type": "integer"});
        let text = || json!({"type": "string"});
        let schema = |props: serde_json::Value, required: &[&str]| json!({"type": "object", "properties": props, "required": required});
        match self {
            Self::GetContext
            | Self::Finish
            | Self::KeepJoin
            | Self::CutStatus
            | Self::FinishCut
            | Self::ListEmotions => schema(json!({}), &[]),
            Self::GetLines | Self::GetEvents | Self::SpeechAround => {
                schema(json!({"from": number(), "to": number()}), &["from", "to"])
            }
            Self::WebSearch => schema(
                json!({"broad": text(), "medium": text(), "narrow": text()}),
                &["broad", "medium", "narrow"],
            ),
            Self::WebRead => schema(json!({"url": text()}), &["url"]),
            Self::GetFrames => schema(
                json!({"clip": integer(), "at": number(), "from": number(), "to": number()}),
                &[],
            ),
            Self::RecordEvent => schema(
                json!({"frame": integer(), "text": text(), "calm": {"type": "boolean"}}),
                &["frame", "text"],
            ),
            Self::SetState | Self::SetTitle | Self::SetDescription => {
                schema(json!({"text": text()}), &["text"])
            }
            Self::FixLine | Self::TranslateLine => {
                schema(json!({"n": integer(), "text": text()}), &["n", "text"])
            }
            Self::FlagLine => schema(json!({"n": integer(), "why": text()}), &["n", "why"]),
            Self::MarkAbandoned => schema(
                json!({"from": integer(), "to": integer(), "again": integer()}),
                &["from", "to", "again"],
            ),
            Self::Unmark => schema(json!({"from": integer(), "to": integer()}), &["from", "to"]),
            Self::DropWords | Self::GetWords => schema(
                json!({"side": {"type": "string", "enum": ["before", "after"]}, "count": integer()}),
                &["side", "count"],
            ),
            Self::AddSegment => schema(
                json!({"start": number(), "end": number(), "why": text()}),
                &["start", "end", "why"],
            ),
            Self::RemoveSegment => schema(json!({"start": number()}), &["start"]),
            Self::SetSpeed => schema(
                json!({"start": number(), "rate": number()}),
                &["start", "rate"],
            ),
            Self::AddCaption => schema(
                json!({"clip": integer(), "start": number(), "end": number(), "text": text()}),
                &["clip", "start", "end", "text"],
            ),
            Self::SetClipSpeed => schema(
                json!({"clip": integer(), "rate": number()}),
                &["clip", "rate"],
            ),
            Self::AddEffect => schema(
                json!({"clip": integer(), "kind": {"type": "string", "enum": ["zoom", "stop", "volume"]},
                    "start": number(), "end": number(), "gain": number()}),
                &["clip", "kind", "start", "end"],
            ),
            Self::WriteLine => schema(
                json!({"clip": integer(), "at": number(), "text": text(), "emotion": text(),
                    "pos": {"type": "string", "enum": ["top", "center", "bottom"]}}),
                &["clip", "at", "text", "emotion"],
            ),
            Self::LeaveSilent | Self::DescribeInsert => {
                schema(json!({"clip": integer()}), &["clip"])
            }
            Self::PickFrame => schema(
                json!({"clip": integer(), "offset": number()}),
                &["clip", "offset"],
            ),
            Self::SetThumbnailInstruction => {
                schema(json!({"text": text(), "negative": text()}), &["text"])
            }
            Self::SetPolicy => schema(
                json!({"field": text(), "value": {"type": ["string", "number", "boolean"]}, "because": text()}),
                &["field", "value", "because"],
            ),
            Self::JudgeText => schema(
                json!({"changed": {"type": "boolean"}, "what": text()}),
                &["changed", "what"],
            ),
        }
    }
}

/// The tool of §3.12, the text gate's question (F1.6 S6) per pair of frames; in no [`Job`].
pub fn text_gate_tools() -> Vec<Tool> {
    vec![Tool::JudgeText]
}

/// The one shape a problem comes back in: `{}` around a sentence the model can act on. Serialised so a quote or newline cannot break the client. Never ends the flow.
pub fn error(reason: &str) -> String {
    #[derive(Serialize)]
    struct Error<'a> {
        error: &'a str,
    }
    serde_json::to_string(&Error { error: reason }).expect("one string field always serialises")
}

/// The one shape a success comes back in: a short JSON object saying what the tool did. Short because the context is limited; only `finish` ends the flow.
pub fn ok(body: &impl Serialize) -> String {
    serde_json::to_string(body).expect("a caller's own struct always serialises")
}

// How long a call may keep asking, re-exported so §2 and §3 share one number. P.eng.llmToolRounds
pub use crate::roles::TOOL_ROUNDS;

/// Whether round `round` (from 1) is past the budget: the step then gets no answer. P.eng.llmToolRounds
pub fn rounds_exhausted(round: u32) -> bool {
    round > TOOL_ROUNDS
}

/// The single nudge of [09 §2]: a round with no tool calls and no `finish` is asked once to call `finish`; true for exactly that round.
pub fn asked_to_finish(silent_rounds: u32) -> bool {
    silent_rounds == 1
}

/// The log line when the budget runs out with the model still calling tools.
pub fn exhausted_log(step: &str) -> String {
    format!("!!! {step}: still calling tools after {TOOL_ROUNDS} rounds -- no answer from it")
}

/// Hits one `web_search` may return, with their snippets.
pub const WEB_SEARCH_MAX_HITS: usize = 8;

/// Page text one `web_read` returns, in bytes — past that the page is a cost, not an answer.
pub const WEB_READ_MAX_BYTES: usize = 6000;

/// Seconds one web tool call gets, browser boot and render wait included.
pub const WEB_CALL_SECONDS: u64 = 45;

/// The search ladder's budget: three queries (broad, medium, narrow) sharing one 45 s.
pub fn web_ladder_budget() -> (u64, u32) {
    (WEB_CALL_SECONDS, 3)
}

/// Whether this job gets the web: "cut, narrate, upload text", §2's `(+ web tools)` column.
pub fn web_search_offered(job: Job) -> bool {
    roles::web_tools(job)
}

/// Whether a tool is one of the pair §8 withdraws after a first rejection.
pub fn is_web_tool(tool: Tool) -> bool {
    matches!(tool, Tool::WebSearch | Tool::WebRead)
}

/// Seconds as `mm:ss`, for quoting a number back in the units the model meant (§3.6). Minutes keep counting past an hour (`62:05`).
pub fn mm_ss(seconds: f64) -> String {
    // A negative has no timeline reading; clamp rather than print "-00:03".
    let whole = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", whole / 60, whole % 60)
}

/// A length of seconds as the Cut page writes it: to a tenth (`8.0 s`).
pub fn tenths(seconds: f64) -> String {
    format!("{:.1} s", seconds)
}

pub mod describe;
pub mod fix;
pub mod retakes;
// §3.6's model-chosen cut, named after the pass rather than the file it writes.
pub mod clips;
pub mod cutpass;
pub mod textedit;
