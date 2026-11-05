//! F2.2 S1 / F2.3 — ▶✂ and ▶✂✂ follow the cut they play, whatever changed it, without a press of ▶ in between.
//! First rewrite: only the window's build and a press of ▶ repainted them, so on a project with no cut yet the
//! first suggestion's green clips stood beside a grey ▶✂ until ▶ had been pressed once.
//!
//! One application, one `connect_activate`, one `#[test]`, as tests/suite/cut_play_the_cut_widgets.rs: the
//! checks share the window's cut slot.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::Cut;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("fixtures");
    dir.push("demo.naivepost");
    dir
}

/// The Cut page of the fixture project, opened on NO cut: the state of a session before its first suggestion.
fn empty_cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let session = naivepost::startup::session_dir(&std::env::current_dir().expect("a cwd"));
    std::fs::create_dir_all(&session).expect("the session folder");
    naivepost::cut::save(&Cut::default(), &naivepost::layout::Tree::new(&session).expect("a .naivepost folder"))
        .expect("an empty cut beside the cwd");
    let window = ui::build_window(app, &model, "Prepare");
    common::size_to_minimum(&window);
    ui::tab_button(&window, Page::Cut).expect("the shell has a Cut tab").emit_clicked();
    common::wait_for_stable_layout(&window);
    window
}

fn settle() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

fn check(app: &adw::Application) {
    let window = empty_cut_window(app);
    settle();
    let play_cut = ui::play_cut_button(&window).expect("the Cut page carries play-cut-button");
    let review = ui::review_cuts_button(&window).expect("the Cut page carries review-cuts-button");
    ui::seed_review_cut(&Cut::default());
    settle();
    assert!(!play_cut.is_sensitive(), "no clips yet: ▶✂ is grey");
    assert!(!review.is_sensitive(), "and so is ▶✂✂");

    // The first suggestion lands: its clips are on the band, and ▶✂ can play them at once. Nothing presses ▶.
    ui::cut_suggest_land::land_spans(&window, &[(1.0, 4.0), (6.0, 9.0)]);
    settle();
    assert!(play_cut.is_sensitive(), "the suggestion's clips are playable without pressing ▶ first");
    assert_eq!(
        review.is_sensitive(),
        naivepost::cut_screen::can_review(&ui::window_preview::newest_review_cut()),
        "▶✂✂ answers from the same cut"
    );

    // And back: a cut emptied by any writer greys ▶✂ again, again without a press.
    ui::seed_review_cut(&Cut::default());
    settle();
    assert!(!play_cut.is_sensitive(), "an emptied cut greys ▶✂ again");
    window.close();
}

#[test]
fn f2_2_s1_play_cut_follows_the_cut_without_a_press_of_play() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        check(app);
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the check never ran");
}
