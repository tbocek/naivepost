#![allow(dead_code)]

//! F5.1 S4 over a socket: the render's **speak** leg really dials audio.cpp.
//!
//! §F5.1 S4 says the run has two halves and §F5.2 S2 makes the render speak "every line without a wav".
//! The leg the page hands that step used to answer a refusal without dialling anything, so a run could
//! finish having synthesized nothing. This drives the page's own door —
//! [`naivepost::ui::produce_page_rows::speak_a_line`], the exact function
//! `produce_snapshot::render_half` passes to `produce_exec::run_render` — over a fake audio.cpp on a
//! loopback port, the same seam `tests/suite/speak_tts_wire_harness.rs` uses: the address in the Settings file
//! is the test's, and nothing about the program's work is scripted.
//!
//! One scenario per binary (cwd and XDG_CONFIG_HOME are process-wide).

//! One scenario per binary, so exactly one `#[test]` lives here.

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::{FakeAudio, Script};
use naivepost::layout::Tree;
use naivepost::produce_exec::ToSpeak;
use naivepost::settings::{self, Conf};

const ITEM: &str = "F5.1";

/// The line the render asks for, and where ITS plan says the take belongs. Bounds are the clip's; the
/// speak leg derives the take's name from the words, the emotion and the voice, so a plan that names a
/// different file for the same line is a real bug this test would see.
fn to_speak(text: &str) -> ToSpeak {
    ToSpeak {
        index: 0,
        text: text.into(),
        wav: std::path::PathBuf::from("unused.wav"),
    }
}

#[test]
fn f5_1_s4_a_line_with_no_wav_is_spoken_for_real_by_the_render_leg() {
    // --- scenario root, and the fake audio.cpp the Settings will name --------------------------
    let root = std::env::temp_dir().join(format!("np-f51-speak-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("config root");
    std::fs::create_dir_all(root.join("data")).expect("data root");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let conf = Conf::default();
    settings::save(&paths, &conf).expect("settings written");

    // A project with a language of its own: the run must speak the project's language, not "en".
    let project = naivepost::project::Project {
        policy: naivepost::project::Policy {
            language: naivepost::project::Field {
                value: "pl".to_string(),
                origin: Default::default(),
                because: None,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    // F4.6's reference, so the speak door has a voice to clone (S1).
    let reference = tree.voice_ref_wav();
    std::fs::create_dir_all(reference.parent().expect("narrate/ has a parent")).unwrap();
    std::fs::write(
        &reference,
        b"RIFF fake reference, forty-odd bytes long, not empty",
    )
    .unwrap();

    // (1) THE FAILURE PATH FIRST: nothing listens anywhere, so the refusal has to name the server.
    // `conf.audio_server` is still the loopback default, which nothing here serves.
    let line = to_speak("The chain moved to proof of stake.");
    let mute = naivepost::ui::produce_speak_leg::speak_one(&tree, &line, "narrator1");
    let said = mute.expect_err("with no audio server standing, the leg must report a failure");
    assert!(
        said.contains("narrator") || said.contains("127.0.0.1") || said.contains("http"),
        "{ITEM} S4: a failure names the voice or the server tried, not a generic string: {said}"
    );
    assert!(
        !said.contains("no speech server here"),
        "the leg must not answer with a canned refusal: {said}"
    );

    // (2) THE REAL CALL: a healthy audio.cpp on a loopback port, named by the Settings file.
    let audio = FakeAudio::start(Script::healthy());
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.audio_server = audio.url();
    conf.tts_model = "index-tts2".to_string();
    settings::save(&paths, &conf).expect("the fake address is what the Settings now say");

    let text = "Welcome back: lecture two is all about Ethereum.";
    let line = to_speak(text);
    let spoken = naivepost::ui::produce_speak_leg::speak_one(&tree, &line, "narrator1")
        .expect("a healthy server speaks the line");

    // (a) the request really crossed a socket: health, models, upload and the speech POST.
    let seen = audio.drain();
    let paths_seen: Vec<&str> = seen.iter().map(|ask| ask.path.as_str()).collect();
    assert!(
        paths_seen.iter().any(|p| p.contains("/v1/audio/speech")),
        "{ITEM} S4: the speech endpoint was dialled for real, saw {paths_seen:?}"
    );
    assert!(
        paths_seen.iter().any(|p| p.contains("/v1/ui/upload")),
        "and the voice reference went up for this line, saw {paths_seen:?}"
    );
    let speech = seen
        .iter()
        .find(|ask| ask.path.contains("/v1/audio/speech"))
        .expect("the speech request");
    assert!(
        speech.body_text().contains(text),
        "and the body carried the line's own words: {}",
        speech.body_text()
    );
    assert!(
        speech.body_text().contains("\"pl\""),
        "P.policy.ttsLanguage is the PROJECT's language, not a hard-coded \"en\": {}",
        speech.body_text()
    );

    // (b) the take landed where the render looks for it. `wav_of` is what `render_half` hands the plan,
    // so the file it names is the file the encoder muxes and the stamp weighs.
    let entry = naivepost::narration::Entry {
        s: 0.0,
        e: 0.0,
        at: 0.0,
        text: text.into(),
        emotion: String::new(),
        pos: String::new(),
        roll: 0,
    };
    let expected = naivepost::produce_stamp::wav_of(&tree, &entry, Some("narrator1"), 0)
        .expect("a line with words has a take path");
    assert_eq!(spoken, expected, "the leg reports the file it wrote");
    assert!(
        expected.is_file(),
        "the take is on disk at {}",
        expected.display()
    );
    assert_eq!(
        expected.parent().expect("narrate/tts"),
        tree.tts_wav("x")
            .parent()
            .expect("narrate/tts has a parent"),
        "and it is in narrate/tts, where the next run's cache check looks"
    );
    // A take is only ever filed from a reply of at least MIN_WAV_BYTES (`narrate_tts::reply_problem`),
    // so the file's own weight is the proof this one arrived over the socket rather than being written.
    let take_bytes = std::fs::read(&expected).expect("the take is readable");
    assert!(
        take_bytes.len() > naivepost::narrate_tts::MIN_WAV_BYTES,
        "a reply too small to be audio would never have been filed: {} bytes",
        take_bytes.len()
    );

    // (c) the same line again costs nothing: the plan's own cache check now finds the take.
    let again = naivepost::narration::key_for(&entry, Some("narrator1"));
    assert!(
        tree.tts_wav(&naivepost::narration::tts_file(&again))
            .is_file(),
        "the key the stamp builds names the same take, so no line is spoken twice"
    );
}
