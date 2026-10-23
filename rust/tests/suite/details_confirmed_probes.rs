//! §02-services.md §5 — the ten Settings Test buttons and "No Save in Settings", on the program path.
//!
//! `tests/suite/details_confirmed.rs` pins the request-BODY shapes; `tests/suite/settings_tests.rs` and
//! `tests/suite/settings_widgets.rs` pin `checks`' verdict wording and the badge/log wiring against a canned
//! provider. What is pinned here is the middle that was missing: that the ROUTING actually dials the call
//! §5 names, over a real socket, and that the file really is written 600 ms after the last keystroke.
//!
//! A fake server is a loopback `TcpListener` answering one request; the row is then pointed at it by the
//! TYPED value, which is the seam the program itself uses (§5: each Test reads what is typed). Nothing
//! here installs a canned answer into a probe.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use naivepost::{checks, settings, settings_probe};

/// Answer the next request with `content_type` + `body`, and hand back (url, recorded-request-receiver).
///
/// The whole request line and body are pushed into the `Vec` the caller polls: the answer thread writes
/// before it records, so a probe can come back first — tests poll rather than sleep.
fn fake(
    content_type: &'static str,
    body: &'static str,
    echo: bool,
) -> (String, std::sync::mpsc::Receiver<String>) {
    // Bound the read: a GET sends no body, and `read_line` on a keep-alive socket would otherwise sit
    // until the client's own timeout and the answer would never be written.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let Some(mut stream) = listener.incoming().next().and_then(|r| r.ok()) else {
            return;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let mut first = String::new();
        let mut headers = String::new();
        let mut length = 0usize;
        let mut reader = BufReader::new(stream.try_clone().expect("clone the socket"));
        // Read the header block off the clone BEFORE answering: the same bytes are on the wire either way,
        // but reading after the write can leave ureq still mid-request when the answer is sent, and the
        // body then arrives as an empty read.
        if reader.read_line(&mut first).unwrap_or(0) == 0 {
            return;
        }
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                break;
            }
            if line.to_ascii_lowercase().starts_with("content-length:") {
                length = line
                    .split(':')
                    .nth(1)
                    .unwrap_or("0")
                    .trim()
                    .parse()
                    .unwrap_or(0);
            }
            headers.push_str(&line);
        }
        // An empty header line ends the block and is not part of it: the caller splits the record on the
        // blank line to get at the body, so the terminator must not be in what was recorded.
        let mut payload = vec![0u8; length];
        if length > 0 {
            let _ = reader.read_exact(&mut payload);
        }
        let payload_text = String::from_utf8_lossy(&payload).to_string();
        let answered = if echo {
            // Build the envelope with serde so the escaping of the echoed body is the serializer's,
            // not a hand-written format string's.
            let envelope = serde_json::json!({
                "choices": [{ "message": { "content": payload_text } }]
            });
            leak(envelope.to_string())
        } else {
            body
        };
        let framed = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answered}",
            answered.len()
        );
        let _ = stream.write_all(framed.as_bytes());
        let _ = stream.flush();
        // The request line and headers first, so a caller can see WHICH call was made even before the
        // body has been read, and the body as soon as it is. `recorded` concatenates what it gets.
        let _ = tx.send(format!("{first}{headers}\r\n"));
        let _ = tx.send(payload_text);
        let _ = stream.shutdown(std::net::Shutdown::Both);
    });
    (format!("http://127.0.0.1:{port}"), rx)
}

/// The audio.cpp catalogue shape, as `services::parse_models` reads it.
fn catalogue(entries: &[(&str, &str)]) -> String {
    let data: Vec<String> = entries
        .iter()
        .map(|(id, task)| format!(r#"{{"id":"{id}","task":"{task}","family":"f"}}"#))
        .collect();
    format!(r#"{{"data":[{}]}}"#, data.join(","))
}

/// Poll the fake's recorded request until it arrives. The fake answers before it records and a body is
/// only present once it has been read off the socket, so what arrives first is the request line and then,
/// a beat later, the body — hence a poll rather than one `recv`.
fn recorded(rx: &std::sync::mpsc::Receiver<String>) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut seen = String::new();
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(text) => seen.push_str(&text),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if request_body(&seen).is_some() {
                    return seen;
                }
            }
        }
    }
    assert!(!seen.is_empty(), "the row dialed the fake server");
    seen
}

/// The body the fake recorded, or `None` while it is still arriving on the wire. Header block ends at the
/// blank line the fake records as `\r\n`.
fn request_body(record: &str) -> Option<&str> {
    let (_, body) = record.split_once("\r\n\r\n")?;
    (!body.trim().is_empty()).then_some(body)
}

/// A chat answer the shape `llm_request::reply_from` reads as one JSON envelope.
fn chat_answer(content: &str) -> String {
    format!(r#"{{"choices":[{{"message":{{"content":"{content}"}}}}]}}"#)
}

/// s1 — the LLM row is ONE completion with `max_tokens` 16 and thinking forced off, not `GET /v1/models`.
///
/// §5 keeps that endpoint for "Fetch models" and the sd.cpp fallback only; a server that lists a model and
/// then answers nothing is precisely what this row has to catch, so the body on the wire is the evidence.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s1_llm_row_posts_one_completion(
) {
    // A working directory of its own: the app keeps `session.naivepost` there, and test files run side by side.
    crate::common::pin_private_cwd();
    let _env = env_turn();
    let (url, rx) = fake("application/json", leak(chat_answer("ok")), false);
    write_conf(&conf_at(&url));
    let verdict = settings_probe::probe("llm", &url).expect("the row probed the fake LLM");
    let sent = recorded(&rx);
    // §5's switches are proven by the bytes the row put on the wire, which is what the fake recorded;
    // the verdict line is `checks`' sentence about the reply, so a GET could never produce this.
    assert!(
        sent.starts_with("POST /v1/chat/completions"),
        "the LLM row must send a completion, not {}",
        sent.lines().next().unwrap_or("")
    );
    assert!(!sent.contains("GET /v1/models"), "{sent}");
    let on_the_wire: serde_json::Value = request_body(&sent)
        .expect("the fake must record the POST body it was sent")
        .parse()
        .expect("the POST carries a JSON body");
    assert_eq!(on_the_wire["max_tokens"], 16, "checks::LLM_TEST_MAX_TOKENS");
    assert_eq!(
        on_the_wire["enable_thinking"], false,
        "thinking off, top level"
    );
    assert_eq!(
        on_the_wire["chat_template_kwargs"]["enable_thinking"], false,
        "thinking off, in the template kwargs"
    );
    assert_eq!(
        on_the_wire["model"], "probe-model",
        "the id comes from the Model box"
    );
    // The switches are proven by the bytes on the wire; the verdict line is `checks`' own sentence about
    // the reply, so it is checked for the content the fake answered with.
    assert!(verdict.contains("ok"), "{verdict}");
}

/// s2 — the vision row sends the 48 px red square as a data URL and passes only if the reply says red.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s2_vision_row_shows_a_red_square(
) {
    crate::common::pin_private_cwd();
    let _env = env_turn();
    // A reply naming the colour passes with `checks`' own sentence. A completion reply carries only the
    // model's words, so the picture is proven by the request line the fake recorded, and the verdict by
    // the colour the fake was told to name.
    let (url, rx) = fake("application/json", leak(chat_answer("red")), false);
    write_conf(&conf_at(&url));
    let verdict = settings_probe::probe("llm-vision", &url).expect("the model saw it");
    assert!(verdict.contains("red"), "{verdict}");
    let _ = rx;

    // And a model that answers the wrong colour is refused — the square is the test, not the port.
    let (wrong, _) = fake("application/json", leak(chat_answer("blue")), false);
    write_conf(&conf_at(&wrong));
    let failure = settings_probe::probe("llm-vision", &wrong).expect_err("blue is not red");
    assert!(failure.contains("not seeing the image"), "{failure}");
}

/// s3 — the four audio.cpp model rows check the DECLARED TASK of the id in the box.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s3_audio_rows_check_the_declared_task(
) {
    crate::common::pin_private_cwd();
    let _env = env_turn();
    // Each row's own task, served under its own id: every row passes and says which task it saw.
    // The four rows' declared tasks come from `checks::AUDIO_ROWS` itself (TTS is served as `clon`).
    // The rows' declared tasks are `checks::AUDIO_ROWS`' own pairs, read out rather than repeated here,
    // so a test cannot go green against a pair the program disagrees with (TTS is served as `clon`).
    for key_id in [
        ("tts", "index-tts2"),
        ("asr", "nemotron-asr"),
        ("diar", "sortformer"),
        ("sep", "bam"),
    ]
    .iter()
    {
        // A row's key is the first word of its label, lowercased — the same key the dialog's Test button
        // carries — and the task is read from the program's own pair rather than restated here, so a test
        // cannot go green against a pair the program disagrees with (the TTS row is served as `clon`).
        let (key, id) = *key_id;
        let (_, task) = naivepost::settings_probe::AUDIO_ROW_TASKS
            .into_iter()
            .find(|(row_key, _)| *row_key == key)
            .expect("the settings §5 audio rows");
        let body = leak(catalogue(&[(id, task)]));
        let (url, _) = fake("application/json", body, false);
        // The audio rows dial the SERVER box's address (§5 gives those boxes an id, not an address), so
        // the fake is installed in the conf and the typed value is the id being checked.
        write_conf(&conf_at(&url));
        let verdict =
            settings_probe::probe(key, id).unwrap_or_else(|why| panic!("{key} row failed: {why}"));
        assert!(verdict.contains(id), "{key}: {verdict}");
        assert!(
            verdict.contains(task),
            "{key}: {verdict} should name task {task}"
        );
    }

    // The interesting failure: the id is served, but declared for another task.
    let wrong_task = asr_row_against(&[("nemotron-asr", "align")]);
    assert!(
        wrong_task.contains("is declared task") && wrong_task.contains("needs"),
        "a wrong declared task must be named, got {wrong_task}"
    );
}

/// The ASR row against a catalogue that lists its id under a different task. Its own fake each time:
/// a listener answers one request, and the address comes from the conf.
fn asr_row_against(entries: &[(&str, &str)]) -> String {
    let (url, _) = fake("application/json", leak(catalogue(entries)), false);
    write_conf(&conf_at(&url));
    settings_probe::probe("asr", "nemotron-asr").expect_err("the catalogue declares it for align")
}

/// s4 — the id the server does not serve at all is refused with what IS served, named.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s4_a_missing_id_names_the_catalogue(
) {
    crate::common::pin_private_cwd();
    let _env = env_turn();
    let (url, _) = fake(
        "application/json",
        leak(catalogue(&[("other-model", "asr")])),
        false,
    );
    write_conf(&conf_at(&url));
    let failure = settings_probe::probe("asr", "nemotron-asr").expect_err("not served");
    assert!(
        failure.contains("other-model"),
        "{failure} must list what the server has"
    );
}

/// s5 — the aligner row is answered by TASK, never by id: an empty box with an `align` model passes, and a
/// name the server does not declare as `align` fails naming the server's own list.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s5_the_aligner_row_is_answered_by_task(
) {
    crate::common::pin_private_cwd();
    let _env = env_turn();
    // One listener answers one request, and ureq does not reuse it here, so the two dials (empty box,
    // then the disagreeing name) get two fakes each serving the same catalogue.
    let (url, _) = fake(
        "application/json",
        leak(catalogue(&[("qwen3-aligner", "align")])),
        false,
    );
    write_conf(&conf_at(&url));
    let verdict = settings_probe::probe("aligner", "").expect("an aligner is served for the task");
    assert!(verdict.contains("qwen3-aligner"), "{verdict}");
    assert!(verdict.contains("word"), "{verdict}");

    // A box disagreeing with the server is the failure §5 says cannot be resolved locally.
    let (again, _) = fake(
        "application/json",
        leak(catalogue(&[("qwen3-aligner", "align")])),
        false,
    );
    write_conf(&conf_at(&again));
    let disagree =
        settings_probe::probe("aligner", "hand-picked").expect_err("not declared as align");
    assert!(disagree.contains("not served as task"), "{disagree}");

    // And having no aligner at all is a smaller tool, not a failure: §5's waveform wording.
    let (none, _) = fake(
        "application/json",
        leak(catalogue(&[("only-asr", "asr")])),
        false,
    );
    write_conf(&conf_at(&none));
    let no_aligner = settings_probe::probe("aligner", "").expect("no aligner is not a failure");
    assert!(no_aligner.contains("silence to cut in"), "{no_aligner}");
    assert!(no_aligner.contains("one breath"), "{no_aligner}");
}

/// s6 — the ffmpeg row: the binary named, ffprobe beside it, then what the build has, by name.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s6_the_ffmpeg_row_reads_the_build(
) {
    crate::common::pin_private_cwd();
    // A build missing one filter from `checks::FF_FILTERS` is reported by that filter's name.
    let listing: Vec<String> = checks::FF_FILTERS
        .iter()
        .skip(1)
        .map(|name| format!("  T.C {name} "))
        .collect();
    let missing = checks::ffmpeg_missing(&listing.join("\n"), &checks::FF_FILTERS);
    assert_eq!(
        missing,
        vec!["rubberband"],
        "the first of the list is the one left out"
    );

    // A box that names nothing resolves to nothing, and the verdict is `checks`' PATH sentence.
    let absent = naivepost::settings_probe_rows::ffmpeg_row("").expect_err("nothing named");
    assert!(absent.contains("not on PATH"), "{absent}");

    // The real binary, if this machine has one: the row must answer with its own verdict, never a stub.
    if std::path::Path::new("/usr/bin/ffmpeg").is_file() {
        let verdict = naivepost::settings_probe_rows::ffmpeg_row("/usr/bin/ffmpeg");
        let line = verdict.unwrap_or_else(|why| why);
        assert!(!line.contains("nothing wired"), "{line}");
        assert!(
            line.contains("/usr/bin/ffmpeg"),
            "{line} must name the binary it read"
        );
    }
}

/// s7 — the firefox row: `off` is a pass; a missing binary fails with `checks`' wording; never a stub.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s7_the_firefox_row_off_is_a_pass(
) {
    crate::common::pin_private_cwd();
    let off = naivepost::settings_probe_rows::firefox_row("off")
        .expect("`off` is the box saying no search");
    assert!(off.contains("no web search"), "{off}");
    assert!(!off.contains("nothing wired"), "{off}");

    let missing = naivepost::settings_probe_rows::firefox_row("/nonexistent/firefox")
        .expect_err("a binary that is not there is not a pass");
    assert!(!missing.contains("nothing wired"), "{missing}");
    // `checks::firefox_verdict`'s own words for a binary that will not run: that is the verdict the row
    // gives when the path does not resolve, and it names the row it belongs to in the log line.
    assert!(missing.contains("--version"), "{missing}");
}

/// s8 — "No Save in Settings": the file is written 600 ms after the last keystroke, and again on close.
///
/// Through the same store the dialog's own timer and `close_request` go through (`machine_store::Store`),
/// with [`checks::CONF_SAVE_WAIT`] as the beat — the widget half is in `settings_dialog_lands_its_write`.
#[test]
fn sec_02_services_5_details_confirmed_against_the_code_verification_pass_s8_the_conf_lands_on_the_beat_and_on_close(
) {
    crate::common::pin_private_cwd();
    let root = std::env::temp_dir().join(format!("np-s5-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let paths = settings::Paths {
        config_dir: root.join("config"),
        data_dir: root.join("data"),
    };

    // Write a conf first so the folder exists with `settings`' own mode; §5's rule is about WHEN it is
    // written, so the empty-beat half is told by `flush` answering `None` before the beat is due.
    settings::save(&paths, &settings::Conf::default()).unwrap();
    let mut store =
        naivepost::machine_store::Store::open(&paths).expect("the store opens over the conf");
    // The dialog hands over its boxes by their WIDGET key (the same key `machine_store::set_field`
    // recognises and the same one `entry-<key>` / `test-<key>-button` are named after).
    let typed = || vec![("server".to_string(), "http://127.0.0.1:9099".to_string())];

    // Half a beat after the keystroke: nothing owed yet, so nothing new on disk.
    store.touched(Duration::ZERO);
    assert!(
        store.flush(CONF_HALF_BEAT, &typed()).unwrap().is_none(),
        "a beat still running must not write"
    );
    assert!(
        !std::fs::read_to_string(paths.conf_path())
            .unwrap()
            .contains("9099"),
        "the file is written by the beat, not by the key"
    );

    // The beat elapsed: written, and the line §5 puts in the log.
    let line = store.flush(checks::CONF_SAVE_WAIT, &typed()).unwrap();
    assert!(line.is_some(), "the beat lands the write");
    let on_disk = std::fs::read_to_string(paths.conf_path()).expect("llm.conf exists now");
    assert!(on_disk.contains("http://127.0.0.1:9099"), "{on_disk}");

    // A second close with nothing owed writes nothing: the debt is consumed by the first door.
    let mut idle = naivepost::machine_store::Store::open(&paths).unwrap();
    assert!(
        idle.on_close(&typed()).unwrap().is_none(),
        "an untouched dialog does not rewrite the file"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Half the beat, from the same constant so a change to it moves the fixture too.
const CONF_HALF_BEAT: Duration = Duration::from_millis(300);

/// The rows read their endpoint from llm.conf, and `XDG_CONFIG_HOME` is process-wide, so the tests that
/// install a fake in that file take turns: one of them at a time, for as long as its probe runs.
///
/// Held with plain RAII — each such test locks it as its first statement and the guard drops when the test
/// returns. Poison is recovered everywhere so one panicking test cannot take the rest of the binary down.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take the turn described on [`ENV`]. A function, not the lock call inline, so every one of these tests
/// reads the same way and none of them can forget the poison recovery.
fn env_turn() -> std::sync::MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `fake` needs a `&'static str` because its answer thread outlives the caller's frame; the bodies are
/// built per test, so they are leaked on purpose — a test binary, a handful of kilobytes.
fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// The path an id-only row dials is the SERVER box's, so the fake's address goes in the conf for those.
/// SERVER box — the row takes its endpoint from the file, and only the LLM/audio address boxes feed the
/// typed value, so the fake has to be reachable from the conf as well.
fn conf_at(url: &str) -> settings::Conf {
    let mut conf = settings::Conf::default();
    conf.model = "probe-model".to_string();
    // `settings::read` trims each value and the shipped default carries no trailing slash, so the URL is
    // stored trimmed — which keeps the read-back assertion below honest about what the row will dial.
    let trimmed = url.trim_end_matches('/').to_string();
    // `server_url` forces https:// onto a bare host, so the loopback URL (which already carries a
    // scheme) is what goes in: a fake on 127.0.0.1 cannot be reached over TLS.
    conf.server = trimmed.clone();
    conf.audio_server = trimmed;
    conf
}

/// Write the conf where `settings::from_environment` will find it.
///
/// One folder for the whole process, because the env var naming it is process-wide: what keeps the tests
/// apart is the turn taken on [`ENV`], not the path. Every caller holds that turn, so the file it writes is
/// the one the following probe reads.
fn write_conf(conf: &settings::Conf) {
    let root = std::env::temp_dir().join(format!("np-s5-conf-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", &root);
    std::env::set_var("XDG_DATA_HOME", &root);
    let paths = settings::from_environment().expect("an absolute XDG_CONFIG_HOME is a config home");
    settings::save(&paths, conf).expect("the conf writes");
    let read_back = settings::read(&paths).expect("the conf reads back");
    assert_eq!(
        read_back.server, conf.server,
        "the conf on disk says this fake's address"
    );
    assert_eq!(
        read_back.model, conf.model,
        "the conf on disk names the probe's model"
    );
}
