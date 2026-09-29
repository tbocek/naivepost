//! F4.4's WIRE through the real widget: a click on a narration row's ▶ (`line-speak-0`) goes
//! through `narrate_page::speak_this_line` into `speak_leg`, which dials the address the Settings
//! file names — here a fake audio.cpp on a loopback port. Nothing is scripted: the upload and the
//! speech call really cross a socket, and the take that lands is made of bytes that arrived over it.
//!
//! One scenario per binary (cwd and XDG are process-wide), one application, one `connect_activate`,
//! exactly one `#[test]`, in the shape `tests/narrate_run_widgets.rs` uses.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)]
mod common;
use common::{hold_last_window, release_last_window, settle, status_text};

#[path = "speak_tts_wire_harness.rs"]
mod harness;
use harness::{wav_bytes, FakeAudio, Script};

static RAN_SPEAK_WIRE: AtomicBool = AtomicBool::new(false);

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

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .unwrap_or_else(|| panic!("the Narrate page carries {name}"))
}

/// The session folder the page resolves its take into: `startup::session_dir(current_dir())`.
fn session_tree() -> Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    Tree::new(&dir).expect("the seeded session folder ends in .naivepost")
}

fn run_round(app: &adw::Application) {
    let root = std::env::temp_dir().join(format!("np-f44-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root.join("config")).expect("config root");
    std::fs::create_dir_all(&root.join("data")).expect("data root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }

    // The fake audio.cpp, and the address the program will read for itself.
    let audio = FakeAudio::start(Script::healthy());
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.audio_server = audio.url();
    settings::save(&paths, &conf).expect("the fake address is what the Settings now say");

    let tree = session_tree();
    // A project with footage so no tab is locked, and a cut the rows sit on.
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
    // F4.6's reference, where the leg reads it from (its parent folder, made from the path).
    let reference = tree.voice_ref_wav();
    std::fs::create_dir_all(reference.parent().expect("narrate/ has a parent"))
        .expect("narrate/ made");
    std::fs::write(&reference, b"RIFF fake voice reference").expect("reference written");

    let entry = Entry {
        s: 0.0,
        e: 10.0,
        at: 1.0,
        text: "The chain moved to proof of stake.".into(),
        emotion: "angry=1".into(),
        pos: String::new(),
        roll: 0,
    };
    let key = naivepost::narration::tts_key(&entry, Some(""), None);
    let expected = naivepost::narrate_tts::take_path(&tree, &key);

    release_last_window();
    let window = ui::build_window(app, &project, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(ui::NarrateState {
        entries: vec![entry.clone()],
        segs: cut.segs.clone(),
        takes: vec![],
        voice: String::new(),
        pitch: 0.0,
        session: 1.0,
        cut_at: 1.0,
        length: 10.0,
        clips: 1,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
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

    // The click. Everything after it is the program's own legs over HTTP.
    button(&window, "line-speak-0").emit_by_name::<()>("clicked", &[]);
    settle();
    // The dial is synchronous inside the handler, but the socket's thread needs its moment too.
    for _ in 0..40 {
        if expected.is_file() {
            break;
        }
        settle();
    }

    let said = status_text(&window);
    assert!(
        said.starts_with("spoken"),
        "the row's ▶ reported the take, not a refusal: {said}"
    );
    assert!(
        expected.is_file(),
        "a real wav landed at {}",
        expected.display()
    );
    let written = std::fs::read(&expected).expect("the take reads back");
    assert_eq!(
        written,
        wav_bytes(2400),
        "the file holds the bytes the fake sent over the socket"
    );
    assert!(
        said.contains(&expected.file_name().unwrap().to_string_lossy().into_owned()),
        "the status names the file it wrote: {said}"
    );

    // The wire: the health/models probe, the upload with its filename header, and the speech post,
    // all seen by the server; the speech body speaks Polish because the project does.
    let seen = audio.drain();
    let paths: Vec<String> = seen
        .iter()
        .map(|seen| format!("{} {}", seen.method, seen.path))
        .collect();
    assert!(
        paths.iter().any(|p| p.ends_with("/health")),
        "S2 asked /health: {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("/v1/models")),
        "S2 asked /v1/models: {paths:?}"
    );
    let upload = seen
        .iter()
        .find(|seen| seen.path == "/v1/ui/upload")
        .unwrap_or_else(|| panic!("S3 uploaded the reference; saw {paths:?}"));
    assert_eq!(
        upload.header("x-audiocpp-filename"),
        Some("voice_ref.wav"),
        "the upload named the file it carried: {:?}",
        upload.headers
    );
    let speech = seen
        .iter()
        .find(|seen| seen.path == "/v1/audio/speech")
        .unwrap_or_else(|| panic!("S4 posted the speech call; saw {paths:?}"));
    let body: serde_json::Value =
        serde_json::from_str(&speech.body_text()).expect("a JSON body went out");
    assert_eq!(
        body["language"], "pl",
        "the project's language rode out, not the prototype's \"en\""
    );
    assert_eq!(
        body["input"], "The chain moved to proof of stake.",
        "the clicked row's own words"
    );
    assert_eq!(
        body["voice_ref"], "/tmp/uploaded/voice_ref.wav",
        "the path this line's upload returned"
    );

    RAN_SPEAK_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f4_4_wire_pressing_line_speak_lands_a_real_wav() {
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
        RAN_SPEAK_WIRE.load(Ordering::SeqCst),
        "the F4.4 speak-wire block never ran"
    );
}
