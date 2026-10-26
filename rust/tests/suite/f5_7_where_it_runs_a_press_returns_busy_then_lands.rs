//! F5.7 "Where it runs" (spec/08-produce.md): a page press that asks a model walks on a worker. The press
//! comes back at once with its button greyed (its busy face), a second press while it walks is ignored, and
//! the model's answer lands on the status line when the walk does (§03-shell §1). The first rewrite waited
//! for the model inside the click.
//!
//! The model is a loopback fake the settings name, slow on purpose: a press that waited for it inline could
//! not come back in time.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use adw::prelude::*;

use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::produce_runs;
use naivepost::settings::{self, Conf};
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

const MODEL: &str = "f57-where-model";

/// The three answers ↻ suggest rewrites, as the model labels them.
const THREE_ANSWERS: &str = "TITLE: a title from the model\nTHUMBNAIL: an instruction from the \
model\nThe model also wrote this paragraph about the video.";

/// How long the fake model takes to answer: long beside a press's own work.
const SLOW: Duration = Duration::from_millis(1500);

/// How long the press may take to come back.
const PRESS_LIMIT: Duration = Duration::from_millis(600);

/// A loopback chat server that answers every request with [`THREE_ANSWERS`] after [`SLOW`], counting them.
fn slow_model(arrivals: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port for the fake model");
    let address = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("an address").port()
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let arrivals = Arc::clone(&arrivals);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("clone the socket"));
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = rest.trim().parse().unwrap_or(0);
                    }
                    if line.trim().is_empty() {
                        break;
                    }
                }
                let mut payload = vec![0u8; length];
                if length > 0 && reader.read_exact(&mut payload).is_err() {
                    return;
                }
                arrivals.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(SLOW);
                let body = serde_json::json!({
                    "choices": [{ "message": { "content": THREE_ANSWERS }, "finish_reason": "stop" }]
                })
                .to_string();
                let framed = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(framed.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            });
        }
    });
    address
}

/// A port nothing listens on, for sd.cpp: no leg waits on a connect.
fn dead_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port to hand out");
    let port = listener.local_addr().expect("an address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// Pin cwd into a folder of our own, point the settings at `server`, and lay down the plain picture a
/// re-word prints onto.
fn scenario(server: &str) -> Tree {
    let root = common::temp_dir_created("f57-where");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario");
    let config = root.join("config");
    let data = root.join("data");
    std::fs::create_dir_all(&config).expect("config folder");
    std::fs::create_dir_all(&data).expect("data folder");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.model = MODEL.to_string();
    conf.server = server.to_string();
    conf.sd_server = dead_address();
    settings::save(&paths, &conf).expect("settings written");
    let tree =
        Tree::new(naivepost::startup::session_dir(&root).as_path()).expect("a session folder");
    tree.create_dirs().ok();
    let plain = tree.thumbnail_plain_png();
    std::fs::create_dir_all(plain.parent().expect("the publish folder")).ok();
    naivepost::produce_upload_print::flat_png_for_test(&plain, 320, 180, 0.3)
        .expect("a flat base for the print pass");
    tree
}

#[test]
fn f5_7_where_it_runs_a_press_returns_busy_then_lands() {
    common::one_app_round(|app| {
        let arrivals = Arc::new(AtomicUsize::new(0));
        let _tree = scenario(&slow_model(Arc::clone(&arrivals)));
        let model = naivepost::project::load(&common::fixture_dir()).expect("fixture loads");
        let window = ui::build_window(app, &model, naivepost::shell::Page::Produce.label());
        window.present();
        common::settle();
        ui::seed_review_cut(
            &window,
            &Cut {
                segs: vec![
                    Seg {
                        s: 0.0,
                        e: 15.0,
                        ..Default::default()
                    },
                    Seg {
                        s: 15.0,
                        e: 30.0,
                        ..Default::default()
                    },
                ],
                aspect: "16:9".to_string(),
                ..Default::default()
            },
        );
        let mut page = ui::produce_page::read_state();
        page.instruction = "an instruction already on the page".to_string();
        page.thumb_title = "a title already on the page".to_string();
        page.title = page.thumb_title.clone();
        ui::produce_page::set_state(page);
        ui::produce_page::refresh(&window);
        common::settle();

        let button = ui::line_step_button(&window, "title-suggest")
            .expect("§1 item 8 drew a widget named `title-suggest`");
        let mark = ui::window_logs().len();
        let pressed = Instant::now();
        button.emit_by_name::<()>("clicked", &[]);
        let took = pressed.elapsed();
        assert!(
            took < PRESS_LIMIT,
            "↻ suggest came back after {took:?}: it waited for the model inside the click"
        );
        assert!(
            !button.is_sensitive(),
            "the press shows it is busy while the model is asked"
        );
        assert_ne!(
            common::status_text(&window),
            produce_runs::REWORD_DONE,
            "the done sentence waits for the model's answer"
        );
        // A second press while the first walks is ignored: one question, one answer.
        button.emit_by_name::<()>("clicked", &[]);

        assert!(
            ui::background::settle_for_test(Duration::from_secs(120)),
            "the press's worker answered"
        );
        assert_eq!(
            common::status_text(&window),
            produce_runs::REWORD_DONE,
            "the model's answer lands on the status line when the walk does"
        );
        assert!(button.is_sensitive(), "and the button is pressable again");
        assert!(
            arrivals.load(Ordering::SeqCst) >= 1,
            "the model the settings name was asked"
        );
        let opened = ui::window_logs()[mark..]
            .iter()
            .filter(|line| line.contains(produce_runs::REWORD_LOG))
            .count();
        assert_eq!(
            opened, 1,
            "the press made while the first one walked was ignored: one re-word opened"
        );
        RAN.store(true, Ordering::SeqCst);
        window.close();
        common::settle();
    });
    assert!(RAN.load(Ordering::SeqCst), "the press check never ran");
}
