//! §07-narrate#1-screen / F4.6 S1 over the WIRE: a voice this project stores but cannot offer leaves the
//! picker empty AND says which of the two ways it happened. The two sentences live in
//! [`naivepost::narrate_rules`] and were already unit-tested there (tests/suite/narrate_page_rules.rs);
//! what was missing is the page ever printing them, and `own` reaching the Narrator 1 row.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: GTK and libadwaita bind to the first
//! thread that initialises them, so every check is a stage of the same activate — the shape
//! tests/suite/sec_07_narrate_1_screen_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::narrate_rules;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_UNTAGGED: AtomicBool = AtomicBool::new(false);
static RAN_VANISHED: AtomicBool = AtomicBool::new(false);
static RAN_OWN: AtomicBool = AtomicBool::new(false);
static RAN_ONCE: AtomicBool = AtomicBool::new(false);

/// A session folder of our own, so nothing these checks write lands in a real project.
fn scratch_root(tag: &str) -> std::path::PathBuf {
    let root = naivepost::startup::session_dir(&std::env::temp_dir().join(format!(
        "np-sec07-missing-{}-{}",
        tag,
        std::process::id()
    )));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    root
}

fn base_state() -> ui::NarrateState {
    ui::NarrateState {
        segs: vec![
            naivepost::cut::Seg {
                s: 0.0,
                e: 30.0,
                ..Default::default()
            },
            naivepost::cut::Seg {
                s: 30.0,
                e: 60.0,
                ..Default::default()
            },
        ],
        voice: "narrator1".into(),
        session: 5.0,
        cut_at: 5.0,
        length: 60.0,
        clips: 2,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec![],
        recording_covers_clip: true,
        ..Default::default()
    }
}

/// The page with `state` published, built the way the shell builds it so the picker's own `wire` runs.
fn narrate_page(
    app: &adw::Application,
    state: ui::NarrateState,
) -> (adw::ApplicationWindow, usize) {
    release_last_window();
    // Where the log stood before this page was built: it is process-wide, and a check that wants to know
    // what THIS page said counts from here on.
    let mark = ui::window_logs().len();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.set_default_size(1280, 800);
    window.present();
    ui::tab_button(&window, naivepost::shell::Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(state);
    // Say nothing on the row before the check reads it: whatever the arrival at this page printed is not
    // this item's sentence, and an empty line makes "the page said it" a claim about one thing only.
    if let Some(status) = ui::find_status(window.upcast_ref()) {
        status.set_text("");
    }
    ui::refresh(&window);
    settle();
    (window, mark)
}

fn picker(window: &adw::ApplicationWindow) -> gtk::DropDown {
    ui::find_source_widget(window, "voice-picker")
        .expect("`voice-picker` is on the page")
        .downcast()
        .expect("`voice-picker` is a dropdown")
}

/// Count the lines the log holds that read like one of S1's two refusal sentences, from `mark` on: the
/// log is process-wide and earlier checks in this file filled it with their own refusals.
fn refusals_in_log(mark: usize) -> usize {
    ui::window_logs()[mark..]
        .iter()
        .filter(|line| {
            line.contains("is not tagged on the Prepare step") || line.contains("is no longer in")
        })
        .count()
}

/// The rows the dropdown offers, as their labels.
fn labels_of(picker: &gtk::DropDown) -> Vec<String> {
    let Some(list) = picker
        .model()
        .and_then(|m| m.downcast::<gtk::StringList>().ok())
    else {
        return vec![];
    };
    (0..list.n_items())
        .filter_map(|i| {
            list.item(i)
                .and_then(|item| item.downcast::<gtk::StringObject>().ok())
        })
        .map(|item| item.string().to_string())
        .collect()
}

/// The voices folder the refusal names — read through the app's own door rather than guessed here, so
/// the assertion holds wherever the settings point.
fn voices_dir() -> String {
    naivepost::ui::narrate_voices::voices_folder()
        .display()
        .to_string()
}

/// S1: a slot Prepare has no recording for is refused in Prepare's own words. What the picker does with an
/// unoffered voice (no row at all) is `sec_07_narrate_5_rules.rs`'s claim over the same setup; this item's
/// half is that the page SAYS which of the two ways the voice went missing.
fn check_untagged_narrator(app: &adw::Application) {
    let _ = scratch_root("untagged");
    let mut state = base_state();
    state.voice = "narrator3".into();
    state.narrators = 2;
    let (window, mark) = narrate_page(app, state);
    assert_eq!(
        status_text(&window),
        narrate_rules::missing_voice("narrator3", &voices_dir()),
        "the status line carries the rules module's sentence word for word"
    );
    assert!(
        refusals_in_log(mark) >= 1,
        "and it reached the log as well as the row"
    );
    RAN_UNTAGGED.store(true, Ordering::SeqCst);
}

/// S1: a voices-folder file that has gone is refused with the folder it is missing from.
fn check_vanished_voice_file(app: &adw::Application) {
    let _ = scratch_root("vanished");
    let mut state = base_state();
    state.voice = "aria".into();
    state.voice_files = vec![];
    let (window, _) = narrate_page(app, state);
    let said = status_text(&window);
    assert!(
        said.contains("is no longer in") && said.contains(&voices_dir()),
        "the sentence names the folder the file left: {said}"
    );
    assert_eq!(said, narrate_rules::missing_voice("aria", &voices_dir()));
    RAN_VANISHED.store(true, Ordering::SeqCst);
}

/// `own` is the legacy spelling of recording slot 1, so it gets the Narrator 1 row and no complaint.
fn check_legacy_own_voice(app: &adw::Application) {
    let _ = scratch_root("own");
    let mut state = base_state();
    state.voice = naivepost::narrate_data::OWN_VOICE.into();
    state.narrators = 2;
    let narrators = state.narrators;
    let (window, mark) = narrate_page(app, state);
    // A page that opens on a voice it CAN show prints no refusal: the log is what proves it, since the
    // status line also carries whatever the arrival at this page said.
    let refusals = || refusals_in_log(mark);
    let picker = picker(&window);
    let options = naivepost::narrate_screen::voice_options(narrators, &[]);
    assert_eq!(
        picker.selected(),
        narrate_rules::picker_row(&options, naivepost::narrate_data::OWN_VOICE)
            .expect("slot 1 is on offer") as u32,
        "the legacy `own` shows the Narrator 1 row: {} vs {:?}",
        picker.selected(),
        labels_of(&picker)
    );
    assert_eq!(
        refusals(),
        0,
        "a voice that IS on offer is not a refusal \u{2014} nothing was said about a missing one"
    );
    RAN_OWN.store(true, Ordering::SeqCst);
}

/// `refresh` runs on every seek, tick and row select, so the reason is told once per voice rather than
/// once per repaint.
fn check_the_refusal_is_told_once(app: &adw::Application) {
    let _ = scratch_root("once");
    let mut state = base_state();
    state.voice = "narrator4".into();
    state.narrators = 1;
    let (window, mark) = narrate_page(app, state);
    let said = status_text(&window);
    assert!(said.contains("not tagged on the Prepare step"), "{said}");
    // Count the copies in the log: `say` writes every sentence there, so a repeated refusal is visible
    // even though the status line can only ever hold the last one.
    let count = refusals_in_log(mark);
    assert!(count >= 1, "the refusal reached the log too");
    for _ in 0..3 {
        ui::refresh(&window);
        settle();
    }
    assert_eq!(
        refusals_in_log(mark),
        count,
        "three more repaints of the same voice added no copy of the sentence"
    );
    assert_eq!(status_text(&window), said, "and the row still carries it");
    // A DIFFERENT missing voice is a different problem and gets its own sentence.
    let mut next = ui::read_state();
    next.voice = "gone-take".into();
    ui::set_state(next);
    ui::refresh(&window);
    settle();
    assert!(
        status_text(&window).contains("\"gone-take\""),
        "another missing voice is told again: {}",
        status_text(&window)
    );
    RAN_ONCE.store(true, Ordering::SeqCst);
}

fn round(app: &adw::Application) {
    check_untagged_narrator(app);
    check_vanished_voice_file(app);
    check_legacy_own_voice(app);
    check_the_refusal_is_told_once(app);
    release_last_window();
}

#[test]
fn sec_07_narrate_1_s1_a_stored_voice_that_is_not_on_offer_is_said() {
    let _ = gtk::init();
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(round);
    app.run_with_args::<&str>(&[]);
    assert!(
        RAN_UNTAGGED.load(Ordering::SeqCst),
        "the untagged-slot check ran"
    );
    assert!(
        RAN_VANISHED.load(Ordering::SeqCst),
        "the vanished-file check ran"
    );
    assert!(RAN_OWN.load(Ordering::SeqCst), "the legacy `own` check ran");
    assert!(RAN_ONCE.load(Ordering::SeqCst), "the told-once check ran");
}
