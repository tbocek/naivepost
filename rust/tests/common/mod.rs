//! Shared helpers for the integration tests. Each helper here had one byte-identical copy in every
//! test file that used it; they live once now, so a change to how a test reads the shell's status line
//! or walks the parameter catalogue is one edit rather than two dozen.
//!
//! A test binary that declares `mod common;` compiles this whole module, so a helper another file
//! does not call would trip `dead_code`. The `#![allow(dead_code)]` below is what keeps that from
//! becoming a warning attributed to whoever added the last `mod common;`.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::{
    cut::{Cut, Fx, Seg},
    edges::AlignedWord,
    project::Source,
    shell::Page,
    timeline::Recording,
    params, ui,
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
/// whole catalogue rather than one page (`tests/hands_off.rs`'s `row()` makes the same choice).
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
    let dir = std::env::temp_dir().join(format!(
        "naivepost-test-{}-{tag}",
        std::process::id()
    ));
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
    std::env::temp_dir().join(format!(
        "naivepost-test-{}-{tag}",
        std::process::id()
    ))
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

/// Let the main context run what the widget emissions queued. Bounded and NON-blocking:
/// `iteration(false)` returns immediately when nothing is ready, where a blocking wait would park the
/// test thread forever. Three files keep their own `settle` because theirs pumps `while pending()`
/// instead of a fixed 64 turns — converting those would change how many turns run, not just where the
/// code lives.
pub fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
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
    Seg { s, e, ..Default::default() }
}

/// The same clip with the camera layer explicitly down, which is how the lane tests state "no drawing".
pub fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
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
    vec![Recording { base: "session-tape".to_string(), start: 0.0, end: 120.0 }]
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
    AlignedWord { word: text.into(), s, e }
}

/// The parameter row with this id, from whichever page catalogues it — `all_rows()` first, so an id that
/// appears on two pages is caught by those rows' own uniqueness check rather than by this one.
pub fn anywhere(id: &str) -> params::Param {
    let found = all_rows().into_iter().filter(|row| row.id == id).collect::<Vec<_>>();
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
    assert!((got - want).abs() < 1e-9, "{what}: got {got}, want {want} (±1e-9)");
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
    naivepost::project::Source { path: path.into(), footage, narrator, ..Default::default() }
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
    naivepost::narration::Entry { s, e, text: text.to_string(), ..Default::default() }
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

/// A project folder under a throwaway root: (root, tree). The root is removed on the NEXT call with
/// the same tag, so a case that fails on an assertion never leaves it behind.
pub fn tree_in(tag: &str) -> (std::path::PathBuf, naivepost::layout::Tree) {
    let root = temp_root(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, naivepost::layout::Tree::new(&dir).unwrap())
}

/// A throwaway root for one test's own files, removed before the run rather than at the end of each
/// test: a case that fails on an assertion never reaches its own cleanup, and a folder left in /tmp is
/// a folder the next reader has to explain.
pub fn temp_root(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("np-tree-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An envelope of `total` seconds, silent at `room` except in these spans.
pub fn wave(total: f64, spans: &[(f64, f64, u8)], hz: f64, room: u8) -> naivepost::wave::Wave {
    let mut peaks = vec![room; (total * hz) as usize];
    for (start, end, level) in spans {
        peaks[(start * hz) as usize..(end * hz) as usize].fill(*level);
    }
    naivepost::wave::Wave { hz, chans: vec![peaks] }
}
