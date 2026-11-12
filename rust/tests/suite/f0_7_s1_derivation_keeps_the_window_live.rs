//! F0.7 (spec/03-shell.md): typing the User Context asks no model. The first rewrite derived the policy after
//! a pause in typing, so a model was called before anybody pressed ▶; now the derivation waits for the press
//! (`f0_7_derive_doors`, `f0_7_s2_the_produce_press_derives_on_a_worker`), and a typed context costs nothing.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

/// A model that counts what reaches it, then sets the language.
fn counting_model(asked: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            asked.fetch_add(1, Ordering::SeqCst);
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
fn f0_7_typing_the_user_context_asks_no_model() {
    common::pin_private_cwd();
    let asked = Arc::new(AtomicUsize::new(0));
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    let mut conf = naivepost::settings::Conf::default();
    conf.server = counting_model(Arc::clone(&asked));
    conf.model = "a-model".into();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the counting model");
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

        // Well past any pause-in-typing timer: nothing may go out until somebody presses ▶.
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(1500) {
            common::settle();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            asked.load(Ordering::SeqCst),
            0,
            "typing the User Context reached the model before any ▶"
        );
        assert_ne!(
            ui::live_session_policy().language.value,
            "de",
            "and no policy was derived from it yet"
        );
        window.close();
    });
}
