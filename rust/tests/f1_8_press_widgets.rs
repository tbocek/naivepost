//! F1.8 Fix the transcripts — a real ▶ press, spec/04-prepare.md.
//!
//! `tests/f1_8_fix_session.rs` walks the fix stage through `run_stages::drive_walk`, which proves
//! §S1's placement and §S5–S7's merge for real. What that cannot show is the wire from the button:
//! whether a person pressing `play-button` on a Prepare page reaches any of it. This binary presses it
//! and then reads the FILES — `offsets.tsv`, `session.tsv`, `session.txt` — through the same `Tree`
//! doors the logic test reads them through, rather than the plan lines the ▶ handler prints up front,
//! because a plan line only proves something was described as pending.
//!
//! The scene is a whole Prepare press, so every stage the plan builds has to get through without a
//! person: an audio.cpp on loopback so §S5's preflight says yes (without it the press is refused before
//! any stage runs); an LLM on loopback answering §3.2's vision calls with `record_event`/`set_state` and
//! §3.3's fix calls with `fix_line`/`finish`, told apart by which tool names appear in the ask; and a stub
//! ffmpeg/ffprobe folder named in the conf that CREATES the file its argv names, so decode, scaling and
//! scene-detect all finish. F1.6's `.frames` marker is planted for both recordings: the frames pass skips
//! with no process, the describe pass finds frames to send, and no stage re-writes the transcripts the
//! press is there to fix.
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
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf, Paths};
use naivepost::textfmt::{self, FrameEvent, Line};
use naivepost::ui;

/// The two recordings' stamped names, 90 s apart, so §S1's placement has a non-zero offset to report.
const VIDEO: &str = "2026-05-01 10-00-00_deck.mkv";
const AUDIO: &str = "2026-05-01 10-01-30_mic.wav";
/// The lanes those names file under: the stem of each name, which is what names every folder here.
const DECK: &str = "2026-05-01 10-00-00_deck";
const MIC: &str = "2026-05-01 10-01-30_mic";
/// Transcript rows per recording: fewer than P.machine.fixBlockLines (25), so each source is one block
/// and the whole press costs a known number of asks.
const LINES: usize = 4;
static RAN_FILES: AtomicBool = AtomicBool::new(false);
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// §3.3's answer for a block of `lines` lines: one `fix_line` each, then `finish`.
fn fix_answer(lines: usize) -> String {
    let mut calls: Vec<serde_json::Value> = (1..=lines)
        .map(|n| tool_call("fix_line", &serde_json::json!({"n": n, "text": format!("respelled line {n}")})))
        .collect();
    calls.push(tool_call("finish", &serde_json::json!({})));
    chat_answer(&calls)
}

/// §3.2's answer for a batch of ANY size: one `record_event` per frame, a `set_state`, then `finish`.
fn describe_answer(frames: usize) -> String {
    let mut calls: Vec<serde_json::Value> = (1..=frames)
        .map(|frame| {
            tool_call(
                "record_event",
                &serde_json::json!({"frame": frame, "text": format!("frame {frame} of the take")}),
            )
        })
        .collect();
    calls.push(tool_call("set_state", &serde_json::json!({"text": "mid-lecture, board half full"})));
    calls.push(tool_call("finish", &serde_json::json!({})));
    chat_answer(&calls)
}

/// One OpenAI-style tool call. `arguments` travels as a STRING of JSON, which is the shape §3 spells.
fn tool_call(name: &str, arguments: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({"function": {"name": name, "arguments": arguments.to_string()}})
}

/// The chat envelope a list of tool calls travels in.
fn chat_answer(calls: &[serde_json::Value]) -> String {
    serde_json::json!({"choices": [{"message": {"content": "", "tool_calls": calls}}]}).to_string()
}

/// The word only §3.3's tool surface carries: an ask naming `fix_line` wants the transcript answer,
/// every other ask is §3.2's picture one. One server, because §1 describes the machine as one LLM.
const FIX_MARK: &str = "fix_line";
struct FakeLlm {
    url: String,
    /// Held so both replies outlive the connection threads that read them; no test changes one here.
    #[allow(dead_code)]
    fix: String,
    #[allow(dead_code)]
    describe: String,
    asks: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl FakeLlm {
    fn start(fix: String, describe: String) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let asks = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let counter = asks.clone();
        let answer_fix = fix.clone();
        let answer_describe = describe.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let counter = counter.clone();
                let fix = answer_fix.clone();
                let describe = answer_describe.clone();
                std::thread::spawn(move || {
                    let body = read_ask(stream.try_clone().expect("clone"));
                    let answer = if body.contains(FIX_MARK) { &fix } else { &describe };
                    if serve(stream, answer).is_some() {
                        if let Ok(mut count) = counter.lock() {
                            *count += 1;
                        }
                    }
                });
            }
        });
        FakeLlm { url: format!("http://127.0.0.1:{port}"), fix, describe, asks }
    }

    /// The asks this server took. The vision reply that writes `events.tsv` lands on the connection's own
    /// thread, so a caller reading that file can wait for a count rather than race it.
    #[allow(dead_code)]
    fn waited(&self, count: usize) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
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

/// The request body, read off the wire so the tool names in it can choose the reply.
fn read_ask(stream: TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
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
    String::from_utf8_lossy(&body).into_owned()
}

fn serve(mut stream: TcpStream, answer: &str) -> Option<()> {
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

/// A fake audio.cpp on loopback (§S5 asks it before any stage runs) — the shape
/// `tests/f1_7_describe_press_widgets.rs` uses: healthy on `/health`, ASR + diarization on `/v1/models`.
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

/// The stub tools: `ffprobe` answers a duration, and `ffmpeg` CREATES whatever file its argv names —
/// the voice copy `-o` names, the scene-detect log, and `<last arg>/<basename of -i arg>` where the
/// describe scaling leg expects its copy — so every leg of the press that forks a process finishes.
fn stubs() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "np-f18p-tools-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the tools folder");
    let probe = dir.join("ffprobe");
    std::fs::write(&probe, "#!/bin/sh\necho \"20.0\"\n").expect("the probe stub");
    let script = dir.join("ffmpeg.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\ninput=''\nout=''\noutdir=''\nprev=''\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-i\" ]; then input=\"$a\"; fi\n  if [ \"$prev\" = \"-f\" ]; then out=\"$a\"; fi\n  outdir=\"$a\"\n  prev=\"$a\"\ndone\nif [ -n \"$out\" ]; then\n  case \"$out\" in\n    /*|*/*) mkdir -p \"$(dirname -- \"$out\")\" ;;\n  esac\n  printf pcm > \"$out\"\nelif [ -n \"$input\" ]; then\n  base=$(basename -- \"$input\")\n  case \"$outdir\" in\n    /*|*/*) mkdir -p \"$outdir\" ;;\n  esac\n  printf JPEG > \"$outdir/$base\"\n  [ -f \"$outdir/$base\" ]\nfi\n",
    )
    .expect("the ffmpeg stub");
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("it runs");
    let ffmpeg = dir.join("ffmpeg");
    std::fs::write(&ffmpeg, format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", script.display())).expect("the wrapper");
    for tool in [&probe, &ffmpeg] {
        std::fs::set_permissions(tool, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("it runs");
    }
    dir
}

/// Point the settings this process reads at all three fakes at once: audio.cpp (so §S5 says yes), the
/// LLM (so both the vision and the fix asks come here) and the stub ffmpeg (so every fork finishes).
/// One conf for the whole binary, because `XDG_CONFIG_HOME` belongs to the process.
fn point_settings_at(audio_url: &str, llm_url: &str, tools: &Path) {
    let config = std::env::temp_dir().join(format!("np-f18p-cfg-{}", std::process::id()));
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

/// A root with `<root>/session.naivepost/` made and cwd pinned to the root: the ▶ handler resolves the
/// project through `startup::session_dir(current_dir())` at press time, so the session has to be here.
/// Both recordings exist, and each already carries its transcript (§S0's marker says the ASR pass is
/// done, so the speech row must not overwrite the words the fix pass is there to respell) plus F1.6's
/// frames, marker included, so the frames pass skips and describe has frames to send.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f18p-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(dir.join("sources")).expect("session + sources folders");
    std::fs::write(dir.join("sources").join(VIDEO), b"mkv-bytes").expect("the recording");
    std::fs::write(dir.join("sources").join(AUDIO), b"wav-bytes").expect("the other recording");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    let tree = Tree::new(dir).expect("a .naivepost folder is a project");
    for lane in [DECK, MIC] {
        let rows: Vec<Line> = (0..LINES)
            .map(|index| Line {
                start: index as f64,
                end: index as f64 + 0.9,
                speaker: "S1".to_string(),
                text: format!("asr heard line {index} mispelled"),
            })
            .collect();
        textfmt::write_lines(&rows, &tree.transcript_tsv(lane)).expect("the transcript");
        // The speech row's own resume markers (§S0): with these four files present `transcribe::run`
        // skips every server — the decode (its existence is the marker), the ASR, the aligner and the
        // diarizer — so the press costs only the vision and fix asks this test is about. The decode is a
        // REAL (silent) wav: §F1.11 measures the envelope from it for real, and placeholder bytes would
        // stop the row before the fixer was ever asked.
        std::fs::write(tree.voice16k_wav(lane), silent_wav_bytes(LINES as f64)).expect("the decoded audio");
        std::fs::write(tree.words_json(lane), words_doc(false).expect("words.json")).expect("the ASR answer");
        std::fs::write(tree.words_aligned_json(lane), words_doc(true).expect("aligned"))
            .expect("the aligner's answer");
        // A BARE ARRAY, which is what `requests::read_turns` parses (`Vec<Turn>`, not a document).
        std::fs::write(
            tree.turns_json(lane),
            r#"[{"start_sample":0,"end_sample":64000,"speaker_id":0}]"#,
        )
        .expect("the diarizer's answer");
        let frames = tree.frames_dir(lane);
        std::fs::create_dir_all(&frames).expect("the frames folder");
        for at in [0.0, 1.0, 2.0, 3.0] {
            std::fs::write(frames.join(format!("{}.jpg", frame_name(at))), b"jpeg").expect("the frame");
        }
        std::fs::write(tree.frames_marker(lane), "0.25|1.0\n").expect("the frames marker: nothing to re-frame");
        std::fs::write(tree.scenes_tsv(lane), "").expect("scenes.tsv, no changes planted");
    }
    // The deck's event log: two picture events, which become EVENT rows on the merged timeline (§S5).
    let events = vec![
        FrameEvent { start: 1.0, end: 4.0, text: "a slide appears".to_string() },
        FrameEvent { start: 6.0, end: 8.0, text: "the speaker writes".to_string() },
    ];
    textfmt::write_events(&events, &tree.events_tsv(DECK)).expect("events.tsv");
    (root, tree)
}

/// `words.json` / `words.aligned.json`: four words 8000 samples (0.5 s at 16 kHz) apart, each with
/// `end_sample > start_sample` — a word whose times are flat is refused as unusable, and this file has
/// to read as an ASR that finished. `aligned` drops the `text` field, which is all that differs.
fn words_doc(aligned: bool) -> Result<String, serde_json::Error> {
    let words: Vec<serde_json::Value> = (0..LINES)
        .map(|index| {
            serde_json::json!({
                "word": format!("word{index}"),
                "start_sample": index * 8000,
                "end_sample": index * 8000 + 4000,
            })
        })
        .collect();
    let text: String = (0..LINES).map(|index| format!("word{index} one{index} ")).collect::<String>();
    let doc = if aligned {
        serde_json::json!({ "words": words })
    } else {
        serde_json::json!({ "text": text.trim(), "words": words })
    };
    serde_json::to_string(&doc)
}

fn recording(path: &str, footage: bool) -> Source {
    Source {
        path: path.to_string(),
        footage,
        narrator: 0,
        // ✂ off: the separation stage has no row, so the press carries no voice-split ask.
        sepvoice: false,
        tracks: vec![],
    }
}

#[test]
fn f1_8_s7_pressing_play_writes_session_txt_and_offsets() {
    // The LLM handle stays alive for the whole binary: every stage's ask comes here.
    let llm = FakeLlm::start(fix_answer(LINES), describe_answer(FRAMES_PER_REQ));
    point_settings_at(&fake_audio_server(), &llm.url, &stubs());
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        check_the_press_writes_the_session_files(app, &llm);
        RAN_FILES.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);

    assert!(RAN_FILES.load(Ordering::SeqCst), "the file check never ran");
}

/// A minimal mono 16 kHz `pcm_s16le` wav of `seconds` of silence: what a real decode leaves in
/// `voice16k.wav`, and what §F1.11's envelope reader needs to find there.
fn silent_wav_bytes(seconds: f64) -> Vec<u8> {
    let data = vec![0u8; (seconds * 16_000.0) as usize * 2];
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // one channel
    out.extend_from_slice(&16_000u32.to_le_bytes());
    out.extend_from_slice(&32_000u32.to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// The press's own work, read off disk through the same doors `tests/f1_8_fix_session.rs` reads
/// through: the placement file, the merged timeline carrying both sources, and the text Cut reads.
fn check_the_press_writes_the_session_files(app: &adw::Application, _llm: &FakeLlm) {
    let (_keep, tree) = pin_session("files");
    assert!(!tree.offsets_tsv().exists(), "nothing is planted: the press must write the placement itself");
    assert!(!tree.session_tsv().exists(), "and nothing planted the session timeline either");

    let model = Project {
        interval: 1.0,
        sources: vec![
            recording(&format!("project:sources/{VIDEO}"), true),
            recording(&format!("project:sources/{AUDIO}"), false),
        ],
        ..Project::default()
    };
    let window = ui::build_window(app, &model, "Prepare");
    window.set_default_size(1400, 900);
    window.present();

    // §S5's preflight, §S0's markers and §S3's asks all run behind this one call.
    ui::play_button(&window).expect("the run bar has a ▶").emit_clicked();
    let logs = ui::window_logs();
    assert!(
        logs.iter().any(|line| line.contains("fixing ") && line.contains("block 1/")),
        "the press asked the fixer for each source's block: {logs:?}"
    );
    assert!(
        logs.iter().any(|line| line.contains("session timeline:")),
        "the log says the timeline was written: {logs:?}"
    );

    // S1's file: one video against one audio, offset by the 90 s their names carry.
    let offsets = std::fs::read_to_string(tree.offsets_tsv()).expect("the press wrote offsets.tsv");
    let rows: Vec<&str> = offsets.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(rows.len(), 1, "one video against one audio: {offsets}");
    let fields: Vec<&str> = rows[0].split('\t').collect();
    assert_eq!(fields, vec![DECK, MIC, "-90.00"], "placed 90 s apart: {offsets}");

    // S5's file: rows from BOTH recordings and the deck's two picture events, sorted by start.
    let timeline =
        textfmt::read_session(&tree.session_tsv()).expect("session.tsv is readable");
    assert!(
        timeline.len() >= LINES * 2,
        "both recordings' words are on it: {timeline:?}"
    );
    let sources: Vec<&str> = timeline.iter().map(|row| row.source.as_str()).collect();
    assert!(sources.contains(&DECK) && sources.contains(&MIC), "rows from both sources: {timeline:?}");
    assert!(
        timeline.iter().any(|row| row.who == "EVENT"),
        "the deck's picture events are rows too: {timeline:?}"
    );
    assert!(
        timeline.windows(2).all(|pair| pair[0].start <= pair[1].start),
        "sorted by start: {timeline:?}"
    );

    // S7's file: the text Cut reads, one line per row, each naming its speaker.
    let session_txt = std::fs::read_to_string(tree.session_txt()).expect("the press wrote session.txt");
    assert!(!session_txt.trim().is_empty(), "session.txt has to be the whole text");
    assert!(
        session_txt.contains("S1:") && session_txt.contains("EVENT:"),
        "who said it is on every line: {session_txt}"
    );

    // §S5's own sentence, which is what a person sees that the merge happened at all.
    assert!(
        logs.iter().any(|line| line.contains("session timeline:")),
        "the log says the timeline was written: {logs:?}"
    );
    window.close();
}
