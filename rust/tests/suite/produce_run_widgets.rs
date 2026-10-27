//! F5.1 ▶ Produce — the WIRE. The rules live in `naivepost::produce_flow` and are pinned by
//! `tests/suite/produce_run_flow.rs`; this file proves the shell's real ▶ on a real Produce page reaches them:
//! the no-cut refusal lands on the status line before the bar opens, a cut standing gets the run's own
//! opening line into the log and "done" on the label, and a matching stamp skips with its said sentence.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: `g_application_run` refuses a second
//! claimant of the default main context, so all three checks run inside that single activate round and
//! each drops its window before the next is built. cwd is pinned to our own temp root BEFORE the run
//! because the page resolves the session through `startup::session_dir(current_dir())`; leaving it at
//! `rust/` would write a stray `rust/session.naivepost/` into the repo.
//!
//! Every click is real: `play-button` (`src/ui/window.rs:4685`), fired with `emit_clicked()` on the
//! button `ui::play_button` finds by name — the same route `tests/suite/narrate_run_widgets.rs` takes.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Cut, Seg};
use naivepost::layout::Tree;
use naivepost::produce_flow as flow;
use naivepost::produce_render as render;
use naivepost::produce_stamp;
use naivepost::project::Project;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{hold_last_window, release_last_window, settle, status_text};

/// A stand-in audio.cpp on a loopback port: `/health`, `/v1/models` and `/v1/ui/upload` are served at
/// once, and the speech POST is where the STOP comes from — answering it, this server raises the shell's
/// cancel switch (`cancel_leg::cancel_now`) before refusing the synthesis. That single call is the only
/// write `RunBar::press_stop` makes (`src/run.rs:347`), so the stop this test applies is the program's own
/// door rather than a poke at private state, and it lands while the run sits inside the request — which is
/// exactly where a real ⏹ finds a speaking run (§02-services#1 puts every request on the cancel context
/// precisely so a stop ends the wait). Timing it with a sleeping thread would race the walk; having the
/// server the run is waiting on raise the switch cannot.
struct StoppingAudio {
    url: String,
}

impl StoppingAudio {
    fn start() -> StoppingAudio {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                std::thread::spawn(move || {
                    use std::io::{BufRead, Read, Write};
                    let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut length = 0usize;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        if let Some(rest) =
                            line.to_ascii_lowercase().strip_prefix("content-length:")
                        {
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
                    let path = request_line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("")
                        .to_string();
                    let (status, body) = match path.as_str() {
                        "/health" => (200u16, r#"{"status":"ok"}"#.to_string()),
                        "/v1/models" => (
                            200,
                            r#"{"data":[{"id":"index-tts2","family":"index_tts2","task":"clon"}]}"#
                                .to_string(),
                        ),
                        "/v1/ui/upload" => {
                            (200, r#"{"path":"/tmp/uploaded/voice_ref.wav"}"#.to_string())
                        }
                        // The speech POST: raise the stop, then refuse the synthesis. The render logs the
                        // failed line and carries on to its NEXT checkpoint (§F5.2 S2 says a line that will
                        // not synthesize costs one silent sentence, not a video) — and that next checkpoint
                        // is the one the stop has just refused.
                        _ => {
                            naivepost::cancel_leg::cancel_now();
                            (500, "the server stopped answering".to_string())
                        }
                    };
                    let framed = format!(
                        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(framed.as_bytes());
                    let _ = stream.flush();
                });
            }
        });
        StoppingAudio {
            url: format!("http://127.0.0.1:{port}"),
        }
    }

    fn url(&self) -> String {
        self.url.clone()
    }
}

/// One narration line: bounds and emotion take the neutral form, only the words matter to the render's
/// speaking half.
fn spoken(text: &str) -> naivepost::narration::Entry {
    naivepost::narration::Entry {
        s: 0.0,
        e: 10.0,
        at: 1.0,
        text: text.into(),
        emotion: String::new(),
        pos: String::new(),
        roll: 0,
    }
}

const ITEM: &str = "F5.1";

static RAN_NO_CUT: AtomicBool = AtomicBool::new(false);
static RAN_RAN_THE_FLOW: AtomicBool = AtomicBool::new(false);
static RAN_SKIP: AtomicBool = AtomicBool::new(false);
static RAN_OVERWRITE: AtomicBool = AtomicBool::new(false);
static RAN_STOPPED: AtomicBool = AtomicBool::new(false);

/// The project folder this round resolves: `startup::session_dir(current_dir())`. cwd is pinned to the
/// test's own temp root, so this never points into `rust/`.
fn session_tree() -> Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    Tree::new(&dir).expect("the seeded session folder ends in .naivepost")
}

/// A project with one footage source: nothing locked (§1 keeps Narrate and Produce unlocked so their ▶
/// can refuse in words rather than hide the tab).
fn base_project() -> Project {
    let mut project = Project::default();
    project.sources.push(naivepost::project::Source {
        path: "/media/take.mp4".into(),
        footage: true,
        ..Default::default()
    });
    project
}

/// A project with one footage source and a publish record standing, so §F5.1 S4's `publish.json exists`
/// branch is the one being taken (and the render has a video to be stale about).
fn published_project() -> Project {
    Project {
        publish: Some(naivepost::project::Publish {
            title: "a title from an earlier run".into(),
            ..Default::default()
        }),
        ..base_project()
    }
}

/// Two clips, so the opening line reads "2 clips at mp4/h264 crf 24".
fn two_clips() -> Vec<Seg> {
    vec![
        Seg {
            s: 0.0,
            e: 10.0,
            cam: 0,
            ..Default::default()
        },
        Seg {
            s: 10.0,
            e: 20.0,
            cam: 0,
            ..Default::default()
        },
    ]
}

/// A real window on the Produce tab, over `project`, with `cut` living in the Cut page's slot (an empty
/// cut leaves the S1 refusal reachable).
fn produce_window(app: &adw::Application, project: &Project, cut: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let window = ui::build_window(app, project, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();
    // The Cut page's own cut is what F5.1 S3 asks first, so seed it: an unsaved tweak must still render.
    ui::seed_review_cut(&window, cut);
    // Publish the page's own state the way a flow does, so `clips` and the readouts match the cut.
    let mut state = ui::produce_page::read_state();
    state.clips = cut.segs.len();
    state.seconds = 20.0;
    ui::produce_page::set_state(state);
    ui::produce_page::refresh(&window);
    settle();
    window
}

/// Answer the overwrite dialog §S2 opens by firing its REAL button, then drain the idle queue until the
/// answer's callback has run: `adw::MessageDialog::response(id)` is what a click on that button fires (the
/// route `produce_words::dialog_answer` takes in `tests/suite/produce_screen_widgets.rs`), so an answering block
/// proves the widget a person presses rather than only reaching for the box. Closing is part of the answer:
/// `connect_response` clears the held dialog and closes the window, and a test that left it standing would
/// find the NEXT press refused by `ask_overwrite`'s one-question guard rather than by its own gate.
fn answer_produce_dialog(response: &str) {
    let dialog =
        naivepost::ui::produce_confirm::open_dialog().expect("S2 opened the overwrite dialog");
    dialog.response(response);
    settle();
    // Overwrite starts the encode on a worker (§03-shell §1): its ending lands before anything is read.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the answer's worker answered"
    );
    assert!(
        naivepost::ui::produce_confirm::open_dialog().is_none(),
        "and answering closes the dialog"
    );
}

/// Fire the shell's ▶ through the toolkit and report what it left on the status line.
fn click_play(window: &adw::ApplicationWindow) -> String {
    ui::play_button(window)
        .expect("the run bar has a play-button")
        .emit_by_name::<()>("clicked", &[]);
    // The press walks on a worker (§03-shell §1): its answer lands before anything is read.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the press's worker answered"
    );
    settle();
    status_text(window)
}

/// Whether the run bar currently shows a run under way.
fn bar_running() -> bool {
    naivepost::ui::run_close::last_run_step_for_test().is_some()
}

/// A stamp that does NOT match what the page will compute, so §F5.1 S2's gate falls to `ConfirmOverwrite`
/// rather than `Skip`: written over an empty cut, which no page holding two clips can reproduce.
fn stale_stamp(tree: &Tree) {
    let project = base_project();
    let settings = naivepost::project::Produce::default();
    let input = produce_stamp::Input {
        settings: &settings,
        segs: &[],
        lines: &[],
        sources: &project.sources,
        aspect: "",
        voice: "",
        fx: &[],
        lanes: &[],
        shape: (1920, 1080),
    };
    produce_stamp::write_stamp(tree, &input.stamp_with(tree, None, 0))
        .expect("stale stamp written");
}

/// The video the overwrite question is about, on disk with known bytes so a later check can see whether it
/// was touched.
fn standing_video(tree: &Tree) -> std::path::PathBuf {
    let video = tree.final_video("mp4");
    if let Some(dir) = video.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(&video, b"a finished video worth asking about").unwrap();
    video
}

/// Did any encode open after `from` — the S4 opening line is what an actual encode writes first.
fn opened_encode_after(from: usize) -> bool {
    ui::window_logs()[from..]
        .iter()
        .any(|l| l.starts_with(">>> transcoding ") || l.starts_with(">>> producing "))
}

fn run_round(app: &adw::Application) {
    // --- (1) no cut: refused at the button, before the bar opens -------------------------------
    {
        let window = produce_window(app, &base_project(), &Cut::default());
        let said = click_play(&window);
        assert_eq!(
            said,
            cut::NO_CUT_YET,
            "{ITEM} S1 through the widget: the refusal is the spec's sentence on the status line"
        );
        assert!(
            !bar_running(),
            "and no run started for work that was refused before it began"
        );
        RAN_NO_CUT.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- (2) a cut stands: the press reaches produce_flow -------------------------------------
    {
        let window = produce_window(
            app,
            &base_project(),
            &Cut {
                segs: two_clips(),
                aspect: "16:9".into(),
                ..Default::default()
            },
        );
        let logs_before = ui::window_logs().len();
        let said = click_play(&window);
        let logs = ui::window_logs();
        assert!(
            logs.len() > logs_before,
            "the press logged something (was {logs_before}, now {})",
            logs.len()
        );
        // S4's opening line landed in the log, in f5_1_s4's shape.
        let opening = logs
            .iter()
            .rev()
            .find(|line| line.starts_with(">>> transcoding ") || line.starts_with(">>> producing "))
            .expect("F5.1 S4's opening line reached the log through the real ▶");
        assert!(
            opening.contains("2 clips at mp4/h264 crf 24"),
            "the opening line says N clips at container/codec crf N: {opening}"
        );
        assert!(
            opening.contains("produce/final.mp4"),
            "and names the file it writes: {opening}"
        );
        // S6: the run ended with one of the three words, on the status line.
        assert!(
            said == render::STAGE_DONE
                || said == render::stopped_log()
                || said == render::failure_log(),
            "S6's ending word is on the status line, got: {said}"
        );
        // ⏹ ends the run this press started, so block (3) sees a bar with nothing under way. The stop
        // button is the same real widget a person uses; the F5.1 flow itself has no reason to clear the
        // bar because its work finishes inside one press.
        ui::stop_button(&window)
            .expect("the run bar has a stop-button")
            .emit_by_name::<()>("clicked", &[]);
        // The press walks on a worker (§03-shell §1): its answer lands before anything is read.
        assert!(
            naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
            "the press's worker answered"
        );
        settle();
        RAN_RAN_THE_FLOW.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- (3) a matching stamp: skipped, and SAID ---------------------------------------------
    {
        let tree = session_tree();
        let cut = Cut {
            segs: two_clips(),
            aspect: "16:9".into(),
            ..Default::default()
        };
        // A video standing there, so the gate has something to be up to date about.
        let video = tree.final_video("mp4");
        if let Some(dir) = video.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(&video, b"a finished video").unwrap();
        // Seed the stamp with the hash THIS page will compute: settings from the page, cut from the page.
        let project = base_project();
        let settings = naivepost::project::Produce::default();
        // The narration's voice as the page reads it: the video is voice 1, so it is that recording's.
        let voice = naivepost::ui::narrate_seed::project_voice(&tree);
        let input = produce_stamp::Input {
            settings: &settings,
            segs: &cut.segs,
            lines: &[],
            sources: &project.sources,
            aspect: &cut.aspect,
            voice: &voice,
            // The page's own gate hashes the cut's effects and beds too; a seed that left them out would
            // never match and the press would ask to overwrite instead of skipping.
            fx: &cut.fx,
            lanes: &cut.lanes,
            shape: (1920, 1080),
        };
        let hash = input.stamp_with(&tree, None, 0);
        produce_stamp::write_stamp(&tree, &hash).expect("stamp written");
        // The page must hold the same settings the stamp was computed from, or the gate sees a change.
        let window = produce_window(app, &project, &cut);
        let said = click_play(&window);
        assert!(
            said.contains("already what this page describes"),
            "S2's skip is SAID rather than silent, through the real ▶: {said}"
        );
        assert!(
            said.contains("\u{21bb} beside Transcode encodes anyway"),
            "and points at ↻ Transcode for the encode anyway: {said}"
        );
        assert_eq!(said, flow::SKIP_LOG, "byte for byte the spec's line");
        // What a skip means for the bar: no encode was opened. The shell still records the press as a
        // started step (F0.2 owns that), so the proof is in the log — the skip sentence is there and NO
        // S4 opening line was logged after it, which is what an actual encode would have written.
        let logs = ui::window_logs();
        let skip_at = logs
            .iter()
            .rposition(|l| l == flow::SKIP_LOG)
            .expect("the skip line is in the log");
        let opened_encode_after = logs[skip_at..]
            .iter()
            .any(|l| l.starts_with(">>> transcoding ") || l.starts_with(">>> producing "));
        assert!(
            !opened_encode_after,
            "S2: a matching stamp opens no encode — nothing logged past the skip: {:?}",
            &logs[skip_at..]
        );
        // S2's skip leaves out the render and nothing else: the words half (F5.6) still asks for the title, the
        // description and the thumbnail. The first rewrite returned after the tag page, so a video that was up
        // to date never got its upload text. No model is configured here, so the ask is refused, out loud.
        crate::common::settle_workers();
        settle();
        let logs = ui::window_logs();
        let first = logs
            .iter()
            .position(|l| l == flow::SKIP_LOG)
            .expect("the skip line");
        assert!(
            logs[first..].iter().any(|l| l.starts_with(">>> youtube:")),
            "a skipped encode still runs the words half: {:?}",
            &logs[first..]
        );
        // F5.6 onto the page when the run lands: what the words half wrote to `publish.json` is what the Title and
        // description boxes then hold (the prototype's `landPublish`). Written here as the words half writes it,
        // and the run pressed again; the first rewrite never read it back, so the boxes stayed empty.
        let written = naivepost::project::Publish {
            title: "Written by the run".to_string(),
            thumb_title: "Written by the run".to_string(),
            description: "The run's own description.".to_string(),
            ..Default::default()
        };
        naivepost::publish::save(&written, &tree).expect("publish.json written");
        click_play(&window);
        crate::common::settle_workers();
        settle();
        let shown = ui::produce_page::read_state();
        assert_eq!(shown.title, "Written by the run", "the title box");
        assert_eq!(
            shown.description, "The run's own description.",
            "and the description box"
        );
        // F5.6, the prototype's `needText = !written`: once the upload text is written, ▶ keeps it and the picture
        // the person chose, and does not ask the model again (↻ beside the title does). The first rewrite asked on
        // every press, and the model's answer replaced the chosen frame and the typed title.
        let chosen = naivepost::project::Publish {
            title: "My own title".to_string(),
            thumb_title: "My own title".to_string(),
            title_seeded: true,
            description: "My own description.".to_string(),
            frames: vec!["frame:11.750".to_string()],
            own: true,
            ..Default::default()
        };
        naivepost::publish::save(&chosen, &tree).expect("publish.json written");
        // ...and on the page, where the person typed the title and chose the frame.
        let mut page = ui::produce_page::read_state();
        page.title = chosen.title.clone();
        page.frames = chosen.frames.clone();
        page.own = true;
        ui::produce_page::set_state(page);
        let mark = ui::window_logs().len();
        click_play(&window);
        crate::common::settle_workers();
        settle();
        let after = &ui::window_logs()[mark..];
        assert!(
            after
                .iter()
                .any(|l| l == naivepost::ui::produce_languages::UPLOAD_TEXT_KEPT_LOG),
            "the written text is kept, said once: {after:?}"
        );
        assert!(
            !after.iter().any(|l| l.starts_with(">>> youtube:")),
            "and the model is not asked: {after:?}"
        );
        let kept = naivepost::publish::load(&tree).expect("publish.json");
        assert_eq!(kept.title, "My own title", "the typed title stands");
        assert_eq!(
            kept.frames,
            vec!["frame:11.750".to_string()],
            "and so does the chosen frame"
        );
        // The page letters the picture's title over the picture only once the title is seeded, so the landing
        // brings the seeding with the words: the first rewrite copied the title and not the flag, and the plate
        // showed the chosen frame with no title while `thumbnail.png` carried it.
        let shown = ui::produce_page::read_state();
        assert!(
            shown.title_seeded && shown.thumb_title == "My own title",
            "the page holds the printed title, seeded: {:?} / {}",
            shown.thumb_title,
            shown.title_seeded
        );
        RAN_SKIP.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- (4) a stale video: the question is ASKED, and an answer runs the encode ----------------
    {
        let tree = session_tree();
        stale_stamp(&tree);
        let video = standing_video(&tree);
        let before = std::fs::read(&video).expect("the standing video");
        let cut = Cut {
            segs: two_clips(),
            aspect: "16:9".into(),
            ..Default::default()
        };
        // A publish record stands (§S4's `publish.json exists` shape), so the encode that follows opens with
        // the "producing … thumbnail written beside them" line rather than the "left as they are" one.
        let window = produce_window(app, &published_project(), &cut);

        // (a) Cancel leaves the video alone and opens no encode.
        let logs_before = ui::window_logs().len();
        let said = click_play(&window);
        assert_eq!(
            said, "Overwrite final.mp4?",
            "{ITEM} S2 on the status line: {said}"
        );
        let dialog = naivepost::ui::produce_confirm::open_dialog()
            .expect("S2 opens a real dialog: the press must not stop at the question");
        assert!(
            dialog
                .body()
                .to_string()
                .contains("The encode takes minutes and there is no undo for it."),
            "and its body says what the overwrite costs: {}",
            dialog.body()
        );
        answer_produce_dialog(naivepost::produce_flow::ANSWER_CANCEL);
        assert_eq!(
            status_text(&window),
            "Overwrite final.mp4?",
            "Cancel leaves the question standing \u{2014} nothing was destroyed"
        );
        assert!(
            !opened_encode_after(logs_before),
            "and no encode opened: {:?}",
            &ui::window_logs()[logs_before..]
        );
        assert_eq!(
            std::fs::read(&video).expect("still there"),
            before,
            "the video is untouched"
        );

        // (b) Overwrite answers the same question by running the encode.
        let logs_before = ui::window_logs().len();
        let _ = click_play(&window);
        answer_produce_dialog(naivepost::produce_flow::ANSWER_OVERWRITE);
        assert!(
            opened_encode_after(logs_before),
            "{ITEM} S2: answering Overwrite runs the encode \u{2014} nothing logged after the answer: {:?}",
            &ui::window_logs()[logs_before..]
        );
        let logs = ui::window_logs();
        let opening = logs
            .iter()
            .rev()
            .find(|l| l.starts_with(">>> transcoding ") || l.starts_with(">>> producing "))
            .expect("the run's own opening line");
        assert!(
            opening.contains("2 clips at mp4/h264 crf 24"),
            "from this cut: {opening}"
        );
        let done = status_text(&window);
        assert!(
            done == render::STAGE_DONE
                || done == render::failure_log()
                || done == render::stopped_log(),
            "and the run ended with one of S6's three words, got {done}"
        );
        ui::stop_button(&window)
            .expect("the run bar has a stop-button")
            .emit_by_name::<()>("clicked", &[]);
        // The press walks on a worker (§03-shell §1): its answer lands before anything is read.
        assert!(
            naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
            "the press's worker answered"
        );
        settle();
        RAN_OVERWRITE.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }
    // --- (5) a run stopped at a checkpoint: §F5.1 S6's "production stopped" -------------------
    {
        // The person presses ⏹ while the render is walking its subprocesses. That press writes the
        // shell's cancel switch (`RunBar::press_stop` → `cancel_leg::cancel_now`, `src/run.rs:347`), and
        // §F5.2 S10's checkpoints are asked of exactly that switch, so the walk halts at the next one and
        // the run ends with S6's stopped word rather than "done".
        let tree = session_tree();
        let stamp = produce_stamp::stamp_path(&tree);
        let _ = std::fs::remove_file(&stamp);
        // No video standing: the gate goes straight to Encode, so the press reaches the walk instead of
        // stopping at §S2's overwrite question (which would leave the encode, and its checkpoints, unasked).
        let _ = std::fs::remove_file(tree.final_video("mp4"));
        // A cut the render CAN plan (well over `P.eng.minClipSeconds`), so S3's "no clip could be placed"
        // failure — which comes before any checkpoint is asked — never competes with the stop.
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
        // TWO lines to speak: `produce_exec::walk` asks its checkpoint once per line (`if !ask(..)` before
        // each synthesis), so the first line pays the checkpoint and dials the server, and it is the
        // SECOND line's checkpoint that the stop refuses. With one line there is no next checkpoint to ask.
        naivepost::narration::save(
            &naivepost::narration::Narration {
                entries: vec![
                    spoken("The chain moved to proof of stake."),
                    spoken("Now the layer two story, in one breath."),
                ],
                silent: vec![],
            },
            &tree,
        )
        .expect("two lines for the run to speak");
        // F4.6's reference on disk, so the speak door gets past its own S1 and really dials; with none it
        // refuses before reaching the wire and the server never sees a request to answer.
        let reference = tree.voice_ref_wav();
        std::fs::create_dir_all(reference.parent().expect("narrate/ has a parent")).unwrap();
        std::fs::write(
            &reference,
            b"RIFF fake reference, forty-odd bytes long, not empty",
        )
        .unwrap();
        // The stand-in audio.cpp whose speech POST raises the stop, named by the Settings file the program
        // re-reads per call — the address is the seam, nothing about the work is scripted.
        let audio = StoppingAudio::start();
        let paths =
            naivepost::settings::from_environment().expect("settings folder from the pinned env");
        let mut conf = naivepost::settings::read(&paths).unwrap_or_default();
        conf.audio_server = audio.url();
        conf.tts_model = "index-tts2".to_string();
        naivepost::settings::save(&paths, &conf)
            .expect("the fake address is what the Settings now say");
        let window = produce_window(app, &base_project(), &cut);
        // Blocks (2) and (4) end on a ⏹, so clear the switch first: `runqueue::start_run` does the same for
        // a fresh generation (`bump_cancel_context`), and this press must not inherit an earlier block's stop.
        naivepost::cancel_leg::clear();
        let said = click_play(&window);
        assert_eq!(
            said,
            render::stopped_log(),
            "{ITEM} S6: a run whose checkpoint was refused says \"{}\", got {said}",
            render::stopped_log()
        );
        assert!(
            produce_stamp::read_stamp(&tree).is_none_or(|hash| hash.is_empty()),
            "and a stopped run writes no stamp, whatever the previous press left: {}",
            stamp.display()
        );
        // The bar closes the way every ending closes it: nothing is under way after the press.
        ui::stop_button(&window)
            .expect("the run bar has a stop-button")
            .emit_by_name::<()>("clicked", &[]);
        // The press walks on a worker (§03-shell §1): its answer lands before anything is read.
        assert!(
            naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
            "the press's worker answered"
        );
        settle();
        RAN_STOPPED.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }
}

/// §F5.1 through the real widget: the shell's ▶ on a real Produce page reaches `produce_flow`, and a run
/// stopped at one of §F5.2 S10's checkpoints ends with §F5.1 S6's "production stopped".
///
/// One application, one `connect_activate`, ONE `#[test]`: `g_application_run` will not claim the default
/// main context twice, and a second test asserting the round's flags would read them from another thread
/// before `run_round` had finished. Each block names its own step where it runs (S1 refusal, S4 opening
/// line + S6 ending, S2 skip, S2 overwrite answer, §F5.2 S10 checkpoint stop).
#[test]
fn f5_1_s6_a_stopped_run_ends_stopped_through_the_press() {
    // Pin cwd to our own temp root BEFORE anything builds.
    let root = std::env::temp_dir().join(format!("np-f51-wire-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        app.run_with_args::<&str>(&[]);
    });

    assert!(
        RAN_NO_CUT.load(Ordering::SeqCst),
        "the no-cut refusal check never ran"
    );
    assert!(
        RAN_RAN_THE_FLOW.load(Ordering::SeqCst),
        "the run-through-the-flow check never ran"
    );
    assert!(
        RAN_SKIP.load(Ordering::SeqCst),
        "the up-to-date skip check never ran"
    );
    assert!(
        RAN_OVERWRITE.load(Ordering::SeqCst),
        "the overwrite-question-and-answer check never ran"
    );
    assert!(
        RAN_STOPPED.load(Ordering::SeqCst),
        "the stopped-at-a-checkpoint check never ran"
    );
}
