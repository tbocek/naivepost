//! §03-shell.md F0.12 Add sources (from Prepare) — through the widgets. Prepare's "Add source files…"
//! decides nothing: it asks [`naivepost::add_sources`] and forwards the chooser's answer, so what
//! these checks assert is that a click either opened the chooser or was refused, never that anything
//! was copied (spec/00-principles.md §5).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::add_sources;
use naivepost::ui;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// One running GTK application for this test binary — the same single-main-loop arrangement
/// `tests/rescan_widgets.rs` needs for the same reason: both checks run in one `connect_activate`,
/// and each records that it got that far, so a check that never happened is a failure rather than a
/// quiet pass. The `Once` makes the pair run once for the binary; GTK's main loop may only be started
/// by the thread that will run it, which is why both checks share one application.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_offers_the_chooser(app);
            RAN_OFFER.store(true, Ordering::SeqCst);
            check_refused_during_a_run(app);
            RAN_REFUSED.store(true, Ordering::SeqCst);
            check_a_pick_lands_rows(app);
            RAN_PICK.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

use std::fs;

static RAN_OFFER: AtomicBool = AtomicBool::new(false);
static RAN_REFUSED: AtomicBool = AtomicBool::new(false);
static RAN_PICK: AtomicBool = AtomicBool::new(false);

/// S1 through the page: both widgets are there with §1's labels, and pressing the button opens the
/// chooser — which a headless test never answers — so nothing is added and the status stays quiet.
fn check_offers_the_chooser(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let button = ui::add_sources_button(&window).expect("Prepare has an Add source files button");
    assert_eq!(
        button.label().as_deref(),
        Some("Add source files\u{2026}"),
        "§1's wording for badge 9's own row"
    );
    let tick = ui::copy_into_project(&window).expect("the button comes with the copy tick");
    assert_eq!(tick.label().as_deref(), Some("copy into project"));

    button.emit_clicked();
    assert_eq!(ui::state(&window).status, "", "opening a dialog says nothing");
}

/// S0 through the widgets: ▶ starts a run (F0.2), and Add then says why it will not go ahead — no
/// chooser, because a copy is a run of its own.
fn check_refused_during_a_run(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    ui::play_button(&window).expect("the run bar has a ▶").emit_clicked();
    // ▶ is one button that becomes ⏸ (F0.2): the press above started the run and had nothing to say,
    // so what this asserts is the Add button's own sentence, not the run bar's.
    ui::add_sources_button(&window)
        .expect("Prepare has an Add source files button")
        .emit_clicked();
    assert_eq!(ui::state(&window).status, add_sources::RUN_REFUSAL);
}

#[test]
fn f0_12_s1_the_add_button_and_the_copy_tick_are_on_prepare() {
    window_round();
    assert!(RAN_OFFER.load(Ordering::SeqCst), "the Add check never ran");
}

/// S0 through the same widgets: refused during a run, with §1's sentence on the status line.
#[test]
fn f0_12_s0_adding_is_refused_while_a_run_is_on() {
    window_round();
    assert!(RAN_REFUSED.load(Ordering::SeqCst), "the refusal check never ran");
}

/// S2/S3/S4/S5 through the widgets: a pick copies into `sources/`, lands rows in the visible list,
/// skips the non-media file and the duplicate, moves F0.5's bar, and says what it did. The seam is
/// `ui::add_files` — the same body the chooser's response calls.
fn check_a_pick_lands_rows(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    // Built before the cwd moves: `add_files` takes the project as `startup::session_dir(cwd)`, and
    // pointing cwd at a throwaway root keeps the copies out of `rust/`.
    let root = std::env::temp_dir().join(format!("naivepost-f012w-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let card = root.join("card");
    fs::create_dir_all(&card).unwrap();
    let dir = naivepost::startup::session_dir(&root);
    let cam = card.join("cam.mkv");
    let wav = card.join("mic.wav");
    let txt = card.join("notes.txt");
    // Same name as the fixture's existing row, so it must count as already in the session.
    let dup = card.join("lecture.mkv");
    fs::write(&cam, vec![b'x'; 5]).unwrap();
    fs::write(&wav, vec![b'x'; 3]).unwrap();
    fs::write(&txt, vec![b'x'; 3]).unwrap();
    fs::write(&dup, vec![b'x'; 5]).unwrap();
    std::env::set_current_dir(&root).unwrap();

    // S2's default is the copy, read off the live tick rather than assumed.
    assert!(
        ui::copy_into_project(&window).expect("copy tick").is_active(),
        "copy defaults on"
    );
    assert!(
        ui::add_files(&window, &[cam.clone(), wav.clone(), txt.clone(), dup.clone()]),
        "two rows should go in"
    );

    let rows = ui::session_sources(&window);
    assert!(rows.contains(&"project:sources/cam.mkv".to_string()), "the video row: {rows:?}");
    assert!(rows.contains(&"project:sources/mic.wav".to_string()), "the audio row: {rows:?}");
    assert!(!rows.iter().any(|path| path.ends_with("notes.txt")), "non-media never becomes a row: {rows:?}");
    assert_eq!(
        rows.iter().filter(|path| *path == "project:sources/lecture.mkv").count(),
        1,
        "the duplicate is not added twice: {rows:?}"
    );
    // S5, the same sentence `tests/add_sources.rs` pins for this 2-of-4 mix.
    assert_eq!(ui::state(&window).status, "added 2 of 4 \u{2014} the rest were already in");
    // S2 on disk: the copy landed under `sources/`, no `.part` left behind.
    let copied = dir.join("sources").join("cam.mkv");
    assert_eq!(
        fs::metadata(&copied).map(|meta| meta.len()).unwrap_or(0),
        5u64,
        "copied into the project folder"
    );
    let parts = fs::read_dir(dir.join("sources"))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().to_string_lossy().ends_with(".part"))
                .count()
        })
        .unwrap_or(usize::MAX);
    assert_eq!(parts, 0, "a finished copy leaves no .part behind");
    // The visible list, not just the model: the fixture draws row 0, two went in.
    assert!(ui::find_source_widget(&window, "source-row-1").is_some(), "row 2 drawn");
    assert!(ui::find_source_widget(&window, "source-row-2").is_some(), "row 3 drawn");
    assert!(ui::find_source_widget(&window, "source-row-3").is_none(), "no row beyond what was added");
    // S3's footage default through the widget: `sources::row_controls` offers `Control::Footage`
    // only for a video ("on an audio file the control is not off — it does not exist"), so the new
    // video row carries `footage-1` and the new audio row carries no `footage-2` at all.
    assert!(ui::find_source_widget(&window, "footage-1").is_some(), "a new video row defaults to footage");
    assert!(ui::find_source_widget(&window, "footage-2").is_none(), "an audio row has no footage control");
    // F0.5's bar moved off zero while the bytes were copied.
    assert!(
        ui::progress_bar(&window).expect("run-progress").fraction() > 0.0,
        "the import drew on the bar"
    );

    // S5 again with nothing new: refused as a no-op, session untouched.
    let before = ui::session_sources(&window);
    assert!(!ui::add_files(&window, &[cam.clone(), wav.clone()]), "nothing new to add");
    assert_eq!(ui::state(&window).status, "already in the session \u{2014} nothing added");
    assert_eq!(ui::session_sources(&window), before, "a repeat pick adds nothing");

    // S0 on this same path: with a run on, the seam refuses before any byte moves.
    ui::play_button(&window).expect("the run bar has a ▶").emit_clicked();
    assert!(!ui::add_files(&window, &[cam]), "a copy is a run of its own, so it waits");
    assert_eq!(ui::state(&window).status, add_sources::RUN_REFUSAL);
    assert_eq!(ui::session_sources(&window), before, "refused before touching the session");
    window.close();
}

#[test]
fn f0_12_s2_a_pick_lands_rows_in_the_list_through_the_widgets() {
    window_round();
    assert!(RAN_PICK.load(Ordering::SeqCst), "the pick check never ran");
}
