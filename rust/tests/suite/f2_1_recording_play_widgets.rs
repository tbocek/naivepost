//! F2.1 through the widgets, on top of what part 1 wired: a real click hands the press REAL filmed runs,
//! starts and stops a real media process (a fake player script pointed at through `NAIVEPOST_PLAYER`, the
//! same seam shape as a loopback fake server), and the button wears ⏸ while our preview runs.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: every check moves process-shared state
//! (the window's player slot, its cut slot, the cwd the session is resolved from) and one shared env var,
//! so they run in sequence in this binary alone — the arrangement `tests/suite/cut_play_recording_widgets.rs` uses.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::play_back;
use naivepost::project::Source;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_S3: AtomicBool = AtomicBool::new(false);
static RAN_ARGV: AtomicBool = AtomicBool::new(false);
static RAN_LABEL: AtomicBool = AtomicBool::new(false);
static RAN_S1: AtomicBool = AtomicBool::new(false);

/// The tape this round plays: two minutes of `lecture.mkv`, filmed whole.
const TAPE_END: f64 = 120.0;

fn main_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_s3_a_press_gives_the_window_real_filmed_runs(app);
            RAN_S3.store(true, Ordering::SeqCst);
            check_s2_cues_the_real_player_program_at_the_red_line(app);
            RAN_ARGV.store(true, Ordering::SeqCst);
            check_the_button_wears_the_pause_label(app);
            RAN_LABEL.store(true, Ordering::SeqCst);
            check_s1_switches_and_carries_on(app);
            RAN_S1.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

/// A root whose session folder holds a real footage file, with the cwd pinned there — that is where the
/// press resolves `project:sources/…` from, exactly as `tests/suite/f1_7_describe_press_widgets.rs` pins it.
fn pin_session(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("naivepost-f21w-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(dir.join("sources")).expect("session + sources folders");
    std::fs::write(dir.join("sources/lecture.mkv"), b"mkv-bytes").expect("the recording");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    root
}

/// The fake player: records each spawn's arguments as ONE line (so a cue and a later loudness re-cue are two
/// readable lines), answers TERM, and stays alive until then. This is the seam the program itself reads
/// (`play_back::player_program` → `NAIVEPOST_PLAYER`), so no test helper is consulted anywhere in `src/`.
fn fake_player(root: &Path) -> PathBuf {
    let log = root.join("player-argv.txt");
    let script = root.join("fake-player.sh");
    common::write_script(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s ' \"$@\" >> '{}'\nprintf '\\n' >> '{}'\ntrap 'exit 0' TERM INT\ni=0; while [ $i -lt 600 ]; do sleep 0.1; i=$((i+1)); done\n",
            log.display(),
            log.display()
        ),
    )
    .expect("the player stub");
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("it runs");
    std::env::set_var("NAIVEPOST_PLAYER", &script);
    script
}

/// What the fake player wrote down: one line per spawn, its arguments space-separated. `argv_of` returns those
/// lines; a check that wants ONE spawn's arguments splits the line it cares about.
fn argv_lines(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join("player-argv.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The arguments of ▶'s OWN cue, split back into words — the LAST gapless spawn in the log, because if a later
/// press re-cued, that is the cue the check below is about.
///
/// A loudness ask is a second spawn on purpose (F2.5 S2 moves the footage's sound by playbin's volume PROPERTY,
/// and this leg's only way to ask that property is to re-cue the same files at the same second), so once the
/// preview is playing the newest thing the player was asked for is a tick's `--volume` re-cue at the tick's own
/// tenth-of-a-second, not the press's cue. The two are told apart by what the app itself puts in the argv:
/// [`naivepost::play_back::command_for`] cues gapless (`--gapless`) and [`naivepost::play_back::volume_args`]
/// never does, so the cue is found by its own flag rather than by being last — which keeps this asserting the
/// press's cue without loosening it to "whatever the leg spawned most recently".
fn cue_argv_of(root: &Path) -> Vec<String> {
    let cue = argv_lines(root)
        .iter()
        .rev()
        .find(|line| line.split_whitespace().any(|arg| arg == "--gapless"))
        .cloned()
        .unwrap_or_default();
    cue.split_whitespace().map(str::to_string).collect()
}

/// A Cut-page window over a session whose tape is `project:sources/lecture.mkv`, whole and filmed.
fn cut_window(app: &adw::Application, tag: &str) -> (adw::ApplicationWindow, PathBuf) {
    let root = pin_session(tag);
    fake_player(&root);
    let mut model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    model.sources = vec![Source {
        path: "project:sources/lecture.mkv".into(),
        footage: true,
        narrator: 0,
        sepvoice: false,
        tracks: vec![],
    }];
    release_last_window();
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut, "the Cut tab is live with footage");
    ui::set_session_recordings(&[Recording { base: "lecture".into(), start: 0.0, end: TAPE_END }]);
    settle();
    (window, root)
}

/// S2/S3: the press got REAL runs, so it played instead of refusing, and the process it started is the
/// configured player asked for the session's own file at the red line. The old code handed an empty list
/// here, so this press could only ever answer "nothing was filmed".
fn check_s3_a_press_gives_the_window_real_filmed_runs(app: &adw::Application) {
    let (window, root) = cut_window(app, "runs");
    ui::set_playhead(&window, 0.0);
    let button = ui::play_recording_button(&window).expect("the Cut page's ▶");
    button.emit_by_name::<()>("clicked", &[]);
    settle();

    let player = ui::preview_player(&window);
    assert!(player.transport.playing, "with real runs the press plays, it does not refuse");
    assert!(
        play_back::playing_pid().is_some(),
        "and a real media process was started for it"
    );

    let args = cue_argv_of(&root);
    println!("PLAYER ARGV: {args:?}");
    assert!(
        args.iter().any(|arg| arg.ends_with("sources/lecture.mkv")),
        "the session's own footage went to the player: {args:?}"
    );
    let at = args.iter().position(|arg| arg == "-s").and_then(|i| args.get(i + 1));
    assert_eq!(at.map(String::as_str), Some("0.0"), "cued AT the red line: {args:?}");

    // The second press pauses: the transport clears and the process is stopped, not left running muted.
    // `play_back::halt` answers whether THIS leg had a process — the press above already halted the one it
    // started — so the state read is `playing_pid`, which is what "stopped" means here.
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(!ui::preview_player(&window).transport.playing, "the second click paused it");
    assert!(play_back::playing_pid().is_none(), "pausing stopped the player process");
    window.close();
    settle();
}

/// S2's red line again, further in: the `-s` the player is cued with is the line's second, not zero.
fn check_s2_cues_the_real_player_program_at_the_red_line(app: &adw::Application) {
    let (window, root) = cut_window(app, "line");
    ui::set_playhead(&window, 42.5);
    ui::play_recording_button(&window)
        .expect("▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let args = cue_argv_of(&root);
    let at = args.iter().position(|arg| arg == "-s").and_then(|i| args.get(i + 1));
    assert_eq!(at.map(String::as_str), Some("42.5"), "the cue names the red line: {args:?}");
    assert!(play_back::playing_pid().is_some(), "and it is a live process");
    play_back::halt();
    window.close();
    settle();
}

/// Inventory §D's three-▶ rule on THIS button: it wears ⏸ only while our preview runs, and comes back to
/// ▶ when it stops. The run bar's ▶ keeps its own icon swap; this face belongs to the preview.
fn check_the_button_wears_the_pause_label(app: &adw::Application) {
    let (window, _root) = cut_window(app, "label");
    let button = ui::play_recording_button(&window).expect("▶");
    assert!(
        button.label().is_some_and(|l| l.starts_with('\u{25b6}')),
        "at rest it says ▶: {:?}",
        button.label()
    );
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        button.label().is_some_and(|l| l.contains('\u{23f8}')),
        "while our preview runs it wears the pause glyph: {:?}",
        button.label()
    );
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        button.label().is_some_and(|l| l.starts_with('\u{25b6}')),
        "paused, it says ▶ again: {:?}",
        button.label()
    );
    play_back::halt();
    window.close();
    settle();
}

/// S1 through the widget: from the ✂ preview, ▶ switches back and CARRIES ON — the transport it found
/// playing stays playing, and the sentence is §D's. The window opens with the cut previewed (the ✂ face),
/// so `cut_only` is read back as false rather than poked: the press itself is what changes it.
fn check_s1_switches_and_carries_on(app: &adw::Application) {
    let (window, root) = cut_window(app, "switch");
    ui::set_preview_state(
        &window,
        true,
        false,
        naivepost::run::Transport { playing: true, started: true },
    );
    assert!(ui::preview_player(&window).cut_only, "the ✂ preview is showing to start with");
    let button = ui::play_recording_button(&window).expect("▶");
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    let after = ui::preview_player(&window);
    assert!(!after.cut_only, "the preview is the recording now");
    assert!(after.transport.playing, "\"already playing: carry on\" — it did not stop");
    assert!(
        common::status_text(&window).contains("preview is the recording again"),
        "and says so: {:?}",
        common::status_text(&window)
    );
    // S1 says "already playing: carry on", so the press that switched starts NO process — the one already
    // running is the one being carried on. A fresh stub log proves nothing was cued by this press.
    let args = argv_lines(&root);
    assert!(args.is_empty(), "carrying on cues nothing new, it keeps what runs: {args:?}");
    // And because ours is now the running preview, the button wears ⏸ rather than lying about ▶.
    assert!(button.label().is_some_and(|l| l.contains('\u{23f8}')), "the face followed the switch");
    play_back::halt();
    window.close();
    settle();
}

#[test]
fn f2_1_s3_a_press_gives_the_window_real_filmed_runs_and_starts_the_player() {
    main_round();
    assert!(RAN_S3.load(Ordering::SeqCst), "the runs half ran");
    assert!(RAN_ARGV.load(Ordering::SeqCst), "the cue half ran");
    assert!(RAN_LABEL.load(Ordering::SeqCst), "the label half ran");
    assert!(RAN_S1.load(Ordering::SeqCst), "the switch half ran");
}
