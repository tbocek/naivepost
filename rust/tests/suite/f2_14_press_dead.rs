// F2.14 §S2b's failure half: the model mode's ▶ dialled a server that is not there.
//
// The rules module answers with a sentence; this proves the press path shows it and touches nothing:
// no cut written, no run left open, and a status line that claims no cut.

use adw::prelude::*;
use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{one_scene_cut, point_llm_at, Fixture};

use crate::common;
use common::{settle, status_text};

#[path = "captions_wire_harness.rs"]
mod harness;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn round(app: &adw::Application) {
    // A port nothing listens on: what a server that is down looks like to `ureq`.
    let dead = harness::FakeLlm::dead_port();
    point_llm_at(&format!("http://127.0.0.1:{dead}"));
    let f = Fixture::fresh("press-dead")
        .with_timeline()
        .with_words(&[("hello", 3.0), ("there", 17.0)]);
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    ui::seed_review_cut(&window, &one_scene_cut());
    ui::set_session_policy_field(&window, "cutMode", "model", "this test", Origin::User);
    settle();
    let before = std::fs::read(press_fixture::run_tree().cut_json()).ok();
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let sentence = status_text(&window);
    assert!(
        !sentence.is_empty() && !sentence.contains("suggested"),
        "a refused dial says something and claims no cut: {sentence:?}"
    );
    assert_eq!(
        std::fs::read(press_fixture::run_tree().cut_json())
            .ok()
            .as_deref(),
        before.as_deref(),
        "a failed suggestion touches the working copy's cut not at all"
    );
    assert!(
        ui::run_bar(&window)
            .expect("the window carries a run bar")
            .borrow()
            .running
            .is_none(),
        "a refused press opens no run"
    );
    cut_suggest_land::point_session_at(None);
    window.close();
}

#[test]
fn f2_14_s2b_a_server_that_refuses_says_so() {
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
        "the dead-server round never ran inside activate"
    );
}
