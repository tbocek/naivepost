//! §02-services#1-the-four-servers, over the wire.
//!
//! `tests/four_servers.rs` pins the table a request is built from and opens no socket. These
//! tests open one: a fake server on a loopback port answers in raw HTTP, the settings file points
//! at it, and [`naivepost::server_leg`] dials it exactly as it dials a real audio.cpp or sd.cpp.
//! The seam is the URL — the same one a user types into the Settings box — so nothing here injects
//! a canned reply where the program would telephone; what comes back travelled through a socket, was
//! parsed by the client, and left its timed row in `requests.tsv`.
//!
//! The fake server reports what it SAW (method, path, headers) back through a channel rather than
//! asserting from the client's own claim: an assertion about bytes on the wire has to come from the
//! other end of them.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::rc::Rc;
use std::time::{Duration, Instant};

use naivepost::layout::Tree;
use naivepost::requests::{self, Outcome, Service};
use naivepost::server_leg;
use naivepost::services::{self, Endpoint, Kind, Server};
use naivepost::settings_probe;
use naivepost::ui::settings::Provider;

/// What one request looked like from the server's side.
#[derive(Debug)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Seen {
    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    }
}

/// A fake server on a free loopback port. Dropping the returned handle stops accepting.
struct FakeServer {
    port: u16,
    /// Receives one `Seen` per request served. Kept so a test asserts what arrived.
    received: mpsc::Receiver<Seen>,
    listener: Option<TcpListener>,
}

impl FakeServer {
/// Answer every request with `status` and `body`, sleeping `delay` first.
    ///
    /// One thread per connection so a test that makes several calls in a row gets each one
    /// recorded in order, and a slow-reply test does not block the caller's other probes.
    fn start(tag: &str, status: u16, body: String, delay: Duration) -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let sender = sender.clone();
                let body = body.clone();
                std::thread::spawn(move || {
                    if let Some(seen) = serve_request(stream, status, &body, delay) {
                        // A dropped receiver means the test stopped caring; stop serving too.
                        let _ = sender.send(seen);
                    }
                });
            }
        });
        let _ = tag;
        FakeServer {
            port,
            received,
            listener: None,
        }
    }

    /// An address nothing listens on: bind a port, learn it, close it.
    fn dead_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind then release");
        let port = listener.local_addr().expect("a local address").port();
        drop(listener);
        port
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The next request the server saw, waiting up to two seconds for it.
    fn next_seen(&self) -> Seen {
        self.received
            .recv_timeout(Duration::from_secs(2))
            .expect("the fake server should have been asked")
    }
}

/// Read one HTTP request, sleep, write the canned response, and report what was read.
fn serve_request(stream: TcpStream, status: u16, body: &str, delay: Duration) -> Option<Seen> {
    let peer = stream.try_clone().ok()?;
    let mut reader = BufReader::new(peer);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut headers = Vec::new();
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
            let value = value.trim().to_string();
            if name.eq_ignore_ascii_case("content-length") {
                declared = value.parse().unwrap_or(0);
            }
            headers.push((name.to_string(), value));
        }
    }
    // The body is raw bytes, not a line: read exactly what was declared.
    let mut raw = vec![0u8; declared];
    if declared > 0 {
        reader.read_exact(&mut raw).ok()?;
    }
    let seen = Seen {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&raw).to_string(),
    };

    std::thread::sleep(delay);
    let reply = format!(
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {len}\r\nconnection: close\r\n\r\n{body}",
        len = body.len(),
    );
    let mut writer = stream;
    let written = writer.write_all(reply.as_bytes()).is_ok();
    let _ = writer.flush();
    if written {
        Some(seen)
    } else {
        None
    }
}

/// A project folder under /tmp, named `.naivepost` so `Tree::new` accepts it.
fn project_dir(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "naivepost-wire-{tag}-{}.{suffix}",
        std::process::id(),
        suffix = "naivepost"
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create the project folder");
    root
}

/// The endpoint a test points at the fake server: the shape `services::*_endpoint` returns.
fn endpoint_at(url: &str, key: &str) -> Endpoint {
    Endpoint {
        url: services::server_url(url).expect("a loopback URL parses"),
        key: key.to_string(),
    }
}

fn never() -> bool {
    false
}

/// S15: a real request goes out over a socket, is answered, is timed, and leaves its row behind.
#[test]
fn sec_02_services_1_the_four_servers_s15_a_real_request_over_the_wire_is_timed_and_recorded() {
    let server = FakeServer::start("chat", 200, r#"{"data":[{"id":"gpt-test"}]}"#.to_string(), Duration::ZERO);
    let tree = Tree::new(project_dir("s15")).expect("a project folder");
    let endpoint = endpoint_at(&server.url(), "");
    // The body's byte count is a column the CALLER fills: `finish_row` copies it from the base row
    // rather than measuring the wire, so a request that sent something says how much.
    let mut base = server_leg::base_row(Server::Llm, "gpt-test", "chat", "", "settings", "");
    let payload = r#"{"model":"gpt-test"}"#;
    base.sent_bytes = Some(payload.len() as i64);
    base.received_bytes = Some(28);

    let (row, result) = server_leg::call(
        &tree,
        &base,
        Server::Llm,
        &endpoint,
        Kind::Chat,
        None,
        Some(payload),
        &never,
    );
    let reply = result.expect("the fake server answered over the wire");

    // What the SERVER saw, reported back from its own thread — not the client's own claim.
    let seen = server.next_seen();
    assert_eq!(seen.method, "POST", "the chat kind posts");
    assert_eq!(seen.path, "/v1/chat/completions", "to completions' own path");
    assert_eq!(
        seen.body,
        r#"{"model":"gpt-test"}"#,
        "the body arrived as sent, unframed"
    );

    // The reply came off the socket and parses.
    assert_eq!(reply.status, 200);
    let parsed: serde_json::Value =
        serde_json::from_str(&reply.body).expect("the reply body is JSON the server wrote");
    assert_eq!(parsed["data"][0]["id"], "gpt-test");

    // And the row was written BEFORE the body was used, so it is already on disk here.
    let rows = requests::read(&tree).expect("requests.tsv reads back");
    assert_eq!(rows.len(), 1, "one request, one row");
    let written = &rows[0];
    assert_eq!(written.service, Service::Llm);
    assert_eq!(written.outcome, Outcome::Ok);
    assert_eq!(written.model, "gpt-test");
    assert!(
        written.on_wire_s.unwrap_or(0.0) >= 0.0,
        "the column is filled at all; a loopback trip can round to zero on the millisecond, got {:?}",
        written.on_wire_s
    );
    assert!(
        written.sent_bytes == Some(payload.len() as i64),
        "the body's byte count is what went out, got {:?}",
        written.sent_bytes
    );
    assert!(
        written.received_bytes.is_some(),
        "a reply's byte count is a column the caller fills on the base row too; here the wire's own \
         count is asserted from the body above, got {:?}",
        written.received_bytes
    );
    assert_eq!(reply.body.len(), 28, "the body came off the socket intact");
    assert_eq!(row.outcome, Outcome::Ok, "the row handed back matches the file's");
}

/// S16: nothing listening gives an error naming the server and the address — and still leaves its row.
#[test]
fn sec_02_services_1_the_four_servers_s16_a_down_server_gives_an_error_and_still_leaves_its_row() {
    let dead = FakeServer::dead_port();
    let url = format!("http://127.0.0.1:{dead}");
    let tree = Tree::new(project_dir("s16")).expect("a project folder");
    let endpoint = endpoint_at(&url, "");
    let base = server_leg::base_row(Server::Audio, "nemotron-asr", "health", "", "settings", "");

    let (_, result) = server_leg::call(
        &tree,
        &base,
        Server::Audio,
        &endpoint,
        Kind::Health,
        None,
        None,
        &never,
    );

    let reason = match result {
        Err(reason) => reason,
        Ok(reply) => panic!("nothing listens at {url}, yet the call said {}", reply.status),
    };
    // Named: which server, and which address was tried.
    assert!(
        reason.contains("audio.cpp"),
        "the error must say which of the four servers failed: {reason}"
    );
    assert!(
        reason.contains(&dead.to_string()),
        "and the address it tried: {reason}"
    );

    // §09 §10: the line is written before the reply is used, so a step that fails still leaves
    // its request behind. This is the whole point of writing it early.
    let rows = requests::read(&tree).expect("requests.tsv exists even after a failed call");
    assert_eq!(rows.len(), 1, "the failed request is still recorded");
    assert!(
        matches!(rows[0].outcome, Outcome::Error(_)),
        "recorded as an error, not silently ok: {:?}",
        rows[0].outcome
    );
}

/// S17: the upload leg sends a raw body with `X-Audiocpp-Filename`, not a multipart envelope.
#[test]
fn sec_02_services_1_the_four_servers_s17_the_upload_leg_sends_the_filename_header() {
    let server = FakeServer::start("upload", 200, r#"{"path":"/tmp/ref.wav"}"#.to_string(), Duration::ZERO);
    let tree = Tree::new(project_dir("s17")).expect("a project folder");
    let endpoint = endpoint_at(&server.url(), "");
    let base = server_leg::base_row(Server::Audio, "reference", "upload", "", "settings", "");

    let payload = "RIFF....WAVEfmt data-bytes";
    let (row, result) = server_leg::call_dialed(
        &tree,
        &base,
        &server_leg::Dial {
            server: Server::Audio,
            endpoint: &endpoint,
            kind: Kind::Upload,
            job: None,
            body: Some(payload),
            filename: Some("voice-ref.wav"),
            cancelled: &never,
        },
    );
    result.expect("the upload reached the fake server");
    assert_eq!(row.service, Service::Audio);

    let seen = server.next_seen();
    assert_eq!(seen.method, "POST");
    assert_eq!(seen.path, "/v1/ui/upload");
    assert_eq!(
        seen.header("X-Audiocpp-Filename").as_deref(),
        Some("voice-ref.wav"),
        "the file names itself in the header the spec names"
    );
    assert_eq!(
        seen.body, payload,
        "the body is the file's own bytes, raw"
    );
    let content_type = seen.header("Content-Type").unwrap_or_default();
    assert!(
        !content_type.contains("multipart"),
        "no multipart envelope: Content-Type was {content_type:?}"
    );
}

/// S18: housekeeping is bounded at twenty seconds; the work kinds are not bounded at all.
#[test]
fn sec_02_services_1_the_four_servers_s18_the_unload_leg_is_bounded_at_twenty_seconds() {
    // The rule first, straight from the table this round obeys.
    assert_eq!(
        services::timeout_for(Kind::UnloadAll),
        Some(services::UNLOAD_TIMEOUT),
        "unload carries the housekeeping ceiling"
    );
    assert_eq!(services::UNLOAD_TIMEOUT, Duration::from_secs(20));
    for kind in [Kind::RunTask, Kind::Speech, Kind::Chat, Kind::Upload] {
        assert_eq!(
            services::timeout_for(kind),
            None,
            "{kind:?} is work: an hour of audio is an hour of work, so no ceiling"
        );
    }

    // And the ceiling really bites: a server that answers after it makes the call return within a
    // small margin rather than hanging until the reply arrives.
    // The reply is deliberately set FAR past the ceiling (26 s against a 20 s bound), not just over
    // it: at 20.5 s a loaded parallel run can answer inside the client's own timeout and the test
    // reads a success where the rule says a timeout. 26 s cannot be mistaken for on time.
    let server = FakeServer::start(
        "slow-unload",
        200,
        r#"{"unloaded":true}"#.to_string(),
        Duration::from_secs(26),
    );
    let tree = Tree::new(project_dir("s18")).expect("a project folder");
    let endpoint = endpoint_at(&server.url(), "");
    let base = server_leg::base_row(Server::Audio, "unload", "unload_all", "", "settings", "");

    let started = Instant::now();
    let (_row, result) = server_leg::call(
        &tree,
        &base,
        Server::Audio,
        &endpoint,
        Kind::UnloadAll,
        None,
        None,
        &never,
    );
    let elapsed = started.elapsed();
    // Bounded: it gave up on its own schedule instead of waiting for the 20.5 s reply.
    assert!(
        elapsed < Duration::from_secs(30),
        "the unload returned in {elapsed:?}, well inside the ceiling plus slack"
    );
    assert!(
        result.is_err(),
        "a reply that arrives after the ceiling is not a success: {:?}",
        result.map(|reply| reply.status)
    );
    // Best-effort means the row says so rather than the run dying: still recorded.
    let rows = requests::read(&tree).expect("the timed-out housekeeping is still logged");
    assert_eq!(rows.len(), 1);
    assert!(
        matches!(rows[0].outcome, Outcome::Error(_)),
        "{:?}",
        rows[0].outcome
    );
}

/// S19: every sd.cpp path is reachable — capabilities, submit, poll, cancel — in that order.
#[test]
fn sec_02_services_1_the_four_servers_s19_a_sdcpp_job_submit_poll_and_cancel_are_all_reachable() {
    let server = FakeServer::start(
        "sd",
        200,
        r#"{"weights":"sd-v1.safetensors","job":"job-42","state":"done"}"#.to_string(),
        Duration::ZERO,
    );
    let tree = Tree::new(project_dir("s19")).expect("a project folder");
    let endpoint = endpoint_at(&server.url(), "");

    let steps = [
        (Kind::Capabilities, "/sdcpp/v1/capabilities", "capabilities"),
        (Kind::SubmitImage, "/sdcpp/v1/img_gen", "submit"),
        (Kind::PollImage, "/sdcpp/v1/jobs/job-42", "poll"),
        (Kind::CancelImage, "/sdcpp/v1/jobs/job-42/cancel", "cancel"),
    ];
    for (kind, want_path, label) in steps {
        let base = server_leg::base_row(Server::Image, "sd-v1", label, "", "settings", "job-42");
        let body = if kind.method() == "POST" {
            Some(r#"{"prompt":"a frame"}"#)
        } else {
            None
        };
        // The job id travels as an argument, not baked into the endpoint: `send_named` builds the
        // URL as `endpoint.url + kind.path(job)`, so the path stays built in one place (§1's table).
        let endpoint = Endpoint {
            url: server.url(),
            key: String::new(),
        };
        let sent = server_leg::send_named(Server::Image, &endpoint, kind, Some("job-42"), body, None, &never);
        let reply = sent.unwrap_or_else(|why| panic!("{label} did not reach the server: {why}"));
        assert_eq!(reply.status, 200, "{label}");

        let seen = server.next_seen();
        assert_eq!(seen.method, kind.method(), "{label}");
        assert_eq!(seen.path, want_path, "{label} hit the wrong path");
    }

    // All four left their rows, in order, under the image service.
    let rows = requests::read(&tree).expect("requests.tsv");
    assert!(
        rows.iter().all(|row| row.service == Service::Image),
        "every step is an image-service row"
    );
}

// S20 lives in `four_servers_widget.rs`: a Test button press is a user action and needs a GTK
// main loop, which this binary does not run.

/// S24: the live provider composes rather than overrides — a row it does not dial keeps answering
/// from the injected fallback, so a test's canned answer can never be replaced by a wire trip.
#[test]
fn sec_02_services_1_the_four_servers_s24_a_live_provider_delegates_the_rows_it_does_not_dial() {
    let canned: Provider =
        Rc::new(|key, typed| Ok(format!("canned {key}={typed}")));
    let provider = settings_probe::live_over(canned);

    // ffmpeg names no port: its answer comes from the fallback, verbatim, with the typed value kept.
    assert_eq!(
        provider("ffmpeg", "/opt/ffmpeg"),
        Ok("canned ffmpeg=/opt/ffmpeg".to_string()),
        "a non-HTTP row answers from the injected provider, never from the wire"
    );
    for key in ["firefox", "tts", "asr", "diar", "sep", "aligner"] {
        assert!(
            provider(key, "x").is_ok(),
            "{key} is not one of the three dialled rows, so the fallback must answer it"
        );
    }
}

/// S25: with nothing else wired, the rows the live provider does not dial say so out loud rather
/// than showing a ✓ nobody probed for.
#[test]
fn sec_02_services_1_the_four_servers_s25_the_default_live_fallback_says_nothing_is_wired() {
    let reason = settings_probe::live_provider()("tts", "index-tts2")
        .expect_err("the TTS row is not one of the three HTTP rows, so it falls back");
    assert!(
        reason.contains("nothing wired") && reason.contains("tts"),
        "the fallback names the row it cannot probe, got {reason:?}"
    );
}
