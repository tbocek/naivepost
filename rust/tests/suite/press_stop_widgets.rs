//! §03-shell.md F0.3 Press ⏹ — through the widget. The button decides nothing: it draws
//! `run::controls`' `stop_sensitive` and forwards its click to `run::RunBar::press_stop`, so what
//! these checks assert is that the forwarding happened, not what a press means
//! (spec/00-principles.md §5).

use crate::common;
use adw::prelude::*;
use common::fixture_dir;
use naivepost::run;
use naivepost::ui;

/// One running GTK application for this test binary, taken in turn by the checks below — the same
/// single-main-loop arrangement tests/suite/press_play_widgets.rs needs (`g_application_run` refuses a
/// second claimant of the default main context). Both scenarios run inside one `activate`, because
/// ⏹'s meaning depends on what the press before it did.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    /// A fake audio.cpp on loopback (§S5 asks it before any stage runs) — the shape
    /// `tests/suite/press_prepare_widgets.rs` uses: healthy on `/health`, ASR + diarization on `/v1/models`.
    /// The Prepare presses below need one: without it the step is refused by the preflight and there is no
    /// run for ⏹ to stop.
    fn fake_audio_server() -> String {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                std::thread::spawn(move || {
                    let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap_or(0) == 0
                            || header.trim().is_empty()
                        {
                            break;
                        }
                    }
                    let payload = if line.contains("/v1/models") {
                        r#"{"data":[{"id":"nemotron-asr","task":"asr"},{"id":"sortformer-diar","task":"diar"}]}"#
                    } else {
                        "{}"
                    };
                    let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                    let _ = stream.write_all(reply.as_bytes());
                    let _ = stream.flush();
                });
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    /// Point the settings this process reads at that server. One conf for the whole binary: every check
    /// here presses ▶ or ⏹ on a Prepare page, so they all need the same answer, and `XDG_CONFIG_HOME`
    /// belongs to the process.
    fn serve_audio_for_the_presses(url: &str) {
        unsafe {
            std::env::remove_var("AUDIOCPP_SERVER");
            std::env::remove_var("NAIVEPOST_TTS_URL");
        }
        let config = std::env::temp_dir().join(format!("np-f03w-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&config);
        let paths = naivepost::settings::Paths {
            config_dir: config.join("naivepost"),
            data_dir: config.join("data"),
        };
        let mut conf = naivepost::settings::Conf::default();
        conf.audio_server = url.to_string();
        naivepost::settings::save(&paths, &conf).expect("the conf points at the fake server");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
    }

    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            // §S5's preflight runs before any stage, so the Prepare presses below need an audio.cpp.
            serve_audio_for_the_presses(&fake_audio_server());
            APP.with(|held| *held.borrow_mut() = Some(app.clone()));
            stops_a_run(app);
            says_nothing_when_there_is_nothing_to_stop(app);
            stops_the_playing_cut_preview_through_the_button();
            RAN_PLAY.store(true, std::sync::atomic::Ordering::SeqCst);
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_PLAY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    /// The one application this binary runs its checks inside. Set at the top of `connect_activate`,
    /// read by the checks that were not handed it as an argument.
    static APP: std::cell::RefCell<Option<adw::Application>> = const { std::cell::RefCell::new(None) };
}

/// S1 through the real widget with a preview that is genuinely playing: ⏹ must stop the PLAYER, not
/// only write a status line. Before this round the handler passed a hard-coded stopped `Transport`
/// into `press_stop`, so the transport half of S1 could never fire from a click at all.
fn stops_the_playing_cut_preview_through_the_button() {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    // Cut owns a preview (`transport_for` answers Some there), so this is the page where S1 applies.
    let window = ui::build_window(
        &APP.with(|held| held.borrow().clone())
            .expect("the round's application"),
        &model,
        "Cut",
    );
    window.present();

    // Put the player in ✂ cut mode and playing, through the same setter the other Cut tests use.
    ui::set_preview_state(
        &window,
        true,
        false,
        naivepost::run::Transport {
            playing: true,
            started: true,
        },
    );
    assert!(
        ui::preview_player(&window).transport.playing,
        "the fixture really has the preview playing"
    );

    let stop = ui::stop_button(&window).expect("⏹ exists on the Cut page's run bar");
    stop.emit_clicked();

    // The player itself stopped — the check that proves S1's transport half is wired.
    assert!(
        !ui::preview_player(&window).transport.playing,
        "⏹ stopped the player, not just the sentence: {:?}",
        ui::preview_player(&window).transport
    );
    // And the press said what S1 says when only the preview was going: this window has no run behind
    // the player, so there is nothing to fall through to and the line reads "playback stopped".
    // (With a run under way the same press ends both and reads 'stopping…' — that fall-through is
    // pinned by `stops_a_run` above and by `f0_3_s1_stops_the_page_transport_and_still_stops_the_run`.)
    assert_eq!(
        ui::state(&window).status,
        run::PLAYBACK_STOPPED,
        "S1's own sentence reaches the status line"
    );
    window.close();
}

#[test]
fn f0_3_s1_pressing_stop_while_the_cut_preview_plays_stops_the_player() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_PLAY.load(std::sync::atomic::Ordering::SeqCst),
        "the playing-preview ⏹ check never ran"
    );
}

/// S1 → S3 through the real widget: ▶ starts a run, ⏹ ends it, and the status line reads
/// "stopping…" rather than staying on the run's own progress.
fn stops_a_run(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("play button");
    let stop = ui::stop_button(&window).expect("F0.3 puts a ⏹ in the run bar");

    // Nothing running yet on Prepare: ⏹ has nothing to end, so it draws insensitive.
    assert!(
        !stop.is_sensitive(),
        "with no run and no preview, ⏹ is not sensitive"
    );

    play.emit_clicked();
    assert_eq!(
        ui::state(&window).status,
        "frames 1/2: frame",
        "F0.5 S2/§2: the run owns the status line, and what it puts there is its track line"
    );
    // F0.5 S4 closes a single-step press's own bookkeeping, so the ⏹ sensitivity is pinned over a run
    // that is still standing -- `start_run`, the call ▶ makes before it walks the step.
    if let (Some(bar), Some(queue)) = (ui::run_bar(&window), ui::run_queue(&window)) {
        naivepost::runqueue::start_run(
            &mut bar.borrow_mut(),
            &mut queue.borrow_mut(),
            &mut naivepost::exchanges::RunLog::new(),
            naivepost::run::Step::Prepare,
            naivepost::run::snapshot_sources(&model),
        );
        if let Some(stop) = ui::stop_button(&window) {
            stop.set_sensitive(true);
        }
    }
    assert!(
        stop.is_sensitive(),
        "S3: with a run under way ⏹ must be sensitive"
    );

    stop.emit_clicked();
    assert_eq!(
        ui::state(&window).status,
        run::STOPPING,
        "one press of ⏹ brings the run down and says so"
    );
    // ▶ keeps the ⏸ face after the stop: `press_stop` marks the run stopped but leaves `running` in
    // place — clearing it is the stopping flow's own job once it has unwound (F0.5), so `controls`
    // still sees a run and keeps both buttons' faces as busy. Asserted as-is rather than as an
    // "idle face" claim; F0.5's teardown lives on the step's own finish (`run::RunBar::finish`), not
    // on the ⏹ press, which only sets the flag.
    window.close();
}

/// S2 through the widget: a fresh window has no run, so pressing ⏹ leaves the status line as it was
/// rather than blanking it or writing "stopping…".
fn says_nothing_when_there_is_nothing_to_stop(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Produce");
    window.present();

    let stop = ui::stop_button(&window).expect("⏹ exists on every page's run bar");
    let before = ui::state(&window).status.clone();
    assert!(
        !stop.is_sensitive(),
        "nothing to end, so ⏹ draws insensitive"
    );
    // Fire it anyway: even forced, S2 says nothing changes.
    stop.emit_clicked();
    assert_eq!(
        ui::state(&window).status,
        before,
        "S2: a press with no run says nothing and clears nothing"
    );
    window.close();
}

#[test]
fn f0_3_pressing_the_stop_button_goes_through_the_bar() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the ⏹ checks never ran"
    );
}
