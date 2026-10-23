//! `spec/09-llm-and-tools.md` §9 — the model list and the two LLM probes, proven on the wire.
//!
//! Every case here presses a named button on the real Settings dialog and reads what the press changed:
//! the dropdown's items, the badge's mark and tooltip, the Model box, and — because the dialog now dials a
//! socket rather than answering itself — what a loopback fake actually received. The canned-provider
//! version of Fetch/Use lives in `tests/suite/settings_widgets.rs`; what that cannot prove is that a press
//! reaches `/v1/models`, which is the thing §9 asks for and the thing that used to be missing.
//!
//! Determinism: no live server (every address is a loopback port this file opened or left closed), no
//! clock assertions (the timed probe line is matched as a shape, not a number), and one shared `ENV` lock
//! because `XDG_*` and the working directory belong to the process while cargo runs these cases as threads.

use crate::common;
use adw::prelude::*;
use common::{fixture_dir, settle};
use gtk4 as gtk;
use naivepost::checks;
use naivepost::ui;
use naivepost::ui::settings;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the cwd are process-wide, so only one case at a time may point
/// them at its own folders.
static ENV: Mutex<()> = Mutex::new(());

/// A fake LLM on a loopback port. One thread per connection, so a row that makes two calls gets two
/// independent answers, and every request line and body is recorded for the case to read back.
struct Fake {
    url: String,
    seen: std::sync::Arc<Mutex<Vec<(String, String)>>>,
}

impl Fake {
    /// Serve `answer(path)` on every request. The closure decides, so one fake can answer `/v1/models`
    /// with a listing and `/v1/chat/completions` with a completion, the way a real server does.
    fn start(answer: impl Fn(&str) -> String + Send + Sync + 'static) -> Fake {
        let answer = std::sync::Arc::new(answer);
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let recorder = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let recorder = recorder.clone();
                let answer = answer.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().expect("a socket clone"));
                    let mut request_line = String::new();
                    if reader
                        .read_line(&mut request_line)
                        .ok()
                        .filter(|n| *n > 0)
                        .is_none()
                    {
                        return;
                    }
                    let path = request_line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("")
                        .to_string();
                    let mut length = 0usize;
                    loop {
                        let mut header = String::new();
                        if reader
                            .read_line(&mut header)
                            .ok()
                            .filter(|n| *n > 0)
                            .is_none()
                            || header.trim().is_empty()
                        {
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
                    let payload = answer(&path);
                    let framed = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    let _ = stream.write_all(framed.as_bytes());
                    let _ = stream.flush();
                    if let Ok(mut row) = recorder.lock() {
                        row.push((path, String::from_utf8_lossy(&body).into_owned()));
                    }
                });
            }
        });
        Fake {
            url: format!("http://127.0.0.1:{port}"),
            seen,
        }
    }

    /// Wait until `count` requests have landed, then hand back everything recorded. The recording happens
    /// on the connection's own thread, so this is the only waiting a case does.
    fn waited(&self, count: usize) -> Vec<(String, String)> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let seen = self.seen.lock().expect("the record lock").clone();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// A port nothing listens on: the honest stand-in for a server that is down.
fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a spare port");
    let port = listener.local_addr().expect("an address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// Point the settings file at `server` and name `model`, so the LIVE provider reads those values back.
/// The Server box carries the address, but the rows dial the id in the Model box, so both are written.
fn conf_pointing_at(server: &str, model: &str) {
    let root = common::temp_dir(&format!(
        "f9-{}-{}",
        std::process::id(),
        CONF_TAG.lock().unwrap_or_else(|p| p.into_inner()).clone()
    ));
    let paths = naivepost::settings::Paths {
        config_dir: root.join("cfg/naivepost"),
        data_dir: root.join("data/naivepost"),
    };
    let mut conf = naivepost::settings::Conf::default();
    conf.server = server.to_string();
    conf.model = model.to_string();
    naivepost::settings::save(&paths, &conf).expect("the conf is written");
    std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000");
    std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
    std::env::set_var("XDG_DATA_HOME", root.join("data"));
}

/// All seven cases run inside ONE application's activate hook — the arrangement
/// `tests/suite/settings_widgets.rs` uses, because GTK may only have its main loop started by the thread that
/// will run it, and cargo runs `#[test]` functions on a worker thread. Each case records that it got that
/// far, so a case that never ran is a failure rather than a quiet pass. The `#[test]` wrappers take the
/// shared env lock, start this round (idempotent) and then assert their own flag.
fn window_round() {
    common::one_app_round(|app| {
        s1_fetch(app);
        RAN_S1.store(true, Ordering::SeqCst);
        s2_use(app);
        RAN_S2.store(true, Ordering::SeqCst);
        s3_down(app);
        RAN_S3.store(true, Ordering::SeqCst);
        s4_empty(app);
        RAN_S4.store(true, Ordering::SeqCst);
        s5_completion(app);
        RAN_S5.store(true, Ordering::SeqCst);
        s6_vision(app);
        RAN_S6.store(true, Ordering::SeqCst);
        s7_blank(app);
        RAN_S7.store(true, Ordering::SeqCst);
    });
}

static RAN_S1: AtomicBool = AtomicBool::new(false);
static RAN_S2: AtomicBool = AtomicBool::new(false);
static RAN_S3: AtomicBool = AtomicBool::new(false);
static RAN_S4: AtomicBool = AtomicBool::new(false);
static RAN_S5: AtomicBool = AtomicBool::new(false);
static RAN_S6: AtomicBool = AtomicBool::new(false);
static RAN_S7: AtomicBool = AtomicBool::new(false);

/// Which folder the CURRENT case's conf lives in. A lock rather than a `thread_local`: the cases run in
/// one application's activate hook, which GTK starts on the process's main thread, while the `#[test]`
/// that tags and waits sits on a worker. The `ENV` guard every case holds is what makes "current" mean one
/// case at a time.
static CONF_TAG: Mutex<String> = Mutex::new(String::new());

fn tag_this_case(tag: &str) {
    *CONF_TAG.lock().unwrap_or_else(|p| p.into_inner()) = tag.to_string();
}

/// Open the dialog over a real window with the LIVE provider over `fallback`, exactly as the header bar's
/// gear does in a session (`ui::window_policy` installs the same composition).
fn open_live(parent: &adw::ApplicationWindow, fallback: settings::Provider) {
    ui::open_settings(parent, naivepost::settings_probe::live_over(fallback));
    assert!(ui::dialog_open(parent), "the settings dialog is open");
}

/// Type an address into one of the dialog's boxes — §5's "reads what is TYPED". A case points its row at a
/// loopback fake this way rather than by editing the file behind the box, which lags by
/// [`checks::CONF_SAVE_WAIT`] and is not what a press reads. `key` is the ROW's own box: `llm` for the LLM
/// row and for Fetch models, `llm-vision` for the Vision row — each row dials the address typed in its own
/// box, so a case types into the box belonging to the button it presses.
fn type_into(key: &str, value: &str) {
    let dialog = settings::open_settings_window()
        .unwrap_or_else(|| panic!("a settings dialog is open to type into"));
    find_in(dialog.upcast_ref(), &format!("entry-{key}"))
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap_or_else(|| panic!("the dialog carries a box named entry-{key}"))
        .set_text(value);
}

/// What one box of the open dialog holds right now, by key (`entry-<key>`).
#[allow(dead_code)] // kept beside `type_into`: reading back is the other half of that helper for a case
                    // that wants to prove a press wrote a box.
fn read_box(key: &str) -> String {
    let dialog = settings::open_settings_window().expect("a dialog is open");
    find_in(dialog.upcast_ref(), &format!("entry-{key}"))
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|e| e.text().to_string())
        .unwrap_or_default()
}

/// The dialog's own log text — where F0.13 says the verdict lands inside the dialog. Read through the
/// program's accessor rather than a widget walk: `ui::dialog_log` is what the dialog paints, and a
/// hand-written tree walk here would miss whatever the expander has not realised yet.
fn dialog_log() -> String {
    naivepost::ui::dialog_log(&last_window())
}

fn find_in(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Some(found) = find_in(&widget, name) {
            return Some(found);
        }
    }
    None
}

fn dialog_button(name: &str) -> gtk::Button {
    let dialog = settings::open_settings_window().expect("a dialog is open");
    find_in(dialog.upcast_ref(), name)
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .unwrap_or_else(|| panic!("the settings dialog carries a button named {name}"))
}

fn badge_mark(key: &str) -> String {
    settings::badge(&last_window(), key)
        .unwrap_or_else(|| panic!("a badge for {key}"))
        .text()
        .to_string()
}

fn badge_tip(key: &str) -> String {
    settings::badge(&last_window(), key)
        .unwrap_or_else(|| panic!("a badge for {key}"))
        .tooltip_text()
        .map(|text| text.to_string())
        .unwrap_or_default()
}

/// Strong handle on the shell window the CURRENT case built: the badge and dropdown accessors resolve
/// against it, and it must outlive the case that made it. Behind a lock for the reason [`CONF_TAG`] states.
static SHELL: Mutex<Option<ShellHandle>> = Mutex::new(None);

/// The shell window, parked where the case can reach it. GTK objects are not `Send`, so the handle crosses
/// threads only as a raw pointer this module re-borrows on the thread that made it — the same single-owner
/// rule `OPEN` inside the settings dialog follows.
struct ShellHandle(adw::ApplicationWindow);
unsafe impl Send for ShellHandle {}

fn last_window() -> adw::ApplicationWindow {
    SHELL
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .map(|handle| handle.0.clone())
        .expect("the case built its shell window first")
}

fn build_shell(app: &adw::Application) -> adw::ApplicationWindow {
    let project = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    *SHELL.lock().unwrap_or_else(|p| p.into_inner()) = Some(ShellHandle(window.clone()));
    window
}

/// §9's one request, issued by the button: a press on Fetch models puts `GET /v1/models` on the wire and
/// fills the dropdown with what came back. This is the wire that was missing — the live provider used to
/// hand this key to a stub that said "nothing wired to probe fetch-models".
#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s1_fetch_models_button_dials_the_llm_server() {
    common::pin_private_cwd();
    let _turn: std::sync::MutexGuard<'_, ()> = ENV.lock().unwrap_or_else(|p| p.into_inner());
    window_round();
    assert!(RAN_S1.load(Ordering::SeqCst), "s1_fetch never ran");
}

fn s1_fetch(app: &adw::Application) {
    tag_this_case("s1");
    let fake = Fake::start(|path| match path {
        "/v1/models" => r#"{"data":[{"id":"qwen2.5vl:7b"},{"id":"llama3.2:3b"}]}"#.to_string(),
        other => panic!("only the model list may be asked, saw {other}"),
    });
    conf_pointing_at(&fake.url, "qwen2.5vl:7b");
    let window = build_shell(app);
    open_live(&window, settings::no_probe());
    type_into("llm", &fake.url);
    dialog_button("fetch-models-button").emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(
        settings::listed_models(&window),
        vec!["qwen2.5vl:7b".to_string(), "llama3.2:3b".to_string()],
        "the dropdown shows the ids the server listed, in its order"
    );
    assert_eq!(
        badge_mark(settings::FETCH_KEY),
        "\u{2713}",
        "a listing marks the tick"
    );
    assert_eq!(
        badge_tip(settings::FETCH_KEY),
        "2 model(s) served",
        "and its tooltip is the verdict"
    );
    assert!(
        dialog_log().contains("2 model(s) served"),
        "F0.13 mirrors the verdict into the dialog's log: {:?}",
        dialog_log()
    );
    let asked = fake.waited(1);
    assert_eq!(asked.len(), 1, "exactly one request went out");
    assert_eq!(
        asked[0].0, "/v1/models",
        "and it was the model list §9 names"
    );
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
}

/// §9's second half of the list job: Use copies the chosen id into Model, and refuses anything the server
/// did not list. Reached through the buttons, checked against the Model box the press writes.
#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s2_use_copies_the_listed_id_into_model() {
    common::pin_private_cwd();
    let _turn: std::sync::MutexGuard<'_, ()> = ENV.lock().unwrap_or_else(|p| p.into_inner());
    window_round();
    assert!(RAN_S2.load(Ordering::SeqCst), "s2_use never ran");
}

fn s2_use(app: &adw::Application) {
    tag_this_case("s2");
    let fake = Fake::start(|path| match path {
        "/v1/models" => r#"{"data":[{"id":"qwen2.5vl:7b"},{"id":"llama3.2:3b"}]}"#.to_string(),
        other => panic!("only the model list may be asked, saw {other}"),
    });
    conf_pointing_at(&fake.url, "llama3.2:3b");
    let window = build_shell(app);
    open_live(&window, settings::no_probe());
    type_into("llm", &fake.url);
    // Nothing chosen yet: the dropdown starts on no selection, so this press has nothing to adopt and
    // `model_list::use_choice` refuses it (Use copies a LISTED id into Model, nothing else).
    dialog_button("use-model-button").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        badge_mark(settings::USE_KEY),
        "\u{2717}",
        "nothing picked is a cross"
    );
    assert!(
        badge_tip(settings::USE_KEY).contains("no model picked"),
        "the refusal says what is missing: {:?}",
        badge_tip(settings::USE_KEY)
    );

    dialog_button("fetch-models-button").emit_by_name::<()>("clicked", &[]);
    settle();

    let list = find_in(
        settings::open_settings_window()
            .expect("a dialog")
            .upcast_ref(),
        "model-list",
    )
    .and_then(|w| w.downcast::<gtk::DropDown>().ok())
    .expect("the dialog carries the model-list dropdown");
    assert_eq!(
        list.model().expect("the dropdown has a model").n_items(),
        2,
        "two ids after the fetch"
    );
    list.set_selected(1);
    dialog_button("use-model-button").emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(
        settings::entry(&window, "llm")
            .expect("the Model box")
            .text()
            .as_str(),
        "llama3.2:3b",
        "§9: Use copies the id into Model"
    );
    assert_eq!(badge_mark(settings::USE_KEY), "\u{2713}");
    assert_eq!(badge_tip(settings::USE_KEY), "Model set to llama3.2:3b");
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
}

/// A server that is down fails the row for its own reason — never a ✓, and never the stub's sentence about
/// nothing being wired. That distinction is the difference between "this build cannot probe" and "your
/// address is wrong", and only the live route can tell them apart.
#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s3_a_down_server_is_reported_not_faked() {
    common::pin_private_cwd();
    let _turn: std::sync::MutexGuard<'_, ()> = ENV.lock().unwrap_or_else(|p| p.into_inner());
    window_round();
    assert!(RAN_S3.load(Ordering::SeqCst), "s3_down never ran");
}

fn s3_down(app: &adw::Application) {
    tag_this_case("s3");
    let dead = closed_port_url();
    conf_pointing_at(&dead, "some-model");
    let window = build_shell(app);
    open_live(&window, settings::no_probe());
    type_into("llm", &dead);
    dialog_button("fetch-models-button").emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(
        badge_mark(settings::FETCH_KEY),
        "\u{2717}",
        "a down server marks the cross"
    );
    let why = badge_tip(settings::FETCH_KEY);
    assert!(
        !why.contains("nothing wired"),
        "the live route answered this one, so the stub's words must not appear: {why}"
    );
    assert!(
        why.contains("127.0.0.1"),
        "the reason names the address it tried: {why}"
    );
    assert_eq!(
        settings::listed_models(&window),
        Vec::<String>::new(),
        "nothing was listed"
    );
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
}

/// A server that answers the list with nothing is refused with the endpoint named, because that is the
/// thing a person goes to look at. Reached through the press, so the sentence is `model_list::list_verdict`'s
/// own rather than one re-typed in a test.
#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s4_an_empty_list_refuses_with_the_endpoint_named() {
    common::pin_private_cwd();
    let _turn: std::sync::MutexGuard<'_, ()> = ENV.lock().unwrap_or_else(|p| p.into_inner());
    window_round();
    assert!(RAN_S4.load(Ordering::SeqCst), "s4_empty never ran");
}

fn s4_empty(app: &adw::Application) {
    tag_this_case("s4");
    let fake = Fake::start(|path| match path {
        "/v1/models" => r#"{"data":[]}"#.to_string(),
        other => panic!("only the model list may be asked, saw {other}"),
    });
    conf_pointing_at(&fake.url, "some-model");
    let window = build_shell(app);
    open_live(&window, settings::no_probe());
    type_into("llm", &fake.url);
    dialog_button("fetch-models-button").emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(
        badge_mark(settings::FETCH_KEY),
        "\u{2717}",
        "an empty list is not a pass"
    );
    let why = badge_tip(settings::FETCH_KEY);
    assert!(
        why.contains("/v1/models"),
        "the refusal names the endpoint: {why}"
    );
    assert!(
        why.contains("no models"),
        "and says what was missing: {why}"
    );
    assert_eq!(settings::listed_models(&window), Vec::<String>::new());
    fake.waited(1);
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
}

/// F0.13's first probe through its button: ONE completion, thinking off in every place a server reads it,
/// sixteen tokens, and a duration the row MEASURED rather than the budget it was given.
#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s5_the_llm_probe_sends_one_completion_thinking_off_16_tokens(
) {
    common::pin_private_cwd();
    let _turn: std::sync::MutexGuard<'_, ()> = ENV.lock().unwrap_or_else(|p| p.into_inner());
    window_round();
    assert!(RAN_S5.load(Ordering::SeqCst), "s5_completion never ran");
}

fn s5_completion(app: &adw::Application) {
    tag_this_case("s5");
    let fake = Fake::start(|path| match path {
        "/v1/chat/completions" => r#"{"choices":[{"message":{"content":"ok"}}]}"#.to_string(),
        other => panic!("the LLM row sends one completion, saw {other}"),
    });
    conf_pointing_at(&fake.url, "qwen2.5vl:7b");
    let window = build_shell(app);
    open_live(&window, settings::no_probe());
    type_into("llm", &fake.url);
    ui::test_button(&window, "llm")
        .expect("the LLM row has a Test button")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(badge_mark("llm"), "\u{2713}", "an answer is a pass");
    let said = badge_tip("llm");
    // The measured-duration shape, not a fixed number: the row used to print its own 60 s budget,
    // which told the operator about the timeout instead of the model.
    let measured = said
        .split(" answered in ")
        .nth(1)
        .and_then(|rest| rest.split(" s:").next())
        .and_then(|secs| secs.parse::<f64>().ok())
        .unwrap_or_else(|| {
            panic!("F0.13's sentence is \"<model> answered in X s: ...\", got {said}")
        });
    assert!(
        measured < 60.0,
        "a local answer cannot take the whole budget, got {said}"
    );
    assert!(
        said.starts_with("qwen2.5vl:7b answered in"),
        "the row names its model: {said}"
    );
    assert!(
        said.ends_with(": \"ok\""),
        "and quotes what came back: {said}"
    );
    let asked = fake.waited(1);
    assert_eq!(asked.len(), 1, "one completion, as F0.13 says");
    assert_eq!(asked[0].0, "/v1/chat/completions");
    let body: serde_json::Value = serde_json::from_str(&asked[0].1).expect("the chat body is JSON");
    assert_eq!(body["max_tokens"], serde_json::json!(16), "sixteen tokens");
    assert_eq!(
        body["enable_thinking"],
        serde_json::json!(false),
        "thinking off"
    );
    assert_eq!(
        body["chat_template_kwargs"]["enable_thinking"],
        serde_json::json!(false),
        "off in the template kwargs too, which is where some servers read it"
    );
    let messages = body["messages"].as_array().expect("the messages");
    assert_eq!(messages.len(), 1, "one turn, nothing else");
    assert_eq!(
        messages[0]["content"],
        serde_json::json!("Reply with the single word: ok")
    );
    assert_eq!(body["model"], serde_json::json!("qwen2.5vl:7b"));
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
}

/// F0.13's second probe: the picture must be the generated red square, the row must carry NO token cap
/// (the table gives the vision probe none — inheriting the text probe's 16 spent them on reasoning and
/// redded a model that saw the square fine), and only the word "red" passes.
#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s6_the_vision_probe_shows_the_red_square_and_needs_the_word_red(
) {
    common::pin_private_cwd();
    let _turn: std::sync::MutexGuard<'_, ()> = ENV.lock().unwrap_or_else(|p| p.into_inner());
    window_round();
    assert!(RAN_S6.load(Ordering::SeqCst), "s6_vision never ran");
}

fn s6_vision(app: &adw::Application) {
    tag_this_case("s6-sees");
    let sees = Fake::start(|path| match path {
        "/v1/chat/completions" => r#"{"choices":[{"message":{"content":"Red"}}]}"#.to_string(),
        other => panic!("the vision row sends one completion, saw {other}"),
    });
    conf_pointing_at(&sees.url, "qwen2.5vl:7b");
    let window = build_shell(app);
    open_live(&window, settings::no_probe());
    type_into("llm-vision", &sees.url);
    ui::test_button(&window, "llm-vision")
        .expect("the Vision row has a Test button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(badge_mark("llm-vision"), "\u{2713}", "\"Red\" is a pass");
    assert!(
        badge_tip("llm-vision").contains("red square"),
        "the verdict says what it proved: {:?}",
        badge_tip("llm-vision")
    );
    let asked = sees.waited(1);
    let body: serde_json::Value = serde_json::from_str(&asked[0].1).expect("a JSON body");
    assert!(
        body.get("max_tokens").is_none(),
        "the vision probe sets no token cap (§5): {body}"
    );
    let parts = body["messages"][0]["content"]
        .as_array()
        .unwrap_or_else(|| panic!("the vision turn is a part list: {body}"));
    let image = parts
        .iter()
        .find(|part| part["type"].as_str() == Some("image_url"))
        .unwrap_or_else(|| panic!("the probe shows the model a picture: {body}"));
    assert_eq!(
        image["image_url"]["url"].as_str().expect("a data url"),
        checks::vision_probe_url(),
        "the picture is exactly the generated 48 px red square"
    );

    // And a model that cannot see is refused with the fix attached, not just marked wrong.
    let blind = Fake::start(|path| match path {
        "/v1/chat/completions" => r#"{"choices":[{"message":{"content":"blue"}}]}"#.to_string(),
        other => panic!("the vision row sends one completion, saw {other}"),
    });
    conf_pointing_at(&blind.url, "qwen2.5vl:7b");
    let window = build_shell(app);
    open_live(&window, settings::no_probe());
    type_into("llm-vision", &blind.url);
    ui::test_button(&window, "llm-vision")
        .expect("the Vision row has a Test button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        badge_mark("llm-vision"),
        "\u{2717}",
        "a wrong colour is a cross"
    );
    let why = badge_tip("llm-vision");
    assert!(why.contains("not seeing the image"), "{why}");
    assert!(
        why.contains("mmproj"),
        "and names what to serve it with: {why}"
    );
    blind.waited(1);
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
}

/// A server that answers with nothing is a failure. The row used to green-light this: the verdict formatted
/// whatever came back, so an empty reply printed an empty quote beside a ✓.
#[test]
fn sec_09_llm_and_tools_9_model_list_and_tests_s7_an_answering_server_that_says_nothing_fails_the_row(
) {
    common::pin_private_cwd();
    let _turn: std::sync::MutexGuard<'_, ()> = ENV.lock().unwrap_or_else(|p| p.into_inner());
    window_round();
    assert!(RAN_S7.load(Ordering::SeqCst), "s7_blank never ran");
}

fn s7_blank(app: &adw::Application) {
    tag_this_case("s7");
    let blank = Fake::start(|path| match path {
        "/v1/chat/completions" => r#"{"choices":[{"message":{"content":""}}]}"#.to_string(),
        other => panic!("the LLM row sends one completion, saw {other}"),
    });
    conf_pointing_at(&blank.url, "qwen2.5vl:7b");
    let window = build_shell(app);
    open_live(&window, settings::no_probe());
    type_into("llm", &blank.url);
    ui::test_button(&window, "llm")
        .expect("the LLM row has a Test button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(badge_mark("llm"), "\u{2717}", "nothing said is not a pass");
    assert!(
        badge_tip("llm").contains("answered nothing at all"),
        "the refusal says so: {:?}",
        badge_tip("llm")
    );
    blank.waited(1);
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
}
