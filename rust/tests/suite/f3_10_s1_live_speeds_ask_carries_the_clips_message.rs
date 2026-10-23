//! F3.10 S1 over a socket: the speed ask that goes out is the spec's own brief, and it carries the two
//! tools §3.7 names for this job. One scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide) — see
//! `captions_wire_harness.rs`.
//!
//! `rust/tests/suite/cut_proposed_speeds.rs` proves the rules with calls handed in as data. This proves the
//! wire: the clip brief reaches the address the settings hold, the SPEEDS reply comes back through a chat
//! envelope, and the rates it named are the calls that arrive — in the order the model asked for them.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use crate::common;
use common::three_clips;
use harness::*;
use naivepost::cut_speed_pass::Call;
use naivepost::params;
use naivepost::speeds_ask;
use serde_json::json;

/// The SPEEDS document §`prompts/system.md`'s `speed` row specifies, in the chat envelope a real server
/// answers with, so the ask reads it through the same reply parser a live server's bytes go through. The
/// cache keeps the ANSWER text (§6 stores what the pass read, not the framing), which is why `bare` also
/// prints the document on its own: that is the shape the slot holds.
fn speeds_json(speeds: &[(u32, f64)]) -> String {
    set_speeds_json(speeds)
}

/// The SPEEDS document alone — the shape a cached answer is kept in.
fn speeds_text(speeds: &[(u32, f64)]) -> String {
    json!({
        "speeds": speeds.iter().map(|(clip, rate)| json!({ "clip": clip, "rate": rate })).collect::<Vec<_>>()
    })
    .to_string()
}

/// Which clips carry a caption in this ask: clip 3, with its one caption — and the ask says so in its own
/// words, `N caption(s) -- runs at 1` spelled as §F3.10 spells it.
fn captioned() -> Vec<(u32, usize)> {
    vec![(3, 1)]
}

/// A `set_clip_speed` tool call, in the shape a real server answers a tool conversation with. S2's reply uses
/// one: a model offered a tool answers with it, and `set_clip_speed` refuses a rate it will not apply rather
/// than staying silent about it.
fn tool_call(clip: u32, rate: f64) -> String {
    json!({
        "choices": [{"message": {"content": null, "tool_calls": [{
            "id": "call_1",
            "type": "function",
            "function": {"name": "set_clip_speed", "arguments": json!({"clip": clip, "rate": rate}).to_string()}
        }]}}]
    })
    .to_string()
}

#[test]
fn f3_10_s1_live_speeds_ask_carries_the_clips_message_to_the_llm() {
    let fixture = fixture("s1-msg");
    // TWO scripted replies, one per real dial: only asks that MISS the cache may reach the fake, so a wire
    // count of 1 after the second ask proves the slot answered it, and the third dial gets its own reply.
    // (The fake hands out one reply per request and repeats the last, so a second ask it never served is
    // invisible here — the requests.tsv WIRE row count is what proves nobody was dialled, as below.)
    let llm = FakeLlm::start(vec![set_speeds_json(&[(1, 1.6), (2, 2.4)])]);
    let addr = point_at(&llm.url());

    // A caption really is on clip 3: the count the ask quotes is read off a batch that holds a caption of
    // the length the tool accepts, not typed into the test as a decoration.
    let mut batch =
        naivepost::tools::clips::Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)]);
    batch.add_caption(3, 1.0, 6.0, "a line already on screen");
    assert_eq!(
        batch.captions_of(3).len(),
        1,
        "the tool placed the caption the brief will name"
    );
    let asked = speeds_ask::ask(
        &fixture.tree,
        "Run the dull stretches fast.",
        &three_clips(),
        &captioned(),
        1,
        &|| false,
    );
    assert!(
        !asked.failed,
        "the ask went out and was answered: {:?}",
        asked.reason
    );
    // S1's answer: both rates, in the order the model asked, nothing dropped on the way in.
    assert_eq!(
        asked.calls,
        vec![Call { clip: 1, rate: 1.6 }, Call { clip: 2, rate: 2.4 }]
    );

    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "the LLM's chat path");

    // The brief §F3.10 S1 spells: the footage total first, then each clip with its OWN seconds, and the
    // captioned one marked so the refusal in S2 is a rule the model was told before it broke it.
    // The request is JSON, so read the user message out of it instead of squinting at escaped text.
    let sent: serde_json::Value = serde_json::from_str(&seen.body).expect("the request is JSON");
    let user = sent["messages"][1]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        seen.body.contains("FOOTAGE: 60 seconds over 3 clip(s)."),
        "the footage total rode out to {addr}: {user}"
    );
    assert!(
        seen.body.contains("CLIP 1: 20.0 s long"),
        "clip 1 went with its own seconds"
    );
    // S1 (`briefs note captions ("…, N caption(s) -- runs at 1")`): the caption count rides in the clip's own
    // line, spelled by `cut_speed_pass::brief` — the ask quotes the very brief the rules already write, so the
    // wording that goes out and the wording the pass is pinned to cannot drift. S2 then refuses a fast rate
    // over that clip: a rule the model was told before it broke it.
    assert!(
        seen.body
            .contains("CLIP 3: 20.0 s long, 1 caption(s) -- runs at 1"),
        "the caption count is in the clip's own line, spelled as §F3.10 spells it"
    );
    assert!(
        !seen.body.contains("CLIP 1: 20.0 s long,"),
        "an uncaptioned clip carries no count -- the mark would refuse a rate the cut could take"
    );
    // No session seconds anywhere: the model is never asked for a number it would have to compute.
    assert!(
        !seen.body.contains("40.0 s long"),
        "clip 2's line is its own length, not where it sits: {addr}"
    );

    // S1's rules, said to the model rather than only enforced afterwards. The gap is quoted, so the number
    // the merge rule will use is the number the ask named: one value, read from §10's catalogue (§05 §6 is
    // where this section's rows live, not Prepare's list `params::find` searches).
    let gap = params::cut()
        .into_iter()
        .find(|row| row.id == "P.policy.speedGapSeconds")
        .expect("the gap is catalogued");
    assert!(
        seen.body
            .contains(&format!("nearer than {} s apart", gap.spelled)),
        "the merge gap the pass will apply is the gap the ask quoted ({}), got {:?}",
        gap.spelled,
        &seen.body
    );
    assert!(
        seen.body.contains("Only rates above 1"),
        "the rate floor was said"
    );
    assert!(
        seen.body
            .contains("Never a timestamp and never a second count"),
        "the no-timestamp rule was said"
    );
    assert!(
        seen.body
            .contains("words on screen at 4 are gone before they are read"),
        "the shipped speed wording rode out with the house rules"
    );

    // The two tools this item names, and ONLY those two. A model that is not told `set_clip_speed` exists
    // answers in prose about dullness instead of asking for a rate.
    let body: serde_json::Value = serde_json::from_str(&seen.body).expect("the request is JSON");
    let offered: Vec<&str> = body["tools"]
        .as_array()
        .expect("tools were offered")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    assert_eq!(
        offered.len(),
        3,
        "the write tool, `finish` and §3.7's read, got {offered:?}"
    );
    assert!(
        offered.contains(&"set_clip_speed"),
        "set_clip_speed was offered: {offered:?}"
    );
    assert!(
        offered.contains(&"finish"),
        "finish was offered so the reply can be closed: {offered:?}"
    );
    assert!(
        offered.contains(&"get_frames"),
        "§3.7's read rode out beside the write tool: {offered:?}"
    );
    assert_eq!(
        offered
            .iter()
            .filter(|name| **name != "set_clip_speed" && **name != "finish" && **name != "get_frames")
            .count(),
        0,
        "nothing else is offered -- a caption or decoration tool here would invite an answer this pass ignores"
    );
    // §4 gives speed no thinking, and the answer arrives with the model that was asked.
    assert_eq!(
        body["enable_thinking"].as_bool(),
        Some(false),
        "thinking is OFF for this prompt"
    );
    assert_eq!(
        body["model"].as_str(),
        Some("captions-model"),
        "the settings' model went out"
    );
    // The timed row landed: every ask writes one, whatever the answer was.
    let rows = read(&fixture.tree.requests_tsv());
    assert!(
        rows.contains("speed"),
        "requests.tsv carries this job's row: {rows}"
    );

    // The cache half of S1: the same ask again costs no wire. One scripted reply exists, so a second
    // request would be handed the same text and pass for the wrong reason — the WIRE row count is the proof,
    // because §09 §10 writes one row per REQUEST, and a cache hit's row is marked `cache`, not one.
    let again = speeds_ask::ask(
        &fixture.tree,
        "Run the dull stretches fast.",
        &three_clips(),
        &captioned(),
        1,
        &|| false,
    );
    assert_eq!(
        again.calls, asked.calls,
        "the cached answer is the same two rates"
    );
    let rows = read(&fixture.tree.requests_tsv());
    let wire = rows
        .lines()
        .filter(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            fields.contains(&"speed") && !fields.contains(&"cache")
        })
        .count();
    assert_eq!(
        wire, 1,
        "one timed row on the wire, so the second ask dialled nobody: {rows}"
    );
    // And the slot really answered the second ask (not a second dial that happened to match): the cache holds
    // the CALL that answered, which is what §6 keeps now that a call IS the answer.

    // Delete the cache and the same question is a fresh one: the wire is dialled again, and the row count says so.
    std::fs::remove_dir_all(fixture.root.join("session.naivepost/cache/llm"))
        .expect("cache folder removed");
    let third = speeds_ask::ask(
        &fixture.tree,
        "Run the dull stretches fast.",
        &three_clips(),
        &captioned(),
        1,
        &|| false,
    );
    assert_eq!(
        third.calls, asked.calls,
        "re-dialling answers the same, since nothing about the cut changed"
    );
    let rows = read(&fixture.tree.requests_tsv());
    let wire = rows
        .lines()
        .filter(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            fields.contains(&"speed") && !fields.contains(&"cache")
        })
        .count();
    assert_eq!(
        wire, 2,
        "a cache that is gone means the ask goes out again: {rows}"
    );
}
