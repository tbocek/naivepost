//! §05-cut#8 bullet 3, the WIRE: a real click on the Cut page's ▶ in the model mode, over a loopback fake
//! server, leaving §8's lines where the person can see them.
//!
//! One scenario per binary — libadwaita initialises only inside the one `run_with_args` per process, the
//! arrangement `tests/suite/f2_14_press_model.rs` documents. What the logic file proves with answers handed in,
//! this proves through the socket: the status row carries the suggest wording while the call runs, and the
//! log holds the §8 lines the loop now writes.

use adw::prelude::*;

use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{one_scene_cut, point_llm_at, retake, tool_turn, Fixture};

use crate::common;
use common::{settle, status_text};

#[path = "captions_wire_harness.rs"]
mod harness;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn round(app: &adw::Application) {
    // One mark so §S1's gate passes, and words whose seconds the walk-back can snap against.
    let f = Fixture::fresh("suggest-wire")
        .with_timeline()
        .with_marks(&[retake(50.0, 51.0)])
        .with_words(&[
            ("point", 4.0),
            ("mid", 10.0),
            ("here", 17.0),
            ("next", 25.0),
            ("end", 38.0),
        ]);
    // Two asks: the first places a 0.5 s scene (under `cutpass::MIN_SCENE_SECONDS`, so the tool refuses it —
    // a rejection, which is what §8 counts the web tools against), the second repairs it and finishes.
    let llm = harness::FakeLlm::start(vec![
        tool_turn(&[
            ("add_segment", serde_json::json!({ "start": 4.0, "end": 18.0, "why": "the point" })),
            ("add_segment", serde_json::json!({ "start": 30.0, "end": 30.5, "why": "a slip" })),
            ("finish_cut", serde_json::json!({})),
        ]),
        tool_turn(&[
            ("remove_segment", serde_json::json!({ "start": 30.0 })),
            ("remove_segment", serde_json::json!({ "start": 4.0 })),
            ("add_segment", serde_json::json!({ "start": 3.0, "end": 19.0, "why": "the point" })),
            ("add_segment", serde_json::json!({ "start": 24.0, "end": 40.0, "why": "the second" })),
            ("finish_cut", serde_json::json!({})),
        ]),
    ]);
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

    // THE CLICK, and the socket it opens.
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    // The request the program actually sent, read from the server's side: the SECOND ask is the one §8 changed,
    // because the first answer was refused. It carries the correction, and it carries no web tools — the same
    // rule `cut_suggest::offers_web` states, proved against bytes on the wire rather than against a log line.
    let seen = llm.drain();
    assert!(seen.len() >= 2, "a refused first plan means a second ask: {} seen", seen.len());
    let second = &seen[1];
    assert!(
        second.body.contains("That is not a valid cut"),
        "the second ask is §S4's correction: {}",
        &second.body[..1200.min(second.body.len())]
    );
    assert!(
        !second.body.contains("\"web_search\"") && !second.body.contains("\"web_read\""),
        "after a rejection the web tools are not offered: {}",
        &second.body[..2000.min(second.body.len())]
    );
    assert!(
        second.body.contains("add_segment"),
        "and the cut tools survive the withdrawal"
    );

    // The log holds the §8 lines the loop now writes, and the status row carries a suggest sentence rather
    // than a stale one — the page visibly knows a suggest ran.
    let logs = ui::window_logs();
    assert!(
        logs.iter().any(|line| line.starts_with(">>> suggested ") && line.contains(" segments, ")),
        "§S3's landing line is in the log: {logs:?}"
    );
    assert!(
        logs.iter().any(|line| line.contains("without the web tools")),
        "§8's withdrawal line reached the log with the click: {logs:?}"
    );
    let said = status_text(&window);
    assert!(
        said.contains("suggest") || said.contains("segments"),
        "the status row reports the suggestion, not a previous press: {said}"
    );
    assert!(
        !ui::review_cut_of(&window).segs.is_empty(),
        "a cut landed on the page"
    );
    RAN.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_the_suggest_lines_reach_the_page() {
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let inner = app.clone();
    app.connect_activate(move |_app| {
        round(&inner);
        inner.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(RAN.load(std::sync::atomic::Ordering::SeqCst), "the click round ran");
}
