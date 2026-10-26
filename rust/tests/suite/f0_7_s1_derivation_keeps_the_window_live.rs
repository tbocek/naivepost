//! F0.7 S1 (spec/03-shell.md): typing the User Context derives the policy after the debounce. The model call
//! takes seconds to minutes, and the first rewrite made it on the GTK thread: the window froze ("not
//! responding") for as long as the model took, a profile's field included. Now the call goes to a worker; the
//! window keeps running its loop, and the derived policy still lands.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

/// A model that takes two seconds, then sets the language.
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
                std::thread::sleep(Duration::from_secs(2));
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

#[test]
fn f0_7_s1_a_slow_model_does_not_freeze_the_window() {
    common::pin_private_cwd();
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    let mut conf = naivepost::settings::Conf::default();
    conf.server = slow_model();
    conf.model = "a-model".into();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the slow model");
    let root = common::temp_dir_tagged("derive-live");
    let dir = root.join("demo.naivepost");
    common::copy_fixture(&dir);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Prepare", &dir);
        window.present();
        common::settle();
        let editor = common::find_named(&window, "bench-text")
            .and_then(|w| w.downcast::<gtk::TextView>().ok())
            .expect("the User Context box");
        editor
            .buffer()
            .set_text("A lecture. The talk is in German.");

        // Through the debounce and the model's two seconds: the loop must keep turning all the while.
        let started = Instant::now();
        let mut longest = Duration::ZERO;
        let mut landed = false;
        while started.elapsed() < Duration::from_secs(8) {
            let turn = Instant::now();
            common::settle();
            longest = longest.max(turn.elapsed());
            if ui::live_session_policy().language.value == "de" {
                landed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            landed,
            "the derived policy landed: the language is the model's"
        );
        assert!(
            longest < Duration::from_millis(1000),
            "the window kept turning while the model answered: its longest stall was {longest:?}"
        );
        window.close();
    });
}
