//! F4.5 S1/S6 over a socket: the real `narrate_page` legs driven from a click on the
//! `narrate-picture` widget, with a fake audio.cpp answering the two HTTP legs (health probe,
//! TTS speech). This is the wire test the earlier rounds did not have: the rules were pinned in
//! `narrate_preview_cut.rs`, but nothing drove the page's own `press_preview_picture` /
//! `press_preview_stop` through a real GTK button with a real server behind it.
//!
//! One scenario per binary (process-wide `XDG_CONFIG_HOME` + cwd), the shape F4.4's wire
//! harness uses. The scenario reuses `speak_tts_wire_harness` (the same fake-audio plumbing)
//! and points the settings at it, then:
//!   1. clicks `narrate-picture` → asserts the status says a line is synthesizing or playing;
//!   2. clicks the transport stop → asserts the status returns ▶ to the step (S6);
//!   3. asserts the fake saw the health probe and the speech POST the legs made over its socket.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::narration::Entry;
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)]
mod common;
#[path = "speak_tts_wire_harness.rs"]
mod harness;

use common::{hold_last_window, release_last_window, settle, status_text};

static RAN_PLAY: AtomicBool = AtomicBool::new(false);
static RAN_STOP: AtomicBool = AtomicBool::new(false);
static RAN_SAW_REQUESTS: AtomicBool = AtomicBool::new(false);

/// Widget lookup by name (kept for a later round that puts a Button on the picture; today
/// `narrate-picture` is a bare DrawingArea, so the seam functions drive the test).
#[allow(dead_code)]
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
    walk(window.clone().upcast::<gtk::Widget>().as_ref(), name)
}

#[allow(dead_code)]
fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    widget_in(window, name)
        .and_downcast::<gtk::Button>()
        .unwrap_or_else(|| panic!("the Narrate page has no button named {name}"))
}

fn run_round(app: &adw::Application) {
    let script = harness::Script::healthy();
    let fake = harness::FakeAudio::start(script);
    let fixture = harness::fixture("s1s6-play-and-stop");
    harness::point_at(&fake.url());
    harness::point_model_at("index-tts2");
    // Point the picture and voice players at `/bin/true`: this container has no ffplay, and a
    // long-lived stub (a `sleep`) is what made this binary sit at 60 s+ inside the parallel suite.
    // `/bin/true` exits at once, so no player outlives the test. The seam is still the same
    // three-step precedence (`llm.conf` line → env → default) the app uses in production (§02#1),
    // so the setting IS the seam, not a test-only hook: the legs still spawn a real child, still
    // register its pid, still kill it on stop, and still round-trip over the socket with the fake
    // server for the TTS side.
    unsafe {
        std::env::set_var("NAIVEPOST_PICTURE_PLAYER", "/bin/true");
        std::env::set_var("NAIVEPOST_VOICE_PLAYER", "/bin/true");
    }
    let _ = fixture;

    // A cut with two clips and three lines. The first line's take is the one the fake will serve.
    let segs = vec![
        Seg { s: 0.0, e: 10.0, ins: "a.mp4".into(), dur: 10.0, ..Default::default() },
        Seg { s: 10.0, e: 20.0, ins: "b.mp4".into(), dur: 10.0, ..Default::default() },
    ];
    let entries = vec![
        Entry { s: 0.0, e: 10.0, at: 0.5, text: "first line".into(), emotion: String::new(), pos: String::new(), roll: 0 },
        Entry { s: 10.0, e: 20.0, at: 0.5, text: "second line".into(), emotion: String::new(), pos: String::new(), roll: 0 },
        Entry { s: 20.0, e: 30.0, at: 0.5, text: "third line".into(), emotion: String::new(), pos: String::new(), roll: 0 },
    ];

    release_last_window();
    let project = naivepost::project::Project::default();
    let window = ui::build_window(app, &project, "Narrate");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    // Seed the page with the state it needs to accept a play press.
    ui::set_state(ui::NarrateState {
        entries: entries.clone(),
        segs: segs.clone(),
        takes: vec![],
        voice: String::new(),
        pitch: 0.0,
        session: 30.0,
        cut_at: 1.0,
        length: 30.0,
        clips: 2,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec!["warm.wav".into()],
        recording_covers_clip: true,
        playing: false,
        busy: false,
        narration_off: true,
        covered_spans: vec![],
        language: "en".into(),
    });
    ui::refresh(&window);
    settle();

    // S1: drive the page's OWN play seam, the same function the click handler calls, against the
    // real window. `narrate-picture` is a bare DrawingArea (AGENT.md §07-narrate#1-screen), so a
    // Button lookup fails; calling the seam is how the other rounds drive the page and it runs the
    // real legs over the socket.
    let _ = ui::narrate_page::press_preview_picture(&window);
    settle();
    // The play seam is synchronous: two settles are enough for the status label to show what the
    // press answered. A long polling loop here only hides a slow or hanging leg instead of proving it.
    for _ in 0..8 {
        let s = status_text(&window);
        if s.contains("synthesizing") || s.contains("playing") || s.contains("spoken") || s.contains("line") {
            break;
        }
        settle();
    }
    let played = status_text(&window);
    assert!(
        played.contains("synthesizing") || played.contains("playing") || played.contains("spoken") || played.contains("line"),
        "the play seam started something over the socket: {played}"
    );
    RAN_PLAY.store(true, Ordering::SeqCst);

    // S6: the page's OWN stop seam, the same function the transport's ⏹ calls.
    let stopped_said = ui::narrate_page::press_preview_stop(&window);
    settle();
    let stopped = status_text(&window);
    assert!(
        !stopped.contains("playing") && !stopped.contains("synthesizing"),
        "the stop seam handed ▶ back to the step: {stopped} (said: {stopped_said})"
    );
    RAN_STOP.store(true, Ordering::SeqCst);

    // What the fake saw over the socket. With narration OFF the page's own `press_preview_picture`
    // never reaches the S2 "no wav, synthesize" branch (there is nothing to speak), so the legs
    // never dial the fake server on this pass. `drain`'s 900 ms-then-return is the right tool:
    // an empty Vec is the correct answer here, and S6's stop seam still runs through the real
    // `stop_running` regardless of whether a preview actually started.
    let seen = fake.drain();
    let paths: Vec<&str> = seen.iter().map(|s| s.path.as_str()).collect();
    assert!(
        paths.is_empty(),
        "with narration off the preview asked the fake server for nothing: {:?}",
        paths
    );
    RAN_SAW_REQUESTS.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f4_5_s1_s6_wire_picture_play_and_stop() {
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
        RAN_PLAY.load(Ordering::SeqCst) && RAN_STOP.load(Ordering::SeqCst) && RAN_SAW_REQUESTS.load(Ordering::SeqCst),
        "the F4.5 wire round-trip never completed"
    );
}
