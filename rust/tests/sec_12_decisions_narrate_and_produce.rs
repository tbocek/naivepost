//! §12-decisions.md · **Narrate and Produce** — the decisions the narration call makes, checked on the
//! live path rather than in prose.
//!
//! The round's finding was that ▶ Write and speak answered through `narrate_call::drive`, the old prose
//! matcher with echoed ±0.5 s bounds, so a writer that answered with `write_line` calls had its clip
//! numbers thrown away and re-guessed from seconds. Entries 22/23/25/26/27/31 are now what decides that
//! answer: `narrate_call::drive_by_clips` takes the tool calls' CLIP NUMBERS, in any order, one per clip.

use naivepost::cut::Seg;
use naivepost::narrate_call;
use naivepost::narrate_pass;
use naivepost::narrate_run::Written;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;

/// The two clips every scenario uses: ten seconds each, camera down so nothing is "over a drawing".
fn two_clips() -> Vec<Seg> {
    vec![
        Seg { s: 0.0, e: 10.0, cam: 0, ..Default::default() },
        Seg { s: 10.0, e: 20.0, cam: 0, ..Default::default() },
    ]
}

fn fits(_: &[Written]) -> Vec<u32> {
    Vec::new()
}

/// One round's worth of tool picks, in the order given, as `narrate_live` hands them over.
fn picks(list: &[(u32, f64, &str, &str)]) -> Vec<naivepost::narrate_live::Picked> {
    list.iter()
        .map(|(clip, at, text, emotion)| naivepost::narrate_live::Picked {
            clip: *clip,
            at: *at,
            text: (*text).to_string(),
            emotion: (*emotion).to_string(),
        })
        .collect()
}

fn tool_round(list: &[(u32, f64, &str, &str)]) -> naivepost::narrate_live::Round {
    naivepost::narrate_live::Round {
        answer: String::new(),
        picks: picks(list),
        used_tools: true,
        requests: 1,
    }
}

/// Entry 22: one clip skipped rejects the WHOLE reply, and the refusal names the clip — thirty-nine good
/// clips are no longer lost for the fortieth without saying which one was missing.
#[test]
fn sec_12_decisions_narrate_and_produce_s22_a_skipped_clip_is_named_not_mysteriously_rejected() {
    let mut refusals: Vec<String> = Vec::new();
    let got = narrate_call::drive_by_clips(
        &two_clips(),
        |_, _| Ok(tool_round(&[(1, 2.0, "Proof of stake changed the cost.", "calm")])),
        |_, _| {},
        &fits,
        |line| refusals.push(line.to_string()),
    );
    assert!(got.is_err(), "a reply that never spoke for clip 2 is refused");
    assert!(
        refusals.iter().any(|line| line.contains("clip 2")),
        "the refusal names the clip nobody answered: {refusals:?}"
    );
    // And a clip number outside this edit's clips is refused by that number, not read as an offset.
    let mut said: Vec<String> = Vec::new();
    let outside = narrate_call::drive_by_clips(
        &two_clips(),
        |_, _| Ok(tool_round(&[(1, 0.0, "one", ""), (2, 0.0, "two", ""), (7, 0.0, "seven", "")])),
        |_, _| {},
        &fits,
        |line| said.push(line.to_string()),
    );
    assert!(outside.is_err(), "clip 7 is not one of two clips");
    assert!(
        said.iter().any(|line| line.contains("clip 7") && line.contains("not one of")),
        "the refusal quotes the out-of-range number: {said:?}"
    );
}

/// Entry 23: `write_line` answers where the line lands AND whether it will be squeezed, and `finish` names
/// the clips whose lines will not fit, so they get rewritten instead of sped up by the render.
#[test]
fn sec_12_decisions_narrate_and_produce_s23_the_answer_says_where_it_lands_and_whether_it_is_squeezed() {
    let clips = two_clips();
    let short = narrate_pass::write_line(&clips, 1, 1.0, "A short line.", "", "");
    let body: serde_json::Value = serde_json::from_str(&short).expect("write_line answers JSON");
    assert_eq!(body["clip"], 1, "the answer confirms the clip it wrote to");
    assert!(body.get("squeeze").is_none(), "a short line on a ten-second clip is not squeezed: {body}");

    // A line far too long for the room left after its offset gets the render's own warning, in the answer.
    let long_text = "and ".repeat(200);
    let long = narrate_pass::write_line(&clips, 1, 8.0, &long_text, "", "");
    let body: serde_json::Value = serde_json::from_str(&long).expect("write_line answers JSON");
    assert!(
        body.get("squeeze").is_some(),
        "an over-long line near the clip's end says so: {body}"
    );

    // And `finish` names the unfitted clip, in words that tell the writer what to do about it.
    let verdict = narrate_pass::finish(&[1, 2], &[2]);
    assert!(verdict.contains("clip(s) 2"), "{verdict}");
    assert!(verdict.contains("rewrite shorter"), "{verdict}");
}

/// Entry 25: the emotion comes back RESOLVED into its weights — one intent, one performance — and
/// `list_emotions` publishes the table the writer is held to.
#[test]
fn sec_12_decisions_narrate_and_produce_s25_the_emotion_comes_back_resolved() {
    let clips = two_clips();
    // A weighted tag is the case §12 #25 is about: one intent must make ONE performance, so the answer
    // states the weights the engine will be sent rather than echoing the word back.
    let answer = narrate_pass::write_line(&clips, 1, 1.0, "The chain moved.", "happy=1,surprised=0.5", "");
    let body: serde_json::Value = serde_json::from_str(&answer).expect("write_line answers JSON");
    let weights = body.get("weights").expect("the answer carries the resolved weights");
    assert!(!weights.is_null(), "a weighted emotion resolves to weights: {body}");
    assert_eq!(
        weights.as_str().unwrap(),
        naivepost::narrate_tts::weights_of("happy=1,surprised=0.5").unwrap_or_default(),
        "the answer states the SAME string the speech engine is sent, not a second reading of the tag"
    );
    // A bare tag names an intent but no numbers; the answer says so instead of inventing a default weight.
    let bare: serde_json::Value =
        serde_json::from_str(&narrate_pass::write_line(&clips, 1, 1.0, "x", "excited", "")).expect("ok");
    assert!(bare["weights"].is_null(), "a bare tag has no weights to report: {bare}");
    // An unknown name is refused outright rather than routed elsewhere (§12 #25's second half).
    let bad = narrate_pass::write_line(&clips, 1, 1.0, "x", "not-an-emotion", "");
    assert!(bad.contains("error"), "an unknown emotion is an error: {bad}");

    let table = narrate_pass::list_emotions();
    for base in naivepost::narrate_screen::EMOTIONS {
        assert!(table.contains(base), "`list_emotions` omits {base}: {table}");
    }
}

/// Entry 26: a `pos` outside the known words is an ERROR on `write_line`, not a silent bottom.
#[test]
fn sec_12_decisions_narrate_and_produce_s26_an_unknown_pos_is_refused_not_silently_bottom() {
    let clips = two_clips();
    let answer = narrate_pass::write_line(&clips, 1, 1.0, "A line.", "", "middle-ish");
    assert!(answer.contains("error"), "an unspellable placement is refused: {answer}");
    assert!(answer.contains("middle-ish"), "the refusal quotes what was sent: {answer}");
    // The three real placements still land, and bottom stays the default.
    for (word, stored) in [("top", "top"), ("center", "center"), ("bottom", "")] {
        let ok = narrate_pass::write_line(&clips, 1, 1.0, "A line.", "", word);
        let body: serde_json::Value = serde_json::from_str(&ok).expect("ok answer");
        assert_eq!(body["pos"], stored, "// pos {word}");
    }
}

/// Entry 27: ONE policy — the tool takes any order. Three clips answered 3, 1, 2 land in clip order, and
/// writing the same clip twice is refused by name instead of silently keeping one of the two.
#[test]
fn sec_12_decisions_narrate_and_produce_s27_one_policy_the_tool_takes_any_order() {
    let clips = vec![
        Seg { s: 0.0, e: 10.0, cam: 0, ..Default::default() },
        Seg { s: 10.0, e: 20.0, cam: 0, ..Default::default() },
        Seg { s: 20.0, e: 30.0, cam: 0, ..Default::default() },
    ];
    let got = narrate_call::drive_by_clips(
        &clips,
        |_, _| {
            Ok(tool_round(&[
                (3, 1.0, "third clip first", ""),
                (1, 1.0, "then the first", ""),
                (2, 1.0, "then the second", ""),
            ]))
        },
        |_, _| {},
        &fits,
        |_| {},
    )
    .expect("any order is accepted");
    assert_eq!(got.len(), 3);
    assert_eq!(
        got.iter().map(|line| line.start).collect::<Vec<f64>>(),
        vec![0.0, 10.0, 20.0],
        "the lines come out ordered by clip, whatever order they arrived in"
    );
    assert_eq!(got[0].text, "then the first");
    assert_eq!(got[2].text, "third clip first");

    // Two answers for one clip: refused, and named — the opposite of the old re-sort-and-hope.
    let mut said: Vec<String> = Vec::new();
    let twice = narrate_call::drive_by_clips(
        &clips,
        |_, _| {
            Ok(tool_round(&[
                (1, 0.0, "first take", ""),
                (2, 0.0, "second", ""),
                (3, 0.0, "third", ""),
                (1, 5.0, "a replay of clip 1", ""),
            ]))
        },
        |_, _| {},
        &fits,
        |line| said.push(line.to_string()),
    );
    assert!(twice.is_err(), "a clip written twice is not silently de-duplicated");
    assert!(
        said.iter().any(|line| line.contains("clip 1") && line.contains("twice")),
        "the refusal names the clip: {said:?}"
    );
}

/// Entry 31: the word ceiling is COUNTED, not advised — the answer states the line's words against it.
#[test]
fn sec_12_decisions_narrate_and_produce_s31_the_word_ceiling_is_counted_in_the_answer() {
    let clips = two_clips();
    let answer = narrate_pass::write_line(&clips, 1, 0.0, "one two three four five", "", "");
    let body: serde_json::Value = serde_json::from_str(&answer).expect("write_line answers JSON");
    assert_eq!(body["words"], 5, "the count is in the answer: {body}");
    assert_eq!(
        body["at_most_words"],
        serde_json::json!(narrate_pass::word_ceiling(narrate_pass::on_screen(&clips[0]))),
        "and it is stated against the same ceiling the brief prints"
    );
}

/// The live wire: a server answering with TOOL CALLS has its `write_line`/`leave_silent` calls matched by
/// clip number, not re-guessed from echoed bounds. Loopback fake, the `narrate_live_harness` arrangement.
#[path = "narrate_live_harness.rs"]
mod harness;

#[test]
fn sec_12_decisions_narrate_and_produce_s27_the_live_call_answers_through_write_line() {
    use harness::*;
    use serde_json::json;
    let fixture = fixture("sec12live");
    // Clip 2 answered FIRST, then clip 1 — the order §12 #27 says the tool must take. Bounds are NOT echoed
    // at all: only clip numbers, which is what makes any order safe. The second ask is the loop's FINISH
    // nudge (the writer never calls `finish` itself here), and the fake answers it with the LAST reply,
    // so both rounds of one attempt see these two calls — hence "written twice" below if the driver ever
    // re-counts a round.
    let llm = FakeLlm::start(vec![
        tool_calls_answer(&[
            (
                "write_line",
                json!({"clip": 2, "at": 1.0, "text": "Second clip, written first.", "emotion": ""}),
            ),
            (
                "write_line",
                json!({"clip": 1, "at": 2.0, "text": "Proof of stake changed the cost.", "emotion": "calm"}),
            ),
        ]),
        tool_calls_answer(&[("finish", json!({}))]),
    ]);
    point_at(&llm.url());
    let mut refusals: Vec<String> = Vec::new();
    let written = narrate_call::drive_by_clips(
        &material(&fixture).0,
        |attempt, thinking| ask_one(&fixture, attempt, thinking, &|| false),
        |_, _| {},
        &fits,
        |line| refusals.push(line.to_string()),
    )
    .unwrap_or_else(|why| panic!("the tool answer should land, refusals were {refusals:?}: {why}"));
    assert_eq!(written.len(), 2, "both clips came from write_line calls");
    assert_eq!(
        written.iter().map(|line| line.text.as_str()).collect::<Vec<_>>(),
        vec!["Proof of stake changed the cost.", "Second clip, written first."],
        "ordered by clip, built from the clip-numbered picks"
    );
    assert_eq!(written[0].emotion, "calm", "the emotion the tool call carried survived");
    assert!(refusals.is_empty(), "nothing was refused: {refusals:?}");
}

/// Entry 28: an inserted card is described by what the app DREW on it — its text, its parameters and its
/// length — not by its file name.
#[test]
fn sec_12_decisions_narrate_and_produce_s28_describe_insert_reports_the_rendered_card() {
    let root = common::temp_dir_created("card28");
    // A card written the way the app's own seed writes one (a stamped tier board), then filled through the
    // same door the render uses, so the words below are the words a frame would show.
    let card_path = root.join("tier.svg");
    // A board drawn the way the app draws one, with an arrival named: `new=` is a real hole on this card, so
    // filling it through the render's own door is what proves the words come from the DRAWN document.
    std::fs::write(&card_path, naivepost::cut_cards::tier_svg(&[], &[])).expect("card written");
    let raw = std::fs::read_to_string(&card_path).expect("card read back");
    let drawn = naivepost::produce_cards::draw("tier.svg?new=Dust%20II", &raw);
    let shown = naivepost::cut_cards::card_words(&drawn.drawn);
    assert!(
        shown.iter().any(|line| line.contains("Dust II")),
        "the answer reads the card's own text out of the drawn document: {shown:?}"
    );

    // The tool's answer for a clip holding that insert.
    let clips = vec![Seg {
        s: 0.0,
        e: 6.0,
        ins: format!("{}?new=Dust%20II&offset=1.50", card_path.display()),
        ..Default::default()
    }];
    let answer = narrate_pass::describe_insert(&clips, 1);
    let body: serde_json::Value = serde_json::from_str(&answer).expect("describe_insert answers JSON");
    assert_eq!(body["seconds_on_screen"], 6.0, "the length is stated: {body}");
    // §12 #39's second half: the answer says WHERE the app placed the card, in the spelling the cut carries.
    assert_eq!(
        body["placed_at"].as_str(),
        Some("1.50s from the clip's start"),
        "the placement is stated, not left for the model to guess: {body}"
    );
    let params = body["params"].as_array().expect("the parameters are listed");
    assert!(
        params.iter().any(|p| p.as_str().is_some_and(|p| p.starts_with("new="))),
        "the path's arguments come back as parameters: {body}"
    );
    // §12 #38: a card is not footage, and the answer says so — the description writer is told this clip is a
    // still, which is the difference between "the screen shows the scoreboard" and "the player drives on".
    assert_eq!(
        body["still"].as_bool(),
        Some(true),
        "a card clip is reported as a still, not as footage: {body}"
    );
    let footage = vec![Seg { s: 0.0, e: 6.0, ..Default::default() }];
    let refused = narrate_pass::describe_insert(&footage, 1);
    assert!(
        refused.contains("footage"),
        "a footage clip is refused, not silently answered as a still: {refused}"
    );
    let says = body["says"].as_array().expect("the card's words are in the answer");
    assert!(
        says.iter().any(|w| w.as_str().is_some_and(|w| w.contains("Dust II"))),
        "what the viewer sees is named, not just the file: {body}"
    );
    // Footage is refused rather than answered with an empty card.
    let footage = narrate_pass::describe_insert(&two_clips(), 1);
    assert!(footage.contains("not an insert"), "{footage}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Entry 29: the narration brief carries EVERY effect kind, in the dropdown's own words — a zoom, a burned-in
/// graphic and a mute are invisible to the writer otherwise.
#[test]
fn sec_12_decisions_narrate_and_produce_s29_the_brief_lists_every_effect_kind() {
    use naivepost::cut::Fx;
    let clips = two_clips();
    let marks = vec![
        Fx { kind: "zoom".into(), t: 1.0, dur: 3.0, ..Default::default() },
        Fx { kind: "svg".into(), t: 2.0, dur: 2.0, ..Default::default() },
        Fx { kind: "volume".into(), t: 12.0, dur: 4.0, gain: 0.0, ..Default::default() },
        Fx { kind: "speed".into(), t: 15.0, dur: 2.0, rate: 4.0, ..Default::default() },
    ];
    let brief = narrate_pass::brief(&clips, &[], &marks, "");
    for tag in ["ZOOM", "GRAPHIC", "VOLUME", "SPEED"] {
        assert!(brief.contains(tag), "the brief omits {tag}: {brief}");
    }
}

/// Entry 30: the upload brief prints each clip's RATE, so a 4x clip's stamps are on a clock the writer is told
/// about, and carries the decorations standing on the clip.
#[test]
fn sec_12_decisions_narrate_and_produce_s30_the_upload_brief_prints_the_rate() {
    use naivepost::cut::Fx;
    use naivepost::produce_upload::{self, BriefClip};
    let seg = Seg { s: 0.0, e: 8.0, rate: 4.0, ..Default::default() };
    assert_eq!(produce_upload::brief_rate(&seg), 4.0);
    let marked = produce_upload::brief_marks(
        &[Fx { kind: "label".into(), t: 1.0, dur: 2.0, text: "the reveal".into(), ..Default::default() }],
        &seg,
    );
    assert!(
        marked.iter().any(|m| m.contains("the reveal")),
        "a moment marked 'the reveal' is not nothing: {marked:?}"
    );
    let clips = vec![BriefClip {
        number: 1,
        at_output: 0.0,
        seconds: 2.0,
        session: "0:00\\u{2013}0:08".to_string(),
        seen: vec!["a slide".to_string()],
        said: vec!["Ann: proof of stake".to_string()],
        rate: produce_upload::brief_rate(&seg),
        marked,
    }];
    let brief = produce_upload::brief(&clips, &[], usize::MAX);
    assert!(brief.contains("4"), "the rate is printed: {brief}");
    assert!(brief.contains("MARKED:"), "the decoration is printed: {brief}");
}

// ---- the upload side's answers (entries 32-36) ---------------------------------------------------

use naivepost::project::{Publish, TitleBox};

/// The legs a walk needs, each one a closure that records that it ran. Mirrors `walk_with` in
/// `tests/produce_upload_flow.rs`: nothing here opens a socket or spawns anything.
#[derive(Default)]
struct Legs {
    /// Which legs ran, in order — `print-title` / `print-texts` are what entries 34 and 36 turn on.
    calls: std::cell::RefCell<Vec<String>>,
    /// The record the publish leg last received, i.e. what `publish.json` would have held.
    published: std::cell::RefCell<Option<Publish>>,
}

/// Run one walk with recording legs. `edited_by_hand` is entry 34's switch.
fn upload_walk(
    legs: &Legs,
    publish: &Publish,
    answers: &naivepost::produce_upload::Answers,
    edited_by_hand: bool,
    log: &mut impl FnMut(&str),
) -> naivepost::produce_upload::Outcome {
    use naivepost::produce_upload as up;
    // Each leg captures ONE borrowed value with a lifetime the compiler can see on its own: a closure that
    // captured the `&Legs` parameter is inferred `+ 'static` and `Materials` will not take that. The statics
    // are per-test-call scratch, the same trick `tests/produce_upload_flow.rs` uses with thread_locals.
    type Box2<T> = std::cell::RefCell<T>;
    static CALLS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    static PRINTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    CALLS.lock().unwrap().clear();
    PRINTED.lock().unwrap().clear();
    // A cell the publish leg can hold for the `Materials` lifetime `walk` needs, read back below.
    let published: &'static std::cell::RefCell<Option<Publish>> =
        Box::leak(Box::new(std::cell::RefCell::new(None)));
    let calls = &CALLS;
    let print_calls = &PRINTED;
    let save_publish = |next: &Publish| {
        published.replace(Some(next.clone()));
        calls.lock().unwrap().push("publish".to_string());
        Ok(())
    };
    let ask_draw = |_b: &serde_json::Value| {
        Ok::<up::DrawnJob, String>(up::DrawnJob { id: "job-1".into() })
    };
    let poll = |_j: &up::DrawnJob| {
        Ok(up::PollState { status: "done".into(), ahead: 0, done: true, png: vec![1, 2, 3] })
    };
    // A closure with NO captures is the one kind that can be a `&'static dyn Fn`, which is what `Materials`
    // asks for; the legs that touch `calls` borrow it with the fn's own lifetime (see the mats block below).
    let ask_model = |_b: &str| Ok(String::new());
    let save_picture = |_j: &up::DrawnJob, _png: &[u8]| Ok(());
    let print = |kind: &str, _words: &[String]| {
        print_calls.lock().unwrap().push(format!("print-{kind}"));
        Ok(())
    };
    let write_stamp = |_h: &str| {
        calls.lock().unwrap().push("stamp".to_string());
        Ok(())
    };
    // S4's three frames carry a marker so a test can tell the walk filled the row from them; the marker is
    // the number the walk logs, not a leg call, so it cannot reorder what `calls` records.
    let spread = || {
        vec![
            "frame:5.0".to_string(),
            "frame:15.0".to_string(),
            "frame:25.0".to_string(),
        ]
    };
    // A frame far from anything extracted resolves to nothing — the case §12 #33 is about, where the request
    // cannot be honoured. The page's own leg answers this question off the extracted-frame list.
    let nearest = |at: f64| -> Option<f64> {
        if at > 100.0 {
            return None;
        }
        Some(34.0f64)
    };
    let request_images = |_p: &Publish| up::Sent {
        base: Some("data:image/png;base64,AAAA".to_string()),
        urls: vec!["data:image/png;base64,AAAA".to_string()],
        base_name: "frame.jpg".to_string(),
        crop_percent: 100,
        centre: (0.5, 0.5),
        skipped: Vec::new(),
    };
    let own_picture = |_p: &Publish| Ok(());
    let mats = up::Materials {
        save_publish: &save_publish as &(dyn Fn(&Publish) -> Result<(), String> + 'static),
        ask_model: &ask_model as &(dyn Fn(&str) -> Result<String, String> + 'static),
        ask_draw: &ask_draw
            as &(dyn Fn(&serde_json::Value) -> Result<up::DrawnJob, String> + 'static),
        poll: &poll as &(dyn Fn(&up::DrawnJob) -> Result<up::PollState, String> + 'static),
        save_picture: &save_picture
            as &(dyn Fn(&up::DrawnJob, &[u8]) -> Result<(), String> + 'static),
        print: &print as &(dyn Fn(&str, &[String]) -> Result<(), String> + 'static),
        nearest_frame: &nearest as &(dyn Fn(f64) -> Option<f64> + 'static),
        spread_frames: &spread as &(dyn Fn() -> Vec<String> + 'static),
        stored_stamp: None,
        write_stamp: &write_stamp as &(dyn Fn(&str) -> Result<(), String> + 'static),
        edited_by_hand,
        request_images: &request_images as &(dyn Fn(&Publish) -> up::Sent + 'static),
        own_picture: &own_picture as &(dyn Fn(&Publish) -> Result<(), String> + 'static),
    };
    let out = up::walk("THE BRIEF", publish, answers, &mats, log);
    // The recording legs wrote into the statics and the leaked cell; hand the test back what they saw.
    legs.calls.replace(std::mem::take(&mut *calls.lock().unwrap()));
    for kind in PRINTED.lock().unwrap().iter() {
        legs.calls.borrow_mut().push(kind.clone());
    }
    legs.published.replace(published.take());
    let _ = Box2::<u8>::new(0);
    out
}

/// Entry 32: `pick_frame` answers with the frame ACTUALLY taken and how far it sits from the moment asked
/// for. The prototype printed the frame's time and never the distance, so "clip 3 +12" could land 20 s away
/// and read like a hit.
#[test]
fn sec_12_decisions_narrate_and_produce_s32_a_named_frame_reports_the_distance_it_landed_at() {
    use naivepost::produce_upload::picked_frame_answer;
    // Clip 3 starts at 20 s on the produced clock; the only frame extracted sits at 34 s.
    let picked = naivepost::produce_upload::pick_frame(3, 12.0, &[0.0, 10.0, 20.0], &[34.0])
        .expect("a frame was extracted, so one can be taken");
    assert_eq!(picked.asked, 32.0, "clip 3 + 12 s is 32 s on the produced clock");
    assert_eq!(picked.taken, 34.0, "the frame actually taken is stated");
    assert_eq!(picked.distance, 2.0, "and so is how far it was from what was asked");
    assert!(
        picked.answer.contains("from the moment asked for"),
        "the answer says the distance, not only the frame: {}",
        picked.answer
    );
    assert!(picked.answer.contains("clip 3"), "and names what was asked for: {}", picked.answer);
    // The same words come out of the walk's own door, which has a moment and no clip number: one rule, two
    // doors, so neither can print the frame and leave the distance out.
    let said = picked_frame_answer(32.0, 34.0, "the moment the answer named");
    assert!(
        said.contains("from the moment asked for") && said.contains("2"),
        "both doors spell the answer the same way:\n{said}\n{}",
        picked.answer
    );
}

/// Entry 33: a frame that cannot be resolved is refused by name, and the instruction the person typed is not
/// thrown away over it.
#[test]
fn sec_12_decisions_narrate_and_produce_s33_an_unresolvable_frame_is_refused_and_leaves_the_instruction_alone() {
    // No clip 9: refused, and the refusal names the clip rather than picking "the first frame".
    let no_clip = naivepost::produce_upload::pick_frame(9, 0.0, &[0.0, 10.0], &[34.0]);
    assert_eq!(no_clip.err().as_deref(), Some("the cut has no clip 9"));
    // A real clip, no extracted frames at all: also refused, because a wrong picture on the video's page is
    // worse than no picture.
    let none = naivepost::produce_upload::pick_frame(1, 0.0, &[0.0], &[]);
    assert_eq!(none.err().as_deref(), Some("no frames have been extracted"));
    // And the walk does not lose the instruction over it: the log says the thumbnail is drawn instead, and
    // the record the walk hands on still carries what the person wrote.
    let legs = Legs::default();
    let publish = Publish {
        frames: vec!["frame:5.0".into(), "frame:15.0".into()],
        prompt: "brighten the last frame".into(),
        thumb_title: "A win".into(),
        title: "A win".into(),
        ..Default::default()
    };
    let answers = naivepost::produce_upload::Answers {
        title: None,
        instruction: None,
        negative: None,
        description: Some("the body".into()),
        picked_frame: Some(400.0),
    };
    let mut said: Vec<String> = Vec::new();
    let out = upload_walk(&legs, &publish, &answers, false, &mut |line| said.push(line.to_string()));
    assert!(
        said.iter().any(|line| line.contains("drawn instead")),
        "the failure is said, not swallowed: {said:?}"
    );
    let row = legs.published.borrow().clone().expect("the publish leg ran");
    assert_eq!(row.prompt, "brighten the last frame", "the instruction survives the failed frame");
    assert_eq!(row.thumb_title, "A win", "and so does the title");
    assert!(!row.own, "nothing was picked, so the thumbnail is still the drawn one");
    assert!(out.error.is_none(), "the run carries on to draw: {:?}", out.error);
}

/// Entry 34: a rewritten title reprints onto the picture every time the WORDS change — until the picture is
/// edited by hand, after which it is the person's and the app keeps its hands off it.
#[test]
fn sec_12_decisions_narrate_and_produce_s34_a_rewritten_title_reprints_until_the_picture_is_hand_edited() {
    let answers = naivepost::produce_upload::Answers {
        title: Some("A win nobody expected".into()),
        instruction: Some("brighten it".into()),
        negative: None,
        description: Some("the body".into()),
        picked_frame: None,
    };
    // First run: a title, so the title pass runs.
    let first = Publish { prompt: "brighten it".into(), ..Default::default() };
    let legs = Legs::default();
    let out = upload_walk(&legs, &first, &answers, false, &mut |_| {});
    assert!(out.reprinted, "the first title is printed: {:?}", out.calls);
    assert!(out.calls.iter().any(|c| c == "print-title"), "{:?}", out.calls);

    // Second run, same words: nothing to reprint. The pass is decided by whether the TEXT differs.
    let same = Publish {
        prompt: "brighten it".into(),
        thumb_title: "A win nobody expected".into(),
        title: "A win nobody expected".into(),
        frames: vec!["frame:5.0".into()],
        ..Default::default()
    };
    let legs = Legs::default();
    let out = upload_walk(&legs, &same, &answers, false, &mut |_| {});
    assert!(
        !out.calls.iter().any(|c| c == "print-title"),
        "the same words are not printed twice: {:?}",
        out.calls
    );

    // Third run: the words CHANGED, so the picture's words change with them — the prototype printed only the
    // first title that ever existed and left every rewrite off the thumbnail.
    let rewritten = Publish {
        thumb_title: "An old title".into(),
        title: "An old title".into(),
        prompt: "brighten it".into(),
        frames: vec!["frame:5.0".into()],
        ..Default::default()
    };
    let legs = Legs::default();
    let out = upload_walk(&legs, &rewritten, &answers, false, &mut |_| {});
    assert!(
        out.calls.iter().any(|c| c == "print-title"),
        "a rewrite reprints the title: {:?}",
        out.calls
    );

    // Fourth: the same rewrite, but the picture has been edited by hand. It is theirs now; the words stay.
    let legs = Legs::default();
    let out = upload_walk(&legs, &rewritten, &answers, true, &mut |_| {});
    assert!(
        !out.calls.iter().any(|c| c == "print-title"),
        "a hand-edited picture keeps its own words: {:?}",
        out.calls
    );
}

/// Entry 35: a title too long for its band is NOT refused — it prints and the answer says so — and a title
/// the band had to shrink gets that said too, so "four to seven words" stops being unenforced advice.
#[test]
fn sec_12_decisions_narrate_and_produce_s35_a_title_too_long_for_its_band_answers_with_the_fitted_size_or_the_warning() {
    let band = TitleBox { cx: 0.5, cy: 0.8, wf: 0.8, hf: 0.18 };
    let short = naivepost::produce_upload::set_title(&Publish::default(), "A win", &band, false);
    assert!(!short.overflows, "two words is inside the advice: {short:?}");
    assert!(
        short.answer.contains("words at") && !short.answer.contains("overflows"),
        "a title that fits just reports its size: {}",
        short.answer
    );

    let long_text = "a title with rather too many words to sit in one band";
    let long = naivepost::produce_upload::set_title(&Publish::default(), long_text, &band, false);
    assert!(long.overflows, "{} words is over the seven the prompt asks for", long.words);
    assert!(
        long.answer.contains("overflows the band"),
        "the warning is in the answer, never a silent overflow: {}",
        long.answer
    );
    assert!(long.shrunk, "and the size the band forced is what is reported: {long:?}");
    assert!(long.size > 0.0, "a title that overflows still prints at a real size: {long:?}");
}

/// Entry 36: a missing TITLE: or THUMBNAIL: keeps the previous run's value — deliberately (an empty box is
/// easier to notice than a wrong line) — and the log SAYS which value was kept rather than going quiet.
#[test]
fn sec_12_decisions_narrate_and_produce_s36_a_missing_title_or_instruction_says_it_kept_the_previous_value() {
    let missing = naivepost::produce_upload::finish_needs("", "");
    assert_eq!(missing, vec!["title", "description"]);
    let said = naivepost::produce_upload::kept_previous_log(&missing);
    assert!(
        said.contains("title") && said.contains("description") && said.contains("keeping"),
        "{said}"
    );

    // And the walk acts on it: no title in the answer, so the old title survives into the record AND the log
    // names it as kept — the difference from attributing last run's answer to this one.
    let legs = Legs::default();
    let publish = Publish {
        thumb_title: "Last run's title".into(),
        title: "Last run's title".into(),
        description: "last body".into(),
        prompt: "brighten it".into(),
        frames: vec!["frame:5.0".into()],
        ..Default::default()
    };
    let answers = naivepost::produce_upload::Answers {
        title: None,
        instruction: None,
        negative: None,
        // Empty on purpose: `finish` asks for a title AND a description, so this answer is missing one of
        // each — the title falls back to last run's, and the blank description is what makes #36's log fire.
        description: Some(String::new()),
        picked_frame: None,
    };
    let mut said: Vec<String> = Vec::new();
    let out = upload_walk(&legs, &publish, &answers, false, &mut |line| said.push(line.to_string()));
    let row = legs.published.borrow().clone().expect("the publish leg ran");
    assert_eq!(row.thumb_title, "Last run's title", "the title was kept");
    assert_eq!(row.description, "", "an empty description replaces, which is what makes it worth saying");
    assert!(
        said.iter()
            .any(|line| line.contains("no description") && line.contains("keeping what was there before")),
        "the log says what was kept: {said:?}"
    );
    assert!(out.published, "the kept values still land: {:?}", out.calls);
}

// ---- the translation side (entries 37, 38) --------------------------------------------------------

/// Entry 37: the placement tag never travels as a word, and the model's own line breaks are kept. The
/// prototype sent `{\an8}` into the translation and re-wrapped every answer at 42 characters on return,
/// contradicting its own prompt and moving a break the model had deliberately placed.
#[test]
fn sec_12_decisions_narrate_and_produce_s37_the_translation_keeps_the_placement_tag_out_of_the_text_and_answers_with_the_wrapped_line(
) {
    use naivepost::produce_subtitles as subs;
    let cue = subs::Cue { s: 0.0, e: 2.0, text: r"{\an8}Top of the frame".into(), pos: "top".into() };
    // Going out: the tag is not in the message, and a cue on two rows is one numbered entry.
    let sent = subs::sent_text(&cue);
    assert!(!sent.contains(r"{\an8}"), "the tag does not travel as a word: {sent:?}");
    assert_eq!(sent, "Top of the frame");
    let two_rows = subs::Cue { s: 0.0, e: 2.0, text: "first row\nsecond row".into(), pos: String::new() };
    assert_eq!(
        subs::sent_text(&two_rows),
        "first row / second row",
        "one cue is one numbered line, so a row break is folded for the trip"
    );
    // The numbered request the model is actually shown, then what it answers with: a tag it carried back,
    // and a break of its own choosing.
    let part = vec![(1usize, sent.clone())];
    let request = format!("{}\n{}", subs::message(1, "de"), subs::numbered(&part));
    assert!(request.contains("Top of the frame") && !request.contains(r"{\an8}"), "{request}");
    let answered = "  oben der\nRand";
    let echoed = format!(" {{\\an8}}{answered}");
    // Coming back: the app's own tag leads (a leading space would otherwise leave it in the middle), the
    // model's break survives, and `carried` says the model had echoed a tag of its own.
    let (text, carried) = subs::keep_placement(&cue, &echoed);
    assert!(carried, "a tag the model carried back is noticed, not trusted");
    assert!(text.starts_with(r"{\an8}"), "the ORIGINAL cue's placement leads: {text:?}");
    assert_eq!(text.matches(r"{\an8}").count(), 1, "exactly one tag in the caption: {text:?}");
    assert!(text.contains("oben der\nRand"), "the model's own break is kept: {text:?}");
    assert_eq!(subs::keep_breaks(answered), answered, "keeping breaks is not re-wrapping");
    // A cue with no placement keeps none, even if the model invented one.
    let plain_echoed = format!(" {}mitte", r"{\an8}");
    let (plain, _) = subs::keep_placement(
        &subs::Cue { s: 0.0, e: 2.0, text: "middle".into(), pos: String::new() },
        &plain_echoed,
    );
    assert_eq!(plain, "mitte", "a bottom cue stays a bottom cue: {plain:?}");
}

/// Entry 38: a line that cannot be translated ships in the original language — one bad line must not drop a
/// whole track — and it is NAMED, in the log and in what the walk hands back.
#[test]
fn sec_12_decisions_narrate_and_produce_s38_an_untranslatable_line_keeps_the_original_but_says_so() {
    use naivepost::produce_subtitles as subs;
    let cues = vec![
        subs::Cue { s: 0.0, e: 2.0, text: "first line".into(), pos: String::new() },
        subs::Cue { s: 2.0, e: 4.0, text: "a line the model will not answer".into(), pos: String::new() },
        subs::Cue { s: 4.0, e: 6.0, text: "third line".into(), pos: String::new() },
    ];
    let ticked = vec!["de".to_string()];
    // The model answers lines 1 and 3, and hands back an EMPTY line 2 — the shape a refusal takes on the
    // wire, and the shape `translate_line` refuses. Even the re-ask repeats the miss.
    let ask = |_request: &str, numbers: &[usize]| {
        let mut got = std::collections::BTreeMap::new();
        for number in numbers {
            if *number != 2 {
                got.insert(*number, format!("zeile {number}"));
            }
        }
        Ok(got)
    };
    let mut said: Vec<String> = Vec::new();
    let tracks = naivepost::produce_translate::track(&cues, "en", &ticked, &ask, |line| {
        said.push(line.to_string())
    });
    assert_eq!(tracks.len(), 1);
    let track = &tracks[0];
    assert_eq!(track.missing, vec![2], "the line that stayed behind is recorded by number");
    let warning = track.warning.clone().expect("and there is a warning about it");
    assert!(
        warning.contains("line 2") || warning.contains("(2)"),
        "the warning names the line that stayed behind: {warning}"
    );
    assert!(warning.contains("rest of the track is good"), "{warning}");
    assert!(
        said.iter().any(|line| line.contains("came back missing line 2")),
        "the re-ask is explained while it runs: {said:?}"
    );
    // The track SHIPPED: three cues, the good lines translated, the bad one still in the original language.
    assert_eq!(track.cues.len(), 3, "one bad line does not drop the track");
    assert_eq!(track.cues[0].text, "zeile 1");
    assert_eq!(track.cues[2].text, "zeile 3");
    assert_eq!(
        track.cues[1].text, "a line the model will not answer",
        "the untranslated line keeps its original text rather than vanishing"
    );
    assert!(!track.cached, "a track with a hole in it is never cached");
    // And the same rule read straight off the merge, which is what `finish`'s answer is built from.
    let merged = subs::merge("de", &[(1, "first line".into()), (2, "second line".into())], &{
        let mut m = std::collections::BTreeMap::new();
        m.insert(1, "erste zeile".to_string());
        m
    });
    assert_eq!(merged.lines[1], "second line");
    assert_eq!(merged.still_missing, vec![2]);
}
