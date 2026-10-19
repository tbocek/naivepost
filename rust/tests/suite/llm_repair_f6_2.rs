//! F6.2 §3's last bullet — "Content repair in degraded (JSON) mode: `noAnswer`, `cutOff`, `thinkAgain`,
//! `retryTurn`. With tools these become tool results."
//!
//! The patience half of §3 already had a wire test (`tests/suite/llm_retry_f6_2_wire.rs`): a call that died in
//! transit walks the ladder. What nothing proved was the OTHER branch — an answer that arrived whole and was
//! unusable. Until this round such a reply was resent UNCHANGED, which cannot help: same messages, same
//! thinking switch, same answer back. These checks pin the four repairs as decisions and then drive them over
//! a loopback fake, so what is asserted is the second request's bytes.
//!
//! One scenario per binary again (`XDG_CONFIG_HOME` and cwd are process-wide), hence the harness include and
//! the single live check; the pure decisions need no fixture.

#![allow(dead_code)]

// §3's short retry is a real 2 s wait and this file's live checks sit through it. The scale seam
// (`llm_retry_leg::nap_duration`) divides every wait; the log still names the seconds §3 spells.
fn init() {
    unsafe { std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000") };
}

// The harness is NOT included here: this target pins §3's decisions and the repair door against closures, never a
// socket, so it sets no process-wide environment (unlike the live scenarios, which are one fixture per binary).
use naivepost::exchanges::{Message, Part};
use naivepost::llm_repair_leg;
use naivepost::llm_request;
use naivepost::llm_retry::{self, Repair};

// ---- S3: which repair applies -------------------------------------------------------------------

/// `noAnswer` and `cutOff` name the two failures a JSON parser cannot tell apart on its own — both read as
/// "unexpected end of JSON input" — and they want opposite fixes: an empty answer wants the whole JSON
/// written, a truncated one wants LESS. A good document gets no repair at all.
#[test]
fn f6_2_s3_a_an_unusable_answer_names_its_repair_and_a_good_one_names_none() {
    // Empty, and reasoning-only, are the SAME failure: there is nothing to parse.
    let empty = llm_retry::repair("", "expected value at line 1 column 1");
    assert_eq!(
        empty.map(|(kind, _)| kind),
        Some(Repair::NoAnswer),
        "an empty reply is no answer, not a truncated one"
    );
    let reasoning = llm_retry::repair("I will now write the JSON.</think>", "unexpected end of JSON input");
    assert_eq!(
        reasoning.as_ref().map(|(_, problem)| problem.as_str()),
        Some(llm_retry::NO_ANSWER),
        "the whole reply was reasoning: the model must be told to write the JSON, not to shorten it"
    );
    assert!(
        llm_retry::no_answer("Here is the cut:\n").is_none(),
        "words after the reasoning ARE an answer, however wrong their shape"
    );

    // Truncated: text survived but ran out. That is `cutOff`, and its sentence asks for fewer items.
    let truncated = llm_retry::repair("{\"clips\":[{\"i\":1,\"fx\"", "unexpected end of JSON input");
    assert_eq!(truncated.map(|(kind, _)| kind), Some(Repair::CutOff));
    assert_eq!(
        llm_retry::cut_off_problem("{\"clips\":[", "unexpected end of JSON input"),
        Some(naivepost::degraded::CUTOFF_RETRY),
        "and the wording is the prototype's, reused from `degraded` rather than retyped"
    );

    // Valid JSON in the wrong shape is neither: it is the pass's own complaint and gets no repair round.
    assert_eq!(
        llm_retry::repair("{\"not\":\"the captions shape\"}", "").map(|(kind, _)| kind),
        None,
        "a parsed answer is usable as far as §3's repairs go"
    );
    let malformed = llm_retry::repair("{\"clips\":[oops]}", "expected value at line 1 column 10");
    assert_eq!(
        malformed.map(|(kind, _)| kind),
        Some(Repair::Other),
        "malformed JSON is neither empty nor truncated: it wants care, not brevity"
    );
}

/// `thinkAgain`: an EMPTY answer means the thinking budget was spent with no words, and the same question
/// asked the same way answers it the same way, so the retry goes WITHOUT thinking. A model that wrote bad
/// JSON is writing and getting it wrong — switching its reasoning off makes the next answer worse.
#[test]
fn f6_2_s3_b_an_empty_answer_turns_thinking_off_for_the_retry_and_bad_json_keeps_it_on() {
    assert!(!llm_retry::think_again(true, ""), "empty answer: thinking off");
    assert!(
        !llm_retry::think_again(true, "all of it was reasoning</think>  "),
        "reasoning-only is the same empty answer"
    );
    assert!(
        llm_retry::think_again(true, "{\"clips\":[oops]"),
        "bad JSON keeps thinking: that model is writing and wrong"
    );
    assert!(!llm_retry::think_again(false, "{\"clips\":[]}"), "it was never on to begin with");
}

/// `retryTurn`: the rejected answer goes back into the conversation so the model can see what it got wrong,
/// followed by the correction in §3's words. An EMPTY answer is NOT echoed — some servers refuse a history
/// holding a blank assistant turn — and with tools the correction arrives as a tool result, because a model
/// mid-round expects its calls answered rather than a new instruction from the user.
#[test]
fn f6_2_s3_c_the_correction_goes_back_into_the_conversation_as_a_turn_or_a_tool_result() {
    let start = vec![Message { role: "user".to_string(), parts: vec![Part::Text("caption it".into())] }];

    let grown = llm_retry::retry_turn(start.clone(), "{\"clips\":[", naivepost::degraded::CUTOFF_RETRY, false);
    assert_eq!(grown.len(), 3, "the ask, the refused answer, the correction");
    assert_eq!(grown[1].role, "assistant", "the refused answer is echoed so the model sees its own truncation");
    assert_eq!(grown[2].role, "user", "in prose mode the correction comes from the user");
    let said = match &grown[2].parts[0] {
        Part::Text(text) => text.clone(),
        other => panic!("expected a text part, got {other:?}"),
    };
    assert!(
        said.contains(&format!("Your answer failed validation: {}", naivepost::degraded::CUTOFF_RETRY)),
        "the correction quotes the repair sentence: {said}"
    );
    assert!(
        said.ends_with("Return corrected strict JSON only."),
        "and closes with §3's exact instruction: {said}"
    );

    let blank = llm_retry::retry_turn(start.clone(), "   ", llm_retry::NO_ANSWER, false);
    assert_eq!(blank.len(), 2, "an empty answer is NOT echoed back: {blank:?}");
    assert_eq!(blank[1].role, "user", "the correction carries the whole message instead");

    let tool_mode = llm_retry::retry_turn(start, "{\"clips\":[", "p", true);
    assert_eq!(tool_mode[2].role, "tool", "with tools the correction is a tool result (§3's last line)");
}

/// The repair door's own contract, without a socket: one extra ask when the caller says the answer is
/// unusable, none when it is fine, and never a third. The first request is dialled HERE because that is how the
/// live flow works — `captions_ask` sends its own first round and hands the reply to [`ask_repair_after`].
#[test]
fn f6_2_s3_c_the_repair_door_asks_once_more_and_never_a_third_time() {
    let log = std::cell::RefCell::new(Vec::new());
    let dials = std::cell::RefCell::new(Vec::new());
    let build = |messages: &[Message], thinking: bool| -> Result<String, String> {
        dials.borrow_mut().push((messages.len(), thinking));
        Ok(format!("body{}", dials.borrow().len()))
    };
    let dial = |body: &str| -> Result<(String, String), String> {
        // First answer: reasoning only. Second: a real document.
        let content = if body == "body1" { "thought hard</think>" } else { "{\"clips\":[]}" };
        Ok(("application/json".to_string(), format!(r#"{{"choices":[{{"message":{{"content":"{content}"}}}}]}}"#)))
    };
    let problem = |reply: &llm_request::Reply| {
        llm_retry::no_answer(reply.answer()).map(str::to_string)
    };
    let messages = || {
        vec![Message { role: "user".to_string(), parts: vec![Part::Text("caption it".into())] }]
    };
    // The flow's own first round, read exactly as a chat ask reads it. Its body is built the way the flow builds
    // one — this door spends no request of its own before the answer reaches it.
    let first_body = build(&messages(), true).expect("the first body");
    assert_eq!(dials.borrow()[0], (1, true), "the first ask thinks");
    let (first_type, first_raw) = dial(&first_body).expect("the first ask answered");
    let first = llm_request::reply_from(&first_type, &first_raw).expect("a chat reply");
    let out = llm_repair_leg::ask_repair_after(
        "captions",
        &|line: &str| log.borrow_mut().push(line.to_string()),
        messages(),
        true,
        false,
        &problem,
        &first,
        &dial,
        &build,
    )
    .expect("the repair went out");
    assert!(out.retried, "the first answer was unusable, so a second went out");
    assert_eq!(dials.borrow().len(), 2, "§3's repair is once-only: {:?}", dials.borrow());
    assert_eq!(dials.borrow()[1], (3, false), "the repair adds the correction AND turns thinking off");
    assert!(out.body.contains("\"clips\""), "the caller reads back the SECOND answer");
    let said = log.borrow();
    assert_eq!(said.len(), 1, "one line about the repair: {said:?}");
    assert!(said[0].starts_with(">>> captions:"), "it names the step: {}", said[0]);
    assert!(said[0].contains("asked again"), "and says what happened: {}", said[0]);

    // A usable answer costs no extra request: the door refuses to spend one on a question already answered, and
    // dials nothing further.
    let dials = std::cell::Cell::new(0usize);
    let good = llm_request::parse_reply(
        &serde_json::from_str::<serde_json::Value>("{\"choices\":[{\"message\":{\"content\":\"{}\"}}]}")
            .expect("an envelope"),
    );
    let err = llm_repair_leg::ask_repair_after(
        "captions",
        &|_| {},
        vec![Message { role: "user".to_string(), parts: vec![Part::Text("x".into())] }],
        true,
        false,
        &|_| None::<String>,
        &good,
        &|_| {
            dials.set(dials.get() + 1);
            Ok(("application/json".to_string(), "{}".to_string()))
        },
        &|_, _| Ok("{}".to_string()),
    )
    .expect_err("nothing to repair");
    assert!(err.contains("nothing to repair"), "it says why: {err}");
    assert_eq!(dials.get(), 0, "a usable answer spends no request: {}", dials.get());
}

// ---- S3 over the wire -------------------------------------------------------------------------
