//! §02-services.md **§1 The four servers**, last paragraph: "Uploads, task runs and the sd.cpp
//! submit/poll ride the run's cancel context, so ⏹ aborts them." An abort that leaves the draw job
//! running on sd.cpp is not an abort, so `POST /sdcpp/v1/jobs/{id}/cancel` has to be a request the
//! program actually makes — this file asserts it off the requests a fake sd.cpp took, exactly as
//! `tests/suite/produce_upload_draw_wire.rs` does for the submit and the poll.
//!
//! Item id: `sec_02_services_1_the_four_servers`. **s12** is this cancel sentence.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use naivepost::produce_upload::DrawnJob;
use naivepost::produce_upload_draw as draw;
use naivepost::settings::{self, Conf};

/// One request the fake took: method, path, and the status it answered with.
#[derive(Debug)]
struct Seen {
    method: String,
    path: String,
}

/// A loopback sd.cpp that answers every request from one script slot and records what it took.
///
/// Every connection is closed after answering: the leg reads to EOF, so a kept-open socket makes each
/// round pay a timeout instead of getting its answer.
fn fake_sd(status: &'static str, body: &'static str) -> (String, Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let (sender, received) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let sender = sender.clone();
            std::thread::spawn(move || serve(stream, status, body, &sender));
        }
    });
    (format!("http://127.0.0.1:{port}"), received)
}

/// Records the request before it answers: a record sent after the reply let the program's next call,
/// which follows the answer at once, report first.
fn serve(mut stream: TcpStream, status: &str, body: &str, seen: &mpsc::Sender<Seen>) {
    if let Some(request) = serve_inner(&mut stream) {
        let _ = seen.send(request);
    }
    let framed = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn serve_inner(stream: &mut TcpStream) -> Option<Seen> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        if line.to_ascii_lowercase().starts_with("content-length:") {
            length = line.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    let method = request_line.split_whitespace().next().unwrap_or_default().to_string();
    let path = request_line.split_whitespace().nth(1).unwrap_or_default().to_string();
    Some(Seen { method, path })
}

/// A session folder whose settings name this fake as the drawing server. The env and the settings file
/// are process-wide, so one scenario runs at a time — the same lock shape `produce_upload_draw_wire` uses.
struct Fixture {
    root: PathBuf,
    tree: naivepost::layout::Tree,
    _scenario: std::sync::MutexGuard<'static, ()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture(tag: &str, sd_url: &str) -> Fixture {
    let scenario = scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("naivepost-sdcancel-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
        // A stray SD_SERVER in the shell would outrank nothing here, but clearing it keeps the fake the
        // only thing that can answer.
        std::env::set_var("SD_SERVER", "");
    }
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = Conf::default();
    conf.sd_server = sd_url.to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture { root, tree, _scenario: scenario }
}

fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

/// Wait for the next request the fake took, failing loudly rather than hanging the suite when none comes.
fn next(received: &Receiver<Seen>, why: &str) -> Seen {
    received
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap_or_else(|_| panic!("timed out: {why}"))
}

/// s12: the cancel really goes out — `POST /sdcpp/v1/jobs/<id>/cancel` — and its row lands in
/// `requests.tsv` like every other draw row.
#[test]
fn sec_02_services_1_the_four_servers_s12_a_cancel_posts_the_job_cancel_path() {
    let (url, received) = fake_sd("200 OK", r#"{"status":"cancelled"}"#);
    let scene = fixture("cancel-ok", &url);
    let job = DrawnJob { id: "job-42".to_string() };

    draw::cancel(&scene.tree, &job).expect("the fake accepted the cancel");
    let seen = next(&received, "the cancel reached the fake sd.cpp");
    assert_eq!(seen.method, "POST", "§1's table gives the cancel POST");
    assert_eq!(seen.path, "/sdcpp/v1/jobs/job-42/cancel", "the job's own cancel path");

    let rows = std::fs::read_to_string(scene.tree.requests_tsv()).expect("requests.tsv");
    assert!(
        rows.contains("thumbnail") && rows.lines().count() >= 2,
        "the cancel left its row: {rows}"
    );
}

/// s12: a refusal is said, not swallowed — the sentence names the drawing server (§Failure-is-specific).
#[test]
fn sec_02_services_1_the_four_servers_s12_a_refused_cancel_names_the_drawing_server() {
    let (url, received) = fake_sd("500 Internal Server Error", "{}");
    let scene = fixture("cancel-bad", &url);
    let job = DrawnJob { id: "job-9".to_string() };

    let why = draw::cancel(&scene.tree, &job).expect_err("a 500 is not an acceptance");
    assert!(
        why.contains(&url) && why.contains("500"),
        "the reason names the server and its answer: {why}"
    );
    let seen = next(&received, "the refused cancel still reached the fake");
    assert_eq!(seen.path, "/sdcpp/v1/jobs/job-9/cancel");
}

/// s12: ⏹ during the one-per-second poll cancels the job before the stop is reported, exactly once, and
/// the loop still returns the same stop sentence it always did.
#[test]
fn sec_02_services_1_the_four_servers_s12_a_stop_during_the_poll_cancels_the_job() {
    let (url, received) = fake_sd("200 OK", r#"{"status":"queued","queue_position":3}"#);
    let scene = fixture("stop", &url);
    let job = DrawnJob { id: "job-stop" .to_string() };

    // Stop after the first ask: the loop polls once, then finds the run stopped on its next turn.
    let asks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let cancelled = || {
        asks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 1
    };
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let logged = lines.clone();

    let stop = draw::wait_until_done(&scene.tree, &job, &cancelled, |line| {
        logged.lock().expect("the log").push(line.to_string())
    })
    .expect_err("a stopped run does not report a finished picture");
    assert!(
        stop.contains("stopped"),
        "the loop's own stop sentence is unchanged: {stop}"
    );

    // Whatever order the fake reports them in, exactly one poll and exactly one cancel went out, and no
    // second ask followed the stop.
    let mut paths = vec![next(&received, "the loop asked the fake sd.cpp")];
    while let Ok(more) = received.recv_timeout(std::time::Duration::from_millis(300)) {
        paths.push(more);
    }
    let cancels: Vec<&Seen> = paths
        .iter()
        .filter(|seen| seen.path.ends_with("/cancel"))
        .collect();
    assert_eq!(cancels.len(), 1, "one stop, one cancel: {paths:?}");
    assert_eq!(cancels[0].method, "POST", "§1's table gives the cancel POST");
    assert_eq!(cancels[0].path, "/sdcpp/v1/jobs/job-stop/cancel");
    assert!(
        paths.iter().any(|seen| seen.path == "/sdcpp/v1/jobs/job-stop"),
        "the loop polled the job before stopping: {paths:?}"
    );
}
