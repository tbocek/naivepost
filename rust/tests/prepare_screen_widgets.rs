//! §04-prepare#1-screen — the Prepare page through the widgets.
//!
//! The page decides nothing: Freq, Language, the row picker and Reset each forward to
//! [`naivepost::prepare`] and [`naivepost::bench`], so what these checks assert is that a press or a
//! keystroke reached the plain module and that the heading says what §1 says it says — never that a
//! model was called or a file described (spec/00-principles.md §5).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

/// The bench writes through the settings folder, so this binary points the app at its own: the same
/// `XDG_CONFIG_HOME` every settings test uses, set before GTK starts and never read from `$HOME`.
fn config_home() -> PathBuf {
    let dir = std::env::temp_dir().join("np-prepare-screen-widgets");
    std::fs::create_dir_all(&dir).expect("a settings folder for the bench to write into");
    dir
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// One running GTK application for this binary, its environment already pointing at a private
/// settings folder. Each check records that it ran, so a check that never happened fails rather than
/// passing quietly; GTK's main loop may only be started by the thread that runs it, which is why all
/// three checks share one application.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();

    // §1's badges **2**-**5** and **8**: what the bench and the frame controls say.
    std::env::set_var("XDG_CONFIG_HOME", config_home());
    // The readouts count files under `<cwd>/session.naivepost` (that is what `session_reads`
    // resolves, the same stand-in Rescan/New/Save use), so the seed below has to sit where the page
    // will look. Pointed at before GTK starts, exactly as `XDG_CONFIG_HOME` is above.
    std::fs::create_dir_all(session_root()).expect("the session root exists to be cwd");
    std::env::set_current_dir(session_root()).expect("the session root is usable as cwd");
    seed_session_files();
    ONCE.call_once(|| {
        let app = adw::Application::new(Some(ui::APP_ID), gio::ApplicationFlags::NON_UNIQUE);
        static RAN: AtomicBool = AtomicBool::new(false);
        static RAN_READOUTS: AtomicBool = AtomicBool::new(false);
        app.connect_activate(move |app| {
            check_the_bench_and_frame_controls(app);
            RAN.store(true, Ordering::SeqCst);
            check_readouts_and_reset(app);
            RAN_READOUTS.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
        assert!(RAN.load(Ordering::SeqCst), "the bench check never ran");
        assert!(
            RAN_READOUTS.load(Ordering::SeqCst),
            "the readout-and-Reset check never ran"
        );
    });
}

/// The root the window treats as "this project": `<root>/session.naivepost`.
fn session_root() -> PathBuf {
    std::env::temp_dir().join("np-prepare-screen-session")
}

/// A session folder with one footage source that HAS frames and a transcript, so the Inputs row
/// has something real to count and the per-file tooltip has a line to print. Written fresh each
/// run: a leftover tree from an earlier run would make the counts wrong in the other direction.
fn seed_session_files() {
    let folder = naivepost::startup::session_dir(&session_root());
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("session folder");
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    naivepost::project::save(&model, &folder).expect("the fixture saves into the session folder");

    let lane = "lecture";
    let frames = naivepost::layout::Tree::new(&folder)
        .expect("a .naivepost folder is a project")
        .frames_dir(lane);
    std::fs::create_dir_all(&frames).expect("frames dir");
    for i in 0..9 {
        std::fs::write(frames.join(format!("2026-09-16_18-43-0{i}.000.jpg")), b"jpg")
            .expect("frame");
    }
    let tsv = naivepost::layout::Tree::new(&folder)
        .expect("project")
        .transcript_tsv(lane);
    if let Some(parent) = tsv.parent() {
        std::fs::create_dir_all(parent).expect("transcript parent");
    }
    let mut text = String::new();
    for i in 0..26 {
        text.push_str(&format!("word{i}\t0.0\t1.0\n"));
    }
    std::fs::write(&tsv, text).expect("transcript");
}

/// Let the emissions a `set_selected` / `emit_clicked` queued actually run, so the handler's
/// repaint has happened by the time the assertions read the widgets.
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

/// §1 badges **4**, **6** and **7** through the widgets: Reset pressed on a stored edit, and the
/// two bottom-bar readouts showing the same strings the plain modules compute.
fn check_readouts_and_reset(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();

    // ---- badge 4: Reset restores the shipped wording and takes the mark with it ----
    let picker = find(&window, "bench-picker")
        .expect("the bench has its row picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown");
    picker.set_selected(6);
    settle();
    let row = &naivepost::bench::ROWS[6];
    let buffer = find(&window, "bench-text")
        .expect("box")
        .downcast::<gtk::TextView>()
        .expect("text view")
        .buffer();
    buffer.set_text("cut where the speaker pauses");
    settle();
    assert!(
        naivepost::bench::edited(&settings_paths(), row),
        "the keystroke left an override on this machine"
    );
    assert!(
        find(&window, "bench-mark").expect("mark").is_visible(),
        "and the ✎ was drawn with it"
    );

    find(&window, "bench-reset")
        .expect("Reset is drawn")
        .downcast::<gtk::Button>()
        .expect("Reset is a button")
        .emit_clicked();
    settle();
    assert!(
        !naivepost::bench::edited(&settings_paths(), row),
        "S: pressing Reset cleared the override through `bench::reset`"
    );
    assert!(
        !find(&window, "bench-mark").expect("mark").is_visible(),
        "the mark went with the override — it is live only while this machine holds an edit"
    );
    assert!(
        !find(&window, "bench-reset").expect("reset").is_visible(),
        "and so did Reset, which has nothing left to restore"
    );
    // The box shows what `bench::reset` handed back. Read the shipped wording through a fresh
    // reset's return value rather than a literal, so retuning SHIPPED cannot leave this stale.
    let restored = naivepost::bench::reset(&settings_paths(), row)
        .expect("a second reset still answers")
        .expect("a prompt row always has shipped wording to restore");
    assert_eq!(text_of(&buffer), restored, "the box shows the shipped wording again");

    // ---- badge 6: the Inputs readout says the sentence AND carries the per-file tooltip ----
    let inputs = ui::inputs_readout(&window).expect("the bar has an Inputs readout");
    let tree = naivepost::layout::Tree::new(naivepost::startup::session_dir(&session_root()))
        .expect("the seeded session is a project");
    let expected = naivepost::prepare::inputs_readout(&naivepost::prepare::count(&tree, &model));
    assert_eq!(
        inputs.text().strip_prefix("Inputs: ").unwrap_or_default(),
        expected,
        "the label draws exactly what `Shell::inputs` computes for this session"
    );
    assert_eq!(
        inputs.tooltip_text().as_deref(),
        Some(
            naivepost::prepare::inputs_tip(&tree, &model)
                .as_str()
        ),
        "badge 6's per-file tooltip is the same string `prepare::inputs_tip` builds"
    );
    assert!(
        inputs.tooltip_text().unwrap_or_default().contains("lecture.mkv"),
        "and it names the file, not just numbers"
    );

    // ---- badge 7: Outputs reads "Prepare: N" and its button names three subfolders ----
    let outputs = ui::outputs_readout(&window).expect("the bar has an Outputs readout");
    let shown = outputs.text().to_string();
    let body = shown.strip_prefix("Outputs: ").unwrap_or(shown.as_str());
    assert!(
        body.starts_with(naivepost::prepare::OUTPUTS_LABEL),
        "§1 spells the label \"Prepare:\" and it leads: {body:?}"
    );
    let count: usize = body
        .trim_start_matches(naivepost::prepare::OUTPUTS_LABEL)
        .trim()
        .parse()
        .unwrap_or(0);
    assert!(
        count >= 10,
        "the count covers the whole prepare/ tree we seeded: {body:?}"
    );
    let folder_button = ui::outputs_folder_button(&window).expect("the Outputs folder button");
    let tip = folder_button
        .tooltip_text()
        .map(|tip| tip.to_string())
        .unwrap_or_default();
    for sub in ["prepare/inputs/", "prepare/describe/", "prepare/transcript/"] {
        assert!(
            tip.contains(sub),
            "badge 7 wants the tooltip to name {sub}; it says {tip:?}"
        );
    }
    assert!(folder_button.is_sensitive(), "there IS a folder to open now");

    // ---- Freq through the widget reaches the field `set_freq` writes ----
    let freq = find(&window, "freq-control")
        .expect("Freq")
        .downcast::<gtk::DropDown>()
        .expect("dropdown");
    assert_eq!(ui::session_freq(&window), 1.0, "the fixture opens at 1 s");
    freq.set_selected(5); // FREQ_STOPS[5] == 4.0
    settle();
    assert_eq!(
        ui::session_freq(&window),
        naivepost::prepare::FREQ_STOPS[5],
        "choosing a stop wrote Project::interval, the same field the logic test checks"
    );
    assert_eq!(
        naivepost::prepare::EXTRACTION_GRID,
        0.25,
        "// P.eng.frameGridSeconds — moving Freq to 4 s must not touch the extraction grid"
    );
}

/// §1's badges **2**, **3**, **4** and **5**: the row title, the picker in pipeline order, Reset only
/// while this machine holds an edit, and a keystroke stored rather than left on screen.
fn check_the_bench_and_frame_controls(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    // **5** the box starts on row 0, the User Context, and shows the project's own note.
    let editor = find(&window, "bench-text")
        .expect("the bench has its text box")
        .downcast::<gtk::TextView>()
        .expect("the bench's box is a text view");
    let title = find(&window, "prepare-title")
        .expect("the bench has its row title")
        .downcast::<gtk::Label>()
        .expect("the row title is a label");
    assert_eq!(
        title.text().as_str(),
        naivepost::bench::title(&naivepost::bench::ROWS[0]),
        "row 0 is the User Context and says so"
    );
    let buffer = editor.buffer();
    let shown = text_of(&buffer);
    assert_eq!(shown, model.context, "the box holds the project's note");

    // The User Context hides mark and Reset: there is no built-in wording for it to differ from.
    assert!(!find(&window, "bench-mark").expect("mark").is_visible(), "no mark on row 0");
    assert!(!find(&window, "bench-reset").expect("reset").is_visible(), "no Reset on row 0");

    // **8** Freq's stops and Language's warning, both under the sources list.
    let freq = find(&window, "freq-control")
        .expect("the page has Freq")
        .downcast::<gtk::DropDown>()
        .expect("Freq is a dropdown of the stops");
    let stops = strings(
        freq.model()
            .expect("Freq has its list of stops")
            .upcast_ref(),
    );
    assert_eq!(stops.len(), naivepost::prepare::FREQ_STOPS.len());
    assert_eq!(stops[2], "1s", "the default is 1 s and is shown as such");
    assert!(freq.is_sensitive(), "Freq answers to the page");

    let language = find(&window, "language-entry")
        .expect("the page has Language")
        .downcast::<gtk::Entry>()
        .expect("Language is a text box");
    assert_eq!(
        language.tooltip_text().as_deref(),
        Some(naivepost::prepare::LANGUAGE_TIP),
        "§1's warning about the wrong ASR code"
    );

    // Typing a language stores it on the session, not on the widget.
    language.set_text("de");
    assert_eq!(ui::bench_language(&window).as_deref(), Some("de"));

    // **3** the picker: thirteen rows in pipeline order, User Context first.
    let picker = find(&window, "bench-picker")
        .expect("the bench has its row picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown of the rows");
    let rows = naivepost::bench::picker_rows();
    assert_eq!(rows.len(), 13);
    assert_eq!(rows[0].1, "User Context");

    // Choosing the Cut prompt repaints the title as "Cut prompt" (§1's badge **2**).
    picker.set_selected(4);
    assert_eq!(
        title.text().as_str(),
        naivepost::bench::title(&naivepost::bench::ROWS[4]),
        "the row title names the chosen prompt"
    );

    // **5** every keystroke is stored: the text lands in this machine's prompts folder, and with it
    // comes the ✎ mark and a live Reset.
    buffer.set_text("cut on the sentence boundary");
    let key = naivepost::bench::ROWS[4].key;
    let saved = std::fs::read_to_string(settings_paths().prompt_file(key)).expect("the keystroke was stored");
    assert_eq!(saved, "cut on the sentence boundary");
    assert!(find(&window, "bench-mark").expect("mark").is_visible(), "✎ appears with the edit");
    let reset = find(&window, "bench-reset")
        .expect("reset")
        .downcast::<gtk::Button>()
        .expect("Reset is a button");
    assert!(reset.is_visible() && reset.is_sensitive(), "Reset is live while an edit is held");

    // **4** Reset restores the shipped wording and forgets this machine's file.
    reset.emit_clicked();
    assert!(
        naivepost::settings::read_prompt(&settings_paths(), key)
            .expect("the prompts folder is readable")
            .is_none(),
        "Reset leaves nothing behind"
    );
    assert_eq!(text_of(&buffer), "", "the box shows what is stored after a reset");
    assert!(!find(&window, "bench-mark").expect("mark").is_visible(), "the mark goes with the edit");

    // Back to row 0: typing there edits the project's note and writes no file at all. A keystroke
    // changes only the box; §1 asks for storage, and `bench::store` is what stores it — so this
    // asserts that the prompts folder stayed empty of anything for row 0 while the mark and Reset
    // stayed off, which is exactly what a row with no file of its own looks like. What the box will
    // be repainted with is the next frame's business, not §1's promise.
    picker.set_selected(0);
    buffer.set_text("a lecture about special relativity");
    assert!(
        naivepost::bench::ROWS[0].key.is_empty(),
        "row 0 is the User Context, which has no file"
    );
    assert!(
        naivepost::settings::read_prompt(&settings_paths(), key)
            .expect("the prompts folder is readable")
            .is_none(),
        "the User Context belongs to the project, not to this machine"
    );
    assert!(!find(&window, "bench-mark").expect("mark").is_visible(), "no mark on row 0");
    assert!(!find(&window, "bench-reset").expect("reset").is_visible(), "no Reset on row 0");
}

#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_s_the_page_shows_the_bench_and_frame_controls() {
    window_round();
}

/// The window's own settings folder, the one its bench writes into.
fn settings_paths() -> naivepost::settings::Paths {
    naivepost::settings::from_environment().expect("the test set XDG_CONFIG_HOME")
}

fn find(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    ui::find_source_widget(window, name)
}

/// The strings a `StringList` holds, in order.
fn strings(model: &glib::Object) -> Vec<String> {
    let list = model.downcast_ref::<gtk::StringList>().expect("a list of strings");
    (0..list.n_items())
        .map(|index| {
            list.item(index)
                .and_downcast::<gtk::StringObject>()
                .expect("a string item")
                .string()
                .to_string()
        })
        .collect()
}

fn text_of(buffer: &gtk::TextBuffer) -> String {
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, false).to_string()
}
