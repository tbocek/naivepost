//! §03-shell.md §5 and §6: the Settings dialog's named server entries. Writing, Audio and Drawing each carry a
//! dropdown of entries with ＋ New and 🗑 under their heading, kept in `servers.json` beside `llm.conf`.
//!
//! One application round drives the real dialog through every statement in order: the first open migrates
//! `llm.conf`, ＋ New refuses and creates, a typed box writes through, a pick fills the boxes and `llm.conf`,
//! 🗑 asks and deletes, and an unreadable file is put aside. Each `#[test]` asserts its own flag, so a check that
//! never ran fails by name. The config home is this process's own throwaway folder.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::common;
use crate::common::real_input::{drain, real_click_centre};
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::server_entries::{self, Section};
use naivepost::settings::{self, Conf, Paths};
use naivepost::ui;
use naivepost::ui::settings::Provider;
use naivepost::ui::settings_servers;

const ITEM: &str = "sec_03_shell_5_server_entries";

/// The two keys this round types, which no log line may ever show.
const LLM_KEY: &str = "sk-SECRET-writing-4711";
const AUDIO_KEY: &str = "ak-SECRET-audio-0815";
const SPARE_AUDIO_KEY: &str = "ak-SECRET-spare-2222";

static RAN_MIGRATE: AtomicBool = AtomicBool::new(false);
static RAN_NEW: AtomicBool = AtomicBool::new(false);
static RAN_EDIT: AtomicBool = AtomicBool::new(false);
static RAN_PICK: AtomicBool = AtomicBool::new(false);
static RAN_DELETE: AtomicBool = AtomicBool::new(false);
static RAN_PRIVATE: AtomicBool = AtomicBool::new(false);
static RAN_BAD: AtomicBool = AtomicBool::new(false);

fn round() {
    common::one_app_round(|app| {
        let paths = pin_config_home();
        settings::save(&paths, &first_conf()).expect("the starting llm.conf writes");
        let model = naivepost::project::Project::default();
        let window = ui::build_window(app, &model, "Prepare");
        window.present();

        ui::open_settings(&window, silent());
        drain(Duration::from_millis(30), Duration::from_millis(500));
        check_the_first_open_migrates_llm_conf(&paths);
        RAN_MIGRATE.store(true, Ordering::SeqCst);
        check_new_refuses_then_creates(&paths);
        RAN_NEW.store(true, Ordering::SeqCst);
        check_a_typed_box_writes_through(&paths);
        RAN_EDIT.store(true, Ordering::SeqCst);
        close_dialog();

        // A second machine's entry put in the file by hand, its voices folder (no box) its own.
        add_gpu2_entry(&paths);
        ui::open_settings(&window, silent());
        drain(Duration::from_millis(30), Duration::from_millis(500));
        check_a_pick_fills_the_boxes_and_llm_conf(&paths);
        RAN_PICK.store(true, Ordering::SeqCst);
        check_delete_asks_and_keeps_one(&paths);
        RAN_DELETE.store(true, Ordering::SeqCst);
        close_dialog();
        check_the_file_is_private_and_no_key_is_logged(&paths);
        RAN_PRIVATE.store(true, Ordering::SeqCst);

        std::fs::write(server_entries::path(&paths), "{ not json").expect("a broken servers.json");
        ui::open_settings(&window, silent());
        drain(Duration::from_millis(30), Duration::from_millis(500));
        check_an_unreadable_file_is_kept_as_bad(&paths);
        RAN_BAD.store(true, Ordering::SeqCst);
        close_dialog();
        window.close();
    });
}

/// The llm.conf the round starts from: one server per section, keys set.
fn first_conf() -> Conf {
    let mut conf = Conf::default();
    conf.server = "http://alpha.lan:8080/v1".to_string();
    conf.key = LLM_KEY.to_string();
    conf.model = "m-alpha".to_string();
    conf.audio_server = "http://audio1:9100".to_string();
    conf.audio_key = AUDIO_KEY.to_string();
    conf.voices = "/srv/voices-one".to_string();
    conf.tts_model = "tts-one".to_string();
    conf
}

// ---------------------------------------------------------------------------
// §6: with no servers.json, llm.conf becomes one entry per section, named after the server host or "default".
// ---------------------------------------------------------------------------

fn check_the_first_open_migrates_llm_conf(paths: &Paths) {
    let on_disk = read_file(paths);
    let names = |section: Section| -> Vec<String> {
        on_disk.group(section).entries.keys().cloned().collect()
    };
    assert_eq!(
        names(Section::Writing),
        ["alpha.lan"],
        "{ITEM} migrate: the writing entry is named after its host"
    );
    assert_eq!(
        names(Section::Audio),
        ["audio1"],
        "{ITEM} migrate: audio named after its host"
    );
    assert_eq!(
        names(Section::Drawing),
        ["default"],
        "{ITEM} migrate: an empty sd.cpp server gives the entry \"default\""
    );
    let writing = &on_disk.writing.entries["alpha.lan"];
    assert_eq!(
        writing["LLM_SERVER"], "http://alpha.lan:8080/v1",
        "{ITEM} migrate: the server"
    );
    assert_eq!(
        writing["LLM_API_KEY"], LLM_KEY,
        "{ITEM} migrate: the key travels with its entry"
    );
    assert_eq!(writing["LLM_MODEL"], "m-alpha", "{ITEM} migrate: the model");
    assert_eq!(
        on_disk.audio.entries["audio1"]["AUDIOCPP_VOICES"], "/srv/voices-one",
        "{ITEM} migrate: the voices folder, which has no box, is part of the audio entry"
    );
    assert_eq!(
        on_disk.writing.active, "alpha.lan",
        "{ITEM} migrate: the one entry is in use"
    );
    assert_eq!(
        shown(Section::Writing),
        (vec!["alpha.lan".to_string()], Some("alpha.lan".to_string())),
        "{ITEM} migrate: the writing dropdown shows the entry"
    );
    for section in Section::ALL {
        assert!(
            !delete_button(section).is_sensitive(),
            "{ITEM} migrate: 🗑 is greyed while {} has one entry",
            section.key()
        );
    }
}

// ---------------------------------------------------------------------------
// §5: ＋ New asks for a name, refuses an empty one and a duplicate in words, and creates from the boxes.
// ---------------------------------------------------------------------------

fn check_new_refuses_then_creates(paths: &Paths) {
    let new = need::<gtk::MenuButton>("servers-audio-new");
    // The dialog was presented a moment ago: let the X server place it before aiming at it.
    drain(Duration::from_millis(300), Duration::from_secs(2));
    real_click_centre(&new);
    let popover = new.popover().expect("{ITEM} new: ＋ New has a popover");
    let create = need::<gtk::Button>("servers-audio-create");
    // The popover is a surface of its own: it maps a few turns of the loop after the click.
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while !(create.is_mapped() && create.width() > 0) && std::time::Instant::now() < until {
        drain(Duration::from_millis(30), Duration::from_millis(200));
    }
    assert!(
        popover.is_visible() && create.is_mapped(),
        "{ITEM} new: a click on ＋ New opens its popover"
    );
    let name = need::<gtk::Entry>("servers-audio-name");
    let refused = need::<gtk::Label>("servers-audio-refused");

    name.set_text("   ");
    real_click_centre(&create);
    assert_eq!(
        refused.text(),
        "give the entry a name",
        "{ITEM} new: an empty name is refused"
    );
    name.set_text("audio1");
    real_click_centre(&create);
    assert_eq!(
        refused.text(),
        "there is already an entry called audio1",
        "{ITEM} new: a name the section has is refused"
    );
    assert_eq!(
        read_file(paths).audio.entries.len(),
        1,
        "{ITEM} new: a refusal makes nothing"
    );

    name.set_text("spare");
    real_click_centre(&create);
    drain(Duration::from_millis(30), Duration::from_millis(500));
    assert!(
        !popover.is_visible(),
        "{ITEM} new: Create closes the popover"
    );
    assert_eq!(
        refused.text(),
        "",
        "{ITEM} new: no refusal is left standing"
    );
    assert_eq!(
        shown(Section::Audio),
        (
            vec!["audio1".to_string(), "spare".to_string()],
            Some("spare".to_string())
        ),
        "{ITEM} new: the new entry is listed and selected"
    );
    let on_disk = read_file(paths);
    assert_eq!(
        on_disk.audio.active, "spare",
        "{ITEM} new: the file says spare is in use"
    );
    assert_eq!(
        on_disk.audio.entries["spare"], on_disk.audio.entries["audio1"],
        "{ITEM} new: the entry is made from the section's current values"
    );
    assert!(
        delete_button(Section::Audio).is_sensitive(),
        "{ITEM} new: 🗑 wakes at two entries"
    );
}

// ---------------------------------------------------------------------------
// §5: editing a box writes through to the selected entry and to llm.conf, on the 600 ms beat.
// ---------------------------------------------------------------------------

fn check_a_typed_box_writes_through(paths: &Paths) {
    box_of("audio").set_text("http://spare:9200");
    // The second box is typed into inside the first one's beat, which moves the write back: the timer armed
    // by the first keystroke finds nothing due yet and has to wait out the rest.
    turn(Duration::from_millis(300));
    box_of("audio-key").set_text(SPARE_AUDIO_KEY);
    let held =
        settings_servers::servers_for_test().expect("{ITEM} edit: the dialog holds its entries");
    assert_eq!(
        held.audio.entries["spare"]["AUDIOCPP_SERVER"], "http://spare:9200",
        "{ITEM} edit: the typed server is the selected entry's"
    );
    assert_eq!(
        held.audio.entries["audio1"]["AUDIOCPP_SERVER"], "http://audio1:9100",
        "{ITEM} edit: the other entry keeps its own server"
    );
    // The beat lands both files, 600 ms after the last keystroke.
    turn(Duration::from_millis(1000));
    let on_disk = read_file(paths);
    assert_eq!(
        on_disk.audio.entries["spare"]["AUDIOCPP_SERVER"], "http://spare:9200",
        "{ITEM} edit: servers.json has the typed server after the beat"
    );
    assert_eq!(
        on_disk.audio.entries["spare"]["AUDIOCPP_API_KEY"], SPARE_AUDIO_KEY,
        "{ITEM} edit: and the typed key"
    );
    let conf = settings::read(paths).expect("llm.conf reads back");
    assert_eq!(
        conf.audio_server, "http://spare:9200",
        "{ITEM} edit: llm.conf has it too"
    );
}

/// A second audio entry, put in the file while the dialog is closed.
fn add_gpu2_entry(paths: &Paths) {
    let mut servers = read_file(paths);
    let mut gpu2 = servers.audio.entries["audio1"].clone();
    gpu2.insert("AUDIOCPP_SERVER".into(), "http://gpu2:9300".into());
    gpu2.insert("AUDIOCPP_VOICES".into(), "/srv/voices-gpu2".into());
    gpu2.insert("AUDIOCPP_TTS_MODEL".into(), "tts-gpu2".into());
    servers.audio.entries.insert("gpu2".into(), gpu2);
    server_entries::save(paths, &servers).expect("the hand-made entry writes");
}

// ---------------------------------------------------------------------------
// §5: picking an entry fills the section's boxes and makes its values the ones llm.conf holds.
// ---------------------------------------------------------------------------

fn check_a_pick_fills_the_boxes_and_llm_conf(paths: &Paths) {
    let (names, selected) = shown(Section::Audio);
    assert_eq!(
        names,
        ["audio1", "gpu2", "spare"],
        "{ITEM} pick: the three audio entries"
    );
    assert_eq!(
        selected.as_deref(),
        Some("spare"),
        "{ITEM} pick: reopening keeps spare in use"
    );
    let at = names.iter().position(|name| name == "gpu2").unwrap() as u32;
    dropdown(Section::Audio).set_selected(at);
    drain(Duration::from_millis(30), Duration::from_millis(300));
    assert_eq!(
        box_of("audio").text(),
        "http://gpu2:9300",
        "{ITEM} pick: the server box is gpu2's"
    );
    assert_eq!(
        box_of("tts").text(),
        "tts-gpu2",
        "{ITEM} pick: the TTS box is gpu2's"
    );
    assert_eq!(
        box_of("audio-key").text(),
        AUDIO_KEY,
        "{ITEM} pick: the key box is gpu2's (copied from audio1)"
    );
    assert_eq!(
        box_of("server").text(),
        "http://alpha.lan:8080/v1",
        "{ITEM} pick: writing untouched"
    );
    assert_eq!(
        read_file(paths).audio.active,
        "gpu2",
        "{ITEM} pick: the file says gpu2 is in use"
    );
    turn(Duration::from_millis(1000));
    let conf = settings::read(paths).expect("llm.conf reads back");
    assert_eq!(
        conf.audio_server, "http://gpu2:9300",
        "{ITEM} pick: llm.conf has gpu2's server"
    );
    assert_eq!(conf.tts_model, "tts-gpu2", "{ITEM} pick: and its TTS model");
    assert_eq!(
        conf.voices, "/srv/voices-gpu2",
        "{ITEM} pick: and its voices folder, which has no box of its own"
    );
    let held = settings_servers::servers_for_test().expect("the dialog's entries");
    assert_eq!(
        held.audio.entries["gpu2"]["AUDIOCPP_SERVER"], "http://gpu2:9300",
        "{ITEM} pick: filling the boxes is not typing: the entry stays as it was"
    );
    assert_eq!(
        held.audio.entries["spare"]["AUDIOCPP_SERVER"], "http://spare:9200",
        "{ITEM} pick: the entry left keeps what was typed into it"
    );
}

// ---------------------------------------------------------------------------
// §5: 🗑 asks with Cancel as the default; Delete removes and selects the next; greyed at one entry.
// ---------------------------------------------------------------------------

fn check_delete_asks_and_keeps_one(paths: &Paths) {
    let trash = delete_button(Section::Audio);
    real_click_centre(&trash);
    let question = settings_servers::confirm_for_test().expect("{ITEM} delete: 🗑 asks first");
    assert_eq!(
        question.default_response().as_deref(),
        Some("cancel"),
        "{ITEM} delete: Cancel is the default"
    );
    question.response("cancel");
    drain(Duration::from_millis(30), Duration::from_millis(300));
    assert_eq!(
        read_file(paths).audio.entries.len(),
        3,
        "{ITEM} delete: Cancel keeps the entry"
    );
    assert_eq!(
        shown(Section::Audio).1.as_deref(),
        Some("gpu2"),
        "{ITEM} delete: still gpu2"
    );

    real_click_centre(&trash);
    settings_servers::confirm_for_test()
        .unwrap()
        .response("delete");
    drain(Duration::from_millis(30), Duration::from_millis(300));
    let on_disk = read_file(paths);
    assert_eq!(
        on_disk.audio.entries.keys().cloned().collect::<Vec<_>>(),
        ["audio1", "spare"],
        "{ITEM} delete: Delete removes gpu2"
    );
    assert_eq!(
        on_disk.audio.active, "spare",
        "{ITEM} delete: the next entry is selected"
    );
    assert_eq!(
        shown(Section::Audio),
        (
            vec!["audio1".to_string(), "spare".to_string()],
            Some("spare".to_string())
        ),
        "{ITEM} delete: the dropdown follows"
    );
    assert_eq!(
        box_of("audio").text(),
        "http://spare:9200",
        "{ITEM} delete: spare fills the boxes"
    );

    real_click_centre(&trash);
    settings_servers::confirm_for_test()
        .unwrap()
        .response("delete");
    drain(Duration::from_millis(30), Duration::from_millis(300));
    assert_eq!(
        read_file(paths)
            .audio
            .entries
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        ["audio1"],
        "{ITEM} delete: the last but one goes, the one before it is selected"
    );
    assert!(
        !trash.is_sensitive(),
        "{ITEM} delete: 🗑 is greyed at the last entry"
    );
}

// ---------------------------------------------------------------------------
// §6: servers.json is written atomically at 0600, and no key reaches a log line.
// ---------------------------------------------------------------------------

fn check_the_file_is_private_and_no_key_is_logged(paths: &Paths) {
    use std::os::unix::fs::PermissionsExt;
    let file = server_entries::path(paths);
    let mode = std::fs::metadata(&file)
        .expect("servers.json is there")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "{ITEM} private: servers.json is 0600, got {mode:o}"
    );
    assert!(
        !file.with_extension("json.part").exists(),
        "{ITEM} private: no .part is left behind"
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains(LLM_KEY),
        "{ITEM} private: the key is stored, which is why the mode matters"
    );
    let logged = ui::window::window_logs();
    assert!(
        logged.iter().any(|line| line.contains("entry spare")),
        "{ITEM} private: the entry lines did reach the log: {logged:?}"
    );
    for key in [LLM_KEY, AUDIO_KEY, SPARE_AUDIO_KEY] {
        assert!(
            !logged.iter().any(|line| line.contains(key)),
            "{ITEM} private: a key reached the log"
        );
    }
}

// ---------------------------------------------------------------------------
// §6: an unparseable servers.json is said, kept as servers.json.bad, and the entries come from llm.conf.
// ---------------------------------------------------------------------------

fn check_an_unreadable_file_is_kept_as_bad(paths: &Paths) {
    let bad = server_entries::bad_path(paths);
    assert_eq!(
        std::fs::read_to_string(&bad).ok().as_deref(),
        Some("{ not json"),
        "{ITEM} bad: the unreadable file is kept as servers.json.bad"
    );
    let said = ui::settings::dialog_log(&main_window());
    assert!(
        said.contains("could not be read") && said.contains(&bad.display().to_string()),
        "{ITEM} bad: the dialog says what happened and where the file went: {said}"
    );
    assert!(
        ui::window::window_logs()
            .iter()
            .any(|line| line.contains("servers.json.bad")),
        "{ITEM} bad: the main log says it too"
    );
    let conf = settings::read(paths).unwrap();
    let host = server_entries::host_name(&conf.audio_server);
    assert_eq!(
        shown(Section::Audio),
        (vec![host.clone()], Some(host)),
        "{ITEM} bad: the dialog falls back to one entry from llm.conf"
    );
}

#[test]
fn sec_03_shell_6_no_servers_json_migrates_llm_conf_one_entry_per_section() {
    round();
    assert!(
        RAN_MIGRATE.load(Ordering::SeqCst),
        "{ITEM}: the migration check never ran"
    );
}

#[test]
fn sec_03_shell_5_new_refuses_empty_and_duplicate_names_and_creates_from_the_boxes() {
    round();
    assert!(
        RAN_NEW.load(Ordering::SeqCst),
        "{ITEM}: the ＋ New check never ran"
    );
}

#[test]
fn sec_03_shell_5_a_typed_box_writes_through_to_the_selected_entry() {
    round();
    assert!(
        RAN_EDIT.load(Ordering::SeqCst),
        "{ITEM}: the write-through check never ran"
    );
}

#[test]
fn sec_03_shell_5_picking_an_entry_fills_the_boxes_and_llm_conf() {
    round();
    assert!(
        RAN_PICK.load(Ordering::SeqCst),
        "{ITEM}: the pick check never ran"
    );
}

#[test]
fn sec_03_shell_5_delete_asks_with_cancel_default_and_keeps_the_last_entry() {
    round();
    assert!(
        RAN_DELETE.load(Ordering::SeqCst),
        "{ITEM}: the 🗑 check never ran"
    );
}

#[test]
fn sec_03_shell_6_servers_json_is_0600_and_no_key_is_logged() {
    round();
    assert!(
        RAN_PRIVATE.load(Ordering::SeqCst),
        "{ITEM}: the privacy check never ran"
    );
}

#[test]
fn sec_03_shell_6_an_unreadable_servers_json_is_kept_as_bad_and_said() {
    round();
    assert!(
        RAN_BAD.load(Ordering::SeqCst),
        "{ITEM}: the unreadable-file check never ran"
    );
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/// This process's own config home, so nothing here reads or writes the real `~/.config/naivepost`.
fn pin_config_home() -> Paths {
    let root: PathBuf = common::temp_dir_created("server-entries");
    std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
    std::env::set_var("XDG_DATA_HOME", root.join("data"));
    let paths = settings::from_environment().expect("a pinned XDG_CONFIG_HOME");
    std::fs::create_dir_all(&paths.config_dir).expect("config folder");
    paths
}

fn silent() -> Provider {
    std::sync::Arc::new(|key, _| Err(format!("{ITEM}: nothing to probe ({key})")))
}

fn read_file(paths: &Paths) -> server_entries::Servers {
    let text =
        std::fs::read_to_string(server_entries::path(paths)).expect("servers.json is written");
    serde_json::from_str(&text).expect("servers.json parses")
}

fn main_window() -> adw::ApplicationWindow {
    gtk::Window::list_toplevels()
        .into_iter()
        .find_map(|w| w.downcast::<adw::ApplicationWindow>().ok())
        .expect("the main window")
}

fn need<T: IsA<gtk::Widget>>(name: &str) -> T {
    let dialog = ui::settings::open_settings_window().expect("{ITEM}: the dialog is open");
    common::find_in(dialog.upcast_ref(), name)
        .unwrap_or_else(|| panic!("{ITEM}: the dialog has no {name}"))
        .downcast::<T>()
        .unwrap_or_else(|_| panic!("{ITEM}: {name} is not the widget it should be"))
}

fn dropdown(section: Section) -> gtk::DropDown {
    need(&format!("servers-{}", section.key()))
}

fn delete_button(section: Section) -> gtk::Button {
    need(&format!("servers-{}-delete", section.key()))
}

fn box_of(key: &str) -> gtk::Entry {
    need(&format!("entry-{key}"))
}

/// The dropdown's names in order and the selected one.
fn shown(section: Section) -> (Vec<String>, Option<String>) {
    let dropdown = dropdown(section);
    let list = dropdown
        .model()
        .and_then(|model| model.downcast::<gtk::StringList>().ok())
        .expect("the dropdown lists names");
    let names = (0..list.n_items())
        .filter_map(|i| list.string(i).map(|name| name.to_string()))
        .collect();
    let selected = dropdown
        .selected_item()
        .and_then(|item| item.downcast::<gtk::StringObject>().ok())
        .map(|item| item.string().to_string());
    (names, selected)
}

/// Turn the main loop for exactly `for_`, busy or not: a beat is a wall-clock wait.
fn turn(for_: Duration) {
    let until = std::time::Instant::now() + for_;
    let context = glib::MainContext::default();
    while std::time::Instant::now() < until {
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn close_dialog() {
    if let Some(dialog) = ui::settings::open_settings_window() {
        dialog.close();
        drain(Duration::from_millis(30), Duration::from_millis(500));
    }
}
