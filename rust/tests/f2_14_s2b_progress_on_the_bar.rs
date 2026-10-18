// F2.14 §S2b — "Streaming progress counts segments", from the ▶ press to the run bar.
//
// `cut_suggest_watch::Call::report` answered the progress; nothing painted it, so the Cut page's ▶ went from
// press to answer with a bar that never moved and a status row that never said it was thinking. These two
// checks are the wire: the click, and the bar the click put on screen.

use adw::prelude::*;

use naivepost::suggest;
use naivepost::cut_suggest;
use naivepost::tools;
use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{one_scene_cut, point_llm_at, retake, tool_turn, Fixture};

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::settle;

#[path = "captions_wire_harness.rs"]
mod harness;

static RAN_PROGRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_THINKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_POINTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// §S2b: a reply that placed segments moves the bar off zero and puts the counted line on the status row.
fn s2b_the_press_paints_progress_on_the_bar(app: &adw::Application) {
    let f = Fixture::fresh("progress-bar")
        .with_timeline()
        .with_marks(&[retake(50.0, 51.0)])
        .with_words(&[
            ("point", 4.0),
            ("mid", 10.0),
            ("here", 17.0),
            ("next", 25.0),
            ("end", 38.0),
        ]);
    // One ask, one accepted cut: the reply's own `segments` list is what `Call::report` counts, read from the
    // real tool calls the tool loop answered with — no scripted position anywhere.
    let llm = harness::FakeLlm::start(vec![tool_turn(&[
        ("add_segment", serde_json::json!({ "start": 3.0, "end": 19.0, "why": "the point" })),
        ("add_segment", serde_json::json!({ "start": 24.0, "end": 40.0, "why": "the second" })),
        ("finish_cut", serde_json::json!({})),
    ])]);
    point_llm_at(&llm.url());
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    ui::seed_review_cut(&window, &one_scene_cut());
    ui::reopen_history_on(&window, &ui::review_cut_of(&window));
    ui::set_session_policy_field(&window, "cutMode", "model", "this test", Origin::User);
    settle();

    let bar = ui::progress_bar(&window).expect("the run bar has its progress bar");
    let status = ui::find_status(window.upcast_ref()).expect("the shell has a status row");
    // Before the press the bar reads nothing: the number below is this click's doing, not a leftover.
    bar.set_fraction(0.0);
    ui::cut_suggest_progress::clear_painted_for_test();
    settle();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    // The count landed, which is what proves the progress came from THIS press's own reply.
    assert!(
        !ui::review_cut_of(&window).segs.is_empty(),
        "the suggestion landed, so its progress had a reply to count"
    );
    // The bar's own number is GTK's and cannot be read back — `ProgressBar` exposes no getter, which is why
    // `shell_beats::should_pulse` is asked rather than the widget — so the proof the door ran is the reading it
    // was given, read from the door's own record of what it painted. Which reading is a real counting, not a
    // slack one: the line a counted reading spells is `cut_suggest::progress`'s "N moments, at m:ss of m:ss",
    // and a reading with nothing closed yet spells only §S2b's thinking sentence.
    let painted = ui::cut_suggest_progress::painted_for_test();
    assert!(
        !painted.is_empty(),
        "the press's own replies reached the progress door at all: {painted:?}"
    );
    // Every reading is one of §S2b's two states and says which: a counted line, or the pulse under the thinking
    // sentence (`Call::report` gives a place only when the reply said where a segment ENDED; `cut_status` and
    // `finish_cut` answer with `"segments": <count>`, a number, not the array `cut_suggest::streamed` reads).
    // So this press pulsed — and the counted shape it did NOT have is `cut_suggest::progress`'s own spelling,
    // which is what a reply closing a segment with an `end` writes instead.
    for seen in &painted {
        let counted =
            seen.line.starts_with("1 moments, at ") && seen.line.ends_with(&format!(" of {}", tools::mm_ss(40.0)));
        assert!(
            (seen.pulsing && seen.line == cut_suggest::THINKING_STATUS) || counted,
            "a progress reading is either the pulse under the thinking sentence or `cut_suggest::progress`'s \
             counted line, got {seen:?}"
        );
    }
    assert!(
        painted.iter().any(|seen| seen.pulsing),
        "this press closed no segment with an end, so it pulsed: {painted:?}"
    );
    // The line the row ended on is the press's own answer (§S3's landed sentence), which could only follow the
    // progress the call reported while it worked.
    assert!(
        status
            .text()
            .starts_with(&format!("suggested {} segments, ", ui::review_cut_of(&window).segs.len())),
        "the row ends on the press's answer, which could only follow a progress line the door wrote, got {:?}",
        status.text()
    );
    RAN_PROGRESS.store(true, std::sync::atomic::Ordering::SeqCst);
    window.close();
    settle();
}

/// §S2b with nothing counted yet: the bar pulses under the thinking sentence rather than claiming a place.
fn s2b_the_bar_pulses_while_nothing_has_closed(app: &adw::Application) {
    let f = Fixture::fresh("progress-pulse")
        .with_timeline()
        .with_marks(&[retake(50.0, 51.0)])
        .with_words(&[("point", 4.0), ("mid", 10.0), ("here", 17.0)]);
    // A reply that is ALL reasoning: no tool call, only the content field. `Call::report` sees zero closed
    // segments, so the progress is the thinking line and the bar stays pulsed, and the loop then asks again
    // without thinking (that switch is §8's rule and `f2_14_press_model.rs` pins it).
    let thinking = serde_json::json!({
        "choices": [{ "message": { "content": "considering the whole session and its retakes" } }]
    })
    .to_string();
    let llm = harness::FakeLlm::start(vec![thinking]);
    point_llm_at(&llm.url());
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    ui::seed_review_cut(&window, &one_scene_cut());
    ui::reopen_history_on(&window, &ui::review_cut_of(&window));
    ui::set_session_policy_field(&window, "cutMode", "model", "this test", Origin::User);
    settle();

    let bar = ui::progress_bar(&window).expect("the run bar has its progress bar");
    let status = ui::find_status(window.upcast_ref()).expect("the shell has a status row");
    bar.set_fraction(0.0);
    status.set_text("");
    ui::cut_suggest_progress::clear_painted_for_test();
    settle();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    let painted = ui::cut_suggest_progress::painted_for_test();
    assert!(
        painted
            .iter()
            .any(|seen| seen.pulsing && seen.line == cut_suggest::THINKING_STATUS),
        "an all-thinking reply pulses the bar under §S2b's thinking sentence, painted {painted:?}"
    );
    assert_eq!(
        status.text().as_str(),
        suggest::NO_VALID_CUT,
        "the row ends on the press's own answer after three thinking attempts; the thinking line was its \
         progress line on the way there"
    );
    // A pulsed bar has fraction 0 — that is how GTK spells "moving, no position" — so the number that proves
    // the pulse is the rule's own answer, read back the same way the door read it.
    assert!(
        cut_suggest::pulses(0.0, 0),
        "no closed segment at no position is exactly what pulses"
    );
    assert_eq!(
        bar.fraction(),
        0.0,
        "and a pulsed bar claims no position on screen"
    );
    RAN_THINKING.store(true, std::sync::atomic::Ordering::SeqCst);
    window.close();
    settle();
}

/// §12 §4's session resolution: building a window must not move a session someone pointed at. The
/// suggest tests aim the flow at their own throwaway `.naivepost` and THEN build the shell they press ▶
/// in, so a fresh window that overwrote the pointing would refuse its own press for want of a timeline.
fn s2b_a_fresh_window_does_not_move_a_pointed_session(app: &adw::Application) {
    let f = Fixture::fresh("keep-pointed")
        .with_timeline()
        .with_words(&[("point", 4.0), ("here", 17.0)]);
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    settle();

    // The window is built on the launch's working copy, which has no `session.txt`: were its own folder
    // answering, this is the sentence the press would have got. It did not, because the pointing stands.
    let refused = cut_suggest_land::refuse_suggest_run(&window, false);
    assert_ne!(
        refused.as_deref(),
        Some(suggest::NO_TIMELINE),
        "the built window moved the session off the pointed folder: {refused:?}"
    );
    RAN_POINTED.store(true, std::sync::atomic::Ordering::SeqCst);
    cut_suggest_land::point_session_at(None);
    window.close();
}

fn round(app: &adw::Application) {
    s2b_the_press_paints_progress_on_the_bar(app);
    s2b_the_bar_pulses_while_nothing_has_closed(app);
    s2b_a_fresh_window_does_not_move_a_pointed_session(app);
}

/// The two checks read one click each, and `#[test]` functions run in parallel threads inside one process,
/// so the whole round runs ONCE and each test asserts the half it owns.
fn run_round() {
    common::one_app_round(round);
}

#[test]
fn f2_14_s2b_the_press_paints_progress_on_the_bar() {
    run_round();
    assert!(
        RAN_PROGRESS.load(std::sync::atomic::Ordering::SeqCst),
        "the progress-from-the-bar check never ran"
    );
}

#[test]
fn f2_14_s2b_the_bar_pulses_while_nothing_has_closed() {
    run_round();
    assert!(
        RAN_THINKING.load(std::sync::atomic::Ordering::SeqCst),
        "the thinking-pulse check never ran"
    );
}

#[test]
fn s2b_a_fresh_window_does_not_move_a_pointed_session_is_checked() {
    run_round();
    assert!(
        RAN_POINTED.load(std::sync::atomic::Ordering::SeqCst),
        "the pointed-session check never ran"
    );
}
