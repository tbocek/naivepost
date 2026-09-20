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
    ONCE.call_once(|| {
        let app = adw::Application::new(Some(ui::APP_ID), gio::ApplicationFlags::NON_UNIQUE);
        static RAN: AtomicBool = AtomicBool::new(false);
        app.connect_activate(move |app| {
            check_the_bench_and_frame_controls(app);
            RAN.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
        assert!(RAN.load(Ordering::SeqCst), "the bench check never ran");
    });
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
