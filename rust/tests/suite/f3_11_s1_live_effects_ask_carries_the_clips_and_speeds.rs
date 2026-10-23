//! F3.11 S1 over a socket: the decorations ask goes out with the brief §F3.11 spells — every clip, told its
//! captions AND its speeds — and offers only the two tools this job has. One scenario per binary
//! (`XDG_CONFIG_HOME`/cwd are process-wide) — see `captions_wire_harness.rs`.
//!
//! `rust/tests/suite/cut_effects_proposed.rs` proves the rules with calls handed in as data. This proves the wire:
//! the EFFECTS request reaches the address the settings hold carrying the caption and rate notes, and the
//! decoration it named comes back as an `add_effect` call.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::cut_effects_pass::{self, Call};
use naivepost::effects_ask;
use naivepost::tools::clips::Clips;
use serde_json::json;

/// The User Context every ask here uses, so the wording pinned below is the one that went out.
const CTX: &str = "Punch in on the score, hold the ending, duck the room under the last line.";

/// Three clips of 20 s each at their session seconds: every sum below is exact in binary, so a wrong total
/// cannot hide inside a rounding error.
fn batch() -> Clips {
    let mut batch = Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)]);
    // A real caption on clip 1 and a real rate on clip 2: the notes the ask quotes are read off a batch that
    // holds them, not typed into the test as a decoration.
    batch.add_caption(1, 1.0, 6.0, "a line already on screen");
    let sped = serde_json::from_str::<serde_json::Value>(&batch.set_clip_speed(2, 1.5))
        .expect("tool answer");
    assert_eq!(
        sped["applied"].as_f64(),
        Some(1.5),
        "clip 2 really plays at 1.5x"
    );
    batch
}

/// The rates the page hands the ask, read off that batch the way `ui::cut_effects_pass` does: only a clip whose
/// picture is already moving is named.
fn rates(batch: &Clips) -> Vec<(u32, f64)> {
    batch
        .clips()
        .iter()
        .filter(|clip| clip.rate != 1.0)
        .map(|clip| (clip.n, clip.rate))
        .collect()
}

/// The EFFECTS document §`prompts/system.md`'s `effects` row specifies, in the chat envelope a real server
/// answers with, so the ask reads it through the same reply parser a live server's bytes go through.
fn effects_json(entries: &[serde_json::Value]) -> String {
    add_effects_json(entries)
}

/// One `{clip, kind, start, end}` entry, gain left off unless given: an omitted field and a 1 are the same ask.
fn fx(clip: u32, kind: &str, start: f64, end: f64, gain: Option<f64>) -> serde_json::Value {
    let mut entry = json!({"clip": clip, "kind": kind, "start": start, "end": end});
    if let Some(gain) = gain {
        entry
            .as_object_mut()
            .expect("object")
            .insert("gain".into(), json!(gain));
    }
    entry
}

#[test]
fn f3_11_s1_live_effects_ask_carries_the_clips_and_speeds_to_the_llm() {
    let fixture = fixture("f311-s1-msg");
    let llm = FakeLlm::start(vec![effects_json(&[fx(3, "zoom", 2.0, 5.0, None)])]);
    let addr = point_at(&llm.url());

    let batch = batch();
    let asked = effects_ask::ask(&fixture.tree, CTX, &batch, &rates(&batch), 1, &|| false);
    assert!(
        !asked.failed,
        "the ask went out and was answered: {:?}",
        asked.reason
    );
    // S1's answer: the decoration, in the order the model asked, nothing dropped on the way in.
    assert_eq!(
        asked.calls,
        vec![Call {
            clip: 3,
            kind: "zoom".into(),
            start: 2.0,
            end: 5.0,
            gain: None
        }]
    );

    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "the LLM's chat path");
    let sent: serde_json::Value = serde_json::from_str(&seen.body).expect("the request is JSON");
    let user = sent["messages"][1]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // S1 (`one call over every clip · told the captions and speeds`). The footage total first, then EVERY clip
    // with its own seconds, the captioned one marked `N caption(s) -- runs at 1` and the sped one
    // `plays at Nx` — both spelled by `cut_effects_pass::brief`, so the wording that goes out and the wording
    // the placement rules are pinned to cannot drift.
    assert!(
        user.contains("FOOTAGE: 60 seconds over 3 clip(s)."),
        "the footage total rode out to {addr}: {user}"
    );
    assert!(
        user.contains("CLIP 1: 20.0 s long, 1 caption(s) -- runs at 1"),
        "the caption count is in clip 1's own line: {user}"
    );
    assert!(
        user.contains("CLIP 2: 20.0 s long, plays at 1.5x"),
        "the rate is in clip 2's own line: {user}"
    );
    assert!(
        !user.contains("CLIP 3: 20.0 s long,"),
        "a plain clip carries no note -- a wrong mark would refuse a decoration the cut could take"
    );
    // No session seconds anywhere: the model is never asked for a number it would have to compute (§00-principles).
    assert!(
        !user.contains("40.0 s long"),
        "clip 3's line is its own length, not where it sits"
    );

    // The rules §F3.11 states ride out as the ask's own wording, so a model that ignores them is refusing a
    // rule it was shown, not one only the app knows. (`spec/prompts/effects.md` itself is found by walking up
    // from the executable or the cwd; this scenario pins the cwd to its temp root, so the shipped file may or
    // may not be in reach here — what must always go out is `effects_ask`'s own statement of these rules.)
    assert!(
        user.contains("Few and deliberate"),
        "the density rule was said"
    );
    assert!(
        user.contains("two to four seconds"),
        "the zoom length rule was said"
    );
    assert!(
        user.contains("One stop per video"),
        "the one-stop rule was said"
    );
    assert!(
        user.contains("1 as recorded, 0 silent"),
        "volume's meaning was said"
    );
    assert!(
        user.contains("FROM THAT CLIP'S START"),
        "the offsets are clip-relative because a model never computes a timestamp: {user}"
    );
    assert!(
        user.contains("do not zoom past them"),
        "the never-past-captions rule was said"
    );

    // The two tools this item names, and ONLY those two: a model that is not told `add_effect` exists answers
    // in prose about pacing instead of asking for a zoom.
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
        offered.contains(&"add_effect"),
        "add_effect was offered: {offered:?}"
    );
    assert!(
        offered.contains(&"finish"),
        "finish was offered so the reply can be closed: {offered:?}"
    );
    assert!(
        offered.contains(&"get_frames"),
        "§3.7's read rode out beside the write tool: {offered:?}"
    );
    assert!(
        !seen.body.contains("set_clip_speed") && !seen.body.contains("add_caption"),
        "neither sibling pass's tool is offered here: {offered:?}"
    );
    // §4 gives `effects` no thinking, and the answer arrives with the model that was asked.
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
        rows.contains("effects"),
        "requests.tsv carries this job's row: {rows}"
    );

    // And the reply the fake printed is placeable: the app's own defaults, not the model's numbers, decide how
    // it lands — which is what makes the call above worth placing at all.
    let reply = cut_effects_pass::place(&batch, &asked.calls);
    assert!(reply.usable(), "a clip in the list keeps the reply usable");
    assert_eq!(reply.effects.len(), 1, "the decoration landed");
    assert_eq!(reply.effects[0].kind, "zoom", "as a zoom on the lane");
}
