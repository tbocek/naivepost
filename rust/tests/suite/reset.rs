//! F0.15 ↺: what each workflow's reset removes, and the button through its question on a real window.

use adw::prelude::*;
use gtk4 as gtk;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use naivepost::layout::Tree;
use naivepost::reset;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;

/// One file under `root`, with its folders.
fn touch(root: &Path, rel: &str) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the folders");
    std::fs::write(&path, b"x").expect("the file");
    path
}

/// Every workflow's files, so each reset can be seen to take its own and leave the others'.
fn full_project(tag: &str) -> (PathBuf, Tree) {
    let root = common::temp_dir_created(tag).join("talk.naivepost");
    std::fs::create_dir_all(&root).expect("the project");
    for rel in [
        "naivepost.json",
        "sources/a.mkv",
        "stems/a.split-voice.wav",
        "prepare/word_list.json",
        "prepare/inputs/a/words.json",
        "cache/waves/a.wave",
        "cache/edges/a.env",
        "cache/llm/fix/k",
        "cache/llm/textedit/k",
        "cut/cut.json",
        "cut/line.json",
        "cache/llm/cut/k",
        "cache/llm/speed/k",
        "narrate/narration.json",
        "narrate/narration.prev.json",
        "narrate/takes.json",
        "narrate/voice.txt",
        "narrate/pitch.txt",
        "narrate/tts/0123.wav",
        "cache/llm/narrate/k",
        "produce/final.mp4",
        "produce/clips/c000_a.mp4",
        "produce/publish/publish.json",
        "cache/llm/translate/k",
        "llm/0101-000000-fix.html",
        "requests.tsv",
    ] {
        touch(&root, rel);
    }
    let tree = Tree::new(&root).expect("a project folder");
    (root, tree)
}

fn after_reset(tag: &str, page: Page) -> (Vec<&'static str>, Vec<&'static str>) {
    let (root, tree) = full_project(tag);
    let removed = reset::remove(&tree, &reset::plan(&tree, page));
    assert!(removed.failed.is_empty(), "{:?}", removed.failed);
    let all = [
        "naivepost.json",
        "sources/a.mkv",
        "stems/a.split-voice.wav",
        "prepare/word_list.json",
        "prepare/inputs/a/words.json",
        "cache/waves/a.wave",
        "cache/edges/a.env",
        "cache/llm/fix/k",
        "cache/llm/textedit/k",
        "cut/cut.json",
        "cut/line.json",
        "cache/llm/cut/k",
        "cache/llm/speed/k",
        "narrate/narration.json",
        "narrate/narration.prev.json",
        "narrate/takes.json",
        "narrate/voice.txt",
        "narrate/pitch.txt",
        "narrate/tts/0123.wav",
        "cache/llm/narrate/k",
        "produce/final.mp4",
        "produce/clips/c000_a.mp4",
        "produce/publish/publish.json",
        "cache/llm/translate/k",
        "llm/0101-000000-fix.html",
        "requests.tsv",
    ];
    all.iter().partition(|rel| !root.join(rel).exists())
}

/// Each reset takes its own outputs and the answers they were made from, and nothing of anybody else's. The
/// sources, the project file and the exchange log are nobody's output and survive all four.
#[test]
fn f0_15_each_reset_removes_its_own_files_and_nothing_else() {
    let (gone, stay) = after_reset("reset-prepare", Page::Prepare);
    assert_eq!(
        gone,
        [
            "stems/a.split-voice.wav",
            "prepare/word_list.json",
            "prepare/inputs/a/words.json",
            "cache/waves/a.wave",
            "cache/edges/a.env",
            "cache/llm/fix/k",
            "cache/llm/textedit/k",
        ]
    );
    assert!(stay.contains(&"cut/cut.json"), "{stay:?}");

    let (gone, stay) = after_reset("reset-cut", Page::Cut);
    assert_eq!(gone, ["cut/cut.json", "cache/llm/cut/k", "cache/llm/speed/k"]);
    assert!(stay.contains(&"prepare/word_list.json"), "{stay:?}");
    assert!(stay.contains(&"cut/line.json"), "the red line is where the page stands: {stay:?}");

    let (gone, stay) = after_reset("reset-narrate", Page::Narrate);
    assert_eq!(
        gone,
        [
            "narrate/narration.json",
            "narrate/narration.prev.json",
            "narrate/takes.json",
            "narrate/tts/0123.wav",
            "cache/llm/narrate/k",
        ]
    );
    assert!(
        stay.contains(&"narrate/voice.txt") && stay.contains(&"narrate/pitch.txt"),
        "the voice settings are choices, not outputs: {stay:?}"
    );

    let (gone, stay) = after_reset("reset-produce", Page::Produce);
    assert_eq!(
        gone,
        [
            "produce/final.mp4",
            "produce/clips/c000_a.mp4",
            "produce/publish/publish.json",
            "cache/llm/translate/k",
        ]
    );
    for kept in ["naivepost.json", "sources/a.mkv", "llm/0101-000000-fix.html", "requests.tsv"] {
        assert!(stay.contains(&kept), "{kept}: {stay:?}");
    }
}

static RAN: AtomicBool = AtomicBool::new(false);

/// Through the page: ↺ sits after ▶ and ⏹, says nothing goes until the question is answered, Cancel leaves it
/// all, Reset removes on a worker and the page re-reads what is left, the cut gone from the band too.
fn check_the_button(app: &adw::Application) {
    let root = std::env::temp_dir().join(format!("naivepost-reset-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch root");
    std::env::set_current_dir(&root).expect("cwd pinned to the scratch root");
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&session).expect("session folder");
    // A source that exists: the refresh drops one that does not, and a page with no sources goes back to Prepare.
    touch(&session, "sources/2026-10-28 17-53-23.mkv");
    let seed = naivepost::project::Project {
        sources: vec![naivepost::project::Source {
            path: "project:sources/2026-10-28 17-53-23.mkv".to_string(),
            footage: true,
            narrator: 0,
            sepvoice: false,
            tracks: vec![],
        }],
        ..Default::default()
    };
    naivepost::project::save(&seed, &session).expect("the project");
    let cut_json = touch(&session, "cut/cut.json");
    std::fs::write(&cut_json, r#"{"segs":[{"s":1.0,"e":2.0}]}"#).expect("a cut");
    let cached = touch(&session, "cache/llm/cut/k");
    let words = touch(&session, "prepare/word_list.json");

    let window = ui::build_window(app, &seed, "Cut");
    common::hold_last_window(window.clone());
    window.present();
    common::wait_for_stable_layout(&window);

    let row = common::widget_in(&window, "run-controls-inner").expect("the controls");
    let names: Vec<String> = row
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .collect();
    assert_eq!(names, ["play-button", "stop-button", "reset-button"]);
    let button: gtk::Button = common::widget_in(&window, "reset-button")
        .expect("the button")
        .downcast()
        .expect("a button");

    button.emit_clicked();
    let question = ui::window_reset::open_question().expect("it asks first");
    assert!(cut_json.exists(), "nothing goes before the answer");
    question.response("cancel");
    common::settle();
    assert!(cut_json.exists() && cached.exists(), "Cancel leaves it all");

    button.emit_clicked();
    ui::window_reset::open_question().expect("it asks again").response("reset");
    assert!(ui::background::settle_for_test(std::time::Duration::from_secs(20)));
    common::wait_for_stable_layout(&window);
    assert!(!cut_json.exists() && !cached.exists(), "the Cut's own files went");
    assert!(words.exists(), "what Prepare made stayed");
    assert!(
        ui::cut_view::band_scene_for(&window).segs.is_empty(),
        "and the band no longer draws the cut"
    );
    let status = ui::find_status(window.upcast_ref()).expect("the status").text();
    assert_eq!(status, "Cut reset");
    assert!(
        ui::window_logs().iter().any(|line| line.starts_with(">>> reset Cut: removed ")),
        "the log says what went"
    );

    let tree = Tree::new(&session).expect("the tree");
    let left = reset::plan(&tree, Page::Cut);
    let body = std::fs::read_to_string(&cut_json).unwrap_or_default();
    assert!(left.is_empty(), "nothing left to reset: {left:?} cut.json holds {body:?}");
    button.emit_clicked();
    assert!(ui::window_reset::open_question().is_none(), "nothing left, nothing to ask");
    window.close();
    RAN.store(true, Ordering::SeqCst);
}

#[test]
fn f0_15_the_button_asks_then_removes_and_the_page_rereads() {
    common::pin_private_cwd();
    common::one_app_round(check_the_button);
    assert!(RAN.load(Ordering::SeqCst), "the button round never ran");
}

/// A test binary opens nothing on the desktop: the launcher reaches the real session whatever display the test
/// window is on, and a test run opened the user's files.
#[test]
fn f0_15_a_test_binary_never_opens_anything_on_the_desktop() {
    let before = ui::log_link::blocked_opens_for_test();
    assert_eq!(ui::log_link::open("file:///tmp/naivepost-never-opened"), Ok(()));
    assert_eq!(ui::log_link::open("https://example.invalid/"), Ok(()));
    assert_eq!(ui::log_link::blocked_opens_for_test(), before + 2);
}
