//! §04-prepare#1-screen — the Prepare page through the widgets.
//!
//! The page decides nothing: Freq, Language, the row picker and Reset each forward to
//! [`naivepost::prepare`] and [`naivepost::bench`], so what these checks assert is that a press or a
//! keystroke reached the plain module and that the heading says what §1 says it says — never that a
//! model was called or a file described (spec/00-principles.md §5).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use common::{fixture_dir, settle};
use gtk4 as gtk;
use naivepost::ui;

/// The bench writes through the settings folder, so this binary points the app at its own: the same
/// `XDG_CONFIG_HOME` every settings test uses, set before GTK starts and never read from `$HOME`.
/// Pid-scoped like [`session_root`]: both tests of this binary call `window_round` and the bench WRITES here.
fn config_home() -> PathBuf {
    let dir = common::temp_dir_noclean("np-prepare-screen-widgets");
    std::fs::create_dir_all(&dir).expect("a settings folder for the bench to write into");
    dir
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
        static RAN_PICKER: AtomicBool = AtomicBool::new(false);
        static RAN_SHIPPED: AtomicBool = AtomicBool::new(false);
        app.connect_activate(move |app| {
            check_the_bench_and_frame_controls(app);
            RAN.store(true, Ordering::SeqCst);
            check_readouts_and_reset(app);
            RAN_READOUTS.store(true, Ordering::SeqCst);
            check_picker_tooltip_follows_the_row(app);
            RAN_PICKER.store(true, Ordering::SeqCst);
            check_shipped_wording_and_reset(app);
            RAN_SHIPPED.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
        assert!(RAN.load(Ordering::SeqCst), "the bench check never ran");
        assert!(
            RAN_READOUTS.load(Ordering::SeqCst),
            "the readout-and-Reset check never ran"
        );
        assert!(
            RAN_PICKER.load(Ordering::SeqCst),
            "the picker-tooltip check never ran"
        );
        assert!(
            RAN_SHIPPED.load(Ordering::SeqCst),
            "the shipped-wording-and-Reset check never ran"
        );
    });
}

/// The root the window treats as "this project": `<root>/session.naivepost`.
///
/// `temp_dir_noclean`, NOT a per-call fresh folder: this binary's tests are THREADS of one process and both
/// call [`window_round`], whose seed clears the session folder. A name shared with other processes (or a
/// `remove_dir_all` of the live cwd) is how one run failed with "naivepost.json: No such file or directory"
/// while the next passed; the pid keeps other runs out, and the seed below still makes the session itself
/// fresh each time.
fn session_root() -> PathBuf {
    common::temp_dir_noclean("np-prepare-screen-session")
}

/// A session folder with one footage source that HAS frames and a transcript, so the Inputs row
/// has something real to count and the per-file tooltip has a line to print. Written fresh each
/// run: a leftover tree from an earlier run would make the counts wrong in the other direction.
fn seed_session_files() {
    let folder = naivepost::startup::session_dir(&session_root());
    // Written fresh, but NOT by deleting the root: this file's own `window_round` has already made `/tmp/…-session`
    // the process's CWD, and removing a live cwd takes it away from every later path resolution in this program —
    // which is how one run of this binary failed with "naivepost.json: No such file or directory" while another
    // passed. The session folder itself is what has to be fresh (stale frames would make the readouts wrong), and
    // clearing THAT leaves the cwd standing.
    std::fs::create_dir_all(session_root()).expect("session root");
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
        std::fs::write(
            frames.join(format!("2026-09-16_18-43-0{i}.000.jpg")),
            b"jpg",
        )
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

/// Lets the emissions a `set_selected` / `emit_clicked` queued actually run, so the handler's
/// repaint has happened by the time the assertions read the widgets. (`common::settle`.)
/// §6 badge 5 through the widget, on a window of its own: with nothing stored on this machine yet, the
/// picker opens on row 0 (the User Context) and already carries that row's description — the tooltip is
/// there before anyone touches the dropdown, not only after a selection changes.
fn check_picker_tooltip_follows_the_row(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();
    let picker = find(&window, "bench-picker")
        .expect("the bench has its row picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown");
    assert_eq!(
        picker.tooltip_text().as_deref(),
        Some(naivepost::bench::tooltip(&naivepost::bench::ROWS[0]).as_str()),
        "the picker says what the row it shows does"
    );
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
    // §6 badge 5: the picker itself says what the row it names does — the job it gets and what it
    // answers — so its tooltip is the SELECTED row's, read through `bench::tooltip` rather than a literal
    // (the Describe row embeds the frames-per-request constant in its sentence).
    assert_eq!(
        picker.tooltip_text().as_deref(),
        Some(naivepost::bench::tooltip(row).as_str()),
        "the dropdown carries the selected row's own description"
    );
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
    // §6: while something IS stored, Reset's tooltip is the live one — the button sits beside a box that
    // may hold an hour of the editor's own tuning, so it has to say what the click gives back. The mark's
    // visibility is repainted by the keystroke handler directly (it does not go through `paint_bench`),
    // and this window's row picker was moved after its own paint, so the tooltip is asked of the rule the
    // same way the handler asks — through `bench::edited`, which says the override really is held now.
    assert!(
        naivepost::bench::edited(&settings_paths(), row),
        "and the rule agrees the override is on this machine"
    );
    assert_eq!(
        naivepost::bench::reset_tip(row, true),
        naivepost::bench::RESET_TIP,
        "live Reset says \"{}\"",
        naivepost::bench::RESET_TIP
    );
    assert!(
        find(&window, "bench-mark").expect("mark").is_visible(),
        "and the ✎ was drawn with it"
    );
    // §6's mark tooltip: the sentence about where the kept wording lives and what Reset does to it.
    assert_eq!(
        find(&window, "bench-mark")
            .expect("mark")
            .tooltip_text()
            .as_deref(),
        Some(naivepost::bench::KEPT_TIP),
        "the mark's tooltip is \"{}\"",
        naivepost::bench::KEPT_TIP
    );
    // §1's badge 2 has to READ as words at window scale: a lone U+270E is a barely visible outline in
    // DejaVu, which is why the face carries the word next to the pencil.
    let mark_face = find(&window, "bench-mark")
        .expect("mark")
        .downcast::<gtk::Label>()
        .expect("the mark is a label")
        .text()
        .to_string();
    assert_eq!(
        mark_face,
        naivepost::bench::EDITED_MARK,
        "the mark wears `bench::EDITED_MARK`"
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
    // §6's two Reset tooltips follow the button's liveness, and `paint_bench_marks` — which Reset's own
    // handler runs — is what swaps them. With the override gone the button says so instead of promising a
    // restore.
    let reset = find(&window, "bench-reset")
        .expect("Reset is drawn")
        .downcast::<gtk::Button>()
        .expect("Reset is a button");
    assert_eq!(
        reset.tooltip_text().as_deref(),
        Some(naivepost::bench::UNCHANGED_TIP),
        "after Reset the tooltip went idle with the button: {:?}",
        reset.tooltip_text()
    );
    assert_eq!(
        naivepost::bench::reset_tip(row, false),
        naivepost::bench::UNCHANGED_TIP,
        "the rule behind the widget: no edit, the idle sentence"
    );
    // The box shows what `bench::reset` handed back. Read the shipped wording through a fresh
    // reset's return value rather than a literal, so retuning the shipped wording cannot leave this stale.
    let restored = naivepost::bench::reset(&settings_paths(), row)
        .expect("a second reset still answers")
        .expect("a prompt row always has shipped wording to restore");
    assert_eq!(
        text_of(&buffer),
        restored,
        "the box shows the shipped wording again"
    );

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
        Some(naivepost::prepare::inputs_tip(&tree, &model).as_str()),
        "badge 6's per-file tooltip is the same string `prepare::inputs_tip` builds"
    );
    assert!(
        inputs
            .tooltip_text()
            .unwrap_or_default()
            .contains("lecture.mkv"),
        "and it names the file, not just numbers"
    );

    // ---- badge 7: Outputs reads "Prepare: N files, size" and its button names three subfolders ----
    let outputs_label = ui::outputs_readout(&window).expect("the bar has an Outputs readout");
    let shown = ui::outputs_sentence(&window).expect("the bar has an Outputs readout");
    let body = shown.strip_prefix("Outputs: ").unwrap_or(shown.as_str());
    assert!(
        body.starts_with(naivepost::prepare::OUTPUTS_LABEL),
        "§1 spells the label \"Prepare:\" and it leads: {body:?}"
    );
    // §6's shape after the label, so the count is read as the leading number of "N files, size".
    let count: usize = body
        .trim_start_matches(naivepost::prepare::OUTPUTS_LABEL)
        .split_whitespace()
        .next()
        .and_then(|lead| lead.parse().ok())
        .unwrap_or(0);
    assert!(
        count >= 10,
        "the count covers the whole prepare/ tree we seeded: {body:?}"
    );
    // The row draws exactly what the plain helpers compute for this folder — round-tripped through them,
    // never a literal — and §6's second half ("newest <ago>") reaches the row as its tooltip.
    let (bytes, newest) = naivepost::prepare::outputs_size(&tree);
    let (line, tip) = naivepost::prepare::outputs_readout(
        naivepost::prepare::outputs_count(&tree),
        bytes,
        newest,
    );
    assert_eq!(
        body,
        &format!("{} {}", naivepost::prepare::OUTPUTS_LABEL, line),
        "the label is §1's prefix over §6's count-and-size sentence, with one space between: {body:?}"
    );
    assert!(
        !tip.is_empty(),
        "a folder with files in it has a newest one, so there is an age to show"
    );
    assert_eq!(
        outputs_label.tooltip_text().as_deref(),
        Some(tip.as_str()),
        "§6's \"newest <ago>\" is the row's tooltip"
    );
    assert!(
        outputs_label
            .tooltip_text()
            .unwrap_or_default()
            .starts_with("newest "),
        "and it reads as an age: {:?}",
        outputs_label.tooltip_text()
    );
    let folder_button = ui::outputs_folder_button(&window).expect("the Outputs folder button");
    let tip = folder_button
        .tooltip_text()
        .map(|tip| tip.to_string())
        .unwrap_or_default();
    for sub in [
        "prepare/inputs/",
        "prepare/describe/",
        "prepare/transcript/",
    ] {
        assert!(
            tip.contains(sub),
            "badge 7 wants the tooltip to name {sub}; it says {tip:?}"
        );
    }
    assert!(
        folder_button.is_sensitive(),
        "there IS a folder to open now"
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
    assert!(
        !find(&window, "bench-mark").expect("mark").is_visible(),
        "no mark on row 0"
    );
    assert!(
        !find(&window, "bench-reset").expect("reset").is_visible(),
        "no Reset on row 0"
    );

    // **1** §4's row controls, on the Prepare page: the 🗣 split toggle is there and a press lands on
    // the same field the logic test checks (`sources::set_sepvoice` writes `Source::sepvoice`).
    let separate = find(&window, "separate-0")
        .expect("the row carries its \u{1f5e3} split control")
        .downcast::<gtk::ToggleButton>()
        .expect("the split control is a toggle");
    assert!(
        naivepost::sources::separable(&model.sources[0]),
        "lecture.mkv is not a split product, so the toggle answers"
    );
    assert!(separate.is_sensitive(), "and it is live on this row");
    assert!(
        !session_of(&window).borrow().sources[0].sepvoice,
        "it starts off"
    );
    separate.emit_clicked();
    settle();
    assert!(
        session_of(&window).borrow().sources[0].sepvoice,
        "pressing it wrote Source::sepvoice on the session the window holds"
    );

    // **8** No Freq under the sources list: the text gate (F1.6 S6) decides which frames describe sees.
    assert!(
        find(&window, "freq-control").is_none() && find(&window, "freq-stepper").is_none(),
        "Freq is gone from Prepare"
    );

    // No Language box: the User Context says the language and the policy holds it (P.policy.language).
    assert!(
        find(&window, "language-entry").is_none(),
        "the language is said in the User Context, not typed into a box on Prepare"
    );

    // **3** the picker: the prompt rows in pipeline order and nothing else — F0.7 has no policy form, so
    // the menu carries no non-prompt row. See `bench::picker_rows`.
    let picker = find(&window, "bench-picker")
        .expect("the bench has its row picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown of the rows");
    let rows = naivepost::bench::picker_rows();
    assert_eq!(rows.len(), naivepost::bench::ROWS.len());
    assert_eq!(rows[0].1, "User Context");

    // §1's own picture is this page with the DESCRIBE prompt open, so the shipped wording has to be
    // what the box draws when nothing on this machine overrides it.
    picker.set_selected(2);
    settle();
    assert_eq!(
        title.text().as_str(),
        naivepost::bench::title(&naivepost::bench::ROWS[2]),
        "the heading reads \u{2039}Describe\u{203a} prompt: {}",
        title.text()
    );
    // §6 badge 5: the picker itself says what the row it names does — the job it gets and what it
    // answers — so its tooltip is the SELECTED row's, read through `bench::tooltip` rather than a literal
    // (the Describe row embeds the frames-per-request constant in its sentence).
    assert_eq!(
        picker.tooltip_text().as_deref(),
        Some(naivepost::bench::tooltip(&naivepost::bench::ROWS[2]).as_str()),
        "choosing another row moved the tooltip with it"
    );
    assert_ne!(
        picker.tooltip_text().as_deref(),
        Some(naivepost::bench::tooltip(&naivepost::bench::ROWS[0]).as_str()),
        "and it is not still the first row's sentence stuck on the face"
    );
    let describe_shipped =
        naivepost::prompts::shipped_text("describe").expect("describe ships a file");
    assert_eq!(
        text_of(&buffer),
        describe_shipped,
        "the box holds the shipped describe wording"
    );
    assert_eq!(
        describe_shipped.lines().next().unwrap_or_default(),
        "You describe screen-recorded footage for a video editor.",
        "and it is the spec's own wording, first line and all"
    );
    assert!(
        !describe_shipped.is_empty(),
        "a fresh machine never opens on an empty box"
    );
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
    let saved = std::fs::read_to_string(settings_paths().prompt_file(key))
        .expect("the keystroke was stored");
    assert_eq!(saved, "cut on the sentence boundary");
    assert!(
        find(&window, "bench-mark").expect("mark").is_visible(),
        "✎ appears with the edit"
    );
    let reset = find(&window, "bench-reset")
        .expect("reset")
        .downcast::<gtk::Button>()
        .expect("Reset is a button");
    assert!(
        reset.is_visible() && reset.is_sensitive(),
        "Reset is live while an edit is held"
    );

    // **4** Reset restores the shipped wording and forgets this machine's file.
    reset.emit_clicked();
    assert!(
        naivepost::settings::read_prompt(&settings_paths(), key)
            .expect("the prompts folder is readable")
            .is_none(),
        "Reset leaves nothing behind"
    );
    assert_eq!(
        text_of(&buffer),
        naivepost::bench::shipped(&naivepost::bench::ROWS[4]),
        "the box shows the shipped wording after a reset, not an empty box: {}",
        text_of(&buffer)
    );
    assert!(
        !find(&window, "bench-mark").expect("mark").is_visible(),
        "the mark goes with the edit"
    );

    // Back to row 0: typing there edits the project's note and writes no file at all. A keystroke
    // changes only the box; §1 asks for storage, and `bench::store` is what stores it — so this
    // asserts that the prompts folder stayed empty of anything for row 0 while the mark and Reset
    // stayed off, which is exactly what a row with no file of its own looks like.
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
    assert!(
        !find(&window, "bench-mark").expect("mark").is_visible(),
        "no mark on row 0"
    );
    assert!(
        !find(&window, "bench-reset").expect("reset").is_visible(),
        "no Reset on row 0"
    );
}

/// §1's badges **2**–**5** on a machine that holds NO edit: the box opens on the wording this build
/// ships (not an empty box), and Reset — pressed through the widget — puts those same words back after
/// an edit and takes the ✎ with it. `check_readouts_and_reset` drives the same button too, but only
/// after typing: what nothing proved before is the OPENING state of a fresh machine, which is exactly
/// what §1's picture shows (the Describe prompt filling the box).
fn check_shipped_wording_and_reset(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();
    // A prompt row, chosen clean: row 6 is `captions`, which no sibling check of this round writes.
    let row = &naivepost::bench::ROWS[6];
    let paths = settings_paths();
    let _ = naivepost::settings::clear_prompt(&paths, row.key);
    let picker = find(&window, "bench-picker")
        .expect("the bench has its row picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown");
    picker.set_selected(6);
    settle();
    let buffer = find(&window, "bench-text")
        .expect("the bench has its text box")
        .downcast::<gtk::TextView>()
        .expect("the box is a text view")
        .buffer();
    let shipped = naivepost::prompts::shipped_text(row.key).expect("every prompt row ships a file");
    assert!(
        !shipped.trim().is_empty(),
        "{} ships wording, so a fresh machine opens on something",
        row.key
    );
    assert_eq!(
        text_of(&buffer),
        shipped,
        "with nothing stored on this machine the box draws the shipped wording"
    );
    assert_eq!(
        text_of(&buffer),
        naivepost::bench::text(&paths, &model, row),
        "the box and the rule behind it cannot disagree about what a row shows"
    );
    // The idle pair: no mark, no live Reset, and Reset says "unchanged" rather than promising a restore.
    assert!(
        !find(&window, "bench-mark").expect("mark").is_visible(),
        "no ✎ without an edit"
    );
    assert!(
        !find(&window, "bench-reset").expect("reset").is_visible(),
        "no live Reset either"
    );
    assert_eq!(
        naivepost::bench::reset_tip(row, false),
        naivepost::bench::UNCHANGED_TIP,
        "and the rule says so too"
    );
    // Type something: every keystroke is stored, and the mark and Reset come up with it.
    buffer.set_text("cut where the speaker pauses");
    settle();
    assert!(
        naivepost::bench::edited(&paths, row),
        "the keystroke reached this machine's prompts"
    );
    let saved = std::fs::read_to_string(paths.prompt_file(row.key)).expect("the edit is on disk");
    assert_eq!(
        saved.trim(),
        "cut where the speaker pauses",
        "and holds what was typed"
    );
    assert!(
        find(&window, "bench-mark").expect("mark").is_visible(),
        "the ✎ came up with it"
    );
    let reset = find(&window, "bench-reset")
        .expect("Reset is drawn")
        .downcast::<gtk::Button>()
        .expect("Reset is a button");
    assert!(
        reset.is_visible() && reset.is_sensitive(),
        "and Reset went live beside it"
    );
    assert_eq!(
        naivepost::bench::reset_tip(row, true),
        naivepost::bench::RESET_TIP,
        "while an edit is held the tooltip offers to put the built-in wording back"
    );
    // The press itself, fired the way the toolkit fires it.
    reset.emit_clicked();
    settle();
    assert_eq!(
        text_of(&buffer),
        shipped,
        "Reset gave the shipped words back, not an empty box"
    );
    assert!(
        !naivepost::bench::edited(&paths, row),
        "and cleared the override"
    );
    assert!(
        naivepost::settings::read_prompt(&paths, row.key)
            .expect("the prompts folder is readable")
            .is_none(),
        "nothing of this row's is left on the machine"
    );
    assert!(
        !find(&window, "bench-mark").expect("mark").is_visible(),
        "the ✎ went with it"
    );
    assert!(
        !find(&window, "bench-reset").expect("reset").is_visible(),
        "and so did Reset"
    );
    window.close();
    settle();
}

/// §1 badge **5** through the widget: a prompt row holds the wording the build ships, and Reset goes
/// back to THAT rather than to an empty box. The machine starts with no override anywhere.
#[test]
fn sec_04_prepare_1_screen_s_the_box_holds_the_shipped_wording_and_reset_returns_to_it() {
    let paths = settings_paths();
    let project = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let describe = naivepost::prompts::shipped_text("describe").expect("describe ships a file");
    assert_eq!(
        describe.lines().next().unwrap_or_default(),
        "You describe screen-recorded footage for a video editor.",
        "the shipped wording is the spec's own sentence"
    );
    let row = &naivepost::bench::ROWS[2];
    let _ = naivepost::settings::clear_prompt(&paths, row.key);
    assert_eq!(
        naivepost::bench::text(&paths, &project, row),
        describe,
        "with no edit on this machine the bench draws the shipped wording"
    );
    // Typing something else makes it this machine's edit; resetting gives the built-in words back.
    naivepost::bench::store(
        &paths,
        &mut naivepost::project::Project::default(),
        row,
        "mine",
    )
    .expect("storing an edit works");
    assert!(
        naivepost::bench::edited(&paths, row),
        "and the ✎ condition is met"
    );
    let restored = naivepost::bench::reset(&paths, row)
        .expect("reset answers")
        .expect("a prompt row has shipped wording to restore");
    assert_eq!(
        restored, describe,
        "Reset restores the shipped wording, not an empty box"
    );
    assert_eq!(
        naivepost::bench::text(&paths, &project, row),
        describe,
        "and the box reads that way again"
    );
}

#[test]
fn sec_04_prepare_1_screen_s_a_fresh_machine_opens_on_the_shipped_wording_and_reset_goes_back_to_it(
) {
    window_round();
}

#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_s_the_page_shows_the_bench_and_frame_controls(
) {
    window_round();
}

/// The window's own session — the state every row press writes through.
fn session_of(
    window: &adw::ApplicationWindow,
) -> std::rc::Rc<std::cell::RefCell<naivepost::project::Project>> {
    ui::session_handle(window)
}

/// The window's own settings folder, the one its bench writes into.
fn settings_paths() -> naivepost::settings::Paths {
    naivepost::settings::from_environment().expect("the test set XDG_CONFIG_HOME")
}

fn find(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    ui::find_source_widget(window, name)
}

fn text_of(buffer: &gtk::TextBuffer) -> String {
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, false).to_string()
}
