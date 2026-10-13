// F3.10 Speeds proposed by the model — the live wire through the real control: pressing ⏩ Speeds telephones
// the address the settings hold, and what comes back lands on the cut, the lane and the status line.
//
// `tests/speeds_pass_widgets.rs` drives the same rules through `ui::run_speeds_pass_with_reply`, which is the
// seam WITHOUT the telephone. This file has no such seam: the reply arrives over HTTP from a loopback fake, so
// what is proven is the wiring between the button and `speeds_ask` — that a click dials, that the rates it
// answers with are the effects that appear, and that a refusal reaches the status line instead of vanishing.
//
// One scenario per binary is NOT enough here (the stages share one app run), but the process-wide env still has
// to be pinned once, before any window is built: the pass resolves "this project" through
// `startup::session_dir(current_dir())`, and leaving cwd at `rust/` writes a stray `rust/session.naivepost/`.

#![allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning

#[path = "captions_wire_harness.rs"]
mod wire;

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::ui;
use serde_json::json;

#[allow(dead_code)]
mod common;
use common::{release_last_window, settle, status_text, widget_in, cut_page};

use wire::{envelope_of, point_at, FakeLlm};

static RAN_LIVE_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_REFUSED_SAID: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);

/// Three kept runs of ten seconds each: clip 1 = 0–10, clip 2 = 10–20, clip 3 = 20–30, so every second the
/// pass writes down is checkable by hand.
fn three_clips() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, cam: 0, ..Default::default() },
            Seg { s: 10.0, e: 20.0, cam: 0, ..Default::default() },
            Seg { s: 20.0, e: 30.0, cam: 0, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// The same three clips with one caption already over CLIP 2 (session second 11), which is what makes the
/// refusal stage real: the pass reads captions off the cut it was given, not off a list someone remembered
/// to pass.
fn three_clips_captioned_on_clip2() -> Cut {
    let mut cut_ = three_clips();
    cut_.fx.push(Fx {
        kind: "text".to_string(),
        t: 11.0,
        dur: 4.0,
        text: "a line already there".to_string(),
        ..Default::default()
    });
    cut_
}

/// The SPEEDS document §`prompts/system.md`'s `speed` row specifies, in the chat envelope a real server
/// answers with — the shape a live reply has, so the ask parses it the way it parses a server's bytes.
fn speeds_json(speeds: &[(u32, f64)]) -> String {
    envelope_of(
        &json!({
            "speeds": speeds.iter().map(|(clip, rate)| json!({ "clip": clip, "rate": rate })).collect::<Vec<_>>()
        })
        .to_string(),
    )
}

/// Press the real ⏩ Speeds button. Emitted by name because a named widget is the decision taken in its
/// spirit: the handler under test is the shell's own, not a seam a test opened for itself.
fn press_speeds(window: &adw::ApplicationWindow) {
    let button = widget_in(window, "speeds-pass-button").expect("⏩ Speeds is on the run-bar row");
    button
        .clone()
        .downcast::<gtk::Button>()
        .expect("it is a button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// The speed effects on the review cut, newest first being the ones this pass added.
fn speeds(window: &adw::ApplicationWindow) -> Vec<Fx> {
    ui::review_cut_of(window)
        .fx
        .iter()
        .filter(|fx| fx.kind == "speed")
        .cloned()
        .collect()
}

fn round(app: &adw::Application) {
    // --- (1) a real click telephones, and the rate it answers with lands on the cut ---------------
    let llm = FakeLlm::start(vec![speeds_json(&[(2, 1.5)])]);
    point_at(&llm.url());
    let window = cut_page(app, &three_clips());
    press_speeds(&window);

    let placed = speeds(&window);
    assert_eq!(placed.len(), 1, "the answered rate became one speed effect, got {placed:?}");
    // Clip 2 sits at session second 10 and is ten seconds long: the effect covers the WHOLE clip at the
    // rate the tool reported applying, which is the record the render will read.
    assert_eq!(placed[0].t, 10.0, "over clip 2's own start");
    assert_eq!(placed[0].dur, 10.0, "covering the whole clip");
    assert_eq!(placed[0].rate, 1.5, "at the rate the answer asked for");
    let said = status_text(&window);
    assert!(said.contains("run fast"), "the status says what the pass did, got {said:?}");
    // And the click really dialled: the fake saw the request the button caused.
    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "on the LLM's chat path");
    assert!(seen.body.contains("set_clip_speed"), "with this job's tool offered");
    RAN_LIVE_CLICK.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (2) a refused rate is SAID, not swallowed ------------------------------------------------
    // A fresh fake: this stage's reply must be its own, or the cached answer from stage 1 would answer it.
    let llm = FakeLlm::start(vec![speeds_json(&[(2, 2.0)])]);
    point_at(&llm.url());
    let window = cut_page(app, &three_clips_captioned_on_clip2());
    let before = ui::review_cut_of(&window).fx.len();
    press_speeds(&window);

    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before,
        "a fast rate over a captioned clip adds nothing -- the caption is the only new effect there"
    );
    assert!(speeds(&window).is_empty(), "no speed landed over the captioned clip, got {:?}", speeds(&window));
    let said = status_text(&window);
    assert!(
        said.contains("cannot be sped up"),
        "§F3.10 S2: the refusal is said on the status line, got {said:?}"
    );
    RAN_REFUSED_SAID.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (3) one ↶ takes the placed speed back ----------------------------------------------------
    let llm = FakeLlm::start(vec![speeds_json(&[(2, 1.5)])]);
    point_at(&llm.url());
    let window = cut_page(app, &three_clips());
    press_speeds(&window);
    assert_eq!(speeds(&window).len(), 1, "the pass placed one speed to undo");
    ui::press_undo(&window);
    settle();
    assert!(
        speeds(&window).is_empty(),
        "one ↶ undoes the whole pass -- it went through record_edit, not seed_review_cut"
    );
    RAN_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();
    release_last_window();
}

#[test]
fn f3_10_widget_the_speeds_button_dials_the_live_ask() {
    // Pin cwd BEFORE anything builds: the pass resolves the session folder from it, and the settings folder
    // travels with it so `point_at` rewrites an address only this scenario can read.
    let root = std::env::temp_dir().join(format!("np-speeds-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("scenario folders created");
    std::fs::create_dir_all(root.join("data")).expect("scenario folders created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    // The address the ask must dial: written now, rewritten per stage by `point_at`.
    let paths = naivepost::settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "speeds-model".to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run();
    });

    assert!(RAN_LIVE_CLICK.load(Ordering::SeqCst), "the live click never ran");
    assert!(RAN_REFUSED_SAID.load(Ordering::SeqCst), "the refused-rate stage never ran");
    assert!(RAN_UNDO.load(Ordering::SeqCst), "the undo of the live pass never ran");
}
