//! F1.9 Mark retakes — reached through the real ▶ button, not by calling the module.
//!
//! The prior rejection of this item was that the flow existed while nothing in the app could reach it.
//! These checks click `run-play-button` on a Prepare page whose transcript is on disk and assert that
//! the marking pass's own lines land in the window log, plus the file it owes Cut.
//!
//! Two things need a live model server and are therefore NOT asserted through the click: the marks
//! themselves (so the completion line `>>> retakes: N abandoned stretch(es) …`) and the ceiling
//! refusal, which only exists once marks have come back over P.machine.retakeCeil. Both are proven on
//! handed-in marks in `tests/retakes_flow.rs` (`f1_9_s6_*`). The gate itself IS proven here, by
//! setting the pass to Joins before the window is built — see below for why it has to be set that early.
//!
//! What this binary proves is the wire: that a press reaches the pass at all, that S1's answer lands,
//! that the empty `retakes.tsv` is written even when nothing is marked, and that a different pass
//! contributes no line and no file.
//!
//! One application drives every check: GTK's main loop may only be run once per thread. Each check
//! pins cwd for itself first, because the ▶ handler resolves the project through
//! `startup::session_dir(current_dir())` at press time rather than at build time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::layout::Tree;
use naivepost::project::{MarkingPass, Project, Source};
use naivepost::run;
use naivepost::ui;

/// A root with `<root>/session.naivepost/` made and cwd pinned to the root — the arrangement
/// `tests/press_prepare_widgets.rs::pin_session` uses for the same reason.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f19w-{tag}-{}", std::process::id()));
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

/// Write `transcript.tsv` for one lane in the shape `textfmt` reads: `start\tend\tspeaker\ttext`.
fn seed_transcript(tree: &Tree, lane: &str, rows: &[(f64, f64, &str)]) {
    let path = tree.transcript_tsv(lane);
    std::fs::create_dir_all(path.parent().expect("transcript dir")).expect("transcript dir");
    let text: String = rows
        .iter()
        .map(|(start, end, said)| format!("{start:.2}\t{end:.2}\tAda\t{said}\n"))
        .collect();
    std::fs::write(&path, text).expect("transcript written");
}

/// The window log from the first line containing `marker` onward. Every check shares one
/// window-wide stream, so a presence-or-order assertion inside one press has to cut the earlier
/// checks off the front.
fn tail_after(logs: &[String], marker: &str) -> Vec<String> {
    let at = logs
        .iter()
        .position(|line| line.contains(marker))
        .unwrap_or_else(|| panic!("no line containing {marker:?} in {logs:?}"));
    logs[at..].to_vec()
}

#[test]
fn f1_9_wire_pressing_play_reaches_the_retake_marking_pass() {
    static RAN_BRIEF: AtomicBool = AtomicBool::new(false);
    static RAN_TOO_FEW: AtomicBool = AtomicBool::new(false);
    static RAN_GATE: AtomicBool = AtomicBool::new(false);

    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        check_briefing_lands_through_the_button(app);
        RAN_BRIEF.store(true, Ordering::SeqCst);
        check_too_few_lines_refuses_and_still_writes_the_file(app);
        RAN_TOO_FEW.store(true, Ordering::SeqCst);
        check_a_different_pass_says_nothing_at_all(app);
        RAN_GATE.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);

    assert!(RAN_BRIEF.load(Ordering::SeqCst), "the briefing check never ran");
    assert!(RAN_TOO_FEW.load(Ordering::SeqCst), "the too-few-lines check never ran");
    assert!(RAN_GATE.load(Ordering::SeqCst), "the policy-gate check never ran");
}

/// Four spoken lines is enough to hold a retake, so the pass runs and says what it is briefing on --
/// including how many pauses of P.machine.retakePauseSeconds or more the brief will draw.
fn check_briefing_lands_through_the_button(app: &adw::Application) {
    let (_keep, tree) = pin_session("brief");
    // Four lines, one of them preceded by a 2 s gap: over the 1.5 s threshold, so it draws a marker.
    seed_transcript(
        &tree,
        "lecture",
        &[
            (0.0, 1.0, "so the ledger records every transfer"),
            (3.0, 4.0, "and the chain keeps the total"),
            (4.5, 5.5, "which is what we audit"),
            (6.0, 7.0, "every quarter without fail"),
        ],
    );

    let mut model = Project::default();
    model.interval = 1.0;
    model.sources = vec![footage("project:sources/lecture.mkv")];
    // Policy default IS retakes, so no override needed.
    assert_eq!(
        model.policy.marking_pass.value,
        MarkingPass::Retakes,
        "the default pass is the one this flow owns"
    );
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let logs = ui::window_logs();
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "the run opened, so the pass ran inside it: {logs:?}"
    );
    assert!(
        logs.iter()
            .any(|line| line == ">>> retakes: briefing on 4 spoken lines, 1 pause(s) of 1.5 s or more"),
        "F1.9's briefing line landed for the clicked source: {logs:?}"
    );
    // It came after Prepare's own opening line, i.e. down the started path, not before it.
    let mine = tail_after(&logs, ">>> prepare: 1 input file(s)");
    let stage = mine
        .iter()
        .position(|line| line.starts_with(">>> retakes:"))
        .expect("the retake line follows Prepare's opening line");
    assert!(stage > 0, "the pass speaks after the run opened: {mine:?}");
}

/// S1 through the button: three spoken lines cannot hold a retake, and the empty `retakes.tsv` is
/// still written -- its presence is what tells Cut the pass ran.
fn check_too_few_lines_refuses_and_still_writes_the_file(app: &adw::Application) {
    let (_keep, tree) = pin_session("few");
    seed_transcript(
        &tree,
        "shortdeck",
        &[
            (0.0, 1.0, "one thought only"),
            (1.0, 2.0, "and a second"),
            (2.0, 3.0, "and a third"),
        ],
    );

    let mut model = Project::default();
    model.interval = 1.0;
    model.sources = vec![footage("project:sources/shortdeck.mkv")];
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let logs = ui::window_logs();
    assert!(
        logs.iter()
            .any(|line| line.contains("retakes:") && line.contains("fewer than 4")),
        "S1's answer reached the log through the button: {logs:?}"
    );
    assert!(
        tree.retakes_tsv().exists(),
        "the empty marks file is still written: {:?}",
        tree.retakes_tsv()
    );
    assert_eq!(
        std::fs::read_to_string(tree.retakes_tsv()).unwrap(),
        "",
        "and it is empty, which is the point"
    );
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "a short session does not stop the run: {logs:?}"
    );
}

/// The gate: name a different marking pass and the retake flow contributes NOTHING -- F1.10's joins
/// pass owns those lines and one press must not speak for both.
///
/// The pass has to be changed in the session the handler READS, not merely in the model handed to
/// `build_window`: ▶ pulls its `asked` project out of `PLAY_SESSION`, and while that slot starts life
/// as a clone of the argument, the F0.7 derive door runs before this arm and replaces the held project
/// with the freshly derived one. Setting the field on the argument alone therefore proves nothing --
/// the derived default (`Retakes`) overwrites it, which is exactly what the first run of this check
/// showed. So the check reaches into the same slot the handler reads, the way a saved project would
/// arrive: same field, same place, after the derive door has closed.
fn check_a_different_pass_says_nothing_at_all(app: &adw::Application) {
    let (_keep, tree) = pin_session("joins");
    // Plenty of lines: if the gate were missing, this session would certainly produce a retake line.
    seed_transcript(
        &tree,
        "joined",
        &[
            (0.0, 1.0, "first line of the take"),
            (3.0, 4.0, "second line after a long pause"),
            (7.0, 8.0, "third line after another"),
            (11.0, 12.0, "fourth line, well past the threshold"),
        ],
    );

    let mut model = Project::default();
    model.interval = 1.0;
    model.sources = vec![footage("project:sources/joined.mkv")];
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    // Same slot the ▶ handler reads, moved after the derive door would have closed.
    ui::set_marking_pass(&window, MarkingPass::Joins);
    assert_eq!(
        ui::marking_pass(&window),
        MarkingPass::Joins,
        "the session now names the joins pass"
    );

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    // This check is about THIS press only: `window_logs()` is one stream shared by every check in
    // the binary, so cut to the tail that starts at this press's own opening line.
    let logs = ui::window_logs();
    let joined_at = logs
        .iter()
        .rposition(|line| line == ">>>   joined.mkv")
        .expect("this press logged its own source line");
    let mine = &logs[joined_at - 1..];
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "the run still opens under the joins pass"
    );
    assert!(
        !mine.iter().any(|line| line.contains("retakes:")),
        "not the retakes pass: the flow stays silent for this press, even with four lines and \
long pauses: {mine:?}"
    );
    assert!(
        !tree.retakes_tsv().exists(),
        "and it writes no marks file either -- silence means silence: {:?}",
        tree.retakes_tsv()
    );
}
