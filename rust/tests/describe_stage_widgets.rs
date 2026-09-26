//! F1.7 Describe — reached through the real ▶ button, not by calling the module.
//!
//! The prior rejection of this item was that the flow existed while nothing in the app could reach it.
//! These checks click `run-play-button` on a Prepare page whose footage source has F1.6's output on
//! disk, and assert that the Describe stage's own lines land in the window log for exactly the input
//! the logic tests in `tests/describe_flow.rs` set.
//!
//! One application drives every check: GTK's main loop may only be run once per thread, so a second
//! `Application::run` would never activate and silently skip what came after. Each check pins cwd for
//! itself first, because the ▶ handler resolves the project through
//! `startup::session_dir(current_dir())` at press time rather than at build time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::clock::frame_name;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::run;
use naivepost::ui;

/// A root with `<root>/session.naivepost/` made and cwd pinned to the root — the arrangement
/// `tests/press_prepare_widgets.rs::pin_session` uses for the same reason.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f17w-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&dir).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

fn footage(path: &str) -> Source {
    Source {
        path: path.to_string(),
        footage: true,
        narrator: 0,
        sepvoice: false,
        tracks: vec![],
    }
}

/// Write F1.6's extraction output for one lane: frames named on the 250 ms grid, plus `scenes.tsv`.
/// The names are the clock names `clock::frame_name` writes, so the stage reads them the way it reads
/// a real folder rather than a made-up shape.
fn seed_frames(tree: &Tree, lane: &str, seconds: &[f64], changes: &[f64]) {
    let dir = tree.frames_dir(lane);
    std::fs::create_dir_all(&dir).expect("frames dir");
    for at in seconds {
        std::fs::write(dir.join(format!("{}.jpg", frame_name(*at))), b"jpeg").expect("frame");
    }
    // F1.6's marker: `<grid>|<scene threshold>`.
    std::fs::write(tree.frames_marker(lane), "0.25|1.0\n").expect("marker");
    let rows: String = changes
        .iter()
        .map(|at| format!("{at}\t5.0\n"))
        .collect();
    std::fs::write(tree.scenes_tsv(lane), rows).expect("scenes.tsv");
}

#[test]
fn f1_7_wire_pressing_play_reaches_the_describe_stage() {
    static RAN_PLAN: AtomicBool = AtomicBool::new(false);
    static RAN_REFUSAL: AtomicBool = AtomicBool::new(false);
    static RAN_NOTHING: AtomicBool = AtomicBool::new(false);

    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        check_plan_lands_through_the_button(app);
        RAN_PLAN.store(true, Ordering::SeqCst);
        check_every_frame_refusal_lands_through_the_button(app);
        RAN_REFUSAL.store(true, Ordering::SeqCst);
        check_a_source_without_frames_is_reported_not_fatal(app);
        RAN_NOTHING.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);

    assert!(RAN_PLAN.load(Ordering::SeqCst), "the plan check never ran");
    assert!(RAN_REFUSAL.load(Ordering::SeqCst), "the every-frame refusal check never ran");
    assert!(RAN_NOTHING.load(Ordering::SeqCst), "the missing-frames check never ran");
}

/// The stage's pick summary reaches the window log through a real click, naming the same frame count
/// and request count the logic test computes for the same frames. P.machine.describeFramesPerReq
fn check_plan_lands_through_the_button(app: &adw::Application) {
    let (_keep, tree) = pin_session("plan");
    // A thinned folder: two pictures per second over eight seconds (one every 0.5 s), with a scene
    // change at 4 s. At Freq 1.0 that is eight frames sent -- one per second, including the change's
    // own first frame. The spacing is deliberately NOT one grid step: an evenly-spaced set is what an
    // every-frame extraction looks like, and this check must not be refused for it.
    let seconds: Vec<f64> = (0..16).map(|i| i as f64 * 0.5).collect();
    seed_frames(&tree, "lecture", &seconds, &[4.0]);

    let mut model = Project::default();
    model.interval = 1.0;
    // Two sources: the footage deck, and the microphone heard alongside it. A single-source project
    // would be thinned by Freq down to one picture per grid step on a short take, which is exactly
    // what an every-frame folder looks like; with a second source the deck keeps its own spacing and
    // this check says something true.
    model.sources = vec![
        footage("project:sources/lecture.mkv"),
        Source {
            path: "project:sources/mic.wav".to_string(),
            footage: false,
            narrator: 1,
            sepvoice: false,
            tracks: vec![],
        },
    ];
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let logs = ui::window_logs();
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "the run opened, so the stage ran inside it: {logs:?}"
    );
    assert!(
        logs.iter().any(|line| line == ">>> [lecture] describe: 8 frames -> 2 requests"),
        "F1.7's pick summary landed in the log for the clicked source: {logs:?}"
    );
    // And it came AFTER Prepare's own opening line, i.e. the stage speaks down the started path.
    let opening = logs
        .iter()
        .position(|line| line.starts_with(">>> prepare: 2 input file(s)"))
        .expect("Prepare's own opening line is logged first");
    let stage = logs
        .iter()
        .position(|line| line.starts_with(">>> [lecture] describe:"))
        .expect("the describe line is logged");
    assert!(opening < stage, "the stage speaks after the run opened: {logs:?}");
}

/// An every-frame folder refuses through the same click, with §F1.7's exact sentence.
fn check_every_frame_refusal_lands_through_the_button(app: &adw::Application) {
    let (_keep, tree) = pin_session("everyframe");
    // Every grid step present over 12 s and no interval applied: 49 pictures at 0.25 s apart.
    let seconds: Vec<f64> = (0..49).map(|i| i as f64 * 0.25).collect();
    seed_frames(&tree, "rawcam", &seconds, &[]);

    let mut model = Project::default();
    model.interval = 1.0;
    model.sources = vec![footage("project:sources/rawcam.mp4")];
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let logs = ui::window_logs();
    assert!(
        logs.iter().any(|line| line
            == "rawcam was extracted as every-frame; describe needs a fixed interval -- rerun Prepare with e.g. 1s"),
        "the every-frame refusal reached the log through the button: {logs:?}"
    );
    assert!(
        !logs.iter().any(|line| line.contains("[rawcam] describe:")),
        "a refused source reports no plan: {logs:?}"
    );
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "one bad source does not stop the run — the refusal is local (§00-principles: failure is specific)"
    );
}

/// A footage source with no frames on disk is reported where the person reads, not panicked, and the
/// run still carries on.
fn check_a_source_without_frames_is_reported_not_fatal(app: &adw::Application) {
    let (_keep, _tree) = pin_session("noframes");
    let mut model = Project::default();
    model.interval = 1.0;
    model.sources = vec![footage("project:sources/gonedeck.mkv")];
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let logs = ui::window_logs();
    assert!(
        logs.iter()
            .any(|line| line.contains("[gonedeck]") && line.contains("no frames to describe")),
        "the empty folder is named in the log rather than swallowed: {logs:?}"
    );
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "and the run is still open: {logs:?}"
    );
}
