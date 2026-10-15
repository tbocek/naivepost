//! F4.1 ▶ Write and speak — the WIRE. The rules live in `naivepost::narrate_run` and are pinned by
//! `tests/narrate_write_and_speak.rs`; this file proves the run bar's ▶ on a real Narrate page reaches
//! them: the four refusals land on the status line without touching a file, and a scripted narration
//! reply lands in `narrate/narration.json` with the previous generation kept, the log lines spoken and
//! the bar's stage sequence moved.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (cwd, the window's session slots), so they run in one ordered round and each sets a flag the
//! test asserts at the end.
//!
//! Every step goes through the shell's real ▶ (`play-button`, `emit_clicked`) — the seam
//! `ui::press_narrate_run_with_reply` exists for the logic tests, not for this file. A written reply
//! is handed to the button through `ui::set_narrate_script`, which stands in for F4.2's endpoint
//! (headless there is none); the CLICK itself is real, and so is the refusal path, which is answered
//! before the bar opens.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Cut, Seg};
use naivepost::layout::Tree;
use naivepost::narrate_run::{self, NO_SESSION_TIMELINE};
use naivepost::narration::{self, Entry, Narration};
use naivepost::project::{Project, Source};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fake_audio_server, hold_last_window, release_last_window, settle, status_text};

static RAN_NO_NARRATION: AtomicBool = AtomicBool::new(false);
static RAN_NO_TIMELINE: AtomicBool = AtomicBool::new(false);
static RAN_NO_CUT: AtomicBool = AtomicBool::new(false);
static RAN_WRITTEN_AND_SPOKEN: AtomicBool = AtomicBool::new(false);
static RAN_CAPTIONS_ONLY: AtomicBool = AtomicBool::new(false);
static RAN_STAGE_SEQUENCE: AtomicBool = AtomicBool::new(false);
static RAN_BLANK_LINE: AtomicBool = AtomicBool::new(false);
static RAN_PULSED: AtomicBool = AtomicBool::new(false);
static RAN_STOPPED: AtomicBool = AtomicBool::new(false);

static HALF: std::time::Duration = std::time::Duration::from_millis(600);
static SHORT: std::time::Duration = std::time::Duration::from_millis(1);

/// The project folder the run resolves: `startup::session_dir(current_dir())`. The test pins cwd to its
/// own temp root first, so this never points into `rust/`.
fn session_tree() -> Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    Tree::new(&dir).expect("the seeded session folder ends in .naivepost")
}

/// A project with one footage source, so no tab is locked (`shell::lock`) and the cut has something to
/// be laid out from. `no_narration` drives the F4.8 refusal.
fn base_project(no_narration: bool) -> Project {
    let mut project = Project { no_narration, ..Default::default() };
    project.sources.push(Source {
        path: "/media/take.mp4".into(),
        footage: true,
        ..Default::default()
    });
    project
}

/// Two clips the run can write for. Kept at two so "N/M" reads unambiguously in the stage assertion.
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

/// Seed the session folder: `naivepost.json`, the cut, and optionally `transcript/session.tsv`. Written
/// through the same writers the app uses, so paths and modes match a real project.
/// Seed the session folder. Every block starts from a clean slate: files left by the block before would
/// otherwise satisfy a gate this block means to prove closed (a kept `session.tsv` hides the missing-
/// timeline refusal, a kept `narration.prev.json` hides whether THIS run made one).
fn seed(project: &Project, cut: &Cut, with_timeline: bool) -> Tree {
    let tree = session_tree();
    let _ = std::fs::remove_dir_all(tree.dir());
    std::fs::create_dir_all(tree.dir()).expect("session folder");
    naivepost::project::save(project, tree.dir()).expect("naivepost.json written");
    cut::save(cut, &tree).expect("cut.json written");
    if with_timeline {
        tree.write_file(
            std::path::Path::new("prepare/transcript/session.tsv"),
            b"0\t0\t0\thello\n",
        )
        .expect("session.tsv written");
    }
    tree
}

/// The lines block 4 starts with: clip 1 already written, clip 2 not. With `narrationRewrite` off (the
/// default) that leaves exactly ONE clip for the run, which is what makes its reply numbering testable --
/// a batch of one, where clip 1 means the unwritten clip rather than the first of the cut.
fn old_entry() -> Vec<Entry> {
    vec![Entry {
        s: 0.0,
        e: 10.0,
        text: "the OLD first line".into(),
        emotion: "calm".into(),
        ..Default::default()
    }]
}

/// Build a window on the Narrate page whose live session is `project`, with `entries` as the lines the
/// page shows. The entries are passed in rather than read back from disk because blocks 2 and 3 seed no
/// record at all — reading there would publish an empty list and clip 2's line would have no row to fold
/// into. Then drop any earlier window.
fn narrate_window(
    app: &adw::Application,
    project: &Project,
    entries: Vec<Entry>,
) -> adw::ApplicationWindow {
    release_last_window();
    let window = ui::build_window(app, project, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    // The rows have to exist for S2's pull to read them: publish the page's state and refresh, which is
    // what a flow does when it pushes what the page shows.
    ui::set_state(ui::NarrateState {
        entries,
        segs: two_clips(),
        takes: vec![],
        voice: String::new(),
        pitch: 0.0,
        session: 5.0,
        cut_at: 5.0,
        length: 20.0,
        clips: 2,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec![],
        recording_covers_clip: true,
        playing: false,
        started: false,
        busy: false,
        narration_off: project.no_narration,
        // F4.4's inputs, left at "nothing checked": this file drives the F4.1 run, not the speak flow.
        language: String::new(),
        // F4.5's coverage spans: unpublished, read as full coverage of the cut.
        covered_spans: Vec::new(),
    });
    ui::refresh(&window);
    settle();
    window
}

/// Click the shell's own ▶ (F4.1's button on this page) and report what it put on the status line.
fn click_play(window: &adw::ApplicationWindow) -> String {
    ui::play_button(window)
        .expect("the run bar has a ▶")
        .emit_clicked();
    status_text(window)
}

/// Feed the narration reply the run would otherwise ask F4.2 for, then click ▶. Headless there is no
/// endpoint to call, so `set_narrate_script` stands in for the model; the CLICK itself is real.
fn click_play_with_reply(
    window: &adw::ApplicationWindow,
    reply: &[(u32, f64, &str, &str)],
) -> String {
    ui::set_narrate_script(reply);
    assert!(
        ui::narrate_script_loaded(),
        "the script must be loaded or ▶ silently takes its no-model path"
    );
    click_play(window)
}

/// Point the Settings conf at `url` as audio.cpp's address, the seam `services::audio_endpoint` reads
/// first (§02-services#1 re-reads the conf per request, which is what makes this live). The env
/// overrides are cleared so a box-level `AUDIOCPP_SERVER` cannot answer for every press here.
/// Point the Settings conf at audio.cpp's address, leaving the LLM at an unused loopback port.
///
/// This is the seam the program itself uses — `services::audio_endpoint` re-reads `llm.conf` per request
/// (§02-services#1), so nothing here hands an answer to the code; S6 really telephones this address. The
/// env overrides are cleared first so a box-level `AUDIOCPP_SERVER` cannot answer for every press here,
/// and the audio address matters because S6 really dials the synth: pointed at the loopback fake it
/// refuses locally instead of hanging on an unreachable host. The LLM half goes to a dead port because
/// these blocks never telephone a writer (they use `ui::set_narrate_script`).
fn point_audio_at(url: &str) {
    unsafe {
        std::env::remove_var("LLM_SERVER");
        std::env::remove_var("NAIVEPOST_LLM_URL");
        std::env::remove_var("AUDIOCPP_SERVER");
        std::env::remove_var("NAIVEPOST_TTS_URL");
    }
    let config = std::env::temp_dir().join(format!("np-f41run-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    // `XDG_CONFIG_HOME` is set BEFORE the save: `settings::from_environment()` resolves it at the moment
    // of the call and `paths_from` prefers it over `$HOME/.config`, so the file written here is the one
    // the run reads. Nothing under `$HOME/.config` or `/etc/xdg` is touched — sibling test binaries in
    // this folder legitimately point `$HOME/.config` at their own fakes, and overwriting that file would
    // steal the endpoint from a test that names its address on purpose.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
    let paths = naivepost::settings::Paths {
        config_dir: config.join("naivepost"),
        data_dir: config.join("data"),
    };
    // Built through `Conf::default()` + field writes: `render` is line-ordered by key and `parse` keeps
    // what it sees last, so a `..Default::default()` struct literal can silently drop a value.
    let mut conf = naivepost::settings::Conf::default();
    conf.server = "http://127.0.0.1:1".to_string();
    conf.audio_server = url.to_string();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the audio fake");
}

fn run_round(app: &adw::Application) {
    // Each block reports where it starts, so a stall is visible in the log rather than looking like a
    // silent pass. (A GTK window whose activate handler never returns leaves `app.run()` holding the
    // main loop; closing every window is what lets it quit.)
    eprintln!("f41-block 1 start");
    // --- (1) narration off: refused, and NOTHING written --------------------------------------
    {
        // A cut stands: with none, the no-cut gate answers first and the narration-off sentence would
        // never be seen. §F4.1 S1's order is what (2) and (3) pin; this block pins the wording.
        let project = base_project(true);
        let tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
        let window = narrate_window(app, &project, Vec::new());
        let refused = ui::refuse_narrate_run(&project).expect("off means refused");
        assert_eq!(
            refused,
            "this video has no narration \u{2014} tick Narration at the top of this page to write one",
            "the F4.8 sentence, byte for byte"
        );
        let said = ui::press_narrate_run(&window);
        assert_eq!(said, refused, "the door returns what it refused with");
        assert_eq!(status_text(&window), refused, "and puts it on the status line");
        assert!(
            !tree.narration_prev_json().exists(),
            "a refusal writes no narration.prev.json"
        );
        assert!(
            !tree.narration_json().exists(),
            "a refusal writes no narration.json either"
        );
        RAN_NO_NARRATION.store(true, Ordering::SeqCst);
    }

    eprintln!("f41-block 2 start");
    // --- (2) no session timeline -------------------------------------------------------------
    {
        let project = base_project(false);
        seed(&project, &Cut { segs: two_clips(), ..Default::default() }, false);
        let window = narrate_window(app, &project, Vec::new());
        let said = click_play(&window);
        assert_eq!(said, NO_SESSION_TIMELINE);
        assert_eq!(status_text(&window), NO_SESSION_TIMELINE);
        RAN_NO_TIMELINE.store(true, Ordering::SeqCst);
    }

    eprintln!("f41-block 3 start");
    // --- (3) no cut --------------------------------------------------------------------------
    {
        let project = base_project(false);
        seed(&project, &Cut::default(), true);
        let window = narrate_window(app, &project, Vec::new());
        let said = click_play(&window);
        assert_eq!(said, naivepost::cut::NO_CUT_YET);
        assert_eq!(status_text(&window), naivepost::cut::NO_CUT_YET);
        RAN_NO_CUT.store(true, Ordering::SeqCst);
    }

    eprintln!("f41-block 4 start");
    // --- (4) everything present: written, prev kept, spoken -----------------------------------
    {
        let project = base_project(false);
        let tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
        let old = Narration {
            entries: vec![Entry {
                s: 0.0,
                e: 10.0,
                text: "the OLD first line".into(),
                emotion: "calm".into(),
                ..Default::default()
            }],
            silent: vec![naivepost::narration::Silent { s: 10.0, e: 20.0 }],
        };
        narration::save(&old, &tree).expect("an old record exists");
        let old_bytes = std::fs::read(tree.narration_json()).unwrap();

        // Toggle rewriting on so the run covers BOTH clips: off, only clip 2 would be asked about and
        // the reply's clip 1 would land on it (§F4.1 S4's "every clip when P.policy.narrationRewrite").
        let mut project = project;
        project.policy.narration_rewrite.value = true;
        naivepost::project::save(&project, tree.dir()).expect("toggle saved");
        let window = narrate_window(app, &project, old_entry());
        let said = click_play_with_reply(
            &window,
            &[
                (1, 0.0, "hello there", "calm"),
                (2, 1.5, "second line", "happy"),
            ],
        );
        assert_eq!(said, narrate_run::DONE_SPOKEN, "S7's spoken status, got {said}");
        assert_eq!(status_text(&window), narrate_run::DONE_SPOKEN);

        // S2: the previous generation holds the old bytes, untouched.
        let prev_bytes = std::fs::read(tree.narration_prev_json()).expect("prev kept");
        assert_eq!(prev_bytes, old_bytes, "narration.prev.json == the OLD narration.json");

        // S4: both lines landed, and the user's silence marker survived the rewrite.
        let after = narration::load(&tree).expect("the record reloads");
        let text: Vec<&str> = after.entries.iter().map(|e| e.text.as_str()).collect();
        assert!(text.contains(&"hello there"), "clip 1's line landed: {text:?}");
        assert!(text.contains(&"second line"), "clip 2's line landed: {text:?}");
        assert_eq!(after.entries.len(), 2, "one entry per clip, no leftovers: {text:?}");
        assert!(
            !text.contains(&"the OLD first line"),
            "rewriting replaced clip 1's old line: {text:?}"
        );
        assert!(after.is_silent(10.0, 20.0), "the silent list survived the run");

        // S3/S4: the log carries the reason line and the written count.
        let logs = ui::window_logs().join("\n");
        assert!(logs.contains(">>> narrate:"), "the reason line was logged:\n{logs}");
        assert!(
            logs.contains(">>> narration written for 2 clips"),
            "the written log line:\n{logs}"
        );
        RAN_WRITTEN_AND_SPOKEN.store(true, Ordering::SeqCst);
    }

    eprintln!("f41-block 5 start");
    // --- (5) captions-only voice: written, none spoken ---------------------------------------
    {
        let project = base_project(false);
        let tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
        naivepost::narrate_data::write_voice(&tree, naivepost::narrate_screen::CAPTIONS)
            .expect("voice.txt set to captions");
        let window = narrate_window(app, &project, Vec::new());
        let before = ui::window_logs().len();
        let said = click_play_with_reply(
            &window,
            &[(1, 0.0, "read not heard", ""), (2, 0.0, "also read", "")],
        );
        assert_eq!(said, narrate_run::captions_only_done(2), "got {said}");
        assert_eq!(status_text(&window), narrate_run::captions_only_done(2));
        let logs = ui::window_logs()[before..].join("\n");
        assert!(
            !logs.contains("    narrate:"),
            "captions only speaks nothing, so no speaking summary:\n{logs}"
        );
        // (A "synthesizing line N" line may still sit in the log from an earlier block's warm-up; what
        // this run must not do is report any speaking of its own.)
        RAN_CAPTIONS_ONLY.store(true, Ordering::SeqCst);
    }

    eprintln!("f41-block 6 start");
    // --- (6) the bar's stage sequence ran ----------------------------------------------------
    {
        let project = base_project(false);
        let _tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
        let window = narrate_window(app, &project, Vec::new());
        let _ = click_play_with_reply(
            &window,
            &[(1, 0.0, "one line", ""), (2, 0.0, "two line", "")],
        );
        assert_eq!(
            ui::narrate_stage(),
            narrate_run::writing(2, 2),
            "the bar finished on \"writing 2/2 clips\""
        );
        // And the progress widget itself carries that text -- proof it was painted, not just stored.
        let bar = find_named(&window, "run-progress")
            .and_then(|w| w.downcast::<gtk::ProgressBar>().ok())
            .expect("the run bar has a progress widget");
        assert_eq!(bar.text().as_deref(), Some(narrate_run::writing(2, 2).as_str()));
        assert!(bar.shows_text(), "the stage is shown, not hidden");
        RAN_STAGE_SEQUENCE.store(true, Ordering::SeqCst);
    }

    eprintln!("f41-block 8 start");
    // --- (8) S3's pulse: the bar moves while the model thinks, then shows the count -------------
    {
        let project = base_project(false);
        let tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
        // Toggle rewriting on so the reply covers BOTH clips: off, only the clip with no line yet
        // would be asked about (§F4.1 S4), and the second reply line would land nowhere — the bar would
        // then honestly read `writing 1/2`, which is what the first cut of this check tripped on.
        let mut project = project;
        project.policy.narration_rewrite.value = true;
        naivepost::project::save(&project, tree.dir()).expect("toggle saved");
        let window = narrate_window(app, &project, Vec::new());
        // The shell's ▶ stood a pulse for the run it opened (`window_run_press.rs`), so one is
        // already on the timer when the press reaches the narrate door. The thinking stage must keep
        // the bar moving, not set a fraction it does not have: `should_pulse` is still true here
        // because nothing has closed, which is exactly §F4.1 S3's "pulses every 150 ms until the
        // first clip closes".
        assert!(
            ui::should_pulse(true, 0.0),
            "a thinking run has nothing countable, so the pulse is the only face it can have"
        );
        let said = click_play_with_reply(
            &window,
            &[(1, 0.0, "pulsed line", ""), (2, 0.0, "second line", "")],
        );
        assert_eq!(said, narrate_run::DONE_SPOKEN, "the run still finished: {said}");
        // The door stood the shell's own timer rather than inventing a second one: `arm_pulse` is
        // `shell_beats`' single 150 ms beat (`runqueue::PROGRESS_PULSE`), the call the ▶ handler makes.
        assert!(
            ui::narrate_pulsed(),
            "the thinking stage stood the pulse instead of setting a 0.0 fraction"
        );
        // The stages this run painted, in order: thinking first, the count second — the bar streamed
        // rather than jumping in one step.
        assert_eq!(
            ui::narrate_stages(),
            vec![
                narrate_run::STAGE_THINKING.to_string(),
                narrate_run::writing(2, 2),
            ],
            "S3's two stages, thinking then the count"
        );
        // And the timer is gone once there was a countable reading: `should_pulse` answers false at
        // the fraction the writing stage carries, so `arm_pulse`'s beat breaks itself.
        let fraction = ui::run_queue(&window).expect("this window has a queue").borrow().fraction();
        assert!(fraction > 0.0, "the run left a countable fraction on the bar: {fraction}");
        // The countable reading exists, so `arm_pulse`'s beat ends itself on its next tick — that is
        // the timer's own rule (`should_pulse`), which the check asks rather than waiting on the clock.
        assert!(
            !ui::should_pulse(true, fraction),
            "countable work needs no pulse, so the timer ends itself: {fraction}"
        );
        // A pulse never outlives the run that stood it: the close hands the bar back through
        // `shell_beats::stop_pulse`, the same door ⏹ uses (F0.1's rule that an idle bar holds no timer).
        assert!(
            !ui::pulse_standing_for_test(),
            "no pulse timer outlives the run that stood it"
        );
        // The bar itself carries the count as its text — proof the second stage was painted, not just
        // stored, and that the needle was handed back to fractions.
        let bar = find_named(&window, "run-progress")
            .and_then(|w| w.downcast::<gtk::ProgressBar>().ok())
            .expect("the run bar has a progress widget");
        assert_eq!(bar.text().as_deref(), Some(narrate_run::writing(2, 2).as_str()));
        // Proof the count landed on the record this press wrote, not only on the bar.
        let after = narration::load(&tree).expect("the run saved the record");
        assert_eq!(
            after.entries.len(),
            2,
            "both lines written, so the count the bar showed had a record behind it"
        );
        RAN_PULSED.store(true, Ordering::SeqCst);
    }

    eprintln!("f41-block 7 start");
    // --- (7) a blank written line counts as neither spoken nor cached ------------------------
    {
        let project = base_project(false);
        let tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
        // F4.4 S1 asks whether a reference exists to clone before anything else, and S6 really asks it
        // now that the run dials the synth — so plant one (the way `sample_play_wire_widgets` does) and
        // let this run speak over an audio.cpp that cannot be reached. The refusal is logged per line;
        // what this block pins is the COUNTING: the blank line is neither spoken nor cached.
        let reference = tree.voice_ref_wav();
        std::fs::create_dir_all(reference.parent().expect("narrate/ has a parent"))
            .expect("narrate/ made");
        std::fs::write(&reference, b"RIFF fake voice reference").expect("reference written");
        // And an audio.cpp to answer it: the shared loopback fake, served over the Settings conf — the
        // seam `services::audio_endpoint` reads per request. It answers /health and /v1/models but has
        // no speech route, so the one real line is refused and named, and the blank stays uncounted.
        point_audio_at(&fake_audio_server());
        let window = narrate_window(app, &project, Vec::new());
        let before = ui::window_logs().len();
        let said = click_play_with_reply(
            &window,
            &[(1, 0.0, "   ", ""), (2, 0.0, "a real line", "")],
        );
        assert_eq!(said, narrate_run::DONE_SPOKEN);
        let logs = ui::window_logs()[before..].join("\n");
        assert!(
            logs.contains("    narrate: 0 line(s) spoken, 1 already in the cache")
                || logs.contains("    narrate: 0 line(s) spoken, 0 already in the cache"),
            "nothing was spoken and the blank counted as neither spoken nor cached:\n{logs}"
        );
        assert!(
            !logs.contains("    narrate: 1 line"),
            "a blank line is never counted as spoken:\n{logs}"
        );
        assert!(
            logs.contains("!!! narrate:"),
            "the line that could not be spoken says why, where it failed:\n{logs}"
        );
        RAN_BLANK_LINE.store(true, Ordering::SeqCst);
    }

    eprintln!("f41-block 9 start");
    // --- (9) ⏹ mid-run names the stage it caught (S7's `<stage> stopped`) --------------------------
    {
        let project = base_project(false);
        let tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
        let window = narrate_window(app, &project, Vec::new());
        // A reply that spends HALF per clip, so with two clips the run is still inside its FIRST one
        // when the thread below raises the switch HALF/2 after the press: `narration` is the stage ⏹
        // catches. The margin is what keeps this deterministic — if it ever mis-times, raise HALF; the
        // sentence asserted below is fixed and must not be relaxed.
        ui::set_narrate_script_slow(&[(1, 0.0, "one line", ""), (2, 0.0, "two line", "")], HALF);
        ui::set_narrate_script(&[]);
        // ⏹ cannot be pressed from inside this thread: the ▶ handler runs on the main thread to its end,
        // so a GTK emission queued here would only fire once the run had already finished (which is what
        // an earlier cut of this check did -- it read `narration ready and spoken`). The cross-thread door
        // is the cancel switch itself, which is exactly what `RunBar::press_stop` raises (`src/run.rs:347`)
        // for legs already in flight; `start_run` has just cleared it for THIS run's generation, so a stop
        // left over from an earlier block cannot answer for this one.
        naivepost::cancel_leg::clear();
        std::thread::spawn(|| {
            std::thread::sleep(HALF / 2);
            naivepost::cancel_leg::cancel_now();
        });
        // The door cannot be entered before the shell has opened this run's cancel context, so waiting
        // for the watcher to report that opening is what makes the timing deterministic rather than a
        // race against whatever the previous block left on the switch.
        // The run is honestly still writing: HALF per clip with two clips, and the watcher raises the
        // switch HALF/2 after this run's own context opened, so `narration` is the stage ⏹ catches.
        let said = click_play(&window);
        assert_eq!(said, "narration stopped", "a stop mid-write names narration, got {said}");
        assert_eq!(status_text(&window), "narration stopped");
        assert!(
            !tree.narration_json().is_file(),
            "a run stopped before its first line wrote nothing to narration.json"
        );
        // The `speaking stopped` ending needs no widget scenario of its own: `SpeakRun::stopped` and
        // `stopped_status(JOB_SPEAKING)` are pinned in tests/narrate_write_and_speak.rs's
        // f4_1_s7_stopped_ends_name_the_stage, which raises the same switch between two lines.
        // Clear everything this block armed, so a later block sees a clean switch and no slow script.
        ui::set_narrate_script(&[]);
        ui::set_narrate_script_slow(&[], SHORT);
        naivepost::cancel_leg::clear();
        RAN_STOPPED.store(true, Ordering::SeqCst);
    }

    // Block 11 used to live here: ▶ pressed over a real socket, watched through the run bar. It is gone
    // on purpose — driving a live HTTP call through the shell's button needs the window, the bar and the
    // scripted-reply doors all in step, and it could not be made deterministic. The same claim (the call
    // really dials the Settings address) is pinned headlessly by tests/f4_2_s3_a_live_narration_call_
    // goes_over_http.rs, and the bar's stages by blocks 6 and 8 above.

    // Every block ran: hand the main loop back so `app.run()` returns and the test finishes. Without
    // this the activate handler never returns and the harness waits forever on a loop with no work in it.
    eprintln!("f41-round done, quitting app");
    let app = app.clone();
    glib::idle_add_local(move || {
        app.quit();
        glib::ControlFlow::Break
    });
}

fn find_named(root: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        for child in node.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
            if let Some(found) = walk(&widget, name) {
                return Some(found);
            }
        }
        None
    }
    walk(root.upcast_ref(), name)
}

#[test]
fn f4_1_run_bar_press_reaches_the_narrate_run() {
    // Pin cwd BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-f41-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        // An explicit argv: `run()` would hand the harness's own flags (`--exact`, `--nocapture`) to
        // libgio, which treats an option it does not know as fatal and aborts before the activate
        // callback runs -- which reads as "the block never ran". Passing our own one-argument list
        // keeps the two argument lists apart for good.
        app.run_with_args(&["naivepost"]);
    });

    assert!(RAN_NO_NARRATION.load(Ordering::SeqCst), "the narration-off block never ran");
    assert!(RAN_NO_TIMELINE.load(Ordering::SeqCst), "the no-timeline block never ran");
    assert!(RAN_NO_CUT.load(Ordering::SeqCst), "the no-cut block never ran");
    assert!(
        RAN_WRITTEN_AND_SPOKEN.load(Ordering::SeqCst),
        "the written-and-spoken block never ran"
    );
    assert!(RAN_CAPTIONS_ONLY.load(Ordering::SeqCst), "the captions-only block never ran");
    assert!(RAN_STAGE_SEQUENCE.load(Ordering::SeqCst), "the stage-sequence block never ran");
    assert!(RAN_BLANK_LINE.load(Ordering::SeqCst), "the blank-line block never ran");
    assert!(RAN_PULSED.load(Ordering::SeqCst), "the thinking-pulse block never ran");
    assert!(RAN_STOPPED.load(Ordering::SeqCst), "the stopped-mid-run block never ran");
}
