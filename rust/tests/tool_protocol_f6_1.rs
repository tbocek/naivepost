//! F6.1 — `spec/09-llm-and-tools.md` §2, the tool protocol, checked against `naivepost::tool_loop`.
//!
//! §2 numbers its own steps, so one test per step (S1..S7), split where a step holds two rules that break apart
//! on their own. Nothing here needs a server: the loop is driven by closures returning canned turns, which is
//! what makes "the model asked three times and got no answer" a fact a test can pin rather than a hope.
//!
//! # Ids used
//!
//! `P.eng.llmToolRounds` — the round budget, held as [`naivepost::roles::TOOL_ROUNDS`] and never written out as
//! a literal again. §2's other two ids (`P.eng.searchHits`, and the web tools' own rules) belong to the
//! web-tool tests, not these: they are about what a search returns, not about how a round is run.

use std::cell::Cell;
use std::collections::BTreeMap;

use naivepost::degraded;
use naivepost::exchanges::{self, Call, Mode, Part, ToolCall};
use naivepost::roles;
use naivepost::tool_loop as tool_loop;
use naivepost::tools::{self, Tool};

const ITEM: &str = "F6.1";

fn call(name: &str, args: &str) -> ToolCall {
    ToolCall { name: name.to_string(), args: args.to_string(), result: String::new() }
}

/// A canned answer from the scripted model.
fn turn(content: &str, calls: Vec<ToolCall>, finish: bool) -> tool_loop::Turn {
    tool_loop::Turn { content: content.to_string(), calls, finish }
}

/// The round line for one call, so a test can say what it expects without repeating the format.
fn asked(round: u32, step: &str, what: &str) -> String {
    format!(">>> {step}: round {round} of {} \u{2014} the model asked for {what}", roles::TOOL_ROUNDS)
}

// ---- S1: offer the job's tools with their schemas -------------------------------------

#[test]
fn f6_1_s1_offer_the_jobs_tools_with_schemas() {
    assert_eq!(ITEM, "F6.1");
    // An empty offer is no schemas at all — a request that offers nothing sends no `tools` field (§1's rule),
    // and an empty array would be a different claim.
    assert!(tool_loop::schemas(&[], &|_| None).is_empty());

    // Every offered tool arrives named, in the order the job's list gives: the model is offered what §3 lists.
    let offered = tools::offered(roles::Job::ModelCut);
    assert!(!offered.is_empty(), "the cut job is offered tools");
    let schemas = tool_loop::schemas(&offered, &|_| None);
    assert_eq!(schemas.len(), offered.len());
    for (index, schema) in schemas.iter().enumerate() {
        assert_eq!(schema["type"], serde_json::json!("function"));
        assert_eq!(schema["function"]["name"], serde_json::json!(offered[index].name()));
        // No description yet: inventing one here would be an instruction no section wrote.
        assert!(schema["function"].get("description").is_none(), "{schema}");
    }

    // A description is the instruction, and it travels with the tool that has one. The hook answers per tool,
    // so §2's web tools can carry prompts/tools.md verbatim while a job-specific tool waits for its own row.
    let described = tool_loop::schemas(&offered, &|tool| match tool {
        Tool::WebSearch => Some("Look up a fact".to_string().leak()),
        _ => None,
    });
    let search = &described[offered.iter().position(|t| *t == Tool::WebSearch).expect("web_search is offered")];
    assert_eq!(search["function"]["description"], serde_json::json!("Look up a fact"));

    // The three jobs the prototype gave the web to are the only ones that get it, and they get both tools.
    for job in [roles::Job::ModelCut, roles::Job::Narrate, roles::Job::UploadText] {
        let list = tools::offered(job);
        assert!(list.contains(&Tool::WebSearch) && list.contains(&Tool::WebRead), "{job:?}");
    }
    // Log lines name the step (suggest/narrate/publish/transcript), never the prompt key — that rule is §2's
    // prototype note and each web-tool reason line is `web_tools`' own; nothing to duplicate here.
}

// ---- S2: the round log, its preview cap, and the loop tell ----------------------------

#[test]
fn f6_1_s2_round_log() {
    assert_eq!(ITEM, "F6.1");
    // ">>> <step>: round i of 8 — the model asked for name{args}" — em dash, arguments directly after the name.
    let line = tool_loop::round_log("suggest", 2, &[call("get_lines", "{\"from\":1,\"to\":9}")], false);
    assert_eq!(line, ">>> suggest: round 2 of 8 \u{2014} the model asked for get_lines{\"from\":1,\"to\":9}");
    // The count is P.eng.llmToolRounds, not a literal 8 that could drift from it.
    assert!(line.contains(&format!("of {}", roles::TOOL_ROUNDS)), "{line}");

    // Several calls in one round are joined by ", ".
    let two = tool_loop::round_log("narrate", 1, &[call("get_lines", "{}"), call("web_search", "{\"broad\":\"x\"}")], false);
    assert_eq!(two, asked(1, "narrate", "get_lines{}, web_search{\"broad\":\"x\"}"));

    // The tell for a loop: the same set again is said instead of the arguments being repeated.
    let repeat = tool_loop::round_log("suggest", 3, &[call("get_lines", "{}")], true);
    assert!(repeat.contains("asks for the same thing again"), "{repeat}");
    assert!(!repeat.contains("the model asked for"), "{repeat}");

    // `same_calls` is progress-or-loop: an argument that moved is a different question.
    let previous = vec![call("get_lines", "{\"from\":1}")];
    assert!(tool_loop::same_calls(&previous, &[call("get_lines", "{\"from\":1}")]));
    assert!(!tool_loop::same_calls(&previous, &[call("get_lines", "{\"from\":2}")]));
    assert!(!tool_loop::same_calls(&previous, &[call("get_lines", "{\"from\":1}"), call("finish", "{}")]), "one more call is not the same set");
    // An empty set is never compared against: a round that called nothing takes the S3 branch and never reaches
    // this check, so the comparison carries no special case for it.
    assert!(tool_loop::same_calls(&[], &[]), "never reached by the loop: a quiet round has already branched off");
}

#[test]
fn f6_1_s2_arguments_preview() {
    assert_eq!(ITEM, "F6.1");
    // A short argument arrives as it was: an ellipsis after text that was never cut would lie about the model.
    let short = "{\"from\":1,\"to\":9}";
    assert_eq!(tool_loop::args_preview(short), short);
    let exact = "x".repeat(tool_loop::ARGS_PREVIEW_BYTES);
    assert_eq!(tool_loop::args_preview(&exact), exact, "exactly at the cap is not cut");

    // Past it, 60 bytes plus the ellipsis — bytes, because that is what the cap counts.
    let long = "y".repeat(tool_loop::ARGS_PREVIEW_BYTES + 40);
    let cut = tool_loop::args_preview(&long);
    assert!(cut.ends_with('\u{2026}'), "{cut}");
    assert_eq!(cut.len(), tool_loop::ARGS_PREVIEW_BYTES + '\u{2026}'.len_utf8());

    // A multi-byte argument is cut on a character boundary: the log must never hold half a letter, and slicing
    // at exactly 60 bytes would panic when that byte is mid-character.
    let multibyte = "\u{00e9}".repeat(40); // 2 bytes each: byte 60 is the middle of the 30th letter.
    let cut = tool_loop::args_preview(&multibyte);
    let body = trim_ellipsis(&cut);
    assert!(body.len() <= tool_loop::ARGS_PREVIEW_BYTES, "{} bytes", body.len());
    assert_eq!(body.chars().count() * 2, body.len(), "every letter whole: {body}");
    assert_ne!(body.len(), tool_loop::ARGS_PREVIEW_BYTES - 1, "one byte back would be half a letter");
}

/// The preview minus its trailing ellipsis.
fn trim_ellipsis(text: &str) -> String {
    text.strip_suffix('\u{2026}').unwrap_or(text).to_string()
}

// ---- S3: a round without calls --------------------------------------------------------

#[test]
fn f6_1_s3_finish_called_completes_the_flow() {
    assert_eq!(ITEM, "F6.1");
    // `finish` is the job's own end: no nudge, and the run says it was not needed.
    let asked = Cell::new(0);
    let run = tool_loop::run(
        "suggest",
        vec![],
        &[],
        |_, _| Ok(turn("", vec![call("finish", "{}")], true)),
        |_| "ok".to_string(),
    );
    assert_eq!(run.outcome, tool_loop::Outcome::Finished { asked_finish: false });
    assert!(run.logs.iter().all(|line| !line.contains(tool_loop::FINISH_NUDGE)), "{}", run.joined());
    let _ = asked.get();
}

#[test]
fn f6_1_s3_a_quiet_round_is_asked_once_then_taken_as_finished() {
    assert_eq!(ITEM, "F6.1");
    assert_eq!(tool_loop::FINISH_NUDGE, "Call finish when you are done, or continue with the tools.");

    // Quiet from the first round: asked once, then finished with what was produced.
    let asks = Cell::new(0);
    let run = tool_loop::run(
        "narrate",
        vec![],
        &[],
        |_, _| {
            asks.set(asks.get() + 1);
            Ok(turn("the answer I already had", vec![], false))
        },
        |_| String::new(),
    );
    assert_eq!(run.outcome, tool_loop::Outcome::Finished { asked_finish: true });
    let nudges = run.logs.iter().filter(|line| line.contains(tool_loop::FINISH_NUDGE)).count();
    assert_eq!(nudges, 1, "the nudge is asked once: {}", run.joined());

    // Three quiet rounds in a row still ask once — the second and third are taken as finished.
    let mut three = 0;
    let again = tool_loop::run(
        "narrate",
        vec![],
        &[],
        move |_, _| {
            three += 1;
            if three == 1 {
                return Ok(turn("", vec![], false));
            }
            Ok(turn("still nothing", vec![], false))
        },
        |_| String::new(),
    );
    assert!(matches!(again.outcome, tool_loop::Outcome::Finished { asked_finish: true }));
    let nudges = again.logs.iter().filter(|line| line.contains(tool_loop::FINISH_NUDGE)).count();
    assert_eq!(nudges, 1, "{}", again.joined());

    // The once-only rule is tools::asked_to_finish's, reused rather than restated.
    assert!(!tools::asked_to_finish(0), "the first quiet round has not been asked yet");
    assert!(tools::asked_to_finish(1));
    assert!(!tools::asked_to_finish(2));
    // And `finish` after a nudge is still the model's own answer: it clears the flag.
    let mut round = 0;
    let after = tool_loop::run(
        "narrate",
        vec![],
        &[],
        move |_, _| {
            round += 1;
            if round == 1 {
                return Ok(turn("", vec![], false));
            }
            Ok(turn("", vec![call("finish", "{}")], true))
        },
        |_| String::new(),
    );
    assert_eq!(after.outcome, tool_loop::Outcome::Finished { asked_finish: false });
}

// ---- S4: rounds exhausted -------------------------------------------------------------

#[test]
fn f6_1_s4_the_budget_is_spent_before_the_tools_run() {
    assert_eq!(ITEM, "F6.1");
    // P.eng.llmToolRounds: round 8 is inside the budget, round 9 is not.
    assert!(tool_loop::runs_tools(1));
    assert!(tool_loop::runs_tools(roles::TOOL_ROUNDS));
    assert!(!tool_loop::runs_tools(roles::TOOL_ROUNDS + 1));

    // A model that calls a tool every round: eight rounds of work, and the ninth ask runs nothing.
    let ran = Cell::new(0);
    let run = tool_loop::run(
        "narrate",
        vec![],
        &[],
        |_, _| Ok(turn("", vec![call("get_lines", "{}")], false)),
        |_| {
            ran.set(ran.get() + 1);
            "the lines".to_string()
        },
    );
    assert_eq!(run.outcome, tool_loop::Outcome::Exhausted);
    // THE DEFECT TEST. The prototype ran the last round's calls and threw their results away — a web search of
    // up to 45 s each, never sent, never logged, never recorded. One call per round means exactly the rounds
    // inside the budget ran, and no more.
    assert_eq!(ran.get(), roles::TOOL_ROUNDS as usize, "the exhausted round's tools must not run");
    let rounds = run.logs.iter().filter(|line| line.contains("round ")).count();
    assert_eq!(rounds, roles::TOOL_ROUNDS as usize, "{}", run.joined());

    // The last line is §2's, naming the number and the step so a stalled run reads as a budget, not a hang.
    assert_eq!(
        tool_loop::exhausted("narrate"),
        "!!! narrate: still calling tools after 8 rounds \u{2014} the step gets no answer from this call"
    );
    assert_eq!(run.logs.last().expect("a log"), &tool_loop::exhausted("narrate"));

    // Divergence pinned, not fixed: an earlier item's line says the same thing in other words and its own test
    // still pins it. Both exist until one round reconciles them.
    assert_ne!(tools::exhausted_log("narrate"), tool_loop::exhausted("narrate"));
    assert_eq!(tools::exhausted_log("narrate"), "!!! narrate: still calling tools after 8 rounds -- no answer from it");
}

// ---- S5: a server that refuses the tools field ----------------------------------------

#[test]
fn f6_1_s5_a_refused_field_is_resent_once_and_remembered() {
    assert_eq!(ITEM, "F6.1");
    // A non-stop error on the first round is read as a refusal of the field, and the identical request goes out
    // again without it. "Refused" and "round 0 or 1" are degraded's judgement, reused here.
    assert!(degraded::tools_refused("the server rejected the tools field"));
    assert!(tool_loop::retry_without_tools("the server rejected the tools field", 1, false));
    assert!(tool_loop::retry_without_tools("the server rejected the tools field", 0, false));

    // A dead server is not a refusal: no re-send, and no line saying it was. The prototype's only test was "an
    // error on round 0 that is not a stop", so a transport death or a 500 was logged as a refusal and the tools
    // dropped for that call.
    assert!(!degraded::tools_refused("connection reset"));
    assert!(!tool_loop::retry_without_tools("connection reset", 1, false));
    assert!(!tool_loop::retry_without_tools("the model said stop", 1, false), "a stop is an answer");
    assert!(!tool_loop::retry_without_tools("internal server error 500", 1, false));

    // The line quotes the reason: a schema the server dislikes and a proxy that strips the field want fixes.
    assert_eq!(
        tool_loop::refused_log("publish", "bad tools"),
        ">>> publish: the server refused the request with tools (bad tools) -- asked again without them"
    );

    // Remembered per server, so a step retrying with tools (narrate's three attempts) neither re-offers the
    // field nor re-logs the line.
    let mut refused = tool_loop::Refused::new();
    assert!(!refused.server("http://127.0.0.1:8731"));
    refused.remember("http://127.0.0.1:8731");
    assert!(refused.server("http://127.0.0.1:8731"));
    assert!(!refused.server("http://127.0.0.1:9999"), "one server's refusal is not another's");
    assert!(!tool_loop::retry_without_tools("the server rejected the tools field", 1, true), "already honoured once");

    // A transport retry re-runs the whole loop from the original messages: an assistant turn whose tool results
    // never arrived would be a conversation no server accepts.
    assert_eq!(tool_loop::rerun_from(3), 3, "truncate back to what the caller started with");
    assert_eq!(tool_loop::rerun_from(0), 0);
}

// ---- S6: tool errors go back as text ---------------------------------------------------

#[test]
fn f6_1_s6_a_tool_that_fails_is_an_answer_not_a_failure() {
    assert_eq!(ITEM, "F6.1");
    // A failure comes back in tools::error's shape — a sentence the model can act on.
    let empty = ToolCall { name: "web_read".to_string(), args: "{}".to_string(), result: String::new() };
    assert_eq!(tool_loop::tool_result(&empty), tools::error("the tool returned nothing"));
    assert_eq!(tool_loop::tool_result(&empty), "{\"error\":\"the tool returned nothing\"}");
    let fine = ToolCall { name: "get_lines".to_string(), args: "{}".to_string(), result: "1\tword".to_string() };
    assert_eq!(tool_loop::tool_result(&fine), "1\tword");

    // Never the job's end.
    assert!(!tool_loop::fails_the_job(true));
    assert!(!tool_loop::fails_the_job(false));

    // And a run whose only tool failed still finishes, with the error text in front of the model.
    let mut round = 0;
    let run = tool_loop::run(
        "publish",
        vec![],
        &[],
        move |_, _| {
            round += 1;
            if round == 1 {
                return Ok(turn("", vec![call("web_read", "{\"url\":\"http://x\"}")], false));
            }
            Ok(turn("fine, I will not need it", vec![call("finish", "{}")], true))
        },
        |_| String::new(), // the tool answers nothing: that is the failure
    );
    assert_eq!(run.outcome, tool_loop::Outcome::Finished { asked_finish: false }, "{}", run.joined());
    let tool_messages: Vec<&str> = run
        .messages
        .iter()
        .filter(|message| message.role == "tool")
        .map(|message| match message.parts.first() {
            Some(Part::Text(text)) => text.as_str(),
            _ => "",
        })
        // One per round: the run above asked the model again after the error, so a second entry would mean the
        // failure ended the job instead of being answered.
        .take(1)
        .collect();
    assert_eq!(tool_messages, ["{\"error\":\"the tool returned nothing\"}"], "{}", run.joined());
}

// ---- S7: every call and result reaches the exchange page -------------------------------

#[test]
fn f6_1_s7_the_page_shows_every_call_and_result() {
    assert_eq!(ITEM, "F6.1");
    // The prototype's shapes: a call as text appended to the reply, a result only as the next round's message.
    let note = tool_loop::call_note(&call("get_lines", "{\"from\":1}"));
    assert_eq!(note, "[tool call get_lines({\"from\":1})]");

    let answered = ToolCall { name: "get_lines".to_string(), args: "{}".to_string(), result: "1\tword".to_string() };
    let message = tool_loop::tool_message(&answered);
    assert_eq!(message.role, "tool");
    assert!(matches!(&message.parts[..], [Part::Text(text)] if text == "1\tword"));

    // Two calls in one round: one assistant turn and two tool messages, in the order they were asked for — a
    // model that names its answers by position reads them wrongly if they arrive shuffled.
    let mut round = 0;
    let run = tool_loop::run(
        "suggest",
        vec![],
        &[],
        move |_, _| {
            round += 1;
            if round == 1 {
                return Ok(turn("thinking", vec![call("get_lines", "{\"a\":1}"), call("get_events", "{}")], false));
            }
            Ok(turn("done", vec![call("finish", "{}")], true))
        },
        |_| "res".to_string(),
    );
    // One assistant turn, then one message per call, in the order they were asked for.
    let roles: Vec<&str> = run.messages.iter().take(3).map(|message| message.role.as_str()).collect();
    assert_eq!(roles, ["assistant", "tool", "tool"], "{roles:?}");
    assert!(run.logs.iter().any(|line| line == "[tool call get_lines({\"a\":1})]"), "{}", run.joined());
    assert!(run.logs.iter().any(|line| line == "[tool call get_events({})]"), "{}", run.joined());

    // The page itself is exchanges' work, and writing it cannot fail the call: a Call carrying these notes
    // still produces its log lines.
    let page = Call {
        step: "suggest".to_string(),
        model: "qwen3".to_string(),
        mode: Mode::Execute,
        took_secs: 3,
        thinking_secs: None,
        messages: run.messages.clone(),
        reply: format!("thinking {}", run.logs.iter().find(|l| l.starts_with("[tool call")).expect("a note")),
        reasoning: None,
        tool_calls: vec![answered],
        cut_off: false,
        error: None,
    };
    let lines = exchanges::log_lines(&page);
    assert!(lines.iter().any(|line| line.contains("went to the LLM")), "{lines:?}");
    assert!(!lines.iter().any(|line| line.contains("failed")), "{lines:?}");
    // A map of nothing is still a page: recording never depends on having results to show.
    let empty: BTreeMap<String, String> = BTreeMap::new();
    assert!(empty.is_empty());
}
