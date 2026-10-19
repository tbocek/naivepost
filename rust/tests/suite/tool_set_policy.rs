//! tool:set_policy — the tool RESULT is ok or error (§02-services §3.11, directive B).
//!
//! `tests/suite/policy_derive.rs` pins what `policy::apply` decides; `tests/f0_7_s3_a_live_refusal_is_handed_back…`
//! pins that a refusal also reaches the model as the next round's user text. What neither covered is the
//! sentence §3.11's result column actually states: the app validates INSIDE the tool and returns the
//! problem AS THE TOOL RESULT. These tests call the leg's own answer function — the one the tool loop
//! calls for every `set_policy` it receives — so the shape of what the model is handed back is what is
//! under test, not a copy of the rule.

use naivepost::exchanges::ToolCall;
use naivepost::policy::{self, Applied, Proposal};
use naivepost::policy_call::tool_answer;
use naivepost::project::{Field, MarkingPass, Origin, Policy};

/// The context the leg builds its request from; `set_policy`'s answer does not read it, but the
/// signature is the loop's and this keeps the call honest.
const CONTEXT: &str = "A lecture read from a script; the speaker restarts after a stumble.";

/// One `set_policy` call exactly as the model spells its arguments over the wire.
fn call(field: &str, value: &str, because: &str) -> ToolCall {
    ToolCall {
        name: "set_policy".to_string(),
        args: serde_json::json!({ "field": field, "value": value, "because": because }).to_string(),
        result: String::new(),
    }
}

/// Answer one call against a policy, and parse what came back. Every assertion below reads the JSON
/// the model reads, so a malformed or double-encoded result cannot pass.
fn answer(policy: &Policy, field: &str, value: &str, because: &str) -> serde_json::Value {
    let req = policy::request(CONTEXT);
    let text = tool_answer(&call(field, value, because), &req, policy);
    serde_json::from_str(&text).unwrap_or_else(|why| panic!("the tool answer must be JSON ({why}): {text}"))
}

/// The same verdict through the validator alone, so a test can assert the tool says EXACTLY what
/// `policy::apply` said rather than a paraphrase of it.
fn applied_sentence(policy: &mut Policy, field: &str, value: &str) -> String {
    let proposal = Proposal {
        field: field.to_string(),
        value: value.to_string(),
        because: "asked for it".to_string(),
    };
    match policy::apply(policy, &proposal) {
        Applied::Refused(say) => say,
        other => panic!("expected a refusal from the validator, got {other:?}"),
    }
}

#[test]
fn tool_set_policy_s1_an_accepted_call_answers_ok_and_changes_nothing_it_was_given() {
    let policy = Policy::default();
    let got = answer(&policy, "markingPass", "joins", "the speaker read a script");
    assert_eq!(got["recorded"], serde_json::json!(true), "{got}");
    assert_eq!(got["field"], "markingPass", "{got}");
    assert_eq!(got["value"], "joins", "{got}");
    assert!(got.get("error").is_none(), "an accepted call carries no error: {got}");
    // The tool ANSWERS; `policy::derive`'s loop applies. Handing in the session policy must leave it
    // untouched, or the derivation would land values twice from two places.
    assert_eq!(
        policy.marking_pass.value,
        MarkingPass::Retakes,
        "the policy the tool was handed still holds its old value"
    );
    assert_eq!(policy.marking_pass.origin, Origin::Default);
}

#[test]
fn tool_set_policy_s2_an_unknown_field_comes_back_as_the_error_apply_writes() {
    let policy = Policy::default();
    let got = answer(&policy, "stylePreset", "lecture", "sounds academic");
    let expected = applied_sentence(&mut Policy::default(), "stylePreset", "lecture");
    let say = got["error"].as_str().unwrap_or_default();
    assert!(
        !say.is_empty(),
        "an unknown field must come back as an error object, got {got}"
    );
    assert_eq!(say, expected, "the tool result IS `policy::apply`'s sentence, not a copy of it");
    assert!(
        say.starts_with("stylePreset is not a policy field"),
        "and it names the field the model invented: {say}"
    );
    for field in ["markingPass", "cutMode", "captionsPass", "speedPass", "decorationsPass"] {
        assert!(say.contains(field), "the refusal lists the real fields so it can self-correct: {say}");
    }
    assert!(got.get("recorded").is_none(), "a refused call claims nothing recorded: {got}");
}

#[test]
fn tool_set_policy_s3_an_out_of_range_value_comes_back_as_the_error_apply_writes() {
    let policy = Policy::default();
    let got = answer(&policy, "cutMode", "vibes", "go with my gut"); // P.policy.cutMode
    let expected = applied_sentence(&mut Policy::default(), "cutMode", "vibes");
    let say = got["error"].as_str().unwrap_or_default();
    assert_eq!(say, expected, "one validator, one sentence: {got}");
    assert!(say.contains("out of range"), "{say}");
    assert!(say.contains("cutMode") && say.contains("words") && say.contains("model"), "{say}");
}

#[test]
fn tool_set_policy_s4_a_number_row_refuses_a_non_number_and_accepts_seconds() {
    // P.policy.targetLengthSeconds — the catalogue's number row: no list to check against, so the
    // refusal it can earn is "not a length in seconds".
    let policy = Policy::default();
    let bad = answer(&policy, "targetLengthSeconds", "soon", "roughly how long?");
    let say = bad["error"].as_str().unwrap_or_default();
    assert_eq!(
        say,
        applied_sentence(&mut Policy::default(), "targetLengthSeconds", "soon"),
        "{bad}"
    );
    assert!(say.contains("targetLengthSeconds") && say.contains("soon"), "{say}");
    let good = answer(&policy, "targetLengthSeconds", "900", "fifteen minutes");
    assert_eq!(good["recorded"], serde_json::json!(true), "{good}");
    assert!(good.get("error").is_none(), "{good}");
}

#[test]
fn tool_set_policy_s5_a_hand_set_field_is_not_an_error_and_claims_nothing_recorded() {
    // S4's precedence inside the tool result: the person's value stands, so the model is told it was
    // KEPT rather than that it failed (nothing was wrong with the call) or that it landed (it did not).
    let policy = Policy {
        speed_pass: Field {
            value: false,
            origin: Origin::User,
            because: None,
        },
        ..Policy::default()
    }; // P.policy.speedPass, set by hand
    let got = answer(&policy, "speedPass", "on", "the pacing could use a lift");
    assert!(got.get("error").is_none(), "a kept value is not a fault: {got}");
    assert_eq!(got["kept"], "user", "{got}");
    assert!(got.get("recorded").is_none(), "nothing was recorded, so nothing may claim it: {got}");
    assert_eq!(got["field"], "speedPass", "{got}");
    assert!(!policy.speed_pass.value, "the person's off stays off");
}

#[test]
fn tool_set_policy_s6_only_a_refusal_carries_the_error_key() {
    // §3.1's one shape: a problem is `tools::error`'s single `error` key, a success has none. A result
    // that put both in one object would read as a broken reply rather than as an answer to act on.
    let policy = Policy::default();
    let refused = answer(&policy, "unknownSwitch", "on", "made up");
    let keys: Vec<&str> = refused.as_object().map(|map| map.keys().map(String::as_str).collect()).unwrap_or_default();
    assert_eq!(keys, vec!["error"], "a refusal is exactly one key: {refused}");
    for accepted in [answer(&policy, "markingPass", "none", "hand-marked"), answer(&policy, "cutMode", "words", "text-driven")] {
        assert!(accepted.get("error").is_none(), "{accepted}");
    }
}
