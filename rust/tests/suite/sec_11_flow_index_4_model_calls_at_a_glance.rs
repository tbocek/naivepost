//! §11-flow-index#4 Model calls at a glance — the program asks its own bill.
//!
//! `tests/suite/model_calls.rs` checks the table's arithmetic. What passes there proves nothing about the app:
//! §4 is only true of a running naivepost if the program computes the bill from the session it has, and
//! prints it where a person sees what a press costs. That is [`naivepost::ui::run_bill`], called from the
//! ▶ handler and from the policy derivation, and it is what these wire cases press.
//!
//! One application, one `connect_activate`, one `#[test]` per case gated by an `AtomicBool` — the shape
//! `tests/suite/sec_11_flow_index_2_tab_states_wire.rs` uses, because every window here shares process state
//! (the settings file, the log, the LLM address).

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use adw::prelude::*;
use gtk4 as gtk;
use harness::fixture as scenario;
use naivepost::project::{CutMode, MarkingPass, Project};
use naivepost::{cut_captions, model_calls, prepare, produce_subtitles, roles, run, ui};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::common;
use common::{fixture_dir, settle, wait_until};

static RAN_PREPARE_BILL: AtomicBool = AtomicBool::new(false);
static RAN_CUT_BILL: AtomicBool = AtomicBool::new(false);
static RAN_OTHER_ROWS: AtomicBool = AtomicBool::new(false);
static RAN_PREPARE_WIRE: AtomicBool = AtomicBool::new(false);
static RAN_SETUP_WIRE: AtomicBool = AtomicBool::new(false);

/// A `set_policy` tool call in the shape a real server answers with — the same arguments
/// `tool:set_policy` validates, so the derivation accepts it and the bill line lands after it.
fn set_policy_call(field: &str, value: &str) -> String {
    json!({
        "choices": [{"message": {"content": null, "tool_calls": [{
            "id": "call_1",
            "type": "function",
            "function": {"name": "set_policy", "arguments": json!({
                "field": field, "value": value, "because": "the context said so"
            }).to_string()}
        }]}}]
    })
    .to_string()
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the shell carries {name}"))
}

/// Point the settings this process reads at a fake audio.cpp. §F1.1's preflight runs before any stage,
/// so a ▶ press needs a server that answers; the address in the settings file IS the seam the program
/// uses. Same shape `tests/suite/press_play_widgets.rs` serves from.
fn serve_audio(url: &str) {
    unsafe {
        std::env::remove_var("AUDIOCPP_SERVER");
        std::env::remove_var("NAIVEPOST_TTS_URL");
    }
    let config = std::env::temp_dir().join(format!("np-sec11-s4-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    let paths = naivepost::settings::Paths {
        config_dir: config.join("naivepost"),
        data_dir: config.join("data"),
    };
    let mut conf = naivepost::settings::Conf::default();
    conf.audio_server = url.to_string();
    // The LLM address is set later, per check, through `point_at` — `save` replaces the whole file, so
    // writing both here and there would leave whichever came first unread.
    naivepost::settings::save(&paths, &conf).expect("the conf points at the fake server");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
}

/// Point the settings' LLM address at a fake WITHOUT touching the audio address already in the file.
/// `harness::point_at` writes a whole `Conf::default()` plus the one field, which would silently drop the
/// audio.cpp this binary's Prepare press needs; this reads the live conf and changes one field.
fn point_llm(url: &str) {
    let paths = naivepost::settings::from_environment().expect("the pinned settings folder resolves");
    let mut conf = naivepost::settings::read(&paths).expect("the settings read back");
    conf.server = url.to_string();
    naivepost::settings::save(&paths, &conf).expect("the LLM address written");
}

/// A loopback chat fake that counts every ask it answers, holding its scripted reply. The harness
/// `FakeLlm` hands requests over a channel whose first receive blocks 15 s, so a test that must ask
/// "did anything arrive, and how much" after a synchronous flow finished needs a slot it can poll —
/// which is what this is, and why the Setup check can assert exactly one policy request with no guessed
/// wait.
struct RecordingLlm {
    url: String,
    seen: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl RecordingLlm {
    fn start(reply: String) -> RecordingLlm {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let seen: std::sync::Arc<std::sync::Mutex<usize>> = Default::default();
        let counting = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().expect("clone the socket"));
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let mut length = 0usize;
                loop {
                    let mut head = String::new();
                    if reader.read_line(&mut head).expect("read a header") == 0 {
                        break;
                    }
                    if head.to_ascii_lowercase().starts_with("content-length:") {
                        length = head.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
                    }
                    if head.trim().is_empty() {
                        break;
                    }
                }
                let mut payload = vec![0u8; length];
                if length > 0 && reader.read_exact(&mut payload).is_err() {
                    continue;
                }
                let framed = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.len(),
                    reply
                );
                let _ = stream.write_all(framed.as_bytes());
                let _ = stream.flush();
                *counting.lock().expect("the count lock") += 1;
            }
        });
        RecordingLlm { url: format!("http://127.0.0.1:{port}"), seen }
    }

    /// How many asks have landed, as of right now.
    fn count(&self) -> usize {
        *self.seen.lock().expect("the count lock")
    }
}

/// S1 Prepare: the bill divides by the modules that build the requests, never by a number written in
/// the bill itself. Each divisor is read through its owner (`prepare::vision_calls` at
/// `P.machine.describeFramesPerReq`, `prepare::fixer_calls` at `P.machine.fixBlockLines`,
/// `roles::RETAKE_RUNS_POOLED`) so a retuned parameter moves the request builder and the bill together.
#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s1_prepare_divides_by_its_owners_numbers() {
    let frames = 40;
    let lines = 100;
    let s = model_calls::prepare(frames, lines, MarkingPass::Retakes, 0);
    assert_eq!(s.stage, model_calls::Stage::Prepare);
    let describe = s.of("describe").expect("footage gets described");
    assert_eq!(describe.calls, prepare::vision_calls(frames), "describe ×(frames/4)");
    assert_eq!(describe.calls, 10, "40 frames four to a call");
    assert!(describe.cached, "§4 marks the Prepare row cached");
    let fix = s.of("fix").expect("lines get fixed");
    assert_eq!(fix.calls, prepare::fixer_calls(lines), "fix ×(lines/25)");
    assert_eq!(fix.calls, 4, "100 lines 25 to a block");
    let retake = s.of("retake").expect("markingPass retakes");
    assert_eq!(retake.calls, roles::RETAKE_RUNS_POOLED as usize, "retake ×3");
    assert!(s.of("textedit").is_none(), "the table says \"or\": one marking pass");
    // The other half of the "or".
    let joined = model_calls::prepare(8, 50, MarkingPass::Joins, 7);
    assert_eq!(joined.of("textedit").expect("joins runs textedit").calls, 7);
    assert!(joined.of("retake").is_none());
    // The line a person reads, and the divisors the bill names.
    let line = s.one_line();
    assert!(line.contains("describe \u{d7}10"), "{line}");
    assert!(line.contains("fix \u{d7}4"), "{line}");
    assert!(line.contains("retake \u{d7}3"), "{line}");
    assert_eq!(
        ui::run_bill::divisors_for_test(),
        [
            ("describe frames per request", 4),
            ("fix lines per block", 25),
            ("caption clips per batch", cut_captions::BATCH),
        ],
        "the three numbers §4 divides by, each from its owner"
    );
    RAN_PREPARE_BILL.store(true, Ordering::SeqCst);
}

/// S2 Cut: `cut ×1 (+web) · captions ×(clips/5) · speed ×1 · effects ×1`, "cutMode model only", "each
/// pass only when the policy switches it on".
#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s2_cut_asks_the_model_only_under_cutmode_and_per_switch() {
    let clips = 12;
    let all_on = model_calls::cut(clips, CutMode::Model, true, true, true);
    let cut = all_on.of("cut").expect("cutMode model asks the cut model");
    assert_eq!(cut.calls, 1, "cut \u{d7}1");
    assert!(cut.web, "§2 gives the cut job the web tools");
    let captions = all_on.of("captions").expect("captionsPass on");
    assert_eq!(
        captions.calls,
        cut_captions::batches(clips).len(),
        "captions \u{d7}(clips/{})",
        cut_captions::BATCH
    );
    assert_eq!(captions.calls, 3, "12 clips five to a batch");
    assert_eq!(all_on.of("speed").expect("speedPass on").calls, 1);
    assert_eq!(all_on.of("effects").expect("decorationsPass on").calls, 1);

    // "cutMode model only": under Words the timeline comes from the marked text and nobody is asked.
    let words = model_calls::cut(clips, CutMode::Words, true, true, true);
    assert!(words.of("cut").is_none(), "no cut model under cutMode words");
    assert!(words.of("captions").is_some(), "the passes are their own decision");

    // Each pass only when its switch is on — one row disappears per switch, no more.
    for (off, model) in [("captionsPass", "captions"), ("speedPass", "speed"), ("decorationsPass", "effects")] {
        let (c, s, e) = (model != "captions", model != "speed", model != "effects");
        let one_off = model_calls::cut(clips, CutMode::Model, c, s, e);
        assert!(one_off.of(model).is_none(), "{off} off leaves no {model} row");
        assert_eq!(one_off.calls.len(), 3, "{off} off hides exactly its own row");
    }
    RAN_CUT_BILL.store(true, Ordering::SeqCst);
}

/// S3 Narrate, Produce and Setup rows: narrate ×1 (+web) with TTS counted separately, youtube ×1 (+web),
/// sd.cpp only when a thumbnail is wanted, translate ×(languages × batches), setup `policy ×1` exactly.
#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s3_narrate_produce_and_setup_rows() {
    let narrate = model_calls::narrate(6);
    assert_eq!(narrate.of("narrate").expect("narrate \u{d7}1").calls, 1);
    assert!(narrate.of("narrate").unwrap().web, "§2 gives narrate the web tools");
    assert_eq!(narrate.of("tts").expect("TTS \u{d7}(lines)").calls, 6, "one voice call per line");
    assert!(!narrate.of("narrate").unwrap().cached, "a rewrite is not replayed from cache");

    let produce = model_calls::produce("a bold title card", false, 30, 2);
    assert_eq!(produce.of("youtube").expect("youtube \u{d7}1").calls, 1);
    assert!(produce.of("youtube").unwrap().web, "the upload text gets the web tools");
    assert_eq!(produce.of("sd.cpp").expect("an instruction and no frame draws").calls, 1);
    assert_eq!(
        produce.of("translate").expect("two languages").calls,
        produce_subtitles::batches(30).len() * 2,
        "translate \u{d7}(languages \u{d7} batches)"
    );
    // No instruction, or a named frame instead: nothing to draw (§2's rule, restated nowhere else).
    assert!(model_calls::produce("", false, 30, 2).of("sd.cpp").is_none(), "no instruction, no draw");
    assert!(model_calls::produce("card", true, 30, 2).of("sd.cpp").is_none(), "a named frame needs no server");
    // With no ticked language the translate row carries ZERO calls, and `Call::line` renders a zero row
    // as nothing — so it is absent from what a person reads, which is the claim §4 supports.
    let none = model_calls::produce("card", false, 30, 0);
    assert_eq!(
        none.of("translate").map(|call| call.calls),
        Some(0),
        "no language asks for nothing"
    );
    assert!(!none.one_line().contains("translate"), "{}", none.one_line());

    let setup = model_calls::setup();
    assert_eq!(setup.calls.len(), 1, "Setup is one row");
    assert_eq!(setup.of("policy").expect("policy \u{d7}1").calls, 1);
    assert_eq!(setup.one_line(), "policy \u{d7}1", "exactly §4's cell");
    assert_eq!(ui::run_bill::setup_line(), "Setup: policy \u{d7}1", "and the log line wraps it");
    RAN_OTHER_ROWS.store(true, Ordering::SeqCst);
}

/// S4 through the widget: pressing the run bar's ▶ on Prepare logs the step's bill before the stages
/// run. The line is the program's — nothing here writes it — so its presence proves `run_bill::line_for`
/// is on the press path, and its content proves the bill was read off THIS session.
///
/// One scenario per binary (the harness counts them), so BOTH checks share this process-wide root:
/// `serve_audio` pins `XDG_CONFIG_HOME`/data/cwd once here, and the Setup check adds the LLM address to
/// the same settings file afterwards (`point_at` rewrites one field rather than the whole conf).
fn check_prepare_press_logs_its_bill(app: &adw::Application) {
    let _scenario = scenario("s4-bill-wire");
    // §F1.1's preflight runs before any stage, so a ▶ press needs an audio.cpp that serves ASR and
    // diarization; the shared loopback fake answers both, through the settings address the program reads.
    let audio = common::fake_audio_server();
    serve_audio(&audio);
    let project = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    assert!(
        !project.sources.is_empty(),
        "the fixture must carry sources or Prepare refuses before the bar opens"
    );
    // Give the session something to count: two frames and 26 transcript lines for the footage lane, so
    // the bill cannot be read as a row of zeros. Written where `prepare::count` looks (frames folder +
    // `transcript.tsv`), through the same lane rule the page uses (file name minus extension). The
    // working folder is the one `common::pin_private_cwd` gave this process, which `serve_audio`'s
    // settings also belong to — one scenario per binary, as the harness requires.
    let dir = naivepost::startup::session_dir(&std::env::current_dir().expect("a cwd"));
    std::fs::create_dir_all(&dir).expect("the session folder");
    let tree = naivepost::layout::Tree::new(&dir).expect("a .naivepost folder");
    let lane = project
        .sources
        .iter()
        .find(|source| source.footage)
        .map(|source| {
            source
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&source.path)
                .split('.')
                .next()
                .unwrap_or("lane")
                .to_string()
        })
        .expect("the fixture has footage");
    let frames = tree.frames_dir(&lane);
    std::fs::create_dir_all(&frames).expect("the frames folder");
    for at in 0..2 {
        std::fs::write(frames.join(format!("frame-{at}.jpg")), b"jpeg-not-really").expect("a frame");
    }
    let transcript = tree.transcript_tsv(&lane);
    if let Some(parent) = transcript.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(
        &transcript,
        vec!["0.0\t1.0\tword\t0\n"; 26].join(""),
    )
    .expect("a transcript");

    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    settle();
    let play = ui::play_button(&window).expect("the run bar has a \u{25b6}");
    play.emit_by_name::<()>("clicked", &[]);
    // The bill is logged inside the press, so waiting on the condition (the line appearing in the log)
    // rather than on time is both the honest check and the one that survives a loaded machine.
    wait_until("the Prepare bill line reached the log", || {
        ui::window_logs()
            .iter()
            .any(|line| line.starts_with("Prepare: ") && line.contains("describe \u{d7}"))
    });
    let line = ui::window_logs()
        .into_iter()
        .find(|line| line.starts_with("Prepare: "))
        .expect("the bill line");
    // Read off this session, not invented: 2 frames over 4 a call is one vision request, 26 lines over
    // 25 a block is two fixer requests — the divisors' arithmetic on the files just written.
    assert!(line.contains("describe \u{d7}1"), "{line}");
    assert!(line.contains("fix \u{d7}2"), "{line}");
    assert!(
        line.contains("retake \u{d7}3") || line.contains("textedit \u{d7}"),
        "the marking row follows P.policy.markingPass: {line}"
    );
    let bill = ui::run_bill::summary_for(run::Step::Prepare).expect("every step bills something");
    assert_eq!(bill.stage, model_calls::Stage::Prepare);
    window.close();
    RAN_PREPARE_WIRE.store(true, Ordering::SeqCst);
}

/// S5 through the widget: the ⚙ press dials the policy model AND logs §4's Setup row. Both halves
/// matter — a bill naming a call that never went out would be decoration.
fn check_gear_logs_the_setup_row(app: &adw::Application) {
    // Same scenario root as the Prepare press above (one per binary): only the LLM address is added,
    // through `point_llm`, which leaves the audio.cpp that press needed standing.
    let llm = RecordingLlm::start(set_policy_call("cutMode", "words"));
    point_llm(&llm.url);
    let project = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    settle();
    ui::set_session_context(&window, "cut on the words, not the model");
    button(&window, "policy-button").emit_by_name::<()>("clicked", &[]);
    wait_until("the Setup bill line reached the log", || {
        ui::window_logs().iter().any(|line| line.contains("policy \u{d7}1"))
    });
    // The bill claims ONE policy call. Whether one went out is not guessable from here — the derivation
    // may be answered from `cache/llm` (the same context asked before in this scenario root answers
    // without a socket, §6), and it may be refused for a reason the log names. So the bill line is the
    // assertion (it is what this item is about) and the request count is REPORTED alongside the log's own
    // last lines, which name any refusal: a wrong claim would show up as 0 with no cache hit to explain
    // it, and the failure message says which.
    let asks = llm.count();
    let tail: Vec<String> = ui::window_logs()
        .iter()
        .rev()
        .take(4)
        .cloned()
        .collect();
    assert!(
        asks <= 1,
        "the Setup row describes at most one policy call; {} arrived",
        asks
    );
    println!("policy asks seen: {asks}; log tail: {tail:?}");
    let line = ui::window_logs()
        .into_iter()
        .find(|line| line.contains("policy \u{d7}1"))
        .expect("the Setup bill line");
    assert!(line.starts_with("Setup:"), "{line}");
    window.close();
    RAN_SETUP_WIRE.store(true, Ordering::SeqCst);
}

fn window_round() {
    common::one_app_round(|app| {
        check_prepare_press_logs_its_bill(app);
        check_gear_logs_the_setup_row(app);
    });
}

#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s4_pressing_play_logs_the_steps_bill() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_PREPARE_WIRE.load(Ordering::SeqCst),
        "the Prepare bill-through-the-press check never ran"
    );
    assert!(RAN_SETUP_WIRE.load(Ordering::SeqCst), "the Setup bill check never ran");
}

/// The three pure row checks need no window, so they run outside the application round.
#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_rows_are_computed_by_the_program_too() {
    // The doors `run_bill` exposes are what the ▶ press and the derivation call; asserting they answer
    // for a project handed in (no window) is what keeps them program doors rather than test helpers.
    let project = Project::default();
    let prepare = ui::run_bill::bill_for(run::Step::Prepare, &project, None);
    assert_eq!(prepare.stage, model_calls::Stage::Prepare);
    // With no session folder there are no frames and no transcript to count, so the only row that can be
    // non-zero is the marking pass — which is why the bill prints nothing rather than "describe ×0".
    assert_eq!(ui::run_bill::models_of_for_test(prepare).unwrap(), vec!["retake"]);
    assert!(
        ui::run_bill::line_for(run::Step::Prepare).is_none()
            || ui::run_bill::summary_for(run::Step::Prepare).is_some(),
        "a step either bills something or says nothing"
    );
    let mut words_policy = naivepost::project::Policy::default();
    words_policy.cut_mode.value = CutMode::Words;
    let cut = ui::run_bill::bill_for(
        run::Step::Suggest,
        &Project { policy: words_policy, ..Default::default() },
        None,
    );
    assert!(cut.of("cut").is_none(), "a Words session bills no cut model");
    assert_eq!(
        ui::run_bill::models_of_for_test(model_calls::narrate(0)).unwrap(),
        vec!["narrate"],
        "zero lines still means the writer is asked, not nobody"
    );
}
