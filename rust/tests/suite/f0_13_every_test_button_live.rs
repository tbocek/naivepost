//! F0.13 (spec/03-shell.md §5), the Test buttons the inventory found never pressed through the LIVE provider:
//! Decision, firefox, TTS, ASR, diarization, separation and the aligner, each clicked on its real button with
//! [`naivepost::settings_probe::live_provider`] (the one a real session runs on) against loopback fakes; Test All
//! with that same provider, every row answering; and each section's ⓘ opening its explanation.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::{settings_probe, ui};

/// A loopback server answering each path from a table (404 off it), on its own port.
fn serve(routes: Vec<(&'static str, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let routes = routes.clone();
            std::thread::spawn(move || answer(stream, &routes));
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn answer(stream: TcpStream, routes: &[(&'static str, String)]) {
    let Ok(peer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(peer);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() {
        return;
    }
    let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    let (status, reply) = routes
        .iter()
        .find(|(known, _)| *known == path)
        .map(|(_, reply)| (200, reply.clone()))
        .unwrap_or((404, r#"{"error":"no such route"}"#.to_string()));
    let mut writer = stream;
    let _ = writer.write_all(
        format!(
            "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
            reply.len()
        )
        .as_bytes(),
    );
}

/// The widget named `name` under `root`.
fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if let Some(found) = find_in(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

/// Type into a row's box, press its Test, wait for the verdict; answer the badge's mark and tooltip.
fn test_row(window: &adw::ApplicationWindow, key: &str, typed: &str) -> (String, String) {
    ui::entry(window, key)
        .unwrap_or_else(|| panic!("the {key} row has its box"))
        .set_text(typed);
    ui::test_button(window, key)
        .unwrap_or_else(|| panic!("the {key} row has a Test button"))
        .emit_clicked();
    assert!(
        naivepost::ui::settings::wait_for_tests(Duration::from_secs(30)),
        "the {key} Test answered"
    );
    common::settle();
    let badge = ui::badge(window, key).unwrap_or_else(|| panic!("the {key} row has a badge"));
    (
        badge.text().to_string(),
        badge.tooltip_text().unwrap_or_default().to_string(),
    )
}

#[test]
fn f0_13_every_test_button_answers_through_the_live_provider() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::pin_private_cwd();
    // The two servers the rows dial: the LLM answers the text gate's question through judge_text; audio.cpp is
    // healthy and serves one model per task, the aligner among them.
    let judged = serde_json::json!({ "choices": [ { "message": { "content": "", "tool_calls": [ {
        "id": "c1", "type": "function", "function": {
            "name": "judge_text", "arguments": "{\"changed\": false, \"what\": \"a red square\"}" } } ] } } ] })
    .to_string();
    let llm = serve(vec![("/v1/chat/completions", judged)]);
    let audio = serve(vec![
        ("/health", r#"{"status":"ok"}"#.to_string()),
        (
            "/v1/models",
            r#"{"data":[{"id":"index-tts2","family":"index_tts2","task":"clon"},{"id":"nemotron-asr","family":"nemotron","task":"asr"},{"id":"sortformer-diar","family":"sortformer","task":"diar"},{"id":"bs-roformer","family":"roformer","task":"sep"},{"id":"qwen3-aligner","family":"qwen3","task":"align"}]}"#.to_string(),
        ),
    ]);
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "probe-model".to_string();
    conf.server = llm.clone();
    conf.audio_server = audio.clone();
    naivepost::settings::save(&paths, &conf).expect("the conf writes");
    common::one_app_round(move |app| {
        let model = naivepost::project::load(&common::fixture_dir()).expect("fixture loads");
        let window = ui::build_window(app, &model, "Prepare");
        window.present();
        common::settle();
        ui::open_settings(&window, settings_probe::live_provider());
        assert!(ui::dialog_open(&window), "the dialog is open");

        // Each row through its own button.
        let (mark, said) = test_row(&window, "decision", "clef");
        assert_eq!(mark, "\u{2713}", "Decision: {said}");
        assert!(
            said.contains("judged two identical frames the same"),
            "{said}"
        );
        let (mark, said) = test_row(&window, "firefox", "off");
        assert_eq!(mark, "\u{2713}", "firefox off: {said}");
        assert!(said.contains("no web search"), "{said}");
        for (key, id) in [
            ("tts", "index-tts2"),
            ("asr", "nemotron-asr"),
            ("diar", "sortformer-diar"),
            ("sep", "bs-roformer"),
        ] {
            let (mark, said) = test_row(&window, key, id);
            assert_eq!(mark, "\u{2713}", "{key} serves {id} for its task: {said}");
        }
        let (mark, said) = test_row(&window, "aligner", "");
        assert_eq!(mark, "\u{2713}", "aligner: {said}");
        assert!(
            said.contains("qwen3-aligner"),
            "the served aligner is named: {said}"
        );

        // Test All with the live provider: every row answers (✓ or ✗), and each says so once in the log.
        let before = ui::window_logs().len();
        ui::test_button(&window, "all")
            .expect("Test All")
            .emit_clicked();
        assert!(
            naivepost::ui::settings::wait_for_tests(Duration::from_secs(120)),
            "every row answered"
        );
        common::settle();
        let verdicts = ui::window_logs()[before..]
            .iter()
            .filter(|line| line.starts_with(naivepost::checks::LOG_PREFIX))
            .count();
        assert_eq!(verdicts, 12, "all twelve rows were tested and logged");
        for key in [
            "llm",
            "llm-vision",
            "decision",
            "ffmpeg",
            "firefox",
            "audio",
            "tts",
            "asr",
            "diar",
            "sep",
            "aligner",
            "sd",
        ] {
            let mark = ui::badge(&window, key)
                .map(|b| b.text().to_string())
                .unwrap_or_default();
            assert!(
                mark == "\u{2713}" || mark == "\u{2717}",
                "{key} carries a verdict after Test All, got {mark:?}"
            );
        }

        // Each section's ⓘ opens its explanation.
        let dialog = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .find(|top| top.title().as_deref() == Some("Settings") && top.is_visible())
            .expect("the Settings window");
        for (key, _) in naivepost::ui::settings_info::SECTIONS {
            let info = find_in(dialog.upcast_ref(), &format!("settings-info-{key}"))
                .and_then(|widget| widget.downcast::<gtk::MenuButton>().ok())
                .unwrap_or_else(|| panic!("the {key} section has its ⓘ"));
            info.popup();
            common::settle();
            let popover = info.popover().expect("the ⓘ has a popover");
            assert!(
                popover.is_visible(),
                "a click on {key}'s ⓘ shows its explanation"
            );
            let words = find_in(popover.upcast_ref(), &format!("settings-info-text-{key}"))
                .and_then(|widget| widget.downcast::<gtk::Label>().ok())
                .expect("the explanation's label");
            assert_eq!(
                Some(words.text().as_str()),
                naivepost::ui::settings_info::explanation(key),
                "{key}'s ⓘ says its own section's text"
            );
            info.popdown();
        }
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
