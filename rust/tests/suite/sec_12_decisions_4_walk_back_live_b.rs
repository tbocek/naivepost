//! §12-decisions.md §4 / S3, the WIRE half: a real click on the Cut page's ▶ in the words mode leaves the
//! page holding the WALKED cut, with the walk-back's own line in the log the person reads.
//!
//! A separate binary from `sec_12_decisions_4_walk_back_live.rs` because libadwaita initialises only inside
//! the one `run_with_args` a process gets; both include the same fixture module.

use adw::prelude::*;
use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;

use crate::common;
use common::settle;

#[path = "_sec_12_walk_back_fixture.rs"]
mod walk_fixture;
use walk_fixture::{base_cut, publish_tape, words_fixture};

/// Entry 4 / S3, the WIRE: a real click on the Cut page's ▶ in the words mode, and the page left holding
/// the walked cut with the walk's own line in the log. One scenario per binary (libadwaita initialises only
/// inside the one `run_with_args` per process), the arrangement `tests/suite/f2_14_press_words.rs` uses.
#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s3_the_cut_page_press_reaches_the_walk_back(
) {
    static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
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
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the walk-back click round never ran inside activate"
    );
}

fn round(app: &adw::Application) {
    let f = words_fixture("wire");
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    let base = base_cut();
    ui::seed_review_cut(&window, &base);
    ui::reopen_history_on(&window, &base);
    ui::set_session_policy_field(&window, "cutMode", "words", "this test", Origin::User);
    assert_eq!(
        ui::session_policy(&window).cut_mode.value,
        naivepost::project::CutMode::Words,
        "the press below is the words arm's"
    );
    settle();
    publish_tape();

    // THE CLICK. Everything after it is what the page was left with.
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    let on_screen = ui::review_cut_of(&window);
    assert!(
        on_screen.segs != base.segs,
        "the words walk changed the page's cut: {:?} -> {:?}",
        base.segs,
        on_screen.segs
    );
    // The state the logic test checks, reached through the button: the walk's line is in the log the person
    // reads, beside §S2a's own sentence.
    let logs = ui::window_logs();
    assert!(
        logs.iter()
            .any(|line| line.starts_with(">>> cut by the words: ")),
        "§S2a's line is there: {logs:?}"
    );
    assert!(
        logs.iter()
            .any(|line| line.contains("walk-back:") && line.contains("edge(s) moved")),
        "the walk-back said what it did on this press: {logs:?}"
    );
    // Fewer stretches than `words_cut` produced means the hole really was closed on the page, not just in
    // the arithmetic: one scene covers what used to be two with a spoken gap between them.
    assert!(
        on_screen.segs.len() < 3,
        "the spoken hole is gone from the page: {:?}",
        on_screen.segs
    );
    cut_suggest_land::point_session_at(None);
    window.close();
}
