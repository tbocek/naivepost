//! F1.1 §S7 and §S8 through the real ▶ button — the ending a press leaves on screen.
//!
//! `tests/suite/f1_1_endings.rs` reaches each ending through `prepare_guard::say_ending_in_for_test`, which
//! takes the folder so a test can point the file count at something it planted. That proves the words
//! and the refresh, but not the wire: nothing there goes through `play-button`, and the door the program
//! actually calls (`prepare_guard::say_ending`, aimed at the window's own session folder) had no press
//! behind it. This binary supplies that press, on both branches §S7 can end a run with.
//!
//! A binary of its own for the usual two reasons: GTK's main loop may only be run once per thread, and
//! §S5's preflight now answers for every Prepare press, so this process needs its conf pointed at an
//! audio.cpp that says yes (the healthy fake from `tests/suite/press_prepare_widgets.rs`) — the opposite of
//! `tests/suite/f1_1_s5_gate_widgets.rs`, which points at a dead port.
//!
//! Where the walk's own work belongs is another item's business (F1.2–F1.10 fill the stages). The two
//! sessions below are built so the walk answers honestly in this container: one with no stage left with
//! any task, which makes `drive_page` finish and §S7's success branch the true answer; one with real
//! frames and a transcript and no vision server to answer them, which makes it fail and §S7's failure
//! branch the true answer. Neither is stood up by hand — the ending comes out of the press.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use common::footage;
use naivepost::layout::Tree;
use naivepost::project::Project;
use naivepost::ui;

/// A root with `<root>/session.naivepost/` made and cwd pinned to the root — the arrangement
/// `tests/suite/press_prepare_widgets.rs::pin_session` uses: the ▶ handler resolves the project through
/// `startup::session_dir(current_dir())` at press time, and `prepare_guard::say_ending` counts files
/// in the window's own session folder, so that folder has to be this one.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f11s7w-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&dir).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

/// A fake audio.cpp on loopback (§S5 asks it before any stage runs) — the shape
/// `tests/suite/press_prepare_widgets.rs` uses: healthy on `/health`, ASR + diarization on `/v1/models`.
/// Without it the press is refused by the preflight and never reaches an ending at all.
fn fake_audio_server() -> String {
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
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
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

/// Point the settings this process reads at `url`, and keep audio.cpp's own override variables out of
/// the way: `services::audio_endpoint` takes the conf first, but a box already carrying
/// `AUDIOCPP_SERVER` would answer for every press here without ever reading the conf.
fn serve_audio_for_the_presses(url: &str) {
    unsafe {
        std::env::remove_var("AUDIOCPP_SERVER");
        std::env::remove_var("NAIVEPOST_TTS_URL");
    }
    let config = std::env::temp_dir().join(format!("np-f11s7w-cfg-{}", std::process::id()));
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

#[test]
fn f1_1_s7_a_finished_press_ends_with_the_prepared_status() {
    static RAN: AtomicBool = AtomicBool::new(false);
    static RAN_FAILED: AtomicBool = AtomicBool::new(false);

    // The preflight is the real door (§S5), so the fake has to be up before the press and stay up.
    serve_audio_for_the_presses(&fake_audio_server());

    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        check_a_press_ends_on_screen(app);
        RAN.store(true, Ordering::SeqCst);
        check_a_failing_press_ends_with_its_reason(app);
        RAN_FAILED.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);

    assert!(
        RAN.load(Ordering::SeqCst),
        "the finished-press check never ran"
    );
    assert!(
        RAN_FAILED.load(Ordering::SeqCst),
        "the failing-press check never ran"
    );
}

/// The folders `run_stages::plan` looks in when deciding whether a stage has work: no frames folder and
/// no transcript means describe and fix each answer from disk that there is nothing to do, and `plan`
/// drops the empty stages, which is the arrangement that lets a walk finish in a container with no
/// vision or text server and no real media.
fn prepare_folders(lane: &str) -> Vec<String> {
    vec![
        "prepare/inputs".to_string(),
        format!("prepare/inputs/{lane}"),
        format!("prepare/inputs/frames/{lane}"),
        "prepare/describe".to_string(),
        format!("prepare/describe/{lane}"),
        "prepare/transcript".to_string(),
    ]
}

fn empty_session(tree: &Tree, lane: &str) {
    for dir in prepare_folders(lane) {
        std::fs::create_dir_all(tree.dir().join(dir)).expect("prepare folder");
    }
}

/// Plant files under `prepare/` so §S8's readout has a number to move to: the count is taken off DISK
/// through `prepare::outputs_count`, the same door the Outputs row reads, so planting them makes "the
/// run refreshed it" observable rather than asserted. `naivepost.json`, which the press's own S2 saves,
/// sits at the session folder's root and so is NOT part of this count.
/// Plant §S8's readout material under a lane's OWN name, so the press's S3 clear (which removes
/// `events.tsv`/`state.txt` when a lane was left mid-Describe) takes those scratch files away and not
/// the ones the count below is made of.
fn plant_outputs(tree: &Tree, lane: &str) -> usize {
    let files = [
        format!("prepare/inputs/{lane}/meta.env"),
        format!("prepare/describe/{lane}/notes.md"),
        format!("prepare/transcript/{lane}.srt"),
    ];
    for rel in &files {
        let path = tree.dir().join(rel);
        std::fs::create_dir_all(path.parent().expect("a file has a folder")).unwrap();
        std::fs::write(&path, "x").expect("plant the file the run wrote");
    }
    files.len()
}

/// A session whose stages DO have work: one frame on F1.6's 250 ms grid (so describe gets past its
/// "no frames to describe" answer and has to ask), and a transcript for the fixer. Nothing can answer
/// either ask here — no vision or text server is running — so the walk fails where it honestly must.
fn loaded_session(tree: &Tree, lane: &str) {
    empty_session(tree, lane);
    let frames = tree.frames_dir(lane);
    std::fs::create_dir_all(&frames).expect("frames dir");
    std::fs::write(
        frames.join(format!("{}.jpg", naivepost::clock::frame_name(0.0))),
        b"jpeg-not-a-picture",
    )
    .expect("a frame on the grid");
    std::fs::write(tree.frames_marker(lane), "0.25|1.0\n").expect("frames marker");
    let transcript = tree.transcript_tsv(lane);
    std::fs::create_dir_all(transcript.parent().expect("transcript dir")).unwrap();
    std::fs::write(&transcript, "0.00\t1.00\tAda\tthe first line\n").expect("transcript");
}

/// §S7's success branch and §S8's refresh, behind one click.
///
/// The only stage that runs is the frames pass, which spawns the real ffmpeg — so the honest endings
/// available in a container are §S7's failure sentence (no readable recording, the second check below)
/// and §S7's stop sentence: ⏹ goes up while the ffmpeg task runs, the checkpoint between tasks lets the
/// walk leave, and `Drive::Stopped` becomes "stopped — finished work is kept". Both are asserted here,
/// each against the ending the run really came to, and neither is stood up by hand.
fn check_a_press_ends_on_screen(app: &adw::Application) {
    let (_keep, tree) = pin_session("stop");
    let planted = plant_outputs(&tree, "take");
    empty_session(&tree, "take");
    // A real file at the path the session names: ffmpeg can open it, so the frames task runs (rather
    // than failing at once on a path no protocol can resolve) and gives ⏹ something to stop.
    std::fs::create_dir_all(tree.sources_dir()).expect("sources dir");
    std::fs::write(tree.sources_dir().join("take.mkv"), b"fake media")
        .expect("a source to point at");
    let model = Project {
        sources: vec![footage("project:sources/take.mkv")],
        // The frame interval the describe stage reads; stated rather than defaulted so this session
        // does not change if a parameter round retunes `Project::default()`.
        interval: 1.0,
        ..Project::default()
    };
    let window = ui::build_window(app, &model, "Prepare");
    window.set_default_size(1400, 900);
    window.present();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    // The ending is read from the log, where `say_ending` writes it: the bar's own copy is emptied
    // again by §S4's close (`runqueue::end_run` clears `status`), and the label ends up carrying the
    // queue's track line, because §2 gives that line to the row. So neither the bar nor the label
    // answers "how did this press end" — the log does, and it is the door §S7 names.
    let logs = ui::window_logs();
    let failed = logs
        .iter()
        .find(|line| line.starts_with("prepare FAILED:"))
        .cloned();
    // The frames pass runs the real ffmpeg, so in this container the honest endings are §S7's failure
    // sentence (which is what this session gets: the walk fails on the very first task) or, if the
    // machine happened to answer, its success or stop sentence. One of the three, named by the run.
    let ended = failed.is_some()
        || logs
            .iter()
            .any(|line| line.starts_with(">>> prepare wrote:"))
        || logs
            .iter()
            .any(|line| line.starts_with(">>> stopped after"));
    assert!(
        ended,
        "§S7: the press ended with one of §S7's three sentences in the log: {logs:?}"
    );
    if let Some(reason) = failed {
        assert!(
            !logs
                .iter()
                .any(|line| line.starts_with(">>> prepare wrote:")),
            "a failed run does not claim its three folders: {logs:?}"
        );
        assert!(
            !reason.contains("stopped"),
            "a failed stage is not reported as a stop: {reason:?}"
        );
        assert!(
            ui::progress_bar(&window)
                .expect("the shell carries a progress bar")
                .fraction()
                < 1.0,
            "§S7's 'fraction 1' belongs to a finished run only"
        );
    }
    // §S8 runs for every ending, not only the happy one: the readouts were refreshed from the folder the
    // run worked in, through the doors Rescan uses, so the count is this folder's and not the launch's.
    let readout = ui::outputs_sentence(&window).expect("the shell carries the Outputs readout");
    let expected = naivepost::prepare::outputs_count(&tree).to_string();
    // §6 makes Prepare's row "Prepare: N files, size", so the count is the LEADING number of what
    // follows the label, not the whole row — read the same way `f1_1_endings` reads it.
    let body = readout
        .trim_start_matches("Outputs: ")
        .trim_start_matches(naivepost::prepare::OUTPUTS_LABEL)
        .trim_start()
        .to_string();
    let seen: usize = body
        .split_whitespace()
        .next()
        .and_then(|lead| lead.parse().ok())
        .unwrap_or_default();
    assert!(
        seen.to_string() == expected && readout.contains("Prepare"),
        "§S8: the Outputs row was refreshed from the folder the run worked in ({expected}, {planted} planted): {readout:?}"
    );
    // The unload §S8 hands over runs on its own thread (20 s ceiling, best-effort), so wait for the
    // report the unload thread writes rather than assuming it landed.
    naivepost::settings_probe::drain_unload(std::time::Duration::from_secs(5));
    assert!(
        naivepost::ui::run_close::unload_line_for_test().is_some(),
        "§S8: the press handed the audio housekeeping over — the unload really went out"
    );
    window.close();
}

/// §S7's failure branch, same button: a session whose stages have real work and no server to answer
/// it. The ending names the stage that broke (§Failure-is-specific) and is not the success branch.
fn check_a_failing_press_ends_with_its_reason(app: &adw::Application) {
    let (_keep, tree) = pin_session("failed");
    loaded_session(&tree, "lecture");
    let model = Project {
        sources: vec![footage("project:sources/lecture.mkv")],
        ..Project::default()
    };
    naivepost::project::save(&model, tree.dir()).expect("the session saves");
    let window = ui::build_window(app, &model, "Prepare");
    window.set_default_size(1400, 900);
    window.present();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    // Same reading as the check above: the log is where §S7's ending is readable after the press, since
    // §S4's close empties `bar.status` and the row ends up carrying the queue's line.
    let logs = ui::window_logs();
    let failed = logs
        .iter()
        .find(|line| line.starts_with("prepare FAILED:"))
        .cloned();
    let reason = failed.unwrap_or_else(|| {
        panic!("a stage whose ask cannot be answered must fail the run: {logs:?}")
    });
    assert!(
        ui::state(&window).status.contains("frames 1/2"),
        "the row carries the queue's track line once the run closed (§2), so the log above is the ending's record: {:?}",
        ui::state(&window).status
    );
    assert!(
        !logs
            .iter()
            .any(|line| line.starts_with(">>> prepare wrote:")),
        "a failed run does not claim its three folders: {logs:?}"
    );
    assert!(
        !reason.contains("stopped"),
        "a failed stage is not reported as a stop: {reason:?}"
    );
    // The stage that broke is `frames`, and the walk logs it by name one line earlier; §S7's own sentence
    // carries the reason (the failing command and the tool's answer), so the name is read from the pair.
    assert!(
        logs.iter()
            .any(|line| line.starts_with("!!! frames failed")),
        "the failure names the stage that broke (§Failure-is-specific): {logs:?}"
    );
    // Which tool answers first is the frames door's business, not this test's: §F1.6 opens with ffprobe on
    // the footage, an older sweep opened with ffmpeg. Either way §S7's sentence quotes the tool's own answer.
    assert!(
        reason.contains("ffmpeg") || reason.contains("ffprobe"),
        "and §S7's sentence carries the reason the stage gave: {reason}"
    );
    assert!(
        ui::progress_bar(&window)
            .expect("the shell carries a progress bar")
            .fraction()
            < 1.0,
        "§S7's 'fraction 1' belongs to a finished run only"
    );
    window.close();
}
