//! F1.1 S5 over a socket: the ▶ Prepare preflight dials audio.cpp for real.
//!
//! Item id: `f1_1`. §S5 asks the run to check the audio server and that its models are served BEFORE
//! it starts anything, so what these cases assert is what a press would be told when the server is not
//! there or does not serve what Prepare needs — through [`naivepost::prepare_run::preflight`], which
//! makes both calls itself (`GET /health`, then `GET /v1/models`). Nothing here hands the door a
//! boolean: the fake servers answer on loopback and the refusal has to come out of what they said.
//!
//! One scenario per binary: `settings::from_environment` reads `XDG_CONFIG_HOME`, which belongs to the
//! whole process and cargo runs these as threads of one.

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use naivepost::layout::Tree;
use naivepost::prepare_run;
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf, Paths};

/// The catalogue a healthy-enough server answers with: the shipped ASR id declared `asr`, and nothing
/// else — so diarization is the first thing missing, which is the order §S5 names.
const ASR_ONLY: &str = r#"{"data":[{"id":"nemotron-asr","family":"qwen3","task":"asr"}]}"#;

/// Every model Prepare needs, each declared for its own task.
const ALL_SERVED: &str = r#"{"data":[
  {"id":"nemotron-asr","family":"qwen3","task":"asr"},
  {"id":"sortformer-diar","family":"qwen3","task":"diar"},
  {"id":"bs-roformer","family":"bs","task":"sep"}]}"#;

/// ASR and diarization both served, separation not: the case where the tick is the whole question.
const ASR_ONLY_DUAL: &str = r#"{"data":[
  {"id":"nemotron-asr","family":"qwen3","task":"asr"},
  {"id":"sortformer-diar","family":"qwen3","task":"diar"}]}"#;

struct Dirs {
    #[allow(dead_code)] // the folder is what keeps each scenario's settings apart; nothing reads the path back
    root: PathBuf,
    paths: Paths,
}

impl Dirs {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-f11-s5-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the temp folder");
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        Dirs { root, paths }
    }

    /// Point the conf's audio address at `url`. This is the seam §1 names ("the settings file is
    /// re-read per request"), and the same one every other audio test uses.
    fn point_audio_at(&self, url: &str) {
        let mut conf = Conf::default();
        conf.audio_server = url.to_string();
        settings::save(&self.paths, &conf).expect("the conf is written");
    }
}

/// A project with one footage source: enough for S1 to pass, so a refusal below can only be S5's.
fn project() -> Project {
    Project {
        sources: vec![Source {
            path: "project:sources/lecture.mkv".to_string(),
            footage: true,
            narrator: 1,
            sepvoice: false,
            tracks: vec![],
        }],
        ..Project::default()
    }
}

/// Read one request far enough to answer it, and answer `/health` and `/v1/models` as asked.
fn handle(mut stream: TcpStream, health: u16, models: &str) {
    let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone the socket"));
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let request_line = line.clone();
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
    let (status, payload) = if request_line.contains("/v1/models") {
        (200u16, models.to_string())
    } else if request_line.contains("/health") {
        (health, "{}".to_string())
    } else {
        (404, "{}".to_string())
    };
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let reply = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(reply.as_bytes());
    let _ = stream.flush();
}

/// A fake audio.cpp on loopback: `/health` answers `health`, `/v1/models` answers `models`.
fn server(health: u16, models: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            std::thread::spawn(move || handle(stream, health, models));
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// A port nothing listens on: every dial to it is refused.
fn dead_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port to hand out");
    let port = listener.local_addr().expect("a local address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// S5: a server that answers nothing refuses with §S5's first sentence — the server itself, because
/// nothing else can be known while it is down.
#[test]
fn f1_1_s5_a_dead_audio_server_refuses_the_press() {
    let dirs = Dirs::new("dead");
    let url = dead_url();
    dirs.point_audio_at(&url);
    let tree: Option<Tree> = None;
    let why = prepare_run::preflight(&dirs.paths, &tree, &project())
        .expect("a server that answers nothing must refuse the run");
    assert_eq!(
        why,
        prepare_run::preflight_error(false, false, false, false, false).expect("the same sentence"),
        "the refusal is `preflight_error`'s wording, not a second sentence invented here"
    );
    assert!(
        why.contains("audio.cpp") && why.contains("not answering"),
        "it names the server that is down: {why}"
    );
    // The dial went to THIS address: a healthy server on another port would have answered, so the
    // refusal below is about the address the conf names rather than about a default.
    assert!(
        url.starts_with("http://127.0.0.1:"),
        "the conf's address is the loopback port being asked: {url}"
    );
}

/// S5: healthy, but serving only the ASR model — the next thing §S5 checks is diarization, and that
/// is what the refusal names.
#[test]
fn f1_1_s5_a_server_without_the_diarization_model_is_refused_by_name() {
    let dirs = Dirs::new("no-diar");
    let url = server(200, ASR_ONLY);
    dirs.point_audio_at(&url);
    let tree: Option<Tree> = None;
    let why = prepare_run::preflight(&dirs.paths, &tree, &project())
        .expect("a server missing a model Prepare needs must refuse");
    assert_eq!(
        why,
        prepare_run::preflight_error(true, true, false, false, false).expect("the diarization sentence"),
        "the refusal is the one §S5's order puts third"
    );
    assert!(
        why.contains("diarization"),
        "and it says which check failed: {why}"
    );
}

/// S5: everything served answers `None` — the press may go through. Also the proof that a healthy
/// server with the right catalogue is NOT refused, which the two cases above cannot show alone.
#[test]
fn f1_1_s5_a_server_serving_everything_needed_does_not_refuse() {
    let dirs = Dirs::new("served");
    let url = server(200, ALL_SERVED);
    dirs.point_audio_at(&url);
    let tree: Option<Tree> = None;
    assert_eq!(
        prepare_run::preflight(&dirs.paths, &tree, &project()),
        None,
        "ASR + diarization served and no row asking for separation"
    );
}

/// S5: separation is checked only when a row carries 🗣 — and when one does, a server without the
/// separation model is refused by name.
#[test]
fn f1_1_s5_separation_is_only_asked_of_a_row_that_ticked_it() {
    let dirs = Dirs::new("sep");
    let url = server(200, ASR_ONLY_DUAL);
    dirs.point_audio_at(&url);
    let tree: Option<Tree> = None;
    // Same server, same models: the difference is the tick.
    let mut split = project();
    split.sources[0].sepvoice = true;
    assert!(
        prepare_run::separation_asked(&split),
        "a footage row with \u{1f5e3} asks for separation"
    );
    let why = prepare_run::preflight(&dirs.paths, &tree, &split)
        .expect("the row asked for a model this server does not serve");
    assert!(
        why.contains("separation"),
        "the refusal says separation was the missing piece: {why}"
    );
    assert_eq!(
        prepare_run::preflight(&dirs.paths, &tree, &project()),
        None,
        "without the tick the same server passes (§S5: 'separation when asked')"
    );
}
