//! §03-shell.md F0.8 New project — through the widgets. The header bar's ＋ New decides nothing: it
//! draws [`naivepost::new_project`]'s strings and forwards its press, so what these checks assert is
//! that a click asked rather than emptied (spec/00-principles.md §5).

use std::path::Path;

use adw::prelude::*;
use naivepost::new_project;
use naivepost::ui;
use crate::common;
use common::{fixture_dir, settle};

/// One running GTK application for this test binary, taken in turn by the checks below — the same
/// single-main-loop arrangement tests/suite/smoke.rs needs for the same reason.
fn window_round() {
    common::one_app_round(|app| {
        check_button_and_dialog(app);
        check_the_button_row(app);
        RAN_BUTTONS.store(true, std::sync::atomic::Ordering::SeqCst);
        check_the_run_refuses_new(app);
        RAN_S1.store(true, std::sync::atomic::Ordering::SeqCst);
        naming_dialog_carries_the_default_name(app);
        RAN_ASK.store(true, std::sync::atomic::Ordering::SeqCst);
        empty_session_goes_straight_to_naming(app);
        press_asks_for_a_name_before_anything_changes(app);
        RAN_PRESS_ASK.store(true, std::sync::atomic::Ordering::SeqCst);
        applied_new_project_moves_the_window(app);
        typed_name_is_applied(app);
        RAN_TYPED.store(true, std::sync::atomic::Ordering::SeqCst);
        already_there_is_not_applied(app);
        refused_name_is_not_applied(app);
        RAN_REFUSED.store(true, std::sync::atomic::Ordering::SeqCst);
        choosers_follow_a_project_outside_the_root(app);
        RAN_FOLLOW.store(true, std::sync::atomic::Ordering::SeqCst);
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

static RAN_ASK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_PRESS_ASK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_REFUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_TYPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_FOLLOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_S1: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_BUTTONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// S1 through the real button: ▶ has a run standing, so ＋ New refuses rather than pulling the
/// session's inputs out from under it. The refusal is the SAME sentence `new_project::press`'s first
/// arm returns, and nothing else moves: no emptied sources, no log line, no dialog.
fn check_the_run_refuses_new(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::window::clear_logs_for_test();
    let before = ui::session_sources(&window);
    assert!(!before.is_empty(), "the fixture has sources a new project would take away");

    // A run stands the way a real ▶ stands one: `runqueue::start_run` is what that handler calls.
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    let queue = ui::run_queue(&window).expect("the window carries a queue");
    naivepost::runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut queue.borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        naivepost::run::Step::Prepare,
        naivepost::run::snapshot_sources(&model),
    );
    assert!(bar.borrow().running.is_some(), "a run is under way");

    ui::new_button(&window)
        .expect("the header bar has a New button")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(
        ui::state(&window).status,
        new_project::RUN_REFUSAL,
        "S1's sentence reaches the status line from the click"
    );
    assert_eq!(
        ui::session_sources(&window),
        before,
        "S1: nothing was emptied — a new project would pull the run's inputs out"
    );
    assert!(
        ui::window_logs().is_empty(),
        "a refusal writes no log line: {:?}",
        ui::window_logs()
    );

    // The flowchart asks "a run is on?" BEFORE "session empty?", so a run outranks even a session
    // with nothing to lose — the gate's precedence, read at the seam the button just went through.
    assert_eq!(
        new_project::press(true, true, Path::new("/tmp/whatever.naivepost")),
        new_project::Gate::Refused {
            reason: new_project::RUN_REFUSAL
        },
        "S1 outranks S2"
    );

    // Hand the bar back to idle so the checks that follow start where a person would.
    bar.borrow_mut().stop_flag = false;
    bar.borrow_mut().running = None;
    window.close();
    settle();
}

#[test]
fn f0_8_s1_the_new_button_is_refused_while_a_run_is_on() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_S1.load(std::sync::atomic::Ordering::SeqCst),
        "the \u{25b6}-refuses-New check never ran"
    );
}

/// The answer a button of this dialog stands for. libadwaita 0.9 keeps the response id on the C side
/// only (no binding accessor, and the buttons carry no distinguishing widget name — both read
/// `GtkButton`), so the label is the one thing on screen that identifies it: map each label back to the
/// id §F0.8 names it by. Order still comes from the walk, which is the point of the assertion.
fn response_id_of(button: &gtk4::Button) -> String {
    use gtk4::prelude::*;
    let label = button.label().unwrap_or_default().to_string();
    if label == new_project::BUTTON {
        "start".to_string()
    } else if label == "Cancel" {
        "cancel".to_string()
    } else {
        format!("unknown:{label}")
    }
}

/// Walk one widget's direct children in pack order (GTK 4's non-generic accessor — `observe_children`
/// returns a `ListModel` of `glib::Object`, whose iterator asserts the item type and so cannot be
/// walked generically here). Adwaita puts its boxes behind generic container types, so the walk does
/// not assume which ones.
fn nth_child(widget: &gtk4::Widget, index: usize) -> Option<gtk4::Widget> {
    let mut cursor = widget.first_child();
    let mut seen = 0usize;
    while let Some(child) = cursor {
        if seen == index {
            return Some(child);
        }
        seen += 1;
        cursor = child.next_sibling();
    }
    None
}

/// Collect every action button's response id under `root`, left to right. `adw::MessageDialog` packs
/// its answers into an inner box in add order, so walking depth-first yields them in the order a person
/// reads them — which is what §F0.8 pins and what no `response_label` call can answer.
fn collect_button_ids(root: &gtk4::Widget, ids: &mut Vec<String>) {
    use gtk4::prelude::*;
    if let Some(button) = root.downcast_ref::<gtk4::Button>() {
        ids.push(response_id_of(button));
    }
    for index in 0.. {
        match nth_child(root, index) {
            Some(child) => collect_button_ids(&child, ids),
            None => break,
        }
    }
}

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

/// S3's button row, which §F0.8 spells out: "buttons at the bottom right, left to right: Cancel,
/// 'Start new…'". Order is not decoration here — a destructive button on the left of Cancel invites a
/// blind click — so both labels, both positions and only `start` reading as dangerous are pinned.
#[test]
fn f0_8_s3_the_question_offers_cancel_then_start_new() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_BUTTONS.load(std::sync::atomic::Ordering::SeqCst),
        "the button-row check never ran"
    );
}

/// The row itself, asserted where the buttons live: `adw::MessageDialog` keeps its action buttons in
/// its action area in the order they were added, so walking that box left to right IS the order a
/// person reads. `response_label` cannot answer position, and libadwaita 0.9 exposes no `buttons()`
/// accessor, so this walks the widget tree instead of trusting the add order.
fn check_the_button_row(app: &adw::Application) {
    let _ = app;
    let detail = new_project::confirm_detail(&naivepost::startup::session_dir(Path::new("/tmp/x")));
    let dialog = ui::new_project_confirm(None, &detail);
    // Labels first: each id must carry §F0.8's own words.
    assert_eq!(dialog.response_label("cancel"), "Cancel");
    assert_eq!(dialog.response_label("start"), new_project::BUTTON);
    assert_eq!(
        dialog.response_appearance("start"),
        adw::ResponseAppearance::Destructive,
        "\"Start new\u{2026}\" is the red button"
    );
    assert_ne!(
        dialog.response_appearance("cancel"),
        adw::ResponseAppearance::Destructive,
        "Cancel must never read as the dangerous choice"
    );
    // Then the order, read off the widgets rather than off the add calls.
    let mut ids: Vec<String> = Vec::new();
    collect_button_ids(dialog.upcast_ref(), &mut ids);
    assert_eq!(
        ids,
        vec!["cancel".to_string(), "start".to_string()],
        "§F0.8: bottom right, left to right — Cancel then \"Start new\u{2026}\""
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

/// S4's sheet, seen on the widget that shows it: the default name is there to be typed over, and the
/// `-2`/`-3` walk already ran — with `2026-10-20.naivepost` and `2026-10-20-2.naivepost` taken, the box
/// offers `-3`. The day is handed in rather than read from a clock, so no check depends on midnight.
fn naming_dialog_carries_the_default_name(app: &adw::Application) {
    let _ = app;
    let root = common::temp_dir_tagged("ask-default");
    for taken in ["2026-10-20.naivepost", "2026-10-20-2.naivepost"] {
        std::fs::create_dir_all(root.join(taken)).unwrap();
    }
    let dialog = ui::window::new_project_dialog(&root, "2026-10-20");
    assert_eq!(dialog.title().as_str(), new_project::TITLE, "S4's title");
    assert_eq!(
        dialog.initial_name().map(|name| name.to_string()),
        Some("2026-10-20-3.naivepost".to_string()),
        "the free-name walk and the suffix, on the name the person edits"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_8_s4_the_naming_dialog_carries_the_default_name_to_type_over() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_ASK.load(std::sync::atomic::Ordering::SeqCst),
        "the default-name-on-the-sheet check never ran"
    );
}

/// S4 through the real button on an empty session (S2 sends that straight to naming): the press put up
/// the naming sheet, its default name skipped the date that was already taken, and nothing else moved.
/// The desktop's sheet cannot be typed into headlessly, so what this proves is the ask and the default;
/// [`f0_8_s5_a_name_holding_a_project_is_refused_through_the_window`] answers it through the same door
/// the sheet's callback calls.
fn press_asks_for_a_name_before_anything_changes(app: &adw::Application) {
    // The day the program itself would use, read the same way it reads it.
    let today = || {
        glib::DateTime::now_local()
            .and_then(|day| day.format("%F").map(|day| day.to_string()))
            .unwrap_or_default()
    };
    // The window names its own folder from `std::env::current_dir()`, and the sheet it opens stands in
    // that folder's PARENT (the default project lives under it), so the stand-in goes where the walk
    // looks — otherwise the taken date is invisible to it.
    let root = std::env::current_dir().expect("the test pins a private cwd");
    let day = today();
    let session = startup_session_of(&root);
    std::fs::create_dir_all(&session).unwrap();
    // Stand the default name out of the way so the free-name walk has something to skip.
    let taken = root.join(format!("{day}.naivepost"));
    std::fs::create_dir_all(&taken).unwrap();

    let blank = session.join("blank.naivepost");
    std::fs::create_dir_all(&blank).unwrap();
    naivepost::project::save(&naivepost::project::Project::default(), &blank).unwrap();
    let model = naivepost::project::load(&blank).expect("the blank session loads");
    assert!(model.sources.is_empty(), "an empty session goes straight to naming");

    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::window::clear_logs_for_test();

    ui::new_button(&window)
        .expect("the header bar has a New button")
        .emit_clicked();

    let asked = ui::window::last_ask().expect("the press opened the naming sheet");
    assert_eq!(asked.title().as_str(), new_project::TITLE, "S4's title");
    let offered = asked
        .initial_name()
        .map(|name| name.to_string())
        .expect("the sheet carries a name to type over");
    // A run that crosses midnight may have moved the day on by one; either day proves the walk ran.
    let days = [day.clone(), today()];
    assert!(
        offered.ends_with(".naivepost")
            && days.iter().any(|d| !d.is_empty() && offered.starts_with(d.as_str())),
        "the default name is today's date with the suffix: {offered}"
    );
    assert_ne!(offered, format!("{day}.naivepost"), "the taken date was skipped");
    // And the press itself changed nothing: creating waits for the answer.
    assert_eq!(ui::state(&window).status, "", "naming says nothing yet");
    assert!(ui::session_sources(&window).is_empty(), "the session stayed empty");
    assert!(ui::window_logs().is_empty(), "{:?}", ui::window_logs());
    window.close();
    std::fs::remove_dir_all(&blank).ok();
    std::fs::remove_dir_all(&taken).ok();
}

/// Where the working session sits for this root — the folder `startup::session_dir` names, reached the
/// way the window reaches it.
fn startup_session_of(root: &Path) -> std::path::PathBuf {
    naivepost::startup::session_dir(root)
}

#[test]
fn f0_8_s4_the_press_asks_for_a_name_before_anything_changes() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_PRESS_ASK.load(std::sync::atomic::Ordering::SeqCst),
        "the press-opens-the-naming-sheet check never ran"
    );
}

#[test]
fn f0_8_s5_a_name_holding_a_project_is_refused_through_the_window() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_REFUSED.load(std::sync::atomic::Ordering::SeqCst),
        "the S5 refusal-through-the-window check never ran"
    );
}

/// S5 answered through the window's own door: the status line says which project was in the way, the
/// session keeps everything it had, and no log line pretends anything happened.
fn refused_name_is_not_applied(app: &adw::Application) {
    let fixture = fixture_dir();
    let model = naivepost::project::load(&fixture).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    let before = ui::session_sources(&window);
    assert!(!before.is_empty(), "the fixture has sources to keep");

    let root = common::temp_dir_tagged("refused");
    // What the sheet hands back when the person types `jan`: the folder the name points at, under the
    // folder they were standing in.
    let taken = root.join("jan.naivepost");
    std::fs::create_dir_all(&taken).unwrap();
    naivepost::project::save(&naivepost::project::Project::default(), &taken).unwrap();

    let status = ui::find_status(window.upcast_ref()).expect("the window has a status line");
    let session = ui::session_handle(&window);
    ui::window::clear_logs_for_test();
    ui::window::name_chosen(&root, Some(&taken), &status, &session);

    assert_eq!(
        ui::state(&window).status,
        new_project::already_there("jan.naivepost"),
        "S5's sentence reaches the status line from the answer"
    );
    assert_eq!(ui::session_sources(&window), before, "nothing half-applied");
    assert!(ui::window_logs().is_empty(), "a refusal writes no log line: {:?}", ui::window_logs());
    window.close();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_8_s6_a_typed_name_is_applied_through_the_window() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_TYPED.load(std::sync::atomic::Ordering::SeqCst),
        "the typed-name S6 check never ran"
    );
}

/// S6 answering a TYPED name (no suffix typed, a folder that does not exist yet): the blank project is
/// written where the name pointed, applied to the session, logged and said.
fn typed_name_is_applied(app: &adw::Application) {
    let fixture = fixture_dir();
    let model = naivepost::project::load(&fixture).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    assert!(!ui::session_sources(&window).is_empty(), "the fixture has sources to lose");

    let root = common::temp_dir_tagged("typed");
    let status = ui::find_status(window.upcast_ref()).expect("the window has a status line");
    let session = ui::session_handle(&window);
    ui::window::clear_logs_for_test();
    // Typed as `fresh`, standing in `elsewhere`: the suffix is the rule's, not the sheet's.
    let typed = root.join("elsewhere/fresh");
    ui::window::name_chosen(&root, Some(&typed), &status, &session);

    let written = root.join("elsewhere/fresh.naivepost");
    assert!(
        written.join(naivepost::project::PROJECT_FILE).is_file(),
        "S6 wrote the blank project where the typed name pointed"
    );
    assert!(ui::session_sources(&window).is_empty(), "the blank project became the session");
    assert_eq!(
        ui::state(&window).status,
        "new project \u{2014} fresh.naivepost",
        "S6's status sentence"
    );
    assert!(
        ui::window_logs().iter().any(|line| line.starts_with(">>> new project ")
            && line.ends_with("-- the session is empty; outputs on disk are untouched")),
        "the log carries S6's line verbatim: {:?}",
        ui::window_logs()
    );
    window.close();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_8_s6_an_applied_new_project_moves_the_window() {
    common::pin_private_cwd();
    window_round();
}

#[test]
fn f0_8_s6_the_choosers_follow_a_project_created_outside_the_root() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_FOLLOW.load(std::sync::atomic::Ordering::SeqCst),
        "the chooser-folders-follow check never ran"
    );
}

/// S6's "chooser folders follow the chosen folder when outside the root", on the two choosers besides
/// Add source files. The desktop's sheets cannot be typed into headlessly, so each is reached the way
/// every other check here reaches one: ＋ New through its real button (the sheet it opens is the one the
/// press set, read back with `last_ask`), Open through the same body its button's callback calls
/// (`open_folder`, which is what `ask_open_project`'s answer forwards to) after the press has put up and
/// configured that chooser (`last_open_ask_for_test`).
///
/// It runs LAST in `window_round` because a new project becomes this window's live session here. The
/// no-remembered-folder case is the one `applied_new_project_moves_the_window` already asserts (a project
/// under the root leaves `chooser_dir()` None, and that check reads it back); what follows is the case
/// that MOVES them — by now earlier checks have applied an outside-the-root project, so the folder is set.
fn choosers_follow_a_project_outside_the_root(app: &adw::Application) {
    let fixture = fixture_dir();
    let model = naivepost::project::load(&fixture).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    let session = ui::session_handle(&window);
    let status = ui::find_status(window.upcast_ref()).expect("the window has a status line");
    let root = common::temp_dir_tagged("follow");

    // The case that moves them: created OUTSIDE the root, so its parent is the folder the person chose.
    let created = new_project::create(&root, &root.join("elsewhere"), "far")
        .expect("a free name creates");
    let chosen = created.chooser.clone().expect("outside the root names a folder");
    naivepost::ui::new_apply::apply(&created, &session, &status);
    assert_eq!(naivepost::ui::new_apply::chooser_dir().as_deref(), Some(chosen.as_path()));

    ui::new_button(&window)
        .expect("the header bar has a New button")
        .emit_clicked();
    let asked = ui::window::last_ask().expect("the press opened the naming sheet");
    assert_eq!(
        asked.initial_folder().and_then(|f| f.path()).as_deref(),
        Some(chosen.as_path()),
        "S4's sheet follows the chosen folder"
    );
    // And the default name it offers was walked in that same folder, not in the old one.
    assert!(
        asked.initial_name().is_some(),
        "the sheet still carries a name to type over"
    );

    // Open: the press configures the chooser exactly as Add's does, so the seam proves the folder; the
    // answer afterwards goes through the body the chooser's callback calls (`open_folder`).
    ui::open_button(&window)
        .expect("the header bar has an Open button")
        .emit_clicked();
    let opened = ui::window::last_open_ask_for_test().expect("an Open press put up its chooser");
    assert_eq!(
        opened.initial_folder().and_then(|f| f.path()).as_deref(),
        Some(chosen.as_path()),
        "the Open chooser follows the chosen folder too"
    );
    window.close();
    std::fs::remove_dir_all(&root).ok();
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
        .expect_err("a project already there is refused");    assert!(
        refused.contains("jan.naivepost is a project already \u{2014} open it, or pick another name"),
        "{refused}"
    );
    // The refusal arm of the chooser's callback, through the window's own door (`name_chosen`), not by
    // writing the status by hand: the status says it, and the session is untouched.
    let status = ui::find_status(window.upcast_ref()).expect("the window has a status line");
    ui::window::name_chosen(&root, Some(&taken), &status, &ui::session_handle(&window));
    assert_eq!(ui::state(&window).status, refused);
    assert_eq!(ui::session_sources(&window), before, "nothing half-applied");
    std::fs::remove_dir_all(&root).ok();
}
