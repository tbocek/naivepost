//! §2's tool loop for the three per-clip jobs of §3.7 — captions, speed and decorations.
//!
//! All three offer their tools (`tools::offered(Job::ClipRules)` carries `add_caption`, `set_clip_speed`,
//! `add_effect`, `get_frames` and `finish`) yet until this round each ask site made ONE chat call and read
//! `tool_calls` off the single reply. Two of §2's promises were therefore unkept for these steps: a model that
//! asked `get_frames` — which §3.7 offers "so a zoom can be aimed" — got silence, and the round lines
//! `>>> captions: round 1 of 8 — the model asked for add_effect{…}` could never print, because there was no
//! round to number.
//!
//! [`run_over_batch`] is that loop: the same [`crate::tool_loop::run`] every other tool flow uses, with the CLIP
//! BATCH as the state the tools write into. A call is answered by the very object §3.7's rules live in
//! ([`Clips::add_caption`], [`Clips::set_clip_speed`], [`Clips::add_effect`]), so a call and a prose answer are
//! decided through ONE route and cannot disagree — the same reason `join_ask` answers `drop_words` on its `Join`.
//!
//! The prose path stays alive on purpose: a server that answers `{"fx": […]}` / `{"clips": […]}` /
//! `{"speeds": […]}` in content rather than calling a tool still gets the F3.9/F3.10/F3.11 treatment, because
//! those passes' tests pin that shape. [`Run::job_calls` / `Run::prose`] is where a caller reads the answer back.

use crate::exchanges::{Message, ToolCall};
use crate::tool_loop;
use crate::tools::clips::Clips;
use crate::tools::Tool;

/// Run one §3.7 ask as a tool conversation.
///
/// * `offered` — the job's tools; the loop refuses to run anything it cannot deliver (§2 S4).
/// * `ask` — one round: the caller dials and answers with the round's calls plus whether `finish` was called.
/// * `batch` — the clips the tools write into; what survives it IS the answer, exactly as the prose path's
///   parser reads the prose answer.
///
/// Returns the loop's log lines (§2's round/call notes, §4's exhaustion line) and its outcome. The caller
/// decides what the outcome means for its own pass, because the three passes disagree on purpose: one bad clip
/// number loses a whole captions batch (§F3.9) while a refused rate costs one clip of a speed batch (§F3.10).
/// How one §3.7 round is asked: the caller dials and answers with the round's calls plus whether `finish` was
/// called. Named because clippy is right that the inline type is hard to read, and both entry points write it.
type Ask<'a> = &'a mut (dyn FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String> + 'a);

pub fn run_over_batch(
    step: &str,
    messages: Vec<Message>,
    offered: &[Tool],
    batch: &mut Clips,
    ask: Ask<'_>,
) -> Run {
    run_over_batch_from(step, &tool_loop::Turn::default(), messages, offered, batch, ask)
}

/// [`run_over_batch`] with the FIRST round's answer already in hand — which is how all three cut passes work:
/// their first request went out before the loop existed (§2 S5's plain-call shape), so feeding that turn back is
/// what keeps a live ask at one request when the model answers in prose, and at two when it asked for something.
pub fn run_over_batch_from(
    step: &str,
    first: &tool_loop::Turn,
    messages: Vec<Message>,
    offered: &[Tool],
    batch: &mut Clips,
    ask: Ask<'_>,
) -> Run {
    // A job with no tools in the catalogue has nothing to loop over; the caller keeps its own plain call.
    if offered.is_empty() {
        return Run {
            run: tool_loop::Run {
                messages,
                logs: vec![crate::tools::exhausted_log(step)],
                outcome: tool_loop::Outcome::Finished { asked_finish: false },
            },
            calls: Vec::new(),
        };
    }
    // Every call the model made, in the order it made them. The assistant turns in `tool_loop::Run::messages`
    // keep only the model's TEXT (§7 puts the calls on the exchange page as notes, not in the turn), so a caller
    // that needs the calls — and all three passes do, because their `Asked`/`Parsed` shapes ARE call lists —
    // could not read them back afterwards. Holding them here is the honest place: the loop asked, this saw the
    // answer, so the list is the program's own and not a test's guess.
    let mut asked: Vec<ToolCall> = Vec::new();
    // Whether the round now being asked is the one whose answer the caller already dialled. An EMPTY first turn is
    // no turn at all: a pass that dials its first round inside the loop (`captions_ask` before any tool call) has
    // nothing to hand back, and feeding an empty turn would end the run before the wire was ever tried.
    let mut owed = !first.calls.is_empty() || !first.content.trim().is_empty();
    let mut probe = |conversation: &[Message], tools: &[Tool]| -> Result<tool_loop::Turn, String> {
        // The caller's first turn is handed straight back: it is already on the wire and in `requests.tsv`, and
        // asking for it again would double the request every pass is counted by.
        let turn = if owed {
            owed = false;
            first.clone()
        } else {
            ask(conversation, tools)?
        };
        asked.extend(turn.calls.iter().cloned());
        Ok(turn)
    };
    let run = tool_loop::run(step, messages, offered, &mut probe, |call| answer(call, batch));
    Run { run, calls: asked }
}

/// Answer a whole round's calls against the batch and return the §7 notes for them, in order — the lines a step
/// logs so a refusal (a rate over a captioned clip, §F3.10 S2) is SAID rather than quietly dropped. Same door the
/// loop uses for its own rounds; a caller that dialled its first round outside the loop needs it to reach the same
/// sentence.
pub fn answer_calls(calls: &[ToolCall], batch: &mut Clips) -> Vec<String> {
    calls
        .iter()
        .map(|call| {
            let mut done = call.clone();
            done.result = answer(call, batch);
            tool_loop::call_note(&done)
        })
        .collect()
}

/// A finished §3.7 conversation: the loop's own record, plus the calls it saw.
pub struct Run {
    pub run: tool_loop::Run,
    /// Every call the model made, reads included, in the order it made them.
    pub calls: Vec<ToolCall>,
}

impl Run {
    /// The model's last spoken text — the prose answer of a round that made no calls, the shape F3.9, F3.10 and
    /// F3.11 were written against. `None` when every round answered by calling a tool.
    pub fn prose(&self) -> Option<String> {
        self.run
            .messages
            .iter()
            .filter(|message| message.role == "assistant")
            .filter_map(|message| {
                message.parts.iter().find_map(|part| match part {
                    crate::exchanges::Part::Text(text) if !text.trim().is_empty() => Some(text.clone()),
                    _ => None,
                })
            })
            .next_back()
    }

    /// This job's own calls only, in the order asked: a `get_frames` read is not an answer, and the passes count
    /// what was asked for, not what was looked at.
    pub fn job_calls(&self, job: &[Tool]) -> Vec<ToolCall> {
        self.calls
            .iter()
            .filter(|call| job.iter().any(|tool| tool.name() == call.name))
            .cloned()
            .collect()
    }

    /// Whether the loop ended by running out of rounds (§2 S4) rather than by an answer.
    pub fn exhausted(&self) -> bool {
        self.run.outcome == tool_loop::Outcome::Exhausted
    }
}

/// Answer one §3.7 call against the batch: the read first, then this job's three tools, then `finish`.
///
/// A problem comes back as [`crate::tools::error`]'s `{"error": …}` sentence, which §2 S6 sends to the model as
/// text and never treats as a failed job — so a call for a clip outside the batch or an unknown effect kind is an
/// answer the model can correct itself from, not an ending.
pub fn answer(call: &ToolCall, batch: &mut Clips) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&call.args).unwrap_or_else(|_| serde_json::json!({}));
    let number = |key: &str| {
        args.get(key)
            .and_then(|value| value.as_f64().or_else(|| value.as_str().and_then(|t| t.trim().parse::<f64>().ok())))
    };
    match call.name.as_str() {
        // §3.7's read: the clip's frames, so a zoom can be aimed. `at` selects the nearest stamped frame.
        "get_frames" => frames_of(batch, number("clip").map(|n| n as u32), number("at")),
        "add_caption" => match (
            number("clip").map(|n| n as u32),
            number("start"),
            number("end"),
            args.get("text").and_then(|text| text.as_str()),
        ) {
            (Some(clip), Some(start), Some(end), Some(text)) => batch.add_caption(clip, start, end, text),
            _ => crate::tools::error("add_caption needs \"clip\", \"start\", \"end\" and \"text\""),
        },
        "set_clip_speed" => match (number("clip").map(|n| n as u32), number("rate")) {
            (Some(clip), Some(rate)) => batch.set_clip_speed(clip, rate),
            _ => crate::tools::error("set_clip_speed needs \"clip\" and \"rate\""),
        },
        "add_effect" => match (
            number("clip").map(|n| n as u32),
            args.get("kind").and_then(|kind| kind.as_str()),
            number("start"),
            number("end"),
        ) {
            (Some(clip), Some(kind), Some(start), Some(end)) => {
                // An omitted gain and a 1 are the same ask (§F3.11); an explicit 0 is silence and must survive.
                let gain = number("gain").filter(|value| *value != 1.0);
                batch.add_effect(clip, kind, start, end, gain, None)
            }
            _ => crate::tools::error("add_effect needs \"clip\", \"kind\", \"start\" and \"end\""),
        },
        // §3.7's `finish` for these three jobs IS the batch, which is what the caller reads back off `Clips`;
        // saying so in words tells the model the conversation is over rather than leaving it guessing.
        "finish" => "{\"finished\":true}".to_string(),
        _ => crate::tools::error(&format!("{} is not one of this job's tools", call.name)),
    }
}

/// `get_frames` over a clip batch: the stamped offsets of one clip, or of every clip when none is named.
///
/// §3.7 offers this "so a zoom can be aimed", so the answer must name the offsets the model can then copy into
/// `add_effect(start/end)` — clip-relative seconds, the space every other number in this request is in. A clip
/// outside the batch is the §3.1 error, not an empty list.
fn frames_of(batch: &Clips, clip: Option<u32>, at: Option<f64>) -> String {
    let wanted: Vec<u32> = match clip {
        Some(clip) => vec![clip],
        None => batch.clips().iter().map(|candidate| candidate.n).collect(),
    };
    let mut frames: Vec<serde_json::Value> = Vec::new();
    for number in wanted {
        for frame in batch.clip_frames(number, at) {
            frames.push(serde_json::json!({ "clip": frame.clip, "at": frame.at }));
        }
    }
    if frames.is_empty() {
        return crate::tools::error(&match clip {
            Some(clip) => format!("clip {clip} is not in this batch"),
            None => "this batch has no clips to show".to_string(),
        });
    }
    serde_json::json!({ "frames": frames }).to_string()
}

/// The batch a §3.7 ask was built about, in the shape [`Clips`] wants.
///
/// The captions and speeds passes carry `(number, length)` pairs, and a clip's captions are OFFSETS inside that
/// clip (§F3.9), so every clip starts at zero in that space. A wrong `start` would make the tool clamp into the
/// wrong clip — the very mistake §3.7's tool exists to prevent.
pub fn batch_of(clips: &[(u32, f64)]) -> Clips {
    Clips::new(&clips.iter().map(|(n, length)| (*n, 0.0, *length)).collect::<Vec<_>>())
}
