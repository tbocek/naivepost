// F2.14 "Where it runs" (§03-shell §1): the Cut page's ▶ in the model mode reads the page, opens the run's bar and
// returns; the model is asked on a worker, and the cut lands (and the run closes) when it answers. The first
// rewrite asked inside the click, the window frozen for every round.

use adw::prelude::*;

use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{one_scene_cut, point_llm_at, retake, tool_turn, Fixture};

use crate::common;
use common::settle;

#[path = "captions_wire_harness.rs"]
mod harness;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// How long the fake model takes to answer each ask.
const MODEL_TAKES: std::time::Duration = std::time::Duration::from_millis(1500);

fn round(app: &adw::Application) {
    let f = Fixture::fresh("press-returns")
        .with_timeline()
        .with_marks(&[retake(50.0, 51.0)])
        .with_words(&[
            ("point", 4.0),
            ("here", 17.0),
            ("next", 25.0),
            ("end", 38.0),
        ]);
    let llm = harness::FakeLlm::start_slow(
        vec![tool_turn(&[
            (
                "add_segment",
                serde_json::json!({ "start": 3.0, "end": 19.0, "why": "the point" }),
            ),
            (
                "add_segment",
                serde_json::json!({ "start": 24.0, "end": 40.0, "why": "the second" }),
            ),
            ("finish_cut", serde_json::json!({})),
        ])],
        MODEL_TAKES,
    );
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
    // Only the cut is asked: the passes after it would each wait on the slow model too.
    ui::set_captions_pass_for_test(&window, false);
    ui::window::set_speeds_pass_for_test(&window, false);
    ui::window::set_decorations_pass_for_test(&window, false);
    settle();
    let before = ui::review_cut_of(&window).segs;

    let pressed = std::time::Instant::now();
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    let took = pressed.elapsed();
    assert!(
        took < std::time::Duration::from_millis(600),
        "the press came back before the model answered: it took {:.3} s",
        took.as_secs_f64()
    );
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    assert!(
        bar.borrow().running.is_some(),
        "the run is open while the model is asked, so ⏹ reaches it"
    );
    assert_eq!(
        ui::review_cut_of(&window).segs,
        before,
        "nothing landed yet: the cut on screen is the one the press found"
    );

    assert!(
        ui::background::settle_for_test(std::time::Duration::from_secs(60)),
        "the suggest workers answered"
    );
    settle();
    assert!(
        bar.borrow().running.is_none(),
        "the run closed when the cut landed"
    );
    assert_ne!(
        ui::review_cut_of(&window).segs,
        before,
        "the model's cut landed on the page"
    );
    cut_suggest_land::point_session_at(None);
    window.close();
}

#[test]
fn f2_14_where_it_runs_the_press_returns_with_the_run_open() {
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        round(app);
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the press round never ran inside activate"
    );
}
