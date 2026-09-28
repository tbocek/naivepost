//! §02-services#1-the-four-servers, through the real Test buttons.
//!
//! `four_servers_wire.rs` puts a socket on the wire and asserts what a fake server saw. This file
//! closes the last gap: a real click on a real row's Test button, with a real server answering on
//! the loopback address typed into that row's box. The provider is
//! [`naivepost::settings_probe::live_provider`] — the one a real session runs on — not a canned
//! answer, so the click travels the whole route: typed box → endpoint → `server_leg` → socket →
//! `checks` verdict → badge tooltip → both logs.
//!
//! Only the three HTTP rows can take a live answer (the ffmpeg, firefox, TTS and per-model rows
//! probe a binary or a box that names no port), so this checks `llm`, `audio` and `sd`. Each gets
//! its OWN fake server, because a live probe reads the address in the box: pointing three rows at
//! one listener would prove only that the last-typed address works.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::settings_probe;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle};

/// A minimal answering server on a free loopback port, replying per path.
///
/// One answer per path rather than one canned body for everything, because the audio row makes TWO
/// calls (`/health` then `/v1/models`) and they must not be the same reply: a health answer that
/// looked like a model listing would hide a server that answers one and not the other.
struct Live {
    port: u16,
    hits: mpsc::Receiver<String>,
}

impl Live {
    fn start(routes: &[(&str, &str)]) -> Live {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, hits) = mpsc::channel();
        let table: Vec<(String, String)> = routes
            .iter()
            .map(|(path, body)| (path.to_string(), body.to_string()))
            .collect();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let sender = sender.clone();
                let table = table.clone();
                std::thread::spawn(move || {
                    if let Some(path) = answer(stream, &table) {
                        let _ = sender.send(path);
                    }
                });
            }
        });
        Live { port, hits }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The paths this server was asked for, in order, waiting briefly for each.
    fn asked(&self, want: usize) -> Vec<String> {
        let mut got = Vec::new();
        for _ in 0..want {
            match self.hits.recv_timeout(Duration::from_secs(3)) {
                Ok(path) => got.push(path),
                Err(_) => break,
            }
        }
        got
    }
}

/// Serve one request from the path table; report the path that was asked for.
fn answer(stream: TcpStream, table: &[(String, String)]) -> Option<String> {
    let peer = stream.try_clone().ok()?;
    let mut reader = BufReader::new(peer);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
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
            if name.eq_ignore_ascii_case("content-length") {
                declared = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut raw = vec![0u8; declared];
    if declared > 0 {
        let _ = reader.read_exact(&mut raw);
    }
    // An unknown path answers 404: a row that asks for something off-table fails loudly rather
    // than being handed another path's healthy reply.
    let known = table.iter().any(|(known, _)| *known == path);
    let body = table
        .iter()
        .find(|(known, _)| *known == path)
        .map(|(_, body)| body.clone())
        .unwrap_or_else(|| r#"{"error":"no such route"}"#.to_string());
    let status = if known { 200 } else { 404 };
    let reply = format!(
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {len}\r\nconnection: close\r\n\r\n{body}",
        len = body.len()
    );
    let mut writer = stream;
    let _ = writer.write_all(reply.as_bytes());
    let _ = writer.flush();
    Some(path)
}

/// One running GTK application for this test binary: the three checks share it for the same
/// reason `tests/settings_widgets.rs` shares one — GTK's main loop may only be started once.
fn round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
            let window = ui::build_window(app, &model, "Prepare");
            window.present();
            settle();

            // The live provider, exactly as a real session installs it: every row's probe goes out
            // over HTTP to the address in that row's box.
            ui::open_settings(&window, settings_probe::live_provider());
            assert!(ui::dialog_open(&window), "the settings dialog is open");

            check_llm_row_answers_live(&window);
            RAN_LLM.store(true, Ordering::SeqCst);
            check_audio_row_needs_health_and_catalogue(&window);
            RAN_AUDIO.store(true, Ordering::SeqCst);
            check_sd_row_asks_capabilities(&window);
            RAN_SD.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_LLM: AtomicBool = AtomicBool::new(false);
static RAN_AUDIO: AtomicBool = AtomicBool::new(false);
static RAN_SD: AtomicBool = AtomicBool::new(false);

fn type_and_test(window: &adw::ApplicationWindow, key: &str, url: &str) {
    ui::entry(window, key)
        .unwrap_or_else(|| panic!("the {key} row has its value box"))
        .set_text(url);
    ui::test_button(window, key)
        .unwrap_or_else(|| panic!("the {key} row has a Test button"))
        .emit_clicked();
    settle();
}

/// S21: a Test press on the LLM row asks the typed address over the wire and badges the verdict.
fn check_llm_row_answers_live(window: &adw::ApplicationWindow) {
    let server = Live::start(&[(
        "/v1/models",
        r#"{"data":[{"id":"qwen3-test"},{"id":"llama-vision"}]}"#,
    )]);

    type_and_test(window, "llm", &server.url());

    // The row really telephoned the address that was typed, at models' own path.
    let asked = server.asked(1);
    assert_eq!(
        asked,
        vec!["/v1/models".to_string()],
        "the LLM row's Test asked the typed address's /v1/models"
    );

    let badge = ui::badge(window, "llm").expect("the llm row has a badge");
    assert_eq!(
        badge.text().as_str(),
        "\u{2713}",
        "an answering server marks the row ✓, got {:?}",
        badge.text()
    );
    let verdict = badge.tooltip_text().unwrap_or_default().to_string();
    assert!(
        verdict.contains("answered"),
        "the tooltip is the verdict and says it answered, got {verdict:?}"
    );
    // §03 §5: mirrored into the main log as "settings: …". The row's name in that line is "LLM",
    // not the widget key, so the check is on the prefix and the verdict's own word.
    let logged = ui::window_logs()
        .last()
        .cloned()
        .unwrap_or_default();
    assert!(
        logged.starts_with("settings:") && logged.contains("answered"),
        "the main log carries the mirrored line, got {logged:?}"
    );
}

/// S22: the audio row makes BOTH calls a healthy server must pass, over the wire.
fn check_audio_row_needs_health_and_catalogue(window: &adw::ApplicationWindow) {
    let server = Live::start(&[
        ("/health", r#"{"status":"ok"}"#),
        (
            "/v1/models",
            // The family spelling `checks::tts_endpoint_verdict` recognises is `index_tts2` (underscore).
            // A hyphen in the listing would read as "serves both -- none of them can clone a voice", so
            // this is the catalogue as audio.cpp really answers it.
            r#"{"data":[{"id":"nemotron-asr","family":"nemotron","task":"asr"},{"id":"index-tts2","family":"index_tts2","task":"clon"}]}"#,
        ),
    ]);

    type_and_test(window, "audio", &server.url());

    let asked = server.asked(2);
    assert_eq!(
        asked,
        vec!["/health".to_string(), "/v1/models".to_string()],
        "the audio row asks health first, then the catalogue: one without the other would \
         green-light a server that cannot narrate"
    );
    let badge = ui::badge(window, "audio").expect("the audio row has a badge");
    assert_eq!(
        badge.text().as_str(),
        "\u{2713}",
        "healthy AND serving a voice marks ✓, got {:?}",
        badge.text()
    );
    assert!(
        badge
            .tooltip_text()
            .unwrap_or_default()
            .contains("healthy in"),
        "the verdict keeps §5's wording, got {:?}",
        badge.tooltip_text()
    );
}

/// S23: the sd row asks sd-server's OWN capabilities endpoint, and an address that answers
/// nothing is called unreachable rather than quietly skipped.
fn check_sd_row_asks_capabilities(window: &adw::ApplicationWindow) {
    let server = Live::start(&[(
        "/sdcpp/v1/capabilities",
        r#"{"weights":"sd-v1.safetensors","modes":["txt2img","img2img"]}"#,
    )]);

    type_and_test(window, "sd", &server.url());

    let asked = server.asked(1);
    assert_eq!(
        asked,
        vec!["/sdcpp/v1/capabilities".to_string()],
        "the sd row probes sd-server's own API, not the OpenAI-shaped one"
    );
    let badge = ui::badge(window, "sd").expect("the sd row has a badge");
    assert_eq!(
        badge.text().as_str(),
        "\u{2713}",
        "weights loaded marks ✓, got {:?}",
        badge.text()
    );
    assert!(
        badge
            .tooltip_text()
            .unwrap_or_default()
            .contains("sd-v1.safetensors"),
        "the verdict names the weights the server reported, got {:?}",
        badge.tooltip_text()
    );

    // And the same press against a dead address fails with §5's "not reachable" story rather than
    // a silent pass: nothing is listening, so both probes fail and the row says so.
    let dead = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind then release");
        let port = listener.local_addr().expect("a local address").port();
        drop(listener);
        format!("http://127.0.0.1:{port}")
    };
    type_and_test(window, "sd", &dead);
    let badge = ui::badge(window, "sd").expect("the sd row still has its badge");
    assert_eq!(
        badge.text().as_str(),
        "\u{2717}",
        "a dead sd address marks ✗, never a leftover ✓, got {:?}",
        badge.text()
    );
}

#[test]
fn sec_02_services_1_the_four_servers_s21_the_llm_test_button_telephones_the_typed_address() {
    round();
    assert!(RAN_LLM.load(Ordering::SeqCst), "the llm row's live check ran");
}

#[test]
fn sec_02_services_1_the_four_servers_s22_the_audio_test_button_makes_both_calls_it_proves() {
    round();
    assert!(
        RAN_AUDIO.load(Ordering::SeqCst),
        "the audio row's live check ran"
    );
}

#[test]
fn sec_02_services_1_the_four_servers_s23_the_sd_test_button_asks_capabilities_or_says_dead() {
    round();
    assert!(RAN_SD.load(Ordering::SeqCst), "the sd row's live check ran");
}
