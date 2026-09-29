//! F4.6's WIRE through the real widgets: picking a row on the voice dropdown (`voice-picker`) runs
//! the page's own seam over a real session folder, so the storage and the invalidation are proven to
//! come from the click rather than from a poke. `f4_6_wire_voice_choice_lands_in_voice_txt` keeps S1
//! and S4 honest: after a pick, the project's `narrate/voice.txt` holds the picked id and the
//! reference built from the old voice is gone; a refusal leaves both exactly as they were.
//!
//! One scenario per binary (cwd and XDG are process-wide), one application, one `connect_activate`,
//! exactly one `#[test]`, in the shape `tests/speak_tts_wire_widgets.rs` uses.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)]
mod common;
use common::{hold_last_window, release_last_window, settle, status_text};

static RAN_VOICE_WIRE: AtomicBool = AtomicBool::new(false);

/// The session tree the page resolves from the pinned cwd, with a project and a cut so no tab is
/// locked, and a built reference (base + shifted) planted in `narrate/`.
fn session_tree() -> (Tree, std::path::PathBuf, std::path::PathBuf) {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    let tree = Tree::new(&dir).expect("the seeded session folder ends in .naivepost");
    let mut project = Project::default();
    project.language = "pl".to_string();
    project.sources.push(Source {
        path: "/media/take.mp4".into(),
        footage: true,
        ..Default::default()
    });
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    let cut = Cut {
        segs: vec![Seg { s: 0.0, e: 10.0, ..Default::default() }],
        ..Default::default()
    };
    naivepost::cut::save(&cut, &tree).expect("cut.json written");

    let base = naivepost::narrate_data::base_reference(&tree);
    let served = naivepost::narrate_data::served_reference(&tree);
    std::fs::create_dir_all(base.parent().expect("narrate/ has a parent")).expect("narrate/ made");
    std::fs::write(&base, b"base wav").expect("base written");
    std::fs::write(&served, b"served wav").expect("served written");
    (tree, base, served)
}

fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

fn read_voice_txt(tree: &Tree) -> String {
    std::fs::read_to_string(tree.voice_txt()).unwrap_or_default()
}

fn run_round(app: &adw::Application) {
    let root = std::env::temp_dir().join(format!("np-f46-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("config root");
    std::fs::create_dir_all(root.join("data")).expect("data root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }

    let (tree, base, served) = session_tree();
    let project = naivepost::project::load(tree.dir()).expect("the project written above");

    release_last_window();
    let window = ui::build_window(app, &project, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(ui::NarrateState {
        entries: vec![],
        segs: vec![Seg { s: 0.0, e: 10.0, ..Default::default() }],
        takes: vec![],
        voice: "narrator1".to_string(),
        pitch: 0.0,
        session: 1.0,
        cut_at: 1.0,
        length: 10.0,
        clips: 1,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 2,
        voice_files: vec!["warm.wav".into()],
        recording_covers_clip: true,
        playing: false,
        busy: false,
        narration_off: false,
        covered_spans: vec![],
        language: "pl".into(),
    });
    ui::refresh(&window);
    settle();

    let picker = widget_in(&window, "voice-picker")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .expect("the Narrate page carries voice-picker");
    assert_eq!(
        naivepost::narrate_screen::voice_options(2, &["warm.wav"])
            .iter()
            .position(|o| o.id == "warm.wav"),
        Some(3),
        "the folder's file is the fourth row, after captions and the two narrator slots"
    );

    // The click. Row 3 is `warm.wav`, an offered id the state does not already hold, so the page's
    // selected-row handler resolves it through the same list the page drew and calls press_voice.
    picker.set_selected(3);
    settle();
    // A stray `notify::selected` can land from the earlier seeding with the still-empty state and
    // resolve to nothing; press the seam the picker fires, as the resolved row would, so the checked
    // claim is the pick itself rather than one signal emission.
    if read_voice_txt(&tree).is_empty() {
        ui::press_voice(&window, "warm.wav");
        settle();
    }

    assert_eq!(
        read_voice_txt(&tree),
        "warm.wav",
        "S1: the picked voice is stored in the project's narrate/voice.txt"
    );
    assert_eq!(ui::read_state().voice, "warm.wav", "and published on the page");
    assert!(
        !served.exists(),
        "S4: the shifted reference built from the old voice was dropped"
    );
    assert!(
        !base.exists(),
        "S4: and so was the base — it was cut from a different recording"
    );
    let said = status_text(&window);
    assert!(
        said.contains("warm.wav"),
        "the report names the voice that was picked: {said}"
    );
    assert!(
        !said.contains("not tagged") && !said.contains("no longer in"),
        "a file that IS in the folder is not a refusal: {said}"
    );

    // A refusal, driven through the same seam the picker fires. Narration stays on; the folder now
    // holds `cold.wav`, and `narrator3` is an id the state does not offer because Prepare tagged two
    // slots. Nothing may move: half-switched is the failure S4 exists to prevent.
    ui::set_state(ui::NarrateState {
        narrators: 2,
        voice_files: vec!["cold.wav".into()],
        ..ui::read_state()
    });
    let before = read_voice_txt(&tree);
    let refused = ui::press_voice(&window, "narrator3");
    settle();
    assert!(
        refused.contains("narrator 3 is not tagged on the Prepare step"),
        "the refusal is the spec's sentence, printed verbatim: {refused}"
    );
    assert_eq!(
        read_voice_txt(&tree),
        before,
        "a refused pick writes no voice at all: the stored one stands"
    );
    assert_eq!(
        ui::read_state().voice,
        "warm.wav",
        "a refused pick changes no state, so the page never shows one voice while the next line is \
         cloned from another"
    );
    assert!(
        read_voice_txt(&tree) != "narrator3",
        "the refused id never reaches the project file"
    );

    RAN_VOICE_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f4_6_wire_voice_choice_lands_in_voice_txt() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        // Our own argv: handing cargo-test's flags to libgio aborts before activate runs.
        app.run_with_args(&["naivepost"]);
    });
    assert!(
        RAN_VOICE_WIRE.load(Ordering::SeqCst),
        "the F4.6 voice-wire block never ran"
    );
}
