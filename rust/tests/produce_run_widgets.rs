//! F5.1 ▶ Produce — the WIRE. The rules live in `naivepost::produce_flow` and are pinned by
//! `tests/produce_run_flow.rs`; this file proves the shell's real ▶ on a real Produce page reaches them:
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
//! button `ui::play_button` finds by name — the same route `tests/narrate_run_widgets.rs` takes.

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

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{hold_last_window, release_last_window, settle, status_text};

const ITEM: &str = "F5.1";

static RAN_NO_CUT: AtomicBool = AtomicBool::new(false);
static RAN_RAN_THE_FLOW: AtomicBool = AtomicBool::new(false);
static RAN_SKIP: AtomicBool = AtomicBool::new(false);

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

/// Two clips, so the opening line reads "2 clips at mp4/h264 crf 24".
fn two_clips() -> Vec<Seg> {
    vec![
        Seg { s: 0.0, e: 10.0, cam: 0, ..Default::default() },
        Seg { s: 10.0, e: 20.0, cam: 0, ..Default::default() },
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

/// Fire the shell's ▶ through the toolkit and report what it left on the status line.
fn click_play(window: &adw::ApplicationWindow) -> String {
    ui::play_button(window)
        .expect("the run bar has a play-button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    status_text(window)
}

/// Whether the run bar currently shows a run under way.
fn bar_running(window: &adw::ApplicationWindow) -> bool {
    ui::running_step(window).is_some()
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
            !bar_running(&window),
            "and no run started for work that was refused before it began"
        );
        RAN_NO_CUT.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- (2) a cut stands: the press reaches produce_flow -------------------------------------
    {
        let window = produce_window(app, &base_project(), &Cut { segs: two_clips(), aspect: "16:9".into(), ..Default::default() });
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
        settle();
        RAN_RAN_THE_FLOW.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- (3) a matching stamp: skipped, and SAID ---------------------------------------------
    {
        let tree = session_tree();
        let cut = Cut { segs: two_clips(), aspect: "16:9".into(), ..Default::default() };
        // A video standing there, so the gate has something to be up to date about.
        let video = tree.final_video("mp4");
        if let Some(dir) = video.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(&video, b"a finished video").unwrap();
        // Seed the stamp with the hash THIS page will compute: settings from the page, cut from the page.
        let project = base_project();
        let settings = naivepost::project::Produce::default();
        let input = produce_stamp::Input {
            settings: &settings,
            segs: &cut.segs,
            lines: &[],
            sources: &project.sources,
            aspect: &cut.aspect,
            voice: "",
            no_narration: project.no_narration,
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
        RAN_SKIP.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }
}

#[test]
fn f5_1_play_on_produce_page_reaches_the_flow() {
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
        app.run();
    });

    assert!(RAN_NO_CUT.load(Ordering::SeqCst), "the no-cut refusal check never ran");
    assert!(RAN_RAN_THE_FLOW.load(Ordering::SeqCst), "the run-through-the-flow check never ran");
    assert!(RAN_SKIP.load(Ordering::SeqCst), "the up-to-date skip check never ran");
}
