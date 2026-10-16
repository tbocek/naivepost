//! §12-decisions#1 — the five homes of a decision, driven by the program.
//!
//! `tests/decision_audit.rs` pins the table's predicates. That alone proves nothing about a running
//! naivepost: §1 is a claim about what happens on the wire and at the doors that change a model's answer,
//! so each home is exercised here through the code that owns it —
//! [`naivepost::tool_loop`] for the two hearing homes (a tool result per item, `finish`'s verdict on the
//! whole), [`naivepost::suggest::walk_back_audit`] for the walk-back the model never sees, and
//! [`naivepost::ui::policy_form`] for the fifth home, where a number or a wording lives instead of a
//! judgement. Rule of thumb under all of them: *if the app changes what the model said, the model hears
//! about it* — and the one place it does not, the walk-back, is only allowed to be invisible because the
//! audit says it is mechanical, explainable and visible.
//!
//! One application, one `connect_activate`, one `#[test]` per window case gated by an `AtomicBool` — the
//! shape `tests/sec_11_flow_index_4_model_calls_at_a_glance.rs` uses, because every window shares process
//! state (the log, this thread's session). The loop cases need no window at all: they drive the same
//! functions the live call drives, with the scripted-turn closures `tests/f2_14_model_mode.rs` uses, so
//! nothing here reaches a socket or waits on a clock.

#![allow(dead_code)]

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut_effect_decisions::Candidate;
use naivepost::decision_audit::{self, Home};
use naivepost::decision_homes;
use naivepost::exchanges::ToolCall;
use naivepost::project::Origin;
use naivepost::settings::{self, Conf};
use naivepost::tool_loop::{self, Turn};
use naivepost::tools::{self, cutpass, Tool};
use naivepost::ui::{self, policy_form};
use naivepost::suggest;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, pin_private_cwd, settle};

static RAN_FORM: AtomicBool = AtomicBool::new(false);

// ---- the loop harness ------------------------------------------------------------------

/// A round's calls in the shape a real server answers with. `finish_cut` sets `finish` because that is
/// the job's own end tool, exactly as the live reply parser decides it.
fn turn(calls: Vec<(&str, &str)>) -> Turn {
    let parsed: Vec<ToolCall> = calls
        .into_iter()
        .map(|(name, args)| ToolCall {
            name: name.to_string(),
            args: args.to_string(),
            result: String::new(),
        })
        .collect();
    Turn {
        content: String::new(),
        finish: parsed.iter().any(|call| call.name == "finish_cut"),
        calls: parsed,
    }
}

/// What the loop sent back to the model after its calls ran: every `tool` message's text, EXCLUDING the
/// trailing rounds this harness keeps feeding (a run that went quiet was answered by `cut_status`, whose
/// reading is not an announcement about a change). Callers take what they need from the order.
fn fed_back(run: &tool_loop::Run) -> Vec<String> {
    run.messages
        .iter()
        .filter(|message| message.role == "tool")
        .flat_map(|message| message.parts.iter())
        .map(|part| match part {
            naivepost::exchanges::Part::Text(text) => text.clone(),
            naivepost::exchanges::Part::Image(_) => String::new(),
        })
        .collect()
}

/// A plan with footage to cut and word edges to snap onto, so `add_segment` really moves an edge and the
/// result has something to report about the move.
fn plan() -> cutpass::Plan {
    cutpass::Plan::new(600.0, 300.0).with_snap_points(vec![10.4, 19.6])
}

fn offered() -> Vec<Tool> {
    tools::offered(naivepost::roles::Job::ModelCut)
}

/// Run the loop over scripted turns, answering each call the way the live cut step does
/// (`cut_suggest_model_run::answer_call` → `cutpass::Plan`). No socket, no clock: the model is a closure
/// and the tools are the real ones.
fn loop_over(turns: Vec<Turn>) -> tool_loop::Run {
    let queue = Rc::new(RefCell::new(turns.into_iter()));
    let mut plan = plan();
    tool_loop::run(
        "cut",
        Vec::new(),
        &offered(),
        {
            let queue = queue.clone();
            move |_messages, _tools| {
                Ok(queue
                    .borrow_mut()
                    .next()
                    .unwrap_or_else(|| turn(vec![("cut_status", "{}")])))
            }
        },
        |call| ui::cut_suggest_model_run::answer_call(&mut plan, call),
    )
}

// ---- s1: tool argument --------------------------------------------------------------

/// Row 1: the model *states* the decision — which seconds to keep, in its own arguments — and gets nothing
/// back telling it what its statement meant, because a tool argument is not a home it may act in.
#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s1_tool_argument_home_states_it() {
    assert!(
        !Home::ToolArgument.may_act_in_same_conversation(),
        "an argument is the model's own statement: there is nothing in it for the model to react to"
    );
    assert_eq!(Home::ToolArgument.scope(), "one item");
    assert!(Home::ToolArgument.model_sees_it(), "it is the model's own words");

    // The arguments the program runs ARE the model's: `add_segment` receives the seconds it asked with,
    // unedited, and the segment it makes carries the model's reason verbatim.
    let mut plan = plan();
    let answer = ui::cut_suggest_model_run::answer_call(
        &mut plan,
        &ToolCall {
            name: "add_segment".to_string(),
            args: r#"{"start": 10.0, "end": 20.0, "why": "the intro"}"#.to_string(),
            result: String::new(),
        },
    );
    // An accepted call answers with the JSON `ok` body; a refused one wraps it in `{"error": …}`. Either way
    // the point here is what landed in the plan: the model's own seconds and its own words.
    assert!(
        !answer.contains("\"error\""),
        "an error would mean the arguments were refused: {answer}"
    );
    let kept = &plan.segments()[0];
    assert_eq!(kept.why, "the intro", "the model's own words travel into the plan");
    assert!(
        (kept.start - 10.0).abs() <= cutpass::SNAP_TOLERANCE_SECONDS
            && (kept.end - 20.0).abs() <= cutpass::SNAP_TOLERANCE_SECONDS,
        "the seconds it named are the seconds it got, within the bound: {:?}",
        (kept.start, kept.end)
    );

    // And a run whose only round was an argument carries no announcement of its own: the news arrives as
    // the RESULT (row 2), never as a note about the argument.
    let run = loop_over(vec![turn(vec![("cut_status", "{}")]), turn(vec![])]);
    assert!(
        !run.announced.contains(&Home::ToolArgument.label()),
        "nothing announces a tool argument: {:?}",
        run.announced
    );
}

// ---- s2: tool result -----------------------------------------------------------------

/// Row 2: the app snapped the model's edges, so the snapped span and how far each edge moved go back to it
/// in the tool's own answer, per item — and the loop records that this is the home it spoke from.
#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s2_tool_result_home_announces_what_the_app_made_of_it() {
    assert!(decision_audit::change_is_announced(Home::ToolResult));
    assert!(Home::ToolResult.may_act_in_same_conversation());

    // 10.0 / 20.0 asked; the snap points sit at 10.4 and 19.6, so both edges move and the result must say so.
    let run = loop_over(vec![
        turn(vec![("add_segment", r#"{"start": 10.0, "end": 20.0}"#)]),
        turn(vec![]),
    ]);
    let answers = fed_back(&run);
    // The first answer is the one about the segment the model just asked for; later rounds answered
    // `cut_status`/`finish_cut`, which report the plan rather than a change to one item.
    assert!(
        answers.iter().any(|answer| answer.contains("\"span\"") && answer.contains("\"moved\"")),
        "the snapped span and the moves went back to the model: {answers:?}"
    );
    let told: serde_json::Value = serde_json::from_str(&answers[0]).expect("a tool answer is JSON");
    assert_eq!(
        told["span"].as_array().expect("span").len(),
        2,
        "the edges as placed: {told}"
    );
    let span = told["span"].as_array().unwrap();
    assert_eq!(span[0].as_f64(), Some(10.4), "the start snapped to the word edge");
    assert_eq!(span[1].as_f64(), Some(19.6), "the end snapped to the word edge");
    let moved = told["moved"].as_array().expect("moved");
    assert!(
        (moved[0].as_f64().unwrap() - 0.4).abs() < 1e-9
            && (moved[1].as_f64().unwrap() + 0.4).abs() < 1e-9,
        "how far each edge moved is on the wire: {moved:?}"
    );
    assert!(
        run.announced.contains(&Home::ToolResult.label()),
        "the loop records that it announced through the tool-result home: {:?}",
        run.announced
    );
}

// ---- s3: `finish` answer -------------------------------------------------------------

/// Row 3: at `finish` the model is told what is still wrong with the WHOLE and may fix it in the same
/// conversation — here a cut under the target's floor, refused with the complaint on the wire.
#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s3_finish_answer_home_says_whats_still_wrong() {
    assert!(decision_audit::change_is_announced(Home::FinishAnswer));
    assert_eq!(Home::FinishAnswer.scope(), "the whole answer");

    // One short segment, then `finish_cut`: too little footage and too few segments is a verdict about the
    // set, not about any one clip, so §1 puts it here rather than in the per-item row.
    let run = loop_over(vec![
        turn(vec![("add_segment", r#"{"start": 10.0, "end": 20.0}"#)]),
        turn(vec![("finish_cut", "{}")]),
    ]);
    let answers = fed_back(&run);
    let verdict = answers.last().expect("the finish answer");
    let told: serde_json::Value = serde_json::from_str(verdict).expect("a tool answer is JSON");
    let why = told["error"].as_str().unwrap_or_default();
    assert!(
        !why.is_empty(),
        "a refused finish must say what is still wrong: {verdict}"
    );
    assert!(
        why.contains("segments") || why.contains("kept"),
        "the complaint is about the whole cut: {why}"
    );
    assert_eq!(
        run.outcome,
        tool_loop::Outcome::Finished { asked_finish: false },
        "the model called finish itself, so this run did not need asking"
    );
    assert!(
        run.announced.contains(&Home::FinishAnswer.label()),
        "the loop records that the whole-cut verdict went back: {:?}",
        run.announced
    );
}

// ---- s4: after `finish` — the walk-back -----------------------------------------------

/// Row 4: the walk-back is the one change the model never sees, so it is admitted only when the audit can
/// say it is mechanical, explainable in one sentence and visible afterwards — and the budget door says out
/// loud which home its silence came from.
#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s4_walk_back_is_audited_and_never_shown() {
    assert!(!Home::WalkBack.model_sees_it(), "this is the row that is deliberately unseen");
    assert!(!decision_audit::change_is_announced(Home::WalkBack));

    // Admitted: the walk places from real word edges, so every move is inside the bound and the result is
    // on the lane afterwards.
    let walked = suggest::Walk {
        talked: vec![(10.4, 19.6)],
        silences: vec![(30.0, 40.0)],
        snap_points: vec![(10.4, Candidate::WordEdge), (19.6, Candidate::WordEdge)],
        marks: vec![],
    };
    assert_eq!(
        suggest::walk_back_audit(&walked),
        Ok(Home::WalkBack.label()),
        "a bounded, stated, visible walk-back may run unseen"
    );
    // And it really runs: `apply` walks the same input and reports the pair §S3 spells.
    let applied = suggest::apply(&[(10.0, 20.0)], &[], &[], &walked);
    assert_eq!(applied.log.len(), 2, "{:?}", applied.log);
    assert!(applied.log[0].starts_with(">>> suggested"), "{:?}", applied.log);

    // Refused: with nothing to place from, nothing was placed mechanically and nothing is left to see, so
    // the change would be invisible AND unaccountable — which §1 sends to `finish` instead.
    let blind = suggest::Walk {
        talked: vec![(10.4, 19.6)],
        silences: vec![],
        snap_points: vec![],
        marks: vec![],
    };
    let why = suggest::walk_back_audit(&blind).expect_err("a walk with no snap points cannot run unseen");
    assert!(
        why.contains("leaves nothing on screen afterwards") && why.contains("`finish`"),
        "the refusal names the failed property and where such a change belongs: {why}"
    );
    // Refused means NOT performed: `apply` touches nothing and logs the reason.
    let untouched = suggest::apply(&[(10.0, 20.0)], &[], &[], &blind);
    assert!(untouched.segs.is_empty(), "a refused walk-back places no cut");
    assert_eq!(untouched.log, vec![format!("!!! {why}")]);

    // The other door: when the budget runs out the model is owed the reason its calls produced nothing,
    // and the line names the home that news should have travelled in.
    let exhausted = tool_loop::unannounced_calls_log("cut", 2);
    assert!(
        exhausted.contains(Home::ToolResult.label()) && exhausted.contains(Home::ToolResult.scope()),
        "the line says which home stayed silent: {exhausted}"
    );
}

// ---- s5: policy / prompt --------------------------------------------------------------

/// Row 5: what lives here is a number or a wording, never a per-item judgement — so every field the policy
/// form presents must route to that home, and a value that routes elsewhere must not be shown as one.
#[test]
fn sec_12_decisions_1_the_five_homes_of_a_decision_s5_policy_home_rows_are_policy() {
    // Not the fifth home at all: an engineering constant and a machine setting are fixed before the
    // conversation and decide how the app or the servers run, not what this video becomes.
    assert!(!policy_form::policy_home("layout.thing"));
    assert!(!policy_form::policy_home("preview.padSeconds"));
    assert_eq!(
        decision_homes::home_of("layout.thing"),
        Some(decision_homes::Home::Engineering),
        "a bare prefixed id is engineering, which is why the form must reject it"
    );
    // Both halves of the fifth home: a `P.policy.` row and a shipped prompt key.
    assert!(policy_form::policy_home("P.policy.cutMode"));
    assert!(policy_form::policy_home("captions"), "a prompt wording is the same kind of non-decision");

    pin_private_cwd();
    settings::save(
        &settings::from_environment().expect("the pinned settings folder resolves"),
        &Conf::default(),
    )
    .expect("the settings file");
    window_round();
    assert!(RAN_FORM.load(Ordering::SeqCst), "the policy-form check never ran");
}

fn check_policy_form(app: &adw::Application) {
    ui::close_policy_forms();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();
    // The live session's policy, through the door the form itself reads — and one field set by hand so the
    // source column has something other than a default to show.
    ui::set_session_policy_field(&window, "cutMode", "model", "this shot", Origin::User);
    let policy = ui::session_policy(&window);
    let before = ui::window_logs().len();
    let form = policy_form::build(None, &policy);
    form.present();
    settle();

    // Every row the catalogue lists is drawn, and each one routed to the policy home — so nothing was
    // skipped, which is what the absence of a refusal line below proves rather than asserts.
    for entry in naivepost::policy::derived_fields() {
        assert!(policy_form::policy_home(entry.id), "{} is not the policy home", entry.field);
        let row = widget_in_window(&form, &format!("policy-row-{}", entry.field));
        assert!(row.is_some(), "the form shows a marker for {}", entry.field);
        let value = widget_in_window(&form, &format!("policy-value-{}", entry.field))
            .expect("the form shows the value");
        let label = value.downcast_ref::<gtk::Label>().expect("a value cell is a label");
        assert_eq!(
            label.label().to_string(),
            policy_form::value_word(&policy, entry.field),
            "the value cell states the policy's own value for {}",
            entry.field
        );
    }
    let cut_mode = widget_in_window(&form, "policy-value-cutMode")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        .expect("cutMode's value cell");
    assert_eq!(cut_mode.label(), "model", "the hand-set value is what the form prints");
    let source = widget_in_window(&form, "policy-source-cutMode")
        .and_downcast::<gtk::Label>()
        .expect("cutMode's source cell");
    assert_eq!(source.label(), "user", "and so is its source");

    let added = &ui::window_logs()[before..];
    assert!(
        !added.iter().any(|line| line.starts_with("!!!")),
        "no shipped row was rejected as outside the policy home: {added:?}"
    );
    form.close();
    window.close();
    RAN_FORM.store(true, Ordering::SeqCst);
}

/// [`common::widget_in`] walks an `ApplicationWindow`; the policy form is its own top level, so the same
/// walk starts from a bare widget instead.
fn widget_in_window(widget: &impl IsA<gtk::Widget>, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(widget.upcast_ref(), name)
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_policy_form(app);
            app.quit();
        });
        let _ = app.run_with_args::<String>(&[]);
    });
}
