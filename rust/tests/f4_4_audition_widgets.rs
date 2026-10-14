//! F4.4's OTHER half over a socket and a real child process: a row's ▶ that ends with a take must put
//! that take on a PLAYER, at the level the render would give it, and a take already on disk must be
//! replayed without paying audio.cpp a second time.
//!
//! The click is `line-speak-0`'s own `clicked` emission; the TTS legs cross a loopback socket to a fake
//! audio.cpp; the players are real children of a shell script that logs the argv it was handed. That seam
//! is the app's own — `P.eng.voicePlayer` / `P.eng.picturePlayer` resolve Settings → `NAIVEPOST_VOICE_PLAYER`
//! / `NAIVEPOST_PICTURE_PLAYER` → `gst-play-1.0`, so nothing here is a test-only hook.
//!
//! What is pinned: spec/07-narrate.md S3 + inventory/narrate.md §C.4 ("sound equals the render's: whole clip
//! ducked by game volume when it speaks", "audition from lead-in … and carry on down the cut"), and B.2's
//! cache rule (a take already in `narrate/tts/` costs nothing).
//!
//! One scenario per binary (cwd and XDG are process-wide), one application, one `connect_activate`, every
//! fact staged inside `run_round` and asserted by a `#[test]` — the shape `tests/speak_tts_wire_widgets.rs`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)]
mod common;
use common::{hold_last_window, narrate_button, release_last_window, settle, status_text};

#[path = "speak_tts_wire_harness.rs"]
mod harness;
use harness::{FakeAudio, Script};

/// The game level saved into `naivepost.json`. `narrate_preview_leg::game_volume_for` reads it off the
/// project file, so the number the player is asked for travels the app's own route.
static GAME_VOLUME: f64 = 0.4;

/// The scenario's entry, shared by the seeding code and by the assertions so both mean one line.
fn entry() -> Entry {
    Entry {
        s: 0.0,
        e: 20.0,
        at: 5.0,
        text: "The chain moved to proof of stake.".into(),
        emotion: "angry=1".into(),
        pos: String::new(),
        roll: 0,
    }
}

/// The lead-in the press is expected to cue from: `narrate_screen::audition` hands `Audition::Speak` this
/// very number (`at - P.eng.narrationRunInSeconds`, floored at the previous line's end, which is 0 here).
fn lead_in() -> f64 {
    (entry().at - naivepost::narrate_screen::AUDITION_LEAD_SECONDS).max(0.0)
}

/// One player spawn as the recorder logged it: `<tag> <file> start=<s> gain=<g>`.
fn arg_of(line: &str, key: &str) -> f64 {
    let arg = line
        .split_whitespace()
        .find(|arg| arg.starts_with(key))
        .unwrap_or_else(|| panic!("the player call carries its {key}: {line}"));
    arg.trim_start_matches(key)
        .parse::<f64>()
        .unwrap_or_else(|why| panic!("{key} parses as a number in {line}: {why}"))
}

/// The scenario's facts. `run_round` fills it once and asserts the waits inside itself, so a press that
/// produced no player aborts the scenario where it happened instead of leaving a test to read nothing (a
/// panic cannot unwind through libgio's activate trampoline, which is why the waits are not in the tests).
struct Stage {
    status_after_first: String,
    speech_after_first: usize,
    speech_after_second: usize,
    take: PathBuf,
    bytes_after_first: Vec<u8>,
    bytes_after_second: Vec<u8>,
    /// The voice / picture call each press ended with — the audition's heard half, straight off the argv
    /// the recorder logged (`<tag> <file> start=… gain=…`).
    voice_first: Vec<String>,
    picture_first: Vec<String>,
    /// Every voice line the scenario logged, so the replay check sees the same file named by both presses.
    voice_second: Vec<String>,
}

static STAGE: OnceLock<Stage> = OnceLock::new();
static RAN_FIRST: AtomicBool = AtomicBool::new(false);
static RAN_SECOND: AtomicBool = AtomicBool::new(false);

/// The session folder the page resolves its take into: `startup::session_dir(current_dir())`.
fn session_tree() -> Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    Tree::new(&dir).expect("the seeded session folder ends in .naivepost")
}

/// Write the recorder both player names point at: a real child that reports the argv it was handed and
/// exits, so no player outlives the test.
fn write_recorder(root: &Path, tag: &str, log: &Path) -> PathBuf {
    let script = root.join(tag);
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho \"{tag} $*\" >> {}\nexit 0\n", log.display()),
    )
    .expect("the recorder script is written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("the recorder is executable");
    }
    script
}

/// The recorder's lines with one tag, newest last.
fn tagged(log: &Path, tag: &str) -> Vec<String> {
    let prefix = format!("{tag} ");
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with(&prefix))
        .map(str::to_string)
        .collect()
}

/// The lines a press added: everything logged after the count taken before it was clicked.
fn new_lines(lines: &[String], before: usize) -> Vec<String> {
    lines.iter().skip(before).cloned().collect()
}

/// The last line of a list, as a one-element vec (empty when there was none).
fn last_of(lines: &[String]) -> Vec<String> {
    lines.last().cloned().into_iter().collect()
}

/// Poll until the recorder has logged more `tag` lines than `before`, then hand back the new ones. A spawn
/// is a real child writing to a file from its own process, so its line lands a moment after the call that
/// spawned it; bounded, so a missing spawn fails with the log it did write instead of hanging the suite.
fn poll_for_new(log: &Path, tag: &str, before: usize) -> Vec<String> {
    for _ in 0..400 {
        let fresh = new_lines(&tagged(log, tag), before);
        if !fresh.is_empty() {
            return fresh;
        }
        settle();
    }
    panic!(
        "no new {tag} player call reached the recorder; log holds {:?}",
        std::fs::read_to_string(log).unwrap_or_default()
    )
}

fn run_round(app: &adw::Application) {
    let root = std::env::temp_dir().join(format!("np-f44-audition-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("config root");
    std::fs::create_dir_all(root.join("data")).expect("data root");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }

    // The player seam, set before anything can spawn: both programs are the recorder.
    let log = root.join("players.log");
    let voice_program = write_recorder(&root, "voice", &log);
    let picture_program = write_recorder(&root, "picture", &log);
    unsafe {
        std::env::set_var("NAIVEPOST_VOICE_PLAYER", &voice_program);
        std::env::set_var("NAIVEPOST_PICTURE_PLAYER", &picture_program);
    }

    // The fake audio.cpp, and the address the program reads for itself out of the Settings file.
    let audio = FakeAudio::start(Script::healthy());
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    // Built from `Conf::default()` rather than a struct literal: `Conf` keeps an unpublished field, so the
    // model file's own idiom is the one that compiles here.
    let mut conf = Conf::default();
    conf.audio_server = audio.url();
    settings::save(&paths, &conf).expect("the fake address is what the Settings now say");

    let tree = session_tree();
    let mut project = Project {
        language: "pl".into(),
        ..Default::default()
    };
    project.produce.game_volume = GAME_VOLUME;
    project.sources.push(Source {
        path: "/media/take.mp4".into(),
        footage: true,
        ..Default::default()
    });
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    let cut = Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 20.0,
            dur: 20.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    naivepost::cut::save(&cut, &tree).expect("cut.json written");
    // F4.6's reference on disk (S1 passes) and NOTHING under narrate/tts/, so the first press synthesizes.
    let reference = tree.voice_ref_wav();
    std::fs::create_dir_all(reference.parent().expect("narrate/ has a parent")).expect("narrate/ made");
    std::fs::write(&reference, b"RIFF fake voice reference").expect("reference written");

    let entry = entry();
    // The take the PROGRAM files it under: the weighted tag becomes the 8-float vector name (§4).
    let expected = naivepost::narrate_tts::take_path(
        &tree,
        &naivepost::narration::key_for(&entry, Some("")),
    );

    release_last_window();
    let window = ui::build_window(app, &project, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(ui::NarrateState {
        entries: vec![entry.clone()],
        segs: cut.segs.clone(),
        takes: vec![],
        voice: String::new(),
        pitch: 0.0,
        session: entry.at,
        cut_at: entry.at,
        length: 20.0,
        clips: 1,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec!["warm.wav".into()],
        recording_covers_clip: true,
        playing: false,
        busy: false,
        narration_off: false,
        covered_spans: vec![],
        language: "pl".into(),
    });
    ui::refresh(&window);
    settle();

    let button = narrate_button(&window, "line-speak-0");

    // --- first press: no take on disk, so S4 speaks it and the press plays it ----------------------
    let voices_before_first = tagged(&log, "voice").len();
    let pictures_before_first = tagged(&log, "picture").len();
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    for _ in 0..40 {
        if expected.is_file() {
            break;
        }
        settle();
    }
    let status_after_first = status_text(&window);
    let bytes_after_first = std::fs::read(&expected).expect("the first press filed the take");
    let speech_after_first = audio
        .drain()
        .iter()
        .filter(|seen| seen.path == "/v1/audio/speech")
        .count();
    // Both players belong to this one press: the bed from the lead-in, and the take over it. Each is counted
    // against what the log held BEFORE the click, so another flow's spawn cannot satisfy the check.
    let voice_first = last_of(&poll_for_new(&log, "voice", voices_before_first));
    let picture_first = last_of(&poll_for_new(&log, "picture", pictures_before_first));
    assert!(
        !voice_first.is_empty() && !picture_first.is_empty(),
        "the first press put the take on a voice player over the cut's own picture: {:?}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );
    RAN_FIRST.store(true, Ordering::SeqCst);

    // --- second press: the take is on disk, so nothing is re-spoken and the wav still plays --------
    let voices_before_second = tagged(&log, "voice").len();
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    let voice_second = poll_for_new(&log, "voice", voices_before_second);
    assert!(
        !voice_second.is_empty(),
        "the warm press never put the take back on a player: {:?}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );
    let speech_after_second = speech_after_first
        + audio
            .drain()
            .iter()
            .filter(|seen| seen.path == "/v1/audio/speech")
            .count();
    let bytes_after_second = std::fs::read(&expected).expect("the take is still readable");
    RAN_SECOND.store(true, Ordering::SeqCst);


    let _ = STAGE.set(Stage {
        status_after_first,
        speech_after_first,
        speech_after_second,
        take: expected.clone(),
        bytes_after_first,
        bytes_after_second,
        voice_first,
        voice_second: tagged(&log, "voice"),
        picture_first,
    });
    window.close();
    settle();
}

/// Both `#[test]`s drive the same single scenario, and only one of them can be first: `call_once` releases
/// the loser as soon as the winner's closure returns, which for the SECOND test is the moment `activate`
/// finishes — before the other test has staged anything. So a test waits on its own flag instead of
/// assuming the sibling already got there.
fn scenario_running() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(drive_app);
}

/// Wait until `flag` says the scenario reached this point. Bounded, so a scenario that died earlier fails
/// with its own reason rather than hanging.
fn wait_for(flag: &AtomicBool, why: &str) {
    for _ in 0..4000 {
        if flag.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("{why}");
}

/// S4 + the audition: the row's ▶ put the take it spoke onto a real voice player at the ducked level, over
/// the cut's own picture cued at the line's lead-in.
#[test]
fn f4_4_s4_the_auditioned_take_reaches_a_real_player() {
    scenario_running();
    wait_for(&RAN_FIRST, "the first-press block never ran");
    let stage = STAGE.get().expect("the scenario staged its facts");
    let take_name = stage
        .take
        .file_name()
        .expect("the take has a file name")
        .to_string_lossy()
        .into_owned();

    assert!(
        stage.status_after_first.starts_with("spoken"),
        "the row's ▶ reported the take, not a refusal: {}",
        stage.status_after_first
    );
    assert_eq!(
        stage.speech_after_first, 1,
        "the first press really asked the speech endpoint once"
    );
    assert!(
        stage.take.is_file(),
        "the take landed at {}",
        stage.take.display()
    );

    let voice = stage
        .voice_first
        .iter()
        .find(|line| line.contains(&take_name))
        .unwrap_or_else(|| {
            panic!(
                "the take {take_name} never reached a voice player: {:?}",
                stage.voice_first
            )
        });
    // S3/inventory C.4: the take sits AT the game volume while it speaks — asserted against the leg's own
    // rule, never a literal, so a retuned floor cannot quietly pass.
    let wanted = naivepost::narrate_preview_leg::voice_gain(GAME_VOLUME);
    let got = arg_of(voice, "gain=");
    assert!(
        (got - wanted).abs() < 5e-4,
        "the take plays at the ducked game volume {wanted:.3}; the player was asked for {got:.3}: {voice}"
    );
    assert!(
        (arg_of(voice, "start=") - 0.0).abs() < 5e-4,
        "an auditioned take plays from its first word, not mid-wav: {voice}"
    );

    let picture = stage
        .picture_first
        .first()
        .expect("the cut's own picture player was spawned too");
    let start = arg_of(picture, "start=");
    assert!(
        (start - lead_in()).abs() < 5e-4,
        "the audition carried on down the cut from its lead-in {:.3}, not {start:.3}: {picture}",
        lead_in()
    );
}

/// The warm half: pressing a row whose take is already on disk replays it and spends NOTHING on audio.cpp
/// — the cache is the app's cost model, so a second call for the same bytes is a bug (B.2).
#[test]
fn f4_4_s4_a_take_already_on_disk_is_auditioned_without_a_second_call() {
    scenario_running();
    wait_for(
        &RAN_SECOND,
        "the second-press block never ran",
    );
    let stage = STAGE.get().expect("the scenario staged its facts");
    let take_name = stage
        .take
        .file_name()
        .expect("the take has a file name")
        .to_string_lossy()
        .into_owned();

    assert_eq!(
        stage.speech_after_second, stage.speech_after_first,
        "the second press dialled /v1/audio/speech again: {} calls after one press, {} after two",
        stage.speech_after_first, stage.speech_after_second
    );
    assert_eq!(
        stage.bytes_after_first, stage.bytes_after_second,
        "the replay wrote nothing to the take"
    );
    let on_take: Vec<&String> = stage
        .voice_second
        .iter()
        .filter(|line| line.contains(&take_name))
        .collect();
    assert_eq!(
        on_take.len(),
        2,
        "both presses handed the SAME take to a player: {:?}",
        stage.voice_second
    );
}

/// One application, one scenario, both presses. Our own argv: handing cargo-test's flags to libgio aborts
/// before `activate` runs.
fn drive_app() {
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(run_round);
    app.run_with_args(&["naivepost"]);
}
