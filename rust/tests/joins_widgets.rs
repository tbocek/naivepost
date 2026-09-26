//! F1.10 Repair the joins — reached through the real ▶ button, not by calling the module.
//!
//! The item was sent back because the flow existed while nothing in the app could reach it. These
//! checks click `play-button` on a Prepare page whose words are on disk and assert that the join
//! pass's own lines land in the window log, plus the two files it owes Cut.
//!
//! What needs a live textedit server is NOT asserted here: the answers themselves, so the per-join
//! accept/refuse paths, the ceilings and the whole-take flag are proven with handed-in replies in
//! `tests/joins_flow.rs`. What this binary proves is the wire — that a press reaches the pass at
//! all, that S1/S2 settle without a server, that both files get written, and that the policy gate
//! keeps silent when the pass named is not joins.
//!
//! One application drives every check: GTK's main loop may only be run once per thread. Each check
//! pins cwd for itself first, because the ▶ handler resolves the project through
//! `startup::session_dir(current_dir())` at press time rather than at build time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::layout::Tree;
use naivepost::project::{MarkingPass, Project, Source};
use naivepost::requests::{self, Word, WordsDoc};
use naivepost::run;
use naivepost::transcribe::SAMPLE_RATE;
use naivepost::ui;

/// A root with `<root>/session.naivepost/` made and cwd pinned to the root — the arrangement
/// `tests/press_prepare_widgets.rs::pin_session` uses for the same reason.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f110w-{tag}-{}", std::process::id()));
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

/// Write one lane's `words.json`: each word one second long, so word `n` spans samples
/// `n*RATE .. (n+1)*RATE` on that recording's own clock.
fn seed_words(tree: &Tree, lane: &str, words: &[&str]) {
    let doc = WordsDoc {
        text: words.join(" "),
        words: words
            .iter()
            .enumerate()
            .map(|(n, word)| Word {
                word: word.to_string(),
                start_sample: n as u64 * SAMPLE_RATE,
                end_sample: (n as u64 + 1) * SAMPLE_RATE,
            })
            .collect(),
    };
    requests::write_words(tree, lane, &doc).expect("words.json written");
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
fn f1_10_wire_pressing_play_reaches_the_joins_pass() {
    static RAN_SEAM: AtomicBool = AtomicBool::new(false);
    static RAN_NO_SEAM: AtomicBool = AtomicBool::new(false);
    static RAN_GATE: AtomicBool = AtomicBool::new(false);

    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        check_a_seam_reaches_the_pass_through_the_button(app);
        RAN_SEAM.store(true, Ordering::SeqCst);
        check_one_recording_reports_no_seam_and_still_writes_both_files(app);
        RAN_NO_SEAM.store(true, Ordering::SeqCst);
        check_another_pass_says_nothing_at_all(app);
        RAN_GATE.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);

    assert!(RAN_SEAM.load(Ordering::SeqCst), "the seam check never ran");
    assert!(RAN_NO_SEAM.load(Ordering::SeqCst), "the no-seam check never ran");
    assert!(RAN_GATE.load(Ordering::SeqCst), "the policy-gate check never ran");
}

/// Two recordings means a seam, so the pass runs and says how many joins it will visit.
fn check_a_seam_reaches_the_pass_through_the_button(app: &adw::Application) {
    let (_keep, tree) = pin_session("seam");
    seed_words(&tree, "take-a", &["one", "two", "three", "four"]);
    seed_words(&tree, "take-b", &["five", "six", "seven", "eight"]);

    let mut model = Project::default();
    model.interval = 1.0;
    model.sources = vec![
        footage("project:sources/take-a.mkv"),
        footage("project:sources/take-b.mkv"),
    ];
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

    let logs = ui::window_logs();
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "the run opened, so the pass ran inside it: {logs:?}"
    );
    // Eight joinable words across two recordings, one seam between them.
    assert!(
        logs.iter()
            .any(|line| line == ">>> text edit: briefing on 8 joinable words, 1 seam(s) of 140 words each side"),
        "F1.10's opening line landed for the clicked sources: {logs:?}"
    );
    // It came after Prepare's own opening line, i.e. down the started path.
    let mine = tail_after(&logs, ">>> prepare: 2 input file(s)");
    let stage = mine
        .iter()
        .position(|line| line.starts_with(">>> text edit:"))
        .expect("the joins line follows Prepare's opening line");
    assert!(stage > 0, "the pass speaks after the run opened: {mine:?}");
    // No files yet on THIS path: with a seam present the pass only plans, and both files are written
    // at S6 once the answers arrive (which needs the textedit server). The no-seam check below is
    // where the writing is proven, because that branch settles with no answer to wait for.
    assert!(
        !tree.final_txt().exists(),
        "a planned-but-unanswered join writes nothing: {:?}",
        tree.final_txt()
    );
}

/// S2 through the button: one recording has no seam, and the pass still writes both files so the
/// next step can tell "ran, found nothing" from "never ran".
fn check_one_recording_reports_no_seam_and_still_writes_both_files(app: &adw::Application) {
    let (_keep, tree) = pin_session("noseam");
    seed_words(&tree, "single", &["alpha", "beta", "gamma", "delta"]);

    let mut model = Project::default();
    model.interval = 1.0;
    model.sources = vec![footage("project:sources/single.mkv")];
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::set_marking_pass(&window, MarkingPass::Joins);

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let logs = ui::window_logs();
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "a single take does not stop the run: {logs:?}"
    );
    assert!(
        logs.iter()
            .any(|line| line == ">>> text edit: one recording, no seam to repair -- every word stands"),
        "S2's answer reached the log through the button: {logs:?}"
    );
    assert!(tree.final_txt().exists(), "final.txt still written");
    assert!(tree.retakes_tsv().exists(), "retakes.tsv still written");
}

/// The gate: leave the pass at its default and the joins flow contributes NOTHING — F1.9's retake
/// pass owns those lines and one press must not speak for both.
///
/// The pass has to be changed in the session the handler READS, not merely in the model handed to
/// `build_window`: ▶ pulls its `asked` project out of `PLAY_SESSION`, and F0.7's derive door runs
/// before this arm and replaces the held project with the freshly derived one. So the check leaves
/// it alone here — the default IS Retakes — and asserts the joins pass stays silent even though this
/// session has plenty of joinable words and a real seam.
fn check_another_pass_says_nothing_at_all(app: &adw::Application) {
    let (_keep, tree) = pin_session("gate");
    seed_words(&tree, "left", &["aa", "bb", "cc", "dd"]);
    seed_words(&tree, "right", &["ee", "ff", "gg", "hh"]);

    let mut model = Project::default();
    model.interval = 1.0;
    model.sources = vec![
        footage("project:sources/left.mkv"),
        footage("project:sources/right.mkv"),
    ];
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    assert_eq!(
        ui::marking_pass(&window),
        MarkingPass::Retakes,
        "the default pass is the other one"
    );

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    // This check is about THIS press only: `window_logs()` is one stream shared by every check in
    // the binary, so cut to the tail that starts at this press's own source line.
    let logs = ui::window_logs();
    let right_at = logs
        .iter()
        .rposition(|line| line.contains("right.mkv"))
        .expect("this press logged its own source line");
    let mine = &logs[right_at - 1..];
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "the run still opens under the retakes pass"
    );
    assert!(
        !mine.iter().any(|line| line.contains("text edit:")),
        "not the joins pass: the flow stays silent for this press, even with eight joinable words \
and a seam: {mine:?}"
    );
}
