//! §04-prepare.md F1.12 Hand-edit the text — through the widgets.
//!
//! The Cut page's ▶ decides nothing about the hand edit: it asks [`naivepost::hand_edit`] whether
//! `final.txt` is newer than `retakes.tsv` and forwards the answer (spec/00-principles.md §5). What
//! these checks assert is that a real click on a real button produced the remade marks on disk, and
//! that a refusal reached the status line rather than being swallowed by the run bar's own write.
//!
//! One application, one `connect_activate`, because the fixture has to point the process's working
//! directory at a throwaway root — `wire_play` builds its [`Tree`] from
//! `startup::session_dir(current_dir())`, not from the folder the window was loaded with. That is
//! process-global state, so every check runs inside the single loop and exactly one `#[test]` drives
//! them (same arrangement as `tests/add_sources_widgets.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use adw::prelude::*;
use naivepost::hand_edit;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::requests::{self, Word, WordsDoc};
use naivepost::shell::Page;
use naivepost::textfmt;
use naivepost::tools::retakes::RETAKE_CEIL;
use naivepost::ui;

/// The spoken session: ten words, one per second, same list the logic tests use.
const WORDS: [&str; 10] = [
    "so", "we", "take", "the", "clip", "and", "cut", "it", "there", "twice",
];

/// Prepare keys each source's sidecar on its base name without the extension
/// (`prepare_data::base`), so `lecture.mkv` reads `prepare/inputs/lecture/words.json`.
const KEY: &str = "lecture";
const STORED: &str = "project:sources/lecture.mkv";

/// The seeded mark: what stands before the hand edit is noticed.
const OLD_MARK_ROW: &str = "1.0\t2.0\t0.0\t2.0\tthe old mark\n";

static RAN_REMAKE: AtomicBool = AtomicBool::new(false);
static RAN_REFUSED: AtomicBool = AtomicBool::new(false);
static RAN_MARKED: AtomicBool = AtomicBool::new(false);

/// The edited file with its join markers LEFT IN and one word nobody ever spoke. Written straight over
/// `final.txt` after `seed`, because `seed` builds plain text from a word list and the whole point here
/// is that a real editor hands back markers and typos together.
///
/// Read against the ten spoken words this leaves: 6 survivors (`so we take and cut there`), one of
/// them (`typo`) never said, so 4 went -- still under P.machine.retakeCeil (0.4), so it must be
/// marked rather than refused. Three stretches: `the clip` (what `|cut 3|` names), `it`, and `twice`.
const MARKED_WITH_TYPO: &str = "so we take |cut 3| and cut typo there\n";

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_remake_through_the_widgets(app);
            RAN_REMAKE.store(true, Ordering::SeqCst);
            check_refusal_reaches_the_status_line(app);
            RAN_REFUSED.store(true, Ordering::SeqCst);
            check_marked_text_and_never_said_word_through_the_widgets(app);
            RAN_MARKED.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-f112w-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A whole-second word list at 16 kHz, as ASR writes it. Whole seconds survive `textfmt`'s `%.2`
/// round-trip exactly, which is what lets the test compare parsed marks against computed ones.
fn samples(count: usize) -> Vec<Word> {
    const HZ: u64 = 16_000;
    (0..count)
        .map(|n| Word {
            word: WORDS[n].to_string(),
            start_sample: (n as u64) * HZ,
            end_sample: (n as u64 + 1) * HZ - (HZ / 10),
        })
        .collect()
}

/// The lines this press added. The log is a thread-local shared by every window in the binary, so a
/// check reads only what came after the marker it took before clicking.
fn added_since(before: &[String]) -> Vec<String> {
    ui::window_logs()[before.len()..].to_vec()
}

/// Set a file's mtime explicitly: the remake-vs-stands question is answered off the clock, so the
/// fixture must fix the order rather than hope the writes landed a second apart.
fn stamp(path: &Path, ago: Duration) {
    let when = SystemTime::now() - ago;
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

/// Build `<root>/session.naivepost` holding one footage source, its `words.json`, an edited
/// `final.txt` (the kept words) and an older `retakes.tsv` carrying the old mark. Returns the tree
/// plus the kept-word list, so the caller can compute the expected marks from the same input.
fn seed(tag: &str, kept: &[&str]) -> (PathBuf, Tree, String) {
    let root = temp_root(tag);
    let folder = naivepost::startup::session_dir(&root);
    fs::create_dir_all(folder.join("sources")).unwrap();
    fs::write(folder.join("sources").join("lecture.mkv"), "video bytes").unwrap();

    let project = Project {
        sources: vec![Source {
            path: STORED.to_string(),
            footage: true,
            narrator: 1,
            sepvoice: false,
            tracks: vec![0],
        }],
        ..Project::default()
    };
    naivepost::project::save(&project, &folder).unwrap();

    let tree = Tree::new(&folder).unwrap();
    // The transcript folder is the app's to create; a test writing into it makes it first.
    fs::create_dir_all(tree.final_txt().parent().unwrap()).unwrap();
    requests::write_words(
        &tree,
        KEY,
        &WordsDoc {
            text: WORDS.join(" "),
            words: samples(WORDS.len()),
        },
    )
    .unwrap();

    let final_text = format!("{}\n", kept.join(" "));
    fs::write(tree.final_txt(), &final_text).unwrap();
    fs::write(tree.retakes_tsv(), OLD_MARK_ROW).unwrap();
    // The text is newer than the marks: that is the whole of §F1.12's trigger.
    stamp(&tree.retakes_tsv(), Duration::from_secs(60));
    stamp(&tree.final_txt(), Duration::from_secs(0));
    (root, tree, final_text)
}

/// Switch the window to Cut through the real tab widget, asserting it actually landed there — a bounce
/// back to Prepare would mean the fixture lacks footage, not that the feature is broken.
fn go_to_cut(window: &adw::ApplicationWindow) {
    ui::tab_button(window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(
        ui::state(window).page,
        Page::Cut,
        "the Cut tab did not open — S2's lock means the fixture has no footage row"
    );
}

/// CHECK 1: a click on the real ▶ remakes the marks from the edited text, writes them, and the fresh
/// marks' own mtime stops any second remake.
fn check_remake_through_the_widgets(app: &adw::Application) {
    // Two of ten removed: under P.machine.retakeCeil (0.4), so it is an edit and gets marked.
    let kept: &[&str] = &["so", "we", "take", "the", "clip", "and", "cut", "twice"];
    let (_root, tree, final_text) = seed("remake", kept);
    // The wire reads cwd at press time, so point it at the root holding session.naivepost.
    std::env::set_current_dir(&_root).unwrap();

    let model = naivepost::project::load(&tree.dir()).expect("the saved project loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    go_to_cut(&window);

    let before = ui::window_logs();
    let final_before = fs::read(tree.final_txt()).unwrap();
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let lines = added_since(&before);
    assert!(
        lines.iter().any(|line| line == hand_edit::EDITED_NOTE),
        "the press logged the remake: {lines:?}"
    );

    // The marks on disk are exactly what the logic path yields for the same input.
    let words: Vec<String> = WORDS.iter().map(|w| (*w).to_string()).collect();
    let times: Vec<(f64, f64)> = (0..WORDS.len()).map(|n| (n as f64, n as f64 + 0.9)).collect();
    let expected = hand_edit::remake(&final_text, &words, &times, |_| None);
    assert!(!expected.refused, "two of ten is under the ceiling");
    let on_disk = textfmt::read_retakes(&tree.retakes_tsv()).unwrap();
    assert_eq!(on_disk.len(), 1, "one deleted stretch: {on_disk:?}");
    assert_eq!(
        on_disk.len(),
        expected.marks.len(),
        "same number of marks as the logic path"
    );
    for (got, want) in on_disk.iter().zip(expected.marks.iter()) {
        assert_eq!(got.s, want.s, "stretch start matches the logic path");
        assert_eq!(got.e, want.e, "stretch end matches the logic path");
        assert_ne!(got.text, "the old mark", "the seeded mark is gone");
    }
    assert_ne!(
        fs::read_to_string(tree.retakes_tsv()).unwrap(),
        OLD_MARK_ROW,
        "the old row was replaced"
    );
    assert_eq!(
        fs::read(tree.final_txt()).unwrap(),
        final_before,
        "the user's edited file is never rewritten here"
    );

    // A second ask over the same files changes nothing: the marks file is now newer than the text, so
    // `before_cut` answers "no edit". Asked directly rather than through another ▶ press, because ⏹
    // does not clear `RunBar::running` (only `finish()` does), so a further click would be S1's
    // pause toggle and would not re-run the check at all.
    let remade_bytes = fs::read(tree.retakes_tsv()).unwrap();
    let again = hand_edit::before_cut(&tree, &[STORED.to_string()]);
    assert!(again.is_none(), "the fresh marks stop a second remake");
    assert_eq!(
        fs::read(tree.retakes_tsv()).unwrap(),
        remade_bytes,
        "retakes.tsv unchanged since the remake"
    );
    assert_eq!(
        ui::window_logs().iter().filter(|line| *line == hand_edit::EDITED_NOTE).count(),
        1,
        "exactly one remake note across the whole log"
    );
    window.close();
}

/// CHECK 2: six of ten removed is over `P.machine.retakeCeil` (0.4), so the edit is refused, nothing
/// is written, and the refusal reaches the status line instead of being blanked by the run bar.
fn check_refusal_reaches_the_status_line(app: &adw::Application) {
    let kept: &[&str] = &["so", "we", "take", "the"];
    let removed = WORDS.len() - kept.len();
    // Guard the fixture itself: if this ever stopped being over the ceiling the check proves nothing.
    assert!(
        removed as f64 > RETAKE_CEIL * WORDS.len() as f64,
        "the fixture must cross the ceiling: {removed}/{} vs {RETAKE_CEIL}",
        WORDS.len()
    );

    let (_root, tree, _) = seed("refused", kept);
    std::env::set_current_dir(&_root).unwrap();

    let model = naivepost::project::load(&tree.dir()).expect("the saved project loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    go_to_cut(&window);

    let before = ui::window_logs();
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let lines = added_since(&before);
    assert!(
        lines.iter().any(|line| line.contains("refused")),
        "the refusal is in the log this press wrote: {lines:?}"
    );
    let status = ui::state(&window).status;
    assert!(
        status.contains("refused"),
        "the refusal sentence is on the status line, not blanked by the run bar's empty status: {status:?}"
    );
    assert_eq!(
        ui::state(&window).page,
        Page::Cut,
        "a refusal leaves the user on Cut"
    );
    assert_eq!(
        fs::read_to_string(tree.retakes_tsv()).unwrap(),
        OLD_MARK_ROW,
        "a refusal writes nothing — the seeded marks stand byte-for-byte"
    );
    window.close();
}

/// CHECK 3: a hand edit that KEEPS its join markers and adds a word nobody spoke still goes through the
/// real ▶ -- the markers are not read as words, the typo is dropped with a warning, and neither tips
/// the edit over `P.machine.retakeCeil` (0.4) into a refusal.
fn check_marked_text_and_never_said_word_through_the_widgets(app: &adw::Application) {
    // Same fixture as CHECK 1; only final.txt differs, replaced with the marker-and-typo spelling.
    let (_root, tree, _) = seed("marked", &["so", "we", "take", "and", "cut", "there", "twice"]);
    fs::write(tree.final_txt(), MARKED_WITH_TYPO).unwrap();
    stamp(&tree.final_txt(), Duration::from_secs(0));
    stamp(&tree.retakes_tsv(), Duration::from_secs(60));
    std::env::set_current_dir(&_root).unwrap();

    let model = naivepost::project::load(&tree.dir()).expect("the saved project loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    go_to_cut(&window);

    let before = ui::window_logs();
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    let lines = added_since(&before);
    assert!(
        lines.iter().any(|line| line == hand_edit::EDITED_NOTE),
        "the press logged the remake: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("were never said")),
        "the invented word was reported, not swallowed: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("refused")),
        "markers plus one typo stay under P.machine.retakeCeil (0.4), so nothing refuses: {lines:?}"
    );

    // The marks written by the click are what before_cut yields for the same bytes on disk.
    let expected = hand_edit::before_cut(&tree, &[STORED.to_string()]);
    let expected = match expected {
        Some(outcome) => outcome,
        // Already remade by the click above, so ask the pure function instead over the same text.
        None => {
            let words: Vec<String> = WORDS.iter().map(|w| (*w).to_string()).collect();
            let times: Vec<(f64, f64)> =
                (0..WORDS.len()).map(|n| (n as f64, n as f64 + 0.9)).collect();
            hand_edit::remake(MARKED_WITH_TYPO, &words, &times, |_| None)
        }
    };
    assert_eq!(expected.extra, 1, "one word nobody spoke");
    assert_eq!(expected.dropped, 4, "`the clip` under the marker, plus `it` and `twice`");
    let on_disk = textfmt::read_retakes(&tree.retakes_tsv()).unwrap();
    assert_eq!(
        on_disk.len(),
        expected.marks.len(),
        "the click wrote as many marks as the rule decides"
    );
    for (got, want) in on_disk.iter().zip(expected.marks.iter()) {
        assert_eq!((got.s, got.e, got.to), (want.s, want.e, want.to));
        assert!(!got.text.contains('|'), "no marker leaked into a mark: {got:?}");
        assert!(!got.text.contains("typo"), "{got:?}");
    }
    window.close();
}

#[test]
fn f1_12_s2_the_next_cut_press_remakes_the_marks_through_the_widgets() {
    window_round();
    assert!(RAN_REMAKE.load(Ordering::SeqCst), "the remake check never ran");
    assert!(RAN_REFUSED.load(Ordering::SeqCst), "the refusal check never ran");
    assert!(
        RAN_MARKED.load(Ordering::SeqCst),
        "the marked-text / never-said-word check never ran"
    );
}
