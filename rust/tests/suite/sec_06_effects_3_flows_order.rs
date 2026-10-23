//! §06-effects.md **3. Flows** — the three post-cut passes as flows, in the two shapes their answer
//! reaches the app in.
//!
//! The section's own lead sentence is what this file pins: *"Prototype, for F3.9–F3.11: none of the tools
//! named there exists. All three passes get one JSON reply the app parses, as the cut pass does; … The tool
//! shapes below are the rewrite."* So each pass has TWO answer shapes and neither may be privileged:
//!
//! * the rewrite's tool round — `add_caption` / `set_clip_speed` / `add_effect` + `finish`, answered by
//!   [`naivepost::clip_rules_tools_leg`] against the very `Clips` batch whose rules §3.7 states;
//! * the one JSON reply the prototype got and the rewrite still accepts (`{"clips":[…]}`, `{"speeds":[…]}`,
//!   `{"fx":[…]}`), read by each pass's own parser.
//!
//! And they run AFTER the cut, in the order §3 lists them (captions F3.9, speeds F3.10, decorations F3.11),
//! only the ones the policy switched on — [`naivepost::cut_suggest_passes::order`], which `ui::cut_suggest_land`
//! calls from the walk-back so a suggested cut really does ask them.
//!
//! No window and no socket here: every rule below is a function over data. The wire halves live in
//! `f3_9_*`, `f3_10_*`, `f3_11_*` and `sec_06_effects_3_flows_wired.rs`.

use naivepost::clip_rules_tools_leg::{self, Run};
use naivepost::cut_suggest_passes;
use naivepost::policy::Pass;
use naivepost::project::{Field, Origin, Policy};
use naivepost::roles::Job;
use naivepost::tool_loop::{self, Turn};
use naivepost::tools::clips::Clips;
use naivepost::tools::Tool;
use naivepost::{captions_ask, cut_speed_pass, effects_ask, speeds_ask, tools};

use crate::common;

/// A policy with the three pass switches set as named. Everything else keeps its default.
fn policy(captions: bool, speeds: bool, decorations: bool) -> Policy {
    let field = |value: bool| Field {
        value,
        origin: Origin::Default,
        because: None,
    };
    Policy {
        captions_pass: field(captions),
        speed_pass: field(speeds),
        decorations_pass: field(decorations),
        ..Policy::default()
    }
}

/// A three-clip batch, each 10 s long and starting at zero: a caption is an OFFSET inside its clip (§F3.9),
/// so the session position does not matter to any rule below.
fn batch() -> Clips {
    clip_rules_tools_leg::batch_of(&[(1, 10.0), (2, 10.0), (3, 10.0)])
}

/// The three §3.7 job tools, exactly as the catalogue offers them.
fn offered() -> Vec<Tool> {
    tools::offered(Job::ClipRules)
}

/// Run one ask through the program's own loop with the answer already in hand — the shape all three cut
/// passes use, because their first request went out before the loop existed.
fn run_with(first: Turn, batch: &mut Clips) -> Run {
    clip_rules_tools_leg::run_over_batch_from(
        "captions",
        &first,
        Vec::new(),
        &offered(),
        batch,
        &mut |_, _| {
            Ok(Turn {
                content: String::new(),
                calls: vec![],
                finish: true,
            })
        },
    )
}

// ---- S1: the flow runs after the cut, in the spec's order, only where the policy says ------------------

/// §3 lists the passes as F3.9 captions, F3.10 speeds, F3.11 decorations, and F2.14's flowchart puts all
/// three between a `finish_cut` that passed and the walk-back. With every switch on, `order` answers exactly
/// those three, in that order.
#[test]
fn sec_06_effects_3_flows_s1_the_three_passes_run_after_the_cut_in_the_spec_order() {
    let all_on = policy(true, true, true);
    assert_eq!(
        cut_suggest_passes::order(&all_on),
        vec![Pass::Captions, Pass::Speeds, Pass::Decorations],
        "§3's own order: captions, then speeds, then decorations"
    );
    // Each pass is the one §3 numbers it as: the filter is keyed on the policy row, not on a list order.
    assert!(naivepost::policy::pass_runs(&all_on, Pass::Captions));
    assert!(naivepost::policy::pass_runs(&all_on, Pass::Speeds));
    assert!(naivepost::policy::pass_runs(&all_on, Pass::Decorations));
}

/// All eight switch combinations: a pass that is off is ABSENT from the order, never present-and-refused.
/// That is F2.14's sentence made testable — "a context that says 'no captions, no speed changes, no
/// effects' means those jobs are never called, not called and told to return nothing".
#[test]
fn sec_06_effects_3_flows_s1_a_pass_the_policy_switched_off_is_never_dialed() {
    for (captions, speeds, decorations) in all_threes() {
        let order = cut_suggest_passes::order(&policy(captions, speeds, decorations));
        let expected = [
            (captions, Pass::Captions),
            (speeds, Pass::Speeds),
            (decorations, Pass::Decorations),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, pass)| pass)
        .collect::<Vec<_>>();
        assert_eq!(
            order, expected,
            "switches ({captions}, {speeds}, {decorations}) must dial exactly {expected:?}"
        );
        // Every off pass is absent rather than present-and-empty: the count IS the number of asks.
        for (on, pass) in [
            (captions, Pass::Captions),
            (speeds, Pass::Speeds),
            (decorations, Pass::Decorations),
        ] {
            if !on {
                assert!(
                    !order.contains(&pass),
                    "{pass:?} is off but was ordered anyway"
                );
            }
        }
    }
    // Nothing on: an empty order, which `run_after_cut` reads as "nothing to ask" and logs NOTHING —
    // a zero-count line would claim a pass ran.
    assert!(cut_suggest_passes::order(&policy(false, false, false)).is_empty());
}

/// All eight (on, on, on) triples, in a fixed order so a failure names the triple.
fn all_threes() -> Vec<(bool, bool, bool)> {
    let mut out = Vec::new();
    for a in [true, false] {
        for b in [true, false] {
            for c in [true, false] {
                out.push((a, b, c));
            }
        }
    }
    out
}

/// The order filter, not the buttons' grey-out, is what makes "never called" true: the doors' own predicate
/// is a SEPARATE question (can this press run now?), so a page could still offer a press whose pass the
/// policy has switched off. `order` is the one that decides whether the endpoint is dialled at all.
#[test]
fn sec_06_effects_3_flows_s1_the_order_filter_is_not_the_buttons_grey_out() {
    let no_captions = policy(false, true, true);
    assert!(
        !cut_suggest_passes::order(&no_captions).contains(&Pass::Captions),
        "the captions pass is not asked, whatever the widget does"
    );
    // The door's predicate answers the OTHER question and stays consultable: asking it does not dial anything.
    assert!(!naivepost::policy::pass_runs(&no_captions, Pass::Captions));
    assert!(naivepost::policy::pass_runs(&no_captions, Pass::Speeds));
    // And the two remaining passes keep their relative order when the first is off — dropping one does not
    // renumber the others or reorder what is left.
    assert_eq!(
        cut_suggest_passes::order(&no_captions),
        vec![Pass::Speeds, Pass::Decorations]
    );
}

// ---- S2: one answer, two shapes ----------------------------------------------------------------------

/// §3: the passes get ONE JSON reply the app parses, AND the rewrite's tool shapes. Same caption, both
/// spellings, one verdict: the tool-call route writes it into the same `Clips` batch the prose parser reads
/// the same entry from, so the two cannot disagree about what was asked for.
#[test]
fn sec_06_effects_3_flows_s2_a_tool_call_round_and_one_json_reply_place_the_same_captions() {
    let args = r#"{"clip":1,"start":0.4,"end":2.2,"text":"a caption said once"}"#;
    // Shape 1 — the rewrite's tool round: add_caption, then finish.
    let mut via_tools = batch();
    let run = run_with(
        Turn {
            content: String::new(),
            calls: vec![
                common::call("add_caption", args),
                common::call("finish", "{}"),
            ],
            finish: false,
        },
        &mut via_tools,
    );
    assert!(
        !run.exhausted(),
        "an answer that called finish is a finished run"
    );
    let placed = via_tools.captions_of(1).to_vec();
    assert_eq!(placed.len(), 1, "the call placed one caption");
    assert_eq!(
        placed[0].span,
        (0.4, 2.2),
        "offsets inside the clip, as sent"
    );
    assert_eq!(placed[0].text, "a caption said once");

    // Shape 2 — the one JSON reply, the document the prompt specifies.
    let prose =
        r#"{"clips":[{"i":1,"fx":[{"start":0.4,"end":2.2,"text":"a caption said once"}]}]}"#;
    let calls = captions_ask::parse_captions(prose).expect("the prose document parses");
    assert_eq!(calls.len(), 1);
    let mut via_prose = batch();
    let read = captions_ask::call_of_args(&serde_json::json!({
        "clip": calls[0].clip, "start": calls[0].start, "end": calls[0].end, "text": calls[0].text
    }))
    .expect("the parsed entry reads back as a call");
    via_prose.add_caption(read.clip, read.start, read.end, &read.text);
    assert_eq!(
        via_prose.captions_of(1).to_vec(),
        placed,
        "one answer, two shapes: the caption is identical either way"
    );
    // And the tool call's own arguments parse through the SAME reader the prose path ends in.
    let direct =
        captions_ask::call_of_args(&serde_json::from_str::<serde_json::Value>(args).unwrap())
            .expect("the call's args read as a caption");
    assert_eq!(
        (direct.clip, direct.start, direct.end, direct.text.as_str()),
        (1, 0.4, 2.2, "a caption said once")
    );
}

/// The speed and decoration tools answer their own work through the dispatcher, and the malformed call is
/// an ANSWER the model can correct itself from (`{"error": …}`), never a silent no-op.
#[test]
fn sec_06_effects_3_flows_s2_the_speed_and_decoration_tools_answer_their_own_work() {
    // set_clip_speed: tool shape vs the {"speeds":[…]} prose document.
    let mut via_tools = batch();
    run_with(
        Turn {
            content: String::new(),
            calls: vec![
                common::call("set_clip_speed", r#"{"clip":2,"rate":2.0}"#),
                common::call("finish", "{}"),
            ],
            finish: false,
        },
        &mut via_tools,
    );
    let speeds = speeds_ask::parse_speeds(r#"{"speeds":[{"clip":2,"rate":2.0}]}"#);
    assert_eq!(speeds.len(), 1, "the prose shape parses one rate");
    assert_eq!(speeds[0], cut_speed_pass::Call { clip: 2, rate: 2.0 });
    // Both shapes leave the batch holding the same applied rate on the same clip.
    let mut via_prose = batch();
    via_prose.set_clip_speed(speeds[0].clip, speeds[0].rate);
    let applied = |clips: &Clips| {
        clips
            .clips()
            .iter()
            .map(|c| (c.n, c.rate))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        applied(&via_prose),
        applied(&via_tools),
        "one rate, two shapes, one batch state"
    );

    // add_effect: tool shape vs the {"fx":[…]} prose document.
    let mut fx_tools = batch();
    run_with(
        Turn {
            content: String::new(),
            calls: vec![
                common::call(
                    "add_effect",
                    r#"{"clip":1,"kind":"zoom","start":1.0,"end":3.0}"#,
                ),
                common::call(
                    "add_effect",
                    r#"{"clip":3,"kind":"volume","start":0.0,"end":4.0,"gain":0.5}"#,
                ),
                common::call("finish", "{}"),
            ],
            finish: false,
        },
        &mut fx_tools,
    );
    let prose_fx = r#"{"fx":[{"clip":1,"kind":"zoom","start":1.0,"end":3.0},{"clip":3,"kind":"volume","start":0.0,"end":4.0,"gain":0.5}]}"#;
    let calls = effects_ask::parse_effects(prose_fx);
    assert_eq!(calls.len(), 2, "both prose entries parse");
    let direct = effects_ask::call_of_args(
        &serde_json::from_str::<serde_json::Value>(
            r#"{"clip":1,"kind":"zoom","start":1.0,"end":3.0}"#,
        )
        .unwrap(),
    )
    .expect("the call's args read as a decoration");
    assert_eq!(
        direct, calls[0],
        "the tool shape and the prose shape decide one way"
    );

    // A call with no arguments is refused in words, not dropped: §2 S6 sends this text back to the model.
    for bare in ["add_caption", "set_clip_speed", "add_effect"] {
        let answer = clip_rules_tools_leg::answer(&common::call(bare, "{}"), &mut batch());
        assert!(
            answer.starts_with("{\"error\""),
            "{bare} with no arguments must answer with the tool's error shape: {answer}"
        );
    }
    // An unknown tool name is likewise an answer, not a crash.
    let unknown =
        clip_rules_tools_leg::answer(&common::call("paint_the_picture", "{}"), &mut batch());
    assert!(
        unknown.contains("is not one of this job's tools"),
        "{unknown}"
    );
}

/// The round bookkeeping the leg module exists for: `>>> <step>: round N of M — …` lines come from a real
/// loop, and an answer that parsed needs no second ask. This is what kept `>>> captions: round 1 of 8 — …`
/// unprintable before the loop existed.
#[test]
fn sec_06_effects_3_flows_s2_a_prose_answer_is_not_asked_again_as_a_tool_round() {
    // First turn already in hand, answering in prose AND calling finish: the whole answer in one request,
    // so the loop must NOT dial again. (A prose answer with no `finish` is not complete, and §2's own rule
    // then nudges for it — pinned by the second half of this test.)
    let asked = std::cell::Cell::new(0usize);
    let mut clips = batch();
    let run = clip_rules_tools_leg::run_over_batch_from(
        "captions",
        &Turn {
            content: r#"{"clips":[]}"#.to_string(),
            calls: vec![common::call("finish", "{}")],
            finish: true,
        },
        Vec::new(),
        &offered(),
        &mut clips,
        &mut |_, _| {
            asked.set(asked.get() + 1);
            Ok(tool_loop::Turn {
                content: String::new(),
                calls: vec![],
                finish: true,
            })
        },
    );
    assert_eq!(
        asked.get(),
        0,
        "a finished prose answer is the whole answer: no second request went out"
    );
    assert_eq!(run.rounds.len(), 1, "one request, one §7 section");
    assert_eq!(
        run.job_calls(&offered())
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>(),
        vec!["finish".to_string()],
        "the only call was `finish`, which is the end of the flow, not the pass's work"
    );

    // A prose answer that never called `finish` is incomplete, and §2 answers it with ONE nudge rather than
    // a re-ask of the question: exactly one further round, and the conversation still ends on the answer.
    let mut nudged = 0usize;
    let mut nudged_batch = batch();
    let nudge_run = clip_rules_tools_leg::run_over_batch_from(
        "captions",
        &Turn {
            content: r#"{"clips":[]}"#.to_string(),
            calls: vec![],
            finish: false,
        },
        Vec::new(),
        &offered(),
        &mut nudged_batch,
        &mut |_, _| {
            nudged += 1;
            Ok(Turn {
                content: String::new(),
                calls: vec![common::call("finish", "{}")],
                finish: true,
            })
        },
    );
    assert_eq!(
        nudged, 1,
        "an unfinished prose answer costs one nudge round, no more: {nudged}"
    );
    assert!(
        nudge_run
            .run
            .logs
            .iter()
            .any(|line| line.contains("Call finish when you are done")),
        "the nudge is said, not silently assumed: {:?}",
        nudge_run.run.logs
    );

    // A round that DID call a tool costs exactly one further ask (the nudge or the next answer), and the
    // round line is said for the round that called.
    let mut asked2 = 0usize;
    let mut fx_batch = batch();
    let run2 = clip_rules_tools_leg::run_over_batch_from(
        "captions",
        &Turn {
            content: String::new(),
            calls: vec![common::call(
                "add_caption",
                r#"{"clip":1,"start":0.5,"end":3.0,"text":"one"}"#,
            )],
            finish: false,
        },
        Vec::new(),
        &offered(),
        &mut fx_batch,
        &mut |_, _| {
            asked2 += 1;
            Ok(Turn {
                content: String::new(),
                calls: vec![],
                finish: true,
            })
        },
    );
    assert_eq!(asked2, 1, "the call needed one more round to be finished");
    assert!(
        run2.run.logs.iter().any(|line| line.contains("round 1 of")),
        "the round that called is logged with its number: {:?}",
        run2.run.logs
    );
    assert_eq!(
        fx_batch.captions_of(1).len(),
        1,
        "and its caption is in the batch"
    );
}

/// The three passes share ONE answering door, which is the reason a call and a prose answer cannot drift:
/// `answer_calls` routes through the same `Clips` methods the loop does.
#[test]
fn sec_06_effects_3_flows_s2_one_door_answers_both_shapes() {
    let calls = vec![common::call(
        "add_caption",
        r#"{"clip":1,"start":0.4,"end":2.2,"text":"same door"}"#,
    )];
    let notes = clip_rules_tools_leg::answer_calls(&calls, &mut batch());
    assert_eq!(notes.len(), 1, "one note per call, in order");
    assert!(
        notes[0].contains("add_caption"),
        "the note names the call: {notes:?}"
    );
    // A call for a clip outside the batch is the §3.1 error naming the batch size — the sentence §F3.9's
    // retry path feeds back to the model.
    let outside = clip_rules_tools_leg::answer(
        &common::call(
            "add_caption",
            r#"{"clip":9,"start":0.0,"end":2.0,"text":"x"}"#,
        ),
        &mut batch(),
    );
    assert!(outside.contains("clip 9 is not in this batch"), "{outside}");
    assert!(
        outside.contains("3 clip(s)"),
        "and it says how big the batch was: {outside}"
    );
}
