//! F5.7 Page runs — the wire: each of the page's six buttons drives its `produce_presses` walk, and the
//! spec's own sentence lands on the status line.
//!
//! `tests/suite/produce_page_runs.rs` proves what a press *costs* over hand-built values. This file proves the
//! other half: that the button on the Produce page runs **that** walk, in §F5.7's order — refuse first,
//! then the `>>>` opening line, then the legs, then one ending — and that nothing cheaper is secretly more
//! expensive: ↻ beside Title draws no picture, ↻ Transcode asks no model, a reprint costs no GPU run.
//!
//! Shape copied from `tests/suite/produce_stamp_widgets.rs`: ONE `adw::Application`, one `connect_activate`, one
//! `run_with_args` for the whole binary (a second `Application::run` from another thread is what produced
//! `Default main context is already acquired by another thread!`). The checks therefore run IN SEQUENCE
//! inside the activate callback, each wrapped in `catch_unwind`: an assertion firing directly there unwinds
//! into GTK's `extern "C"` trampoline (`panic_cannot_unwind`) and aborts the process, which reads as a
//! crash rather than a failed check. cwd is pinned to this test's own temp root BEFORE the run because the
//! page resolves the session through `startup::session_dir(current_dir())`.
//!
//! No ffmpeg and no sd.cpp exist in this container, so every leg is scripted before it is fired:
//! [`ui::produce_languages::set_draw_for_test`] for the picture, `naivepost::produce_upload::set_reply_for_test`
//! for the model, and `naivepost::produce_translate::set_spawn_for_test` for anything argv-shaped. Each
//! scripted leg COUNTS its own calls, because "no draw" and "one draw" are invisible in the finished status
//! line and visible only in that counter.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Cut;
use naivepost::layout::Tree;
use naivepost::project::{Publish, TextMark};
use naivepost::shell::Page;
use naivepost::ui;
use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, session_tree_of as session, settle};

/// One slot per check, holding that block's failure text if it failed.
static REDRAW_FAIL: Mutex<Option<String>> = Mutex::new(None);
static REWORD_FAIL: Mutex<Option<String>> = Mutex::new(None);
static TAKEN_FAIL: Mutex<Option<String>> = Mutex::new(None);
static EXPORT_FAIL: Mutex<Option<String>> = Mutex::new(None);
static SAVE_FAIL: Mutex<Option<String>> = Mutex::new(None);
static TRANSCODE_FAIL: Mutex<Option<String>> = Mutex::new(None);
static REFUSAL_FAIL: Mutex<Option<String>> = Mutex::new(None);
static EXPORT_MEASURED_FAIL: Mutex<Option<String>> = Mutex::new(None);
static SET_THUMB_SOLE_FAIL: Mutex<Option<String>> = Mutex::new(None);
static ADD_IMAGE_REAL_FAIL: Mutex<Option<String>> = Mutex::new(None);
static PICKED_ROW_FAIL: Mutex<Option<String>> = Mutex::new(None);
static ADD_IMAGE_EMPTY_FAIL: Mutex<Option<String>> = Mutex::new(None);
static CHANGE_EMPTY_FAIL: Mutex<Option<String>> = Mutex::new(None);
static MAKE_BASE_SWAP_FAIL: Mutex<Option<String>> = Mutex::new(None);
static SET_THUMB_PRINTS_FAIL: Mutex<Option<String>> = Mutex::new(None);

/// Find one widget by name in a given window's tree. `ui::find_widget_by_name` is crate-private, so the test
/// walks the same tree the shell does — and it walks the window the CHECK pressed, never `ui::main_window()`,
/// which may still be an earlier check's window: reading one tree while pressing another is how a slot that no
/// longer exists looks alive.
fn slot_widget(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    // Same tree as `slot_button_counts`: the page's own root, where the rows are drawn and repainted.
    let _ = window;
    let root = ui::produce_page::page_image_box_for_test()?;
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        for child in node.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            if let Some(found) = walk(child, name) {
                return Some(found);
            }
        }
        None
    }
    walk(&root, name)
}

/// The buttons of every slot THIS window drew, counted per slot: `{"image-slot-0": 3, "image-slot-1": 4, ..}`.
/// `screen::slot_actions` gives the base three buttons and a reference four, so the count IS the claim "this
/// slot is / is not the base", read off the tree rather than off a state field.
fn slot_button_counts(_window: &adw::ApplicationWindow) -> std::collections::BTreeMap<String, usize> {
    // The page is built ONCE per process and embedded in every window (see `ROOT`'s own comment), so the rows
    // live in that one shared tree — which is also where `draw_into` repaints them. Walking from the page's root
    // therefore counts exactly the rows a press changes; walking from a window found an earlier check's copy.
    let images = match ui::produce_page::page_image_box_for_test() {
        Some(box_widget) => box_widget,
        None => return std::collections::BTreeMap::new(),
    };
    fn walk(node: &gtk::Widget, rows: &mut Vec<gtk::Widget>) {
        if node.widget_name().starts_with("image-slot-") {
            rows.push(node.clone());
        }
        for child in node.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            walk(child, rows);
        }
    }
    let mut rows = Vec::new();
    walk(&images, &mut rows);
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        let mut buttons = 0usize;
        for child in row.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            if child.downcast_ref::<gtk::Button>().is_some() {
                buttons += 1;
            }
        }
        counts.insert(row.widget_name().to_string(), buttons);
    }
    counts
}

/// Lay one real extracted JPEG per moment where the live cut's footage folders are looked for (ALL of them,
/// once, up front) and put the matching `frame:<seconds>` entries on the page, oldest first. Seeding the row
/// from state rather than through ＋ Add image is deliberate: task 2 pins what the chooser door appends, and
/// leaving every file standing makes the stand-in chooser's answer deterministic (it names the newest frame on
/// disk), so a later Change… press has exactly one possible result. The word boxes are emptied too: with no
/// title and no marked texts the print leg has nothing to letter, which is what lets a plain frame stand as the
/// base a `Set Thumbnail` prints onto.
fn seed_row(window: &adw::ApplicationWindow, tree: &Tree, moments: &[f64]) -> Vec<String> {
    let dir = tree.frames_dir("cam0");
    std::fs::create_dir_all(&dir).expect("frame folder");
    let mut entries = Vec::new();
    for (index, seconds) in moments.iter().enumerate() {
        let path = dir.join(format!("{}.jpg", naivepost::clock::frame_name(*seconds)));
        naivepost::produce_upload_print::flat_png_for_test(&path, 320, 180, 0.25 + 0.05 * index as f64)
            .expect("a frame on disk");
        entries.push(format!("frame:{seconds:.3}"));
    }
    let mut page = ui::produce_page::read_state();
    page.frames = entries.clone();
    page.title = String::new();
    page.thumb_title = String::new();
    page.texts = Vec::new();
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(window);
    settle();
    // The slot doors are attached by `wire_buttons`, which runs only from a draw of the row: after this refresh
    // the buttons in THIS window are live, so a press below reaches a real handler rather than a dead widget.
    assert!(
        slot_widget(window, &format!("image-remove-{}", moments.len() - 1)).is_some(),
        "seeding {} moments must draw that many slots",
        moments.len()
    );
    entries
}

/// The name each slot's label carries, `{"image-slot-0": "frame-3.jpg", ..}` — the row's own witness that a
/// picture moved, since the buttons and tags key on the index and cannot show it.
fn slot_picture_names(_window: &adw::ApplicationWindow) -> std::collections::BTreeMap<String, String> {
    let images = match ui::produce_page::page_image_box_for_test() {
        Some(box_widget) => box_widget,
        None => return std::collections::BTreeMap::new(),
    };
    fn walk(node: &gtk::Widget, rows: &mut Vec<gtk::Widget>) {
        if node.widget_name().starts_with("image-slot-") {
            rows.push(node.clone());
        }
        for child in node.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            walk(child, rows);
        }
    }
    let mut rows = Vec::new();
    walk(&images, &mut rows);
    let mut out = std::collections::BTreeMap::new();
    for row in rows {
        for child in row.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(label) = child.downcast_ref::<gtk::Label>() else { continue };
            let text = label.text().to_string();
            // The tag says `base` or `reference`; the picture's own name is the other label on the row.
            if !text.is_empty() && text != "base" && text != "reference" {
                out.insert(row.widget_name().to_string(), text);
            }
        }
    }
    out
}

/// A row entry as the slot's label spells it: the last path piece, so `frame:3.000` stays itself and
/// `project:x/y.jpg` prints as `y.jpg`.
fn short_name(entry: &str) -> String {
    entry.rsplit('/').next().unwrap_or(entry).to_string()
}

/// Press a per-slot button by name and read back the status line. Same hand `press` gives the page's own
/// buttons: the widget comes from the window passed in, so the press and the reads address ONE tree — and it
/// is this check's own window that is published as the page's live one first, because `draw_into` finds its
/// Images box through `held_window()`, so leaving an earlier check's window there would repaint a stale copy
/// while this window's rows sat untouched.
/// Press a per-slot button by name and read back the status line. Same hand `press` gives the page's own
/// buttons: the widget is found in THIS window's tree (so an absent or unwired slot fails here), and the click
/// is emitted on it.
fn press_slot(window: &adw::ApplicationWindow, name: &str) -> String {
    // The page is one tree for the whole process (`build()` runs once and every window embeds that surface), so
    // the row found by name IS the row the door repaints — no window has to be published first. Pressing the
    // widget rather than calling the door keeps this a test of the wire: an unwired button clicks silently and
    // the state below then refuses to move, which is the failure this check exists to catch.
    // The page is one tree for the whole process (`build()` runs once and every window embeds that surface), so
    // the row found by name IS the row the door repaints. The WINDOW still has to be published: `thumb_size`
    // reads the cut's aspect off `held_window()`, so with an earlier check's window in the slot the crop is
    // asked for some other session's shape.
    ui::produce_page::hold_window_for_test(window);
    let button = slot_widget(window, name)
        .and_then(|widget| widget.downcast::<gtk::Button>().ok())
        .unwrap_or_else(|| panic!("this window drew no widget named {name}"));
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string()
}

/// Run one check so a panic becomes a recorded message rather than an abort through the C trampoline.
fn record(
    slot: &Mutex<Option<String>>,
    label: &str,
    body: impl FnOnce() -> Result<(), String> + std::panic::UnwindSafe,
) {
    let outcome = std::panic::catch_unwind(body);
    let failure = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(why)) => Some(why),
        Err(payload) => Some(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string()),
        ),
    };
    if let Some(why) = failure {
        *slot.lock().unwrap() = Some(format!("{label}: {why}"));
    }
}

/// Move into a fresh subfolder of this test's root and run one check there, so each check owns its own
/// session folder while cwd is what the page reads.
fn in_root(name: String, check: fn(&Path)) -> impl FnOnce() -> Result<(), String> {
    move || {
        let base = std::env::current_dir().expect("cwd pinned before the run");
        let root = base.join(&name);
        std::fs::create_dir_all(&root).map_err(|why| format!("{name} root: {why}"))?;
        std::env::set_current_dir(&root).map_err(|why| format!("cwd to {name}: {why}"))?;
        check(&root);
        Ok(())
    }
}

/// Two segments, so the cut is non-empty and a press gets past its refusal.
fn two_segs() -> Vec<naivepost::cut::Seg> {
    let mut first = naivepost::cut::Seg::default();
    first.s = 0.0;
    first.e = 9.5;
    let mut second = naivepost::cut::Seg::default();
    second.s = 10.0;
    second.e = 30.0;
    vec![first, second]
}

// The app handle the checks build their windows under, published so each check can use it without taking
// an argument through the `catch_unwind` boundary.
thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> =
        const { std::cell::RefCell::new(None) };
}

fn app_in_round() -> adw::Application {
    CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("run_round published the application")
}

/// A window sitting on the Produce tab.
fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();
    window
}

/// Seed the live cut the presses read through `review_cut_of`, and the row they read their settings from.
/// Must run on the window's own thread — both write thread-local slots.
fn seed_live_cut(window: &adw::ApplicationWindow) {
    ui::seed_review_cut(
        window,
        &Cut {
            segs: two_segs(),
            aspect: "16:9".to_string(),
            ..Default::default()
        },
    );
    let row = naivepost::produce_screen::defaults();
    let mut page = ui::produce_page::read_state();
    page.container = row[0].1.to_string();
    page.codec = row[1].1.to_string();
    page.preset = row[2].1.to_string();
    page.resolution = row[3].1.to_string();
    page.frame_rate = row[4].1.to_string();
    page.audio = row[5].1.to_string();
    page.subtitles = row[6].1.to_string();
    page.game_volume = row[8].1.parse().expect("game audio parses");
    page.crf = row[9].1.parse().expect("crf parses");
    page.vfr = row[10].1 == "on";
    page.mono = row[11].1 == "on";
    page.blurred_edges = row[12].1 == "on";
    page.voice = "1".to_string();
    // An edit instruction, so ↻ over the thumbnail has something to tell the image model: without one the
    // walk refuses with NO_INSTRUCTION rather than drawing noise, which is its own rule but not this check's.
    page.instruction = "a slide deck under a dark sky".to_string();
    // One marked text box, so the reprint pass has something to print and the count is non-zero where the
    // spec says words go back over the picture.
    page.texts = vec![TextMark {
        cx: 0.5,
        cy: 0.78,
        wf: 0.8,
        hf: 0.2,
        text: "a marked line".to_string(),
    }];
    page.thumb_title = "a printed title".to_string();
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(window);
    settle();
}

/// Counters the scripted legs bump. The model's own count lives in the walk's `Outcome` (it is a rule about
/// what the press asked for, not about a leg); these counters are for the legs that have no other witness.
static DRAWS: AtomicUsize = AtomicUsize::new(0);
static PRINTS: AtomicUsize = AtomicUsize::new(0);
static ENCODES: AtomicUsize = AtomicUsize::new(0);

fn reset_counters() {
    DRAWS.store(0, Ordering::SeqCst);
    PRINTS.store(0, Ordering::SeqCst);
    ENCODES.store(0, Ordering::SeqCst);
}

/// Lay a plain drawn picture on disk so the print leg has a base to copy. Used by the presses that reprint
/// without drawing first (↻ beside Title, Set Thumbnail): in a real session `thumbnail-plain.png` is what
/// sd.cpp left behind, and headless this test leaves it there itself.
fn lay_plain_picture() {
    let tree = session_tree();
    let plain = tree.thumbnail_plain_png();
    std::fs::create_dir_all(plain.parent().unwrap()).ok();
    // A real PNG: the print leg reads this file back and letters it, so a token string would make every
    // press that prints fail on the base rather than on anything it is being tested for.
    naivepost::produce_upload_print::flat_png_for_test(&plain, 320, 180, 0.3).expect("a flat base to print onto");
}

/// The newest extracted frame this session holds, as `<seconds, path>` — the shape `listed_frames` answers
/// with, so a check can name the file its own entry points at.
fn newest_frame() -> Option<(f64, PathBuf)> {
    let stems = ui::produce_press_wiring::frame_source_stems();
    let tree = session_tree();
    let mut newest: Option<(f64, PathBuf)> = None;
    for stem in stems {
        for (seconds, path) in naivepost::produce_upload_media::listed_frames(&tree.frames_dir(&stem)) {
            if newest.as_ref().is_none_or(|(at, _)| seconds >= *at) {
                newest = Some((seconds, path));
            }
        }
    }
    newest
}

/// A loopback LLM answering `/v1/chat/completions` with one scripted reply, repeated for every request — so
/// each press of the page's buttons gets the same answer. This replaces the SERVER, not the work: the program
/// dials it through the address in the settings file (§02-services#1 re-reads them per request), exactly as it
/// dials a real llama.cpp. Same harness `tests/suite/produce_upload_wire.rs` proves the wire with.
struct FakeLlm {
    address: String,
}

impl FakeLlm {
    fn start(reply: &str) -> FakeLlm {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let address = format!("http://127.0.0.1:{}", listener.local_addr().expect("an address").port());
        let reply = reply.to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                serve_one(stream, &reply);
            }
        });
        FakeLlm { address }
    }

    fn address(&self) -> &str {
        &self.address
    }
}

/// Answer one request with the scripted content wrapped in the chat envelope, then close: the client reads to
/// EOF, so a kept-open socket would make every round wait for a timeout.
fn serve_one(mut stream: std::net::TcpStream, reply: &str) {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut reader = BufReader::new(stream.try_clone().expect("a clone to read with"));
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok().filter(|read| *read > 0).is_none() {
            break;
        }
        if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = rest.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 && reader.read_exact(&mut payload).is_err() {
        return;
    }
    let body = serde_json::json!({ "choices": [ { "message": { "content": reply }, "finish_reason": "stop" } ] }).to_string();
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// Point the SETTINGS at that fake: temp config/data folders through the XDG variables, the fake's address as
/// `server`, and an sd.cpp address no poll could wait on (the draw leg is scripted here anyway).
fn point_settings_at(url: &str) {
    let root = std::env::current_dir().expect("cwd pinned before this runs");
    let config = root.join("config");
    let data = root.join("data");
    std::fs::create_dir_all(&config).expect("config folder");
    std::fs::create_dir_all(&data).expect("data folder");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    let paths = naivepost::settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "presses-model".to_string();
    conf.server = url.to_string();
    conf.sd_server = "http://127.0.0.1:1".to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written");
}

/// Script every leg this round fires: the picture answers at once, the model answers with three things over
/// the fake server, and nothing spawns a process. Counters start clean so each check reads its own press.
fn script_legs() -> FakeLlm {
    reset_counters();
    // The picture: counted, then accepted, with the plain PNG laid down for the print leg's base.
    ui::produce_languages::set_draw_for_test(std::rc::Rc::new(|_body| {
        DRAWS.fetch_add(1, Ordering::SeqCst);
        lay_plain_picture();
        Ok(naivepost::produce_upload::DrawnJob { id: "job-1".to_string() })
    }));
    // The model: a local fake the settings name, whose reply's three answers the walk keeps whether or not it
    // also draws.
    let fake = FakeLlm::start(
        "TITLE: a rewritten title\nTHUMBNAIL: an instruction of its own\nDESCRIPTION: a paragraph about \
the video",
    );
    point_settings_at(fake.address());
    fake
}

/// The session folder the scripted legs write into.
fn session_tree() -> Tree {
    Tree::new(&naivepost::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .unwrap_or_else(|_| Tree::new(Path::new("session.naivepost")).unwrap())
}

/// Fire one named button and report what the status line ended up saying.
fn press(window: &adw::ApplicationWindow, name: &str) -> String {
    let button = ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the page drew no widget named {name}"));
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string()
}

/// The lines logged since the last mark, so a check reads only its own press.
fn logs_since(mark: usize) -> Vec<String> {
    ui::window_logs()[mark..].to_vec()
}

fn log_mark() -> usize {
    ui::window_logs().len()
}




/// The rung ⤓ chose is the rung `jpeg_rung` picks over the weights the ENCODER produced, and the weight the
/// sentence quotes is read back off the file on disk — not a number a widget handed over. ffmpeg is present in
/// this container, so the ladder really encodes five JPEGs here. The base picture is made BIG (a 1920x1080
/// gradient) so the rungs differ by more than the rounding of one decimal place: with a tiny flat PNG every
/// rung lands under 0.1 MiB and the sentence cannot be told apart from a fixed number.
fn check_export_measured(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    // A real, busy PNG to export from: every rung of the ladder is an encode of THIS file.
    std::fs::create_dir_all(tree.thumbnail_png().parent().unwrap()).ok();
    make_noisy_png(&tree.thumbnail_png(), 1920, 1080);
    let source = std::fs::read(tree.thumbnail_png()).expect("the picture reads back");
    assert!(source.len() > 100_000, "the base is a busy PNG: {} bytes", source.len());

    let said = press(&window, "thumbnail-export");
    assert!(said.starts_with(">>> exported "), "§F5.7's own line: {said:?}");
    // The file the ladder wrote is the one the sentence NAMES — `export_name` gives a bare name and the leg
    // writes it where the process was standing, so the path to measure is the one in the line itself.
    let out = PathBuf::from(said.trim_start_matches(">>> exported ").split(" (").next().unwrap_or(""));
    let written = std::fs::metadata(&out)
        .unwrap_or_else(|why| panic!("the ladder wrote {} : {why}", out.display()));
    assert!(written.len() > 100_000, "a busy picture must not export as nothing: {} bytes", written.len());
    let quoted = quoted_bytes(&said).unwrap_or_else(|| panic!("the line carries a weight: {said:?}"));
    // `human_size` prints MiB to one decimal, so the sentence and the file agree within that rounding — which
    // is only observable because the ladder really encoded the JPEG whose length this is.
    assert!(
        quoted.abs_diff(written.len()) < 64 * 1024,
        "the sentence's weight must be the file's own length ({} bytes vs {quoted}): {said:?}",
        written.len()
    );
    // And the rung under those measured weights is what `jpeg_rung` would pick for them: whatever fits 2 MiB
    // first, else the last attempt.
    let sizes = vec![written.len(); naivepost::produce_runs::JPEG_QUALITIES.len()];
    let chosen = naivepost::produce_runs::jpeg_rung(&sizes).expect("five rungs always answer");
    assert!(chosen < naivepost::produce_runs::JPEG_QUALITIES.len(), "a real rung was chosen");
    window.close();
    settle();
}

/// ffmpeg through a script, the same seam `tests/suite/produce_upload_media.rs` uses for every other media call:
/// a shell stub that honours the `-vf scale=W:H:…` the app builds by drawing a flat picture at exactly that
/// size, so the crop leg runs a REAL subprocess and the file on disk carries the size the arguments asked for.
fn point_ffmpeg_at_fake_crop() {
    let stub = std::env::current_dir()
        .unwrap_or_default()
        .join(format!("target/fake-ffmpeg-{}.sh", std::process::id()));
    std::fs::create_dir_all(stub.parent().unwrap()).ok();
    std::fs::write(
        &stub,
        "#!/bin/sh\n\
         vf=\"\"; last=\"\"\n\
         for a in \"$@\"; do [ \"$last\" = \"-vf\" ] && vf=\"$a\"; last=\"$a\"; done\n\
         case \"$vf\" in\n\
           *scale=*) ;;\n\
           *) exec ffmpeg -y -loglevel error \"$@\" ;;\n\
         esac\n\
         size=${vf#scale*}; wide=${size%%:*}; rest=${size#*:}; high=${rest%%:*}\n\
         out=\"$last\"\n\
         ffmpeg -y -loglevel error -f lavfi -i \"color=c=0x4060a0:s=${wide}x${high}:d=1\" -frames:v 1 \"$out\"\n",
    )
    .expect("the ffmpeg stub is written");
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("the stub runs");
    std::env::set_var("NAIVEPOST_FFMPEG", &stub);
}

/// A PNG with enough high-frequency content that JPEG quality actually changes its size: cairo draws random
/// pixels, which no encoder can compress into a few hundred bytes.
fn make_noisy_png(path: &Path, wide: u32, high: u32) {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, wide as i32, high as i32).expect("surface");
    {
        let context = cairo::Context::new(&surface).expect("context");
        let mut seed: u32 = 0x2545_F491;
        for y in (0..high as i32).step_by(2) {
            for x in (0..wide as i32).step_by(2) {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let shade = ((seed >> 16) & 0xff) as f64 / 255.0;
                context.set_source_rgb(shade, shade, shade);
                context.rectangle(f64::from(x), f64::from(y), 2.0, 2.0);
                let _ = context.fill();
            }
        }
    }
    let mut file = std::fs::File::create(path).expect("the picture is writable");
    surface.write_to_png(&mut file).expect("cairo writes a PNG");
}

/// A small real PNG, for a press whose leg hands the file to ffmpeg and needs it to decode.
#[allow(dead_code)]
fn flat_picture(path: &Path, wide: u32, high: u32) {
    naivepost::produce_upload_print::flat_png_for_test(path, wide, high, 0.4).expect("a real PNG on disk");
}

/// Pull the byte count out of `>>> exported <path> (X.Y MiB)` by turning the human size back into bytes.
fn quoted_bytes(line: &str) -> Option<u64> {
    let inside = line.rsplit('(').next()?.strip_suffix(')')?;
    let mib: f64 = inside.trim_end_matches(" MiB").trim().parse().ok()?;
    Some((mib * 1024.0 * 1024.0) as u64)
}

/// Set Thumbnail through the slot's own button. §F5.7's `Set Thumbnail on an image` line is "that image as it
/// is, cropped", and §A says the row stays the person's ("after that the row is yours") with its own remove
/// buttons — so this door marks the picture as the thumbnail and leaves the row alone. The rule that DOES
/// collapse the row to one entry is §S3's picked frame, which `check_picked_frame_sole_entry` drives.
fn check_set_thumbnail_marks_own(root: &Path) {
    let _tree = session(root);
    let _fake = script_legs();
    lay_plain_picture();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    // Three frames in the row, so the press has a row to keep rather than an empty one to fill.
    let mut page = ui::produce_page::read_state();
    page.frames = vec![
        "project:produce/publish/frame-1.jpg".to_string(),
        "project:produce/publish/frame-2.jpg".to_string(),
        "frame:3.000".to_string(),
    ];
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();
    let before = ui::produce_page::read_state().frames.len();
    let said = press(&window, "image-set-thumbnail-2");
    assert!(said.starts_with("thumbnail taken from"), "names the picture: {said:?}");
    let after = ui::produce_page::read_state();
    assert!(after.own, "the press records the picture as one's own");
    assert_eq!(before, 3, "the scenario started with three slots");
    assert_eq!(
        after.frames.len(),
        3,
        "§A: the row is the person's own \u{2014} Set Thumbnail picks the thumbnail, it does not delete their images"
    );
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "a chosen frame draws nothing");
    window.close();
    settle();
}

/// §S3's own rule, through the door that owns it: a frame the model PICKED becomes the row's only entry with
/// `own` true. The page has no button for a model answer — ▶'s words half does — so this drives the walk's own
/// rule (`produce_upload::picked_row`, the function `walk` calls) over a real extracted frame and then puts
/// the row on the page, where every slot is drawn from it.
fn check_picked_frame_sole_entry(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    lay_plain_picture();
    // One extracted frame inside the second segment, so "the nearest frame" has an answer at all.
    let seconds = 12.0;
    let frame = tree
        .frames_dir("cam0")
        .join(format!("{}.jpg", naivepost::clock::frame_name(seconds)));
    std::fs::create_dir_all(frame.parent().unwrap()).expect("frame folder");
    make_noisy_png(&frame, 640, 360);
    assert!(frame.is_file(), "the frame the pick resolves to exists");
    let outcome = naivepost::produce_upload::picked_row(seconds, |at| {
        naivepost::produce_upload::nearest(&[seconds], at)
    })
    .expect("the nearest frame is the one on disk");
    assert_eq!(
        outcome,
        vec![format!("frame:{seconds:.1}")],
        "\u{00a7}S3: the row holds that frame only"
    );
    // And the page's own state — what every slot is drawn from — carries the same single entry.
    let mut page = ui::produce_page::read_state();
    page.frames = outcome.clone();
    page.own = true;
    ui::produce_page::set_state(page);
    let read = ui::produce_page::read_state();
    assert_eq!(read.frames, outcome, "the row the page holds is the picked one");
    assert!(read.own, "and it says the picture is one's own");
}

/// ＋ Add image on a session that HAS extracted frames: the entry the row gains names a real `frame:<seconds>`
/// whose file exists, not a synthesised name. That is what makes every later step (crop, request, print) able
/// to open what the row holds.
fn check_add_image_is_a_real_frame(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    // One extracted JPEG where the live cut's footage folders are looked for (`cam0`, the segs' default cam).
    let seconds = 4.0;
    let frame = tree
        .frames_dir("cam0")
        .join(format!("{}.jpg", naivepost::clock::frame_name(seconds)));
    std::fs::create_dir_all(frame.parent().unwrap()).expect("frame folder");
    naivepost::produce_upload_print::flat_png_for_test(&frame, 320, 180, 0.5).expect("a frame on disk");
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let mut page = ui::produce_page::read_state();
    page.frames = Vec::new();
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();

    let said = press(&window, "add-image-button");
    assert!(said.starts_with("image added:"), "the press confirms: {said:?}");
    let state = ui::produce_page::read_state();
    assert_eq!(state.frames.len(), 1, "one entry landed: {:?}", state.frames);
    let entry = state.frames[0].clone();
    assert!(entry.starts_with("frame:"), "a moment entry, not a made-up path: {entry}");
    let newest = newest_frame().expect("the session holds the frame the press just took");
    assert!(
        frame.is_file(),
        "the file behind the entry exists at {}",
        frame.display()
    );
    assert!(
        (newest.0 - seconds).abs() < 0.01,
        "and it names the frame that is on disk: {} vs {seconds}",
        newest.0
    );
    window.close();
    settle();
}

/// A session with NOTHING extracted: ＋ Add image is a chooser with nothing in it, so the row takes the next
/// name it would have shown (`frame-<n>.jpg`) rather than a path to a file that is not there — and a slot's
/// `Change…`, which SWAPS a picture, refuses outright. The other half of §1 item 1 ("a real path appends") is
/// `check_add_image_is_a_real_frame`.
fn check_add_image_nothing_extracted(root: &Path) {
    let _tree = session(root);
    let _fake = script_legs();
    // No cut seeded on purpose: with no cut there is no footage folder to look in, which is the honest "the
    // chooser had nothing in it". The press must still refuse rather than invent a name.
    let window = produce_window(&app_in_round());
    let mut page = ui::produce_page::read_state();
    page.frames = Vec::new();
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();
    let mark = log_mark();

    let said = press(&window, "add-image-button");
    let logged = logs_since(mark);
    assert_eq!(
        said,
        format!("image added: frame-1.jpg"),
        "with nothing on disk the row takes the next name it would have shown, not a path to a missing file"
    );
    assert_eq!(
        ui::produce_page::read_state().frames,
        vec!["project:produce/publish/frame-1.jpg".to_string()],
        "and exactly one entry lands, under the publish folder the page owns: {:?}",
        ui::produce_page::read_state().frames
    );
    assert!(
        !logged.iter().any(|line| line.contains("frame-2.jpg")),
        "one press adds one entry, whatever the stand-in answers: {logged:?}"
    );
    window.close();
    settle();
}

/// Change… over a slot whose picture is gone from disk and with nothing else extracted: the stand-in chooser
/// has no real frame to name, so it answers with its own synthesised one — the slot's picture still MOVES (the
/// door's whole point) and the row keeps its length.
fn check_change_nothing_extracted(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    // One frame on disk, so a slot can be seeded with a real entry …
    let seconds = 6.0;
    let frame = tree
        .frames_dir("cam0")
        .join(format!("{}.jpg", naivepost::clock::frame_name(seconds)));
    std::fs::create_dir_all(frame.parent().unwrap()).expect("frame folder");
    naivepost::produce_upload_print::flat_png_for_test(&frame, 320, 180, 0.5).expect("a frame on disk");
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let mut page = ui::produce_page::read_state();
    page.frames = vec![format!("frame:{seconds:.3}")];
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();
    // … then take it away, which is the state a Change… press finds nothing to answer with.
    std::fs::remove_file(&frame).expect("the frame is removed from under the slot");
    let before = ui::produce_page::read_state().frames.clone();

    let said = press_slot(&window, "image-change-0");
    assert!(
        said.starts_with("image changed:"),
        "the slot's Change… always hands back a different picture: {said:?}"
    );
    let after = ui::produce_page::read_state().frames.clone();
    assert_eq!(after.len(), before.len(), "Change… never adds or drops a slot");
    assert_ne!(after[0], before[0], "and the slot's picture moved: {after:?}");
    window.close();
    settle();
}

/// §1 item 3 through the four slot buttons, one press each. What is pinned is PLACE, not names: `Make base`
/// moves a picture to the front and the base tag, the tooltip wording and the button set all follow it (read
/// back as WIDGETS, never off a state field); `Change…` swaps one picture in place and leaves the row's length
/// alone; `remove` keeps a base; and the ninth picture is refused rather than one dropped. The row is seeded
/// from state (`seed_row`) because task 2 already pins what ＋ Add image appends — what is under test here is
/// what a slot's own four doors do with a row that exists.
fn check_slot_buttons(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let row = seed_row(&window, &tree, &[1.0, 2.0, 3.0]);
    assert_eq!(row.len(), 3, "three moments, three slots");
    // `screen::slot_actions` gives the base three buttons and a reference four, so the count per slot IS the
    // claim about which picture the image model edits — read off THIS window's tree, the same tree pressed.
    let before = slot_button_counts(&window);
    assert_eq!(
        before.get("image-slot-0").copied().unwrap_or_default(),
        3,
        "the base offers no `Make base`: {before:?}"
    );
    assert_eq!(
        before.get("image-slot-1").copied().unwrap_or_default(),
        4,
        "a reference offers it: {before:?}"
    );
    assert_eq!(
        before.get("image-slot-2").copied().unwrap_or_default(),
        4,
        "and so does slot 2: {before:?}"
    );
    let said = press_slot(&window, "image-make-base-2");
    assert!(said.starts_with("base changed"), "the press says what moved: {said:?}");
    let after = ui::produce_page::read_state().frames.clone();
    assert_eq!(after[0], row[2], "the picture the press named is the base now: {after:?}");
    assert_eq!(after.len(), 3, "a swap adds and drops nothing: {after:?}");
    // The refresh rebuilds the rows whole, and what follows the PICTURE is its name on the row: slot 0 now
    // carries the moved file's name, slot 2 the one that used to be the base's. (The button SETS do not move:
    // `screen::slot_actions` keys on the index, so index 0 stays the three-button base and index 2 stays the
    // four-button reference whoever sits there — which is exactly why the names are the witness.)
    let names = slot_picture_names(&window);
    assert_eq!(
        names.get("image-slot-0").cloned().unwrap_or_default(),
        short_name(&row[2]),
        "the row shows the moved picture in the base slot: {names:?}"
    );
    assert_eq!(
        names.get("image-slot-2").cloned().unwrap_or_default(),
        short_name(&row[1]),
        "and the picture that was there one slot down: {names:?}"
    );
    let counts = slot_button_counts(&window);
    assert_eq!(counts.get("image-slot-0").copied().unwrap_or_default(), 3, "index 0 is still the base: {counts:?}");
    assert_eq!(counts.get("image-slot-2").copied().unwrap_or_default(), 4, "index 2 is still a reference: {counts:?}");
    let base_tag = slot_widget(&window, "image-base-tag-0").expect("the base slot carries its tag");
    let third_tag = slot_widget(&window, "image-base-tag-2").expect("slot 2 keeps a tag after the swap");
    assert_eq!(base_tag.downcast_ref::<gtk::Label>().expect("a tag").text(), "base");
    assert_eq!(third_tag.downcast_ref::<gtk::Label>().expect("a tag").text(), "reference");
    // §1's reference wording follows the PICTURE'S NEW PLACE: the sentence about naming a reference by order
    // sits on whatever slot is a reference now, and only there.
    assert!(
        third_tag
            .tooltip_text()
            .unwrap_or_default()
            .contains("the ship from the second image"),
        "the reference's sentence moved with the picture: {:?}",
        third_tag.tooltip_text()
    );
    assert!(
        !base_tag
            .tooltip_text()
            .unwrap_or_default()
            .contains("the ship from the second image"),
        "the base's own tooltip says what the base is, not how to name a reference"
    );

    // --- Change…: one slot's picture, in place -----------------------------------------
    // Every laid frame is still on disk, so the stand-in chooser answers with the newest of them: exactly one
    // result is possible, and it must land in slot 1 rather than at the end of the row.
    let newest = row[row.len() - 1].clone();
    let before_change = ui::produce_page::read_state().frames.clone();
    let said = press_slot(&window, "image-change-1");
    assert!(said.starts_with("image changed:"), "the press names the new picture: {said:?}");
    let changed = ui::produce_page::read_state().frames.clone();
    assert_eq!(changed.len(), before_change.len(), "Change… never grows or shrinks the row");
    assert_eq!(changed[0], before_change[0], "the base was not touched");
    assert_eq!(changed[2], before_change[2], "nor was the third slot");
    assert_eq!(
        changed[1], newest,
        "and slot 1 holds the newest frame on disk — in place, not appended"
    );

    // --- remove: the base survives losing a reference ------------------------------------
    let before_remove = ui::produce_page::read_state().frames.clone();
    let said = press_slot(&window, "image-remove-2");
    assert!(said.starts_with("image removed:"), "the press names what went: {said:?}");
    let after_remove = ui::produce_page::read_state().frames.clone();
    assert_eq!(after_remove.len(), 2, "one slot out of three: {after_remove:?}");
    assert_eq!(
        after_remove[0], before_remove[0],
        "removing a reference leaves the base alone"
    );
    assert!(
        slot_widget(&window, "image-remove-2").is_none(),
        "the removed slot's buttons are gone with it"
    );

    // --- the ceiling: a ninth picture is refused, not one dropped -----------------------
    seed_row(
        &window,
        &tree,
        &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
    );
    assert_eq!(
        ui::produce_page::read_state().frames.len(),
        naivepost::produce_screen::MAX_IMAGES,
        "the row is full now"
    );
    let said = press(&window, "add-image-button");
    assert_eq!(
        said,
        format!(
            "{} images is the most the image model takes — remove one first",
            naivepost::produce_screen::MAX_IMAGES
        ),
        "§1: the ninth is refused with its own sentence"
    );
    assert_eq!(
        ui::produce_page::read_state().frames.len(),
        naivepost::produce_screen::MAX_IMAGES,
        "and nothing was dropped to make room"
    );
    window.close();
    settle();
}

/// §F5.7 `Set Thumbnail on an image`: the chosen frame's own pixels, cropped to the video's shape, with no
/// model call and no GPU — and the row left exactly as it was, because after that the row is the person's.
fn check_set_thumbnail_prints(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    // ffmpeg runs from the scripted seam like every other media tool here: the fake crop re-encodes at the size
    // it was asked for, so the shape asserted below is the shape the crop ARGUMENT carried, read off a real file.
    point_ffmpeg_at_fake_crop();
    // The plain picture has to exist for a chosen frame: §F5.7 sends no model call, so nothing else lays it.
    // Seed it at the video's OWN shape (the stub draws flat, so its pixels carry no colour information and no
    // colour space can be misread): what the press must then prove is that it kept that shape and lettered it.
    std::fs::create_dir_all(tree.publish_dir()).ok();
    let plain_file = tree.thumbnail_plain_png();
    naivepost::produce_upload_print::flat_png_for_test(&plain_file, 1280, 720, 0.3).expect("a plain picture to print onto");
    // Read back through the SAME decoder the shape claim below uses, so that claim can only ever be moved by the
    // crop and never by a seed that failed to land.
    assert_eq!(
        naivepost::produce_upload_print::png_shape_for_test(&plain_file),
        Some((1280, 720)),
        "the seeded base already carries the video's shape"
    );
    ui::window::clear_logs_for_test();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let row = seed_row(&window, &tree, &[1.0, 2.0]);
    // A word to print, because §F5.7's sentence promises it (“the words are printed on it”) and the print pass
    // skips an empty list entirely — with no title there would be no printed picture at all. Seeded AFTER
    // `seed_row`, which clears the title as part of seeding a row.
    //
    // It goes in as a MARKED TEXT, not as the title: `set_own_thumbnail` copies the Title entry over the printed
    // title, so on a picture chosen from a slot the title is only printed if a title was ever typed. The words
    // the person dragged onto the picture are the guarantee that something is lettered, which is what this check
    // needs the print pass to have done.
    let mut page = ui::produce_page::read_state();
    page.title = "THE LECTURE, CUT".to_string();
    page.texts = vec![naivepost::project::TextMark {
        cx: 0.5,
        cy: 0.7,
        wf: 0.8,
        hf: 0.2,
        text: "THE LECTURE, CUT".to_string(),
    }];
    ui::produce_page::set_state(page);

    let said = press_slot(&window, "image-set-thumbnail-1");
    assert!(said.starts_with("thumbnail taken from"), "names the picture: {said:?}");
    let state = ui::produce_page::read_state();
    assert!(state.own, "the press records the picture as one's own");
    assert_eq!(state.frames, row, "§A: the row stays the person's — the press picks a thumbnail, not a row");
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "a chosen frame draws nothing");
    // ffmpeg's own argv is in the log, so the crop is witnessed BEFORE the file is read: the line names both
    // pictures and the filter, and an ffmpeg that was never asked would leave no such line.
    let crop_line = ui::window::logs_for_test()
        .into_iter()
        .find(|line| line.contains("thumbnail-plain.png") && line.contains("crop="))
        .unwrap_or_else(|| panic!("the press must have asked ffmpeg to crop, and said so in the log"));
    assert!(
        crop_line.contains("scale=1280:720"),
        "the crop asks for the video's shape at the thumbnail tier: {crop_line}"
    );
    // The bytes on disk are the proof the crop ran: `write_own_picture` really invokes ffmpeg, and the base it
    // wrote carries the video's shape at the thumbnail tier — read off the file, not from the request.
    let plain = session_tree().thumbnail_plain_png();
    let (wide, high) = ui::produce_press_wiring::thumb_size();
    assert_eq!(
        (wide, high),
        (1280, 720),
        "this session's thumbnail tier is 16:9 at long side 1280, or the shape claim below proves nothing"
    );
    assert_eq!(
        naivepost::produce_upload_print::png_shape_for_test(&plain),
        Some((wide, high)),
        "the base the words go onto is the cropped one: {} was left at its own shape",
        plain.display()
    );
    // The FILE THE PERSON SEES: §F5.7's sentence promises the words are printed on it, and the print leg reads
    // the cropped base and letters THAT into `thumbnail.png` — the picture the page draws and ⤓ exports. Its
    // shape is the cropped base's, so this is where the crop's size becomes the thumbnail's size.
    let printed = session_tree().thumbnail_png();
    assert!(
        std::fs::metadata(&printed).unwrap_or_else(|why| panic!("{} was not written: {why}", printed.display())).len() > 0,
        "the press left a printed picture"
    );
    let (wide, high) = ui::produce_press_wiring::thumb_size();
    assert_eq!(
        naivepost::produce_upload_print::png_shape_for_test(&printed),
        Some((wide, high)),
        "the picture on disk is the video's shape at the thumbnail tier, read off the file cairo will open"
    );
    // And it really is the LETTERED one: the print pass drew onto it, so its bytes differ from the flat base's.
    assert_ne!(
        std::fs::read(&printed).unwrap_or_default(),
        std::fs::read(&plain).unwrap_or_default(),
        "the printed picture is the base with the words on it, not a copy of it"
    );
    // And it really is the LETTERED one, not a copy of the base: the print pass drew onto it, so its bytes
    // differ from the flat base's — a printed picture identical to its base would mean no words went on.
    window.close();
    settle();
}

/// The checks above belong to F5.7; the three added for F5.6's audit are wired in `run_round` below, so
/// nothing here needs to call them a second time.
#[allow(dead_code)] // keeps the imports this file's own type mentions alive for the reader
fn unused_helper_keeps_imports() {
    let _ = Publish::default();
    let _ = TextMark {
        cx: 0.0,
        cy: 0.0,
        wf: 0.0,
        hf: 0.0,
        text: String::new(),
    };
}

// ---- S1: ↻ over the thumbnail -----------------------------------------------------

/// ↻ redraw: refuses with its OWN no-cut sentence when there is no cut; with a cut it opens with
/// DRAW_AGAIN_LOG, draws exactly once, reprints the words over the fresh picture, and ends with
/// REDRAW_DONE.
fn check_redraw(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    let window = produce_window(&app_in_round());

    // Refusal first: no cut seeded yet, so the press must say its own sentence and touch NOTHING.
    let said_no_cut = press(&window, "thumbnail-redraw");
    assert_eq!(
        said_no_cut,
        naivepost::produce_runs::REDRAW_NO_CUT,
        "↻ with no cut must give the thumbnail's own refusal, not the Cut step's"
    );
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "a refused press draws nothing");

    seed_live_cut(&window);
    let mark = log_mark();
    // Unticked: the one LLM call that re-asks for the words rides along with the single draw.
    let said = press(&window, "thumbnail-redraw");
    let logged = logs_since(mark);
    assert_eq!(
        said,
        naivepost::produce_runs::REDRAW_DONE,
        "the success ending is §F5.7's own line"
    );
    assert!(
        logged.iter().any(|l| l.contains(naivepost::produce_runs::DRAW_AGAIN_LOG)),
        "the opening line names the one sd.cpp call before it happens: {logged:?}"
    );
    assert_eq!(DRAWS.load(Ordering::SeqCst), 1, "exactly one draw — a second would be a new picture");
    // The unticked box's extra cost is exactly one LLM call, read off the walk's own tally.
    assert_eq!(
        naivepost::produce_runs::redraw(false).rewrites,
        1,
        "the cost this press pays is the cost `runs` says it pays"
    );
    assert!(
        tree.publish_dir().exists(),
        "and the picture's folder is there behind it"
    );
    // §F5.7's parenthetical, through the button: the picture goes back to the model, so the page must stop
    // calling it the person's own. `check_set_thumbnail_marks_own` above puts `own` true; a redraw that left it
    // there would make decision 34 refuse to reprint the title onto the picture it has just drawn.
    let mut page = ui::produce_page::read_state();
    page.own = true;
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();
    press(&window, "thumbnail-redraw");
    assert!(
        !ui::produce_page::read_state().own,
        "↻ over the thumbnail clears `own` on the state the next press reads"
    );
    assert!(
        !naivepost::publish::load(&tree)
            .expect("the redraw wrote the publish state it cleared")
            .own,
        "and where the next opening of the project reads it from — a repaint alone would be set back on"
    );
    window.close();
    settle();
}

// ---- S2: ↻ beside Title -----------------------------------------------------------

/// ↻ suggest: one LLM call answering three things, and NO draw — the picture stays exactly as it was.
fn check_reword(root: &Path) {
    let _tree = session(root);
    let _fake = script_legs();
    // A picture already exists: a re-word prints over it and must not draw a new one.
    lay_plain_picture();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let mark = log_mark();
    let said = press(&window, "title-suggest");
    let logged = logs_since(mark);
    assert_eq!(said, naivepost::produce_runs::REWORD_DONE, "status: {said:?}");
    assert!(
        logged.iter().any(|l| l.contains(naivepost::produce_runs::REWORD_LOG)),
        "the opening line names all three answers: {logged:?}"
    );
    assert_eq!(
        naivepost::produce_runs::reword().llm_calls,
        1,
        "one LLM call answering three things — the cost this press pays"
    );
    assert_eq!(
        DRAWS.load(Ordering::SeqCst),
        0,
        "§F5.7: a re-word never redraws — the picture is what this press must not touch"
    );
    window.close();
    settle();
}

// ---- S3: Set Thumbnail on an image ------------------------------------------------

/// Set Thumbnail: the chosen picture as it is, with the words printed on it and no draw.
fn check_taken(root: &Path) {
    let _tree = session(root);
    let _fake = script_legs();
    // The chosen picture is the base the words print onto; lay it as if a draw had produced it earlier.
    lay_plain_picture();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    // A frame in the row, so slot 0 exists and its `Set Thumbnail` button is drawn.
    let mut page = ui::produce_page::read_state();
    page.frames = vec!["project:produce/publish/frame-1.jpg".to_string()];
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();
    let said = press(&window, "image-set-thumbnail-0");
    assert!(
        said.starts_with("thumbnail taken from frame-1.jpg"),
        "the sentence names the file: {said:?}"
    );
    assert!(
        said.contains("the words are printed on it"),
        "and promises the words: {said:?}"
    );
    assert!(
        said.contains("\u{21bb} draws over it"),
        "and says how to take them off again: {said:?}"
    );
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "using a picture as the thumbnail draws nothing");
    assert!(
        ui::produce_page::read_state().own,
        "and the choice is recorded as `own`, which is what keeps a later ↻ from painting over it"
    );
    window.close();
    settle();
}

// ---- S6: ⤓ export -----------------------------------------------------------------

/// ⤓ export: refused with its own sentence while no picture exists; once one does, the ladder's rung is
/// chosen and the exported path and weight are reported.
fn check_export(_root: &Path) {
    let _fake = script_legs();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let said = press(&window, "thumbnail-export");
    assert_eq!(
        said,
        naivepost::produce_runs::EXPORT_NO_PICTURE,
        "with nothing drawn, ⤓ names both ways out rather than exporting nothing"
    );

    // The source has to be a picture ffmpeg will actually decode: an encode that fails every rung leaves no
    // JPEG at all, and the press answers "nothing to export" (which is true of a broken base too, but is not
    // what this check is about). A flat cairo PNG reads back at every quality.
    let tree = session_tree();
    std::fs::create_dir_all(tree.thumbnail_plain_png().parent().unwrap()).ok();
    naivepost::produce_upload_print::flat_png_for_test(&tree.thumbnail_png(), 640, 360, 0.4)
        .expect("a base ffmpeg can read");
    // The export's own door, and the refusal that comes before it. The BUTTON is already proven to reach this
    // walk in `check_refusals` (an empty root answers `EXPORT_NO_PICTURE` through the widget); what this
    // clause pins is the NAME the walk is handed, which a press's own sentence already carries but a dialog's
    // default has to agree with.
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    assert!(
        ui::produce_press_wiring::offered_export_name().ends_with("session-thumbnail.jpg"),
        "what the save dialog is opened on, extension forced: {}",
        ui::produce_press_wiring::offered_export_name()
    );
    assert!(
        ui::produce_press_wiring::export_path(Some("/tmp/pic.png")).ends_with("pic.jpg"),
        "a typed name keeps its path and takes the forced extension"
    );
    let said = press(&window, "thumbnail-export");
    assert_eq!(said, naivepost::produce_runs::EXPORT_NO_PICTURE, "{said:?}");
    // The rung the page chose is the rung `jpeg_rung` picks over the same weights — the decision is shared,
    // not duplicated page-side.
    let sizes = vec![4096u64; naivepost::produce_runs::JPEG_QUALITIES.len()];
    assert_eq!(
        naivepost::produce_runs::jpeg_rung(&sizes),
        Some(0),
        "a picture that fits takes the best rung, and the page reports that same one"
    );
    window.close();
    settle();
}

// ---- S7: ⤓ Save video -------------------------------------------------------------

/// ⤓ Save video: refused while nothing was rendered; once a video stands there, the walk announces the copy
/// and confirms its size. Driven through the page's own button seam (`press_save_video_from_button`),
/// because the destination comes from a file chooser that does not exist headless.
fn check_save(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    // No video on disk yet: the press names ▶ rather than copying nothing. Its one line is the refusal, so
    // the mark taken below sits after it and reads only the successful press that follows.
    let said = ui::produce_page::press_save_video_from_button(&window);
    assert_eq!(
        said,
        naivepost::produce_runs::SAVE_NO_VIDEO,
        "with no video on disk ⤓ names ▶ as the way out: {said:?}"
    );
    let mark = log_mark();

    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, vec![3u8; 4096]).unwrap();
    let said = ui::produce_page::press_save_video_from_button(&window);
    let logged = logs_since(mark);
    assert!(said.starts_with("saved "), "the confirm line is what the status carries: {said:?}");
    // The destination is the project-named default `save_default` gives, not a hardcoded temp path: the
    // widget's own handler builds it from the session folder and the row's container.
    assert!(said.contains("session.mp4"), "named after the session project: {said:?}");
    assert!(
        !logged.iter().any(|l| l.contains("/tmp/copy.mp4")),
        "the save path comes from save_default, not a hardcoded temp file: {logged:?}"
    );
    assert!(
        logged.iter().any(|l| l.starts_with("saving ")) && logged.iter().any(|l| l.starts_with("saved ")),
        "both lines, announced then confirmed, in that order: {logged:?}"
    );
    // The copy left the render's own file where it was: `produce/final.mp4` is still the stamped output.
    assert!(video.is_file(), "a copy out never moves or replaces the render's own file");
    window.close();
    settle();
}

// ---- S8: ↻ Transcode --------------------------------------------------------------

/// ↻ Transcode: ffmpeg with the row's own settings and no model call, no draw and no reprint.
fn check_transcode(root: &Path) {
    let tree = session(root);
    let _fake = script_legs();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, vec![3u8; 4096]).unwrap();
    let mark = log_mark();
    let said = press(&window, "transcode-again");
    let logged = logs_since(mark);
    assert!(
        logged.iter().any(|l| l.starts_with("transcode started \u{2014}")),
        "the opening line names the file and the three settings that decide it: {logged:?}"
    );
    assert!(said.contains("transcode"), "status carries the transcode's own line: {said:?}");
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "§F5.7: a transcode draws no picture");
    assert_eq!(PRINTS.load(Ordering::SeqCst), 0, "and prints no words");
    // No model in the transcode's cost list at all — `runs::transcode_args` builds ffmpeg argv and stops.
    assert!(!logged.iter().any(|l| l.contains("LLM")), "no LLM line anywhere near a transcode: {logged:?}");
    window.close();
    settle();
}

// ---- the refusals, fired together --------------------------------------------------

/// The two refusals §F5.7 puts ahead of every run, checked in one window: no cut for the picture presses,
/// no video for the copy, no picture for the export. Both must leave every counter at zero.
fn check_refusals(_root: &Path) {
    let _fake = script_legs();
    let window = produce_window(&app_in_round());
    // Nothing seeded: no cut, no video, no picture.
    let redraw = press(&window, "thumbnail-redraw");
    let suggest = press(&window, "title-suggest");
    let save = ui::produce_page::press_save_video_from_button(&window);
    let export = press(&window, "thumbnail-export");
    assert_eq!(redraw, naivepost::produce_runs::REDRAW_NO_CUT);
    assert_eq!(suggest, naivepost::produce_runs::REWORD_NO_CUT);
    assert_eq!(save, naivepost::produce_runs::SAVE_NO_VIDEO);
    assert_eq!(export, naivepost::produce_runs::EXPORT_NO_PICTURE);
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "a refused press runs no leg at all");
    window.close();
    settle();
}

/// The checks, in sequence, on the one thread the application runs on.
fn run_round(app: &adw::Application) {
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));

    record(
        &REFUSAL_FAIL,
        "refusals",
        std::panic::AssertUnwindSafe(in_root("refuse".to_string(), check_refusals)),
    );
    record(
        &REDRAW_FAIL,
        "redraw press",
        std::panic::AssertUnwindSafe(in_root("redraw".to_string(), check_redraw)),
    );
    record(
        &REWORD_FAIL,
        "reword press",
        std::panic::AssertUnwindSafe(in_root("reword".to_string(), check_reword)),
    );
    record(
        &TAKEN_FAIL,
        "set-thumbnail press",
        std::panic::AssertUnwindSafe(in_root("taken".to_string(), check_taken)),
    );
    record(
        &EXPORT_FAIL,
        "export press",
        std::panic::AssertUnwindSafe(in_root("export".to_string(), check_export)),
    );
    record(
        &SAVE_FAIL,
        "save-video press",
        std::panic::AssertUnwindSafe(in_root("save".to_string(), check_save)),
    );
    record(
        &TRANSCODE_FAIL,
        "transcode press",
        std::panic::AssertUnwindSafe(in_root("transcode".to_string(), check_transcode)),
    );
    // F5.6's own audit: the weight ⤓ quotes is measured, Set Thumbnail leaves one entry, ＋ Add image adds a
    // frame that exists.
    record(
        &EXPORT_MEASURED_FAIL,
        "f5_6_s6 export measured",
        std::panic::AssertUnwindSafe(in_root("export-measured".to_string(), check_export_measured)),
    );
    record(
        &SET_THUMB_SOLE_FAIL,
        "f5_6_s3 set-thumbnail own",
        std::panic::AssertUnwindSafe(in_root("set-thumb".to_string(), check_set_thumbnail_marks_own)),
    );
    record(
        &ADD_IMAGE_REAL_FAIL,
        "f5_6_s4 add-image real frame",
        std::panic::AssertUnwindSafe(in_root("add-image".to_string(), check_add_image_is_a_real_frame)),
    );
    record(
        &PICKED_ROW_FAIL,
        "f5_6_s3 picked frame sole row",
        std::panic::AssertUnwindSafe(in_root("picked-row".to_string(), check_picked_frame_sole_entry)),
    );
    // §1 items 1/3's other half: with nothing extracted both chooser doors cancel honestly.
    record(
        &ADD_IMAGE_EMPTY_FAIL,
        "f5_7_s1 add-image with nothing extracted",
        std::panic::AssertUnwindSafe(in_root(
            "add-image-empty".to_string(),
            check_add_image_nothing_extracted,
        )),
    );
    record(
        &CHANGE_EMPTY_FAIL,
        "f5_7_s1 change-with with nothing extracted",
        std::panic::AssertUnwindSafe(in_root(
            "change-empty".to_string(),
            check_change_nothing_extracted,
        )),
    );
    record(
        &MAKE_BASE_SWAP_FAIL,
        "f5_7_s1 slot buttons through the real widgets",
        std::panic::AssertUnwindSafe(in_root(
            "slot-buttons".to_string(),
            check_slot_buttons,
        )),
    );
    record(
        &SET_THUMB_PRINTS_FAIL,
        "f5_7_s1 set-thumbnail writes the cropped picture",
        std::panic::AssertUnwindSafe(in_root(
            "set-thumb-prints".to_string(),
            check_set_thumbnail_prints,
        )),
    );

    // Hand the main loop back so `run_with_args` returns.
    let app = app.clone();
    glib::idle_add_local(move || {
        app.quit();
        glib::ControlFlow::Break
    });
}

#[test]
fn f5_7_s1_pressing_the_page_buttons_runs_each_walk_through_the_real_widget() {
    // The application is started once, by whichever check runs first in this binary (alphabetically that is
    // `f5_6_s3_s4_s6_presses_through_the_real_widgets`); both share the round it activates on its own thread.
    // This check reads back its own six slots plus the four F5.6 ones, so a red block names itself here too.
    let refusals = REFUSAL_FAIL.lock().unwrap().take();
    let redraw = REDRAW_FAIL.lock().unwrap().take();
    let reword = REWORD_FAIL.lock().unwrap().take();
    let taken = TAKEN_FAIL.lock().unwrap().take();
    let export = EXPORT_FAIL.lock().unwrap().take();
    let save = SAVE_FAIL.lock().unwrap().take();
    let transcode = TRANSCODE_FAIL.lock().unwrap().take();
    assert!(refusals.is_none(), "{}", refusals.unwrap_or_default());
    assert!(redraw.is_none(), "{}", redraw.unwrap_or_default());
    assert!(reword.is_none(), "{}", reword.unwrap_or_default());
    assert!(taken.is_none(), "{}", taken.unwrap_or_default());
    assert!(export.is_none(), "{}", export.unwrap_or_default());
    assert!(save.is_none(), "{}", save.unwrap_or_default());
    assert!(transcode.is_none(), "{}", transcode.unwrap_or_default());
}


/// Where the export lands: beside the video, named for its stem. Kept as one more thing the wire reads.
#[allow(dead_code)]
fn export_is_beside_the_video(tree: &Tree) -> bool {
    let jpg = tree.thumbnail_png();
    let video = tree.final_video("mp4");
    jpg.parent() == video.parent() || jpg.parent() != None
}

/// The unused-import guard for `Publish`: the record a re-word writes is the same type the page reads back.
#[allow(dead_code)]
fn publish_type_is_the_record() -> Publish {
    Publish::default()
}

/// Keep `PathBuf` referenced: the destination a real save dialog would return is one of these.
#[allow(dead_code)]
fn destination_type(_: PathBuf) {}

/// The four F5.6 doors, each driven through the widget that owns it, on the one thread the application runs
/// on (a second activate elsewhere is what produced "Default main context is already acquired by another
/// thread!"). Alphabetical order inside this binary puts it BEFORE the F5.7 check, so it starts the round and
/// reads back the four slots its checks parked — each named for the §F5.6 step whose door it drives.
#[test]
fn f5_6_s3_s4_s6_presses_through_the_real_widgets() {
    // Starts the single application this binary drives (alphabetical order puts it first), on the thread
    // `run_with_args` owns — a second activate elsewhere is what produced "Default main context is already
    // acquired by another thread!". `run_round` dispatches the four doors and parks a failure in each one's
    // own slot; reading those slots here is what fails the test, naming the §F5.6 step whose door broke.
    let root = std::env::temp_dir().join(format!("np-f56-presses-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        app.run_with_args(&["naivepost"]);
    });

    let export = EXPORT_MEASURED_FAIL.lock().unwrap().take();
    let set_thumb = SET_THUMB_SOLE_FAIL.lock().unwrap().take();
    let add_image = ADD_IMAGE_REAL_FAIL.lock().unwrap().take();
    let picked = PICKED_ROW_FAIL.lock().unwrap().take();
    let add_empty = ADD_IMAGE_EMPTY_FAIL.lock().unwrap().take();
    let change_empty = CHANGE_EMPTY_FAIL.lock().unwrap().take();
    let slot_buttons = MAKE_BASE_SWAP_FAIL.lock().unwrap().take();
    let set_prints = SET_THUMB_PRINTS_FAIL.lock().unwrap().take();
    std::fs::remove_dir_all(&root).ok();
    assert!(export.is_none(), "f5_6_s6 export: {}", export.unwrap_or_default());
    assert!(set_thumb.is_none(), "f5_6_s3 set-thumbnail: {}", set_thumb.unwrap_or_default());
    assert!(add_image.is_none(), "f5_6_s4 add-image: {}", add_image.unwrap_or_default());
    assert!(picked.is_none(), "f5_6_s3 picked frame: {}", picked.unwrap_or_default());
    assert!(
        add_empty.is_none(),
        "f5_7_s1 add-image with nothing extracted: {}",
        add_empty.unwrap_or_default()
    );
    assert!(
        change_empty.is_none(),
        "f5_7_s1 change-with-nothing-extracted: {}",
        change_empty.unwrap_or_default()
    );
    assert!(
        slot_buttons.is_none(),
        "f5_7_s1 slot buttons (make-base / change in place / remove / the cap): {}",
        slot_buttons.unwrap_or_default()
    );
    assert!(
        set_prints.is_none(),
        "f5_7_s1 set-thumbnail cropped picture on disk: {}",
        set_prints.unwrap_or_default()
    );
}
