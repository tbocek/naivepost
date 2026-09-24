//! §12-decisions#1-the-five-homes-of-a-decision — the audit table of `spec/12-decisions.md` §1.
//!
//! Directive B asks which home each decision lives in, and the homes differ by **what the model is
//! told**, not by where the code runs. So every test below takes its evidence from the live path
//! that makes the decision — a real tool reply, a real `finish_cut`, a real [`place_edges`] call —
//! rather than from the prose. A label that stops matching the behaviour fails here instead of
//! drifting away from it unnoticed.

use naivepost::decision_audit::{self as audit, Home};
use naivepost::decision_homes;
use naivepost::edges::{self as edge, AlignedWord};
use naivepost::params;
use naivepost::prompts;
use naivepost::textfmt::Retake;
use naivepost::tools::cutpass::{self, Plan};
use naivepost::tools::retakes::{Line, Marks, Trim};

/// A twelve-minute session cut to six: room enough for four ninety-second segments inside the window.
const SESSION: f64 = 720.0;
const TARGET: f64 = 360.0;

fn json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).expect("a tool always answers JSON")
}

/// The error text of a refusal that must be one.
fn error(reply: &str) -> String {
    json(reply)["error"]
        .as_str()
        .expect("a refusal is an error, never a silent ok")
        .to_string()
}

fn num(value: &serde_json::Value, key: &str) -> f64 {
    value[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{key} is not a number in {value}"))
}

fn a_plan() -> Plan {
    Plan::new(SESSION, TARGET)
}

fn word(text: &str, s: f64, e: f64) -> AlignedWord {
    AlignedWord {
        word: text.into(),
        s,
        e,
    }
}

/// No envelope anywhere on the clock.
fn no_envelope(_: f64) -> Option<&'static edge::Edges> {
    None
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s1_there_are_exactly_five_and_each_is_named_as_spec_does() {
    let labels: Vec<&str> = Home::all().iter().map(|h| h.label()).collect();
    assert_eq!(
        labels,
        [
            "tool argument",
            "tool result",
            "`finish` answer",
            "the walk-back",
            "policy / prompt"
        ],
        "{labels:?}"
    );
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s2_only_two_homes_let_the_model_act_in_the_same_conversation()
{
    // §1 marks two cells with "may act on it" / "may fix it"; the other three cannot.
    let acting: Vec<Home> = Home::all()
        .into_iter()
        .filter(|h| h.may_act_in_same_conversation())
        .collect();
    assert_eq!(acting, vec![Home::ToolResult, Home::FinishAnswer]);

    // Three of the five are seen. The walk-back is unseen by design; policy/prompt is unseen because it
    // is fixed before the conversation starts, not withheld from it.
    let unseen: Vec<Home> = Home::all()
        .into_iter()
        .filter(|h| !h.model_sees_it())
        .collect();
    assert_eq!(unseen, vec![Home::WalkBack, Home::PolicyOrPrompt]);

    // Seen and actionable are different questions: a tool argument is seen but is the model's own
    // statement, so there is nothing in it for the model to react to.
    assert!(Home::ToolArgument.model_sees_it());
    assert!(!Home::ToolArgument.may_act_in_same_conversation());
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s3_scope_splits_per_item_from_whole_answer() {
    let scopes: Vec<&str> = Home::all().iter().map(|h| h.scope()).collect();
    assert_eq!(
        scopes,
        [
            "one item",
            "one item",
            "the whole answer",
            "after the answer",
            "no judgement at all"
        ],
        "{scopes:?}"
    );
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s4_the_walk_back_admits_only_mechanical_explainable_visible() {
    // All three hold and the walk-back will take it. Any one missing and the change would be invisible
    // to the model and unaccountable afterwards, which is what S9 shows a real walk-back avoiding.
    assert!(audit::walk_back_admits(true, true, true));
    assert!(!audit::walk_back_admits(false, true, true), "not mechanical");
    assert!(
        !audit::walk_back_admits(true, false, true),
        "not explainable in one sentence"
    );
    assert!(
        !audit::walk_back_admits(true, true, false),
        "not visible on screen afterwards"
    );
    assert!(!audit::walk_back_admits(false, false, false));
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s5_announcement_follows_the_rule_of_thumb() {
    // Per-item changes travel in the tool's answer; whole-cut changes at finish. Nothing else announces.
    assert!(audit::change_is_announced(Home::ToolResult));
    assert!(audit::change_is_announced(Home::FinishAnswer));
    assert!(!audit::change_is_announced(Home::ToolArgument));
    assert!(!audit::change_is_announced(Home::WalkBack));
    assert!(!audit::change_is_announced(Home::PolicyOrPrompt));
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s6_tool_argument_example_lines_stated_seconds_returned() {
    // §1's first example: "which lines are an abandoned attempt". The model states line numbers — it
    // is never asked for a timestamp it would have to compute — and the tool answers in seconds.
    let mut marks = Marks::new(
        vec![
            Line { n: 1, start: 0.0, end: 2.0 },
            Line { n: 2, start: 180.0, end: 182.0 },
            Line { n: 3, start: 400.0, end: 402.0 },
        ],
        6.0,
    );

    let replied = json(&marks.mark_abandoned(2, 2, None, Trim::none()));
    assert!(replied["error"].is_null(), "{replied}");
    // What was stated comes back as the range it names; what it costs comes back as seconds, plus the
    // running share against the ceiling — both reported, neither asked for.
    assert_eq!(replied["lines"], serde_json::json!([2, 2]));
    assert!((num(&replied, "seconds") - 2.0).abs() < 1e-9);
    assert!((num(&replied, "ceiling") - 0.4).abs() < 1e-9);

    let more = json(&marks.mark_abandoned(3, 3, None, Trim::none()));
    assert_eq!(more["lines"], serde_json::json!([3, 3]));
    assert!(
        num(&more, "speech_marked") > num(&replied, "speech_marked"),
        "the share grows with each mark"
    );

    // A line that does not exist is refused outright: the model may state only what is there.
    let bad = error(&marks.mark_abandoned(9, 9, None, Trim::none()));
    assert!(bad.contains("line"), "{bad}");
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s7_tool_result_example_snapped_edges_and_how_far_they_moved()
{
    // §1's second example: "a segment's snapped edges and how far they moved", within
    // P.policy.snapToleranceSeconds. Both numbers arrive unprompted in the same reply.
    let tolerance = cutpass::SNAP_TOLERANCE_SECONDS;
    // The id §1 cites is catalogued in §6's list, not Prepare's — `params::find` answers the latter only.
    let row = params::cut()
        .into_iter()
        .find(|row| row.id == "P.policy.snapToleranceSeconds")
        .expect("catalogued");
    assert_eq!(row.from, "tools::cutpass::SNAP_TOLERANCE_SECONDS");

    let mut plan = a_plan().with_snap_points(vec![100.0, 200.0]);
    let replied = json(&plan.add_segment(100.2, 190.0, "just after the visual cut"));
    assert!(replied["error"].is_null(), "{replied}");

    let span = &replied["span"];
    assert_eq!(span[0].as_f64().unwrap(), 100.0, "snapped onto the allowed point");
    let moved = &replied["moved"];
    assert!(
        (moved[0].as_f64().unwrap() + 0.2).abs() < 1e-6,
        "left edge moved 0.2 s onto the point: {replied}"
    );
    // The movement is bounded by the parameter §1 cites, and the right edge had nowhere to go.
    assert!(moved[0].as_f64().unwrap().abs() <= tolerance);
    assert!((moved[1].as_f64().unwrap()).abs() < 1e-9, "right edge unmoved: {replied}");
    // And the same reply carries the whole-cut context, since the model is choosing a set one at a time.
    assert!(num(&replied, "footage") > 0.0);
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s8_finish_answer_examples_complain_about_the_whole() {
    // §1's third home: "is told what is still wrong with the whole; may fix it". Both examples come
    // through finish_cut, whose complaints count over everything added rather than one item.
    let too_few = error(&a_plan().finish_cut());
    assert!(too_few.contains("0 segments is under the"), "{too_few}");
    assert!(too_few.contains("add more"), "fixable, not fatal: {too_few}");

    // Reached through the named helper, with the bound taken from the status rather than restated.
    let status = a_plan().status();
    assert_eq!(status.min_segments, cutpass::min_segments(TARGET));
    let problems = cutpass::cut_problems(&status);
    assert!(problems.iter().any(|p| p.starts_with("0 segments")), "{problems:?}");

    // Narrate's side of the same home: a clip nobody answered. It names the clip and echoes its bounds,
    // which is what makes the fault repairable in the same conversation.
    let segs = vec![
        naivepost::cut::Seg { s: 0.0, e: 10.0, ..Default::default() },
        naivepost::cut::Seg { s: 10.0, e: 25.0, ..Default::default() },
    ];
    let fault = naivepost::narrate_reply::clip_without_entry(1, &segs);
    assert!(fault.contains("clip 2"), "{fault}");
    assert!(fault.contains("10.0-25.0"), "bounds echoed: {fault}");
    assert!(fault.contains("got no entry"), "{fault}");
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s9_walk_back_example_placement_moves_marks_unannounced() {
    // §1's fourth home: "never sees it". place_edges rewrites the marks it is handed and returns the
    // rewritten set plus notes. Those notes go into the human transcript (§05), not back into the
    // conversation that made the mark — which is why the walk-back demands mechanical + explainable +
    // visible before anything may live here.
    let words = vec![word("one", 1.0, 1.4), word("two", 5.0, 5.4)];
    let mark = Retake {
        s: 1.0,
        e: 4.0,
        again: 5.0,
        to: 4.0,
        text: String::new(),
        whole: String::new(),
    };
    let (placed, notes) = edge::place_edges(vec![mark.clone()], &words, no_envelope, &[]);

    assert_eq!(placed.len(), 1);
    // Placement only ever widens toward the retake; it never invents a new mark or drops one.
    assert!(placed[0].to >= mark.to);
    assert_eq!(placed[0].s, mark.s);
    // Every note is one line addressed to a human reader, not a question to the model.
    for note in &notes {
        assert!(note.starts_with(">>> "), "a transcript note: {note}");
        assert!(!note.contains('\n'), "one sentence: {note}");
        assert!(!note.contains('?'), "not a request: {note}");
    }
    // And the home itself says so: nothing changed about what the model said, so nothing is announced.
    assert!(!audit::change_is_announced(Home::WalkBack));
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s10_policy_prompt_examples_are_one_number_and_one_wording()
{
    // The 40 % ceiling on marks: one number, held once under its P. id, applied to every item alike.
    assert_eq!(audit::examples()[5].home, Home::PolicyOrPrompt);
    assert_eq!(audit::examples()[5].what, "the 40 % ceiling on marks");
    assert_eq!(
        audit::examples()[5].lives_in,
        "tools::retakes::RETAKE_CEIL = P.machine.retakeCeil"
    );
    let row = params::find("P.machine.retakeCeil").expect("catalogued");
    assert_eq!(row.spelled, "0.4");
    assert_eq!(row.from, "tools::retakes::RETAKE_CEIL");
    // Its directive-A home is Machine, and §1 does NOT count Machine as its fifth home: the ceiling
    // bounds what the servers allow, it is not a per-item judgement either way.
    assert_eq!(decision_homes::home_of("P.machine.retakeCeil"), Some(decision_homes::Home::Machine));
    assert!(!audit::is_number_or_wording(decision_homes::Home::Machine));

    // "clean it like a subtitler": one wording, known to prompts::is_known, owned by Prompt — and that
    // pairing is exactly what §1 means by the fifth home.
    assert!(prompts::is_known("captions"));
    assert_eq!(decision_homes::home_of("captions"), Some(decision_homes::Home::Prompt));
    assert!(audit::is_number_or_wording(decision_homes::Home::Prompt));
    assert_eq!(audit::examples()[6].lives_in, "spec/prompts/captions.md");
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s11_the_table_covers_every_home_and_no_row_contradicts_its_own_rule()
{
    let table = audit::examples();
    // Seven examples, four of them paired, covering all five homes.
    assert_eq!(table.len(), 7);
    let count = |home: Home| table.iter().filter(|r| r.home == home).count();
    assert_eq!(count(Home::ToolArgument), 1);
    assert_eq!(count(Home::ToolResult), 1);
    assert_eq!(count(Home::FinishAnswer), 2);
    assert_eq!(count(Home::WalkBack), 1);
    assert_eq!(count(Home::PolicyOrPrompt), 2);

    for row in &table {
        assert!(!row.what.is_empty() && !row.lives_in.is_empty(), "{row:?}");
        // No row sits under a home whose announcement rule contradicts it.
        match row.home {
            Home::ToolResult | Home::FinishAnswer => assert!(audit::change_is_announced(row.home), "{row:?}"),
            _ => assert!(!audit::change_is_announced(row.home), "{row:?}"),
        }
    }
}

#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s12_is_number_or_wording_spans_policy_and_prompt_only() {
    // §1's fifth home spans two of directive A's four, and only those two.
    assert!(audit::is_number_or_wording(decision_homes::Home::Policy));
    assert!(audit::is_number_or_wording(decision_homes::Home::Prompt));
    assert!(!audit::is_number_or_wording(decision_homes::Home::Machine));
    assert!(!audit::is_number_or_wording(decision_homes::Home::Engineering));

    // Reached through real ids rather than the enum alone: a policy number yes, a machine limit no,
    // an engineering constant no, a prompt wording yes.
    let snap = decision_homes::home_of("P.policy.snapToleranceSeconds").expect("routed");
    assert!(audit::is_number_or_wording(snap));
    let tokens = decision_homes::home_of("P.machine.llmMaxTokens").expect("routed");
    assert!(!audit::is_number_or_wording(tokens));
    let pulse = decision_homes::home_of("machine.pulse").expect("routed");
    assert!(!audit::is_number_or_wording(pulse));
    let narrate = decision_homes::home_of("narrate").expect("routed");
    assert!(audit::is_number_or_wording(narrate));
}
