//! F0.7's ▶ door on the Produce page (spec/03-shell.md F0.7, §1 "the window thread never waits"): a context
//! nobody derived from is derived when ▶ is pressed, and the model call goes to a worker. The first worker
//! version asked the model inside the click on every page but Prepare, so the first ▶ on Produce after
//! opening a project froze the window ("not responding") for as long as the model took. The run is open
//! while the derivation walks, and the render starts only once the derived policy has landed.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::project::Project;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;

/// How long the model takes over the policy ask: long beside what a press may hold the window for.
const MODEL: Duration = Duration::from_millis(1500);

/// How long the press may take to come back: the bar's opening and a worker started, nothing more.
const PRESS_LIMIT: Duration = Duration::from_millis(600);

const CONTEXT: &str = "A lecture. The talk is in German.";

/// A model that takes [`MODEL`], then sets the language.
fn slow_model() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                    if header.to_ascii_lowercase().starts_with("content-length:") {
                        length = header[15..].trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                std::thread::sleep(MODEL);
                let answer = "{\"choices\":[{\"message\":{\"content\":\"\",\"tool_calls\":[{\"function\":{\"name\":\"set_policy\",\"arguments\":\"{\\\"field\\\":\\\"language\\\",\\\"value\\\":\\\"de\\\",\\\"because\\\":\\\"the talk is in German\\\"}\"}}]}}]}";
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                    answer.len()
                );
            });
        }
    });
    url
}

fn base_project() -> Project {
    let mut project = Project::default();
    project.sources.push(naivepost::project::Source {
        path: "/media/take.mp4".into(),
        footage: true,
        ..Default::default()
    });
    project
}

/// The scripted encoder: each command's output appears on disk at once.
fn quick_spawn() -> naivepost::produce_translate::SpawnSlot {
    std::sync::Arc::new(|command: &naivepost::produce_exec::Command| {
        if let Some(out) = command.argv.last() {
            let path = std::path::Path::new(out);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, b"a rendered file of some length");
        }
        Ok(())
    })
}

#[test]
fn f0_7_s2_the_produce_press_derives_on_a_worker_and_renders_after() {
    common::pin_private_cwd();
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    let mut conf = naivepost::settings::Conf::default();
    conf.server = slow_model();
    conf.model = "a-model".into();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the slow model");
    common::one_app_round(|app| {
        let window = ui::build_window(app, &base_project(), Page::Produce.label());
        window.present();
        common::settle();
        let cut = Cut {
            segs: vec![Seg {
                s: 0.0,
                e: 10.0,
                cam: 0,
                ..Default::default()
            }],
            aspect: "16:9".into(),
            ..Default::default()
        };
        ui::seed_review_cut(&cut);
        let mut state = ui::produce_page::read_state();
        state.clips = 1;
        state.seconds = 10.0;
        ui::produce_page::set_state(state);
        ui::produce_page::refresh(&window);
        common::settle();
        naivepost::produce_translate::set_spawn_for_test(quick_spawn());

        // A context typed but never derived from: what a person leaves behind when they open a project and
        // go straight to Produce.
        ui::set_session_context(CONTEXT);
        assert!(
            !ui::policy_is_current(CONTEXT),
            "a fresh window has derived nothing"
        );
        let logged_before = ui::window_logs().len();

        let pressed = Instant::now();
        ui::play_button(&window)
            .expect("the run bar has a play-button")
            .emit_by_name::<()>("clicked", &[]);
        let took = pressed.elapsed();
        assert!(
            took < PRESS_LIMIT,
            "▶ came back after {took:?}: the model was asked inside the click"
        );
        assert_eq!(
            ui::running_step(),
            Some(naivepost::run::Step::Produce),
            "the run is open while the derivation walks, so ⏹ can reach it"
        );
        assert!(
            !ui::window_logs()[logged_before..]
                .iter()
                .any(|line| line.starts_with(">>> transcoding ")
                    || line.starts_with(">>> producing ")),
            "the render waits for the derived policy: no opening line at the press"
        );

        // Through the model's wait: the loop must keep turning all the while, and the policy must land.
        let mut longest = Duration::ZERO;
        let mut landed = false;
        while pressed.elapsed() < Duration::from_secs(10) {
            let turn = Instant::now();
            common::settle();
            longest = longest.max(turn.elapsed());
            if ui::policy_is_current(CONTEXT) {
                landed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(landed, "the ▶ door's derivation landed on the session");
        assert!(
            longest < Duration::from_millis(1000),
            "the window kept turning while the model answered: its longest stall was {longest:?}"
        );
        assert_eq!(
            ui::live_session_policy().language.value,
            "de",
            "the derived policy is the one the step reads"
        );

        assert!(
            ui::background::settle_for_test(Duration::from_secs(120)),
            "the render's worker answered"
        );
        let logs = ui::window_logs();
        let policy_at = logs[logged_before..]
            .iter()
            .position(|line| line.starts_with("policy: "))
            .expect("the derivation is said in the log");
        let opening_at = logs[logged_before..]
            .iter()
            .position(|line| {
                line.starts_with(">>> transcoding ") || line.starts_with(">>> producing ")
            })
            .expect("the render's opening line follows");
        assert!(
            policy_at < opening_at,
            "the policy landed before the render opened: policy at {policy_at}, opening at {opening_at}"
        );
        assert_eq!(
            ui::running_step(),
            None,
            "and the run's own ending closed the bar"
        );
        window.close();
        common::settle();
    });
}
