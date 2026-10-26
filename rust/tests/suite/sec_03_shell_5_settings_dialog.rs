//! §03-shell.md §5 — the Settings dialog's own statements, as this item's tests.
//!
//! F0.13's rounds already pin the Test buttons (`settings_widgets.rs`, `settings_tests.rs`). What is pinned
//! here is §5 as a section: the no-Save write beat with its 600 ms constant and its close line, the ⓘ that
//! explains each section's server, the dialog's layout, its keys, its width and its close.
//!
//! The conf lives in a throwaway `$XDG_CONFIG_HOME` named for this binary and pid, so no check reads
//! another run's leftovers and none writes the real `$HOME`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use crate::common;
use adw::prelude::*;
use common::{fixture_dir, settle};
use gtk4 as gtk;
use naivepost::checks;
use naivepost::ui;
use naivepost::ui::settings::Provider;
use naivepost::{machine_store, settings};

const ITEM: &str = "sec_03_shell_5_settings_dialog";

// ---------------------------------------------------------------------------
// s1 — "No Save/Cancel: every box written 600 ms after the last keystroke and on close."
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_5_settings_dialog_s1_the_write_is_600ms_after_the_last_keystroke_and_on_close() {
    // §5 states the delay outright; it is the constant the dialog arms its timer with.
    assert_eq!(
        checks::CONF_SAVE_WAIT,
        Duration::from_millis(600),
        "{ITEM} s1: §5 says 600 ms after the last keystroke"
    );

    // One keystroke arms exactly one owed write, due 600 ms later, and a second keystroke pushes it
    // back rather than queueing a second file write.
    let mut state = checks::SaveState::default();
    assert_eq!(state.due_at(), None, "nothing owed before the first key");
    state.touched(Duration::from_secs(0));
    assert_eq!(
        state.due_at(),
        Some(Duration::from_millis(600)),
        "one keystroke owes a write 600 ms out"
    );
    state.touched(Duration::from_millis(200));
    assert_eq!(
        state.due_at(),
        Some(Duration::from_millis(800)),
        "the next keystroke moves the same write back — one write per pause, not per letter"
    );

    // And the store says where it wrote, which is the whole of §5's "settings saved to <path>". The
    // beat has to be armed first -- `on_close` writes only what a keystroke owes (§5: the close lands
    // what is still owed, and an untouched dialog owes nothing).
    let root = pin_config_home("s1");
    let paths = settings::Paths {
        config_dir: root.join("cfg").join("naivepost"),
        data_dir: root.join("data"),
    };
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    let mut store = machine_store::Store::open(&paths).expect("a store over the throwaway conf");
    store.touched(Duration::from_millis(0));
    assert_eq!(
        store.due_at(),
        Some(Duration::from_millis(600)),
        "one keystroke arms the same beat"
    );
    // The dialog hands `on_close` EVERY box's current text (its own `typed_all`), not only what changed:
    // the store writes the whole file, so an unrecognised key would be a row that stores nothing.
    let said = store
        .on_close(&[("audio".to_string(), "http://127.0.0.1:9001".to_string())])
        .expect("the close write succeeds")
        .expect("closing lands what the last keystroke owes");
    assert_eq!(
        said,
        format!("settings saved to {}", paths.conf_path().display()),
        "{ITEM} s1: the close line names the file it wrote"
    );
    let conf = settings::read(&paths).expect("the conf reads back");
    assert_eq!(
        conf.audio_server, "http://127.0.0.1:9001",
        "and the typed value is in it"
    );
    // Nothing owed now: closing twice does not say it saved twice.
    assert_eq!(
        store
            .on_close(&[])
            .expect("the second close is fine")
            .map(|_| ()),
        None,
        "{ITEM} s1: a close with nothing owed says nothing (§5: only \"if a write is still owed\")"
    );
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// s2 — "Each section's ⓘ explains at length what the server is expected to speak."
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_5_settings_dialog_s2_every_section_info_says_what_its_server_must_speak() {
    use naivepost::ui::settings_info;

    let mut seen: Vec<&str> = Vec::new();
    for (key, _) in settings_info::SECTIONS {
        let text = settings_info::explanation(key)
            .unwrap_or_else(|| panic!("{ITEM} s2: section {key} has no ⓘ text"));
        assert!(
            text.chars().count() > 120,
            "{ITEM} s2: {key}'s ⓘ is a paragraph in §5's words, got {} chars",
            text.chars().count()
        );
        seen.push(text);
    }
    for (i, a) in seen.iter().enumerate() {
        for b in seen.iter().skip(i + 1) {
            assert_ne!(
                a, b,
                "{ITEM} s2: two sections share an explanation, so one of them lies"
            );
        }
    }

    // Each one names the endpoints that server has to answer — the part a person cannot guess from an
    // address box. spec/02-services.md §1's table is the source.
    let writing = settings_info::explanation("writing").unwrap();
    assert!(
        writing.contains("/v1/chat/completions") && writing.contains("/v1/models"),
        "{ITEM} s2: the writing ⓘ must name the chat and models endpoints: {writing}"
    );
    let cutting = settings_info::explanation("cutting").unwrap();
    assert!(
        cutting.contains("ffprobe")
            && cutting.contains("rubberband")
            && cutting.contains("libx264"),
        "{ITEM} s2: the cutting ⓘ must name ffprobe and what the build has to carry: {cutting}"
    );
    assert!(
        cutting.contains("off"),
        "{ITEM} s2: firefox \"off\" is a valid answer and must be said: {cutting}"
    );
    let audio = settings_info::explanation("audio").unwrap();
    assert!(
        audio.contains("/health") && audio.contains("/v1/models") && audio.contains("task"),
        "{ITEM} s2: the audio ⓘ must name health, the catalogue and the task each model declares: {audio}"
    );
    let drawing = settings_info::explanation("drawing").unwrap();
    assert!(
        drawing.contains("/sdcpp/v1/capabilities") && drawing.to_lowercase().contains("openai"),
        "{ITEM} s2: the drawing ⓘ must name sd-server's capabilities call and call out an OpenAI-shaped port: {drawing}"
    );
    // An unknown section gets no ⓘ rather than someone else's.
    assert!(settings_info::explanation("nope").is_none());
}

// ---------------------------------------------------------------------------
// The widget half: the header's Settings button, the ⓘ controls, the layout.
// ---------------------------------------------------------------------------

fn widgets_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let root = pin_config_home("widgets");
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_the_gear_opens_the_dialog(app);
            RAN_OPEN.store(true, Ordering::SeqCst);
            check_every_section_has_its_info_button(app);
            RAN_INFO.store(true, Ordering::SeqCst);
            // §5's layout statements, all measured off the one open dialog.
            let window = open_window(app);
            ui::open_settings(&window, silent());
            lay_out();
            check_the_sections_are_divided_by_lines(&window);
            RAN_DIVIDER.store(true, Ordering::SeqCst);
            check_the_field_labels_are_right_aligned(&window);
            RAN_LABELS.store(true, Ordering::SeqCst);
            check_the_badges_and_tests_share_one_column(&window);
            RAN_COLUMN.store(true, Ordering::SeqCst);
            check_the_writing_last_line_uses_its_columns(&window);
            RAN_FETCH_ROW.store(true, Ordering::SeqCst);
            check_test_all_is_bottom_right_and_the_log_is_last(&window);
            RAN_TEST_ALL.store(true, Ordering::SeqCst);
            check_every_server_has_its_key_and_the_dialog_is_wide(&window);
            RAN_KEYS.store(true, Ordering::SeqCst);
            check_the_close_button_and_escape_close_it(&window);
            RAN_CLOSE.store(true, Ordering::SeqCst);
            ui::open_settings(&window, silent());
            lay_out();
            // §5's Drawing row, over the wire: the same dialog, but answering from the provider a real
            // session installs (`window_policy.rs` passes exactly this one when the gear is pressed).
            close_dialog();
            install_live_conf();
            ui::open_settings(&window, naivepost::settings_probe::live_provider());
            lay_out();
            check_the_drawing_test_button_follows_its_badge(&window);
            RAN_SD_ORDER.store(true, Ordering::SeqCst);
            check_the_drawing_row_tests_sd_server(&window);
            RAN_SD_OK.store(true, Ordering::SeqCst);
            check_a_port_that_is_not_sd_server_is_called_out(&window);
            RAN_SD_BAD.store(true, Ordering::SeqCst);
            close_dialog();
            window.close();
            app.quit();
        });
        app.run_with_args::<String>(&[]);
        let _ = std::fs::remove_dir_all(&root);
    });
}

static RAN_OPEN: AtomicBool = AtomicBool::new(false);
static RAN_INFO: AtomicBool = AtomicBool::new(false);
static RAN_DIVIDER: AtomicBool = AtomicBool::new(false);
static RAN_LABELS: AtomicBool = AtomicBool::new(false);
static RAN_COLUMN: AtomicBool = AtomicBool::new(false);
static RAN_FETCH_ROW: AtomicBool = AtomicBool::new(false);
static RAN_TEST_ALL: AtomicBool = AtomicBool::new(false);
static RAN_KEYS: AtomicBool = AtomicBool::new(false);
static RAN_CLOSE: AtomicBool = AtomicBool::new(false);

/// A conf with a model id, written into this round's throwaway config home before the live dialog opens.
/// §5 refuses a completion with no model id, and the dialog seeds its boxes from the file, so it has to
/// be down BEFORE `open_settings` — same order `four_servers_probe_widgets.rs` uses.
fn install_live_conf() {
    let paths =
        naivepost::settings::from_environment().expect("{ITEM} s7: a pinned XDG_CONFIG_HOME");
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "sd-test-model".to_string();
    naivepost::settings::save(&paths, &conf).expect("{ITEM} s7: the conf writes");
}

/// The s7 rounds. GTK's main loop may only be run by the thread that starts it, and only ONCE per
/// process — so these checks join `widgets_round`, the one application this binary runs, instead of
/// starting a second one (a second `run_with_args` returns at once and nothing runs).
fn live_round() {
    widgets_round();
}

static RAN_SD_OK: AtomicBool = AtomicBool::new(false);
static RAN_SD_BAD: AtomicBool = AtomicBool::new(false);
static RAN_SD_ORDER: AtomicBool = AtomicBool::new(false);

/// The open dialog's window — its own top level, so bounds are taken against it.
fn dialog() -> adw::Window {
    ui::settings::open_settings_window().expect("{ITEM}: no settings dialog is open")
}

/// Let GTK lay the dialog out. `present()` alone does not allocate until an iteration runs, so without
/// this every bound below would read 0.
fn lay_out() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

/// A widget inside the open dialog, by name, or a failure naming what was missing.
fn need(name: &str) -> gtk::Widget {
    in_dialog(name).unwrap_or_else(|| panic!("{ITEM}: the dialog has no {name}"))
}

/// Bounds of one dialog widget, printed so the log carries the numbers a check decided on.
fn bounds(widget: &gtk::Widget, what: &str) -> gtk::graphene::Rect {
    let b = widget
        .compute_bounds(&dialog())
        .unwrap_or_else(|| panic!("{ITEM}: {what} is not laid out"));
    println!(
        "SETTINGS GRID {what} x={:.1} y={:.1} w={:.1} h={:.1}",
        b.x(),
        b.y(),
        b.width(),
        b.height()
    );
    b
}

/// The LEFT edge of a dialog widget, printed so the log carries it.
fn left(widget: &gtk::Widget, what: &str) -> f32 {
    bounds(widget, what).x()
}

/// Vertical centre, which is how "on one line" is asked of two widgets.
fn centre(b: &gtk::graphene::Rect) -> f32 {
    b.y() + b.height() / 2.0
}

/// Find the field caption showing `text` (the label column's right-aligned captions).
fn caption_showing(text: &str) -> gtk::Label {
    fn walk(root: &gtk::Widget, text: &str) -> Option<gtk::Label> {
        if let Some(label) = root.downcast_ref::<gtk::Label>() {
            if label.text() == text {
                return Some(label.clone());
            }
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else {
                continue;
            };
            if let Some(found) = walk(&widget, text) {
                return Some(found);
            }
        }
        None
    }
    walk(dialog().upcast_ref(), text).unwrap_or_else(|| panic!("{ITEM}: no field label {text:?}"))
}

fn pin_config_home(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("np-sec035-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("cfg").join("naivepost")).expect("config root");
    std::fs::create_dir_all(root.join("data")).expect("data root");
    std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
    std::env::set_var("XDG_DATA_HOME", root.join("data"));
    root
}

/// A provider that answers nothing: these checks press the gear and read the layout, and neither needs a server.
fn silent() -> Provider {
    std::sync::Arc::new(|key, _| Err(format!("{ITEM}: nothing to probe ({key})")))
}

fn open_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    window
}

/// A named widget inside the open dialog — its own top level, so the main window's tree cannot reach it.
fn in_dialog(name: &str) -> Option<gtk::Widget> {
    let dialog = ui::settings::open_settings_window()?;
    walk(dialog.upcast_ref(), name)
}

fn walk(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Some(found) = walk(&widget, name) {
            return Some(found);
        }
    }
    None
}

fn close_dialog() {
    if let Some(dialog) = ui::settings::open_settings_window() {
        dialog.close();
        settle();
    }
}

/// A loopback fake answering ONE answer per path, so a row that asks for something off-table gets a 404
/// rather than another path's healthy reply. Same shape as `four_servers_probe_widgets.rs`'s; copied here
/// because that one is private to its own test file and this item's sd cases need three different tables.
struct Live {
    port: u16,
    hits: mpsc::Receiver<String>,
}

impl Live {
    fn start(routes: &[(&str, &str)]) -> Live {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, hits) = mpsc::channel();
        let table: Vec<(String, String)> = routes
            .iter()
            .map(|(path, body)| (path.to_string(), body.to_string()))
            .collect();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let sender = sender.clone();
                let table = table.clone();
                std::thread::spawn(move || {
                    if let Some(path) = answer(stream, &table) {
                        let _ = sender.send(path);
                    }
                });
            }
        });
        Live { port, hits }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The paths this server was asked for, in order, waiting up to 3 s each for them to arrive.
    fn asked(&self, want: usize) -> Vec<String> {
        let mut got = Vec::new();
        for _ in 0..want {
            match self.hits.recv_timeout(Duration::from_secs(3)) {
                Ok(path) => got.push(path),
                Err(_) => break,
            }
        }
        got
    }
}

/// Serve one request from the path table; report the path that was asked for. An unknown path answers
/// 404, which is what lets a refusal case tell "answered /v1/models, not the sdcpp API" from "dead".
fn answer(stream: std::net::TcpStream, table: &[(String, String)]) -> Option<String> {
    let peer = stream.try_clone().ok()?;
    let mut reader = BufReader::new(peer);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    let mut declared = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                declared = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut raw = vec![0u8; declared];
    if declared > 0 {
        let _ = reader.read_exact(&mut raw);
    }
    let known = table.iter().any(|(known, _)| *known == path);
    // The leg keeps a non-2xx body (`http_status_as_error(false)` upstream), so an off-table path has to
    // answer like a real server with nothing to say: empty, not a JSON error object. A body there would
    // parse as a capabilities reply that merely names no weights -- a DIFFERENT verdict from "this port
    // does not speak sd-server's API", which is the distinction this case exists to check.
    let body = table
        .iter()
        .find(|(known, _)| *known == path)
        .map(|(_, body)| body.clone())
        .unwrap_or_default();
    let status = if known { 200 } else { 404 };
    let reason = if known { "OK" } else { "Not Found" };
    let reply = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {len}\r\nconnection: close\r\n\r\n{body}",
        len = body.len()
    );
    let mut writer = stream;
    let _ = writer.write_all(reply.as_bytes());
    let _ = writer.flush();
    Some(path)
}

/// §5's entry point: the header's Settings gear opens the dialog, and closing it lands the write.
fn check_the_gear_opens_the_dialog(app: &adw::Application) {
    let window = open_window(app);
    let gear = settings_button(&window);
    gear.emit_by_name::<()>("clicked", &[]);
    assert!(
        ui::dialog_open(&window),
        "{ITEM} s1: pressing the gear opened the Settings dialog"
    );
    // No Save, no Cancel: the dialog's whole write story is the beat and the close.
    for name in ["save-button", "cancel-button"] {
        assert!(
            in_dialog(name).is_none(),
            "{ITEM} s1: §5 has no {name}; every box writes by itself"
        );
    }
    // One keystroke into a box, then close: the close lands what that keystroke owed and says where.
    // The box must be one the dialog watches for keystrokes -- those are the rows with a Test button
    // (`test_keys`), so the audio SERVER row is used here; §5's rule is about any box, not this one.
    let before = ui::window_logs().len();
    let typed = "http://127.0.0.1:9109";
    let audio = ui::settings::entry(&window, "audio")
        .expect("{ITEM} s1: the dialog has its audio Server box");
    audio.set_text(typed);
    settle();
    close_dialog();
    let said = ui::window_logs()[before..]
        .iter()
        .find(|line| line.contains("settings saved to"))
        .cloned()
        .unwrap_or_default();
    assert!(
        said.starts_with("settings: settings saved to"),
        "{ITEM} s1: closing wrote and mirrored the line into the main log as §5 words it, logs: {:?}",
        &ui::window_logs()[before..]
    );
    let paths = settings::from_environment().expect("{ITEM}: the pinned config home resolves");
    let conf = settings::read(&paths).expect("the conf reads back");
    assert_eq!(
        conf.audio_server, typed,
        "{ITEM} s1: what was typed before the close is what the file holds"
    );
    window.close();
}

fn settings_button(window: &adw::ApplicationWindow) -> gtk::Button {
    ui::settings::settings_button(window)
        .unwrap_or_else(|| panic!("{ITEM}: the header has no settings-button"))
}

/// §5's ⓘ: one per section, in the heading's own row, carrying the paragraph as its tooltip, and opening it
/// in a popover when clicked.
fn check_every_section_has_its_info_button(app: &adw::Application) {
    let window = open_window(app);
    ui::open_settings(&window, silent());
    assert!(
        ui::dialog_open(&window),
        "{ITEM} s2: the dialog did not open"
    );
    for (key, title) in naivepost::ui::settings_info::SECTIONS {
        let info = in_dialog(&format!("settings-info-{key}"))
            .unwrap_or_else(|| panic!("{ITEM} s2: no settings-info-{key} on the dialog"))
            .downcast::<gtk::MenuButton>()
            .unwrap_or_else(|_| panic!("{ITEM} s2: settings-info-{key} is not a menu button"));
        let want = naivepost::ui::settings_info::explanation(key).expect("the section's text");
        let got = info
            .tooltip_text()
            .map(|text| text.to_string())
            .unwrap_or_default();
        assert_eq!(
            got, want,
            "{ITEM} s2: the {key} ⓘ carries its own explanation as its tooltip"
        );
        assert!(
            got.chars().count() > 120,
            "{ITEM} s2: {key}'s ⓘ is at length"
        );
        // A click opens the same paragraph: a tooltip alone answered a click with nothing.
        info.popup();
        common::settle();
        let shown = info.popover().expect("the ⓘ has its popover");
        assert!(
            shown.is_visible(),
            "{ITEM} s2: clicking the {key} ⓘ shows its explanation"
        );
        let words = shown
            .child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .map(|label| label.text().to_string())
            .unwrap_or_default();
        assert_eq!(
            words, want,
            "{ITEM} s2: the popover holds the {key} explanation"
        );
        info.popdown();
        common::settle();

        // In the heading's row: the box named for the section holds both the title and the ⓘ.
        let head = in_dialog(&format!("settings-section-{key}"))
            .unwrap_or_else(|| panic!("{ITEM} s2: no settings-section-{key} row box"));
        assert!(
            holds(&head, &format!("settings-info-{key}")),
            "{ITEM} s2: the {key} ⓘ sits beside its heading, not elsewhere on the grid"
        );
        assert!(
            holds_text(&head, title),
            "{ITEM} s2: the {key} row still shows the heading {title:?}"
        );
    }
    close_dialog();
    window.close();
}

/// Whether `root`'s subtree carries a widget with `name`.
fn holds(root: &gtk::Widget, name: &str) -> bool {
    walk(root, name).is_some()
}
/// Whether `root`'s subtree shows `text` in one of its labels.
fn holds_text(root: &gtk::Widget, text: &str) -> bool {
    if let Some(label) = root.downcast_ref::<gtk::Label>() {
        if label.text() == text {
            return true;
        }
    }
    root.observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .any(|child| {
            let Ok(widget) = child.downcast::<gtk::Widget>() else {
                return false;
            };
            holds_text(&widget, text)
        })
}

#[test]
fn sec_03_shell_5_settings_dialog_s1_the_settings_button_opens_the_dialog_that_saves_on_close() {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_OPEN.load(Ordering::SeqCst),
        "{ITEM} s1: the gear check never ran"
    );
}

#[test]
fn sec_03_shell_5_settings_dialog_s2_every_section_carries_an_info_button_that_explains_its_server()
{
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_INFO.load(Ordering::SeqCst),
        "{ITEM} s2: the ⓘ check never ran"
    );
}

// ---------------------------------------------------------------------------
// s6 — §5's layout of the dialog itself: "One window 'Settings', top to bottom: the sections …
// divided by separator lines; then Test All at the bottom right; then the log (collapsed) last.
// The sections share one grid of five columns …". Measured off a real dialog for the same reason
// `sources_list.rs` measures its rows: a grid that renders and is misaligned renders fine.
// ---------------------------------------------------------------------------

/// A line above each section but the first, spanning the dialog, sitting between the sections it divides.
fn check_the_sections_are_divided_by_lines(window: &adw::ApplicationWindow) {
    let _ = window;
    assert!(
        in_dialog("settings-separator-writing").is_none(),
        "{ITEM} s6: Writing is the first section; nothing divides it from the title"
    );
    // Each divider introduces its section: below the previous section's heading, above its own.
    let order = ["writing", "cutting", "audio", "drawing"];
    for index in 1..order.len() {
        let here = order[index];
        let before = order[index - 1];
        let line = need(&format!("settings-separator-{here}"));
        let separator = line
            .clone()
            .downcast::<gtk::Separator>()
            .unwrap_or_else(|_| panic!("{ITEM} s6: settings-separator-{here} is not a Separator"));
        assert_eq!(
            separator.orientation(),
            gtk::Orientation::Horizontal,
            "{ITEM} s6: the {here} divider is horizontal"
        );
        let b = bounds(&line, &format!("separator-{here}"));
        assert!(
            b.width() > 400.0,
            "{ITEM} s6: the {here} divider spans {:.1} px, not the dialog",
            b.width()
        );
        let previous = bounds(
            &need(&format!("settings-section-{before}")),
            &format!("section-{before}"),
        );
        let own = bounds(
            &need(&format!("settings-section-{here}")),
            &format!("section-{here}"),
        );
        assert!(
            b.y() > previous.y() && b.y() < own.y(),
            "{ITEM} s6: the {here} divider sits between the sections, not at {:.1}",
            b.y()
        );
    }
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_the_sections_are_divided_by_separator_lines() {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_DIVIDER.load(Ordering::SeqCst),
        "{ITEM} s6: the divider check never ran"
    );
}

/// §5: the field label column is right-aligned — every caption ends on the same edge.
fn check_the_field_labels_are_right_aligned(window: &adw::ApplicationWindow) {
    let _ = window;
    let mut edges: Vec<f32> = vec![];
    for text in ["Server", "API key", "Model", "Vision", "ffmpeg", "firefox"] {
        let label = caption_showing(text);
        assert_eq!(
            label.halign(),
            gtk::Align::End,
            "{ITEM} s6: the {text:?} caption is not right-aligned"
        );
        let b = bounds(label.upcast_ref(), &format!("caption-{text}"));
        edges.push(b.x() + b.width());
    }
    let widest = edges.iter().cloned().fold(f32::MIN, f32::max);
    for edge in &edges {
        assert!(
            (widest - edge).abs() <= 1.0,
            "{ITEM} s6: captions end at {:?} — they do not share one right edge",
            edges
        );
    }
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_the_field_labels_are_right_aligned() {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_LABELS.load(Ordering::SeqCst),
        "{ITEM} s6: the label check never ran"
    );
}

/// One grid, so every row lands its badge and its Test button in the same two columns — otherwise the marks
/// would step sideways down the dialog.
fn check_the_badges_and_tests_share_one_column(window: &adw::ApplicationWindow) {
    let _ = window;
    for prefix in ["badge", "test"] {
        // Every kind of row lands on the same x, which is what makes §5's five columns one set of columns.
        let keys = ["llm", "llm-vision", "ffmpeg", "firefox", "audio", "sd"];
        let xs: Vec<f32> = keys
            .iter()
            .map(|key| {
                let name = if prefix == "badge" {
                    format!("badge-{key}")
                } else {
                    format!("test-{key}-button")
                };
                bounds(need(&name).upcast_ref(), &name).x()
            })
            .collect();
        let left = xs.iter().cloned().fold(f32::MAX, f32::min);
        for (key, x) in keys.iter().zip(&xs) {
            assert!(
                (x - left).abs() <= 1.0,
                "{ITEM} s6: {prefix} of {key} sits at {x}, others at {left}: {xs:?}"
            );
        }
    }
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_the_badges_and_the_test_buttons_share_one_column() {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_COLUMN.load(Ordering::SeqCst),
        "{ITEM} s6: the shared-column check never ran"
    );
}

/// §5's Writing last line: Fetch models in the label column, the dropdown in the box column, Use in
/// the Test column.
fn check_the_writing_last_line_uses_its_columns(window: &adw::ApplicationWindow) {
    let _ = window;
    let server = bounds(need("entry-server").upcast_ref(), "entry-server");
    // A right-aligned caption starts wherever its text happens to begin; the column's own left edge is
    // the widest thing in it, which here is the Fetch models button (§5 puts that button in this column).
    let label_x = left(caption_showing("Server").upcast_ref(), "caption-Server");
    let fetch = bounds(
        need("fetch-models-button").upcast_ref(),
        "fetch-models-button",
    );
    let list = bounds(need("model-list").upcast_ref(), "model-list");
    let use_button = bounds(need("use-model-button").upcast_ref(), "use-model-button");
    let test_x = bounds(need("test-llm-button").upcast_ref(), "test-llm-button").x();
    assert!(
        fetch.x() <= label_x + 1.0 && fetch.x() + fetch.width() <= server.x(),
        "{ITEM} s6: Fetch models is not inside the label column ({fetch:?}, caption at {label_x}, box at {})",
        server.x()
    );
    assert!(
        (list.x() - server.x()).abs() <= 1.0,
        "{ITEM} s6: the model dropdown is not in the box column ({list:?} vs {server:?})"
    );
    assert!(
        (use_button.x() - test_x).abs() <= 1.0,
        "{ITEM} s6: Use is not in the Test column ({use_button:?} vs Test at {test_x})"
    );
    // Every box is one line tall: a section's server box sits beside the heading and its entry row, and when
    // those two lines shared the server row alone the box stretched to twice its height.
    let line = bounds(need("entry-llm").upcast_ref(), "entry-llm").height();
    for name in ["entry-server", "entry-audio", "entry-sd"] {
        let h = bounds(need(name).upcast_ref(), name).height();
        assert!(
            (h - line).abs() <= 1.0,
            "{ITEM} s6: {name} is {h} px tall where a box is {line} px"
        );
    }
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_the_writing_last_line_puts_fetch_the_dropdown_and_use_in_their_columns(
) {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_FETCH_ROW.load(Ordering::SeqCst),
        "{ITEM} s6: the Fetch-row check never ran"
    );
}

/// "then Test All at the bottom right; then the log (collapsed) last."
fn check_test_all_is_bottom_right_and_the_log_is_last(window: &adw::ApplicationWindow) {
    let _ = window;
    let top = dialog();
    let dialog_b = bounds(top.upcast_ref(), "dialog");
    let all = need("test-all-button");
    let all_b = bounds(all.upcast_ref(), "test-all-button");
    assert!(
        dialog_b.x() + dialog_b.width() - (all_b.x() + all_b.width()) <= 30.0,
        "{ITEM} s6: Test All ends {:.1} px short of the right edge — it belongs at the bottom right",
        dialog_b.x() + dialog_b.width() - (all_b.x() + all_b.width())
    );
    let expander = need("settings-log-expander");
    let expander_b = bounds(expander.upcast_ref(), "settings-log-expander");
    let drawing = bounds(
        need("settings-section-drawing").upcast_ref(),
        "section-drawing",
    );
    assert!(
        all_b.y() > drawing.y() && all_b.y() < expander_b.y(),
        "{ITEM} s6: Test All is not between the sections and the log ({} / {} / {})",
        drawing.y(),
        all_b.y(),
        expander_b.y()
    );
    let expander_widget = expander
        .clone()
        .downcast::<gtk::Expander>()
        .expect("{ITEM} s6: settings-log-expander is an Expander");
    assert!(
        !expander_widget.is_expanded(),
        "{ITEM} s6: §5 opens the dialog's log collapsed"
    );
    // Ten lines tall however many it holds: a Test All that writes thirty lines scrolls the log, it does
    // not stretch it (the first version grew a line per verdict and pushed the sections up).
    expander_widget.set_expanded(true);
    let view = need("settings-log")
        .downcast::<gtk::TextView>()
        .expect("{ITEM} s6: settings-log is a TextView");
    let scroller = need("settings-log-scroller");
    common::wait_until("the opened log is laid out", || scroller.height() > 0);
    let empty = bounds(&scroller, "settings-log-scroller empty").height();
    let lines: Vec<String> = (1..=40).map(|n| format!("settings: line {n}")).collect();
    view.buffer().set_text(&lines.join("\n"));
    // Laid out once the forty lines are measured: the view's own scroll range then runs past one page.
    common::wait_until("the forty lines are measured", || {
        view.vadjustment()
            .is_some_and(|adj| adj.upper() > adj.page_size() * 2.0)
    });
    let full = bounds(&scroller, "settings-log-scroller full").height();
    let ten = ui::settings::log_lines_px(&view) as f32;
    assert_eq!(
        full, empty,
        "{ITEM} s6: the log grew from {empty} to {full} px with 40 lines"
    );
    assert!(
        (full - ten).abs() <= 2.0,
        "{ITEM} s6: the log is {full} px, ten lines are {ten} px"
    );
    view.buffer().set_text("");
    expander_widget.set_expanded(false);
    lay_out();
    // Last: the root box's final child is the expander.
    let root = expander
        .parent()
        .expect("{ITEM} s6: the expander sits in the dialog's root box");
    assert_eq!(
        root.last_child().as_ref(),
        Some(expander.upcast_ref()),
        "{ITEM} s6: the log is not the last thing in the dialog"
    );
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_test_all_is_at_the_bottom_right_and_the_log_is_last_and_collapsed(
) {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_TEST_ALL.load(Ordering::SeqCst),
        "{ITEM} s6: the Test All / log check never ran"
    );
}

/// §5 (inventory §D): each server has an API key box under it, masked like a password and stored in its
/// own setting, and the dialog opens 960 wide.
fn check_every_server_has_its_key_and_the_dialog_is_wide(window: &adw::ApplicationWindow) {
    for (key, stored) in [
        ("llm-key", "the writing key"),
        ("audio-key", "the audio key"),
        ("sd-key", "the drawing key"),
    ] {
        let entry = ui::settings::entry(window, key)
            .unwrap_or_else(|| panic!("{ITEM} s6: no API key box for {stored}"));
        assert!(
            !entry.property::<bool>("visibility"),
            "{ITEM} s6: {stored} is masked"
        );
        let mut conf = naivepost::settings::Conf::default();
        assert!(
            naivepost::machine_store::set_field(&mut conf, key, "secret-1"),
            "{ITEM} s6: {stored} is stored"
        );
        let read = naivepost::machine_store::field_of(key).expect("a stored field");
        assert_eq!(read(&conf), "secret-1", "{ITEM} s6: {stored} reads back");
    }
    // …and the dialog itself takes every one of them as typed, the Server box included: a Test reads
    // them, and the 600 ms beat writes them back.
    for key in ["server", "llm-key", "audio-key", "sd-key"] {
        assert!(
            ui::settings::typed_values().iter().any(|(k, _)| k == key),
            "{ITEM} s6: the {key} box is one the dialog reads and writes"
        );
    }
    assert_eq!(
        ui::settings::open_settings_window()
            .expect("the dialog is open")
            .default_width(),
        ui::settings::DIALOG_WIDTH_PX,
        "{ITEM} s6: the dialog opens {} wide",
        ui::settings::DIALOG_WIDTH_PX
    );
}

/// §5's picture: a title bar with its close button; and Esc closes the dialog as that button does.
fn check_the_close_button_and_escape_close_it(_window: &adw::ApplicationWindow) {
    let dialog = ui::settings::open_settings_window().expect("{ITEM} s6: the dialog is open");
    let header = in_dialog("settings-header")
        .and_then(|w| w.downcast::<adw::HeaderBar>().ok())
        .unwrap_or_else(|| panic!("{ITEM} s6: the dialog has no title bar"));
    assert!(
        header.shows_end_title_buttons(),
        "{ITEM} s6: the title bar carries the close button"
    );
    // Esc, through the dialog's own shortcut: the same close request the close button sends.
    let escape = dialog
        .observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|c| c.downcast::<gtk::ShortcutController>().ok())
        .find(|c| c.name().as_deref() == Some("settings-escape"))
        .unwrap_or_else(|| panic!("{ITEM} s6: no Esc shortcut on the dialog"));
    let shortcut = escape
        .item(0)
        .and_then(|item| item.downcast::<gtk::Shortcut>().ok())
        .expect("the Esc shortcut");
    assert_eq!(
        shortcut
            .trigger()
            .map(|t| t.to_str().to_string())
            .as_deref(),
        Some("Escape"),
        "{ITEM} s6: the shortcut is Esc"
    );
    shortcut.action().expect("the shortcut's action").activate(
        gtk::ShortcutActionFlags::empty(),
        &dialog,
        None,
    );
    settle();
    assert!(!dialog.is_visible(), "{ITEM} s6: Esc closed the dialog");
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_the_close_button_and_escape_close_the_dialog() {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_CLOSE.load(Ordering::SeqCst),
        "{ITEM} s6: the close check never ran"
    );
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_every_server_has_its_api_key_and_the_dialog_opens_wide() {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_KEYS.load(Ordering::SeqCst),
        "{ITEM} s6: the API key check never ran"
    );
}

// ---------------------------------------------------------------------------
// s7 — §5's Drawing section, tested through its own Test button (F0.13's sd.cpp row).
//
// The gap this closes is a wiring one: a Test button exists only once the Drawing row passes a test
// key, and the sd verdict (`<weights> is loaded and can draw`, or the OpenAI-shaped call-out) has to be
// reachable by a press rather than only by a unit call. So these go out over a loopback socket through
// the provider a real session installs (`settings_probe::live_provider`), exactly as `window_policy.rs`
// installs it when the gear is pressed.
// ---------------------------------------------------------------------------

/// Press the Drawing row's own Test with `url` typed in its box, and hand back the badge's mark and
/// tooltip — the state that press changes. The 600 ms save beat is left un-armed on purpose: a check that
/// ran the main loop for it would write into whatever `$XDG_CONFIG_HOME` this process points at.
fn press_sd(window: &adw::ApplicationWindow, url: &str) -> (String, String) {
    // The button must EXIST under §5's name: that is the wire the completion check said was missing.
    let test = ui::test_button(window, "sd")
        .expect("{ITEM} s7: the Drawing row carries a Test button named test-sd-button");
    ui::entry(window, "sd")
        .expect("{ITEM} s7: the Drawing row has its value box")
        .set_text(url);
    test.emit_clicked();
    crate::common::settle();
    let badge = ui::badge(window, "sd").expect("{ITEM} s7: the Drawing row has a badge-sd");
    (
        badge.text().to_string(),
        badge.tooltip_text().unwrap_or_default().to_string(),
    )
}

/// F0.13 through the widget: the Drawing row's Test dials sd-server's OWN capabilities endpoint and
/// says which weights are loaded — nothing asked of the OpenAI-shaped API on the way.
fn check_the_drawing_row_tests_sd_server(window: &adw::ApplicationWindow) {
    let server = Live::start(&[(
        "/sdcpp/v1/capabilities",
        r#"{"model":{"name":"sd-v1.gguf","path":"/mnt/models/sd-v1.gguf","stem":"sd-v1"},"supported_modes":["img_gen"]}"#,
    )]);
    let before = ui::window_logs().len();
    let (mark, verdict) = press_sd(window, &server.url());
    assert_eq!(
        server.asked(1),
        vec!["/sdcpp/v1/capabilities".to_string()],
        "{ITEM} s7: the Drawing row probes sd-server's own API and nothing else"
    );
    assert_eq!(
        mark, "\u{2713}",
        "{ITEM} s7: loaded weights mark \u{2713}, got {verdict:?}"
    );
    assert!(
        verdict.contains("sd-v1 is loaded and can draw"),
        "{ITEM} s7: the verdict names the weights the server reported: {verdict}"
    );
    // §5 mirrors each test into the main log as "settings: …", naming this row.
    let mirrored: Vec<String> = ui::window_logs()[before..].to_vec();
    assert!(
        mirrored
            .iter()
            .any(|line| line.starts_with(checks::LOG_PREFIX) && line.contains("sd.cpp")),
        "{ITEM} s7: the press is mirrored into the main log: {mirrored:?}"
    );
}

#[test]
fn sec_03_shell_5_settings_dialog_s7_the_drawing_row_tests_sd_server_through_its_own_button() {
    common::pin_private_cwd();
    live_round();
    assert!(
        RAN_SD_OK.load(Ordering::SeqCst),
        "{ITEM} s7: the sd.cpp pass case never ran"
    );
}

/// F0.13's other two sd answers, both through the same button: a port that answers `/v1/models` but not
/// the sdcpp API is called out as not sd-server (it needs a different fix), and a port where nothing
/// listens says so plainly. Each asserts its own badge, so neither reads another case's state.
fn check_a_port_that_is_not_sd_server_is_called_out(window: &adw::ApplicationWindow) {
    // Only the OpenAI-shaped route answers; sd-server's own path comes back 404 from the fake.
    let openai = Live::start(&[("/v1/models", r#"{"data":[{"id":"gpt-ish"}]}"#)]);
    let (mark, verdict) = press_sd(window, &openai.url());
    assert_eq!(
        mark, "\u{2717}",
        "{ITEM} s7: an OpenAI-shaped port is a failure, got {verdict}"
    );
    assert!(
        verdict.contains("not sd-server"),
        "{ITEM} s7: the reason names what the port actually is: {verdict}"
    );
    // Both calls really went out: sd-server's API first, then the listing that identifies the tenant.
    let asked = openai.asked(2);
    assert!(
        asked.contains(&"/sdcpp/v1/capabilities".to_string()),
        "{ITEM} s7: the row tried sd-server's own API before calling it out: {asked:?}"
    );
    assert!(
        asked.contains(&"/v1/models".to_string()),
        "{ITEM} s7: and read /v1/models to tell an OpenAI-shaped server from a dead one: {asked:?}"
    );

    // Nothing listening at all: a closed port, read off a listener that is then dropped.
    let dark_port = {
        let probe_socket = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        probe_socket.local_addr().expect("its address").port()
    };
    let (mark, verdict) = press_sd(window, &format!("http://127.0.0.1:{dark_port}"));
    assert_eq!(mark, "\u{2717}", "{ITEM} s7: a dead sd port is a failure");
    assert!(
        verdict.contains("/sdcpp/v1/capabilities"),
        "{ITEM} s7: the reason says what did not answer: {verdict}"
    );
}

#[test]
fn sec_03_shell_5_settings_dialog_s7_an_openai_shaped_port_is_called_out_as_not_sd_server() {
    common::pin_private_cwd();
    live_round();
    assert!(
        RAN_SD_BAD.load(Ordering::SeqCst),
        "{ITEM} s7: the sd.cpp refusal cases never ran"
    );
}

/// §5's column order on the Drawing row too: box, ✓/✗ badge, Test — the Test button sits in the Test column
/// after the badge, not beside the box.
fn check_the_drawing_test_button_follows_its_badge(_window: &adw::ApplicationWindow) {
    // Read off the open dialog itself (`need` walks it), which is the one this round just opened live.
    let entry = bounds(need("entry-sd").upcast_ref(), "entry-sd");
    let badge = bounds(need("badge-sd").upcast_ref(), "badge-sd");
    let test = bounds(need("test-sd-button").upcast_ref(), "test-sd-button");
    assert!(
        entry.x() + entry.width() <= badge.x() + 1.0 && badge.x() + badge.width() <= test.x() + 1.0,
        "{ITEM} s7: the Drawing row is not box \u{2192} badge \u{2192} Test ({entry:?} {badge:?} {test:?})"
    );
    assert!(
        (centre(&test) - centre(&entry)).abs() <= 6.0,
        "{ITEM} s7: the Drawing row's Test is not on the same line as its box"
    );
}

#[test]
fn sec_03_shell_5_settings_dialog_s7_the_drawing_test_button_sits_after_its_badge() {
    common::pin_private_cwd();
    live_round();
    assert!(
        RAN_SD_ORDER.load(Ordering::SeqCst),
        "{ITEM} s7: the Drawing row's column-order check never ran"
    );
}
