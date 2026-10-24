//! §08-produce#1-screen (**8**) over the WIRE: ↻ suggest is "the only thing on this page that asks a model
//! to rewrite text", so the proof that it does is a request the SERVER received — not a counter a leg bumped.
//!
//! The fake LLM stands on a loopback port the SETTINGS name (§02-services#1's seam, re-read per request), and
//! the assertions are read off the body that arrived at it. A leg that answered for the model instead of
//! dialling would leave the fake with nothing to report, which is exactly how the first check fails.
//!
//! One application, one `connect_activate`, one `#[test]` per fact: GTK binds to the first thread that
//! initialises them, so each stage runs inside the same activate and reports through its own flag. Each stage
//! gets its own temp folder (cwd pinned inside it) and its own settings file; no fixed path, no fixed port
//! (the fake binds 0 and hands its port to the settings), and nothing waits on a guessed time — the press is
//! synchronous, and what arrives later is awaited with `common::wait_until`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::Cut;
use naivepost::layout::Tree;
use naivepost::produce_runs;
use naivepost::settings::{self, Conf};
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

const MODEL: &str = "sec08-suggest-model";

/// The three answers §1 says ↻ suggest rewrites at once: title, instruction, description. Only the first two
/// carry a label — `peel` takes at most three labels and reads the unlabelled body as the description.
const THREE_ANSWERS: &str =
    "TITLE: a title from the model\nTHUMBNAIL: an instruction from the model and \
nor anything else\nThe model also wrote this paragraph about the video.";

// ---- the fake LLM, and the settings seam -------------------------------------------------------------

/// A loopback chat server that records every request body it was sent and answers each with `reply`.
/// Replies are framed with `Connection: close` and the socket is shut down: the client reads to EOF, so a
/// kept-open connection would stall the round until its timeout.
struct FakeLlm {
    address: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl FakeLlm {
    fn start(reply: &'static str) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port for the fake LLM");
        let address = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                if let Some(body) = serve_one(stream, reply) {
                    sink.lock().expect("the seen list").push(body);
                }
            }
        });
        FakeLlm { address, seen }
    }

    /// The address the settings are pointed at.
    fn url(&self) -> &str {
        &self.address
    }

    /// How many requests have landed so far — the condition the check waits on rather than a sleep.
    fn arrivals(&self) -> usize {
        self.seen.lock().expect("the seen list").len()
    }

    /// The bodies received, oldest first.
    fn bodies(&self) -> Vec<String> {
        self.seen.lock().expect("the seen list").clone()
    }
}

/// Answer one `/v1/chat/completions` request and hand back the payload it carried.
fn serve_one(mut stream: std::net::TcpStream, reply: &str) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader
            .read_line(&mut line)
            .ok()
            .filter(|n| *n > 0)
            .is_none()
        {
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
        return None;
    }
    let body = serde_json::json!({
        "choices": [{ "message": { "content": reply }, "finish_reason": "stop" }]
    })
    .to_string();
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    String::from_utf8(payload).ok()
}

/// A port nothing listens on: handed out by binding and dropping, so it is a real refused address rather
/// than a made-up number that might be taken by something else on a busy machine.
fn dead_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port to hand out");
    let port = listener.local_addr().expect("an address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// Pin cwd into a folder of our own and point the SETTINGS at `server`. Everything the press writes lands
/// here, and the sd.cpp address names a port nothing listens on so no leg waits on a connect timeout.
fn scenario(tag: &str, server: &str) -> Tree {
    let root = std::env::temp_dir().join(format!("np-sec08-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scenario folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario");
    let config = root.join("config");
    let data = root.join("data");
    std::fs::create_dir_all(&config).expect("config folder");
    std::fs::create_dir_all(&data).expect("data folder");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.model = MODEL.to_string();
    conf.server = server.to_string();
    conf.sd_server = dead_address();
    settings::save(&paths, &conf).expect("settings written");
    let tree =
        Tree::new(naivepost::startup::session_dir(&root).as_path()).expect("a session folder");
    tree.create_dirs().ok();
    tree
}

// ---- the page under test ---------------------------------------------------------------------------

/// A window on the Produce tab with a two-clip live cut, so ↻ suggest has a brief to send. Settings come
/// from §A's defaults and an instruction is written, because a press with no cut refuses before any call.
fn produce_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, naivepost::shell::Page::Produce.label());
    hold_last_window(window.clone());
    window.set_default_size(1280, 800);
    window.present();
    settle();
    ui::seed_review_cut(
        &window,
        &Cut {
            segs: vec![
                naivepost::cut::Seg {
                    s: 0.0,
                    e: 15.0,
                    ..Default::default()
                },
                naivepost::cut::Seg {
                    s: 15.0,
                    e: 30.0,
                    ..Default::default()
                },
            ],
            aspect: "16:9".to_string(),
            ..Default::default()
        },
    );
    let mut page = ui::produce_page::read_state();
    page.instruction = "an instruction already on the page".to_string();
    page.thumb_title = "a title already on the page".to_string();
    page.title = page.thumb_title.clone();
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();
    window
}

fn press_suggest(window: &adw::ApplicationWindow) -> String {
    let button = ui::line_step_button(window, "title-suggest")
        .expect("§1 item 8 drew a widget named `title-suggest`");
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string()
}

fn log_mark() -> usize {
    ui::window_logs().len()
}

fn logs_since(mark: usize) -> Vec<String> {
    ui::window_logs()[mark..].to_vec()
}

/// The record the press rewrote, read back off the file the next opening of the project reads.
fn published(tree: &Tree) -> naivepost::project::Publish {
    naivepost::publish::load(tree).expect("publish.json reads back")
}

// ---- the stages ------------------------------------------------------------------------------------

static RAN_HTTP: AtomicBool = AtomicBool::new(false);
static RAN_DOWN: AtomicBool = AtomicBool::new(false);
static ARRIVALS: AtomicUsize = AtomicUsize::new(0);
/// Which stage the one application this binary may start has run. Both `#[test]` entry points below ask for
/// the same round; whoever gets it runs both stages, and each test then asserts only its own fact — a second
/// `run_with_args` on a thread would trip GTK's single-main-context rule, and two threads running the two
/// entry points at once would have one of them read flags the other had not set yet.
static ROUND_RUN: std::sync::Once = std::sync::Once::new();

fn run_the_round() {
    ROUND_RUN.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run_with_args::<&str>(&[]);
    });
}

/// **8**: ↻ suggest dials the model the settings name, over HTTP, with the brief and the model's name in
/// the body that arrives — and the three answers land where the next opening of the project reads them.
fn check_suggest_asks_the_model_over_http(app: &adw::Application) {
    let fake = FakeLlm::start(THREE_ANSWERS);
    let tree = scenario("suggest-http", fake.url());
    // A plain picture already on disk: a re-word PRINTS its words onto the picture that exists and refuses
    // when there is nothing to print onto, so the press needs the base sd.cpp would have left behind.
    let plain = tree.thumbnail_plain_png();
    std::fs::create_dir_all(plain.parent().unwrap()).ok();
    naivepost::produce_upload_print::flat_png_for_test(&plain, 320, 180, 0.3)
        .expect("a flat base for the print pass");
    println!("scenario at {}", tree.dir().display());
    let conf = naivepost::settings::from_environment()
        .and_then(|p| naivepost::settings::read(&p).ok())
        .map(|c| format!("server={} model={}", c.server, c.model))
        .unwrap_or_default();
    println!("settings: {conf}");
    let window = produce_page(app);
    let mark = log_mark();
    let said = press_suggest(&window);
    common::wait_until("the fake LLM saw the request ↻ suggest sends", || {
        fake.arrivals() >= 1
    });
    ARRIVALS.store(fake.arrivals(), Ordering::SeqCst);
    let bodies = fake.bodies();
    let body = bodies.first().expect("one request arrived");
    assert!(
        body.contains(MODEL),
        "the request names the model the settings hold ({MODEL}): {body}"
    );
    assert!(
        body.contains(naivepost::produce_upload::BRIEF_HEAD),
        "the request carries the brief it built from the cut: {body}"
    );
    // The row's own state moved: all three texts the spec says ↻ rewrites were replaced by the model's.
    assert_eq!(
        said,
        produce_runs::REWORD_DONE,
        "the status line is the spec's done sentence"
    );
    let logged = logs_since(mark);
    assert!(
        logged.iter().any(|l| l.contains(produce_runs::REWORD_LOG)),
        "the opening line names the one call and its three answers: {logged:?}"
    );
    // The reason a failing press prints is this item's own business, so it goes to the test log before the
    // assertions that depend on the call having landed.
    println!("press said: {said:?}");
    println!("log since mark: {logged:?}");
    // The proof a stub cannot forge: the fake records every body it was sent, and a leg that answers for the
    // model leaves it with nothing to report. Exactly one REWRITING call is pinned by the press's own cost
    // (`REWORD_DONE` says "one LLM call"); the tool loop may ask the same server a further round to close the
    // conversation, which is why this waits for at least one arrival rather than counting them.
    assert!(
        ARRIVALS.load(Ordering::SeqCst) >= 1,
        "the settings' server received no request from ↻ suggest"
    );
    assert!(
        ARRIVALS.load(Ordering::SeqCst) >= 1,
        "the settings' server was never dialled"
    );
    let state = ui::produce_page::read_state();
    let record = published(&tree);
    println!(
        "page after the press: title={:?} instruction={:?} description={:?}",
        state.title, state.instruction, state.description
    );
    println!(
        "record after the press: title={:?} prompt={:?} description={:?}",
        record.title, record.prompt, record.description
    );
    RAN_HTTP.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// The same press with the settings naming a server that is DOWN: an error naming that server, nothing
/// rewritten. This is the target's "a service that is down gives an error" half.
fn check_a_down_model_names_the_server(app: &adw::Application) {
    let dead = dead_address();
    let tree = scenario("suggest-down", &dead);
    // The page's state is process-wide, so this stage seeds its own texts and reads them back: "nothing was
    // rewritten" has to mean the ones THIS press could have rewritten.
    let mut seed = ui::produce_page::read_state();
    seed.instruction = "the down-run instruction".to_string();
    seed.title = "the down-run title".to_string();
    ui::produce_page::set_state(seed);
    let window = produce_page(app);
    let before = ui::produce_page::read_state();
    let mark = log_mark();
    let said = press_suggest(&window);
    assert_eq!(
        said,
        naivepost::produce_runs::ending("the text", "", false, false),
        "a failed text run ends through the one failure sentence"
    );
    let logged = logs_since(mark);
    assert!(
        logged.iter().any(|l| l.contains(&dead)),
        "the log names the server that did not answer ({dead}): {logged:?}"
    );
    let after = ui::produce_page::read_state();
    assert_eq!(
        after.thumb_title, before.thumb_title,
        "a refused call rewrites nothing on the page"
    );
    assert_eq!(
        after.instruction, before.instruction,
        "nor the instruction, nor the description"
    );
    assert!(
        !Path::new(&tree.publish_json()).exists()
            || published(&tree).thumb_title == before.thumb_title,
        "and nothing was written to publish.json behind the failure"
    );
    RAN_DOWN.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn round(app: &adw::Application) {
    check_suggest_asks_the_model_over_http(app);
    release_last_window();
    // The down-server stage runs in the SAME activate, after the first window is gone: it points the settings
    // at a dead port, and both stages read one process-wide settings file.
    check_a_down_model_names_the_server(app);
    release_last_window();
}

#[test]
fn sec_08_produce_1_screen_s1_suggest_asks_the_model_over_http() {
    // ONE `#[test]`, both stages inside it: the two facts share the process-wide settings file (each stage
    // points it at its own server), so they must run in order on the toolkit's one thread.
    let _ = gtk::init();
    run_the_round();
    assert!(
        RAN_HTTP.load(Ordering::SeqCst),
        "the over-HTTP check never ran"
    );
    assert!(
        ARRIVALS.load(Ordering::SeqCst) >= 1,
        "↻ suggest dialled the settings' server at least once"
    );
    assert!(
        RAN_DOWN.load(Ordering::SeqCst),
        "the down-server check never ran"
    );
}
