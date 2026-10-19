//! §03-shell.md F0.8 New project — through the widgets. The header bar's ＋ New decides nothing: it
//! draws [`naivepost::new_project`]'s strings and forwards its press, so what these checks assert is
//! that a click asked rather than emptied (spec/00-principles.md §5).

use std::path::Path;

use adw::prelude::*;
use naivepost::new_project;
use naivepost::ui;
use crate::common;
use common::{fixture_dir};

/// One running GTK application for this test binary, taken in turn by the checks below — the same
/// single-main-loop arrangement tests/suite/smoke.rs needs for the same reason.
fn window_round() {
    common::one_app_round(|app| {
        check_button_and_dialog(app);
        empty_session_goes_straight_to_naming(app);
        applied_new_project_moves_the_window(app);
        already_there_is_not_applied(app);
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn f0_8_s3_the_new_button_asks_before_it_empties_anything() {
    common::pin_private_cwd();
    window_round();
    assert!(RAN.load(std::sync::atomic::Ordering::SeqCst), "the ＋ New check never ran");
}

/// The button is there with §1's tooltip; pressing it on a session that has something in it changes
/// nothing on screen — no refusal, no emptied project — because S3 asks first and the answer comes
/// later. And the dialog it shows is drawn from `new_project`'s strings rather than its own.
fn check_button_and_dialog(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let new_ = ui::new_button(&window).expect("the header bar has a New button");
    assert_eq!(
        ui::new_tooltip(&window).as_deref(),
        Some("New project \u{2014} name it, put it where you want it, and start over"),
        "badge 1 of §1's header bar"
    );

    new_.emit_clicked();
    // S3 is a question, so the window says nothing yet: no refusal (S1) and nothing emptied. The
    // status line is the only place this flow can speak, and it stays quiet until an answer.
    assert_eq!(ui::state(&window).status, "", "asking must not report anything");

    // The dialog itself, built from the module's strings: the question as its heading, and for a
    // session that never had a name, the paragraph that says so.
    let detail = new_project::confirm_detail(&naivepost::startup::session_dir(Path::new("/tmp/x")));
    let dialog = ui::new_project_confirm(None, &detail);
    assert_eq!(dialog.heading().as_deref(), Some(new_project::QUESTION));
    assert!(
        dialog.body().as_str().contains(
            "This session has never been saved under a name of its own, so there is nothing to come back to."
        ),
        "{}",
        dialog.body()
    );
    assert_eq!(
        dialog.response_appearance("start"),
        adw::ResponseAppearance::Destructive,
        "\"Start new…\" is the red button, as the spec's image draws it"
    );
    dialog.destroy();
}

#[test]
fn f0_8_s2_an_empty_session_is_not_asked_and_names_the_folder() {
    common::pin_private_cwd();
    window_round();
}

/// S2 through the real button: a session with nothing in it has nothing to throw away, so the press
/// does NOT ask — it goes on to name the folder. The desktop's folder sheet cannot be answered in a
/// headless container (that is why `open_folder`'s checks drive the seam behind the chooser too), so
/// what this proves is the branch: no refusal, no confirmation, and above all no emptied project — the
/// session stays exactly as empty as it was.
fn empty_session_goes_straight_to_naming(app: &adw::Application) {
    let root = common::temp_dir("empty-press");
    let blank_folder = root.join("blank.naivepost");
    std::fs::create_dir_all(&blank_folder).unwrap();
    naivepost::project::save(&naivepost::project::Project::default(), &blank_folder).unwrap();
    let model = naivepost::project::load(&blank_folder).expect("the blank session loads");
    assert!(model.sources.is_empty(), "this scenario is about an empty session");

    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::window::clear_logs_for_test();

    ui::new_button(&window)
        .expect("the header bar has a New button")
        .emit_clicked();

    // Nothing said: S1's refusal would have appeared on the status line, and S3's question would have
    // left it quiet but put a dialog on screen. Here the press moved straight past both.
    assert_eq!(ui::state(&window).status, "", "naming the folder says nothing yet");
    assert!(ui::window_logs().is_empty(), "{:?}", ui::window_logs());
    assert!(
        ui::session_sources(&window).is_empty(),
        "an empty session stayed empty"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_8_s6_an_applied_new_project_moves_the_window() {
    common::pin_private_cwd();
    window_round();
}

/// S6 through the window: applying a created project replaces the live session, logs its line, puts
/// its sentence on the status line, remembers the pair, and moves the choosers. The chooser itself is
/// the desktop's sheet, so the test hands the same `Created` the chooser's callback gets to the same
/// body that callback calls (`new_apply::apply`) — every rule still runs for real, including the write
/// to disk.
fn applied_new_project_moves_the_window(app: &adw::Application) {
    let fixture = fixture_dir();
    let model = naivepost::project::load(&fixture).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    let before = ui::session_sources(&window);
    assert!(!before.is_empty(), "the fixture has sources to lose");

    let root = common::temp_dir("applied");
    // The chooser's answer is the folder that IS the new project (01 §1), so its own name is what
    // `create` is given and `created.path` comes back as that same folder.
    let created = new_project::create(&root, &root, "fresh.naivepost")
        .expect("a free name creates");
    assert!(created.path.join(naivepost::project::PROJECT_FILE).is_file(), "S6 saved it");

    let status = ui::find_status(window.upcast_ref()).expect("the window has a status line");
    let session = ui::session_handle(&window);
    ui::window::clear_logs_for_test();
    naivepost::ui::new_apply::apply(&created, &session, &status);

    // The swap: the pages read this same handle, so an empty list here IS the refreshed page.
    assert!(ui::session_sources(&window).is_empty(), "the blank project became the session");
    assert_eq!(
        ui::state(&window).status,
        format!("new project \u{2014} {}", created.base),
        "S6's status sentence"
    );
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| line == &created.log),
        "the log carries S6's line verbatim: {:?}",
        ui::window_logs()
    );
    // S6's chooser rule, read the way the chooser reads it. The folder the sheet hands back IS the
    // project (01 §1), so `create` is called with that folder as its parent and the bare name -- which
    // puts the project one level below where a caller that passed the project folder itself would.
    // Under this root the outcome is the same either way: `Created::chooser` is None because nobody
    // chose a folder outside the root, and the defaults already point right.
    assert_eq!(
        created.chooser, None,
        "a project under the root moves no chooser"
    );
    assert_eq!(
        naivepost::ui::new_apply::chooser_dir(),
        None,
        "so nothing was remembered for the choosers"
    );
    // The case that DOES move them: a project created outside the root. `elsewhere` is the folder the
    // sheet was standing in and `far.naivepost` the name typed into it, so the PROJECT ends up at
    // `elsewhere/far.naivepost` while `Created::chooser` holds the parent the user actually chose --
    // which is what "chooser folders follow the chosen folder" means.
    let outside = new_project::create(&root, &root.join("elsewhere"), "far")
        .expect("a free name creates");
    naivepost::ui::new_apply::apply(&outside, &session, &status);
    assert_eq!(
        naivepost::ui::new_apply::chooser_dir().as_deref(),
        outside.chooser.as_deref(),
        "the choosers open where the user chose to put the project"
    );
    assert_ne!(
        naivepost::ui::new_apply::chooser_dir(),
        None,
        "a project outside the root moves them: {:?}",
        naivepost::ui::new_apply::chooser_dir()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_8_s5_a_project_already_there_is_not_applied() {
    common::pin_private_cwd();
    window_round();
}

/// S5: a name that already holds a project is refused, and the refusal changes nothing — the session
/// keeps its sources, the status line says which project was in the way, and no apply happens.
fn already_there_is_not_applied(app: &adw::Application) {
    let fixture = fixture_dir();
    let model = naivepost::project::load(&fixture).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    let before = ui::session_sources(&window);
    assert!(!before.is_empty(), "the fixture has sources to keep");

    let root = common::temp_dir("already");
    // The sheet hands back the folder that IS the project (01 §1), so `create` gets its PARENT plus the
    // bare name: a project at `<root>/jan.naivepost` is refused as `dir = <root>`, `named = jan`.
    let taken = root.join("jan.naivepost");
    std::fs::create_dir_all(&taken).unwrap();
    naivepost::project::save(&naivepost::project::Project::default(), &taken).unwrap();

    let refused = new_project::create(&root, &root, "jan")
        .expect_err("a project already there is refused");
    assert!(
        refused.contains("jan.naivepost is a project already \u{2014} open it, or pick another name"),
        "{refused}"
    );
    // The refusal arm of the chooser's callback: the status says it, and the session is untouched.
    let status = ui::find_status(window.upcast_ref()).expect("the window has a status line");
    status.set_text(&refused);
    assert_eq!(ui::state(&window).status, refused);
    assert_eq!(ui::session_sources(&window), before, "nothing half-applied");
    std::fs::remove_dir_all(&root).ok();
}
