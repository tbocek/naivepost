// F2.14 §S2a — `P.policy.cutMode = words`: the cut is BUILT FROM THE WORDS, no model asked.
//
// spec/05-cut.md §F2.14 S2a: marks from `retakes.tsv`; every filmed run trimmed to its words (just
// before the first to just after the last, never outside the run); marked stretches removed; coalesced;
// then persist, become the base, say "cut by the words: N segments" and log
// ">>> cut by the words: N stretch(es) taken out, m:ss of silence, N segments, M:SS total".
//
// The reading half is `cut_suggest_words_run`, the arithmetic is `suggest::words_cut`, and the landing
// is `cut_suggest_land::land_spans` — this file only feeds real files into them and reads back what the
// page was left with, which is why every case here writes real `words.json` / `words.aligned.json` /
// `retakes.tsv` in a throwaway `.naivepost` instead of mocking anything.
//
// The dead-air half of S2a is NOT pinned here on purpose: §S2a's silences come from each row's cached
// wave (`cut_wave_edges::read_lanes`, which needs a `cache/waves` file that matches the recording's
// size and mtime). Faking one would test the fake, not the reader, so the case is left to the item that
// owns wave caching; what IS pinned is that an absent cache contributes no silences at all.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::cut_suggest_words_run::{self as words_run};
use naivepost::layout::Tree;
use naivepost::project::Project;
use naivepost::requests::{self, AlignedDoc, Word, WordsDoc};
use naivepost::shell::Page;
use naivepost::textfmt::{self, Retake};
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text, temp_dir};

static RAN_TRIM: AtomicBool = AtomicBool::new(false);
static RAN_MARKS: AtomicBool = AtomicBool::new(false);
static RAN_SILENCES: AtomicBool = AtomicBool::new(false);
static RAN_LANDED: AtomicBool = AtomicBool::new(false);
static RAN_EMPTY: AtomicBool = AtomicBool::new(false);

/// A project folder under a throwaway root, gone when the test ends.
struct Project_ {
    root: std::path::PathBuf,
    tree: Tree,
    project: Project,
}

impl Project_ {
    fn fresh(tag: &str) -> Self {
        let root = temp_dir(tag);
        std::fs::create_dir_all(&root).expect("create the throwaway root");
        let dir = root.join(format!("{tag}.naivepost"));
        std::fs::create_dir_all(&dir).expect("create the fixture project");
        let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
        let project = naivepost::project::load(&fixture_dir()).expect("fixture loads");
        // Write the project file so `open_project_dir`-style readers find a real project here.
        let json = serde_json::to_string_pretty(&project).expect("project serialises");
        std::fs::write(dir.join(naivepost::project::PROJECT_FILE), json).expect("write naivepost.json");
        Self {
            root,
            tree,
            project,
        }
    }

    /// Words for `source`, at the aligner's 16 kHz: both files, aligned answered. An empty `spoken`
    /// means an aligner that answered NOTHING, which the word gate reads as "not aligned" — the dry
    /// case below wants the gate passed and `words_cut` given no words, so it writes real ASR words and
    /// then removes the aligner's answer.
    fn with_words(self, source: &str, spoken: &[(&str, f64)]) -> Self {
        let this = self.with_asr_words(source, spoken);
        if spoken.is_empty() {
            return this;
        }
        let words = this.asr(source);
        requests::write_aligned(&this.tree, source, &AlignedDoc { words })
            .expect("write words.aligned.json");
        this
    }

    /// `words.json` only — the ASR's answer with no aligner behind it.
    fn with_asr_words(self, source: &str, spoken: &[(&str, f64)]) -> Self {
        let words: Vec<Word> = spoken
            .iter()
            .map(|(text, at)| Word {
                word: text.to_string(),
                start_sample: (*at * 16000.0) as u64,
                end_sample: ((at + 0.3) * 16000.0) as u64,
            })
            .collect();
        let doc = WordsDoc {
            text: spoken.iter().map(|(w, _)| *w).collect::<Vec<&str>>().join(" "),
            words,
        };
        requests::write_words(&self.tree, source, &doc).expect("write words.json");
        self
    }

    /// The word list `with_asr_words` just wrote for `source`.
    fn asr(&self, source: &str) -> Vec<Word> {
        requests::read_words(&self.tree, source)
            .ok()
            .flatten()
            .map(|doc| doc.words)
            .unwrap_or_default()
    }

    /// `retakes.tsv`, written by the same writer `hand_edit::write_marks` uses.
    fn with_marks(self, marks: &[Retake]) -> Self {
        textfmt::write_retakes(marks, &self.tree.retakes_tsv()).expect("write retakes.tsv");
        self
    }
}

impl Drop for Project_ {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A cut showing one camera's footage from `s` to `e` — what `runs_of` builds its runs from.
fn cam_cut(s: f64, e: f64) -> Cut {
    Cut {
        segs: vec![Seg { s, e, cam: 0, ..Default::default() }],
        ..Default::default()
    }
}

/// The same footage shown from a hand-slid tape (§B's `shift`): the words §S2a trims to are the ones
/// the tape holds, and the slide is what puts them on the session clock.
fn cam_cut_shifted(s: f64, e: f64, shift: f64) -> Cut {
    Cut {
        segs: vec![Seg { s, e, cam: 0, ..Default::default() }],
        shift: [("cam0".to_string(), shift)].into_iter().collect(),
        ..Default::default()
    }
}

/// The cut the page is seeded with below — footage plus one hand-placed card.
fn seeded_cut() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: 60.0, cam: 0, ..Default::default() },
            Seg { s: 0.0, e: 0.0, dur: 3.0, ins: "card.png".to_string(), cam: 0, ..Default::default() },
        ],
        ..Default::default()
    }
}

fn retake(s: f64, e: f64) -> Retake {    Retake {
        s,
        e,
        again: 0.0,
        to: e,
        text: String::new(),
        whole: String::new(),
    }
}

/// A word at `at` seconds, for the trim case.
fn spoken(at: f64) -> (&'static str, f64) {
    ("word", at)
}

#[test]
fn f2_14_s2a_a_run_is_trimmed_to_its_words_never_outside_the_run() {
    common::pin_private_cwd();
    let p = Project_::fresh("trim").with_words("cam0", &[spoken(3.0), spoken(9.0)]);
    let runs = words_run::runs_of(&p.tree, &cam_cut(0.0, 30.0));
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].span, (0.0, 30.0), "the run is the filmed span");
    assert_eq!(runs[0].first_spoken, 3.0, "first word's second");
    assert_eq!(runs[0].last_spoken, 9.0, "last word's second");
    // `words_cut` does the ±0.4 s reach and the clamp; the trimmed span is what §S2a says to build.
    let made = naivepost::suggest::words_cut(&runs, &[], &[]);
    assert_eq!(made.segs, vec![(2.6, 9.4)], "just before the first, just after the last");
    // And never outside the run: words whose reach crosses the run's own end stay inside it.
    let clamped = Project_::fresh("clamp").with_words("cam0", &[spoken(29.9)]);
    let clamped_runs = words_run::runs_of(&clamped.tree, &cam_cut(0.0, 30.0));
    assert_eq!(
        naivepost::suggest::words_cut(&clamped_runs, &[], &[]).segs,
        vec![(29.5, 30.0)],
        "the reach stops at the run's end"
    );
    // No words at all: `runs_of` says so by answering no run for that footage — with nothing said there
    // is no stretch of video the trim could point at, which is §S2a's "trimmed to its words" read to
    // its end rather than a reason to keep a silent minute.
    let quiet = Project_::fresh("quiet");
    let quiet_runs = words_run::runs_of(&quiet.tree, &cam_cut(0.0, 30.0));
    assert_eq!(quiet_runs, Vec::new(), "footage with no words contributes no run");
    // A tape's seconds are its OWN (§04 F1.9 files them that way); §B's `sessionAt` is what moves them
    // onto the session clock, and a hand slide (`cut.json shift`) rides along with them.
    let slid = Project_::fresh("slid").with_words("cam0", &[spoken(1.0), spoken(4.0)]);
    let slid_runs = words_run::runs_of(&slid.tree, &cam_cut_shifted(20.0, 30.0, 3.0));
    assert_eq!(slid_runs[0].first_spoken, 21.0, "1 s into the tape, slid +3 s = session 21");
    // ...and the run is CLAMPED to its own footage: the tape's later words (session 24) are outside the
    // scene this cut keeps, so §S2a trims to what the run actually shows.
    assert_eq!(slid_runs[0].last_spoken, 21.0);
    RAN_TRIM.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2a_a_mark_over_gone_footage_is_not_counted() {
    common::pin_private_cwd();
    let p = Project_::fresh("marks")
        .with_words("cam0", &[spoken(2.0), spoken(6.0), spoken(20.0), spoken(26.0)])
        .with_marks(&[retake(4.0, 5.0), retake(100.0, 110.0)]);
    let marks = words_run::marks_of(&p.tree);
    assert_eq!(marks, vec![(4.0, 5.0), (100.0, 110.0)], "the file's own spans");
    let runs = words_run::runs_of(&p.tree, &cam_cut(0.0, 30.0));
    let made = naivepost::suggest::words_cut(&runs, &marks, &[]);
    assert_eq!(made.marks_out, 1, "only the mark that took seconds out counts");
    assert!(made.log.contains("1 stretch(es) taken out"), "{}", made.log);
    RAN_MARKS.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2a_an_absent_wave_cache_contributes_no_silences() {
    common::pin_private_cwd();
    // The §S2a dead-air step reads cached waves; with none on disk the list is empty, which is what
    // lets `remove_dead_air` find nothing long enough rather than guess a cut in a sentence.
    let p = Project_::fresh("nowave");
    assert!(words_run::silences_of(&p.tree, &p.project).is_empty());
    RAN_SILENCES.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2a_the_cut_lands_persists_and_keeps_hand_placed_inserts() {
    common::pin_private_cwd();
    window_round();
    assert!(RAN_LANDED.load(Ordering::SeqCst), "the landing check never ran");
    assert!(RAN_EMPTY.load(Ordering::SeqCst), "the empty-cut check never ran");
}

fn run_checks(app: &adw::Application) {
    let p = Project_::fresh("land")
        .with_words("cam0", &[spoken(2.0), spoken(6.0), spoken(20.0), spoken(26.0)])
        .with_marks(&[retake(4.0, 5.0)]);
    let window = ui::build_window(app, &p.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    // The page shows cam0 with one hand-placed card nobody may trim.
    ui::seed_review_cut(&window, &seeded_cut());
    // The page opened on the cut it was seeded with: without this the history would still be the one
    // `build_window` took from an empty page, and ↶ would answer with nothing instead of the pre-cut cut.
    ui::reopen_history_on(&window, &seeded_cut());
    ui::cut_suggest_land::point_session_at(Some(p.tree.clone()));
    settle();

    let made = words_run::run_words(&window, &p.tree, &p.project);
    assert_eq!(made.status, "cut by the words: 2 segments", "the page's own sentence");
    assert_eq!(
        made.log,
        ">>> cut by the words: 1 stretch(es) taken out, 00:00 of silence, 2 segments, 00:23 total",
        "the log's own line"
    );
    assert!(made.wrote, "a real cut was written");
    let on_screen = ui::review_cut_of(&window);
    assert_eq!(on_screen.segs.len(), 3, "two scenes plus the card that was already there");
    assert!(
        on_screen.segs.iter().any(|seg| seg.is_insert()),
        "a hand-placed insert survives a words cut: {:?}",
        on_screen.segs
    );
    // §S2a's persist: the same file the page reads back is what a re-open would read.
    let saved = naivepost::cut::load(&p.tree).expect("cut.json is there");
    assert_eq!(saved.segs.len(), 3, "the landed cut is the one on disk");
    // One ↶ takes the whole words cut back, and Revert keeps it as the new base.
    ui::press_undo(&window);
    settle();
    assert!(
        ui::review_cut_of(&window).segs.iter().any(|seg| seg.e == 60.0),
        "one Undo undid the words cut in one step"
    );
    RAN_LANDED.store(true, Ordering::SeqCst);

    // An empty result writes nothing and says the same status: no new sentence was invented for it.
    // No footage on the page means no runs at all, which is the one input that cannot answer anything
    // but zero — the emptiness is structural, not a coincidence of word times.
    let dry = Project_::fresh("dry");
    ui::seed_review_cut(&window, &Cut::default());
    ui::reopen_history_on(&window, &Cut::default());
    ui::cut_suggest_land::point_session_at(Some(dry.tree.clone()));
    let empty = words_run::run_words(&window, &dry.tree, &dry.project);
    assert_eq!(empty.status, "cut by the words: 0 segments");
    assert!(!empty.wrote, "nothing was written");
    assert_eq!(
        naivepost::cut::load(&dry.tree).expect("a cut loads").segs,
        vec![],
        "and the project's cut file was not touched"
    );
    RAN_EMPTY.store(true, Ordering::SeqCst);
    ui::cut_suggest_land::point_session_at(None);
    window.close();
}

fn window_round() {
    common::one_app_round(|app| {
        run_checks(app);
    });
}

// `status_text` is used by the guard-order binary's shape of this file; keep the import honest here too.
#[allow(dead_code)]
fn printed(window: &adw::ApplicationWindow) -> String {
    status_text(window)
}
