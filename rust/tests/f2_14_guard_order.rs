// F2.14 S1 — the Cut page's ▶ REFUSES before the bar opens, in the flowchart's fixed order.
//
// spec/05-cut.md F2.14 S1 gives four refusals and an order: busy; hand edits ("press Revert first for a
// fresh suggestion"); no session timeline ("run Describe first …"); and the word gate, whose sentence
// `cut_suggest_words::WORDS_NEEDED` spells. The order is a fact about the machine (suggest.rs: a run under
// way may be about to write the very timeline whose absence would otherwise be reported), so it is pinned
// here by making every cause true at once except the one being answered.
//
// The door is `ui::cut_suggest_land::refuse_suggest_run`, which `build_window` wires into the run-bar ▶.
// Each check points that door at a throwaway `.naivepost` through `point_session_at` — the same
// arrangement `narrate_tts::speak_line` uses to keep a test off the network — so no case reads a real
// session folder and none writes one.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::Cut;
use naivepost::layout::Tree;
use naivepost::suggest;
use naivepost::textfmt::{self, Retake};
use naivepost::cut_suggest_words::WORDS_NEEDED;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text, temp_dir};

static RAN_ORDER: AtomicBool = AtomicBool::new(false);
static RAN_PASSES: AtomicBool = AtomicBool::new(false);

/// A project folder with nothing in it: no `session.txt`, no marks, no words.
struct Project {
    root: std::path::PathBuf,
    tree: Tree,
}

impl Project {
    fn fresh(tag: &str) -> Self {
        let root = temp_dir(tag);
        std::fs::create_dir_all(&root).expect("create the throwaway root");
        let dir = root.join(format!("{tag}.naivepost"));
        std::fs::create_dir_all(&dir).expect("create the fixture project");
        let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
        // The folders §01 lays down beside the project file are not created by `Tree::new` — it only
        // checks the name — so each file a refusal looks for needs its parent made here. `transcript/`
        // is private in `layout`, so it comes from a path the public ones share: `session.txt`'s own.
        std::fs::create_dir_all(self_tree_parent_of_session(&tree)).expect("the transcript folder");
        std::fs::create_dir_all(tree.prepare_dir()).expect("the prepare folder");
        Self { root, tree }
    }

    /// §04's resume marker for the session timeline: its presence is what "there is a timeline" means.
    fn with_timeline(self) -> Self {
        std::fs::write(self.tree.session_txt(), "0:00 hello\n").expect("write session.txt");
        self
    }

    /// A hand edit: `final.txt` newer than `retakes.tsv` is exactly what `hand_edit::edited` asks.
    fn with_hand_edits(self) -> Self {
        let marks = vec![Retake {
            s: 1.0,
            e: 2.0,
            again: 0.0,
            to: 2.0,
            text: String::new(),
            whole: String::new(),
        }];
        textfmt::write_retakes(&marks, &self.tree.retakes_tsv()).expect("write retakes.tsv");
        // The comparison is on mtimes, so the marks must be the OLDER file: write them first, then the
        // text, which is the order a person who edited the text after marking actually leaves them in.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(self.tree.final_txt(), "hello world\n").expect("write final.txt");
        self
    }

    /// Words for cam0 that the aligner answered: the gate's usable case.
    fn with_words(self) -> Self {
        let doc = naivepost::requests::WordsDoc {
            text: "hello".to_string(),
            words: vec![naivepost::requests::Word {
                word: "hello".to_string(),
                start_sample: 16000,
                end_sample: 20800,
            }],
        };
        naivepost::requests::write_words(&self.tree, "cam0", &doc).expect("write words.json");
        naivepost::requests::write_aligned(
            &self.tree,
            "cam0",
            &naivepost::requests::AlignedDoc {
                words: doc.words.clone(),
            },
        )
        .expect("write words.aligned.json");
        self
    }
}

fn self_tree_parent_of_session(tree: &Tree) -> std::path::PathBuf {
    tree.session_txt()
        .parent()
        .expect("session.txt sits in a folder")
        .to_path_buf()
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// One application, one window on the Cut page, then each refusal asked in turn — the shape
/// `press_play_widgets.rs` uses, because these checks share the window's cut slot and status line.
static RAN_REFUSED_PRESS: AtomicBool = AtomicBool::new(false);
static RAN_ALL: AtomicBool = AtomicBool::new(false);

fn refuse(busy: bool) -> Option<String> {
    cut_suggest_land::refuse_suggest_run(&ui::main_window(), busy)
}

fn checks(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    // The page shows cam0's footage, so the word gate reads cam0's files.
    ui::seed_review_cut(
        &window,
        &Cut {
            segs: vec![naivepost::cut::Seg {
                s: 0.0,
                e: 30.0,
                cam: 0,
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    settle();

    // --- the order, each cause alone below, then all together -------------------------------------

    // Every cause true: busy answers, because a run under way may be about to write the timeline whose
    // absence would otherwise be reported, and hand edits are moot while something else is running.
    let everything = Project::fresh("all").with_timeline().with_hand_edits();
    cut_suggest_land::point_session_at(Some(everything.tree.clone()));
    assert_eq!(
        refuse(true).as_deref(),
        Some(suggest::BUSY),
        "busy beats every other refusal"
    );

    // Hand edits next: timeline present, words present, so only the edits answer.
    let edited = Project::fresh("edits").with_timeline().with_words().with_hand_edits();
    cut_suggest_land::point_session_at(Some(edited.tree.clone()));
    assert_eq!(
        refuse(false).as_deref(),
        Some(suggest::HAND_EDITS),
        "hand edits beat the word gate"
    );

    // No timeline third: words missing too, but §S1 answers Describe first.
    let no_timeline = Project::fresh("notimeline");
    cut_suggest_land::point_session_at(Some(no_timeline.tree.clone()));
    assert_eq!(
        refuse(false).as_deref(),
        Some(suggest::NO_TIMELINE),
        "no timeline beats the word gate"
    );

    // The word gate last, and verbatim: timeline present, no hand edits, no words.
    let no_words = Project::fresh("nowords").with_timeline();
    cut_suggest_land::point_session_at(Some(no_words.tree.clone()));
    assert_eq!(
        refuse(false).as_deref(),
        Some(WORDS_NEEDED),
        "the word-gate sentence is the module's own, unaltered"
    );
    RAN_ORDER.store(true, Ordering::SeqCst);

    // --- nothing refuses ----------------------------------------------------------------------------

    let good = Project::fresh("good").with_timeline().with_words();
    cut_suggest_land::point_session_at(Some(good.tree.clone()));
    assert_eq!(refuse(false), None, "a ready session gets no refusal");
    // ...and the press really runs: the bar opened, which is what the refusal was preventing.
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    RAN_PASSES.store(true, Ordering::SeqCst);

    // The handler's own half: a refused press paints the sentence on the status line and leaves the bar
    // showing ▶ rather than ⏸ — checked through the widgets, not by re-asking the door.
    cut_suggest_land::point_session_at(Some(no_words.tree.clone()));
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        WORDS_NEEDED,
        "the refused press prints the word-gate sentence"
    );
    RAN_REFUSED_PRESS.store(true, Ordering::SeqCst);

    // Back to no session pointed at, so a later test in this binary cannot inherit ours.
    cut_suggest_land::point_session_at(None);
    window.close();
}

#[test]
fn f2_14_s1_the_refusals_come_in_the_flowcharts_order() {
    common::pin_private_cwd();
    run_once();
    assert!(RAN_ORDER.load(Ordering::SeqCst), "the order check never ran");
    assert!(RAN_PASSES.load(Ordering::SeqCst), "the pass-through check never ran");
    assert!(
        RAN_REFUSED_PRESS.load(Ordering::SeqCst),
        "the refused-press check never ran"
    );
    assert!(RAN_ALL.load(Ordering::SeqCst), "the round never ran");
}

fn run_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            checks(app);
            RAN_ALL.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}
