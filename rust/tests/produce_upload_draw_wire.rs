//! F5.6 S6 over the WIRE: `naivepost::produce_upload_draw` dialling a local fake sd.cpp, the same seam a
//! real settings edit uses — `sd_server` in the settings file names the address (§02-services#1), and no
//! answer is scripted inside the library. The two job kinds are asserted off the requests the server took:
//! submit goes to `/sdcpp/v1/img_gen`, each poll to `/sdcpp/v1/jobs/<id>`, one ask per second.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use naivepost::produce_upload as up;
use naivepost::produce_upload_draw as draw;
use naivepost::settings::{self, Conf};

/// Held while a scenario's environment is being pointed at it: `XDG_*` and the settings file are process-wide.
fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

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

/// A scenario folder whose settings name this fake as the drawing server.
fn fixture(tag: &str, sd_url: &str) -> Fixture {
    let scenario = scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("naivepost-f56draw-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
        // An exported SD_SERVER would outrank nothing here, but clearing it keeps a stray dev value from
        // answering instead of the fake this check built.
        std::env::set_var("SD_SERVER", "");
    }
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = Conf::default();
    conf.sd_server = sd_url.to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture { root, tree, _scenario: scenario }
}

// ---- the fake sd.cpp ----------------------------------------------------------------------------

/// A loopback server that answers the two job kinds from a script, one reply per request, and hands back
/// each request's method + path so the wire itself can be asserted.
struct FakeSd {
    port: u16,
    received: Receiver<Seen>,
    script: Arc<Mutex<Script>>,
}

/// One request the fake took.
#[derive(Debug)]
struct Seen {
    path: String,
    body: String,
}

#[derive(Default)]
struct Script {
    replies: Vec<String>,
    served: usize,
}

impl Script {
    fn take(&mut self) -> String {
        let index = self.served;
        self.served += 1;
        self.replies
            .get(index)
            .cloned()
            .or_else(|| self.replies.last().cloned())
            .unwrap_or_default()
    }
}

impl FakeSd {
    fn start(replies: Vec<String>) -> FakeSd {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(Script { replies, served: 0 }));
        let serving = script.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let serving = serving.clone();
                let sender = sender.clone();
                // One thread per connection: the leg reads until EOF, so a serial accept loop lets a request
                // whose reply was already taken hold its socket open and stalls every later check behind it.
                std::thread::spawn(move || {
                    if let Some(seen) = serve(stream, &serving) {
                        let _ = sender.send(seen);
                    }
                });
            }
        });
        FakeSd { port, received, script }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn next(&self) -> Seen {
        self.received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the drawing call reached the fake sd.cpp")
    }
}

fn serve(mut stream: TcpStream, script: &Arc<Mutex<Script>>) -> Option<Seen> {
    // Close the socket on every exit: the client reads to EOF, so a kept-open connection makes each round
    // pay a timeout (this binary stalled 3+ min before).
    let seen = serve_inner(&mut stream, script);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    seen
}

fn serve_inner(stream: &mut TcpStream, script: &Arc<Mutex<Script>>) -> Option<Seen> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            length = rest.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or_default().to_string();
    let body = String::from_utf8_lossy(&payload).to_string();
    let reply = script.lock().expect("script lock").take();
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.len(),
        reply
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    Some(Seen { path, body })
}

fn silent(_line: &str) {}

/// §F5.6 S6 over the wire: the submit leg dials `/sdcpp/v1/img_gen` with the body the walk built, and the
/// job id comes out of the reply — the only handle the two job kinds have.
#[test]
fn f5_6_s6_the_submit_leg_dials_img_gen_and_reads_the_job_id() {
    let sd = FakeSd::start(vec![serde_json::json!({ "id": "job-77" }).to_string()]);
    let scene = fixture("submit", &sd.url());
    let body = up::request_body(
        &up::draw_prompt("brighten it", up::Band::Upper),
        Some("no blur"),
        1280,
        720,
        &[],
    );
    let job = draw::submit(&scene.tree, &body).expect("the fake queued the job");
    assert_eq!(job.id, "job-77", "the id came off the reply");
    let seen = sd.next();
    assert_eq!(seen.path, "/sdcpp/v1/img_gen", "§3.5's submit path");
    let sent: serde_json::Value = serde_json::from_str(&seen.body).expect("the body is JSON");
    assert_eq!(sent["width"], 1280, "the frame is the video's shape at long side 1280");
    assert_eq!(sent["height"], 720);
    assert_eq!(sent["seed"], -1, "a fresh draw every time");
    assert_eq!(sent["output_format"], "png");
    assert!(
        sent["prompt"].as_str().unwrap_or_default().contains("Do not write any words"),
        "the no-words tail belongs in the prompt: {sent}"
    );
}

/// The poll leg reports the status word AND the queue position, once per second until the job is done; the
/// lines are §F5.6's `drawing (<status>)` / `drawing (<status>, N ahead in the queue)`.
#[test]
fn f5_6_s6_the_poll_leg_reports_status_and_queue() {
    let sd = FakeSd::start(vec![
        serde_json::json!({ "status": "queued", "queue_position": 3 }).to_string(),
        serde_json::json!({ "status": "drawing" }).to_string(),
        serde_json::json!({ "status": "done" }).to_string(),
    ]);
    let scene = fixture("poll", &sd.url());
    let job = up::DrawnJob { id: "job-77".to_string() };
    let mut lines: Vec<String> = Vec::new();
    let started = std::time::Instant::now();
    let last = draw::wait_until_done(&scene.tree, &job, &|| false, |line| lines.push(line.to_string()))
        .expect("the job finished");
    assert!(last.done, "the loop stops on the answer that says done");
    assert_eq!(lines.len(), 3, "one line per ask: {lines:?}");
    assert_eq!(lines[0], "drawing (queued, 3 ahead in the queue)", "{}", lines[0]);
    assert_eq!(lines[1], "drawing (drawing)", "{}", lines[1]);
    assert_eq!(lines[2], "drawing (done)", "{}", lines[2]);
    // Every poll went to the JOB's own path, and the cadence is one ask per second (§F5.6).
    for _ in 0..3 {
        let seen = sd.next();
        assert_eq!(seen.path, "/sdcpp/v1/jobs/job-77", "the job kind carries its id");
    }
    let elapsed = started.elapsed();
    // The first ask goes out at once and the next two wait a second each: three asks cannot finish inside
    // two seconds when the cadence is one per second (§F5.6). Measured, not assumed.
    assert!(
        elapsed >= std::time::Duration::from_millis(1900),
        "three asks at one per second take about two seconds: {elapsed:?}"
    );
    assert_eq!(draw::poll_interval(), std::time::Duration::from_secs(1), "the spec's cadence");
}

/// A drawing server that is not there fails by NAME, and the text published before the draw stays on disk
/// (S5: a failed draw keeps the thinking rather than losing an LLM call to a GPU's bad night).
#[test]
fn f5_6_s6_a_down_image_server_keeps_the_published_text() {
    let live = FakeSd::start(vec![]);
    let scene = fixture("down", &live.url());
    // Point the settings at a port nothing listens on — the same edit a user makes in the dialog.
    let dead = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port to hand out");
        let port = listener.local_addr().expect("an address").port();
        drop(listener);
        format!("http://127.0.0.1:{port}")
    };
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.sd_server = dead.clone();
    settings::save(&paths, &conf).expect("address moved");
    // Publish first, exactly as the walk does (S5), then fail the draw.
    let mut record = naivepost::project::Publish::default();
    record.prompt = "brighten it".to_string();
    record.thumb_title = "A win nobody expected".to_string();
    naivepost::publish::save(&record, &scene.tree).expect("publish.json written");
    let body = up::request_body("brighten it", None, 1280, 720, &[]);
    let err = draw::submit(&scene.tree, &body).expect_err("nothing listens there");
    assert!(err.contains(&dead), "the reason names the address tried: {err}");
    assert!(
        naivepost::publish::is_written(&scene.tree),
        "the publish files survive a failed draw (S5)"
    );
    assert_eq!(live.script.lock().expect("script lock").served, 0, "the live fake was never dialled");
}
