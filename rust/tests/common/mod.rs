//! Shared helpers for the integration tests. Each helper here had one byte-identical copy in every
//! test file that used it; they live once now, so a change to how a test reads the shell's status line
//! or walks the parameter catalogue is one edit rather than two dozen.
//!
//! A test binary that declares `mod common;` compiles this whole module, so a helper another file
//! does not call would trip `dead_code`. The `#![allow(dead_code)]` below is what keeps that from
//! becoming a warning attributed to whoever added the last `mod common;`.

use adw::prelude::*;
use gtk4 as gtk;
pub mod fit_geometry;
pub mod real_input;
use naivepost::{
    cut::{Cut, Fx, Seg},
    edges::AlignedWord,
    params,
    project::Source,
    shell::Page,
    timeline::Recording,
    ui,
};

/// The shell's status line as plain text — where the app says what a press did, so most widget tests
/// end by reading this rather than reaching into the widget that was pressed.
pub fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

/// Every list that carries rows, chained across all five pages so uniqueness is checked against the
/// whole catalogue rather than one page (`tests/suite/hands_off.rs`'s `row()` makes the same choice).
pub fn all_rows() -> Vec<params::Param> {
    params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .collect()
}

/// A directory under the temp dir that no other test run shares; `tag` says which test made it.
/// The copy every JSON/format test file carried, with the per-file prefix replaced by one, so two
/// runs never read each other's leftovers and a leftover is attributable to one suite.
pub fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A `temp_dir` that also EXISTS on return, and is re-exported under the name a caller's tests already
/// use it by (`use common::temp_dir_created as temp_dir;`).
pub fn temp_dir_created(tag: &str) -> std::path::PathBuf {
    let dir = temp_dir(tag);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// [`temp_dir`] WITHOUT the `remove_dir_all` first — for the rare test that reads a folder another
/// check just left behind and expects to find it.
pub fn temp_dir_noclean(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("naivepost-test-{}-{tag}", std::process::id()))
}

/// [`temp_dir`] with `tag` FIRST in the name and a per-call counter after it, so a test that asks for
/// the same tag twice still gets a distinct folder, and the folder EXISTS on return.
pub fn temp_dir_tagged(tag: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "naivepost-{tag}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// The project every test loads: `fixtures/demo.naivepost`, resolved from the crate root. `env!` is
/// expanded where this module is compiled, which is the same `rust/` directory for every test binary,
/// so moving the 42 copies here leaves each file's path exactly where it was.
pub fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// The demo project copied into `into`, so a test can change it without touching the fixture on disk:
/// test files run side by side, and one that writes into `fixtures/` changes what the others load.
pub fn copy_fixture(into: &std::path::Path) {
    copy_dir(&fixture_dir(), into);
}

fn copy_dir(from: &std::path::Path, into: &std::path::Path) {
    std::fs::create_dir_all(into).expect("folder exists");
    for entry in std::fs::read_dir(from).expect("folder readable") {
        let entry = entry.expect("an entry");
        let target = into.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("file copied");
        }
    }
}

/// How long a live watchdog check lets the watcher wait before it calls a hang: shorter than §1's 3 s,
/// since a suite may not sleep three seconds; the door is the program's own.
pub const WATCHDOG_TEST_LIMIT: std::time::Duration = std::time::Duration::from_millis(300);

/// A settings folder of the caller's own for a watchdog check, installed as `$XDG_CONFIG_HOME` and
/// `$XDG_DATA_HOME`. Both are process-wide and the watcher starts once per process, so a check that needs
/// a watcher of its own sits in a test file of its own. The `config_dir` is made up front:
/// `settings::write_hang_dump` only forces 0700 on a folder it creates itself.
pub fn watchdog_paths(tag: &str) -> naivepost::settings::Paths {
    let dir = temp_dir_tagged(&format!("sec03wd-{tag}"));
    std::env::set_var("XDG_CONFIG_HOME", &dir);
    std::env::set_var("XDG_DATA_HOME", &dir);
    naivepost::settings::from_environment().expect("an absolute XDG_CONFIG_HOME is a config home")
}

/// Every `hang-*` file in `paths.config_dir`, sorted. Empty while nothing has hung.
pub fn hang_dumps(paths: &naivepost::settings::Paths) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(&paths.config_dir) else {
        return Vec::new();
    };
    let mut files: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with("hang-"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

/// Let the main context run what the widget emissions queued. Bounded and NON-blocking:
/// `iteration(false)` returns immediately when nothing is ready, where a blocking wait would park the
/// test thread forever. Three files keep their own `settle` because theirs pumps `while pending()`
/// instead of a fixed 64 turns — converting those would change how many turns run, not just where the
/// code lives.
pub fn settle() {
    // A Settings Test runs on a thread of its own and lands its verdict through the loop: a settle after a
    // press waits for it, so "pressed, then settled" still means "answered". Nothing in flight returns at once.
    assert!(
        naivepost::ui::settings::wait_for_tests(WAIT_LIMIT),
        "a Settings Test did not answer within {WAIT_LIMIT:?}"
    );
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

/// How long [`wait_until`] waits before it fails. Generous on purpose: on a busy machine, or with test
/// programs running side by side, a timer, a thread or a child process takes longer than on a quiet one.
pub const WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Runs the main loop until `done` holds, and fails after [`WAIT_LIMIT`] naming `what` never happened.
/// The wait for anything that happens later (a timer, a thread, a child process, a file another part
/// writes): a fixed number of turns or [`settle`]'s "until nothing is pending" runs out before such work
/// is done on a busy machine.
pub fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + WAIT_LIMIT;
    while !done() {
        assert!(
            std::time::Instant::now() < deadline,
            "waited {WAIT_LIMIT:?}, and {what} never happened"
        );
        if !context.iteration(false) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// The window log's last line other than the two background savers': the autosave's "saved — autosave"
/// on its 2 s beat and the settings dialog's "settings saved to …" 600 ms after a keystroke. Both land on
/// real time, so on a busy machine they sit after the line a check is about.
pub fn last_log() -> Option<String> {
    ui::window_logs().into_iter().rev().find(|line| {
        line != naivepost::project_autosave::SAVED_STATUS && !line.contains("settings saved to ")
    })
}

/// Writes an executable script (a fake player, a fake ffmpeg) by way of a child process, so this process
/// never holds a write handle on it. Tests run on parallel threads: a fork on another thread inherits an
/// open handle for a moment, and running the script then fails with "Text file busy".
pub fn write_script(
    path: impl AsRef<std::path::Path>,
    body: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    use std::io::Write;
    let path = path.as_ref();
    let mut child = std::process::Command::new("sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(body.as_ref())?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "writing {} failed: {status}",
            path.display()
        )))
    }
}

/// Moves this test program into a working directory of its own, once. The program keeps its working
/// copy in `session.naivepost` beside the working directory, and every test program starts in `rust/`:
/// running side by side, they would read and overwrite one shared `cut.json`.
pub fn pin_private_cwd() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_current_dir(temp_dir_created("cwd")).expect("a private working directory");
    });
}

thread_local! {
    /// Strong handle on the window last built, so the next check drops it first: a closed GTK window is not
    /// destroyed and its names stay parented, which would send a lookup to the wrong tree.
    pub static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

/// Remember the window a check is about to work in, so the next check can release it first.
pub fn hold_last_window(window: adw::ApplicationWindow) {
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = Some(window));
}

/// Close the window the previous check left standing, then let the close's own emissions run.
pub fn release_last_window() {
    LAST_WINDOW.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            old.close();
        }
    });
    settle();
}

/// Find a widget by the name its builder gave it, walking the whole tree below the window. Several
/// widget tests need this and none of them reached for `gtk::WidgetExt::child` chains, so the walk lives
/// here once; the files whose version differs keep their own.
pub fn find_named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        for child in widget.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else {
                continue;
            };
            if let Some(found) = walk(child, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

/// Find a widget by the name its builder gave it, walking the whole tree below the window. Several
/// widget tests need this and none of them reached for `gtk::WidgetExt::child` chains, so the walk lives
/// here once; the three files whose version differs keep their own.
pub fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

/// One kept clip of the cut. The plain form — every other field at its default.
pub fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

/// The same clip on an explicit camera row — the shape the lane and verb tests state a shot with.
pub fn seg_cam(s: f64, e: f64, cam: i32) -> naivepost::cut::Seg {
    naivepost::cut::Seg {
        s,
        e,
        cam,
        ..Default::default()
    }
}

/// Held while a scenario re-points the process-wide settings env and `cwd`. Poisoned means a check
/// panicked mid-scenario; the env is still ours to set, so the guard is taken anyway rather than
/// unwrapping into a second panic.
pub fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

/// The same clip with the camera layer explicitly down, which is how the lane tests state "no drawing".
pub fn clip(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        cam: 0,
        ..Default::default()
    }
}

/// One effect on the lane — every field but kind, start and length at its default.
pub fn fx(kind: &str, t: f64, dur: f64) -> Fx {
    Fx {
        kind: kind.into(),
        t,
        dur,
        ..Default::default()
    }
}

/// The session filmed 0–120 as one take.
pub fn tape() -> Vec<Recording> {
    vec![Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]
}

/// A footage source by path — the shape §F1's gates and joins read, with no narrator and no split voice.
pub fn footage(path: &str) -> Source {
    Source {
        path: path.to_string(),
        footage: true,
        narrator: 0,
        sepvoice: false,
        tracks: vec![],
    }
}

/// Fire a named button the way a press does: the `clicked` signal, then let the queued handler run.
pub fn click(window: &adw::ApplicationWindow, name: &str) {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the Cut page drew no clickable {name}"))
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// Three clips of 20 s each: every sum built on them is exact in binary, so a wrong total cannot hide
/// inside a rounding error.
pub fn three_clips() -> Vec<(u32, f64)> {
    vec![(1u32, 20.0), (2u32, 20.0), (3u32, 20.0)]
}

/// Three kept runs of ten seconds each — clip 1 = 0–10, clip 2 = 10–20, clip 3 = 20–30 — so every second a
/// pass writes down is checkable by hand.
pub fn three_clips_cut() -> naivepost::cut::Cut {
    naivepost::cut::Cut {
        segs: vec![
            naivepost::cut::Seg {
                s: 0.0,
                e: 10.0,
                cam: 0,
                ..Default::default()
            },
            naivepost::cut::Seg {
                s: 10.0,
                e: 20.0,
                cam: 0,
                ..Default::default()
            },
            naivepost::cut::Seg {
                s: 20.0,
                e: 30.0,
                cam: 0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}
/// A parameter row from the PREPARE and CUT pages only — where a test wants to prove an id is catalogued
/// on exactly one of those two lists rather than anywhere in the catalogue.
pub fn row(id: &str) -> params::Param {
    let mut found = params::prepare()
        .into_iter()
        .chain(params::cut())
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
    found.pop().unwrap()
}

/// The Cut page's source widget by name, panicking with the page's own words when it was never drawn.
pub fn widget(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::find_source_widget(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"))
}

/// That widget narrowed to an editable entry.
pub fn entry(window: &adw::ApplicationWindow, name: &str) -> gtk::Entry {
    widget(window, name)
        .downcast::<gtk::Entry>()
        .unwrap_or_else(|_| panic!("`{name}` is an Entry"))
}

/// A window sitting on the Cut page with `seeded` as its review cut and history open over it — the shape
/// every lane/effect widget test starts from, so the seeding order lives once.
pub fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// One aligned word, as the aligner's file prints it.
pub fn word(text: &str, s: f64, e: f64) -> AlignedWord {
    AlignedWord {
        word: text.into(),
        s,
        e,
    }
}

/// The parameter row with this id, from whichever page catalogues it — `all_rows()` first, so an id that
/// appears on two pages is caught by those rows' own uniqueness check rather than by this one.
pub fn anywhere(id: &str) -> params::Param {
    let found = all_rows()
        .into_iter()
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert!(!found.is_empty(), "{id} catalogued nowhere");
    found.into_iter().next().unwrap()
}

/// Floats from arithmetic, never `assert_eq!` on a difference. One copy for the seven files whose local
/// version read a file-local `const EPS: f64 = 1e-9`; the tolerance is the constant below now.
pub const EPS: f64 = 1e-9;

/// [`EPS`] spelled out where a test wants the bound in its own words.
pub fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < EPS, "{asked}: got {got}, want {want}");
}

/// The same check with the tolerance in the message, which is how the floor tests print it.
pub fn assert_close_nano(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: got {got}, want {want} (±1e-9)"
    );
}

/// A Cut-page step button by name, panicking with the page's own words when it was never drawn.
pub fn cut_button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"))
}

/// [`cut_button`] where the file's panic says "carries" rather than "drew no".
pub fn cut_button_carries(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page carries {name}"))
}

/// A Narrate-page widget narrowed to a button, panicking with the page's own words.
pub fn narrate_button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .unwrap_or_else(|| panic!("the Narrate page carries {name}"))
}

/// A project folder inside a throwaway root.
pub fn project_tree(tag: &str) -> (std::path::PathBuf, naivepost::layout::Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, naivepost::layout::Tree::new(&dir).unwrap())
}

/// A session project with one footage row, the shape the produce flows start from.
pub fn produce_project() -> naivepost::project::Project {
    naivepost::project::Project {
        sources: vec![naivepost::project::Source {
            path: "/media/take.mp4".into(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// A project file's text — the two files that read the whole tree's own file.
pub fn read_tree(t: &naivepost::layout::Tree) -> String {
    std::fs::read_to_string(t.project_file()).unwrap()
}

/// A file's text as written, for the sidecar tests.
pub fn read_file_text(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// A source row as §B stores it.
pub fn source_row(path: &str, footage: bool, narrator: u32) -> naivepost::project::Source {
    naivepost::project::Source {
        path: path.into(),
        footage,
        narrator,
        ..Default::default()
    }
}

/// One session-timeline line, as `textfmt` prints it.
pub fn session_row(
    start: f64,
    end: f64,
    source: &str,
    who: &str,
    text: &str,
) -> naivepost::textfmt::SessionLine {
    naivepost::textfmt::SessionLine {
        start,
        end,
        source: source.into(),
        who: who.into(),
        text: text.into(),
    }
}

/// A cue sheet holding only the ticked languages: a TEST seam for a check that owns the settings but not the
/// cut. The program's own door is `ui::produce_snapshot`, which fills `languages` and `session_language` from
/// the live project, because the cues themselves are cut from the plan.
pub fn produce_cues_for_test(languages: Vec<String>) -> naivepost::produce_exec::Cues {
    naivepost::produce_exec::Cues {
        languages,
        ..Default::default()
    }
}

/// A Produce row's spelled label, from §10's catalogue.
pub fn produce_row(id: &str) -> String {
    params::produce()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{} is catalogued in §10's Produce rows", id))
        .spelled
}

/// A narration line with only its span and text.
pub fn entry_span(s: f64, e: f64, text: &str) -> naivepost::narration::Entry {
    naivepost::narration::Entry {
        s,
        e,
        text: text.to_string(),
        ..Default::default()
    }
}

/// [`entry_span`] built through `default()` so every other field is whatever the type starts at.
pub fn entry_span_default(s: f64, e: f64, text: &str) -> naivepost::narration::Entry {
    let mut line = naivepost::narration::Entry::default();
    line.s = s;
    line.e = e;
    line.text = text.to_string();
    line
}

/// A narration line with its spoken position set as well.
pub fn entry_at(s: f64, e: f64, at: f64, text: &str) -> naivepost::narration::Entry {
    let mut line = naivepost::narration::Entry::default();
    line.s = s;
    line.e = e;
    line.at = at;
    line.text = text.to_string();
    line
}

/// A window sitting on the Cut page with `seeded` as its review cut and history open over it, without the
/// effects-lane repaint [`cut_page`] does — the shape the form tests seed to.
pub fn cut_page_unpainted(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// [`cut_page_unpainted`] with one settle before the seeding as well — the drawing-flow tests' order.
pub fn cut_page_settled_first(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// A readout label's text, by the name its builder gave it.
pub fn label_text(window: &adw::ApplicationWindow, name: &str) -> String {
    widget(window, name)
        .downcast::<gtk::Label>()
        .unwrap_or_else(|_| panic!("`{name}` is a Label"))
        .text()
        .to_string()
}

/// Find a named widget inside a subtree we already hold (a form row), without walking the whole window.
pub fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if let Some(found) = find_in(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

/// A fake audio.cpp on loopback: `/health` answers 200 and `/v1/models` names the ASR and diarization
/// models Prepare needs. §S5 is a real door now — the ▶ press asks the server before any stage runs —
/// so a test that presses ▶ has to have something to answer.
pub fn fake_audio_server() -> String {
    use std::io::{BufRead, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                }
                let payload = if line.contains("/v1/models") {
                    r#"{"data":[{"id":"nemotron-asr","task":"asr"},{"id":"sortformer-diar","task":"diar"}]}"#
                } else {
                    "{}"
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// A project folder under a throwaway root of its own: (root, tree).
pub fn tree_in(tag: &str) -> (std::path::PathBuf, naivepost::layout::Tree) {
    let root = temp_root(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, naivepost::layout::Tree::new(&dir).unwrap())
}

/// A throwaway root for one test's own files, distinct per call: tests of one program run on parallel
/// threads, and a root shared by tag was wiped by a sibling while a test still wrote into it.
pub fn temp_root(tag: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("np-tree-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The Cut page opened on the fixture with an empty history, for the tests that stage or read the
/// effect menu rather than drive a seeded cut. Three test files each carried one byte-identical copy.
pub fn cut_page_empty(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, &Cut::default());
    settle();
    window
}

/// An envelope of `total` seconds, silent at `room` except in these spans.
/// What the shared [`serve`] read off one request, and answered.
///
/// The four harnesses that had their own copy spelled this identically (only the doc comment on
/// `reply` differed), so it lives once here. `reply` starts empty: the caller that knows what it
/// scripted fills it in, which is how a test asserts the exact bytes a leg was handed.
pub struct Seen {
    pub method: String,
    pub path: String,
    pub body: String,
    /// The exact bytes this request was answered with, so a test can assert what the pass was
    /// handed rather than only what it sent.
    pub reply: String,
}

/// Read one HTTP request off `stream`, answer it with `body`, and return what was seen.
///
/// Shared by the accept thread and by a harness's own poll, so both halves put identical bytes on
/// the wire. Returns `None` when the socket dies before a request line arrives.
pub fn serve(mut stream: std::net::TcpStream, body: &str) -> Option<Seen> {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            length = rest.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let seen = Seen {
        method: parts.first().copied().unwrap_or_default().to_string(),
        path: parts.get(1).copied().unwrap_or_default().to_string(),
        body: String::from_utf8_lossy(&payload).to_string(),
        reply: String::new(),
    };
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    Some(seen)
}

/// One chat ask read off the wire: the endpoint it arrived at and the body it carried.
///
/// `path` is recorded because the endpoint matters to a wire test, and is kept in the struct so one
/// ask is one value; a file that never reads it takes the `#[allow(dead_code)]` with it.
#[derive(Debug, Clone)]
pub struct ChatAsk {
    #[allow(dead_code)]
    pub path: String,
    pub body: String,
}

/// Read one HTTP request off `stream`, answer it with `answer`, and return the ask.
pub fn serve_chat(stream: std::net::TcpStream, answer: &str) -> Option<ChatAsk> {
    serve_chat_with(stream, &|_body: &str| answer.to_string())
}

/// [`serve_chat`] with the answer computed from the request's body: a fake that answers what it was asked.
pub fn serve_chat_with(
    mut stream: std::net::TcpStream,
    answer_for: &dyn Fn(&str) -> String,
) -> Option<ChatAsk> {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header
                .split(':')
                .nth(1)
                .unwrap_or("0")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let body = String::from_utf8_lossy(&body).into_owned();
    let payload = answer_for(&body);
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(ChatAsk { path, body })
}

/// Read one HTTP request off `stream`, answer it with `answer`, and return the body it carried.
pub fn serve_body(mut stream: std::net::TcpStream, answer: &str) -> Option<String> {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header
                .split(':')
                .nth(1)
                .unwrap_or("0")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let payload = answer.to_string();
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(String::from_utf8_lossy(&body).into_owned())
}

/// One GTK application for this test binary: `gio` refuses a second `run_with_args`, so every widget
/// test walks its own checks inside this single main loop. `checks` runs once, then the app quits; a
/// second call from the same binary finds `ONCE` used and returns with the checks already recorded.
pub fn one_app_round(checks: impl FnOnce(&adw::Application) + 'static) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let checks = std::rc::Rc::new(std::cell::RefCell::new(Some(checks)));
    ONCE.call_once(move || {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let take = checks.clone();
        // `connect_activate` wants an `Fn`, so the one-shot lives in a cell that empties itself.
        app.connect_activate(move |app| {
            if let Some(round) = take.borrow_mut().take() {
                round(app);
            }
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

/// The session folder of a temp root, created: what a Produce test points the app at.
///
/// Seven files spelled this identically; `Tree::new` is what makes the folder a project.
pub fn session_tree_of(root: &std::path::Path) -> naivepost::layout::Tree {
    let dir = naivepost::startup::session_dir(root);
    std::fs::create_dir_all(&dir).expect("session folder created");
    naivepost::layout::Tree::new(&dir).expect("the session folder is a project tree")
}

/// A Cut-page window over the shipped fixture, with the footage row the Cut tab needs asserted.
///
/// Two widget files spelled this identically. The assert is part of the helper on purpose: without a
/// footage row `shell::lock` keeps the Cut tab closed and such a test would drive the wrong page.
pub fn cut_window_with_footage(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock) and this would \
         test the wrong page"
    );
    let window = ui::build_window(app, &model, "Prepare");
    size_to_minimum(&window);
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(
        ui::state(&window).page,
        Page::Cut,
        "the Cut page is showing"
    );
    wait_for_stable_layout(&window);
    window
}

/// Give a window §1's minimum size for real and wait until GTK has laid it out at that size.
///
/// The window manager-less X server sizes the window to its default size, and the size request moves the X
/// window; nothing here allocates boxes by hand. A box forced with `size_allocate` is undone at GTK's next
/// frame, so a check read it or the real layout depending on timing, and a layout bug the program has (a
/// divider that never opens) passed in the tests and showed in the shots.
pub fn size_to_minimum(window: &adw::ApplicationWindow) {
    let (w, h) = (
        naivepost::shell::WINDOW_MIN_W,
        naivepost::shell::WINDOW_MIN_H,
    );
    window.set_default_size(w, h);
    window.present();
    wait_for_stable_layout(window);
    // The X window is checked, not `window.width()`: without a window manager GTK keeps a 5 px resize border
    // inside it on every side, so the content box of a 1240×900 window is 1230×890.
    let surface = window.surface().expect("a presented window has a surface");
    assert_eq!(
        (surface.width(), surface.height()),
        (w, h),
        "the window laid out at another size than §1's minimum: its content asks for more"
    );
}

/// How many paints in a row must show every widget in the same place before the layout counts as settled.
/// Measured on Xvfb: a fresh window settles in two to three frames, a paned's resting ask (an idle) included.
const STABLE_PAINTS: u32 = 3;

/// Runs the main loop until the window has been painted [`STABLE_PAINTS`] times in a row with every widget's
/// box unchanged: the layout GTK settled on, the one a person would see, read after its frames rather than
/// guessed with a number of loop turns. Fails after [`WAIT_LIMIT`] when the layout keeps moving.
pub fn wait_for_stable_layout(window: &adw::ApplicationWindow) {
    let clock = window
        .frame_clock()
        .expect("a presented window has a frame clock");
    let paints = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let counter = paints.clone();
    let handler = clock.connect_after_paint(move |_| counter.set(counter.get() + 1));
    let (mut seen, mut same, mut last) = (0, 0, None);
    window.queue_draw();
    wait_until("the window's layout came to rest", || {
        if paints.get() == seen {
            return false;
        }
        seen = paints.get();
        let now = boxes(window.upcast_ref(), window.upcast_ref(), Vec::new());
        if last.as_ref() == Some(&now) {
            same += 1;
        } else {
            same = 1;
            last = Some(now);
        }
        // A frame is only painted when something asks for one; ask, so a layout at rest is seen to stay so.
        window.queue_draw();
        same >= STABLE_PAINTS
    });
    clock.disconnect(handler);
}

/// Every mapped widget's box in window coordinates, in tree order.
fn boxes(root: &gtk::Widget, widget: &gtk::Widget, mut out: Vec<[i32; 4]>) -> Vec<[i32; 4]> {
    if widget.is_mapped() {
        if let Some(r) = widget.compute_bounds(root) {
            out.push([r.x(), r.y(), r.width(), r.height()].map(|v| v.round() as i32));
        }
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        out = boxes(root, &c, out);
        child = c.next_sibling();
    }
    out
}

/// One narration entry starting halfway through its span — the shape a frozen preview reads back.
pub fn entry_at_mid(s: f64, e: f64, text: &str) -> naivepost::narration::Entry {
    naivepost::narration::Entry {
        s,
        e,
        at: 0.5,
        text: text.into(),
        ..Default::default()
    }
}

/// An envelope of `total` seconds, silent at `room` except in these spans.
pub fn wave(total: f64, spans: &[(f64, f64, u8)], hz: f64, room: u8) -> naivepost::wave::Wave {
    let mut peaks = vec![room; (total * hz) as usize];
    for (start, end, level) in spans {
        peaks[(start * hz) as usize..(end * hz) as usize].fill(*level);
    }
    naivepost::wave::Wave {
        hz,
        chans: vec![peaks],
    }
}

/// One model tool call with no result yet — the shape every tool-loop test hands to `tool_loop`.
pub fn call(name: &str, args: &str) -> naivepost::exchanges::ToolCall {
    naivepost::exchanges::ToolCall {
        name: name.to_string(),
        args: args.to_string(),
        result: String::new(),
    }
}

/// The Firefox this machine has, found the way the program finds it with an empty box. The web checks are
/// about the real browser, so a machine without one fails here rather than passing on a stand-in.
pub fn firefox() -> std::path::PathBuf {
    naivepost::web_browser::firefox_path("")
        .expect("a firefox to drive (firefox-esr in the build container)")
}

/// A local web server for the browser checks: pages by path (changeable while it runs), `/slow` answering
/// nothing for 30 s, anything else 404. Each connection is served on its own thread, so a browser's parallel
/// requests never queue behind each other.
pub struct Site {
    base: String,
    pages: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}

impl Site {
    pub fn start() -> Site {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a local port");
        let base = format!("http://{}", listener.local_addr().expect("its address"));
        let pages: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>> = Default::default();
        let served = std::sync::Arc::clone(&pages);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let pages = std::sync::Arc::clone(&served);
                std::thread::spawn(move || {
                    let Ok(clone) = stream.try_clone() else {
                        return;
                    };
                    let mut reader = BufReader::new(clone);
                    let mut request = String::new();
                    if reader.read_line(&mut request).is_err() {
                        return;
                    }
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).map(|n| n == 0).unwrap_or(true)
                            || line == "\r\n"
                        {
                            break;
                        }
                    }
                    if path == "/slow" {
                        std::thread::sleep(std::time::Duration::from_secs(30));
                        return;
                    }
                    let route = path.split('?').next().unwrap_or("/").to_string();
                    let found = pages
                        .lock()
                        .expect("the page table")
                        .iter()
                        .find(|(at, _)| *at == route)
                        .map(|(_, body)| body.clone());
                    let (status, body) = match found {
                        Some(body) => ("200 OK", body),
                        None => ("404 Not Found", String::new()),
                    };
                    let mut stream = stream;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        Site { base, pages }
    }

    /// Serve `body` at `path` from now on.
    pub fn set(&self, path: &str, body: impl Into<String>) {
        let mut pages = self.pages.lock().expect("the page table");
        pages.retain(|(at, _)| at != path);
        pages.push((path.to_string(), body.into()));
    }

    /// The address of `path` on this server.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

/// `stop_legs`' describe flags (in the stage, a restart armed) are process-wide, so the tests that arm, take
/// or read them take turns on this lock instead of racing each other inside one test binary.
pub fn describe_flags_turn() -> std::sync::MutexGuard<'static, ()> {
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Let every worker the window handed work to land (§03-shell §1: readouts, probes, copies run off the window
/// thread), so what a test reads next is the landed answer.
pub fn settle_workers() {
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(60)),
        "the window's workers did not land within a minute"
    );
}

/// A real, silent mono 16 kHz wav of `seconds`: what a stub ffmpeg "decodes" to, and what a test plants as a
/// recording's `voice16k.wav`, because §F1.11's envelope reader parses the file for real.
pub fn silent_wav_bytes(seconds: f64) -> Vec<u8> {
    let data = vec![0u8; (seconds * 16_000.0) as usize * 2];
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // one channel
    out.extend_from_slice(&16_000u32.to_le_bytes());
    out.extend_from_slice(&32_000u32.to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// What a model that follows the job answers a fix ask with: the block's own rows (read off the request, as
/// the model reads them), each text replaced by "respelled line n". Any other ask (one with no rows) gets an
/// empty answer.
pub fn respelled_rows_answer(body: &str) -> String {
    let sent: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let user: String = sent["messages"]
        .as_array()
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|message| message["role"].as_str() == Some("user"))
        })
        .and_then(|message| message["content"].as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<&str>>()
                .join("\n")
        })
        .unwrap_or_default();
    let rows: Vec<String> = naivepost::fix_transcripts::answered_rows(&user)
        .iter()
        .enumerate()
        .map(|(index, row)| {
            format!(
                "{:.2}\t{:.2}\t{}\trespelled line {}",
                row.start,
                row.end,
                row.speaker,
                index + 1
            )
        })
        .collect();
    serde_json::json!({ "choices": [ { "message": { "content": rows.join("\n") } } ] }).to_string()
}
