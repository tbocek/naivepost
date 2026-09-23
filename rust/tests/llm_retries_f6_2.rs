//! F6.2 — `spec/09-llm-and-tools.md` §3, retries, checked against `naivepost::llm_retry`.
//!
//! §3's bullets are the steps here (s1 the transport death and its ladder, s2 the one short retry, s3 the 5xx
//! that names a device, s4 content repair in JSON mode, s5 the two numbers kept apart). Nothing sleeps and no
//! clock runs: `run` is driven by closures, so "the fifth death asks for nothing" and "⏹ ends a four-minute wait"
//! are assertions rather than things a reviewer hopes about.
//!
//! # Ids used
//!
//! `llm.backoffSeconds` (5, 20, 60, 120, 240) and `llm.retrySeconds` (2) — §10 lists both only in its by-area
//! line ("backoff 5 s–4 min"), so they carry bare prefixes rather than a `P.` id. Neither is
//! `P.eng.llmAttempts` (3 validation attempts per job) nor `P.eng.llmToolRounds`; that distinction is s5.

use std::cell::Cell;
use std::collections::VecDeque;

use naivepost::degraded;
use naivepost::exchanges::Part;
use naivepost::llm_retry as retry;
use naivepost::params;
use naivepost::roles;

const ITEM: &str = "F6.2";

/// A failure that reads as a dead socket, and one that reads as an answer this app cannot use.
#[derive(Debug, Clone, Copy)]
struct Failure(&'static str);

impl std::fmt::Display for Failure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(self.0)
    }
}

/// The real classifier, as the caller would hand it to `run`.
fn died_by_words() -> impl Fn(&Failure) -> bool {
    |problem: &Failure| retry::transport_death(problem.0)
}

// ---- s1: which failures are "the server went away" -------------------------------------

#[test]
fn f6_2_s1_transport_death_words_are_matched_whole() {
    assert_eq!(ITEM, "F6.2");
    // §3's word list, each in the shape a real client writes it.
    for death in [
        "unexpected EOF reading stream",
        "read tcp 10.0.0.2:8731: read: connection reset by peer",
        "dial tcp 127.0.0.1:8731: connect: connection refused",
        "write: broken pipe",
        "the server closed the stream mid-reply",
        "lookup llama on 127.0.0.53: no such host",
        "http2: transport is closing",
    ] {
        assert!(retry::transport_death(death), "{death} is a death");
    }
    // §3's condition words. `reset` and `refused` count beside their noun, which is how every client in use
    // writes them; `unreachable` is specific to a socket already and stands alone.
    assert!(retry::transport_death("connection reset"));
    assert!(retry::transport_death("connection refused"));
    assert!(retry::transport_death("host unreachable"));

    // Near-misses that are prose. This is the whole reason for whole-word matching: a five-minute wait on a
    // healthy server, on the strength of a sentence about the model's behaviour, is worse than no retry at all.
    assert!(!retry::transport_death("the model refused to answer the question"), "bare `refused` is prose");
    assert!(!retry::transport_death("a resettable timer fired"), "a word that merely starts like one");
    assert!(!retry::transport_death("the leof graph changed"), "`eof` inside a word is not `eof`");
    assert!(!retry::transport_death("the answer names 3 segments and 2 effects"), "no death word at all");

    // A status code is never "the server went away" (§3, verbatim). The server answered; asking again in four
    // minutes gets the same answer.
    assert_eq!(retry::status_of("HTTP 400 bad request"), Some(400));
    assert!(!retry::transport_death("the server answered 500: internal server error"));
    assert!(!retry::went_away("", Some(500), "internal server error"), "a status alone is not a death");
    // And a port in an address is not a status — otherwise every connection failure would read as one the
    // server sent, which is §3's rule inverted.
    assert_eq!(retry::status_of("dial tcp 127.0.0.1:8731: connect: connection refused"), None);
    assert!(retry::went_away("dial tcp 127.0.0.1:8731: connect: connection refused", None, ""));
}

#[test]
fn f6_2_s1_the_ladder_and_its_log() {
    assert_eq!(ITEM, "F6.2");
    // Five waits, in order, outlasting a container restart and a weight load (§3; §10's "backoff 5 s–4 min").
    assert_eq!(retry::BACKOFF_SECONDS, [5, 20, 60, 120, 240]);
    for (failure, want) in retry::BACKOFF_SECONDS.into_iter().enumerate() {
        assert_eq!(retry::wait_seconds(true, failure as u32), Some(want), "failure {failure}");
    }
    // The patience runs out: a sixth death is a server that is not coming back, and the step fails.
    assert_eq!(retry::wait_seconds(true, 5), None);
    assert_eq!(retry::wait_seconds(true, 99), None);

    // Spelled as §3 spells them, not as Go's duration formatter would ("1m0s").
    let spelled: Vec<String> = retry::BACKOFF_SECONDS.iter().map(|s| retry::wait_spelling(*s)).collect();
    assert_eq!(spelled, ["5 s", "20 s", "1 min", "2 min", "4 min"]);

    // §3's line byte for byte — ASCII `--`, one-based count, the total from the ladder's length.
    assert_eq!(
        retry::gone_log("narrate", "connection reset by peer", 5, 0),
        "!!! narrate: the server went away mid-call (connection reset by peer) -- waiting 5 s and asking again (1 of 5)"
    );
    let last = retry::gone_log("publish", "EOF", 240, 4);
    assert_eq!(last, "!!! publish: the server went away mid-call (EOF) -- waiting 4 min and asking again (5 of 5)");
    assert!(last.contains(&format!("(5 of {})", retry::BACKOFF_SECONDS.len())), "{last}");

    // The short retry has no line in §3; a pause nobody can see reads as a hang.
    assert_eq!(retry::waited_log("suggest", "not valid JSON: x"), ">>> suggest: asked again after 2 s (not valid JSON: x)");
    assert_eq!(retry::RETRY_SECONDS, 2);
}

#[test]
fn f6_2_s1_a_wait_ends_when_the_run_is_stopped() {
    assert_eq!(ITEM, "F6.2");
    // ⏹ pressed during a wait: the run ends there rather than sleeping through eight minutes of dead button.
    let waits = std::cell::RefCell::new(Vec::new());
    let calls = Cell::new(0);
    let run = retry::run::<(), Failure>(
        "narrate",
        &mut || {
            calls.set(calls.get() + 1);
            Err(Failure("connection reset by peer"))
        },
        // The first wait is granted; during it the user stops the run.
        &|seconds| {
            waits.borrow_mut().push(seconds);
            true
        },
        &|| true,
        &died_by_words(),
    );
    assert!(run.stopped, "the caller must not report this as a server failure: {}", run.joined());
    assert_eq!(run.answer, None);
    assert_eq!(calls.get(), 1, "stopped during the wait, so nothing was asked again");
    // The wait was *asked for*, with the ladder's first value — the module never sleeps, so this is the proof.
    assert_eq!(*waits.borrow(), vec![retry::BACKOFF_SECONDS[0]]);
    assert_eq!(run.logs.len(), 1, "{}", run.joined());
    assert!(run.logs[0].starts_with("!!! narrate: the server went away mid-call"), "{}", run.joined());

    // Or the sleep itself reports the stop (the real one returns false when the context is done).
    let stopped = retry::run::<(), Failure>(
        "narrate",
        &mut || Err(Failure("broken pipe")),
        &|_| false,
        &|| false,
        &died_by_words(),
    );
    assert!(stopped.stopped);
    assert_eq!(stopped.failures, 1);
}

// ---- s2: anything else ---------------------------------------------------------------

#[test]
fn f6_2_s2_anything_else_is_asked_again_once_after_2_s() {
    assert_eq!(ITEM, "F6.2");
    // The first failure only: a 4xx or an unusable answer is the server stating its position, and one short
    // retry covers the local trip-up without pretending otherwise.
    assert_eq!(retry::wait_seconds(false, 0), Some(retry::RETRY_SECONDS));
    assert_eq!(retry::wait_seconds(false, 1), None);
    assert_eq!(retry::wait_seconds(false, 2), None);

    let calls = Cell::new(0);
    let run = retry::run::<(), Failure>(
        "suggest",
        &mut || {
            calls.set(calls.get() + 1);
            Err(Failure("HTTP 400 bad request: messages[1] has no role"))
        },
        &|_| true,
        &|| false,
        &died_by_words(),
    );
    assert_eq!(run.answer, None, "the step fails");
    assert!(!run.stopped, "a failure is not a stop: {}", run.joined());
    assert_eq!(calls.get(), 2, "one retry, first failure only");
    assert_eq!(run.failures, 2);
    // Only the short line — no death was claimed for a server that answered.
    assert_eq!(run.logs.len(), 1, "{}", run.joined());
    assert!(!run.joined().contains("went away"), "{}", run.joined());
}

#[test]
fn f6_2_s2_a_dead_server_is_resent_the_same_request() {
    assert_eq!(ITEM, "F6.2");
    // Two deaths then an answer. The retry is the *same request*: `call` closes over its own arguments and is
    // invoked unchanged, which is why a transport retry of a tool loop re-runs from the original messages rather
    // than continuing a half-finished conversation whose tool results never arrived.
    let arguments = "narrate the third clip";
    let seen = std::cell::RefCell::new(Vec::new());
    let mut answers = VecDeque::from([Err(Failure("connection reset by peer")), Err(Failure("EOF")), Ok(())]);
    let waits = std::cell::RefCell::new(Vec::new());
    let run = retry::run::<(), Failure>(
        "narrate",
        &mut || {
            seen.borrow_mut().push(arguments);
            answers.pop_front().expect("the script is exhausted")
        },
        &|seconds| {
            waits.borrow_mut().push(seconds);
            true
        },
        &|| false,
        &died_by_words(),
    );
    assert_eq!(run.answer, Some(()));
    assert_eq!(run.failures, 2);
    assert!(!run.stopped);
    assert_eq!(seen.borrow().len(), 3, "asked twice more after two deaths");
    assert!(seen.borrow().iter().all(|arg| *arg == arguments), "the same request every time");
    // The ladder is walked in order, not restarted.
    assert_eq!(*waits.borrow(), vec![5, 20]);
    let deaths: Vec<&String> = run.logs.iter().filter(|line| line.starts_with("!!!")).collect();
    assert_eq!(deaths.len(), 2, "{}", run.joined());
    assert!(deaths[0].contains("(1 of 5)") && deaths[1].contains("(2 of 5)"), "{}", run.joined());

    // A call that answers first asks once and logs nothing: no waiting, no apology.
    let quiet = retry::run::<(), Failure>(
        "narrate",
        &mut || Ok(()),
        &|_| true,
        &|| false,
        &died_by_words(),
    );
    assert_eq!(quiet.answer, Some(()));
    assert_eq!(quiet.failures, 0);
    assert!(quiet.logs.is_empty(), "{}", quiet.joined());
}

// ---- s3: the 5xx that names a device -------------------------------------------------

#[test]
fn f6_2_s3_a_5xx_naming_a_device_backs_off() {
    assert_eq!(ITEM, "F6.2");
    // §3's new rule: an out-of-memory is not an answer about the request, it is the box having no room *yet* —
    // which is exactly what a wait fixes. So it joins the ladder instead of being resent at once.
    assert!(retry::device_failure(Some(500), "CUDA error: out of memory"));
    let died = retry::went_away("the server answered 500", Some(500), "CUDA error: out of memory");
    assert!(died, "a device failure backs off like a transport death");
    assert_eq!(retry::wait_seconds(died, 0), Some(5), "the ladder, not the 2 s retry");

    // A 4xx never qualifies: that is the request's fault and no wait changes what the server objects to.
    assert!(!retry::device_failure(Some(400), "out of memory"));
    assert!(!retry::went_away("the server answered 400", Some(400), "out of memory"));
    // A 5xx that says nothing of the kind is still just an answer — one short retry, first failure only.
    assert!(!retry::device_failure(Some(500), "internal server error"));
    let plain = retry::went_away("the server answered 500", Some(500), "internal server error");
    assert!(!plain);
    assert_eq!(retry::wait_seconds(plain, 0), Some(retry::RETRY_SECONDS));
    // A 2xx cannot be a device failure either: nothing failed.
    assert!(!retry::device_failure(Some(200), "ok"));
    assert!(!retry::device_failure(None, "out of memory"), "no status means no server answer to read");

    // Every phrase in the const is a phrase some server actually prints with a 5xx.
    for phrase in retry::DEVICE_FAILURE_PHRASES {
        let body = format!("the model server failed: {phrase} while loading weights");
        assert!(retry::device_failure(Some(503), &body), "{phrase} is not recognised");
    }
    // Case: servers shout.
    assert!(retry::device_failure(Some(500), "CUDA ERROR: OUT OF MEMORY"));
}

// ---- s4: content repair in degraded (JSON) mode --------------------------------------

#[test]
fn f6_2_s4_content_repair_in_json_mode() {
    assert_eq!(ITEM, "F6.2");
    // The whole reply was reasoning: a parser would call this "unexpected end of JSON input", the same words it
    // uses for an answer truncated at the ceiling — and those two want opposite fixes.
    let reasoning = "Let me think about the cut... I will list every segment</think>";
    let (kind, problem) = retry::repair(reasoning, "").expect("a repair");
    assert_eq!(kind, retry::Repair::NoAnswer);
    assert!(problem.contains("whole reply was reasoning"), "{problem}");

    // No text at all is the same failure. Telling it apart from a truncated answer matters more than the model
    // can see: one wants a shorter answer, the other wants an answer.
    let (kind, _) = retry::repair("   \n ", "unexpected end of JSON input").expect("a repair");
    assert_eq!(kind, retry::Repair::NoAnswer, "empty is no answer, not a truncated one");

    // A non-empty reply whose parse stopped for want of a tail: the ceiling was hit.
    let (kind, problem) = retry::repair("{\"segments\": [{\"start\": 1", "unexpected end of JSON input").expect("a repair");
    assert_eq!(kind, retry::Repair::CutOff);
    assert_eq!(problem, degraded::CUTOFF_RETRY, "one sentence, one home");
    assert!(problem.contains("far fewer items"), "{problem}");

    // A real syntax problem wants care, not brevity — telling it to shorten would be the wrong advice.
    let (kind, problem) = retry::repair("{\"a\":}", "expected value at line 1 column 5").expect("a repair");
    assert_eq!(kind, retry::Repair::Other);
    assert!(problem.starts_with("not valid JSON:"), "{problem}");

    // And a reply that parsed is not a problem.
    assert_eq!(retry::repair("{\"segments\": []}", ""), None);
    // `cut_off_problem` alone: an empty reply is no_answer's case, and another error is not a cutoff.
    assert_eq!(retry::cut_off_problem("", "unexpected end of JSON input"), None);
    assert_eq!(retry::cut_off_problem("{\"a\":1", "trailing characters"), None);

    // thinkAgain: an empty answer means the thinking budget was spent with no words, so the retry turns it off.
    assert!(!retry::think_again(true, ""), "thinking off for the retry");
    assert!(retry::think_again(true, "{\"a\":1}"), "bad JSON keeps thinking: that model is writing and wrong");
    assert!(!retry::think_again(false, "{\"a\":1}"), "it was never on");
    // Reasoning with no answer after it is the same spent budget.
    assert!(!retry::think_again(true, "thinking hard</think>  "));
}

#[test]
fn f6_2_s4_a_rejected_answer_goes_back_into_the_history() {
    assert_eq!(ITEM, "F6.2");
    let text_of = |message: &naivepost::exchanges::Message| match message.parts.first() {
        Some(Part::Text(text)) => text.clone(),
        other => panic!("expected text, got {other:?}"),
    };

    let history = retry::retry_turn(vec![], "{\"segments\": []", "not valid JSON: x", false);
    assert_eq!(history.len(), 2);
    // The model reads its own answer first, then the correction — the other order asks it to correct a reply it
    // has not just seen.
    assert_eq!(history[0].role, "assistant");
    assert_eq!(text_of(&history[0]), "{\"segments\": []");
    assert_eq!(history[1].role, "user");
    let correction = text_of(&history[1]);
    assert!(correction.starts_with("Your answer failed validation: "), "{correction}");
    assert!(correction.contains("not valid JSON: x"), "{correction}");
    assert!(correction.ends_with("Return corrected strict JSON only."), "{correction}");

    // An empty reply is not appended: there is no message to send, and an assistant turn holding nothing makes
    // the history read as though the model had answered with a blank.
    let empty = retry::retry_turn(vec![], "   ", "no answer at all", false);
    assert_eq!(empty.len(), 1, "{}", text_of(&empty[0]));
    assert_eq!(empty[0].role, "user");

    // What was already there survives in front: the correction goes at the end. `Message` has no PartialEq, so
    // the comparison is on roles and text — which is all of it that could move.
    let earlier = retry::retry_turn(history.clone(), "x", "p", false);
    assert_eq!(earlier.len(), history.len() + 2);
    let shape = |messages: &[naivepost::exchanges::Message]| -> Vec<(String, String)> {
        messages.iter().map(|m| (m.role.clone(), text_of(m))).collect()
    };
    assert_eq!(shape(&earlier[..history.len()]), shape(&history));
}

#[test]
fn f6_2_s4_with_tools_the_repairs_are_tool_results() {
    assert_eq!(ITEM, "F6.2");
    // §3: "With tools these become tool results." On that path the model is mid-round and expects its calls
    // answered; a user turn in the middle of one reads as a new instruction and ends the round.
    let history = retry::retry_turn(vec![], "{\"a\":}", "not valid JSON: x", true);
    assert_eq!(history.last().expect("a correction").role, "tool");
    let without = retry::retry_turn(vec![], "{\"a\":}", "not valid JSON: x", false);
    assert_eq!(without.last().expect("a correction").role, "user");

    // The rejected answer is still an assistant turn either way — the round's shape does not change.
    assert_eq!(history.first().expect("the answer").role, "assistant");
}

// ---- s5: the two patiences are not the same number -----------------------------------

#[test]
fn f6_2_s5_the_two_patiences_are_not_confused() {
    assert_eq!(ITEM, "F6.2");
    // Catalogued, with §10's by-area spelling quoted: "backoff 5 s–4 min". No `P.` row exists for either, so
    // both carry a bare prefix and land in Family::Other.
    let ladder = params::find("llm.backoffSeconds").expect("catalogued");
    assert_eq!(ladder.from, "llm_retry::BACKOFF_SECONDS");
    assert_eq!(ladder.spelled.as_str(), "5, 20, 60, 120, 240");
    let short = params::find("llm.retrySeconds").expect("catalogued");
    assert_eq!(short.from, "llm_retry::RETRY_SECONDS");
    assert_eq!(short.spelled.as_str(), "2");
    for id in ["llm.backoffSeconds", "llm.retrySeconds"] {
        assert_eq!(params::family(id), params::Family::Other, "{id} has no P. row in §10");
    }

    // Not the two numbers a reader might reach for. `P.eng.llmAttempts` counts *validation* attempts at one job
    // (the JSON was rejected and asked for again); `P.eng.llmToolRounds` counts tool rounds inside one call. A
    // job can spend three of the first inside one successful call, and a call can die five times without either.
    assert_eq!(params::family("P.eng.llmAttempts"), params::Family::Eng);
    assert_eq!(roles::LLM_ATTEMPTS, 3, "validation attempts: a different item");
    assert_eq!(roles::TOOL_ROUNDS, 8, "tool rounds: a different item again");
    assert_ne!(retry::BACKOFF_SECONDS.len() as u32, roles::LLM_ATTEMPTS);
    assert_ne!(retry::RETRY_SECONDS as u32, roles::TOOL_ROUNDS);

    // The ladder and the retry are read from the two consts, never retyped: what `wait_seconds` answers is
    // exactly the array's contents, so changing a wait changes the log line with it.
    assert_eq!(retry::BACKOFF_SECONDS.len(), 5, "§3 says five waits (i of 5)");
    let total: u64 = retry::BACKOFF_SECONDS.iter().sum();
    assert_eq!(total, 445, "roughly eight minutes of patience (§3's comment)");
    for failure in 0..retry::BACKOFF_SECONDS.len() as u32 {
        assert_eq!(retry::wait_seconds(true, failure), Some(retry::BACKOFF_SECONDS[failure as usize]));
    }
    // And the source holds no second copy of either number where a copy would matter. The two places a bare 60
    // does appear — `wait_spelling`'s minute boundary and `gone_log`'s array length — are unit arithmetic and the
    // ladder's size, not waits: neither could survive changing a wait, which is exactly what makes them safe.
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/llm_retry.rs")).expect("the module");
    let body = |name: &str| -> String {
        source
            .split(&format!("pub fn {name}"))
            .nth(1)
            .and_then(|rest| rest.split("\n}\n").next())
            .unwrap_or_else(|| panic!("{name}'s body"))
            .to_string()
    };
    // The decision reads only the two consts: no wait is a literal there.
    let decides = body("wait_seconds");
    assert!(decides.contains("BACKOFF_SECONDS") && decides.contains("RETRY_SECONDS"), "{decides}");
    for literal in ["5,", "20,", "240", "(2)", "== 2"] {
        assert!(!decides.contains(literal), "{literal} in wait_seconds is a second copy of a wait: {decides}");
    }
    // The log line takes its total from the ladder rather than writing 5.
    let logged = body("gone_log");
    assert!(logged.contains("BACKOFF_SECONDS.len()"), "{logged}");
    assert!(!logged.contains("(i of 5)") && !logged.contains("of 5)"), "a literal 5 in the line: {logged}");
}
