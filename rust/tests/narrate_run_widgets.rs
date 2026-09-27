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
use naivepost::narrate_screen;
use naivepost::narration::{self, Entry, Narration};
use naivepost::project::{Project, Source};
use naivepost::shell::Page;
use naivepost::ui;

static RAN_NO_NARRATION: AtomicBool = AtomicBool::new(false);
static RAN_NO_TIMELINE: AtomicBool = AtomicBool::new(false);
static RAN_NO_CUT: AtomicBool = AtomicBool::new(false);
static RAN_WRITTEN_AND_SPOKEN: AtomicBool = AtomicBool::new(false);
static RAN_CAPTIONS_ONLY: AtomicBool = AtomicBool::new(false);
static RAN_STAGE_SEQUENCE: AtomicBool = AtomicBool::new(false);
static RAN_BLANK_LINE: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Strong handle on the window last built, so the next block drops it first: a closed GTK window is
    /// not destroyed and its names stay parented, which would send a lookup to the wrong tree.
    static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn release_last_window() {
    LAST_WINDOW.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            old.close();
        }
    });
    settle();
}

/// Let the main context run what the widget emissions queued.
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

/// The project folder the run resolves: `startup::session_dir(current_dir())`. The test pins cwd to its
/// own temp root first, so this never points into `rust/`.
fn session_tree() -> Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    Tree::new(&dir).expect("the seeded session folder ends in .naivepost")
}

/// A project with one footage source, so no tab is locked (`shell::lock`) and the cut has something to
/// be laid out from. `no_narration` drives the F4.8 refusal.
fn base_project(no_narration: bool) -> Project {
    let mut project = Project::default();
    project.no_narration = no_narration;
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
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
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
        busy: false,
        narration_off: project.no_narration,
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
        let tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
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

    eprintln!("f41-block 7 start");
    // --- (7) a blank written line counts as neither spoken nor cached ------------------------
    {
        let project = base_project(false);
        let tree = seed(&project, &Cut { segs: two_clips(), ..Default::default() }, true);
        let window = narrate_window(app, &project, Vec::new());
        let before = ui::window_logs().len();
        let said = click_play_with_reply(
            &window,
            &[(1, 0.0, "   ", ""), (2, 0.0, "a real line", "")],
        );
        assert_eq!(said, narrate_run::DONE_SPOKEN);
        let logs = ui::window_logs()[before..].join("\n");
        assert!(
            logs.contains("    narrate: 1 line(s) spoken, 0 already in the cache"),
            "the blank line is neither spoken nor cached:\n{logs}"
        );
        RAN_BLANK_LINE.store(true, Ordering::SeqCst);
    }

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
}
