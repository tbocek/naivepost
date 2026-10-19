//! §09-llm-and-tools#9-model-list-and-tests — the model list and the two Settings probes.
//!
//! `GET /v1/models` (15 s) fills the Settings dropdown; F0.13's two LLM probes are one completion
//! and one red-square vision probe. Both halves are pinned here as data rather than as traffic: the
//! numbers a person reads off the spec (15 / 60 / 120 s, 16 tokens, 48 px) are checked against the
//! code that carries them.
//!
//! Ids named in these tests: the endpoint `GET /v1/models` with `model_list::LIST_MODELS_SECONDS`
//! (15 s), `checks::LLM_TEST_SECONDS` (60 s), `checks::LLM_TEST_MAX_TOKENS` (16),
//! `checks::VISION_TEST_SECONDS` (120 s), the red square built by `checks::red_square_png()` and
//! sent as `checks::vision_probe_url()`, `checks::LIST_MODELS_LLM_USES`, and
//! `checks::LOG_PREFIX` ("settings:") which is how every verdict here reaches the main log.

use naivepost::checks;
use naivepost::model_list as models;
use serde_json::json;

const ITEM: &str = "§09-llm-and-tools#9-model-list-and-tests";

#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s1_the_list_is_a_get_with_a_fifteen_second_budget() {
    assert_eq!(ITEM, "§09-llm-and-tools#9-model-list-and-tests");
    // GET /v1/models, 15 s (§9).
    let (method, path, seconds) = models::request();
    assert_eq!(method, "GET");
    assert_eq!(path, "/v1/models");
    assert_eq!(path, models::ENDPOINT);
    assert_eq!(seconds, 15);
    assert_eq!(models::LIST_MODELS_SECONDS, 15);
    // Not the completion budget: a minute of spinner on Fetch models reads as a hung dialog, and a
    // model list loads no weights, so fifteen seconds already means the address is wrong.
    assert_ne!(seconds, checks::LLM_TEST_SECONDS, "must not inherit the 60 s completion budget");
    // And it is its own rule, not /health's, even though both are 15 today.
    assert_eq!(seconds, checks::HEALTH_SECONDS, "both 15 by coincidence, different sections");
}

#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s2_ids_are_parsed_in_the_servers_own_order() {
    let body = json!({"data": [{"id": "z-model"}, {"id": "a-model"}]});
    let ids = models::parse_ids(&body);
    assert_eq!(ids, vec!["z-model".to_string(), "a-model".to_string()], "order preserved, not sorted");

    // Junk entries are skipped, never fatal: a server that grows a field must not take the dropdown
    // down with it.
    let messy = json!({"data": [
        {"id": "good-one"},
        {"name": "no id at all"},
        {"id": ""},
        {"id": 42},
        {"id": "   "},
        {"id": "good-two"},
    ]});
    assert_eq!(models::parse_ids(&messy), vec!["good-one".to_string(), "good-two".to_string()]);

    // Nothing shaped like a list yields nothing, without panicking.
    assert!(models::parse_ids(&json!({"data": []})).is_empty());
    assert!(models::parse_ids(&json!({})).is_empty());
    assert!(models::parse_ids(&json!("not an object")).is_empty());
    assert!(models::parse_ids(&json!({"data": "instead of an array"})).is_empty());
}

#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s3_an_empty_list_names_the_endpoint_it_came_from() {
    // Empty is an error about the port, because that is the thing to go look at.
    let empty = models::list_verdict(&[]);
    assert!(empty.is_err());
    assert!(
        empty.unwrap_err().contains("/v1/models"),
        "the verdict must name what was asked"
    );

    // Non-empty reports the count, which is what the row says next to the ✓.
    let three: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    let verdict = models::list_verdict(&three).expect("three models is a success");
    assert!(verdict.contains('3'), "{verdict}");

    // The two things the LLM's list is ever used for (§9 / checks).
    assert_eq!(
        checks::LIST_MODELS_LLM_USES,
        ["Fetch models", "sd.cpp fallback probe"]
    );
}

#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s4_use_copies_the_picked_id_into_model() {
    let listed: Vec<String> = ["qwen-vl", "qwen-text", "llama-3"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    // A member comes back exactly as listed -- that string is what lands in the Model box.
    let picked = models::use_choice(&listed, "qwen-vl").expect("a listed id is usable");
    assert_eq!(picked, "qwen-vl");
    assert!(listed.contains(&picked), "round-trips into the list it came from");

    // A non-member is refused, naming the id so the operator can see what went wrong.
    let refused = models::use_choice(&listed, "qwen-vision").unwrap_err();
    assert!(refused.contains("qwen-vision"), "{refused}");

    // No prefix matching: ids share prefixes across vision/text variants, and half an id produces a
    // completion that fails for a reason nobody can see.
    assert!(models::use_choice(&listed, "qwen").is_err(), "a prefix is not an id");
    assert!(models::use_choice(&listed, "qwen-v").is_err());
    // Nor a superstring.
    assert!(models::use_choice(&listed, "qwen-vl-plus").is_err());
    // Nothing chosen is not a choice.
    assert!(models::use_choice(&listed, "").is_err());
    assert!(models::use_choice(&listed, "   ").is_err());
    // An empty list offers nothing to pick from.
    assert!(models::use_choice(&[], "anything").is_err());
}

#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s5_the_two_probes_carry_f013s_numbers() {
    let probes = models::probes();
    assert_eq!(probes.len(), 2, "F0.13 sends exactly two LLM probes");
    let text = &probes[0];
    let vision = &probes[1];

    // The completion: 60 s, 16 tokens, no picture.
    assert_eq!(text.name, "LLM");
    assert_eq!(text.seconds, 60);
    assert_eq!(text.seconds, checks::LLM_TEST_SECONDS);
    assert_eq!(text.max_tokens, Some(16));
    assert_eq!(text.max_tokens, Some(checks::LLM_TEST_MAX_TOKENS));
    assert!(!text.vision);

    // The vision probe: 120 s, no token cap, a picture.
    assert_eq!(vision.name, "LLM vision");
    assert_eq!(vision.seconds, 120);
    assert_eq!(vision.seconds, checks::VISION_TEST_SECONDS);
    assert_eq!(vision.max_tokens, None, "an image prompt is not capped at sixteen tokens");
    assert!(vision.vision);

    // Vision gets the longer budget of the two, and they are not equal.
    assert!(vision.seconds > text.seconds, "vision is the slower probe");
    assert_ne!(text.seconds, vision.seconds);
    // All three spec numbers together: 15 s list, 60 s completion, 120 s vision.
    assert_eq!(
        (models::LIST_MODELS_SECONDS, text.seconds, vision.seconds),
        (15, 60, 120)
    );

    // Pass conditions: any non-blank answer passes the completion; blank does not.
    assert!(models::probe_passes(text, "ok").is_ok());
    assert!(models::probe_passes(text, "").is_err(), "nothing back is not a pass");
    assert!(models::probe_passes(text, "   ").is_err());
    // Vision passes only on a reply that names red -- same rule as checks::vision_verdict.
    assert!(models::probe_passes(vision, "Red").is_ok(), "case-insensitive");
    assert!(models::probe_passes(vision, "It is red.").is_ok());
    assert!(models::probe_passes(vision, "blue").is_err(), "it is not seeing the image");
    assert!(models::probe_passes(vision, "").is_err());
    // And it really is the same function behind both, not a second copy of the rule.
    assert_eq!(
        models::probe_passes(vision, "green").unwrap_err(),
        checks::vision_verdict("green").unwrap_err()
    );
}

#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s6_the_probe_bodies_are_what_f013_describes() {
    // The completion body: the single-word prompt, max_tokens 16, thinking forced off. These are the
    // keys checks::llm_test_body actually sets -- asserted rather than assumed.
    let body = checks::llm_test_body("some-model");
    assert_eq!(body["model"], "some-model");
    let messages = body["messages"].as_array().expect("one message");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "Reply with the single word: ok");
    assert_eq!(body["max_tokens"], 16);
    assert_eq!(body["max_tokens"], checks::LLM_TEST_MAX_TOKENS);
    // Thinking off, spelled the three ways servers read it.
    assert_eq!(body["enable_thinking"], false);
    let kwargs = &body["chat_template_kwargs"];
    assert_eq!(kwargs["enable_thinking"], false);
    // It is not streamed: there is nothing to watch arrive.
    assert!(
        body.get("stream").map(|v| v.as_bool() == Some(false)).unwrap_or(true),
        "the test request must not stream"
    );

    // The red square: a real PNG, 48x48, truecolour. IHDR starts at byte 8 (length) then the type
    // at 12, width at 16..20 and height at 20..24, big-endian.
    let png = checks::red_square_png();
    assert_eq!(&png[0..4], b"\x89PNG", "PNG magic");
    assert_eq!(&png[12..16], b"IHDR");
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    assert_eq!(width, 48, "F0.13 asks for a 48x48 square");
    assert_eq!(height, 48);
    assert_eq!(png[24], 8, "bit depth 8");
    assert_eq!(png[25], 2, "colour type truecolour, no alpha");

    // Sent as a data URL, which is how a chat-completions request carries an inline picture.
    let url = checks::vision_probe_url();
    assert!(url.starts_with("data:image/png;base64,"), "{url}");
    assert!(url.len() > 100, "the base64 body is present");
}

#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s7_every_verdict_reaches_the_log_under_settings() {
    // F0.13 mirrors each row's verdict into the main log as "settings: ...". Both halves of this
    // item's answers travel that way, pass or fail.
    let passed = models::list_verdict(&["m".to_string()]);
    let failed = models::list_verdict(&[]);
    let line_pass = checks::log_line("Fetch models", &passed);
    let line_fail = checks::log_line("Fetch models", &failed);
    assert!(line_pass.starts_with(checks::LOG_PREFIX), "{line_pass}");
    assert!(line_fail.starts_with(checks::LOG_PREFIX), "{line_fail}");
    assert_eq!(checks::LOG_PREFIX, "settings:");
    // A failure says which row failed, not just that something did -- otherwise ten rows and one
    // unreadable line.
    assert!(line_fail.contains("Fetch models"), "{line_fail}");
}
