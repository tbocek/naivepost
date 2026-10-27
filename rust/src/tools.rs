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
    // 3.12 Text gate
    JudgeText,
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
            Self::JudgeText,
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
            Self::JudgeText => "judge_text",
        }
    }
}

/// The five shared tools of §3.1 that every tool-using job gets — the prefix [`offered`] starts
/// each list with, spelled once so a section's own table is the only thing it has to add.
/// `web_search` and `web_read` are not here because they go only where the prototype offered them:
/// see [`web_search_offered`].
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
        // §3.3: the transcript fix offers no tools, as the prototype asked it. It answers the block's own rows with
        // only the text changed, in one reply; offered `fix_line`, `flag_line` and reads with nothing behind
        // them, the model asked for an empty context and answered nothing, 4 blocks out of 5.
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
        // §3.5: the text edit offers no tools. It answers the words it removes, quoted, in one reply: offered
        // drop_words/keep_join beside a prompt asking for text, the 27B answered nothing usable at 125 of 127 asks
        // on 07-news (2026-10-26), and the quoted answer beat the tool flow on the same 18 joins.
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

impl Tool {
    /// The tool's arguments as a JSON schema, from §3's tables (`name?` is optional). Sent with the offer: a
    /// llama.cpp server builds its grammar from it, and a tool offered by name alone is answered with `{}` (probed
    /// 2026-10-25 on the user's server: `judge_text` came back `{}` without a schema and `{"changed":true,…}`
    /// with one), so every tool the first rewrite offered reached its executor with no arguments.
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

/// The tool of §3.12, the text gate's one question (F1.6 S6), asked of the decision model per pair of frames —
/// like §3.11's it belongs to no [`Job`].
pub fn text_gate_tools() -> Vec<Tool> {
    vec![Tool::JudgeText]
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
    serde_json::to_string(&Error { error: reason }).expect("one string field always serialises")
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
    format!("!!! {step}: still calling tools after {TOOL_ROUNDS} rounds -- no answer from it")
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

/// Whether a tool is one of the pair §8 withdraws after a first rejection. Named once here so the cut loop
/// and any later flow take the same two tools away, and never mistake `FinishCut` for a web tool.
pub fn is_web_tool(tool: Tool) -> bool {
    matches!(tool, Tool::WebSearch | Tool::WebRead)
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

/// A length of seconds the way the Cut page writes one: to a tenth (`8.0 s`, `8.5 s`). One spelling of a
/// duration for every status line and tooltip that quotes one back, so a trim and a paste never disagree
/// about whether 8 was measured or estimated.
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
