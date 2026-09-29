#![allow(dead_code)]

//! F4.4's live half over a socket: the harness the one-scenario-per-binary tests share.
//!
//! `tests/narrate_speak_tts.rs` proves S1–S6 with both network steps handed in as closures. This
//! proves the WIRE: `speak_leg` reads the audio.cpp address out of the Settings file, dials it,
//! uploads the reference, posts `/v1/audio/speech`, and files the bytes that really crossed the
//! socket at `narrate/tts/‹hash›.wav`. The seam is the address itself — the same box a person types
//! into Settings — so no canned reply stands where the program would telephone.
//!
//! `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory are process-wide, so each scenario
//! gets its own test BINARY (the sibling files), sharing this harness by `#[path]` include.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use naivepost::bodies::ServerPath;
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::settings::{self, Conf, Paths};

/// What one request looked like from the server's side.
pub struct Seen {
    pub method: String,
    pub path: String,
    /// Every header, lower-cased key mapped to its value — so `x-audiocpp-filename` is checkable
    /// without caring how the client spelled it.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

/// One answer the fake gives: status plus the bytes to send back.
#[derive(Clone)]
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json(status: u16, body: &str) -> Reply {
        Reply { status, body: body.as_bytes().to_vec() }
    }
}

/// Per-path answers, in order per path: the first ask for a path spends its entry, later asks reuse
/// the last one (which is what a retry or a second line wants to see).
#[derive(Clone, Default)]
pub struct Script {
    health: Vec<Reply>,
    models: Vec<Reply>,
    upload: Vec<Reply>,
    speech: Vec<Reply>,
}

impl Script {
    /// A healthy server serving one model that can clone a voice.
    pub fn healthy() -> Script {
        Script {
            health: vec![Reply::json(200, r#"{"status":"ok"}"#)],
            models: vec![Reply::json(
                200,
                r#"{"data":[{"id":"index-tts2","family":"index_tts2","task":"clon"}]}"#,
            )],
            upload: vec![Reply::json(200, r#"{"path":"/tmp/uploaded/voice_ref.wav"}"#)],
            speech: vec![Reply { status: 200, body: wav_bytes(2400) }],
        }
    }

    pub fn health_with(mut self, replies: Vec<Reply>) -> Script {
        self.health = replies;
        self
    }

    pub fn models_with(mut self, replies: Vec<Reply>) -> Script {
        self.models = replies;
        self
    }

    pub fn upload_with(mut self, replies: Vec<Reply>) -> Script {
        self.upload = replies;
        self
    }

    pub fn speech_with(mut self, replies: Vec<Reply>) -> Script {
        self.speech = replies;
        self
    }
}

/// The fake audio.cpp: four endpoints, every request reported over a channel.
pub struct FakeAudio {
    port: u16,
    received: mpsc::Receiver<Seen>,
}

impl FakeAudio {
    pub fn start(script: Script) -> FakeAudio {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(script));
        let counters = Arc::new([
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        ]);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let sender = sender.clone();
                let script = script.clone();
                let counters = counters.clone();
                std::thread::spawn(move || {
                    if let Some(seen) = serve(stream, &script, &counters) {
                        let _ = sender.send(seen);
                    }
                });
            }
        });
        FakeAudio { port, received }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn next_seen(&self) -> Seen {
        self.received
            .recv_timeout(Duration::from_secs(25))
            .expect("the speak call reached the fake audio server")
    }

    /// Take every request that has arrived by now, waiting briefly for stragglers.
    pub fn drain(&self) -> Vec<Seen> {
        let mut got = Vec::new();
        loop {
            match self.received.recv_timeout(Duration::from_millis(900)) {
                Ok(seen) => got.push(seen),
                Err(_) => return got,
            }
        }
    }

    /// How many requests hit one endpoint.
    pub fn hits(&self, path: &str) -> usize {
        self.drain()
            .into_iter()
            .filter(|seen| seen.path == path)
            .count()
    }
}

fn take_at(list: &[Reply], index: &AtomicUsize) -> Reply {
    if list.is_empty() {
        return Reply::json(500, "nothing scripted for this endpoint");
    }
    let step = index.fetch_add(1, Ordering::SeqCst);
    list[step.min(list.len() - 1)].clone()
}

fn serve(
    mut stream: TcpStream,
    script: &Arc<Mutex<Script>>,
    counters: &Arc<[AtomicUsize; 4]>,
) -> Option<Seen> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut length = 0usize;
    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let path = parts.get(1).copied().unwrap_or_default().to_string();
    let seen = Seen {
        method: parts.first().copied().unwrap_or_default().to_string(),
        path: path.clone(),
        headers,
        body: payload,
    };

    // Which endpoint, which answer. The script is read under the lock so two lines asking at once
    // still spend their entries in order.
    let reply = {
        let guard = script.lock().ok()?;
        match path.as_str() {
            "/health" => take_at(&guard.health, &counters[0]),
            "/v1/models" => take_at(&guard.models, &counters[1]),
            "/v1/ui/upload" => take_at(&guard.upload, &counters[2]),
            "/v1/audio/speech" => take_at(&guard.speech, &counters[3]),
            _ => Reply::json(404, "no such endpoint on this fake"),
        }
    };
    let framed = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reason(reply.status),
        reply.body.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.write_all(&reply.body);
    let _ = stream.flush();
    Some(seen)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

// --- the session this scenario runs in --------------------------------------------------------

/// The scenario's folder, its settings, and the session tree the speak writes into.
pub struct Fixture {
    pub root: PathBuf,
    pub tree: Tree,
}

static FIXTURES: AtomicUsize = AtomicUsize::new(0);

/// Build the scenario under one temp root and point everything at it: settings, data, cwd, project.
/// ONE scenario per test binary — the env is process-wide.
pub fn fixture(tag: &str) -> Fixture {
    if FIXTURES.fetch_add(1, Ordering::SeqCst) > 0 {
        panic!("two fixtures in one test binary: the process-wide env races -- one scenario per binary");
    }
    let root = std::env::temp_dir().join(format!("naivepost-f44wire-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    // The address is deliberately NOT written here: `save` replaces the whole file, so an address set
    // before this call would be wiped. Every scenario sets it with `point_at` after building.
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let conf = Conf::default();
    settings::save(&paths, &conf).expect("settings written");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture { root, tree }
}

/// Rewrite the audio.cpp address in the settings file. This is the seam: the next leg dials what the
/// file says NOW (§02-services#1's re-read-per-request rule).
pub fn point_at(url: &str) {
    let paths = settings_folder();
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.audio_server = url.to_string();
    settings::save(&paths, &conf).expect("audio address written");
}

/// The TTS model id the settings name, so a row can assert which voice was dialled.
pub fn point_model_at(id: &str) {
    let paths = settings_folder();
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.tts_model = id.to_string();
    settings::save(&paths, &conf).expect("tts model written");
}

pub fn settings_folder() -> Paths {
    settings::from_environment().expect("settings folder")
}

/// Put the F4.6 reference where the page and the leg both look for it, and hand back its bytes.
pub fn write_reference(fixture: &Fixture, bytes: &[u8]) -> PathBuf {
    let file = fixture.tree.voice_ref_wav();
    std::fs::create_dir_all(file.parent().expect("narrate/ has a parent")).expect("narrate/ made");
    std::fs::write(&file, bytes).expect("reference written");
    file
}

/// A line to speak: one clip, words, and the emotion tag under test.
pub fn line(emotion: &str) -> Entry {
    Entry {
        s: 0.0,
        e: 10.0,
        at: 1.0,
        text: "The chain moved to proof of stake.".into(),
        emotion: emotion.into(),
        pos: String::new(),
        roll: 0,
    }
}

/// Speak one line through the program's OWN legs (`speak_leg::speak`): health asked, reference
/// uploaded, speech posted, take filed. Nothing injected. `tag` varies the words so two lines of the
/// same emotion get different keys and land on different takes.
pub fn speak_tagged(fixture: &Fixture, emotion: &str, language: &str, tag: &str) -> naivepost::narrate_tts::Outcome {
    let entry = Entry {
        text: format!("The chain moved to proof of stake. ({tag})"),
        ..line(emotion)
    };
    let key = naivepost::narration::tts_key(&entry, Some("narrator1"), None);
    let seed = naivepost::narration::tts_seed(&key);
    naivepost::speak_leg::speak(
        &fixture.tree,
        &entry.text,
        &entry.emotion,
        seed,
        &key,
        language,
    )
}

/// Speak one line with no tag suffix — the plain form every single-line scenario uses.
pub fn speak(fixture: &Fixture, emotion: &str, language: &str) -> naivepost::narrate_tts::Outcome {
    speak_tagged(fixture, emotion, language, "one")
}

/// Where that line's take belongs, computed the way the leg computes it.
pub fn expected_take(fixture: &Fixture, emotion: &str) -> PathBuf {
    let entry = line(emotion);
    let key = naivepost::narration::tts_key(&entry, Some("narrator1"), None);
    naivepost::narrate_tts::take_path(&fixture.tree, &key)
}

/// Bytes big enough to pass for a wav: the size is what S5 checks, not the contents.
pub fn wav_bytes(len: usize) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    while bytes.len() < len {
        bytes.push(b'w');
    }
    bytes.truncate(len);
    bytes
}

/// The timed rows this scenario's `requests.tsv` holds, header off.
pub fn rows(fixture: &Fixture) -> Vec<String> {
    let path = fixture.tree.requests_tsv();
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.to_string())
        .collect()
}

/// Rows whose service column is `audio`.
pub fn audio_rows(fixture: &Fixture) -> Vec<String> {
    rows(fixture)
        .into_iter()
        .filter(|row| row.split('\t').nth(4) == Some("audio"))
        .collect()
}

/// The upload path this fake says every upload landed at, as the leg would turn it in.
pub const UPLOADED: &str = "/tmp/uploaded/voice_ref.wav";

/// The item these scenarios pin: F4.4 Speak a line (TTS), `spec/07-narrate.md`.
pub const ITEM: &str = "f4_4";

/// Every step id of this item, so a scenario can name which step it covers without restating the
/// prefix and a reader can grep one file for the whole set.
pub const STEPS: [&str; 7] = [
    "f4_4_s1",
    "f4_4_s2",
    "f4_4_s3",
    "f4_4_s4",
    "f4_4_s5",
    "f4_4_s6",
    "f4_4_s7",
];

/// Read a file the scenario wrote.
pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("expected a file at {path:?}"))
}

/// The step set, asserted by the widget binary so a reader of `speak_tts_wire_widgets.rs` sees the
/// whole item covered rather than one click. (Each step also has its own scenario binary.)
#[test]
fn f4_4_s1_to_s7_are_the_steps_this_harness_covers() {
    assert_eq!(
        STEPS,
        [
            "f4_4_s1", // no reference on disk -> refused before any dial
            "f4_4_s2", // health + a clonable model, or a named refusal
            "f4_4_s3", // the reference uploads itself for every line
            "f4_4_s4", // language, five keys, vector-vs-judge routing
            "f4_4_s5", // 200 and >= MIN_WAV_BYTES or no file
            "f4_4_s6", // unknown names go to the judge, never an error
            "f4_4_s7", // every call its own timed row
        ],
        "{ITEM}'s steps are S1-S6 plus the §09 §10 timing row"
    );
    assert_eq!(ITEM, "f4_4");
    let _ = uploaded_path();
}

/// The upload path the fake answers with, as the leg would turn it in.
pub fn uploaded_path() -> ServerPath {
    ServerPath::from_upload(UPLOADED).expect("an absolute upload path")
}
