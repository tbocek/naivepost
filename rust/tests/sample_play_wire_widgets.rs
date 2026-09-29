//! F4.6 S5's WIRE through the real widget: a click on the sample's ▶ (`sample-play`) runs
//! `narrate_page::speak_fresh_sample` into `speak_leg`, which dials the audio.cpp address the
//! Settings file names — here a fake on a loopback port — and files the bytes that crossed the socket
//! under `narrate/samples/`. Then a SECOND press of the same words in the same voice is heard from
//! that file with no second call at all, which is the whole point of S5's caching.
//!
//! Nothing is scripted in place of the work: the player is a real subprocess too, pointed at
//! `/bin/true` through the same `voice_program()` seam the preview uses, so the program's own
//! "spawn the player, hold the child" path runs.
//!
//! One scenario per binary (cwd and XDG are process-wide), one application, one `connect_activate`,
//! exactly one `#[test]`, in the shape `tests/speak_tts_wire_widgets.rs` uses.

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
use common::{hold_last_window, release_last_window, settle, status_text, widget_in};

#[path = "speak_tts_wire_harness.rs"]
mod harness;
use harness::{wav_bytes, FakeAudio, Script};

static RAN_SAMPLE_WIRE: AtomicBool = AtomicBool::new(false);

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .unwrap_or_else(|| panic!("the Narrate page carries {name}"))
}

/// The session folder the page resolves its sample into: `startup::session_dir(current_dir())`.
fn session_tree() -> Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    Tree::new(&dir).expect("the seeded session folder ends in .naivepost")
}

/// How many speech calls landed in a batch of requests the fake reported.
fn speech_calls(seen: &[harness::Seen]) -> usize {
    seen.iter()
        .filter(|seen| seen.path == "/v1/audio/speech")
        .count()
}

fn run_round(app: &adw::Application) {
    let root = std::env::temp_dir().join(format!("np-f46-s5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root.join("config")).expect("config root");
    std::fs::create_dir_all(&root.join("data")).expect("data root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        // The sample's player, through the program's own seam: `voice_program()` falls back to this
        // when the settings file names no `preview.voicePlayer`. `/bin/true` takes the player's
        // arguments and exits 0, so a real child is really spawned and held.
        std::env::set_var("NAIVEPOST_VOICE_PLAYER", "/bin/true");
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
        path: "/media/lecture.mkv".into(),
        footage: true,
        narrator: 1,
        ..Default::default()
    });
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    let cut = Cut {
        segs: vec![Seg { s: 0.0, e: 10.0, ..Default::default() }],
        ..Default::default()
    };
    naivepost::cut::save(&cut, &tree).expect("cut.json written");
    // F4.4 S1's first question is whether a reference exists to clone; plant one so the sample's
    // leg is asked about the wire rather than about the missing file.
    let reference = tree.voice_ref_wav();
    std::fs::create_dir_all(reference.parent().expect("narrate/ has a parent"))
        .expect("narrate/ made");
    std::fs::write(&reference, b"RIFF fake voice reference").expect("reference written");

    release_last_window();
    let window = ui::build_window(app, &project, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let state = ui::NarrateState {
        entries: vec![Entry {
            s: 0.0,
            e: 10.0,
            at: 1.0,
            text: "placeholder".into(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        }],
        segs: cut.segs.clone(),
        takes: vec![],
        voice: "narrator1".into(),
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
        voice_files: vec![],
        recording_covers_clip: true,
        playing: false,
        busy: false,
        narration_off: false,
        covered_spans: vec![],
        language: "en".into(),
    };
    ui::set_state(state.clone());
    ui::refresh(&window);
    settle();

    let text = "This is the voice the narration will be spoken in.";
    let entry = widget_in(&window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .expect("the Narrate page carries sample-sentence");
    entry.set_text(text);
    settle();

    let sample = naivepost::voice_ref::sample_file(&tree, "narrator1", text);
    assert!(
        !sample.exists(),
        "the sample starts unspoken at {}",
        sample.display()
    );

    // ---- press 1: the leg runs, the take lands, and it is heard ------------------------------
    button(&window, "sample-play").emit_by_name::<()>("clicked", &[]);
    settle();
    for _ in 0..40 {
        if sample.is_file() {
            break;
        }
        settle();
    }
    assert!(
        sample.is_file(),
        "the take was filed under S5's own name at {}",
        sample.display()
    );
    let filed = std::fs::read(&sample).expect("the sample reads back");
    assert_eq!(
        &filed,
        &wav_bytes(2400),
        "the file holds the bytes that crossed the socket"
    );
    assert_eq!(
        status_text(&window),
        naivepost::narrate_details::sample_playing_status(),
        "the press reported the sample playing, not the ladder's asking sentence"
    );

    // §6: who spoke what is logged BEFORE any outcome can be reported, because a sample has no
    // output file to inspect afterwards when the dial itself fails. Pinned by index, not presence.
    let logs = ui::window_logs();
    let ask_line = naivepost::narrate_details::sample_log("narrator1", 0.0, None, text);
    let asked = logs
        .iter()
        .position(|line| line.contains(ask_line.as_str()))
        .unwrap_or_else(|| panic!("the press logged the ask first; logs were {logs:?}"));
    // `sample_file_log`'s OWN text, built here rather than matched loosely: the cached door writes
    // "spoken earlier", the fresh write writes "spoken in just now", and the two must not be
    // confused with each other when pinning the order.
    // The outcome line is told apart by its OWN wording, not by a loose "sample: " match: the fresh
    // write says `spoken in just now` and the cached door says `spoken earlier`, and confusing the two
    // is what made the ask and the outcome look like the same line.
    let outcome = "spoken in just now";
    let taken = logs
        .iter()
        .rposition(|line| line.contains(outcome))
        .unwrap_or_else(|| panic!("the press logged the take after the ask; logs were {logs:?}"));
    assert!(
        asked < taken,
        "§6's order: the ask (line {asked}) must precede the outcome (line {taken})"
    );

    let first = audio.drain();
    assert_eq!(
        speech_calls(&first),
        1,
        "one speech call for the first press: {:?}",
        first.iter().map(|s| s.path.clone()).collect::<Vec<_>>()
    );

    // ---- press 2: the same words, the same voice: no second call -----------------------------
    button(&window, "sample-play").emit_by_name::<()>("clicked", &[]);
    settle();
    let second = audio.drain();
    assert_eq!(
        speech_calls(&second),
        0,
        "S5 caches per voice and text, so the second press must not dial again; saw {:?}",
        second.iter().map(|s| s.path.clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        status_text(&window),
        naivepost::narrate_details::sample_playing_status(),
        "the cached take is heard, not refused"
    );

    // ---- press 3: busy is refused before anything is dialed ---------------------------------
    ui::set_state(ui::NarrateState { busy: true, ..state });
    ui::refresh(&window);
    settle();
    button(&window, "sample-play").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        naivepost::narrate_details::busy_sample_status(),
        "§6 puts 'still synthesizing the last sample' ahead of even a cached take"
    );
    let third = audio.drain();
    assert_eq!(
        speech_calls(&third),
        0,
        "a busy press puts nothing on the wire"
    );

    RAN_SAMPLE_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f4_6_s5_the_sample_play_leg_files_the_take_and_hears_it() {
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
        RAN_SAMPLE_WIRE.load(Ordering::SeqCst),
        "the F4.6 S5 sample-wire block never ran"
    );
}
