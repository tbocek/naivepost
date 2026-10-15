//! F1.7 Describe — the work behind a real ▶ press, spec/04-prepare.md.
//!
//! `tests/f1_7_describe_run.rs` walks the describe stage through `run_stages::drive_walk`, which proves
//! the row's dial does S0–S7 for real. What that cannot show is the wire from the button: whether a
//! person pressing `play-button` on a Prepare page reaches any of it. This binary presses it and then
//! reads the FILES — `events.tsv`, `state.txt` and `.llmframes/` — rather than the plan lines the ▶
//! handler prints up front, because a plan line only proves something was described as pending.
//!
//! Three fakes, each reached through a seam the program itself uses (§1's re-read-per-request settings):
//! an audio.cpp on loopback so §S5's preflight says yes (without it the press is refused before any
//! stage runs), an LLM on loopback answering with §3.2's `record_event`/`set_state` calls, and a stub
//! ffmpeg/ffprobe folder named in the conf that CREATES the file its argv names. The frames are planted
//! with F1.6's completed marker beside them, so the frames stage skips with no process at all and what
//! remains in the walk is describe's own scaling and asking.
//!
//! One application drives every check: GTK's main loop may only be run once per thread.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use adw::prelude::*;
use naivepost::clock::frame_name;
use naivepost::describe::FRAMES_PER_REQ;
use naivepost::layout::Tree;
use naivepost::project::Project;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::footage;
use naivepost::settings::{self, Conf, Paths};
use naivepost::ui;

/// The lane of the footage row here: the stem of its name, which is what names `events.tsv`, `state.txt`
/// and `.llmframes/`.
const LANE: &str = "lecture";
static RAN_FILES: AtomicBool = AtomicBool::new(false);
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A root with `<root>/session.naivepost/` made and cwd pinned to the root: the ▶ handler resolves the
/// project through `startup::session_dir(current_dir())` at press time, so the session has to be here.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f17p-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(dir.join("sources")).expect("session + sources folders");
    std::fs::write(dir.join("sources/lecture.mkv"), b"mkv-bytes").expect("the recording");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    let tree = Tree::new(dir).expect("a .naivepost folder is a project");
    (root, tree)
}

/// A fake audio.cpp on loopback (§S5 asks it before any stage runs) — the shape
/// `tests/describe_stage_widgets.rs` uses: healthy on `/health`, ASR + diarization on `/v1/models`.
fn fake_audio_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
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

/// The §3.2 answer for a batch of ANY size: one `record_event` per frame, a `set_state`, then `finish`.
fn events_answer(frames: usize) -> String {
    let mut calls = String::new();
    for frame in 1..=frames {
        calls.push_str(&format!(
            "{{\"function\":{{\"name\":\"record_event\",\"arguments\":\"{{\\\"frame\\\":{frame},\\\"text\\\":\\\"frame {frame} of the take\\\"}}\"}}}},",
        ));
    }
    calls.push_str(
        "{\"function\":{\"name\":\"set_state\",\"arguments\":\"{\\\"text\\\":\\\"mid-lecture, board half full\\\"}\"}},",
    );
    calls.push_str("{\"function\":{\"name\":\"finish\",\"arguments\":\"{}\"}}");
    format!("{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[{calls}]}}}}]}}")
}

/// A fake LLM that answers every chat ask with `answer`, and counts the asks it took.
struct FakeLlm {
    url: String,
    asks: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl FakeLlm {
    fn start(answer: String) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let asks = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let counter = asks.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let counter = counter.clone();
                let answer = answer.clone();
                std::thread::spawn(move || {
                    if serve(stream, &answer).is_some() {
                        if let Ok(mut count) = counter.lock() {
                            *count += 1;
                        }
                    }
                });
            }
        });
        FakeLlm { url: format!("http://127.0.0.1:{port}"), asks }
    }

    /// How many chat asks arrived — recording happens on the connection's own thread, so a caller that
    /// needs them all waits for the count it expects.
    #[allow(dead_code)]
    fn waited(&self, count: usize) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if *self.asks.lock().expect("the count lock") >= count {
                return true;
            }
            if std::time::Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn serve(mut stream: TcpStream, answer: &str) -> Option<()> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let payload = answer.to_string();
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(())
}

/// The stub tools: `ffprobe` answers a duration, `ffmpeg` writes `<last arg>/<basename of -i arg>` —
/// where the scaling leg expects its copy — and fails if the file is not there afterwards.
fn stubs() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "np-f17p-tools-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the tools folder");
    let probe = dir.join("ffprobe");
    common::write_script(&probe, "#!/bin/sh\necho \"20.0\"\n").expect("the probe stub");
    let script = dir.join("ffmpeg.sh");
    common::write_script(
        &script,
        "#!/bin/sh\ninput=''\nout=''\nprev=''\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-i\" ]; then input=\"$a\"; fi\n  out=\"$a\"\n  prev=\"$a\"\ndone\nbase=$(basename -- \"$input\")\nmkdir -p \"$out\"\nprintf JPEG > \"$out/$base\"\n[ -f \"$out/$base\" ]\n",
    )
    .expect("the ffmpeg stub");
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("it runs");
    let ffmpeg = dir.join("ffmpeg");
    common::write_script(&ffmpeg, format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", script.display())).expect("the wrapper");
    for tool in [&probe, &ffmpeg] {
        std::fs::set_permissions(tool, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("it runs");
    }
    dir
}

/// Point the settings this process reads at all three fakes at once: audio.cpp (so §S5 says yes), the
/// vision server (so the describe ask comes here) and the stub ffmpeg (so scaling runs a real process).
/// One conf for the whole binary, because `XDG_CONFIG_HOME` belongs to the process.
fn point_settings_at(audio_url: &str, llm_url: &str, tools: &Path) {
    let config = std::env::temp_dir().join(format!("np-f17p-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    let paths = Paths { config_dir: config.join("naivepost"), data_dir: config.join("data") };
    let mut conf = Conf::default();
    conf.audio_server = audio_url.to_string();
    conf.server = llm_url.to_string();
    conf.model = "qwen-vl".to_string();
    conf.ffmpeg = tools.join("ffmpeg").to_string_lossy().into_owned();
    settings::save(&paths, &conf).expect("the conf points at the fakes");
    std::env::set_var("XDG_CONFIG_HOME", &config);
}

/// Plant F1.6's output for one lane: frames on the interval, `scenes.tsv`, and the `.frames` marker that
/// says the pass is DONE (`<grid>|<threshold>`), which makes the frames stage skip without a process.
fn seed_frames(tree: &Tree, seconds: &[f64], changes: &[f64]) {
    let dir = tree.frames_dir(LANE);
    std::fs::create_dir_all(&dir).expect("frames dir");
    for at in seconds {
        std::fs::write(dir.join(format!("{}.jpg", frame_name(*at))), b"jpeg").expect("the frame");
    }
    std::fs::write(tree.frames_marker(LANE), "0.25|1.0\n").expect("the marker");
    let rows: String = changes.iter().map(|at| format!("{at}\t5.0\n")).collect();
    std::fs::write(tree.scenes_tsv(LANE), rows).expect("scenes.tsv");
}

#[test]
fn f1_7_s6_pressing_play_writes_events_tsv_and_the_state() {
    // The LLM handle stays alive for the whole binary: every check presses ▶ and needs it answering.
    let llm = FakeLlm::start(events_answer(FRAMES_PER_REQ));
    point_settings_at(&fake_audio_server(), &llm.url, &stubs());
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        check_the_press_writes_the_two_files(app);
        RAN_FILES.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);

    assert!(RAN_FILES.load(Ordering::SeqCst), "the file check never ran");
}

/// The press's own work, read off disk: one `events.tsv` row per described frame, the STATE the model
/// set, scaled copies that did not exist before, and the closing sentence in the window log.
fn check_the_press_writes_the_two_files(app: &adw::Application) {
    let (_keep, tree) = pin_session("files");
    // Four frames at 1 s over a 4-second take: exactly one chunk of P.machine.describeFramesPerReq.
    seed_frames(&tree, &[0.0, 1.0, 2.0, 3.0], &[]);
    assert!(
        !tree.llm_frames_dir(LANE).join(format!("{}.jpg", frame_name(0.0))).exists(),
        "nothing is pre-scaled by the test: the program must make the copies"
    );

    let model = Project {
        interval: 1.0,
        sources: vec![footage("project:sources/lecture.mkv")],
        ..Project::default()
    };
    let window = ui::build_window(app, &model, "Prepare");
    window.set_default_size(1400, 900);
    window.present();

    ui::play_button(&window).expect("the run bar has a ▶").emit_clicked();
    let logs = ui::window_logs();

    // The two files S6 owns, read through the same doors the logic test reads them through.
    let rows: Vec<String> = std::fs::read_to_string(tree.events_tsv(LANE))
        .expect("the press wrote events.tsv")
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    assert_eq!(rows.len(), FRAMES_PER_REQ, "one row per described frame: {rows:?} / {logs:?}");
    for row in &rows {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields.len(), 3, "§6's three fields: {row}");
        assert!(
            fields[0].parse::<f64>().is_ok() && fields[1].parse::<f64>().is_ok(),
            "both times are numbers: {row}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(tree.describe_state(LANE)).expect("the press wrote state.txt").trim(),
        "mid-lecture, board half full",
        "the running STATE is what the model set"
    );
    // S1's copies exist now, named by their second, and they are what the request sent.
    let copies: Vec<String> = std::fs::read_dir(tree.llm_frames_dir(LANE))
        .expect(".llmframes after the press")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".jpg"))
        .collect();
    assert_eq!(copies.len(), FRAMES_PER_REQ, "one scaled copy per frame: {copies:?}");
    assert!(
        logs.iter().any(|line| line.contains("event log complete")),
        "the pass closed in the log the person reads: {logs:?}"
    );
    window.close();
}
