// F2.14 §S2b — the model cut on a project that has no cut yet.
//
// The press asks on a worker, and the page's recordings are a thread-local of the window's thread: read on the worker
// they were empty, the fallback (the recordings the cut implies) was empty too with no cut, so the press found no
// footage and refused "run Describe first" on a project that had been described (09-admin, 2026-11-07). The press
// now takes the recordings at the click and hands them to its worker.

use adw::prelude::*;

use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::suggest;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{point_llm_at, tool_turn, Fixture};

use crate::common;
use common::settle;

#[path = "captions_wire_harness.rs"]
mod harness;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn s2b_the_model_press_reads_the_page_recordings_with_no_cut(app: &adw::Application) {
    let f = Fixture::fresh("model-no-cut").with_timeline().with_words(&[
        ("point", 4.0),
        ("mid", 10.0),
        ("here", 17.0),
        ("next", 25.0),
        ("end", 38.0),
    ]);
    let llm = harness::FakeLlm::start(vec![tool_turn(&[
        (
            "add_segment",
            serde_json::json!({ "start": 3.0, "end": 19.0, "why": "the point" }),
        ),
        (
            "add_segment",
            serde_json::json!({ "start": 24.0, "end": 40.0, "why": "the second" }),
        ),
        ("finish_cut", serde_json::json!({})),
    ])]);
    point_llm_at(&llm.url());
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    // No cut at all, as on a freshly prepared project: the footage is only on the page's tape.
    ui::seed_review_cut(&naivepost::cut::Cut::default());
    ui::reopen_history_on(&ui::window_preview::newest_review_cut());
    ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "cam0".to_string(),
        start: 0.0,
        end: 60.0,
    }]);
    ui::window_policy::write_policy_field("cutMode", "model", "this test", Origin::User);
    settle();
    let status = ui::find_status(window.upcast_ref()).expect("the shell has a status row");

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(60)),
        "the suggest workers answered"
    );
    settle();

    assert_ne!(
        status.text().as_str(),
        suggest::NO_TIMELINE,
        "the press saw the page's footage on its worker"
    );
    assert!(
        !ui::window_preview::newest_review_cut().segs.is_empty(),
        "and the model's cut landed, status {:?}",
        status.text()
    );
    RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    cut_suggest_land::point_session_at(None);
    window.close();
    settle();
}

#[test]
fn f2_14_s2b_the_model_press_reads_the_page_recordings_with_no_cut() {
    common::one_app_round(s2b_the_model_press_reads_the_page_recordings_with_no_cut);
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the check never ran"
    );
}
