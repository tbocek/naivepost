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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
            app.quit();
        });
        app.run_with_args::<String>(&[]);
        let _ = std::fs::remove_dir_all(&root);
    });
}

static RAN_OPEN: AtomicBool = AtomicBool::new(false);
static RAN_INFO: AtomicBool = AtomicBool::new(false);
static RAN_SPIN: AtomicBool = AtomicBool::new(false);

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
