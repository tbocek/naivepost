//! F0.2 S4: the ▶ press itself asks no model. Every model call belongs to a stage of the run, off the window
//! thread. F1.9's retake pass used to run inside the click handler: six LLM calls on the window thread, the
//! window "Not Responding" for the whole pass, and the marks made from last run's transcripts before this run
//! rewrote them. It now runs only as the fix stage's merge row (`f1_8_fix_session` drives that row).
//!
//! The check clicks the real `run-play-button` on a Prepare page whose transcript holds four spoken lines,
//! enough for the pass to brief, and asserts that the click left neither the pass's lines nor its file. A
//! second press does the same for F1.10's joins pass, which sat beside it in the handler.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use common::fake_audio_server;
use common::footage;
use naivepost::layout::Tree;
use naivepost::project::{MarkingPass, Project};
use naivepost::requests::{self, Word, WordsDoc};
use naivepost::transcribe::SAMPLE_RATE;
use naivepost::ui;

/// A root with `<root>/session.naivepost/` made and cwd pinned to the root — the arrangement
/// `tests/suite/press_prepare_widgets.rs::pin_session` uses for the same reason.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f02s4-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&dir).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

/// Point the settings this process reads at that server. One conf for the whole binary: every check
/// here presses ▶, so they all need the same answer, and `XDG_CONFIG_HOME` belongs to the process.
fn serve_audio_for_the_presses(url: &str) {
    let config = std::env::temp_dir().join(format!("np-f02s4-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    let paths = naivepost::settings::Paths {
        config_dir: config.join("naivepost"),
        data_dir: config.join("data"),
    };
    let mut conf = naivepost::settings::Conf::default();
    conf.audio_server = url.to_string();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the fake server");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
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

/// Write `words.json` for one lane, one word a second: what F1.10's joins pass reads.
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

#[test]
fn f0_2_s4_the_press_asks_no_model() {
    static RAN: AtomicBool = AtomicBool::new(false);
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        // §S5's preflight runs before any stage, so the press needs an audio.cpp that answers.
        serve_audio_for_the_presses(&fake_audio_server());
        let (_keep, tree) = pin_session("press");
        // Four lines, one after a 2 s gap: what the retake pass would brief on, had the press run it.
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
        model.sources = vec![footage("project:sources/lecture.mkv")];
        assert_eq!(
            model.policy.marking_pass.value,
            MarkingPass::Retakes,
            "the default pass is the one that used to run at the press"
        );
        let window = ui::build_window(app, &model, "Prepare");
        window.present();

        ui::play_button(&window)
            .expect("the run bar has a ▶")
            .emit_clicked();
        // Prepare's ▶ hands its walk to a worker (§03-shell §1); its lines and ending land on the poll.
        assert!(
            naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
            "the press's worker answered"
        );

        let logs = ui::window_logs();
        assert!(
            logs.iter()
                .any(|line| line == ">>> prepare: 1 input file(s)"),
            "the press reached Prepare's started path: {logs:?}"
        );
        assert!(
            !logs.iter().any(|line| line.starts_with(">>> retakes:")),
            "the press briefed no retake pass: {logs:?}"
        );
        assert!(
            !tree.retakes_tsv().exists(),
            "and wrote no marks file: {:?}",
            tree.retakes_tsv()
        );
        press_joins_asks_nothing(app);
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(RAN.load(Ordering::SeqCst), "the press check never ran");
}

/// The joins twin: two recordings, eight words, one seam, the pass named in the slot ▶ reads. The press
/// leaves neither the pass's briefing nor the two files it owes Cut.
fn press_joins_asks_nothing(app: &adw::Application) {
    let (_keep, tree) = pin_session("joins");
    seed_words(&tree, "take-a", &["one", "two", "three", "four"]);
    seed_words(&tree, "take-b", &["five", "six", "seven", "eight"]);
    let mut model = Project::default();
    model.sources = vec![
        footage("project:sources/take-a.mkv"),
        footage("project:sources/take-b.mkv"),
    ];
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    // Same slot the ▶ handler reads, set after the derive door would have closed.
    ui::set_marking_pass(&window, MarkingPass::Joins);

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();
    // Prepare's ▶ hands its walk to a worker (§03-shell §1); its lines and ending land on the poll.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the press's worker answered"
    );

    let logs = ui::window_logs();
    assert!(
        logs.iter()
            .any(|line| line == ">>> prepare: 2 input file(s)"),
        "the joins press reached Prepare's started path: {logs:?}"
    );
    assert!(
        !logs.iter().any(|line| line.starts_with(">>> text edit:")),
        "the press briefed no joins pass: {logs:?}"
    );
    assert!(
        !tree.final_txt().exists() && !tree.retakes_tsv().exists(),
        "and wrote neither file: {:?} {:?}",
        tree.final_txt(),
        tree.retakes_tsv()
    );
}
