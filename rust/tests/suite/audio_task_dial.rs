//! §02-services.md **§1 The four servers**, the audio.cpp half: `POST /v1/tasks/run`
//! `{"model","request"}` — the envelope every audio task rides (speech to text, alignment,
//! diarization, voice separation).
//!
//! Item id: `sec_02_services_1_the_four_servers`. These tests dial a loopback server that behaves
//! like audio.cpp and assert what went on the wire and what came back, through the door the program
//! dials (`crate::audio_task`), so the endpoint rules, the timed row and the install hint are all
//! proven on the real path rather than on a copy of it.
//!
//! S-numbers here follow the item's own sentences: **s1** the task run itself (the envelope on the
//! wire, the model id, the timed row), **s2** the missing model and its install hint, **s3** the
//! bearer rule. Timeout and env-precedence halves of §1 are pinned in `tests/services.rs` already.

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use naivepost::audio_task::{self, Ask};
use naivepost::bodies::{self, ServerPath};
use naivepost::layout::Tree;
use naivepost::settings::{self, Conf, Paths};
use naivepost::services;

use crate::common;

/// A per-test temp folder plus the `Paths` that point at it, so nothing here reads the real `$HOME`.
struct Dirs {
    root: PathBuf,
    paths: Paths,
}

impl Dirs {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-svcs1-{label}-{pid}-{n}",
            pid = std::process::id(),
            n = COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the temp folder");
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        Dirs { root, paths }
    }

    /// Write the conf, then point `AUDIOCPP_SERVER` at `url` — the seam §1 names ("the settings file
    /// is re-read per request"), and the same one every other audio test uses.
    fn point_audio_at(&self, url: &str) {
        let mut conf = Conf::default();
        conf.audio_server = url.to_string();
        settings::save(&self.paths, &conf).expect("the conf is written");
    }

    /// Point the conf's ffmpeg box at a stub script instead of a real tool, and hand back `(stub, log)`:
    /// the script writes its output file and appends its argv to `log`, one line per argument.
    ///
    /// This container has no ffmpeg and several tests assert that absence, so a door that spawns the
    /// configured tool must be provable without installing one. Settings holds an ffmpeg path (§ "What it
    /// needs": the box answers from whatever the user pointed at), so naming a stub here drives the same
    /// resolution the program uses rather than a substitute seam.
    fn ffmpeg_stub(&self) -> (PathBuf, PathBuf) {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).expect("the stub's folder");
        let stub = bin.join("ffmpeg");
        let log = bin.join("argv.log");
        common::write_script(
            &stub,
            format!(
                "#!/bin/sh\nlast=\"\"\nfor arg in \"$@\"; do last=\"$arg\"; printf '%s\\n' \"$arg\" >> '{log}'; done\nprintf 'x' > \"$last\"\n",
                log = log.display()
            ),
        )
        .expect("the stub is written");
        std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("the stub runs");
        // The conf already exists by the time a test asks for this (every test points its audio server
        // first), so the box is appended through the same parse/render pair the settings dialog uses —
        // writing a fresh `Conf` here would erase that endpoint.
        let mut conf = crate::suite::audio_task_dial::settings::read(&self.paths).unwrap_or_default();
        conf.ffmpeg = stub.to_string_lossy().into_owned();
        settings::save(&self.paths, &conf).expect("the conf is written");
        (stub, log)
    }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Held while any test resolves an endpoint, so the env pinning in `dial_here` is exclusive.
static ENV: Mutex<()> = Mutex::new(());

/// One recorded request: the request line and the headers that matter here.
#[derive(Debug, Clone)]
struct Seen {
    line: String,
    body: String,
    auth: Option<String>,
    filename: Option<String>,
    /// The `Host:` value the client sent, which is how a recorded request is traced back to the fake
    /// server that answered it (cargo runs these tests as threads of one process).
    host: String,
    /// The status this request was answered with. 200 everywhere except the refusal fakes, which set it
    /// after the fact because their answer is scripted per request.
    outcome: u16,
}

impl Seen {
    fn header(&self, name: &str) -> Option<String> {
        match name.to_ascii_lowercase().as_str() {
            "authorization" => self.auth.clone(),
            "x-audiocpp-filename" => self.filename.clone(),
            _ => None,
        }
    }
}

/// What every fake server has recorded, newest last. Each test's own server is the last one built,
/// but a request can land in any row while tests run in parallel, so callers search this list.
fn recorded() -> Vec<Seen> {
    SEEN.lock()
        .expect("the record lock")
        .iter()
        .flat_map(|row| row.iter().cloned())
        .collect()
}

/// The dial each test makes, with the env pinned under a process-wide lock.
///
/// `AUDIOCPP_SERVER` is read out of the environment by [`services::endpoint_for`] at dial time, and
/// the environment belongs to the whole process: cargo runs these tests as threads of one binary, so
/// without this no two tests may resolve an endpoint at the same moment. Every test points its own
/// conf at its own fake server (the seam §1 names) and dials through here.
fn dial_here(dirs: &Dirs, ask: Ask<'_>, model: &str, envelope: &serde_json::Value) -> Result<String, String> {
    let _guard = ENV.lock().expect("the env lock");
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    audio_task::dial(&None, &dirs.paths, ask, model, envelope)
}

/// [`wait_for_recorded`] scoped to ONE fake server, which is what makes it safe for a claim about a
/// header or a byte count.
///
/// `wait_for_recorded` searches every fake this binary has started, and cargo runs these tests as
/// threads of one process: two tests dialling the same model id can each see the other's request, so an
/// assertion about what THIS test sent (an absent bearer, say) reads whichever landed last. Filtering
/// by the port this test's own fake listens on attributes the request honestly.
fn wait_for_recorded_at(url: &str, needle: &str, in_body: &str) -> Seen {
    let deadline = std::time::Instant::now() + crate::common::WAIT_LIMIT;
    loop {
        if let Some(seen) = recorded_at(url)
            .into_iter()
            .rev()
            .find(|seen| seen.line.contains(needle) && seen.body.contains(in_body))
        {
            return seen;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no request to {needle} carrying {in_body:?} reached {url} within {:?}",
            crate::common::WAIT_LIMIT
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Every request every fake server in this binary has answered, grouped per server, newest last.
static SEEN: Mutex<Vec<Vec<Seen>>> = Mutex::new(Vec::new());

/// Read one request, answer it with `status`/`payload`, and describe what was read.
fn handle(stream: TcpStream, answer: &str, models: &str) -> Seen {
    handle_with(stream, 200, answer, models)
}

/// The upload's file-name header, read case-insensitively and stripped of everything but its value.
fn filename_of(headers: &[String]) -> Option<String> {
    headers.iter().find_map(|header| {
        let (name, value) = header.split_once(':')?;
        name.trim().eq_ignore_ascii_case("x-audiocpp-filename")
            .then(|| value.trim().to_string())
    })
}

/// [`handle`] at any status — the refusals need a 404 or a 500 on the same reading code. The model
/// list always answers 200 with the catalogue: a refusal about one model must still be able to quote
/// the list of the ones there are.
fn handle_with(stream: TcpStream, status: u16, answer: &str, models: &str) -> Seen {
    let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone the socket"));
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let request_line = line.trim_end().to_string();
    let mut length = 0usize;
    let mut headers: Vec<String> = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
        headers.push(header.trim_end().to_string());
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    // The model list gets the catalogue, so a missing-model refusal has a list to quote; everything
    // else (the task run, an upload) gets the answer the test asked for.
    let payload = if request_line.contains("/v1/models") {
        models
    } else {
        answer
    };
    let seen = answer_with(stream, &request_line, &headers, &body, payload, status);
    Seen { outcome: seen.outcome, ..seen }
}

/// Write `payload` back and describe the request that was read — the shared tail of every fake here, so
/// a reader that collected the headers itself records exactly what [`handle`] records.
fn answer_with(
    mut stream: TcpStream,
    request_line: &str,
    headers: &[String],
    body: &[u8],
    payload: &str,
    status: u16,
) -> Seen {
    let code = if request_line.contains("/v1/models") { 200 } else { status };
    let reason = if code == 200 { "OK" } else { "Not Found" };
    let framed = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Seen {
        line: request_line.to_string(),
        body: String::from_utf8_lossy(body).into_owned(),
        auth: headers.iter().find_map(|header| {
            let (name, value) = header.split_once(':')?;
            name.trim().eq_ignore_ascii_case("authorization")
                .then(|| value.trim().to_string())
        }),
        filename: filename_of(headers),
        outcome: code,
        host: headers
            .iter()
            .find_map(|header| {
                let (name, value) = header.split_once(':')?;
                name.trim().eq_ignore_ascii_case("host")
                    .then(|| value.trim().to_string())
            })
            .unwrap_or_default(),
    }
}

/// The plain 200 case of [`answer_with`].
fn answer_and_record(
    stream: TcpStream,
    request_line: &str,
    headers: &[String],
    body: &[u8],
    payload: &str,
) -> Seen {
    answer_with(stream, request_line, headers, body, payload, 200)
}

/// The port a fake server listens on, so a recorded request can be attributed to the test that made
/// it. Cargo runs these tests as threads of one process and every dial appends to `SEEN`, so a count
/// over all of it is another test's traffic too.
fn port_of(url: &str) -> String {
    url.rsplit(':').next().unwrap_or("").to_string()
}

/// Every request dialed to `url`'s port, newest last — this test's own traffic only.
fn recorded_at(url: &str) -> Vec<Seen> {
    let port = port_of(url);
    recorded()
        .into_iter()
        .filter(|seen| seen.host.ends_with(&port))
        .collect()
}

/// A server that answers a status other than 200, for the refusal tests. Its `/v1/models` still
/// answers the shipped-default catalogue — and answers it with 200 — because a missing-model refusal
/// has to be able to name what the server does serve, which is the whole point of the extra read.
fn server_refusing(status: u16, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    SEEN.lock().expect("the record lock").push(Vec::new());
    let slot = SEEN.lock().expect("the record lock").len() - 1;
    let payload = body.to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let payload = payload.clone();
            std::thread::spawn(move || {
                let seen = handle_with(stream, status, &payload, CATALOGUE);
                if let Some(row) = SEEN.lock().expect("the record lock").get_mut(slot) {
                    row.push(seen);
                }
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// A fake audio.cpp: answers every request with `answer`, remembering each one in order.
///
/// Answers `/v1/models` with the shipped-default catalogue so a missing-model refusal can name what
/// the server does serve, and everything else with `answer`. Serves until the test binary ends; one
/// thread per connection, so a dial and its follow-up catalogue read both land.
fn server_answering(answer: &str, models: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    SEEN.lock().expect("the record lock").push(Vec::new());
    let slot = SEEN.lock().expect("the record lock").len() - 1;
    let payload = answer.to_string();
    let catalogue = models.to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let payload = payload.clone();
            let catalogue = catalogue.clone();
            std::thread::spawn(move || {
                let seen = handle(stream, &payload, &catalogue);
                if let Some(row) = SEEN.lock().expect("the record lock").get_mut(slot) {
                    row.push(seen);
                }
            });
        }
    });
    SEEN.lock().expect("the record lock").push(Vec::new());
    format!("http://127.0.0.1:{port}")
}

/// A server that answers an upload with the path it wrote, and every task run with `answer`.
///
/// The one fake a door that uploads before it asks needs: §1 makes the upload's answer the only path a
/// task may name, so a fake that replies the same body to both leaves the upload "without naming where
/// it put it".
fn server_answering_uploads(upload: &'static str, answer: &str, models: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    SEEN.lock().expect("the record lock").push(Vec::new());
    let slot = SEEN.lock().expect("the record lock").len() - 1;
    let task = answer.to_string();
    let catalogue = models.to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let task = task.clone();
            let catalogue = catalogue.clone();
            std::thread::spawn(move || {
                let seen = handle_upload_or_task(stream, upload, &task, &catalogue);
                if let Some(row) = SEEN.lock().expect("the record lock").get_mut(slot) {
                    row.push(seen);
                }
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// [`handle`] with a separate body for the upload: the request line decides which one goes back.
///
/// The headers and the raw body are read HERE, on this reader, and handed to [`answer_and_record`]: a
/// second `BufReader` over a clone of the socket would re-read from the kernel buffer that this one has
/// already drained, and the door would wait forever for an answer that was never written.
fn handle_upload_or_task(stream: TcpStream, upload: &'static str, answer: &str, models: &str) -> Seen {
    let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone the socket"));
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let request_line = line.trim_end().to_string();
    let mut length = 0usize;
    let mut headers: Vec<String> = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
        headers.push(header.trim_end().to_string());
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let payload = if request_line.contains("/v1/ui/upload") {
        upload
    } else if request_line.contains("/v1/models") {
        models
    } else {
        answer
    };
    answer_and_record(stream, &request_line, &headers, &body, payload)
}

/// The catalogue a fake server with the shipped defaults answers with.
const CATALOGUE: &str = r#"{"data":[{"id":"nemotron-asr","family":"qwen3","task":"asr"},{"id":"qwen3-aligner","family":"qwen3","task":"align"}]}"#;

/// `POST /v1/tasks/run` for speech to text: the envelope on the wire is exactly what §5 confirms.
#[test]
fn sec_02_services_1_the_four_servers_s1_a_task_run_is_posted_to_the_task_endpoint() {
    let dirs = Dirs::new("s1");
    let url = server_answering(r#"{"text":"hello there","words":[]}"#, CATALOGUE);
    dirs.point_audio_at(&url);
    let audio = ServerPath::from_upload("/uploads/c00.wav").expect("an absolute server path");
    let envelope = bodies::asr_body(services::ASR_MODEL, &audio, "en");

    let reply = dial_here(
        &dirs,
        Ask { step: "Prepare", job: "c00.wav", task: "asr" },
        services::ASR_MODEL,
        &envelope,
    )
    .expect("the task run answers");

    assert_eq!(reply, r#"{"text":"hello there","words":[]}"#);
    let run = wait_for_recorded_at(&url, "/v1/tasks/run", services::ASR_MODEL);
    assert!(run.line.starts_with("POST "), "a task run is a POST: {}", run.line);
    assert!(run.line.contains("/v1/tasks/run"), "the path §1 names: {}", run.line);
    let sent: serde_json::Value = serde_json::from_str(&run.body).expect("a JSON envelope");
    assert_eq!(sent["model"], services::ASR_MODEL, "the model id rides in the envelope");
    assert_eq!(sent["request"]["audio"], "/uploads/c00.wav");
    assert_eq!(sent["request"]["language"], "en");
    // No key box means no Authorization header at all — not an empty bearer (§1, pinned again here on
    // the task leg because the task leg is the one that carries real work). Scoped to this test's own
    // fake: another test dials the same model WITH a key, and an unscoped search would read that one.
    let run = wait_for_recorded_at(&url, "/v1/tasks/run", services::ASR_MODEL);
    assert_eq!(run.header("authorization"), None, "an empty key box sends no credential");
}

/// Diarization and alignment ride the same endpoint with their own requests, so one door covers the
/// three §1 tasks rather than three copies of a client.
#[test]
fn sec_02_services_1_the_four_servers_s1_diarization_and_alignment_ride_the_same_endpoint() {
    let dirs = Dirs::new("s1b");
    let url = server_answering(
        r#"{"turns":[{"start_sample":0,"end_sample":16000,"speaker_id":1}]}"#,
        CATALOGUE,
    );
    dirs.point_audio_at(&url);
    let audio = ServerPath::from_upload("/uploads/lecture.wav").expect("absolute");
    let ask = Ask { step: "Prepare", job: "lecture.wav", task: "diarize" };

    let body = dial_here(&dirs, ask, services::DIAR_MODEL, &bodies::diar_body(services::DIAR_MODEL, &audio))
        .expect("the diarizer answers");
    // The turns come back through the same reading `diar_ask` applies on its way into
    // `transcribe::run`, so what is checked is the reply this dial actually got.
    let turns = audio_task::diar_answer(
        &serde_json::from_str::<serde_json::Value>(&body).expect("JSON"),
    )
    .expect("the turns read");
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].speaker_id, 1);

    let align = Ask { step: "Prepare", job: "w000001500.wav", task: "align" };
    let envelope = bodies::align_body("qwen3-aligner", &audio, "hello there", "en");
    // The dial is what is under test here — that alignment rides the same endpoint with its own body —
    // so the reply itself is only checked to have arrived.
    let _body = dial_here(&dirs, align, "qwen3-aligner", &envelope).expect("the aligner answers");
    // The word times come out of the reply the way the alignment stage reads them — `align::run` calls
    // `parse_words` on whatever its `ask` handed back, so this is the same door's reading rather than a
    // second copy of it. This fake answers every task with the diarizer's turns, so what is proven here
    // is that an align reply is readable as words once the stage gets it.
    let words = naivepost::align::parse_words(
        &serde_json::from_str::<serde_json::Value>(r#"{"words":[{"word":"hello","start":0.0,"end":0.4}]}"#)
            .expect("JSON"),
    )
    .expect("word times read");
    assert_eq!(words.len(), 1);

    // The server records on its own thread, so the aligner's request can still be unwritten when the count is
    // taken: wait for it before reading the list, or the count reads 1 and this test fails at random.
    wait_for_recorded_at(&url, "/v1/tasks/run", "hello there");
    let all = recorded_at(&url);
    // Only this test's two envelopes, told apart by the text only the aligner's request carries and
    // the diarizer's own model id; other tests dial the same endpoint from other threads.
    let runs: Vec<&Seen> = all
        .iter()
        .filter(|seen| {
            seen.line.contains("/v1/tasks/run")
                && (seen.body.contains(services::DIAR_MODEL) || seen.body.contains("hello there"))
        })
        .collect();
    assert_eq!(runs.len(), 2, "both kinds were dialed, once each: {runs:#?}");
    assert!(
        runs.iter().any(|seen| seen.body.contains("qwen3-aligner") && seen.body.contains("hello there")),
        "the aligner's request carries the text to place"
    );
}

/// The timed row: §09 §10 asks for one line per request, and the door writes it through the same leg
/// every other request uses — including the model column, which is how a reader knows which voice or
/// which ASR was dialled.
#[test]
fn sec_02_services_1_the_four_servers_s1_the_task_run_leaves_its_row_in_requests_tsv() {
    let dirs = Dirs::new("s1c");
    let url = server_answering(r#"{"text":"hello there"}"#, CATALOGUE);
    dirs.point_audio_at(&url);
    let project = std::env::temp_dir().join(format!("np-svcs1-proj-{}.naivepost", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).expect("the project folder");
    let tree = Tree::new(&project).expect("a session tree");
    let audio = ServerPath::from_upload("/uploads/c00.wav").expect("absolute");

    let _guard = ENV.lock().expect("the env lock");
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    audio_task::dial(
        &Some(tree.clone()),
        &dirs.paths,
        Ask { step: "Prepare", job: "c00.wav", task: "asr" },
        services::ASR_MODEL,
        &bodies::asr_body(services::ASR_MODEL, &audio, "en"),
    )
    .expect("the task run answers");

    let rows = std::fs::read_to_string(tree.requests_tsv()).expect("requests.tsv was written");
    let row = rows.lines().last().expect("one row");
    let fields: Vec<&str> = row.split('\t').collect();
    assert_eq!(fields[4], "audio", "the service column names audio.cpp: {row}");
    assert_eq!(fields[5], services::ASR_MODEL, "the model column says which model: {row}");
    assert_eq!(fields[2], "Prepare", "the step column says who asked: {row}");
    assert_eq!(fields[3], "c00.wav", "the job column names the chunk: {row}");
    assert_eq!(fields[16], "ok", "the outcome is recorded: {row}");
}

/// §F1.3 S1/S3 on the door: a take longer than the model's ceiling is asked about in CHUNKS, each one
/// cut by ffmpeg from the decoded audio, uploaded under its own name, and dialled as its own task run.
///
/// The fake records every request, so what this pins is the count and the identity of the asks: two chunks
/// means two uploads and two `POST /v1/tasks/run`, each naming the chunk it heard — never one request for
/// the whole recording.
#[test]
fn sec_02_services_1_the_four_servers_s1_asr_is_asked_per_chunk_with_its_own_cut() {
    let dirs = Dirs::new("s1chunk");
    let url = server_answering_uploads(
        r#"{"path":"/uploads/c00.wav"}"#,
        r#"{"text":"hello there","words":[{"word":"hello","start":0.1,"end":0.4}]}"#,
        CATALOGUE,
    );
    dirs.point_audio_at(&url);
    // The cut runs the tool Settings names, so this test names a stub instead of needing ffmpeg installed:
    // it records its argv and writes its output file, which is all the door asks of the real tool.
    let (_stub, argv_log) = dirs.ffmpeg_stub();
    let project = std::env::temp_dir().join(format!("np-svcs1-chunk-{}.naivepost", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).expect("the project folder");
    let tree = Tree::new(&project).expect("a session tree");
    // The cut reads the decoded pcm the stage wrote first (§F1.3 S1). Its bytes do not matter to this
    // assertion — the stub never opens it, and what matters is that the door found an input to name.
    let voice = tree.voice16k_wav("lecture");
    std::fs::create_dir_all(voice.parent().expect("the input folder")).expect("the input folder");
    std::fs::write(&voice, b"x").expect("the decoded audio");
    let held = Some(tree.clone());

    // The env belongs to the process, so no other test may resolve an endpoint mid-dial.
    let _guard = ENV.lock().expect("the env lock");
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    // Two chunks' worth at the shipped ASR model's ceiling, and a scratch note per chunk standing in for
    // the one `asr::run` writes before it asks — the door reads its span from that note, so writing it here
    // is what makes this drive the real ask rather than a copy of it.
    let limit = naivepost::asr::chunk_limit(services::ASR_MODEL);
    let duration = 2.5 * limit;
    let edges = naivepost::asr::cut_points(duration, &[], limit, naivepost::asr::SEEK_MAX);
    for index in 0..edges.len() - 1 {
        let file = naivepost::asr::part(index);
        let plan = naivepost::asr::cut_plan(edges[index], edges[index + 1]);
        audio_task::write_scratch_for_test(
            &held,
            "lecture",
            &file,
            &plan,
            &naivepost::asr::body(&file, "en"),
        )
        .expect("the scratch note");
    }

    let mut ask = audio_task::asr_ask(&held, &dirs.paths, "lecture", services::ASR_MODEL, "en");
    for index in 0..edges.len() - 1 {
        ask(&naivepost::asr::part(index)).expect("the chunk answers");
    }
    drop(_guard);

    // Only THIS test's traffic, and it is identified by its OWN bytes rather than by a port: cargo runs
    // these tests as threads of one process and every fake appends to one list, while a port number can be
    // handed back to another fake after one is dropped. The upload carries the stub's ONE-byte output AND
    // the name the door gave it — `c01.wav` belongs to no other test here.
    let mine = |seen: &Seen| {
        seen.line.contains("/v1/ui/upload")
            && seen.body == "x"
            && seen.filename.as_deref() == Some("c01.wav")
    };
    // The recording happens on the server's own thread, so a dial can return before its entry appears:
    // re-scan until it is in the record instead of sleeping a fixed time (`wait_for_recorded` below waits
    // the same way for a single request).
    let counted = std::time::Instant::now();
    while recorded().iter().filter(|seen| mine(seen)).count() < 1 {
        assert!(
            counted.elapsed() < crate::common::WAIT_LIMIT,
            "the second chunk never reached the fake upload endpoint on its own within {:?}",
            crate::common::WAIT_LIMIT
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let seen: Vec<Seen> = recorded().into_iter().filter(mine).collect();
    let uploads: Vec<&Seen> = seen.iter().collect();
    assert_eq!(uploads.len(), 1, "the second chunk went up on its own, not inside one whole-take ask");
    // Every chunk got its OWN ffmpeg cut, at its own span. The stub's log is this test's alone (its folder
    // name is unique), so the `-ss` values it recorded are exactly the cuts this door made — and they differ,
    // which is only possible if the take was split rather than asked about whole.
    let cuts = std::fs::read_to_string(&argv_log).expect("the stub logged its calls");
    let lines: Vec<&str> = cuts.lines().collect();
    let starts: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(_, arg)| **arg == "-ss")
        .map(|(index, _)| lines[index + 1])
        .collect();
    assert!(
        starts.windows(2).all(|pair| pair[0] != pair[1]),
        "each chunk is cut from its own point: {starts:?}"
    );
    assert!(
        starts.len() >= 2,
        "a take longer than the model's ceiling is cut more than once: {starts:?}"
    );
    assert!(
        uploads.iter().all(|seen| seen.filename.is_some()),
        "each upload carries its own file name (§1's `X-Audiocpp-Filename`)"
    );
    let names: Vec<&str> = uploads
        .iter()
        .map(|seen| seen.filename.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(names, vec!["c01.wav"], "the upload names the chunk it carries");
}

/// A shipped default the server does not serve comes back with §1's install hint, and the door pays
/// for one extra model-list read to be able to say what the server does serve.
#[test]
fn sec_02_services_1_the_four_servers_s2_a_missing_shipped_model_carries_its_install_hint() {
    let dirs = Dirs::new("s2");
    // The catalogue serves an ASR but NOT the diarization default, so the diarizer's ask is missing.
    let url = server_refusing(404, r#"{"error":"model not found"}"#);
    dirs.point_audio_at(&url);
    let audio = ServerPath::from_upload("/uploads/lecture.wav").expect("absolute");

    let err = dial_here(
        &dirs,
        Ask { step: "Prepare", job: "lecture.wav", task: "diarize" },
        services::DIAR_MODEL,
        &bodies::diar_body(services::DIAR_MODEL, &audio),
    )
    .expect_err("a server without the model refuses the ask");

    assert!(err.contains("sortformer_diar_4spk_v1_q8_0"), "the shipped default names its package: {err}");
    assert!(
        err.contains("docker compose exec audio python3 tools/model_manager_v2.py install"),
        "the hint is §1's own wording: {err}"
    );
    assert!(err.contains("--models-root models"), "the hint ends as §1 spells it: {err}");
    assert!(err.contains("add it on that server"), "the refusal says what to do about it: {err}");
}

/// A hand-picked id gets no install hint: for an id this build does not ship, the weights would be a
/// guess, and a wrong install line is worse than none (§1).
#[test]
fn sec_02_services_1_the_four_servers_s2_a_hand_picked_model_is_named_without_a_hint() {
    let dirs = Dirs::new("s2b");
    let url = server_refusing(404, r#"{"error":"model not found"}"#);
    dirs.point_audio_at(&url);
    let audio = ServerPath::from_upload("/uploads/lecture.wav").expect("absolute");

    let err = dial_here(
        &dirs,
        Ask { step: "Prepare", job: "lecture.wav", task: "diarize" },
        "my-own-diarizer",
        &bodies::diar_body("my-own-diarizer", &audio),
    )
    .expect_err("a server without the model refuses the ask");

    assert!(err.contains("my-own-diarizer"), "the refusal names what was asked for: {err}");
    assert!(!err.contains("model_manager_v2.py"), "no install guess for a hand-picked id: {err}");
}

/// Any other failure names the model, the status and the server's own sentence, and is not dressed up
/// as a missing model (spec: failure is specific and local).
#[test]
fn sec_02_services_1_the_four_servers_s2_an_ordinary_failure_keeps_the_servers_own_words() {
    let dirs = Dirs::new("s2c");
    let url = server_refusing(500, "gpu is busy");
    dirs.point_audio_at(&url);
    let audio = ServerPath::from_upload("/uploads/c00.wav").expect("absolute");

    let err = dial_here(
        &dirs,
        Ask { step: "Prepare", job: "c00.wav", task: "asr" },
        services::ASR_MODEL,
        &bodies::asr_body(services::ASR_MODEL, &audio, "en"),
    )
    .expect_err("a 500 is a failure");

    assert!(err.contains("500"), "the status is in the sentence: {err}");
    assert!(err.contains("gpu is busy"), "so is the server's own reason: {err}");
    assert!(err.contains("nemotron-asr"), "and which model was dialled: {err}");
    assert!(!err.contains("model_manager_v2"), "an ordinary failure is not an install hint: {err}");
}

/// §1's install hint reaches a LOG, not only the caller's `Err`: the ▶ Prepare stage turns a failed audio
/// task into its `Drive::Failed` reason and logs that verbatim, so this asserts the sentence carrying the
/// install line is what the door hands over — a fine message a caller threw away would be invisible.
#[test]
fn sec_02_services_1_the_four_servers_s4_a_missing_model_reaches_the_stages_failure_reason() {
    let dirs = Dirs::new("s4");
    // The catalogue serves an ASR but not the diarization default, so the diarizer's ask is missing.
    let url = server_refusing(404, r#"{"error":"model not found"}"#);
    dirs.point_audio_at(&url);
    let audio = ServerPath::from_upload("/uploads/lecture.wav").expect("absolute");

    let err = dial_here(
        &dirs,
        Ask { step: "Prepare", job: "lecture.wav", task: "diarize" },
        services::DIAR_MODEL,
        &bodies::diar_body(services::DIAR_MODEL, &audio),
    )
    .expect_err("a server without the model refuses the ask");

    assert!(
        err.contains("the weights install with:"),
        "the refusal carries §1's install hint for the run to log: {err}"
    );
    assert!(
        err.contains(services::DIAR_MODEL),
        "it names the model that was missing, which is how a log line says where it failed: {err}"
    );
}

/// §1's bearer rule on the task leg: a key typed in the box becomes `Authorization: Bearer …`, and
/// only a key does (the same rule every leg obeys, proven here on the leg that does the work).
#[test]
fn sec_02_services_1_the_four_servers_s3_a_key_goes_as_a_bearer_and_nothing_else_goes() {
    let dirs = Dirs::new("s3");
    let url = server_answering(r#"{"text":"hello"}"#, CATALOGUE);
    let mut conf = Conf::default();
    conf.audio_server = url.clone();
    conf.audio_key = "  sk-audio  ".to_string();
    settings::save(&dirs.paths, &conf).expect("the conf");

    let audio = ServerPath::from_upload("/uploads/c00.wav").expect("absolute");
    dial_here(
        &dirs,
        Ask { step: "Prepare", job: "c00.wav", task: "asr" },
        services::ASR_MODEL,
        &bodies::asr_body(services::ASR_MODEL, &audio, "en"),
    )
    .expect("the task run answers");

    let last = wait_for_recorded_at(&url, "/v1/tasks/run", services::ASR_MODEL);
    assert_eq!(
        last.header("authorization").as_deref().map(|value| value.to_ascii_lowercase()),
        Some("bearer sk-audio".to_string()),
        "the key is trimmed into a bearer"
    );
}

/// The upload that must precede every task: raw body, the name in `X-Audiocpp-Filename`, and the
/// path the server answers with — which is the only path a task may name.
#[test]
fn sec_02_services_1_the_four_servers_s1_an_upload_carries_the_bytes_and_its_name() {
    let dirs = Dirs::new("s1d");
    let url = server_answering(r#"{"path":"/uploads/c00.wav"}"#, CATALOGUE);
    dirs.point_audio_at(&url);

    let path = audio_task::upload_bytes(&None, &dirs.paths, "c00.wav", b"fake wav bytes")
        .expect("the upload lands");
    assert_eq!(path.as_str(), "/uploads/c00.wav");

    let upload = wait_for_recorded_at(&url, "/v1/ui/upload", "fake wav bytes");

    assert!(upload.line.starts_with("POST "), "the upload is a POST: {}", upload.line);
    assert_eq!(
        upload.header("x-audiocpp-filename").as_deref(),
        Some("c00.wav"),
        "the name rides the header §1 names"
    );
    assert_eq!(upload.body, "fake wav bytes", "the body is the raw file, not a multipart envelope");
}

/// A server that refuses uploads answers with the fix rather than the status (§1's management-API
/// rule), on this door as on the narrate one.
#[test]
fn sec_02_services_1_the_four_servers_s1_an_upload_refusal_names_the_start_up_flag() {
    let dirs = Dirs::new("s1e");
    let url = server_refusing(403, "uploads need /models mounted");
    dirs.point_audio_at(&url);

    let err = audio_task::upload_bytes(&None, &dirs.paths, "c00.wav", b"x")
        .expect_err("a 403 is a refusal");
    assert!(err.contains("--ui-management"), "the refusal says what to change: {err}");
}

/// Separation's reply names two stems by server path; a reply missing one says which is missing rather
/// than writing a half-split row.
#[test]
fn sec_02_services_1_the_four_servers_s1_a_separation_reply_names_both_stems() {
    let both: serde_json::Value =
        serde_json::from_str(r#"{"voice":"/uploads/voice.wav","rest":"/uploads/rest.wav"}"#).expect("JSON");
    let (kind, voice, rest) = audio_task::stem_paths(&both).expect("both stems read");
    assert_eq!(kind, audio_task::Stems::Both);
    assert_eq!(voice.as_str(), "/uploads/voice.wav");
    assert_eq!(rest.as_str(), "/uploads/rest.wav");

    let one: serde_json::Value = serde_json::from_str(r#"{"voice":"/uploads/voice.wav"}"#).expect("JSON");
    let err = audio_task::stem_paths(&one).expect_err("a reply with one stem is not a split");
    assert!(err.contains("accompaniment"), "the refusal says which half is missing: {err}");
}
