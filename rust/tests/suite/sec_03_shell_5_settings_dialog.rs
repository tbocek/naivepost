//! §03-shell.md §5 — the Settings dialog's own statements, as this item's tests.
//!
//! F0.13's rounds already pin the Test buttons (`settings_widgets.rs`, `settings_tests.rs`) and the
//! Slots spin's existence and range (`f0_13_slots_widgets.rs`). What is pinned here is §5 as a section:
//! the no-Save write beat with its 600 ms constant and its close line, the ⓘ that explains each section's
//! server (the sentence §5 adds and nothing else in the program had), the slot counts the request doors
//! actually hold, and a request parking behind a busy model at the number the dialog wrote.
//!
//! The conf lives in a throwaway `$XDG_CONFIG_HOME` named for this binary and pid, so no check reads
//! another run's leftovers and none writes the real `$HOME`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::checks;
use naivepost::ui::settings::Provider;
use naivepost::{machine_store, services, settings, slot_gate};
use naivepost::ui;
use crate::common;
use common::{fixture_dir, settle};

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
    let paths = settings::Paths { config_dir: root.join("cfg").join("naivepost"), data_dir: root.join("data") };
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    let mut store = machine_store::Store::open(&paths).expect("a store over the throwaway conf");
    store.touched(Duration::from_millis(0));
    assert_eq!(store.due_at(), Some(Duration::from_millis(600)), "one keystroke arms the same beat");
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
    assert_eq!(conf.audio_server, "http://127.0.0.1:9001", "and the typed value is in it");
    // Nothing owed now: closing twice does not say it saved twice.
    assert_eq!(
        store.on_close(&[]).expect("the second close is fine").map(|_| ()),
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
            assert_ne!(a, b, "{ITEM} s2: two sections share an explanation, so one of them lies");
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
        cutting.contains("ffprobe") && cutting.contains("rubberband") && cutting.contains("libx264"),
        "{ITEM} s2: the cutting ⓘ must name ffprobe and what the build has to carry: {cutting}"
    );
    assert!(cutting.contains("off"), "{ITEM} s2: firefox \"off\" is a valid answer and must be said: {cutting}");
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
// s3 — "a Slots spin button, 1 to 16, default 1" / absent = 1.
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_5_settings_dialog_s3_seven_model_counts_default_to_one_within_1_to_16() {
    // The seven boxes §5 lists: the LLM model, each audio.cpp model and the image model.
    assert_eq!(
        machine_store::SLOT_KEYS,
        ["llm", "tts", "asr", "diar", "sep", "aligner", "sd"],
        "{ITEM} s3: one count per model §5 names, and one per model it does not"
    );
    assert_eq!((machine_store::SLOT_MIN, machine_store::SLOT_MAX), (1, 16), "§5: 1 to 16");

    let defaults = settings::Slots::default();
    assert_eq!(
        (defaults.llm, defaults.asr, defaults.diar, defaults.align, defaults.tts, defaults.sep, defaults.sd),
        (1, 1, 1, 1, 1, 1, 1),
        "{ITEM} s3: every count defaults to 1"
    );
    // A conf that never wrote one answers 1 through the read path the dialog seeds itself from (§6).
    let bare = settings::Conf::default();
    for key in machine_store::SLOT_KEYS {
        assert_eq!(
            machine_store::slot_of_key(&bare, key),
            1,
            "{ITEM} s3: absent = 1 for {key}, read the way the spin is seeded"
        );
    }
    // And a number beyond the span is brought inside it on the way IN, which is what makes the spin's
    // ceiling a rule rather than a widget property: 99 typed becomes 16 stored, 0 becomes 1.
    let mut conf = settings::Conf::default();
    machine_store::set_field(&mut conf, &machine_store::slot_key("tts"), "99");
    assert_eq!(conf.slots.tts, 16, "{ITEM} s3: above the ceiling lands on it");
    machine_store::set_field(&mut conf, &machine_store::slot_key("tts"), "0");
    assert_eq!(conf.slots.tts, 1, "{ITEM} s3: below the floor lands on it");
}

// ---------------------------------------------------------------------------
// s4 — "Respected by every request the app makes to that model."
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_5_settings_dialog_s4_the_number_written_is_the_number_the_doors_hold() {
    let root = pin_config_home("s4");
    let paths = settings::Paths { config_dir: root.join("cfg").join("naivepost"), data_dir: root.join("data") };
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    // Written the way the dialog writes it: through the store, under the keys §6 names, with the beat
    // armed because that is what a keystroke does. A slot at its default of 1 is deliberately left out --
    // §6 keeps defaults out of the file, and s3 already shows an absent count reads back as 1 -- so every
    // number below had to have been set.
    let mut store = machine_store::Store::open(&paths).expect("store");
    store.touched(Duration::from_millis(0));
    store
        .on_close(&[
            (machine_store::slot_key("llm"), "3".to_string()),
            (machine_store::slot_key("asr"), "2".to_string()),
            (machine_store::slot_key("diar"), "4".to_string()),
            (machine_store::slot_key("tts"), "5".to_string()),
            (machine_store::slot_key("sep"), "6".to_string()),
            (machine_store::slot_key("aligner"), "7".to_string()),
            (machine_store::slot_key("sd"), "8".to_string()),
        ])
        .expect("the write lands");
    let conf = settings::read(&paths).expect("the conf reads back");
    assert_eq!(
        conf.slots.llm, 3,
        "{ITEM} s4: the file itself carries what was written -- the store's keys must be the ones §6 names"
    );

    // The accessors the dial paths call — not a copy of their arithmetic.
    assert_eq!(services::llm_slots(&conf), 3, "{ITEM} s4: the chat leg holds LLM_SLOTS");
    assert_eq!(services::slots_for(&conf, services::AudioTask::Asr), 2, "asr");
    assert_eq!(services::slots_for(&conf, services::AudioTask::Diar), 4, "diar");
    assert_eq!(services::slots_for(&conf, services::AudioTask::Tts), 5, "tts");
    assert_eq!(services::slots_for(&conf, services::AudioTask::Sep), 6, "sep");
    assert_eq!(services::slots_for(&conf, services::AudioTask::Align), 7, "align");
    assert_eq!(services::image_slots(&conf), 8, "{ITEM} s4: the drawing leg holds SD_SLOTS");
    // The task WORDS the pipeline passes agree with the enum, so one spin drives both spellings.
    for (word, task) in [
        ("asr", services::AudioTask::Asr),
        ("diarize", services::AudioTask::Diar),
        ("tts", services::AudioTask::Tts),
        ("separate", services::AudioTask::Sep),
        ("align", services::AudioTask::Align),
    ] {
        assert_eq!(
            slot_gate::slots_for_task(&conf, word),
            services::slots_for(&conf, task),
            "{ITEM} s4: the word {word:?} answers the same field as its task"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// s5 — "a request that finds every slot taken waits its turn."
// ---------------------------------------------------------------------------

/// Where the second ask got to: still queued (it never went out), or through with how long it waited.
#[derive(Debug)]
enum Ask {
    Through(f64),
}

/// How long a queued ask gets before the test calls it parked. A queue is a normal state, so this is
/// generous; it is a deadline for a failure message, not a pause the test sits through on purpose.
const WAIVE: Duration = Duration::from_secs(20);

#[test]
fn sec_03_shell_5_settings_dialog_s5_a_request_waits_its_turn_when_every_slot_is_taken() {
    // A model whose count came from the dialog's spin (see s4 for the round trip): ONE slot.
    let url = format!("http://127.0.0.1:{}", free_port());
    let model = "wait-its-turn-model";
    let slots = 1;
    let never = || false;

    // A free gate hands the slot straight over and reports no wait. Measured, so "no wait" is a tiny
    // number rather than an exact zero a scheduler could miss.
    let first = slot_gate::hold(&url, model, slots, "describe", &never).expect("a free slot is taken");
    assert!(
        first.1 < 0.05,
        "{ITEM} s5: a slot free on arrival means effectively no wait: {}",
        first.1
    );
    assert_eq!(slot_gate::holders(&url, model).len(), 1, "and the holder is listed while held");

    let (tx, rx) = std::sync::mpsc::channel::<Ask>();
    std::thread::spawn({
        let url = url.clone();
        let model = model.to_string();
        move || match slot_gate::hold(&url, &model, slots, "publish", &never) {
            Ok((held, waited)) => {
                // The guard must outlive the answer, or the release lands before the assert reads it.
                std::mem::forget(held);
                let _ = tx.send(Ask::Through(waited));
            }
            // ⏹ ended the wait rather than the queue handing over: that is not "went out", and the
            // assertion after the release would rather see it than a phantom wait time.
            Err(slot_gate::Wait::Cancelled) => {}
        }
    });
    assert!(
        rx.recv_timeout(WAIVE).is_err(),
        "{ITEM} s5: with every slot busy the ask has not gone out"
    );

    // Give the slot back: the waiter takes it, first come first served, and carries the time it spent.
    drop(first.0);
    let waited = wait_recv(&rx);
    assert!(waited > 0.0, "{ITEM} s5: the wait is measured, not swallowed: {waited}");
    assert!(
        slot_gate::holders(&url, model).iter().any(|h| h.starts_with("publish")),
        "{ITEM} s5: the waiter is now the holder"
    );

    // ⏹ ends a queued wait rather than making it finish first. The cancellation is the ask's own local stop
    // flag -- the same shape every real caller passes: `llm_retry_leg`, `audio_task`, `speak_leg` and
    // `produce_upload_draw` each hand `hold` a closure over their run's cancel state, and `hold` consults
    // it on every pass of its wait loop. Local here on purpose: the process-wide switch belongs to
    // `runqueue`, and pressing it from one test would stop the legs of every test running alongside.
    //
    // It needs a slot of its OWN. This gate is process-global and keyed by (server, model), so an ask here
    // for a model another test is using queues behind THAT test's holder -- one nobody here ever drops --
    // and no ⏹ could end that wait within any budget. A dedicated model makes "every slot busy" mean
    // "busy with this test's own holder", which is exactly what §4 says ⏹ may interrupt.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let held_flag = Arc::clone(&stop);
    let stop_flag = move || held_flag.load(Ordering::SeqCst);
    let stopper = format!("{model}-stop");
    let held_alone = slot_gate::hold(&url, &stopper, 1, "produce", &never)
        .expect("a fresh model's only slot is free");
    let (ctx, crx) = std::sync::mpsc::channel::<bool>();
    std::thread::spawn({
        let url = url.clone();
        let stopper = stopper.clone();
        move || match slot_gate::hold(&url, &stopper, 1, "narrate", &stop_flag) {
            Ok((held, _)) => {
                std::mem::forget(held);
                let _ = ctx.send(false);
            }
            Err(slot_gate::Wait::Cancelled) => {
                let _ = ctx.send(true);
            }
        }
    });
    // Nothing can hand this ask its slot -- the guard above is alive and it is the model's only slot -- so
    // the ask sits in the queue; the answer below is awaited on its channel, bounded, so a wait that could
    // not be cancelled fails here instead of hanging.
    stop.store(true, Ordering::SeqCst);
    let cancelled = crx
        .recv_timeout(WAIVE)
        .unwrap_or_else(|_| panic!("{ITEM} s5: ⏹ left the queued wait sitting there"));
    drop(held_alone.0);
    assert!(
        cancelled,
        "{ITEM} s5: ⏹ ends a queued wait; the request never went out"
    );
}

/// Read the asker's answer, bounded: a queued request is a normal state, so this waits on the condition
/// (an answer arriving) but only as far as `WAIVE` -- a thread that died without answering fails here
/// instead of hanging the suite.
fn wait_recv(rx: &std::sync::mpsc::Receiver<Ask>) -> f64 {
    match rx.recv_timeout(WAIVE) {
        Ok(Ask::Through(waited)) => waited,
        Err(_) => panic!("{ITEM} s5: the queued ask never took the slot it was handed"),
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a loopback port")
        .local_addr()
        .expect("an address")
        .port()
}

// ---------------------------------------------------------------------------
// The widget half: the header's Settings button, the ⓘ controls, a spin's number.
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
            check_a_spin_written_here_is_the_number_the_door_holds(app);
            RAN_SPIN.store(true, Ordering::SeqCst);
            // §5's layout statements, all measured off the one open dialog.
            let window = open_window(app);
            ui::open_settings(&window, silent());
            lay_out();
            check_the_sections_are_divided_by_lines(&window);
            RAN_DIVIDER.store(true, Ordering::SeqCst);
            check_the_field_labels_are_right_aligned(&window);
            RAN_LABELS.store(true, Ordering::SeqCst);
            check_the_spin_sits_between_box_and_badge(&window);
            RAN_BETWEEN.store(true, Ordering::SeqCst);
            check_the_badges_and_tests_share_one_column(&window);
            RAN_COLUMN.store(true, Ordering::SeqCst);
            check_the_writing_last_line_uses_its_columns(&window);
            RAN_FETCH_ROW.store(true, Ordering::SeqCst);
            check_test_all_is_bottom_right_and_the_log_is_last(&window);
            RAN_TEST_ALL.store(true, Ordering::SeqCst);
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
static RAN_SPIN: AtomicBool = AtomicBool::new(false);
static RAN_DIVIDER: AtomicBool = AtomicBool::new(false);
static RAN_LABELS: AtomicBool = AtomicBool::new(false);
static RAN_BETWEEN: AtomicBool = AtomicBool::new(false);
static RAN_COLUMN: AtomicBool = AtomicBool::new(false);
static RAN_FETCH_ROW: AtomicBool = AtomicBool::new(false);
static RAN_TEST_ALL: AtomicBool = AtomicBool::new(false);

/// A conf with a model id, written into this round's throwaway config home before the live dialog opens.
/// §5 refuses a completion with no model id, and the dialog seeds its boxes from the file, so it has to
/// be down BEFORE `open_settings` — same order `four_servers_probe_widgets.rs` uses.
fn install_live_conf() {
    let paths = naivepost::settings::from_environment().expect("{ITEM} s7: a pinned XDG_CONFIG_HOME");
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
    println!("SETTINGS GRID {what} x={:.1} y={:.1} w={:.1} h={:.1}", b.x(), b.y(), b.width(), b.height());
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
            let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
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

/// A provider that answers nothing: these checks press the gear and the spins, and neither needs a server.
fn silent() -> Provider {
    std::rc::Rc::new(|key, _| Err(format!("{ITEM}: nothing to probe ({key})")))
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
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
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
    ui::settings::settings_button(window).unwrap_or_else(|| panic!("{ITEM}: the header has no settings-button"))
}

/// §5's ⓘ: one per section, in the heading's own row, carrying the paragraph as its tooltip.
fn check_every_section_has_its_info_button(app: &adw::Application) {
    let window = open_window(app);
    ui::open_settings(&window, silent());
    assert!(ui::dialog_open(&window), "{ITEM} s2: the dialog did not open");
    for (key, title) in naivepost::ui::settings_info::SECTIONS {
        let info = in_dialog(&format!("settings-info-{key}"))
            .unwrap_or_else(|| panic!("{ITEM} s2: no settings-info-{key} on the dialog"))
            .downcast::<gtk::Button>()
            .unwrap_or_else(|_| panic!("{ITEM} s2: settings-info-{key} is not a button"));
        let want = naivepost::ui::settings_info::explanation(key).expect("the section's text");
        let got = info.tooltip_text().map(|text| text.to_string()).unwrap_or_default();
        assert_eq!(got, want, "{ITEM} s2: the {key} ⓘ carries its own explanation as its tooltip");
        assert!(got.chars().count() > 120, "{ITEM} s2: {key}'s ⓘ is at length");

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
    root.observe_children().iter::<glib::Object>().filter_map(Result::ok).any(|child| {
        let Ok(widget) = child.downcast::<gtk::Widget>() else { return false };
        holds_text(&widget, text)
    })
}

/// The spin's number, set through the widget, is the number the request side asks the gate for.
fn check_a_spin_written_here_is_the_number_the_door_holds(app: &adw::Application) {
    let window = open_window(app);
    ui::open_settings(&window, silent());
    let spin = in_dialog("slots-tts")
        .unwrap_or_else(|| panic!("{ITEM} s4: no slots-tts spin"))
        .downcast::<gtk::SpinButton>()
        .expect("slots-tts is a spin button");
    spin.set_value(3.0);
    settle();
    close_dialog();

    let paths = settings::from_environment().expect("{ITEM}: the pinned config home resolves");
    let conf = settings::read(&paths).expect("the conf reads back after close");
    assert_eq!(
        services::slots_for(&conf, services::AudioTask::Tts),
        3,
        "{ITEM} s4: the TTS spin's 3 is what the speech leg will hold"
    );
    window.close();
}

#[test]
fn sec_03_shell_5_settings_dialog_s1_the_settings_button_opens_the_dialog_that_saves_on_close() {
    common::pin_private_cwd();
    widgets_round();
    assert!(RAN_OPEN.load(Ordering::SeqCst), "{ITEM} s1: the gear check never ran");
}

#[test]
fn sec_03_shell_5_settings_dialog_s2_every_section_carries_an_info_button_that_explains_its_server() {
    common::pin_private_cwd();
    widgets_round();
    assert!(RAN_INFO.load(Ordering::SeqCst), "{ITEM} s2: the ⓘ check never ran");
}

#[test]
fn sec_03_shell_5_settings_dialog_s4_a_spin_written_in_the_dialog_is_the_number_the_door_holds() {
    common::pin_private_cwd();
    widgets_round();
    assert!(RAN_SPIN.load(Ordering::SeqCst), "{ITEM} s4: the spin check never ran");
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
        assert!(b.width() > 400.0, "{ITEM} s6: the {here} divider spans {:.1} px, not the dialog", b.width());
        let previous = bounds(&need(&format!("settings-section-{before}")), &format!("section-{before}"));
        let own = bounds(&need(&format!("settings-section-{here}")), &format!("section-{here}"));
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
    assert!(RAN_DIVIDER.load(Ordering::SeqCst), "{ITEM} s6: the divider check never ran");
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
            "{ITEM} s6: captions end at {:?} — they do not share one right edge", edges
        );
    }
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_the_field_labels_are_right_aligned() {
    common::pin_private_cwd();
    widgets_round();
    assert!(RAN_LABELS.load(Ordering::SeqCst), "{ITEM} s6: the label check never ran");
}

/// §5's Slots spin goes "right after every model box, between the box and its ✓/✗ badge", and the
/// Test button after the badge — all four on one line.
fn check_the_spin_sits_between_box_and_badge(window: &adw::ApplicationWindow) {
    let _ = window;
    for key in naivepost::machine_store::SLOT_KEYS {
        let entry = bounds(need(&format!("entry-{key}")).upcast_ref(), &format!("entry-{key}"));
        let spin = bounds(need(&format!("slots-{key}")).upcast_ref(), &format!("slots-{key}"));
        let badge = bounds(need(&format!("badge-{key}")).upcast_ref(), &format!("badge-{key}"));
        let test = bounds(
            need(&format!("test-{key}-button")).upcast_ref(),
            &format!("test-{key}-button"),
        );
        assert!(
            entry.x() + entry.width() <= spin.x() + 1.0
                && spin.x() + spin.width() <= badge.x() + 1.0
                && badge.x() + badge.width() <= test.x() + 1.0,
            "{ITEM} s6: {key}'s order is not box → spin → badge → Test ({entry:?} {spin:?} {badge:?} {test:?})"
        );
        for (what, other) in [("spin", spin), ("badge", badge), ("test", test)] {
            assert!(
                (centre(&other) - centre(&entry)).abs() <= 6.0,
                "{ITEM} s6: {key}'s {what} is not on the same line as its box"
            );
        }
    }
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_the_slots_spin_sits_between_the_box_and_its_badge() {
    common::pin_private_cwd();
    widgets_round();
    assert!(RAN_BETWEEN.load(Ordering::SeqCst), "{ITEM} s6: the spin-order check never ran");
}

/// One grid, so a row without a spin still lands its badge and its Test button where a row with one
/// does — otherwise the marks would step sideways down the dialog.
fn check_the_badges_and_tests_share_one_column(window: &adw::ApplicationWindow) {
    let _ = window;
    for prefix in ["badge", "test"] {
        // `llm-vision`, `ffmpeg` and `firefox` have no spin; the others do — both kinds must land on
        // the same x, which is what makes §5's five columns one set of columns.
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
        // Rows without a spin (`llm-vision`, `ffmpeg`, `firefox`) must land on the same x as the spin
        // rows, which is what makes the column one column rather than two.
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
    assert!(RAN_COLUMN.load(Ordering::SeqCst), "{ITEM} s6: the shared-column check never ran");
}

/// §5's Writing last line: Fetch models in the label column, the dropdown in the box column, Use in
/// the Test column.
fn check_the_writing_last_line_uses_its_columns(window: &adw::ApplicationWindow) {
    let _ = window;
    let server = bounds(need("entry-server").upcast_ref(), "entry-server");
    // A right-aligned caption starts wherever its text happens to begin; the column's own left edge is
    // the widest thing in it, which here is the Fetch models button (§5 puts that button in this column).
    let label_x = left(caption_showing("Server").upcast_ref(), "caption-Server");
    let fetch = bounds(need("fetch-models-button").upcast_ref(), "fetch-models-button");
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
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_the_writing_last_line_puts_fetch_the_dropdown_and_use_in_their_columns() {
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
    let drawing = bounds(need("settings-section-drawing").upcast_ref(), "section-drawing");
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
    // Last: the root box's final child is the expander.
    let root = expander.parent().expect("{ITEM} s6: the expander sits in the dialog's root box");
    assert_eq!(
        root.last_child().as_ref(),
        Some(expander.upcast_ref()),
        "{ITEM} s6: the log is not the last thing in the dialog"
    );
}

#[test]
fn sec_03_shell_5_settings_dialog_s6_test_all_is_at_the_bottom_right_and_the_log_is_last_and_collapsed() {
    common::pin_private_cwd();
    widgets_round();
    assert!(
        RAN_TEST_ALL.load(Ordering::SeqCst),
        "{ITEM} s6: the Test All / log check never ran"
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
    let badge = ui::badge(window, "sd").expect("{ITEM} s7: the Drawing row has a badge-sd");
    (badge.text().to_string(), badge.tooltip_text().unwrap_or_default().to_string())
}

/// F0.13 through the widget: the Drawing row's Test dials sd-server's OWN capabilities endpoint and
/// says which weights are loaded — nothing asked of the OpenAI-shaped API on the way.
fn check_the_drawing_row_tests_sd_server(window: &adw::ApplicationWindow) {
    let server = Live::start(&[(
        "/sdcpp/v1/capabilities",
        r#"{"weights":"sd-v1.safetensors","modes":["txt2img","img2img"]}"#,
    )]);
    let before = ui::window_logs().len();
    let (mark, verdict) = press_sd(window, &server.url());
    assert_eq!(
        server.asked(1),
        vec!["/sdcpp/v1/capabilities".to_string()],
        "{ITEM} s7: the Drawing row probes sd-server's own API and nothing else"
    );
    assert_eq!(mark, "\u{2713}", "{ITEM} s7: loaded weights mark \u{2713}, got {verdict:?}");
    assert!(
        verdict.contains("sd-v1.safetensors is loaded and can draw"),
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
    assert_eq!(mark, "\u{2717}", "{ITEM} s7: an OpenAI-shaped port is a failure, got {verdict}");
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

/// §5's column order on the Drawing row too: box, Slots spin, ✓/✗ badge, Test — the Test button sits in
/// the Test column after the badge, not beside the box.
fn check_the_drawing_test_button_follows_its_badge(_window: &adw::ApplicationWindow) {
    // Read off the open dialog itself (`need` walks it), which is the one this round just opened live.
    let entry = bounds(need("entry-sd").upcast_ref(), "entry-sd");
    let spin = bounds(need("slots-sd").upcast_ref(), "slots-sd");
    let badge = bounds(need("badge-sd").upcast_ref(), "badge-sd");
    let test = bounds(need("test-sd-button").upcast_ref(), "test-sd-button");
    assert!(
        entry.x() + entry.width() <= spin.x() + 1.0
            && spin.x() + spin.width() <= badge.x() + 1.0
            && badge.x() + badge.width() <= test.x() + 1.0,
        "{ITEM} s7: the Drawing row is not box \u{2192} spin \u{2192} badge \u{2192} Test ({entry:?} {spin:?} {badge:?} {test:?})"
    );
    assert!(
        (centre(&test) - centre(&entry)).abs() <= 6.0,
        "{ITEM} s7: the Drawing row's Test is not on the same line as its box"
    );
}

#[test]
fn sec_03_shell_5_settings_dialog_s7_the_drawing_test_button_sits_after_the_slots_spin() {
    common::pin_private_cwd();
    live_round();
    assert!(
        RAN_SD_ORDER.load(Ordering::SeqCst),
        "{ITEM} s7: the Drawing row's column-order check never ran"
    );
}
