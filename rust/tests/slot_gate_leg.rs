//! §02-services.md **§1 The four servers**, its first sentence: "each model is given at most its
//! Settings slot count of requests at once ([09 §4](spec/09-llm-and-tools.md#4-liveness-and-the-gate-f63))".
//!
//! Item id: `sec_02_services_1_the_four_servers`. These drive the real dial
//! ([`naivepost::audio_task::dial`]) against a loopback server that holds the connection open until the
//! test lets it go, so the gate is proven where §1 puts it — around the HTTP call — rather than on the
//! gate alone. **s6** is this item's slot sentence.
//!
//! Each test uses its own model id and its own fake server, because the registry is process-wide by
//! design (one app, one set of slots per model) and cargo runs these as threads of one binary.

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use naivepost::audio_task::{self, Ask};
use naivepost::bodies::{self, ServerPath};
use naivepost::settings::{self, Conf, Paths};
use naivepost::slot_gate;

/// A temp config folder pointing the audio box at a fake server, with the slot count §6 spells per task.
struct Dirs {
    paths: Paths,
}

impl Dirs {
    fn new(label: &str, url: &str, asr_slots: u32) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-svcs6-{label}-{pid}-{n}",
            pid = std::process::id(),
            n = COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let mut conf = Conf::default();
        conf.audio_server = url.to_string();
        conf.slots.asr = asr_slots;
        settings::save(&paths, &conf).expect("the conf");
        Dirs { paths }
    }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Held while a test pins the process-wide env, so two tests never resolve endpoints at once.
static ENV: Mutex<()> = Mutex::new(());

/// A fake audio.cpp that answers each request only when the test releases it.
///
/// Every accepted connection lives until its release handle fires, so a dial in flight is a dial holding
/// a slot — which is the only state the slot rule is about. The reply body is an ASR answer, so the door
/// reports success.
fn held_server() -> (String, Releaser) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let gate = Arc::new(Mutex::new(false));
    let shared = gate.clone();
    let arrived = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let noted = arrived.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let gate = shared.clone();
            let noted = noted.clone();
            std::thread::spawn(move || {
                drain(&stream);
                // The request is fully read: whoever dialed it is on the wire now, which is what a test
                // waits for instead of guessing at a sleep.
                noted.store(true, std::sync::atomic::Ordering::SeqCst);
                // The request is fully read: whoever dialed it is on the wire now, which is what a test
                // waits for instead of guessing at a sleep.
                // Wait until the test says so: the request stays unanswered, exactly as a busy model's does.
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    let open = *gate.lock().expect("the release flag");
                    if open || Instant::now() > deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                answer(stream);
            });
        }
    });
    (
        format!("http://127.0.0.1:{port}"),
        Releaser {
            gate,
            arrived,
        },
    )
}

/// Lets the held connections answer, and reports whether a request has been fully read yet.
struct Releaser {
    gate: Arc<Mutex<bool>>,
    arrived: Arc<std::sync::atomic::AtomicBool>,
}

impl Releaser {
    fn release(&self) {
        *self.gate.lock().expect("the release flag") = true;
    }

    /// True once a request has arrived and been read — the honest signal that a dial is on the wire.
    fn arrived(&self) -> bool {
        self.arrived.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Forget any request seen so far, so the next arrival can be waited for.
    fn reset(&self) {
        self.arrived.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Drop for Releaser {
    fn drop(&mut self) {
        self.release();
    }
}

/// Read one request far enough that the client is not left waiting to finish writing.
fn drain(stream: &TcpStream) {
    let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone the socket"));
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
}

/// Answer with an ASR-shaped reply and close, ending the caller's read.
fn answer(mut stream: TcpStream) {
    let payload = r#"{"text":"hello there","words":[]}"#;
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// One dial of the real door, with the env pinned under the lock like every other test here.
fn dial(dirs: &Dirs, step: &str, model: &str) -> Result<String, String> {
    dial_at(&dirs.paths, None, step, model)
}

/// [`dial`] with the session tree that makes the timed row land: same door, same env pinning, and the
/// only difference is whether there is a project folder to append `requests.tsv` to.
///
/// The env is process-wide, so it is pinned once for the whole binary (`main`) rather than per dial:
/// these dials block on purpose, and a lock held across one would starve every other test here. Each
/// test points its own conf at its own fake server; clearing the two audio variables once means nothing
/// inherited from the shell outranks that box.
fn dial_at(paths: &Paths, tree: Option<naivepost::layout::Tree>, step: &str, model: &str) -> Result<String, String> {
    let audio = ServerPath::from_upload("/uploads/c00.wav").expect("absolute");
    audio_task::dial(
        &tree,
        paths,
        Ask { step, job: "c00.wav", task: "asr" },
        model,
        &bodies::asr_body(model, &audio, "en"),
    )
}

/// Clear the audio env once, before any dial resolves an endpoint.
fn pin_env() {
    std::env::remove_var("NAIVEPOST_TTS_URL");
    std::env::remove_var("AUDIOCPP_SERVER");
}

/// Dial from a thread and report when it finished, so a blocked dial is observable.
fn dial_async(dirs: &'static Dirs, step: &'static str, model: &'static str) -> std::thread::JoinHandle<bool> {
    std::thread::spawn(move || dial(dirs, step, model).is_ok())
}

/// One request on the wire holds the only slot; a second waits and goes through exactly when the first ends.
#[test]
fn sec_02_services_1_the_four_servers_s6_one_slot_admits_one_request_at_a_time() {
    let (url, releaser) = held_server();
    pin_env();
    let dirs = Box::leak(Box::new(Dirs::new("s6a", &url, 1)));
    let model = "gate-one-slot";

    let first = dial_async(dirs, "Prepare", model);
    // Wait for the server to have read a request: that is when the first dial holds its slot and is on
    // the wire, which is the state the second ask has to queue behind.
    wait_until(|| releaser.arrived(), "the first dial reached the server");
    // The gate is keyed by the URL the door resolved, which is the conf's box verbatim; `server_url`
    // trims a trailing slash, so compare against what the door itself sees.
    let seen = slot_gate::holders(&url, model);
    assert_eq!(seen.len(), 1);

    // A second ask arrives while the only slot is busy: it must not reach the wire.
    let second = dial_async(dirs, "Marking", model);
    std::thread::sleep(Duration::from_millis(120));
    assert!(
        !second.is_finished(),
        "with one slot taken the second request has to wait, not go out"
    );

    releaser.release();
    assert!(first.join().expect("the first dial ends"), "the first succeeded");
    // The hand-off is automatic on release, so the waiter goes in without asking again.
    wait_until(|| second.is_finished(), "the released slot went to whoever waited");
    assert!(second.join().expect("the second dial ends"), "the second succeeded after the wait");
    assert!(
        slot_gate::holders(&url, model).is_empty(),
        "both slots came back: {:?}",
        slot_gate::holders(&url, model)
    );
}

/// Two slots admit two at once, and a third still waits — the count is what was configured, not one.
#[test]
fn sec_02_services_1_the_four_servers_s6_two_slots_admit_two_at_once() {
    let (url, releaser) = held_server();
    pin_env();
    let dirs = Box::leak(Box::new(Dirs::new("s6b", &url, 2)));
    let model = "gate-two-slots";

    let first = dial_async(dirs, "Prepare", model);
    let second = dial_async(dirs, "Marking", model);
    wait_until(
        || slot_gate::holders(&url, model).len() == 2,
        "two slots hold two requests",
    );
    let third = dial_async(dirs, "Narrate", model);
    std::thread::sleep(Duration::from_millis(120));
    assert!(!third.is_finished(), "the third request waits behind two busy slots");

    releaser.release();
    assert!(first.join().expect("first done"));
    assert!(second.join().expect("second done"));
    wait_until(|| third.is_finished(), "the freed slot reached the waiter");
    assert!(third.join().expect("third done"));
}

/// The wait is not swallowed: §09 §10 gives `requests.tsv` a "how long it waited for a slot" column
/// precisely so slot counts can be chosen from it, so a row written after a wait carries a number.
#[test]
fn sec_02_services_1_the_four_servers_s6_a_row_written_after_a_wait_carries_the_wait() {
    let (url, releaser) = held_server();
    pin_env();
    let dirs = Dirs::new("s6c", &url, 1);
    let project = std::env::temp_dir().join(format!("np-svcs6-proj-{}.naivepost", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).expect("the project folder");
    let tree = naivepost::layout::Tree::new(&project).expect("a session tree");
    let model = "gate-wait-column";

    // First dial takes the slot and blocks; the second writes its row only after waiting for it.
    let held = std::thread::spawn({
        let paths = dirs.paths.clone();
        move || dial_at(&paths, None, "Prepare", model)
    });
    wait_until(|| !slot_gate::holders(&url, model).is_empty(), "the first dial holds the slot");
    let waited = std::thread::spawn({
        let paths = dirs.paths.clone();
        let timed = tree.clone();
        move || dial_at(&paths, Some(timed), "Marking", model)
    });
    std::thread::sleep(Duration::from_millis(150));
    releaser.release();
    assert!(held.join().expect("first dial").is_ok());
    assert!(waited.join().expect("second dial").is_ok());

    let rows = std::fs::read_to_string(tree.requests_tsv()).expect("requests.tsv");
    let row = rows.lines().last().expect("a row");
    let fields: Vec<&str> = row.split('\t').collect();
    // Column 12 is `wait_s`; empty means "never queued", which would be a lie about this request.
    let wait_field = fields.get(12).copied().unwrap_or_default();
    assert!(!wait_field.is_empty(), "the waiting row records its wait: {row}");
    let secs: f64 = wait_field.parse().expect("the wait column parses as seconds");
    assert!(secs > 0.0, "the wait is a positive number of seconds: {row}");
}

/// Poll `check` until it holds, failing loudly rather than hanging the suite when it never does.
fn wait_until(check: impl Fn() -> bool, why: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !check() {
        assert!(Instant::now() < deadline, "timed out: {why}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
