#![allow(dead_code)]

//! §4's key tail: `<emotion or 8-float vector>`. A weighted tag is worth its eight floats — the same
//! mapping that decides what the engine is sent (`narrate_tts::options`) — so a moved named-tag→vector
//! recipe moves a take's identity instead of serving an old performance under a new intent. A bare tag
//! has no vector and keeps naming itself, which is the "deliberately stable across versions" half.
//!
//! The wire check drives the Narrate row's ▶ over a fake audio.cpp on a loopback port, in the shape
//! `tests/suite/speak_tts_wire_widgets.rs` uses: one scenario per binary, because cwd and XDG are process-wide.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry};
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{hold_last_window, narrate_button as button, release_last_window, settle, status_text};

#[path = "speak_tts_wire_harness.rs"]
mod harness;
use harness::{wav_bytes, FakeAudio, Script};

fn entry(emotion: &str) -> Entry {
    Entry {
        s: 1.5,
        e: 34.7,
        at: 2.0,
        text: "We start with \u{2026}".to_string(),
        emotion: emotion.to_string(),
        pos: String::new(),
        roll: 0,
    }
}

/// S1: a weighted tag keys its take by the vector it resolves to, not by the words that spelled it.
#[test]
fn sec_01_project_and_files_4_narratenarrationjson_s1_a_weighted_tag_keys_its_take_as_the_vector() {
    let angry = entry("angry=1");
    // `angry` is axis 1 of the eight (narrate_tts::base), at full weight. The empty voice adds nothing
    // (§4's `[voiceKey|]` is the OTHER voices' salt), so this is `25e0.85|text|vector`.
    assert_eq!(
        narration::key_for(&angry, None),
        "25e0.85|We start with \u{2026}|0,1,0,0,0,0,0,0",
        "the vector stands where the tag used to"
    );
    // And the bare-tag spelling is now a DIFFERENT name for the same words — which is the whole point:
    // the program never builds that one any more.
    assert_ne!(
        narration::key_for(&angry, None),
        narration::tts_key(&angry, None, None),
        "a weighted tag must not key as its own spelling"
    );
    // The seed is cut from the same digest, so the performance changes too, not just the filename.
    assert_ne!(
        narration::tts_seed(&narration::key_for(&angry, None)),
        narration::tts_seed(&narration::tts_key(&angry, None, None)),
        "one digest names the file and seeds the draw"
    );
    // A named blend resolves through its recipe, so "excited" is happy plus surprised.
    assert_eq!(
        narration::key_for(&entry("excited=1"), None),
        "25e0.85|We start with \u{2026}|1,0,0,0,0,0,0.55,0",
        "a blend spends its weight across several axes"
    );
}

/// S2: an unmoved bare tag names the take it always named — §4's stability across versions.
#[test]
fn sec_01_project_and_files_4_narratenarrationjson_s2_a_bare_tag_still_keys_as_the_tag() {
    let calm = entry("calm");
    assert_eq!(
        narration::key_for(&calm, None),
        narration::tts_key(&calm, None, None),
        "no weights, no vector: the tag IS the field"
    );
    assert_eq!(narration::key_for(&calm, None), "25e0.85|We start with \u{2026}|calm");
    // A kin word is still a bare tag: nothing to be exact about, so nothing becomes a vector.
    assert_eq!(narration::key_for(&entry("furious"), None), "25e0.85|We start with \u{2026}|furious");
    // No emotion at all adds nothing beyond the empty field, and `own` still adds nothing either.
    assert_eq!(narration::key_for(&entry(""), None), "25e0.85|We start with \u{2026}|");
    assert_eq!(narration::key_for(&calm, Some("own")), narration::key_for(&calm, None));
}

/// S3: three performances of one line get three names, which is what lets a moved recipe take effect.
#[test]
fn sec_01_project_and_files_4_narratenarrationjson_s3_a_recipe_move_names_a_different_take() {
    let angry = entry("angry=1");
    let full = narration::key_for(&angry, None);
    // Half as hard — what a reader would write as `angry=0.5`, or what a recipe scaled by a weight lands on.
    let half = narration::tts_key(&angry, None, Some([0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
    let spelled = narration::tts_key(&angry, None, None);
    assert_ne!(full, half, "a stronger blend is a different performance");
    assert_ne!(half, spelled, "and neither is the bare tag");
    assert_ne!(full, spelled);
    // Each names its own file and its own seed: nothing collides.
    for key in [&full, &half, &spelled] {
        assert_eq!(narration::tts_file(key).len(), 16, "{key}");
    }
    let files: std::collections::HashSet<String> =
        [&full, &half, &spelled].iter().map(|k| narration::tts_file(k)).collect();
    assert_eq!(files.len(), 3, "three keys, three filenames");
    let seeds: std::collections::HashSet<u32> =
        [&full, &half, &spelled].iter().map(|k| narration::tts_seed(k)).collect();
    assert_eq!(seeds.len(), 3, "three keys, three seeds");
}

/// S4 — THE WIRE: the row's ▶ lands the take under the vector name, not the tag name.
static RAN_VECTOR_WIRE: AtomicBool = AtomicBool::new(false);

fn session_tree() -> Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    Tree::new(&dir).expect("the seeded session folder ends in .naivepost")
}

fn run_round(app: &adw::Application) {
    let root = std::env::temp_dir().join(format!("np-keyvec-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root.join("config")).expect("config root");
    std::fs::create_dir_all(&root.join("data")).expect("data root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }

    let audio = FakeAudio::start(Script::healthy());
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.audio_server = audio.url();
    settings::save(&paths, &conf).expect("the fake address is what the Settings now say");

    let tree = session_tree();
    let mut project = Project::default();
    project.language = "en".to_string();
    project.sources.push(Source {
        path: "/media/take.mp4".into(),
        footage: true,
        ..Default::default()
    });
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    let cut = Cut { segs: vec![Seg { s: 0.0, e: 10.0, ..Default::default() }], ..Default::default() };
    naivepost::cut::save(&cut, &tree).expect("cut.json written");
    let reference = tree.voice_ref_wav();
    std::fs::create_dir_all(reference.parent().expect("narrate/ has a parent")).expect("narrate/ made");
    std::fs::write(&reference, b"RIFF fake voice reference").expect("reference written");

    // A WEIGHTED tag: this is the case where the key's emotion field is a vector.
    let line = Entry {
        s: 0.0,
        e: 10.0,
        at: 1.0,
        text: "The chain moved to proof of stake.".into(),
        emotion: "angry=1".into(),
        pos: String::new(),
        roll: 0,
    };
    let vector_named = naivepost::narrate_tts::take_path(&tree, &narration::key_for(&line, Some("")));
    let tag_named = naivepost::narrate_tts::take_path(&tree, &narration::tts_key(&line, Some(""), None));
    assert_ne!(vector_named, tag_named, "the two spellings name different files");

    release_last_window();
    let window = ui::build_window(app, &project, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(ui::NarrateState {
        entries: vec![line.clone()],
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
        started: false,
        busy: false,
        narration_off: false,
        covered_spans: vec![],
        language: "en".into(),
    });
    ui::refresh(&window);
    settle();

    button(&window, "line-speak-0").emit_by_name::<()>("clicked", &[]);
    settle();
    for _ in 0..40 {
        if vector_named.is_file() {
            break;
        }
        settle();
    }

    let said = status_text(&window);
    assert!(said.starts_with("spoken"), "the row's ▶ reported the take, not a refusal: {said}");
    assert!(
        vector_named.is_file(),
        "the click filed the take under the vector name {}",
        vector_named.display()
    );
    let written = std::fs::read(&vector_named).expect("the take reads back");
    assert_eq!(written, wav_bytes(2400), "bytes that crossed the socket");
    assert!(
        !tag_named.is_file(),
        "nothing was filed under the bare-tag spelling {}",
        tag_named.display()
    );
    // And the request that went out carried the same vector, so name and performance agree.
    let speech = audio
        .drain()
        .into_iter()
        .find(|seen| seen.path == "/v1/audio/speech")
        .expect("the speech call went out");
    let body: serde_json::Value =
        serde_json::from_str(&speech.body_text()).expect("a JSON body went out");
    assert_eq!(
        body["options"]["emotion_vector"], "0,1,0,0,0,0,0,0",
        "the engine was sent the vector the file is named for: {}",
        speech.body_text()
    );

    RAN_VECTOR_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn sec_01_project_and_files_4_narratenarrationjson_s4_the_row_press_lands_the_vector_named_take() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        app.run_with_args(&["naivepost"]);
    });
    assert!(RAN_VECTOR_WIRE.load(Ordering::SeqCst), "the vector-name wire check never ran");
}
