//! §03-shell.md F0.13 / §5's Slots spin — through the widgets. The dialog's Test rows are checked in
//! `tests/suite/settings_widgets.rs`; what these five assert is that §5's NEW control beside every model box
//! exists, holds 1 to 16 with a default of 1, lands in `llm.conf` through the same no-Save beat as a
//! text box, and arrives at the door that actually holds a request (`services::slots_for`).
//!
//! The conf lives in a throwaway `$XDG_CONFIG_HOME` named for this binary and test, so no check reads
//! another run's leftovers and none writes the real `$HOME`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use common::{fixture_dir, settle};
use gtk4 as gtk;
use naivepost::checks;
use naivepost::ui;
use naivepost::ui::settings::Provider;
use naivepost::{machine_store, services, settings, slot_gate};

/// The item's id, interpolated into every assertion message so a failure names the item.
const ITEM: &str = "f0_13";

/// One running GTK application for this test binary — the same single-main-loop arrangement
/// `tests/suite/settings_widgets.rs` needs for the same reason: all five checks run in one
/// `connect_activate`, each records that it got that far, and GTK's main loop may only be started by
/// the thread that will run it.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Pinned BEFORE anything opens: the dialog reads the conf out of the environment when it is
        // built, so the pointer has to be set once for the whole round rather than per check.
        let root = pin_config_home();
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_every_model_box_has_a_spin(app);
            RAN_SPINS.store(true, Ordering::SeqCst);
            check_default_and_ceiling(app);
            RAN_RANGE.store(true, Ordering::SeqCst);
            check_a_written_count_lands_in_the_conf(app);
            RAN_SAVE.store(true, Ordering::SeqCst);
            check_the_gate_holds_that_number();
            RAN_GATE.store(true, Ordering::SeqCst);
            check_the_health_line_carries_a_time();
            RAN_HEALTH.store(true, Ordering::SeqCst);
            check_the_id_box_keeps_its_id(app);
            RAN_ID.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
        let _ = std::fs::remove_dir_all(&root);
    });
}

static RAN_SPINS: AtomicBool = AtomicBool::new(false);
static RAN_RANGE: AtomicBool = AtomicBool::new(false);
static RAN_SAVE: AtomicBool = AtomicBool::new(false);
static RAN_GATE: AtomicBool = AtomicBool::new(false);
static RAN_HEALTH: AtomicBool = AtomicBool::new(false);
static RAN_ID: AtomicBool = AtomicBool::new(false);

/// A config/data pair under the temp dir named for THIS binary and pid — a pid alone collides across
/// test binaries, which all share one process name length and often one pid namespace.
fn pin_config_home() -> PathBuf {
    let root = std::env::temp_dir().join(format!("np-f113-{}-slots", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("cfg")).expect("config root");
    std::fs::create_dir_all(root.join("data")).expect("data root");
    std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
    std::env::set_var("XDG_DATA_HOME", root.join("data"));
    root
}

/// A provider that answers nothing: these checks press spins, not Test buttons, and a spin must not
/// need a server to hold its number.
fn silent() -> Provider {
    std::rc::Rc::new(|key, _| Err(format!("{ITEM}: nothing to probe ({key})")))
}

/// Build a window and open a dialog answering from `silent()`. Every accessor resolves against the
/// LAST dialog opened, so this is how a check gets the one it wants.
fn open_dialog(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::open_settings(&window, silent());
    assert!(
        ui::dialog_open(&window),
        "{ITEM}: the settings dialog is open"
    );
    window
}

/// A named widget inside the open dialog (its own top level, so the main window's tree cannot reach it
/// — the `window` argument only keeps the call sites reading like every other accessor here).
fn in_dialog(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    let _ = window;
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

/// The spin beside a model box, by key.
fn spin(window: &adw::ApplicationWindow, key: &str) -> gtk::SpinButton {
    in_dialog(window, &format!("slots-{key}"))
        .and_then(|w| w.downcast::<gtk::SpinButton>().ok())
        .unwrap_or_else(|| panic!("{ITEM}: no slots-{key} spin on the dialog"))
}

/// S14: §5 gives a Slots spin to the LLM model, each audio.cpp model and the image model — and to
/// nothing else. Bounds are part of the row, not an accident of the widget.
fn check_every_model_box_has_a_spin(app: &adw::Application) {
    let window = open_dialog(app);
    for key in machine_store::SLOT_KEYS {
        let spin = spin(&window, key);
        let bounds = (
            spin.adjustment().lower(),
            spin.adjustment().upper(),
            spin.adjustment().step_increment(),
        );
        assert_eq!(
            bounds,
            (1.0, 16.0, 1.0),
            "{ITEM} S14: slots-{key} spans §5's 1..=16 in steps of one"
        );
        let tip = spin
            .tooltip_text()
            .map(|text| text.to_string())
            .unwrap_or_default();
        assert!(
            tip.contains("--parallel"),
            "{ITEM} S14: slots-{key} says in words what the number means: {tip:?}"
        );
    }
    // A count belongs to a MODEL. The servers, the key, the two binaries and Vision (which tests the
    // model in the box above it) have none, and a stray spin would write a slot nobody asked for.
    for key in ["server", "llm-key", "ffmpeg", "firefox", "llm-vision"] {
        assert!(
            in_dialog(&window, &format!("slots-{key}")).is_none(),
            "{ITEM} S14: {key} is not a model box and carries no spin"
        );
    }
    settle();
}

/// S15: absent from the file means 1 (§6), and the span stops at 16 however hard the arrow is pushed.
/// What is asserted is the number that reaches the file, not how the clamp was achieved.
fn check_default_and_ceiling(app: &adw::Application) {
    let window = open_dialog(app);
    assert_eq!(
        spin(&window, "llm").value(),
        1.0,
        "{ITEM} S15: a conf that never wrote a slot shows 1"
    );
    spin(&window, "llm").set_value(99.0);
    settle();
    close_dialog(&window);

    let paths = settings::from_environment().expect("{ITEM}: the pinned config home resolves");
    let conf = settings::read(&paths).expect("{ITEM}: the conf is readable after close");
    assert_eq!(
        conf.slots.llm, 16,
        "{ITEM} S15: 99 typed into the spin lands on disk as §5's ceiling, not as a queue"
    );
}

/// S16 + S17: two counts changed in one dialog session, then the SAME numbers the gate asks for.
fn check_a_written_count_lands_in_the_conf(app: &adw::Application) {
    // A fresh dialog seeded from the conf S15 wrote (llm 16), so this proves a count is OVERWRITTEN
    // rather than left at whatever an earlier check happened to leave behind.
    let window = open_dialog(app);
    assert_eq!(
        spin(&window, "llm").value(),
        16.0,
        "{ITEM} S16: the spin shows what the file says, not what was last typed"
    );
    spin(&window, "llm").set_value(3.0);
    spin(&window, "asr").set_value(2.0);
    settle();
    close_dialog(&window);

    let paths = settings::from_environment().expect("{ITEM}: the pinned config home resolves");
    let conf = settings::read(&paths).expect("{ITEM}: the conf is readable after close");
    assert_eq!(
        conf.slots.llm, 3,
        "{ITEM} S16: the LLM box's count wrote through"
    );
    assert_eq!(
        conf.slots.asr, 2,
        "{ITEM} S16: the ASR box's count wrote through"
    );
    assert_eq!(
        conf.slots.diar, 1,
        "{ITEM} S16: a count nobody touched stays at 1"
    );

    // And the main log carries §5's no-Save line naming the file, which is how a user knows the
    // spin's click was written without pressing anything.
    let saved = ui::window_logs()
        .into_iter()
        .find(|line| line.contains("settings saved to"))
        .unwrap_or_default();
    assert!(
        saved.contains("settings saved to"),
        "{ITEM} S16: closing said where the counts went, logs: {:?}",
        ui::window_logs()
    );

    let rendered = settings::render(&conf);
    assert!(
        rendered.contains("LLM_SLOTS=\"3\"") || rendered.contains("LLM_SLOTS=3"),
        "{ITEM} S16: the file carries the pair §6 names, got {rendered:?}"
    );
    assert!(
        rendered.contains("AUDIOCPP_ASR_SLOTS=\"2\"") || rendered.contains("AUDIOCPP_ASR_SLOTS=2"),
        "{ITEM} S16: and this one: {rendered:?}"
    );
    // §6: a slot at its default is not written at all, so a machine left alone looks untouched.
    assert!(
        !rendered.contains("AUDIOCPP_DIAR_SLOTS"),
        "{ITEM} S16: an untouched slot stays out of the file: {rendered:?}"
    );
}

/// S17: the number on disk is the number the request side holds — one table, not two copies of it.
fn check_the_gate_holds_that_number() {
    let paths = settings::from_environment().expect("{ITEM}: the pinned config home resolves");
    let conf = settings::read(&paths).expect("{ITEM}: the conf is readable after close");
    assert_eq!(
        services::llm_slots(&conf),
        3,
        "{ITEM} S17: the writing leg holds what the LLM spin wrote"
    );
    assert_eq!(
        services::slots_for(&conf, services::AudioTask::Asr),
        2,
        "{ITEM} S17: the audio leg holds what the ASR spin wrote"
    );
    // The aligner box's count is `Slots::align`; the task word the pipeline passes is "align".
    assert_eq!(
        slot_gate::slots_for_task(&conf, "align"),
        conf.slots.align,
        "{ITEM} S17: the aligner's task word answers its own field"
    );
    assert_eq!(
        slot_gate::slots_for_task(&conf, "asr"),
        services::slots_for(&conf, services::AudioTask::Asr),
        "{ITEM} S17: the words and the enum agree, so one spin drives both doors"
    );
}

/// S18: the audio row's sentence carries a MEASURED time. `settings_probe`'s audio arm now times its
/// own `/health` leg and hands the number here, so the wording is pinned with a non-zero one.
fn check_the_health_line_carries_a_time() {
    let models = vec![services::AudioModel {
        id: "indextts2".to_string(),
        family: "index_tts2".to_string(),
        task: "clon".to_string(),
    }];
    let line =
        checks::audio_health_verdict(412, &models).expect("{ITEM} S18: a healthy server passes");
    assert!(
        line.contains("healthy in 412 ms"),
        "{ITEM} S18: the verdict spells the measured milliseconds: {line:?}"
    );
    assert!(
        line.contains("indextts2"),
        "{ITEM} S18: and names the voice it will narrate with: {line:?}"
    );
    // Zero is what the stand-in used to answer with; a timed leg can still be that fast, so the rule
    // this pins is the WORDING, which only a real measurement can fill in.
    assert!(
        checks::audio_health_verdict(0, &[]).is_err(),
        "{ITEM} S18: health without a catalogue is still a refusal"
    );
}

/// S19: a model row carries TWO controls under one key — the id box and its Slots spin — so seeding
/// must feed each from its own field. This is the regression the rendered dialog showed: with the store
/// answering a plain `tts` by its slot count, every model box read `1` instead of its id.
fn check_the_id_box_keeps_its_id(app: &adw::Application) {
    let paths = settings::from_environment().expect("{ITEM}: the pinned config home resolves");
    let mut conf = settings::Conf::default();
    conf.model = "qwen3.8".to_string();
    conf.tts_model = "index-tts2".to_string();
    conf.asr_model = "qwen3-asr".to_string();
    conf.slots = settings::Slots {
        tts: 6,
        asr: 4,
        ..Default::default()
    };
    settings::save(&paths, &conf).expect("{ITEM} S19: the conf is written before the dialog opens");

    let window = open_dialog(app);
    for (key, id) in [
        ("llm", "qwen3.8"),
        ("tts", "index-tts2"),
        ("asr", "qwen3-asr"),
    ] {
        let shown = ui::entry(&window, key)
            .unwrap_or_else(|| panic!("{ITEM} S19: no entry-{key} on the dialog"))
            .text()
            .to_string();
        assert_eq!(
            shown, id,
            "{ITEM} S19: the {key} id box shows its id, not its slot count"
        );
    }
    // And beside each, its own count — the two controls seeded from two fields of one row.
    assert_eq!(
        spin(&window, "tts").value(),
        6.0,
        "{ITEM} S19: the TTS spin holds 6"
    );
    assert_eq!(
        spin(&window, "asr").value(),
        4.0,
        "{ITEM} S19: the ASR spin holds 4"
    );
    settle();
}

/// Close the dialog the way the user does, so `close_store` lands whatever the last change owes.
fn close_dialog(_window: &adw::ApplicationWindow) {
    let Some(dialog) = ui::settings::open_settings_window() else {
        return;
    };
    dialog.close();
    settle();
}

#[test]
fn f0_13_s14_a_slots_spin_sits_beside_every_model_box_and_only_there() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_SPINS.load(Ordering::SeqCst),
        "{ITEM} S14: the spin check never ran"
    );
}

#[test]
fn f0_13_s15_a_spin_defaults_to_one_and_stops_at_sixteen_on_disk() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_RANGE.load(Ordering::SeqCst),
        "{ITEM} S15: the range check never ran"
    );
}

#[test]
fn f0_13_s16_a_spin_written_by_the_dialog_lands_in_llm_conf() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_SAVE.load(Ordering::SeqCst),
        "{ITEM} S16: the save check never ran"
    );
}

#[test]
fn f0_13_s17_the_written_slot_is_the_number_the_request_doors_hold() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_GATE.load(Ordering::SeqCst),
        "{ITEM} S17: the gate check never ran"
    );
}

#[test]
fn f0_13_s18_the_audio_row_says_how_long_health_took() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_HEALTH.load(Ordering::SeqCst),
        "{ITEM} S18: the health-line check never ran"
    );
}

#[test]
fn f0_13_s19_the_id_box_keeps_its_id_while_its_spin_holds_the_count() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_ID.load(Ordering::SeqCst),
        "{ITEM} S19: the id-box check never ran"
    );
}
