//! F6.1 — `spec/09-llm-and-tools.md` §2, the tool protocol.
//!
//! A model is offered its job's tools and may keep asking for them for [`roles::TOOL_ROUNDS`] rounds. This
//! module holds that loop as plain state and pure functions, driven by two closures — one that asks the model
//! and one that runs a tool — so every rule in §2 (the round log, the once-only nudge, stopping before
//! undeliverable tools, the remembered refusal) is testable with no server and no browser.
//!
//! The steps are numbered as the spec numbers them: S1 the offer with its schemas, S2 the loop and its round
//! line, S3 a round without calls, S4 rounds exhausted, S5 a server refusing the `tools` field, S6 tool errors
//! as text, S7 everything reaching the exchange page.
//!
//! # Names
//!
//! A log line names a step by its client name (`suggest`, `narrate`, `publish`, `transcript`), never by its
//! prompt key (`cut`, `youtube`, `fix`) — §2's prototype note, and the reason [`run`] takes `step: &str`: the
//! caller passes the name a person would read in the log, not the key a config file is filed under.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::degraded::{self, ToolsFallback};
use crate::exchanges::{Message, Part, ToolCall};
use crate::roles;
use crate::tools::{self, Tool};

/// machine.argsPreviewBytes — how much of a call's raw argument JSON a log line carries. 60 bytes because the
/// line is one of many in a run and its job is to identify the call, not to reproduce it: an argument can hold
/// a whole paragraph of narration text, which would push the rest of the round off the screen. Cut on bytes so
/// the cap means what it says; cut back to a character boundary so it never prints half a letter.
pub const ARGS_PREVIEW_BYTES: usize = 60;

/// §2 S3's one sentence, asked once. It is a question rather than an instruction because the model may genuinely
/// be finished and simply not have called `finish`; the next round is taken as finished whatever it says.
pub const FINISH_NUDGE: &str = "Call finish when you are done, or continue with the tools.";

// ---- S1: the offer -------------------------------------------------------------------

/// The job's tools as the wire wants them: a name, and the description that *is* the instruction telling the
/// model when to reach for the tool.
///
/// Descriptions come from the caller rather than from this module because each is written by the section that
/// owns the tool — `web_search`/`web_read` quote [`crate::prompts`] wording (§2's "descriptions verbatim in
/// prompts/tools.md"), and a job-specific tool's description belongs to its own §3 row. A tool with no
/// description yet is offered name-only: inventing one here would be an instruction the model reads that no
/// section wrote.
pub fn schemas(offered: &[Tool], describe: &dyn Fn(&Tool) -> Option<&'static str>) -> Vec<Value> {
    offered
        .iter()
        .map(|tool| {
            let mut schema = json!({ "type": "function", "function": { "name": tool.name() } });
            if let Some(text) = describe(tool) {
                schema["function"]["description"] = json!(text);
            }
            schema
        })
        .collect()
}

// ---- S2: the loop's round line --------------------------------------------------------

/// A call's arguments as the log shows them: raw JSON, cut at [`ARGS_PREVIEW_BYTES`] bytes on a character
/// boundary, with `…` only when something was actually removed — an ellipsis after a short argument would tell
/// a reader the model said more than it did.
pub fn args_preview(args: &str) -> String {
    if args.len() <= ARGS_PREVIEW_BYTES {
        return args.to_string();
    }
    // Walk back from the cap until the byte is a UTF-8 boundary: slicing at 60 would panic mid-character.
    let mut cut = ARGS_PREVIEW_BYTES;
    while cut > 0 && !args.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &args[..cut])
}

/// One call as the round line names it: `name{args}` — the arguments directly after the name, which is how the
/// prototype wrote them and how a reader tells `finish` (no braces) from `finish{}`.
pub fn call_preview(call: &ToolCall) -> String {
    format!("{}{}", call.name, args_preview(&call.args))
}

/// Did the model ask for exactly what it asked for last round? Compared on name and arguments together, because
/// a same-named call with different arguments is progress, not a loop.
pub fn same_calls(previous: &[ToolCall], calls: &[ToolCall]) -> bool {
    previous.len() == calls.len()
        && previous
            .iter()
            .zip(calls.iter())
            .all(|(was, now)| was.name == now.name && was.args == now.args)
}

/// `>>> <step>: round 2 of 8 — the model asked for get_lines{"from":1,"to":9}` (§2 S2 verbatim: em dash, calls
/// joined by `", "`). When the set is identical to last round's the line says so instead — that repetition is
/// the tell for a model stuck in a loop, and it is worth more to a reader than the arguments again.
pub fn round_log(step: &str, round: u32, calls: &[ToolCall], repeats: bool) -> String {
    let asked = calls
        .iter()
        .map(call_preview)
        .collect::<Vec<String>>()
        .join(", ");
    if repeats {
        return format!(
            ">>> {step}: round {round} of {} \u{2014} asks for the same thing again \u{2014} {asked}",
            roles::TOOL_ROUNDS
        );
    }
    format!(
        ">>> {step}: round {round} of {} \u{2014} the model asked for {asked}",
        roles::TOOL_ROUNDS
    )
}

// ---- S3: a round without calls --------------------------------------------------------

/// What a round decided the caller should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// The model asked for tools: run them and ask again.
    RunTools,
    /// The flow is complete — `finish` was called, or the nudge round passed without one.
    Finished,
    /// Ask [`FINISH_NUDGE`] once, then treat the next quiet round as finished.
    AskFinish,
}

/// S3: calls → run them; none and `finish` called → complete; none and this is the first quiet round → ask
/// [`FINISH_NUDGE`] once; none again after that → taken as finished with what was produced.
///
/// `silent_rounds` counts the rounds already spent quiet, so 0 means "this one asks". The cap is
/// [`tools::asked_to_finish`]'s — a rule this module reuses rather than restates, because a caller that asked
/// twice would be running a round the spec does not give the model.
pub fn after_round(calls: &[ToolCall], finish_called: bool, silent_rounds: u32) -> Next {
    if !calls.is_empty() {
        return Next::RunTools;
    }
    if finish_called {
        return Next::Finished;
    }
    if tools::asked_to_finish(silent_rounds + 1) {
        return Next::AskFinish;
    }
    Next::Finished
}

// ---- S4: rounds exhausted -------------------------------------------------------------

/// Whether this round's tools may run at all. The prototype ran the last round's calls and then threw their
/// results away — a web search of up to 45 seconds each, never sent, never logged, never recorded. §2 says the
/// rewrite MUST NOT keep that, so the loop asks this first and stops before running work it cannot deliver.
pub fn runs_tools(round: u32) -> bool {
    !tools::rounds_exhausted(round)
}

/// `!!! <step>: still calling tools after 8 rounds — the step gets no answer from this call` (§2 S4 verbatim,
/// counting from [`roles::TOOL_ROUNDS`]).
///
/// This is §2's wording; [`tools::exhausted_log`] carries an earlier item's shorter one (`-- no answer from it`)
/// and its test still pins that, so both exist until one round reconciles them.
pub fn exhausted(step: &str) -> String {
    format!(
        "!!! {step}: still calling tools after {} rounds \u{2014} the step gets no answer from this call",
        roles::TOOL_ROUNDS
    )
}

// ---- S5: a server that refuses the tools field ----------------------------------------

/// Which servers have already refused a `tools` field. The prototype re-offered the field on every attempt and
/// re-logged the refusal each time — narrate's three attempts logged it three times for one server. Remembering
/// per server means the second attempt simply does not send it.
#[derive(Debug, Default, Clone)]
pub struct Refused {
    servers: BTreeSet<String>,
}

impl Refused {
    pub fn new() -> Self {
        Self::default()
    }

    /// A server that answered "no tools" to this request.
    pub fn remember(&mut self, server: &str) {
        self.servers.insert(server.to_string());
    }

    /// Has this server already said no? True means do not offer the field again and do not log it again.
    pub fn server(&self, server: &str) -> bool {
        self.servers.contains(server)
    }
}

/// S5: an error on round 0 that is not a stop is read as a refusal of the `tools` field, so the identical
/// request goes out again without it. "Not a stop", "refused" and "round 0 or 1" are already
/// [`degraded::tools_fallback`]'s judgement — reused rather than re-derived, which is also what keeps a dead
/// server (a transport death, a 500) from being misread as a refusal. A refusal already honoured does not get
/// asked twice.
pub fn retry_without_tools(error: &str, round: u32, already_refused: bool) -> bool {
    if already_refused {
        return false;
    }
    matches!(
        degraded::tools_fallback(error, round),
        ToolsFallback::RunAsOneJsonAnswer
    ) && degraded::tools_refused(error)
}

/// `>>> <step>: the server refused the request with tools (<error>) -- asked again without them`. The reason is
/// quoted because "refused" covers a 400 about a schema, a 501 and a proxy that strips the field, and those want
/// different fixes.
pub fn refused_log(step: &str, error: &str) -> String {
    format!(">>> {step}: the server refused the request with tools ({error}) -- asked again without them")
}

/// Where a transport retry starts from: the index of the first message this loop added, so the whole tool loop
/// is re-run from the original messages rather than continuing a conversation whose last request never arrived.
/// Continuing from the grown list would leave an assistant turn with no matching tool results — which some
/// servers reject outright, turning one dropped connection into a hard failure.
pub fn rerun_from(original_len: usize) -> usize {
    original_len
}

// ---- S6: a tool that fails -------------------------------------------------------------

/// A tool's text back to the model. A problem is [`tools::error`]'s `{"error": "…"}`, which is an answer the
/// model can act on — it may pick another tool, or another argument, or call `finish` with what it has.
pub fn tool_result(call: &ToolCall) -> String {
    if call.result.is_empty() {
        return tools::error("the tool returned nothing");
    }
    call.result.clone()
}

/// S6: never. A tool failing is information for the model, not a reason to lose the step; the job's answer comes
/// from a later round or from `finish`, and the only way to end the flow is S3 or S4.
pub fn fails_the_job(_tool_failed: bool) -> bool {
    false
}

// ---- S7: the exchange page -------------------------------------------------------------

/// A call as §7's page shows it: `[tool call name(args)]`, appended to the reply. The page is written by
/// [`crate::exchanges`], and neither drawing it nor a tool failing may fail the call — recording is a report on
/// the work, never part of it.
pub fn call_note(call: &ToolCall) -> String {
    format!("[tool call {}({})]", call.name, args_preview(&call.args))
}

/// The `tool` message carrying one result to the next round. One per call, in the order the model asked for
/// them, because a reply that names its answers by position reads them wrongly if they arrive shuffled.
pub fn tool_message(call: &ToolCall) -> Message {
    Message { role: "tool".to_string(), parts: vec![Part::Text(tool_result(call))] }
}

/// The assistant turn that asked for the calls, so the next request shows what it already asked for. Its text is
/// whatever the model said alongside the calls — often nothing, and an empty text part is kept rather than
/// dropped so the turn exists at all.
pub fn assistant_turn(turn: &Turn) -> Message {
    Message { role: "assistant".to_string(), parts: vec![Part::Text(turn.content.clone())] }
}

// ---- the loop --------------------------------------------------------------------------

/// One round's answer from the model.
#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub content: String,
    pub calls: Vec<ToolCall>,
    /// The job's own `finish` tool was called.
    pub finish: bool,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// S3: complete, whether or not the nudge was needed. `asked_finish` says the model never called `finish`.
    Finished { asked_finish: bool },
    /// S4: still calling tools when the budget ran out, so this call answers nothing.
    Exhausted,
}

/// What a run left behind: the grown conversation, every log line in order, and how it ended.
#[derive(Debug, Clone)]
pub struct Run {
    pub messages: Vec<Message>,
    pub logs: Vec<String>,
    pub outcome: Outcome,
}

impl Run {
    /// The lines a caller appends to its own log; kept as a slice so a test can pin the whole run at once.
    pub fn joined(&self) -> String {
        self.logs.join("\n")
    }
}

/// F6.1's loop in one call: ask, run what was asked for, feed the results back, and stop at
/// [`roles::TOOL_ROUNDS`] rounds.
///
/// A round that arrives with calls when the budget is already spent is answered by S4 without running anything.
/// The loop therefore asks once more than it can run tools: `rounds_exhausted(9)` is only reachable after eight
/// rounds of work, so the ninth ask is where "still calling tools" is discovered — and nothing runs there.
///
/// `ask` is the model (it sees the messages and the offer; an `Err` is a transport or server error), `call` runs
/// one tool and returns its text. The loop never decides what a tool means — that is each tool's own section —
/// which is what lets every rule here be tested with two closures.
pub fn run(
    step: &str,
    messages: Vec<Message>,
    offered: &[Tool],
    mut ask: impl FnMut(&[Message], &[Tool]) -> Result<Turn, String>,
    mut call: impl FnMut(&ToolCall) -> String,
) -> Run {
    let mut run = Run { messages, logs: Vec::new(), outcome: Outcome::Exhausted };
    let mut silent_rounds = 0u32;
    // Whether the flow ended only because the nudge round passed without a `finish` call.
    let mut asked_finish = false;
    let mut previous: Vec<ToolCall> = Vec::new();

    for round in 1..=roles::TOOL_ROUNDS + 1 {
        let turn = match ask(&run.messages, offered) {
            Ok(turn) => turn,
            // S5 is the caller's to honour (it owns the request and its refusal memory); the loop reports the
            // error and stops rather than inventing a retry it cannot perform.
            Err(why) => {
                run.logs.push(format!(">>> {step}: {why}"));
                return run;
            }
        };

        if turn.calls.is_empty() {
            // `finish` is the flow's own end and says so here: a model that was nudged and then called it did
            // call it, so the run is not recorded as having needed asking.
            if turn.finish {
                run.outcome = Outcome::Finished { asked_finish: false };
                return run;
            }
            match after_round(&turn.calls, false, silent_rounds) {
                Next::Finished => {
                    // Complete without `finish`; `asked_finish` says whether an earlier round was nudged.
                    run.outcome = Outcome::Finished { asked_finish };
                    return run;
                }
                Next::AskFinish => {
                    // Count the round as quiet *before* pushing the nudge: `silent_rounds` is what the next
                    // round's `after_round` reads, and a second quiet round must fall through to Finished.
                    silent_rounds += 1;
                    asked_finish = true;
                    run.messages.push(Message { role: "user".to_string(), parts: vec![Part::Text(FINISH_NUDGE.to_string())] });
                    run.logs.push(format!(">>> {step}: {FINISH_NUDGE}"));
                }
                Next::RunTools => unreachable_run(&mut run.logs, "a round with no calls"),
            }
            continue;
        }

        // S4 before S2: the budget is spent before this round's work, so nothing runs that cannot be delivered.
        if !runs_tools(round) {
            run.logs.push(exhausted(step));
            run.outcome = Outcome::Exhausted;
            return run;
        }

        let repeats = same_calls(&previous, &turn.calls);
        run.logs.push(round_log(step, round, &turn.calls, repeats));
        run.messages.push(assistant_turn(&turn));
        for asked in &turn.calls {
            let mut done = asked.clone();
            done.result = call(asked);
            // S7: the note goes on the reply as well as into the next request's `tool` message.
            run.logs.push(call_note(&done));
            run.messages.push(tool_message(&done));
        }
        previous = turn.calls;
        silent_rounds = 0;
        if turn.finish {
            // A `finish` that arrived with calls still ends the flow — the tool ran and was answered, and the
            // model has said it is done.
            run.outcome = Outcome::Finished { asked_finish: false };
            return run;
        }
    }

    // The budget is spent and the model went quiet only after the last round's tools ran: S4's line belongs to a
    // round that arrived with calls, so this is the one other way a run ends without an answer.
    run.logs.push(exhausted(step));
    run.outcome = Outcome::Exhausted;
    run
}

/// A branch the spec does not have. Logged rather than panicked on: a tool loop that dies takes a person's whole
/// run with it, and an honest line in the log is cheaper than a lost session.
fn unreachable_run(logs: &mut Vec<String>, where_: &str) {
    logs.push(format!(">>> tool loop reached a round it should not have: {where_}"));
}
