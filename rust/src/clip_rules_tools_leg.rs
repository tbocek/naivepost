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

use crate::exchanges::{Message, Part, ToolCall};
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
    run_over_batch_from(
        step,
        &tool_loop::Turn::default(),
        messages,
        offered,
        batch,
        ask,
    )
}

/// One answered round: what the model was shown, and its calls with each result already written on them.
#[derive(Clone)]
pub struct Round {
    pub messages: Vec<Message>,
    pub turn: tool_loop::Turn,
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
                outcome: tool_loop::Outcome::Finished {
                    asked_finish: false,
                },
                announced: Vec::new(),
            },
            calls: Vec::new(),
            rounds: Vec::new(),
        };
    }
    // Every call the model made, in the order it made them. The assistant turns in `tool_loop::Run::messages`
    // keep only the model's TEXT (§7 puts the calls on the exchange page as notes, not in the turn), so a caller
    // that needs the calls — and all three passes do, because their `Asked`/`Parsed` shapes ARE call lists —
    // could not read them back afterwards. Holding them here is the honest place: the loop asked, this saw the
    // answer, so the list is the program's own and not a test's guess.
    let mut asked: Vec<ToolCall> = Vec::new();
    // Every round as it ran: the conversation at that moment plus the answered calls. §7 files one section per
    // request, so a step that asked three times owes three sections, and each one can only be drawn from the state
    // THIS round had — reading it back off the finished conversation would show every round the last one's messages.
    let mut rounds: Vec<Round> = Vec::new();
    // Whether the round now being asked is the one whose answer the caller already dialled. An EMPTY first turn is
    // no turn at all: a pass that dials its first round inside the loop (`captions_ask` before any tool call) has
    // nothing to hand back, and feeding an empty turn would end the run before the wire was ever tried.
    let mut owed = !first.calls.is_empty() || !first.content.trim().is_empty();
    let mut probe = |conversation: &[Message], tools: &[Tool]| -> Result<tool_loop::Turn, String> {
        // The caller's first turn is handed straight back: it is already on the wire and in `requests.tsv`, and
        // asking for it again would double the request every pass is counted by.
        let mut turn = if owed {
            owed = false;
            first.clone()
        } else {
            ask(conversation, tools)?
        };
        asked.extend(turn.calls.iter().cloned());
        // Remember what this round was SHOWN and what it ANSWERED. §7 files one section per request, and only the
        // round itself can say which conversation it was looking at: reading it back off the finished conversation
        // would show every round the last one's messages. The calls travel unanswered because the loop answers
        // them next, through [`answer`]; writing their results back afterwards (see below) is what keeps a call's
        // verdict identical everywhere it is read — the log line, the next request's `tool` message, and the page.
        rounds.push(Round {
            messages: conversation.to_vec(),
            turn: turn.clone(),
        });
        // `finish` is this job's own end signal (§2 S3) and the loop reads it off the Turn, so set it here, at
        // the one place that sees the real calls: a round that called `finish` beside its work tool (exactly how
        // a model closes an `add_caption` answer) ENDED the job. Leaving it false would make the loop treat a
        // closed answer as an open one and keep asking for what it was already given.
        turn.finish = turn.finish
            || turn
                .calls
                .iter()
                .any(|call| call.name == Tool::Finish.name());
        Ok(turn)
    };
    let run = tool_loop::run(step, messages, offered, &mut probe, |call| {
        answer(call, batch)
    });
    // Write each round's ANSWERED calls back into the round that made them. This happens after the loop rather
    // than inside the ask because answering there would need the batch twice over at once (the loop answers the
    // same calls itself), and re-running `answer` afterwards could report a different verdict for a call whose
    // effect the first run already consumed — e.g. a stretch merged or a clip count that moved. So the results
    // are taken from the conversation the loop grew: §2 S2 put one `tool` message per call there, in ask order,
    // which is exactly where the wire carries them too.
    //
    // The rounds are matched to their turns by what each was SHOWN rather than by counting assistant turns: the
    // loop sometimes inserts a user turn of its own (§2's `finish` nudge) and a quiet round adds no assistant
    // turn at all, so position-in-the-list is not a reliable identity. What IS reliable is that the loop grows
    // the conversation strictly, so the messages recorded for round k+1 always contain everything round k was
    // shown, plus that round's answer.
    let assistants: Vec<usize> = run
        .messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.role == "assistant")
        .map(|(at, _)| at)
        .collect();
    let last = rounds.len();
    for (index, round) in rounds.iter_mut().enumerate() {
        let shown = round.messages.len();
        // This round's own turn: the first assistant turn that arrived after it was asked.
        let mine = assistants
            .iter()
            .copied()
            .filter(|at| *at >= shown)
            .min()
            .unwrap_or(usize::MAX);
        let end = assistants
            .iter()
            .copied()
            .filter(|at| *at > mine)
            .min()
            .unwrap_or(run.messages.len());
        if mine == usize::MAX || mine >= run.messages.len() {
            continue;
        }
        let window = &run.messages[mine..end.min(run.messages.len())];
        if let Some(Part::Text(spoken)) = window.first().and_then(|turn| turn.parts.first()) {
            round.turn.content = spoken.clone();
        }
        let mut answers = window
            .iter()
            .filter(|message| message.role == "tool")
            .filter_map(|message| match message.parts.first() {
                Some(Part::Text(text)) => Some(text.clone()),
                _ => None,
            });
        for call in &mut round.turn.calls {
            if let Some(result) = answers.next() {
                call.result = result;
            }
        }
        // Whatever the loop decided about this round, `finish` was the model's own word: keep it set on the round
        // that said it, so a caller reading its rounds back sees the same end signal the loop saw.
        round.turn.finish = round
            .turn
            .calls
            .iter()
            .any(|call| call.name == Tool::Finish.name())
            || (round.turn.calls.is_empty() && index + 1 == last);
    }
    Run {
        run,
        calls: asked,
        rounds,
    }
}

/// Answer a whole round's calls against the batch and return the §7 notes for them, in order — the lines a step
/// logs so a refusal (a rate over a captioned clip, §F3.10 S2) is SAID rather than quietly dropped. Same door the
/// loop uses for its own rounds; a caller that dialled its first round outside the loop needs it to reach the same
/// sentence.
pub fn answer_calls(calls: &[ToolCall], batch: &mut Clips) -> Vec<String> {
    calls.iter().map(|call| call_note_of(call, batch)).collect()
}

/// One answered call as §7 shows it. [`tool_loop::call_note`] is the log's wording — a bare
/// `[tool call name(args)]`, which is right in a scrolling log and says nothing about the outcome there.
fn call_note_of(call: &ToolCall, batch: &mut Clips) -> String {
    let mut done = call.clone();
    done.result = answer(call, batch);
    tool_loop::call_note(&done)
}

/// A finished §3.7 conversation: the loop's own record, plus the calls it saw and the rounds it ran.
pub struct Run {
    pub run: tool_loop::Run,
    /// Every call the model made, reads included, in the order it made them.
    pub calls: Vec<ToolCall>,
    /// One entry per request the loop made, each holding the conversation that round was shown and its ANSWERED
    /// calls. This is what lets a step file §7's page honestly: one section per round, results and all.
    pub rounds: Vec<Round>,
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
                    crate::exchanges::Part::Text(text) if !text.trim().is_empty() => {
                        Some(text.clone())
                    }
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
        args.get(key).and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|t| t.trim().parse::<f64>().ok()))
        })
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
            (Some(clip), Some(start), Some(end), Some(text)) => {
                batch.add_caption(clip, start, end, text)
            }
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

/// §3's reading of these three calls, as answered in `spec/QUESTIONS.md`: the tools ARE on the wire, and
/// "a reply that only carries a JSON document instead of tool calls [is] unreadable, asking again".
///
/// These three doors are the wording and the one-more-dial that decision needs, shared by all three passes for
/// the same reason `cut_captions::clip_outside` is: one judgement, phrased once, so captions, speed and
/// decorations cannot disagree about what an answer-without-a-call means.
///
/// The correction sent back into the conversation: what the model wrote instead of calling.
///
/// This is the sentence `llm_repair_leg::ask_repair_after` puts in the retry turn (`retryTurn`) and the one its
/// log quotes, so a person reading the log sees the model's own mistake, not a parse error it never made.
pub fn no_calls_problem(tool_name: &str) -> String {
    format!("the answer is a JSON document, not a tool call -- call `{tool_name}` (or `finish`) instead")
}

/// The failure a person reads when even the retry answered in prose. `step` is the pass's own step string, and
/// the callers wrap this unchanged (`!!! captions: {reason}`), so the sentence is final here.
pub fn no_calls_reason(step: &str, tool_name: &str) -> String {
    format!("{step}: no tool call in the answer -- asked again and got a JSON document instead of `{tool_name}`")
}

/// The log sink `ask_repair_after` will take. Its `log` argument must be a `&dyn Fn(&str)` of SOME concrete
/// type, and a `&dyn Fn` passed straight in is a `&dyn Fn` — a double indirection it cannot name — so the
/// caller's sink is wrapped in a small locally-typed closure. The wording it prints is
/// [`llm_repair_leg::repair_log`]'s, one call to it, so the two cannot drift.
fn log_through<'a>(log: &'a (dyn Fn(&str) + 'a)) -> Box<dyn Fn(&str) + 'a> {
    Box::new(move |line: &str| log(line))
}

/// Ask once more, and only once, for a tool call. §3's repair leg does the mechanics (the correction rides in
/// the conversation, one extra request, never two); this decides what counts as answered: a call, not prose.
///
/// * `Ok(reply)` — the second round called at least one tool, so the caller reads ITS calls as the answer.
/// * `Err(reason)` — it answered in prose again ([`no_calls_reason`]), or the extra dial itself failed.
///
/// The repair leg is [`crate::llm_repair_leg::ask_repair_after`] reached through its own dial/build closures, not
/// its own body: everything about the retry except the correction (§1's request contract, this step's `Mode`,
/// the offer the first round carried) belongs to the calling pass, which hands those two closures below.
/// `ask_repair_after` writes §3's `>>> <step>: the answer could not be used (…) -- asked again` line itself.
#[allow(clippy::too_many_arguments)] // three closures, each owned by the caller's own wire shape
#[allow(clippy::type_complexity)] // the two closures ARE the caller's wire shape; naming them elsewhere helps nothing
pub fn ask_again_for_calls(
    step: &str,
    tool_name: &str,
    messages: Vec<crate::exchanges::Message>,
    first: &crate::llm_request::Reply,
    log: &dyn Fn(&str),
    build: &dyn Fn(&[crate::exchanges::Message]) -> Result<String, String>,
    dial: &dyn Fn(&str) -> Result<(String, String), String>,
) -> Result<crate::llm_request::Reply, String> {
    // `retry_turn`'s `tools_active` is false on purpose: the model made NO call, so it is not mid-round and the
    // correction arrives as a `user` turn, which is what a model that answered with a document expects.
    // `thinking` is false for the same kind of reason one level up: all three clip-rule prompts are thinking-OFF
    // (`prompts::mode_of` over §4's table picks `Mode::Execute` for `captions`/`speed`/`effects`), so §3's
    // `thinkAgain` has nothing left to turn off and cannot change the retry.
    let verdict = |again: &crate::llm_request::Reply| {
        again
            .tool_calls
            .is_empty()
            .then(|| no_calls_problem(tool_name))
    };
    let repaired = crate::llm_repair_leg::ask_repair_after(
        step,
        &log_through(log),
        messages,
        false,
        false,
        &verdict,
        first,
        dial,
        &|messages, _thinking| build(messages),
    )?;
    let reply = repaired.reply()?;
    if reply.tool_calls.is_empty() {
        return Err(no_calls_reason(step, tool_name));
    }
    Ok(reply)
}

/// The chat envelope a round answered with, rebuilt for the cache. §6 keeps what the pass READ, and what these
/// three passes read out of a tool round is its CALLS, so the stored bytes carry them the same way the wire did
/// (which is also the shape `parse_speeds`/`parse_effects` already read back). `content` holds only what the
/// model SAID beside its calls, spelled out: a document in the content is not the answer (§3's answer on this
/// item: a call is), and a reader would otherwise prefer it over the calls.
pub fn answering_envelope(reply: &crate::llm_request::Reply) -> serde_json::Value {
    let said: Vec<String> = reply
        .tool_calls
        .iter()
        .map(|call| format!("{}({})", call.name, call.args))
        .collect();
    serde_json::json!({
        "choices": [{ "message": {
            "content": said.join(" "),
            "tool_calls": reply.tool_calls.iter().map(|call| serde_json::json!({
                "function": { "name": call.name, "arguments": call.args }
            })).collect::<Vec<_>>(),
        } }]
    })
}

/// Whether an answer carries any work call at all -- `finish` and a read (`get_frames`) aside. A round that
/// asked for none is a whole answer (§F3.9's `{clips:[]}` in tool shape), so nothing is cached for it: §6 will
/// not store a `nothing`, because a step that could still be answered later never would be.
pub fn asks_for_anything(reply: &crate::llm_request::Reply) -> bool {
    reply
        .tool_calls
        .iter()
        .any(|call| call.name != Tool::Finish.name() && call.name != Tool::GetFrames.name())
}

/// The one timed row `llm_retry_leg::chat` would have written for a request an ask sent WITHOUT riding that
/// door -- which is exactly the ask-again round: the repair leg owns how the retry is built and dialled, so it
/// cannot also go through the patience ladder, and §09 §10 still wants the request on the record ("every request
/// to any of them is timed and written to the project's `requests.tsv`"; a refused answer is still a request the
/// run paid for). `base` carries the identity fields of the ask that made it; the timings are the round's own
/// elapsed time on the same clock every other row is stamped on. `llm_retry_leg::chat` files a row only for what
/// IT dials, so a repair that had to wait leaves its wait rows there and this one on top: one line per request.
pub fn record_repair_row(
    tree: &crate::layout::Tree,
    mut base: crate::requests::Request,
    went_out: std::time::Instant,
) {
    let sent_ms = crate::server_leg::epoch_ms();
    let mut timer =
        crate::request_timing::Timer::sent(sent_ms - went_out.elapsed().as_millis() as i64);
    timer.finished(sent_ms);
    crate::request_timing::stamp_run(&mut base);
    let row = crate::request_timing::finish_row(&base, &timer, crate::requests::Outcome::Ok, 1);
    let _ = crate::requests::record(tree, &row);
}

/// The batch a §3.7 ask was built about, in the shape [`Clips`] wants.
///
/// The captions and speeds passes carry `(number, length)` pairs, and a clip's captions are OFFSETS inside that
/// clip (§F3.9), so every clip starts at zero in that space. A wrong `start` would make the tool clamp into the
/// wrong clip — the very mistake §3.7's tool exists to prevent.
pub fn batch_of(clips: &[(u32, f64)]) -> Clips {
    Clips::new(
        &clips
            .iter()
            .map(|(n, length)| (*n, 0.0, *length))
            .collect::<Vec<_>>(),
    )
}
