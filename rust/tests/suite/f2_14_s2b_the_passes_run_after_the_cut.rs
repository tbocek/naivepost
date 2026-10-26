// F2.14 §S2b's `M2` node — the three passes that run AFTER a cut, only the ones the policy switches on.
//
// spec/05-cut.md §F2.14: "captions F3.9 → speeds F3.10 → decorations F3.11, only the passes
// P.policy.captionsPass · speedPass · decorationsPass switch on", and "(a context that says 'no captions, no
// speed changes, no effects' means those jobs are NEVER CALLED, not called and told to return nothing)".
//
// The first three checks are the pure rule in `cut_suggest_passes::order` plus §S3's effect line; the fourth
// is the wire: a real ▶ click on the Cut page, over a fake LLM, and the log afterwards shows which of the
// three were asked — with all three switched off, none of their sentences appears at all.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::Seg;
use naivepost::policy::Pass;
use naivepost::project::{Field, Origin, Policy};
use naivepost::shell::Page;
use naivepost::suggest;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{point_llm_at, retake, tool_turn, Fixture};

use crate::common;
use common::settle;

#[path = "captions_wire_harness.rs"]
mod harness;

use naivepost::cut_suggest_passes;

static RAN_ORDER: AtomicBool = AtomicBool::new(false);
static RAN_NONE: AtomicBool = AtomicBool::new(false);
static RAN_LINE: AtomicBool = AtomicBool::new(false);
static RAN_PRESS_ON: AtomicBool = AtomicBool::new(false);
static RAN_PRESS_OFF: AtomicBool = AtomicBool::new(false);

fn on(value: bool) -> Field<bool> {
    Field {
        value,
        origin: Origin::Default,
        because: None,
    }
}

/// A policy with exactly the three pipeline switches set as asked; everything else stays at its default.
fn policy_with(captions: bool, speeds: bool, decorations: bool) -> Policy {
    Policy {
        captions_pass: on(captions),
        speed_pass: on(speeds),
        decorations_pass: on(decorations),
        ..Policy::default()
    }
}

#[test]
fn f2_14_s2b_only_the_switched_on_passes_are_asked() {
    // All three on: the spec's order, all three present.
    assert_eq!(
        cut_suggest_passes::order(&policy_with(true, true, true)),
        vec![Pass::Captions, Pass::Speeds, Pass::Decorations],
        "the full set comes back in the order §S2b lists it"
    );
    // Each one off drops exactly that pass and moves nothing else.
    assert_eq!(
        cut_suggest_passes::order(&policy_with(false, true, true)),
        vec![Pass::Speeds, Pass::Decorations],
        "captions off means captions is not in the list at all"
    );
    assert_eq!(
        cut_suggest_passes::order(&policy_with(true, false, true)),
        vec![Pass::Captions, Pass::Decorations],
        "speeds off keeps captions before decorations"
    );
    assert_eq!(
        cut_suggest_passes::order(&policy_with(true, true, false)),
        vec![Pass::Captions, Pass::Speeds],
        "decorations off keeps the two that ran before it"
    );
    // Two off, one on: the survivor alone, still in order.
    assert_eq!(
        cut_suggest_passes::order(&policy_with(false, false, true)),
        vec![Pass::Decorations]
    );
    assert_eq!(
        cut_suggest_passes::order(&policy_with(false, true, false)),
        vec![Pass::Speeds]
    );
    assert_eq!(
        cut_suggest_passes::order(&policy_with(true, false, false)),
        vec![Pass::Captions]
    );
    // §S2b's count, read off the same list: the run holds the cut PLUS one job per switched-on pass, so
    // "up to four jobs in the run" is 4 with all three on and 1 with none — a number the policy answers,
    // never a control that stands beside the run.
    assert_eq!(
        1 + cut_suggest_passes::order(&policy_with(true, true, true)).len(),
        4,
        "all three on: the cut plus its three passes"
    );
    assert_eq!(
        1 + cut_suggest_passes::order(&policy_with(false, false, false)).len(),
        1,
        "none on: the run is the cut alone"
    );
    RAN_ORDER.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_a_context_ruling_out_all_three_asks_for_none() {
    // The pure rule: with all three switched off there is nothing to ask, so `run_after_cut` returns before
    // it touches a window — which is what makes "never called" true rather than "called and ignored". The
    // same claim from the door, with a real window and all three switches off, is in
    // `f2_14_s2b_the_press_runs_the_passes_after_the_cut` below.
    assert!(
        cut_suggest_passes::order(&policy_with(false, false, false)).is_empty(),
        "a context ruling out all three leaves nothing to ask"
    );
    assert_eq!(
        suggest::effects_line(0),
        ">>> \u{2026}and 0 effect(s): the speeds, the captions and the decorations",
        "the line a run with nothing to place would print is still a zero, not a swallowed sentence"
    );
    RAN_NONE.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_the_effects_line_counts_what_landed() {
    // §S3's sentence, verbatim, with the count that actually landed in it.
    assert_eq!(
        suggest::effects_line(3),
        ">>> \u{2026}and 3 effect(s): the speeds, the captions and the decorations",
        "the wording is the spec's own, once"
    );
    assert_eq!(
        suggest::effects_line(0),
        ">>> \u{2026}and 0 effect(s): the speeds, the captions and the decorations",
        "a zero is said as a zero, not swallowed"
    );
    RAN_LINE.store(true, Ordering::SeqCst);
}

/// The model answer that lands a cut: one scene per ask round, then `finish_cut`. Same shape as
/// `tests/suite/f2_14_press_model.rs`, whose fixture seconds these stamps snap onto.
fn good_plan() -> String {
    tool_turn(&[
        (
            "add_segment",
            serde_json::json!({ "start": 4.0, "end": 18.0, "why": "the point" }),
        ),
        (
            "add_segment",
            serde_json::json!({ "start": 24.0, "end": 38.0, "why": "the second" }),
        ),
        ("finish_cut", serde_json::json!({})),
    ])
}

/// One press of ▶ with the three switches set as asked, answering with the log lines the press added.
fn press_once(app: &adw::Application, switches: (bool, bool, bool)) -> Vec<String> {
    let f = Fixture::fresh("passes-after-cut")
        .with_timeline()
        .with_marks(&[retake(50.0, 51.0)])
        .with_words(&[
            ("point", 4.0),
            ("mid", 10.0),
            ("here", 17.0),
            ("next", 25.0),
            ("end", 38.0),
        ]);
    // The cut call gets the plan; every later ask (a pass dialling) repeats the last reply, which is no
    // usable caption/speed/decoration answer at all — the honest headless outcome.
    let llm = harness::FakeLlm::start(vec![good_plan()]);
    point_llm_at(&llm.url());
    cut_suggest_land::point_session_at(Some(f.tree.clone()));

    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    ui::seed_review_cut(
        &window,
        &naivepost::cut::Cut {
            segs: vec![Seg {
                s: 0.0,
                e: 60.0,
                cam: 0,
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    ui::reopen_history_on(&window, &ui::review_cut_of(&window));
    ui::set_session_policy_field(&window, "cutMode", "model", "this test", Origin::User);
    let (captions, speeds, decorations) = switches;
    ui::set_session_policy_field(
        &window,
        "captionsPass",
        if captions { "on" } else { "off" },
        "this test",
        Origin::User,
    );
    ui::set_session_policy_field(
        &window,
        "speedPass",
        if speeds { "on" } else { "off" },
        "this test",
        Origin::User,
    );
    ui::set_session_policy_field(
        &window,
        "decorationsPass",
        if decorations { "on" } else { "off" },
        "this test",
        Origin::User,
    );
    settle();

    let logs_before = ui::window_logs().len();
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    // Suggest asks on a worker (§03-shell §1, F2.14 "Where it runs"): the press lands when it answers.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(60)),
        "the suggest workers answered"
    );
    settle();
    let added: Vec<String> = ui::window_logs()[logs_before..].to_vec();
    cut_suggest_land::point_session_at(None);
    window.close();
    added
}

fn round(app: &adw::Application) {
    // --- switches ON: the passes are asked, and §S3's effect line counts what they placed -----------
    let logs = press_once(app, (true, true, true));
    assert!(
        logs.iter().any(|line| line.starts_with(">>> suggested ")),
        "the cut landed, so the passes had a cut to work over: {logs:?}"
    );
    // One of the three sentences is the tell that a pass was actually asked rather than skipped: each door
    // says its own line about its own attempt. The headless run has no endpoint wired for the passes, so what
    // they say is their own "nothing usable" line — logged as it stands, never rewritten here.
    let asked = ["caption", "speed", "decoration"]
        .iter()
        .filter(|word| logs.iter().any(|line| line.to_lowercase().contains(*word)))
        .count();
    assert!(
        asked >= 1,
        "at least one pass spoke up after the cut, so the press really asked: {logs:?}"
    );
    RAN_PRESS_ON.store(true, Ordering::SeqCst);

    // --- switches OFF: none of the three is asked, and nothing speaks for them ---------------------
    let quiet = press_once(app, (false, false, false));
    assert!(
        quiet.iter().any(|line| line.starts_with(">>> suggested ")),
        "the cut still lands with the passes off: {quiet:?}"
    );
    for refusal in ["captions are off", "speeds are off", "decorations are off"] {
        assert!(
            !quiet.iter().any(|line| line.contains(refusal)),
            "a ruled-out pass is never called, so it never gets to say `{refusal}` either: {quiet:?}"
        );
    }
    RAN_PRESS_OFF.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_the_press_runs_the_passes_after_the_cut() {
    one_round();
    assert!(
        RAN_PRESS_ON.load(Ordering::SeqCst),
        "the switches-on press never ran"
    );
    assert!(
        RAN_PRESS_OFF.load(Ordering::SeqCst),
        "the switches-off press never ran"
    );
}

fn one_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            round(app);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}
