//! F6.1 — `spec/09-llm-and-tools.md` §2, the tool protocol.
//!
//! A model may keep asking for its job's tools for [`roles::TOOL_ROUNDS`] rounds. The loop is plain state and
//! pure functions driven by two closures (ask the model, run a tool), so every §2 rule is testable with no
//! server. Steps are numbered as the spec numbers them (S1 offer ... S7 exchange page). A log line names a
//! step by its client name (`suggest`, `narrate`, `publish`, `transcript`), never its prompt key.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::decision_audit;
use crate::degraded::{self, ToolsFallback};
use crate::exchanges::{Message, Part, ToolCall};
use crate::roles;
use crate::tools::{self, Tool};

/// machine.argsPreviewBytes — how much of a call's raw argument JSON a log line carries. Cut on bytes, then
/// back to a character boundary so it never prints half a letter.
pub const ARGS_PREVIEW_BYTES: usize = 60;

/// §2 S3's one sentence, asked once. A question, because the model may be finished without having called
/// `finish`; the next round is taken as finished whatever it says.
pub const FINISH_NUDGE: &str = "Call finish when you are done, or continue with the tools.";

/// The job's tools as the wire wants them: a name, and the description that tells the model when to reach for it.
///
/// Descriptions come from the caller, written by the section that owns each tool (`web_search`/`web_read` quote
/// [`crate::prompts`]). A tool with no description yet is offered name-only rather than given an invented one.
pub fn schemas(offered: &[Tool], describe: &dyn Fn(&Tool) -> Option<&'static str>) -> Vec<Value> {
    offered
        .iter()
        .map(|tool| {
            let mut schema = json!({
                "type": "function",
                "function": { "name": tool.name(), "parameters": tool.parameters() }
            });
            if let Some(text) = describe(tool) {
                schema["function"]["description"] = json!(text);
            }
            schema
        })
        .collect()
}

/// A call's arguments as the log shows them: raw JSON cut at [`ARGS_PREVIEW_BYTES`] on a character boundary,
/// with `…` only when something was actually removed.
pub fn args_preview(args: &str) -> String {
    if args.len() <= ARGS_PREVIEW_BYTES {
        return args.to_string();
    }
    // Walk back to a UTF-8 boundary: slicing at 60 would panic mid-character.
    let mut cut = ARGS_PREVIEW_BYTES;
    while cut > 0 && !args.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &args[..cut])
}

/// One call as the round line names it: `name{args}`, so a reader tells `finish` from `finish{}`.
pub fn call_preview(call: &ToolCall) -> String {
    format!("{}{}", call.name, args_preview(&call.args))
}

/// Did the model ask for exactly what it asked last round? Name and arguments together: same name with
/// different arguments is progress.
pub fn same_calls(previous: &[ToolCall], calls: &[ToolCall]) -> bool {
    previous.len() == calls.len()
        && previous
            .iter()
            .zip(calls.iter())
            .all(|(was, now)| was.name == now.name && was.args == now.args)
}

/// `>>> <step>: round 2 of 8 — the model asked for get_lines{"from":1,"to":9}` (§2 S2 verbatim: em dash, calls
/// joined by `", "`). An identical set to last round's says so instead: the tell for a model stuck in a loop.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    RunTools,
    Finished,
    AskFinish,
}

/// S3: calls → run them; none and `finish` called → complete; none and the first quiet round → ask
/// [`FINISH_NUDGE`] once; none again → taken as finished.
///
/// `silent_rounds` counts rounds already spent quiet, so 0 means "this one asks". The cap is
/// [`tools::asked_to_finish`]'s, reused rather than restated.
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

/// Whether this round's tools may run at all. The prototype ran the last round's calls and threw the results
/// away (a web search is up to 45 s); §2 says the rewrite MUST NOT, so the loop stops before undeliverable work.
pub fn runs_tools(round: u32) -> bool {
    !tools::rounds_exhausted(round)
}

/// `!!! <step>: still calling tools after 8 rounds — the step gets no answer from this call` (§2 S4 verbatim,
/// counting from [`roles::TOOL_ROUNDS`]).
///
/// [`tools::exhausted_log`] carries an earlier item's shorter wording and its test still pins it.
pub fn exhausted(step: &str) -> String {
    format!(
        "!!! {step}: still calling tools after {} rounds \u{2014} the step gets no answer from this call",
        roles::TOOL_ROUNDS
    )
}

/// Which servers have already refused a `tools` field, so a second attempt simply does not send it (the
/// prototype logged the refusal on every attempt).
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

    /// Has this server already said no? True means do not offer the field or log it again.
    pub fn server(&self, server: &str) -> bool {
        self.servers.contains(server)
    }
}

/// S5: an error on round 0 that is not a stop is read as a refusal of the `tools` field, so the identical
/// request goes out again without it. The judgement is [`degraded::tools_fallback`]'s, which keeps a dead
/// server (a transport death, a 500) from being misread as a refusal. A refusal already honoured is not asked twice.
pub fn retry_without_tools(error: &str, round: u32, already_refused: bool) -> bool {
    if already_refused {
        return false;
    }
    matches!(
        degraded::tools_fallback(error, round),
        ToolsFallback::RunAsOneJsonAnswer
    ) && degraded::tools_refused(error)
}

/// `>>> <step>: the server refused the request with tools (<error>) -- asked again without them`. The reason
/// is quoted because "refused" covers a 400 about a schema, a 501 and a proxy that strips the field.
pub fn refused_log(step: &str, error: &str) -> String {
    format!(">>> {step}: the server refused the request with tools ({error}) -- asked again without them")
}

/// A tool's text back to the model. A problem is [`tools::error`]'s `{"error": "…"}`, an answer the model can act on.
pub fn tool_result(call: &ToolCall) -> String {
    if call.result.is_empty() {
        return tools::error("the tool returned nothing");
    }
    call.result.clone()
}

/// A call as §7's page shows it: `[tool call name(args)]`, appended to the reply. Recording is a report on
/// the work, never part of it: neither drawing the page nor a tool failing may fail the call.
pub fn call_note(call: &ToolCall) -> String {
    format!("[tool call {}({})]", call.name, args_preview(&call.args))
}

/// The `tool` message carrying one result to the next round, one per call in the order asked (a reply naming
/// answers by position reads them wrongly if shuffled).
pub fn tool_message(call: &ToolCall) -> Message {
    Message {
        role: "tool".to_string(),
        parts: vec![Part::Text(tool_result(call))],
    }
}

/// The assistant turn that asked for the calls. Its text is whatever the model said alongside, often nothing;
/// an empty text part is kept so the turn exists.
pub fn assistant_turn(turn: &Turn) -> Message {
    Message {
        role: "assistant".to_string(),
        parts: vec![Part::Text(turn.content.clone())],
    }
}

/// One round's answer from the model.
#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub content: String,
    pub calls: Vec<ToolCall>,
    pub finish: bool,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Finished { asked_finish: bool },
    Exhausted,
}

#[derive(Debug, Clone)]
pub struct Run {
    pub messages: Vec<Message>,
    pub logs: Vec<String>,
    pub outcome: Outcome,
    /// Which of §1's homes carried an announcement during this run, in the order they first did, so the two
    /// hearing homes are observable on a live run ([`crate::decision_audit`] is the classifier).
    pub announced: Vec<&'static str>,
}

/// The line saying a round's calls went unanswered because the budget was spent (§2 S4's consequence).
///
/// It names the home the news should have travelled in ([`crate::decision_audit::Home::ToolResult`]), and
/// is built from the predicate so it never promises a report that never arrives.
pub fn unannounced_calls_log(step: &str, unrun: usize) -> String {
    let home = decision_audit::Home::ToolResult;
    if !decision_audit::change_is_announced(home) {
        return format!(">>> {step}: {unrun} call(s) not run");
    }
    format!(
        ">>> {step}: {unrun} call(s) not run \u{2014} their {} ({}) was never sent back to the model",
        home.label(),
        home.scope()
    )
}

impl Run {
    /// The lines a caller appends to its own log.
    pub fn joined(&self) -> String {
        self.logs.join("\n")
    }
}

/// F6.1's loop in one call: ask, run what was asked for, feed the results back, stop at [`roles::TOOL_ROUNDS`].
///
/// A round with calls after the budget is spent is answered by S4 without running anything, so the loop asks
/// once more than it can run tools: "still calling tools" is discovered on the ninth ask.
///
/// `ask` is the model (an `Err` is a transport or server error), `call` runs one tool and returns its text.
pub fn run(
    step: &str,
    messages: Vec<Message>,
    offered: &[Tool],
    mut ask: impl FnMut(&[Message], &[Tool]) -> Result<Turn, String>,
    mut call: impl FnMut(&ToolCall) -> String,
) -> Run {
    let mut run = Run {
        messages,
        logs: Vec::new(),
        outcome: Outcome::Exhausted,
        announced: Vec::new(),
    };
    let mut silent_rounds = 0u32;
    // Whether the flow ended only because the nudge round passed without a `finish` call.
    let mut asked_finish = false;
    let mut previous: Vec<ToolCall> = Vec::new();

    for round in 1..=roles::TOOL_ROUNDS + 1 {
        let turn = match ask(&run.messages, offered) {
            Ok(turn) => turn,
            // S5 is the caller's to honour (it owns the request and its refusal memory); the loop reports and stops.
            Err(why) => {
                run.logs.push(format!(">>> {step}: {why}"));
                return run;
            }
        };

        // §2 S3's end signal is the `finish` CALL, not the absence of other calls: a model may ask for work AND say
        // it is done in one round (`add_caption(…) + finish`). Dialling again would waste a request and, with the
        // budget spent (§2 S4), end as "still calling tools"; so a round that called `finish` ENDS the run after its
        // calls are answered below. `Turn::finish` means the same and is honoured too.
        let closed = turn.finish
            || turn
                .calls
                .iter()
                .any(|call| call.name == Tool::Finish.name());

        if turn.calls.is_empty() {
            // A model that was nudged and then called `finish` did call it, so the run is not recorded as having needed asking.
            if turn.finish {
                run.outcome = Outcome::Finished {
                    asked_finish: false,
                };
                return run;
            }
            match after_round(&turn.calls, false, silent_rounds) {
                Next::Finished => {
                    // Complete without `finish`; `asked_finish` says whether an earlier round was nudged.
                    run.outcome = Outcome::Finished { asked_finish };
                    return run;
                }
                Next::AskFinish => {
                    // Count the round as quiet *before* pushing the nudge: a second quiet round must fall through to Finished.
                    silent_rounds += 1;
                    asked_finish = true;
                    let finish_home = decision_audit::Home::FinishAnswer;
                    if decision_audit::change_is_announced(finish_home)
                        && finish_home.may_act_in_same_conversation()
                    {
                        run.messages.push(Message {
                            role: "user".to_string(),
                            parts: vec![Part::Text(FINISH_NUDGE.to_string())],
                        });
                        run.logs.push(format!(">>> {step}: {FINISH_NUDGE}"));
                        note_home(&mut run.announced, finish_home);
                    }
                }
                Next::RunTools => unreachable_run(&mut run.logs, "a round with no calls"),
            }
            continue;
        }

        // S4 before S2: nothing runs that cannot be delivered.
        if !runs_tools(round) {
            run.logs.push(exhausted(step));
            // The model must not be left holding calls it never heard about; the audit decides whether that sentence is
            // true ([`unannounced_calls_log`]), and when it is not, §2's line above stands alone.
            if decision_audit::change_is_announced(decision_audit::Home::ToolResult) {
                run.logs.push(unannounced_calls_log(step, turn.calls.len()));
            }
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
            // The result text just fed back IS §1's second home: per-item, so the model can act on it in this conversation.
            note_home(&mut run.announced, decision_audit::Home::ToolResult);
        }
        previous = turn.calls;
        silent_rounds = 0;
        if closed {
            // A `finish` that arrived with calls still ends the flow; its verdict on the WHOLE travelled in that result,
            // so this run used §1's third home as well.
            note_home(&mut run.announced, decision_audit::Home::FinishAnswer);
            run.outcome = Outcome::Finished {
                asked_finish: false,
            };
            return run;
        }
    }

    // The budget is spent and the model went quiet only after the last round's tools ran: the other way a run ends without an answer.
    run.logs.push(exhausted(step));
    run.outcome = Outcome::Exhausted;
    run
}

/// Record that one of §1's hearing homes carried news to the model during this run, once per home.
pub fn note_home(announced: &mut Vec<&'static str>, home: decision_audit::Home) {
    let label = home.label();
    if !announced.contains(&label) {
        announced.push(label);
    }
}

/// A branch the spec does not have. Logged rather than panicked on: a dead tool loop takes the whole run with it.
fn unreachable_run(logs: &mut Vec<String>, where_: &str) {
    logs.push(format!(
        ">>> tool loop reached a round it should not have: {where_}"
    ));
}
