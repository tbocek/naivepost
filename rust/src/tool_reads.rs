//! The §3.1 SHARED reading tools, answered: `get_context`, `get_lines`, `get_events` and `get_frames`.
//!
//! Offered to most jobs (`tools::offered(Job::CleanTranscript)` carries all four reads), and an unanswered read reads to the model as a tool that does not exist; §3 says a problem comes back as `{"error": …}` and the flow continues.
//!
//! [`dispatch`] answers from what the CALLING leg has in hand ([`Context`]), not files of its own: a read must return the same seconds the request was built from. No globals, no re-read.
//! Returns `None` for a name this module does not own (including `finish`, whose check list belongs to each job).

use serde_json::json;

use crate::exchanges::ToolCall;
use crate::roles::Job;
use crate::textfmt::{FrameEvent, Line};
use crate::tools;

/// What the leg asking has on hand, borrowed: a read must describe THAT batch, not a fresh copy of the session.
pub struct Context<'a> {
    /// The User Context, which `get_context` hands back verbatim (§3.1).
    pub user_context: &'a str,
    /// Which job is asking: `get_context` answers with the policy fields THAT job reads, not all of them.
    pub job: Job,
    /// The policy fields worth telling this job about, as `(name, value)` pairs spelled as `naivepost.json` spells them; the caller picks them (it knows which §10 section its job reads).
    pub policy: &'a [(&'a str, String)],
    /// The transcript rows, in the order the request numbered them — `get_lines` counts from 1 over THESE.
    pub lines: &'a [Line],
    /// The event log beside those rows.
    pub events: &'a [FrameEvent],
    /// The frames this request was stamped from, so `get_frames` names the same ones the pictures are.
    pub frames: &'a [f64],
}

/// Answer one shared read, or `None` when the call belongs to the caller's own job tools.
///
/// Every bad argument is a [`tools::error`] sentence naming what the model should send instead (§3), never a panic or silent drop.
pub fn dispatch(ctx: &Context, call: &ToolCall) -> Option<String> {
    let args: serde_json::Value = serde_json::from_str(&call.args).unwrap_or_else(|_| json!({}));
    match call.name.as_str() {
        "get_context" => Some(context_of(ctx)),
        "get_lines" => Some(lines_of(ctx, &args)),
        "get_events" => Some(events_of(ctx, &args)),
        "get_frames" => Some(frames_of(ctx, &args)),
        // `finish` is NOT answered here although §3.1 lists it as shared: every job ends with ITS OWN check list, and one anonymous `{"finished":true}` would pre-empt them.
        _ => None,
    }
}

/// `get_context`: the User Context plus the fields this job's rules read. An empty context answers with an empty field, not an error: "no User Context" is information the model needs.
fn context_of(ctx: &Context) -> String {
    let mut fields = serde_json::Map::new();
    for (name, value) in ctx.policy {
        fields.insert(
            (*name).to_string(),
            serde_json::Value::String(value.clone()),
        );
    }
    tools::ok(&json!({
        "job": job_name(ctx.job),
        "userContext": ctx.user_context,
        "policy": fields,
    }))
}

/// The job spelled as §2's table spells it; maps only the jobs that get shared reads (`roles::Job` has no display name and a second catalogue would drift).
fn job_name(job: Job) -> &'static str {
    match job {
        Job::Describe => "what is on screen",
        Job::CleanTranscript => "clean transcript",
        Job::Joins => "joins",
        Job::Retakes => "retakes",
        Job::ModelCut => "model cut",
        Job::ClipRules => "captions, speed, effects",
        Job::Narrate => "narration",
        Job::UploadText => "upload text",
        Job::Thumbnail => "thumbnail",
        Job::Subtitles => "subtitles in other languages",
        Job::Asr => "speech to text",
        Job::Align => "word times",
        Job::Diarize => "who spoke when",
        Job::Separate => "voice split",
    }
}

/// `get_lines(from, to)`: 1-based over the rows the request numbered, stamped as the request stamps them.
fn lines_of(ctx: &Context, args: &serde_json::Value) -> String {
    let (Some(from), Some(to)) = (number(args, "from"), number(args, "to")) else {
        return tools::error("get_lines needs \"from\" and \"to\" line numbers");
    };
    if from > to {
        return tools::error(&format!(
            "get_lines was asked from {from} to {to}, which is backwards -- send the smaller number first"
        ));
    }
    if ctx.lines.is_empty() {
        return tools::error("this request carried no transcript lines to read");
    }
    let first = from.max(1.0);
    let last = (to as usize).min(ctx.lines.len());
    if first as usize > last {
        return tools::error(&format!(
            "lines {from}-{to} are outside this block, which runs 1..={}",
            ctx.lines.len()
        ));
    }
    let rows: Vec<String> = (first as usize..=last)
        .filter_map(|index| ctx.lines.get(index - 1))
        .map(|line| {
            format!(
                "{}\t{}\t{}",
                crate::fix_transcripts::stamp_span(line.start, line.end),
                line.speaker,
                line.text
            )
        })
        .collect();
    tools::ok(&json!({"from": first, "to": last, "lines": rows}))
}

/// `get_events(from, to)`: the EVENT lines whose span touches those session seconds (overlap, not containment; as `fix_transcripts::context_text`).
fn events_of(ctx: &Context, args: &serde_json::Value) -> String {
    let (Some(from), Some(to)) = (number(args, "from"), number(args, "to")) else {
        return tools::error("get_events needs \"from\" and \"to\" session seconds");
    };
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    let rows: Vec<String> = ctx
        .events
        .iter()
        .filter(|event| event.end > from && event.start < to)
        .map(|event| {
            format!(
                "EVENT {}\t{}",
                crate::fix_transcripts::stamp_span(event.start, event.end),
                event.text
            )
        })
        .collect();
    tools::ok(&json!({"from": from, "to": to, "events": rows}))
}

/// `get_frames`: name the frames this request covers.
///
/// The pictures travel with the request as data URLs; what the model lacks is which FRAME NUMBER each stamp belongs to. `clip` is accepted and echoed because §3.7's per-clip passes ask by clip; the range form is `from`/`to` seconds.
fn frames_of(ctx: &Context, args: &serde_json::Value) -> String {
    if ctx.frames.is_empty() {
        return tools::error("this request carried no frames to read");
    }
    let picked: Vec<f64> = match (number(args, "from"), number(args, "to")) {
        (Some(from), Some(to)) => {
            let (from, to) = if from <= to { (from, to) } else { (to, from) };
            ctx.frames
                .iter()
                .copied()
                .filter(|at| *at >= from && *at < to)
                .collect()
        }
        // No range: the whole batch the request was built from.
        _ => ctx.frames.to_vec(),
    };
    if picked.is_empty() {
        return tools::error(&format!(
            "no frame falls in that stretch; this batch runs {} to {}",
            crate::fix_transcripts::stamp(ctx.frames[0]),
            crate::fix_transcripts::stamp(*ctx.frames.last().unwrap_or(&0.0)),
        ));
    }
    let frames: Vec<serde_json::Value> = ctx
        .frames
        .iter()
        .enumerate()
        .filter(|(_, at)| picked.contains(at))
        .map(|(index, at)| json!({"frame": index + 1, "at": crate::fix_transcripts::stamp(*at)}))
        .collect();
    tools::ok(&json!({"frames": frames, "already_sent": true}))
}

/// A JSON number read as f64, accepting a quoted number (servers quote them).
fn number(args: &serde_json::Value, key: &str) -> Option<f64> {
    match args.get(key)? {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}
