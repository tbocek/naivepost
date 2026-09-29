//! F4.2 S3 over a socket: the narration call goes out over HTTP to the address the Settings hold, and
//! a content answer comes back matched to the clip it echoed. One scenario per binary (`XDG_CONFIG_HOME`
//! and cwd are process-wide) -- see the harness.

#![allow(dead_code)]

#[path = "narrate_live_harness.rs"]
mod harness;

use harness::*;
use naivepost::narrate_live;

#[test]
fn f4_2_s3_a_live_narration_call_goes_over_http_to_the_settings_address() {
    let fixture = fixture("http");
    // The answer echoes BOTH clips' bounds: §F4.2's audit says every clip must be answered, so a
    // reply that speaks for one clip and skips the other is refused ("clip 2 ... got no entry") —
    // silence has to be said, here as an empty-text entry on clip 2.
    let answer = entries_answer(&[
        (0.0, 10.0, 2.0, "Proof of stake changed the whole cost.", "calm"),
        (10.0, 20.0, 1.0, "", ""),
    ]);
    let llm = FakeLlm::start(vec![answer]);
    let addr = point_at(&llm.url());
    let (segs, transcript, narrator) = material(&fixture);
    let request = build_request(&fixture);

    // One ask over the socket, then the matching through drive's own rules. Asked explicitly rather
    // than inside the loop because every round lands its own `requests.tsv` row, and the claim below
    // is about ONE request. A real run keeps all three attempts (`narrate_ask` gates nothing).
    // Count what was on the wire BEFORE asking, and compare by DELTA. `requests.tsv` is the
    // session's own log and every caller appends to it, and one `ask_one` may itself make more than
    // one dial (§09 §2's tool loop: a round that arrives with no calls and no `finish` spends a round
    // on FINISH_NUDGE). So the claim below is about the rows THIS ask added — exactly one, for the
    // request whose body and reply are asserted above.
    let before = std::fs::read_to_string(fixture.tree.requests_tsv())
        .map(|_| rows(&fixture).len())
        .unwrap_or(0);
    let round = ask_one(&fixture, 1, true, &|| false)
        .expect("the live call answers over the socket");
    assert!(!round.used_tools, "this answer came back as content, not as tool calls");
    // The matching is what `drive` does, run over the answer ALREADY read from this one request:
    // calling `drive` here would dial again and land a second `requests.tsv` row for a request this
    // test never made. Same forward-only ±0.5 s rule, same tolerance constant.
    let mut written: Vec<naivepost::narrate_run::Written> = Vec::new();
    let document: serde_json::Value = serde_json::from_str(&round.answer)
        .expect("the answer read off the socket is the entries document");
    let entries = document.get("entries").and_then(|e| e.as_array())
        .expect("the document has an entries list");
    let mut cursor = 0usize;
    for entry in entries {
        let start = entry.get("start").and_then(|v| v.as_f64()).unwrap_or_default();
        let end = entry.get("end").and_then(|v| v.as_f64()).unwrap_or_default();
        let found = (cursor..segs.len()).find(|&i| {
            (start - segs[i].s).abs() <= naivepost::narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS
                && (end - segs[i].e).abs() <= naivepost::narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS
        });
        let Some(index) = found else {
            panic!("{}", naivepost::narrate_reply::unmatched_entry_fault(start, end));
        };
        cursor = index + 1;
        written.push(naivepost::narrate_run::Written {
            start: segs[index].s,
            end: segs[index].e,
            at: entry.get("at").and_then(|v| v.as_f64()).unwrap_or_default(),
            text: entry.get("text").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            emotion: entry.get("emotion").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        });
    }

    assert_eq!(written.len(), 2, "both clips answered over the socket — one line, one said-silent");
    assert_eq!(written[0].start, 0.0, "matched to clip 1's own start");
    assert_eq!(written[0].end, 10.0, "and its end");
    assert_eq!(written[0].at, 2.0, "at the offset the reply gave");
    assert_eq!(written[0].text, "Proof of stake changed the whole cost.");
    // The empty-text entry survives as an ANSWER (F4.2: empty text is a deliberate answer), it is not
    // a missing clip; only a whole reply of empty lines is refused.
    assert_eq!(written[1].start, 10.0, "clip 2 got its answer too");
    assert!(written[1].text.is_empty(), "and that answer is silence, kept as such");

    // The wire: a POST to the chat path, at the address the Settings now name.
    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "the LLM's chat path");
    assert!(
        seen.body.contains("narrate-model"),
        "the model from the Settings file went out with the request"
    );
    // The brief rode out: clips, their headings, and what was said over them.
    assert!(
        seen.body.contains(naivepost::narrate_pass::CLIPS_HEADER),
        "the clips brief header went out"
    );
    assert!(
        seen.body.contains("CLIP 1: 0.0\u{2013}10.0"),
        "clip 1's heading with its own bounds: {}",
        &seen.body[..seen.body.len().min(600)]
    );
    assert!(
        seen.body.contains("at most") && seen.body.contains("fewer is better, none is fine"),
        "the word ceiling rode with the heading"
    );
    // The transcript lines are offsets inside the clip, never session seconds.
    assert!(
        seen.body.contains("[+1s]"),
        "the line over clip 1 went as a clip-relative offset"
    );

    // requests.tsv: the two rows `ask_one` lands for a content answer — the live request itself,
    // then the round the tool loop spends on FINISH_NUDGE because that answer came back with no
    // `finish` call on it. Both really went over the socket, so both really get a row (§09 §10: one
    // line per request, a retry a line of its own), and each holds this job, this service, a success.
    // Asked explicitly rather than inside the attempt loop so the count is one ask, not three.
    let logged = rows(&fixture);
    assert_eq!(
        logged.len(),
        before + 2,
        "the ask left the live row and the nudge row (was {before}, now {}): {logged:?}",
        logged.len()
    );
    // The fields §10 names, asserted by column on the row the live call left. Bytes and tokens are
    // left alone here (the fake server sends no usage block); the timings come off the row rather
    // than being hardcoded, since they are the fake server's real clock.
    let last = logged.last().expect("the ask left its rows");
    let fields: Vec<&str> = last.split('\t').collect();
    assert_eq!(fields[2], "narrate", "step column: {last}");
    assert_eq!(fields[3], "narrate", "job column: {last}");
    assert_eq!(fields[4], "llm", "service column: {last}");
    assert_eq!(fields[5], "narrate-model", "model column: {last}");
    assert_eq!(fields[6], "chat", "kind column: {last}");
    assert_eq!(fields[16], "ok", "outcome column: {last}");
    assert!(
        !fields[14].is_empty() && fields[14].parse::<f64>().is_ok(),
        "on_wire_s filled for a request that really was on the wire: {last}"
    );
}
