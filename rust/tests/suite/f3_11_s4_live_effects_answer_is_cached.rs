//! F3.11's caching side over a socket, and the app's defaults that a cached answer still has to earn: an
//! effects answer is kept in the slot its own key names, the next identical ask opens no socket and writes no
//! second timed row, and the next ROUND keys elsewhere so a retry never reads back what it is retrying. One
//! scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide) — see `captions_wire_harness.rs`.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::cut_effects_pass::{self, Call};
use naivepost::effects_ask;
use naivepost::tools::clips::Clips;
use serde_json::json;

/// The User Context, model id and first round every ask here uses, so the cache key the test names is the one
/// the program computed.
const CTX: &str = "Punch in on the score, hold the ending.";
const MODEL: &str = "captions-model";
const ATTEMPT: u32 = 1;

/// Three clips of 20 s each at their session seconds.
fn batch() -> Clips {
    Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)])
}

/// The EFFECTS document in the chat envelope a real server answers with. The cache keeps the ANSWER text
/// (§6 stores what the pass read, not the framing).
fn effects_json(entries: &[serde_json::Value]) -> String {
    add_effects_json(entries)
}

fn fx(clip: u32, kind: &str, start: f64, end: f64, gain: Option<f64>) -> serde_json::Value {
    let mut entry = json!({"clip": clip, "kind": kind, "start": start, "end": end});
    if let Some(gain) = gain {
        entry.as_object_mut().expect("object").insert("gain".into(), json!(gain));
    }
    entry
}

/// The file this ask's answer belongs in — named by the program's own keying, not by a second hash recipe.
fn slot(fixture: &Fixture, attempt: u32) -> std::path::PathBuf {
    effects_ask::cache_file_for_test(&fixture.tree, CTX, &batch(), &[], MODEL, attempt)
}

/// How many asks this fake has ANSWERED on the wire, counted by timed rows (§09 §10: one row per
/// request, and a cache hit's row is not a request). The `job` column is `effects` for both kinds of
/// row, so the outcome is what tells them apart.
fn asked(fixture: &Fixture) -> usize {
    read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            fields.contains(&"effects")
                && !fields.contains(&"cache")
        })
        .count()
}

#[test]
fn f3_11_s4_live_effects_answer_is_cached_so_the_next_identical_ask_opens_no_socket() {
    let fixture = fixture("f311-s4-cached");
    // ONE scripted reply: were a later ask to reach this server it would be handed the same text and pass for
    // the wrong reason. What proves an ask did NOT go out is that the fake never saw it.
    let llm = FakeLlm::start(vec![effects_json(&[
        fx(1, "zoom", 3.0, 9.0, None),
        fx(2, "stop", 5.0, 7.0, None),
        fx(3, "volume", 0.0, 8.0, Some(0.0)),
        fx(3, "volume", 10.0, 14.0, Some(1.0)),
        fx(3, "volume", 15.0, 18.0, None),
    ])]);
    point_at(&llm.url());

    let batch = batch();
    let first = effects_ask::ask(&fixture.tree, CTX, &batch, &[], ATTEMPT, &|| false);
    assert_eq!(first.calls.len(), 5, "every line came back: the two volumes the spec ignores are `place`'s call, not the reader's");
    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "on the LLM's chat path");

    // §6 keeps what the pass READ — the answer's CALLS — so a replay needs no repair and no second dial. Since
    // §3's answer on this item the answer IS a call list, and the slot holds the answering round's envelope
    // carrying `add_effect` (see `clip_rules_tools_leg::answering_envelope`). This is the file the ask's own
    // key names, so the test reads the real slot.
    let kept = std::fs::read_to_string(slot(&fixture, ATTEMPT)).expect("the answer was cached under its own key");
    assert!(kept.contains("add_effect"), "the slot holds the call that answered: {kept}");

    // Same settings, same prompt, same round: the slot answers it. No socket, so no second WIRE row;
    // §09 §10 does want a line for the hit, marked `cache` with no time on the wire.
    let second = effects_ask::ask(&fixture.tree, CTX, &batch, &[], ATTEMPT, &|| false);
    assert_eq!(second.calls, first.calls, "the cached answer is the same decoration list");
    assert_eq!(asked(&fixture), 1, "the identical ask dialled nobody, so the wire rows did not grow");
    let cached_rows = read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| line.split('\t').any(|field| field == "cache"))
        .count();
    assert_eq!(cached_rows, 1, "§10: the cache hit is a line of its own");

    // A DIFFERENT round of the same cut is a different question and keys elsewhere, so it dials again: §F3.11's
    // second round must not be served the reply that was just rejected.
    assert_ne!(slot(&fixture, ATTEMPT), slot(&fixture, 2), "and the two rounds really do keep separate slots");
    let retry = effects_ask::ask(&fixture.tree, CTX, &batch, &[], 2, &|| false);
    assert_eq!(retry.calls, first.calls, "the retry is answered afresh (the fake repeats; the socket proves it)");
    assert_eq!(asked(&fixture), 2, "one timed row per request that went out, and the retry wrote no cache line");

    // ---- S4's defaults, applied to what the cached answer asked for -------------------------------
    //
    // The tool says WHERE a decoration landed; how tall its region is, how it fades, and whether a stop is a
    // speed are the app's rules. Every number below is exact in binary at these spans, so a default that moved
    // by a hair shows up as a wrong assertion rather than a rounding argument.
    let reply = cut_effects_pass::place(&batch, &second.calls);
    assert!(reply.usable(), "every clip named is in the batch");
    assert_eq!(reply.effects.len(), 3, "two of the four volumes are ignored: a gain of 1 or none asks for nothing");
    let ignored = reply
        .answers
        .iter()
        .filter(|answer| matches!(answer, cut_effects_pass::Answer::Ignored))
        .count();
    assert_eq!(ignored, 2, "`gain of 1 or none ignored` — both of them said so");

    // zoom: centred at 60 % of the frame height, glides min(1, d/3) → 6 s / 3 = 2 clamped to the 1 s cap.
    let zoom = &reply.effects[0];
    assert_eq!((zoom.kind.as_str(), zoom.t, zoom.dur), ("zoom", 3.0, 6.0), "the zoom sits where the clip's offsets put it");
    assert_eq!((zoom.cx, zoom.cy), (Some(0.5), Some(0.5)), "centred: the pass sends no box");
    assert_eq!(zoom.hf, Some(cut_effects_pass::ZOOM_HEIGHT), "at {} of the frame height", cut_effects_pass::ZOOM_HEIGHT);
    assert_eq!(zoom.hf, Some(0.6), "// effects.proposedZoomHeight");
    assert_eq!((zoom.trans, zoom.tout), (1.0, 1.0), "glides min(1, d/3) with d = 6");
    assert_eq!(cut_effects_pass::glide(1.5), 0.5, "and a short zoom gets the quarter itself");

    // stop: a rate-0 speed, fades min(0.3, d/4) → 2 s / 4 = 0.5 clamped to 0.3.
    let stop = &reply.effects[1];
    assert_eq!((stop.kind.as_str(), stop.t, stop.dur, stop.rate), ("speed", 25.0, 2.0, 0.0), "a stop is a rate-0 speed on the clip's own seconds");
    assert_eq!((stop.trans, stop.tout), (0.3, 0.3), "fades min(0.3, d/4) with d = 2");
    assert_eq!(cut_effects_pass::stop_fade(0.8), 0.2, "and a shorter stop gets its quarter instead");

    // volume: the gain as sent, KEPT at 0 (silence is a decision someone can mean), ramps min(1, d/4) → 8 / 4 = 2
    // clamped to the 1 s cap.
    let volume = &reply.effects[2];
    assert_eq!((volume.kind.as_str(), volume.t, volume.dur, volume.gain), ("volume", 40.0, 8.0, 0.0), "gain 0 is kept, not waved away");
    assert_eq!((volume.trans, volume.tout), (1.0, 1.0), "ramps min(1, d/4) with d = 8");
    assert_eq!(cut_effects_pass::ramp(2.0), 0.5, "and a short stretch gets its quarter");

    // The call list itself, for anyone reading the reply shape rather than the records.
    assert_eq!(
        second.calls,
        vec![
            Call { clip: 1, kind: "zoom".into(), start: 3.0, end: 9.0, gain: None },
            Call { clip: 2, kind: "stop".into(), start: 5.0, end: 7.0, gain: None },
            Call { clip: 3, kind: "volume".into(), start: 0.0, end: 8.0, gain: Some(0.0) },
            Call { clip: 3, kind: "volume".into(), start: 10.0, end: 14.0, gain: Some(1.0) },
            Call { clip: 3, kind: "volume".into(), start: 15.0, end: 18.0, gain: None },
        ][..],
        "gain stays an Option: absent and 1.0 are the same ask, 0.0 is silence"
    );
}
