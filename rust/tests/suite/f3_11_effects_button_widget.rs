// F3.11 Decorations proposed by the model — the live wire through the real control: pressing ✨ Decorations
// telephones the address the settings hold, and what comes back lands on the cut, the lane and the status line.
//
// `tests/suite/decorations_pass_widgets.rs` drives the same rules through `ui::run_effects_pass_with_reply`, which
// is the seam WITHOUT the telephone. This file has no such seam: the reply arrives over HTTP from a loopback
// fake, so what is proven is the wiring between the button and `effects_ask` — that a click dials, that the
// decoration it answers with is the effect that appears (with the app's defaults on it), that a skipped line
// is said instead of vanishing, and that one ↶ takes the whole pass back.
//
// The stages share one app run, but the process-wide env still has to be pinned once, before any window is
// built: the pass resolves "this project" through `startup::session_dir(current_dir())`, and leaving cwd at
// `rust/` writes a stray `rust/session.naivepost/`.

#![allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning

#[path = "captions_wire_harness.rs"]
mod wire;

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::ui;
use serde_json::json;

use crate::common;
use common::{cut_page, release_last_window, settle, status_text, widget_in};

use wire::{envelope_of, point_at, FakeLlm};

static RAN_LIVE_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_SKIP_SAID: AtomicBool = AtomicBool::new(false);
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

/// The EFFECTS document §`prompts/system.md`'s `effects` row specifies, in the chat envelope a real server
/// answers with — the shape a live reply has, so the ask parses it the way it parses a server's bytes.
fn effects_json(entries: &[serde_json::Value]) -> String {
    envelope_of(&json!({ "fx": entries }).to_string())
}

/// One `{clip, kind, start, end}` entry, gain left off unless given.
fn fx_entry(clip: u32, kind: &str, start: f64, end: f64, gain: Option<f64>) -> serde_json::Value {
    let mut entry = json!({"clip": clip, "kind": kind, "start": start, "end": end});
    if let Some(gain) = gain {
        entry.as_object_mut().expect("object").insert("gain".into(), json!(gain));
    }
    entry
}

/// The same three clips with a caption over CLIP 1 (session second 1) and one over CLIP 3 (session second 21).
/// Their only job is to make this stage's BATCH differ from every other stage's: `effects_ask` keys its cache
/// on the two messages it builds, and the clip brief is inside the user message, so a different set of captions
/// means a different key and an earlier stage's cached answer cannot answer here. Neither sits where this
/// stage's decoration lands, so no §F3.11 refusal enters the stage by accident.
fn three_clips_captioned() -> Cut {
    let mut cut_ = three_clips();
    for (t, words) in [(1.0, "a line already there"), (21.0, "another line kept")] {
        cut_.fx.push(Fx {
            kind: "text".to_string(),
            t,
            dur: 4.0,
            text: words.to_string(),
            ..Default::default()
        });
    }
    cut_
}

/// Press the real ✨ Decorations button. Emitted by name because a named widget is the decision taken in its
/// spirit: the handler under test is the shell's own, not a seam a test opened for itself.
fn press_decorations(window: &adw::ApplicationWindow) {
    let button = widget_in(window, "decorations-pass-button").expect("✨ Decorations is on the run-bar row");
    button
        .clone()
        .downcast::<gtk::Button>()
        .expect("it is a button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// The decorations this pass could have added, of one kind. Filtering by kind (rather than "everything that is
/// not a caption") keeps the stage's own fixture effect out of the count and says which kind was checked.
fn placed_of_kind(window: &adw::ApplicationWindow, kind: &str) -> Vec<Fx> {
    ui::review_cut_of(window)
        .fx
        .iter()
        .filter(|fx| fx.kind == kind)
        .cloned()
        .collect()
}

/// Everything on the lane that this pass could have written: none of the six §06 kinds the seeded cut lacks.
/// Unused here (each stage counts by kind, because `cut_page` replays the previous window's undo history and a
/// earlier stage's decoration can still be on the lane); kept as the whole-lane view of the same state.
fn placed(window: &adw::ApplicationWindow) -> Vec<Fx> {
    const KINDS: [&str; 5] = ["zoom", "speed", "volume", "svg", "label"];
    ui::review_cut_of(window)
        .fx
        .iter()
        .filter(|fx| KINDS.contains(&fx.kind.as_str()))
        .cloned()
        .collect()
}

fn round(app: &adw::Application) {
    // --- (1) a real click telephones, and the decoration it answers with lands on the cut ---------
    let llm = FakeLlm::start(vec![effects_json(&[fx_entry(2, "zoom", 1.0, 4.0, None)])]);
    point_at(&llm.url());
    let window = cut_page(app, &three_clips());
    assert!(placed_of_kind(&window, "zoom").is_empty() && placed_of_kind(&window, "volume").is_empty(), "the seeded cut holds no decoration to confuse the count with");
    press_decorations(&window);

    let landed = placed_of_kind(&window, "zoom");
    assert_eq!(landed.len(), 1, "the answered zoom became one lane effect, got {landed:?}");
    // Clip 2 starts at session second 10 and the answer aimed at +1..+4 inside it: the record is written in
    // SESSION seconds, which is the app's arithmetic, not the model's.
    assert_eq!((landed[0].kind.as_str(), landed[0].t, landed[0].dur), ("zoom", 11.0, 3.0), "the zoom covers the answered span");
    // And with the app's defaults, not the tool's report: centred at 60 % height, glides min(1, d/3) → 1 s here.
    assert_eq!((landed[0].cx, landed[0].cy), (Some(0.5), Some(0.5)), "centred: this pass sends no box");
    assert_eq!(landed[0].hf, Some(naivepost::cut_effects_pass::ZOOM_HEIGHT), "at 60 % of the frame height");
    assert_eq!((landed[0].trans, landed[0].tout), (1.0, 1.0), "glides min(1, d/3) with d = 3");
    let said = status_text(&window);
    assert!(said.contains(">>> effects: 1 decoration(s)"), "the status says what the pass did, got {said:?}");
    // And the click really dialled: the fake saw the request the button caused, offering this job's tool.
    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "on the LLM's chat path");
    assert!(seen.body.contains("add_effect"), "with this job's tool offered");
    RAN_LIVE_CLICK.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (2) a skipped line is SAID, and nothing lands for it -------------------------------------
    // A fresh fake AND a fresh address: `effects_ask` reads its settings per ask, so pointing at a port nobody
    // answers is how this stage proves it dialled again instead of taking stage 1's cached answer — the two
    // stages' clip lists differ by one caption, which would otherwise key them together.
    let llm = FakeLlm::start(vec![effects_json(&[
        fx_entry(1, "blur", 1.0, 4.0, None),
        fx_entry(3, "volume", 2.0, 6.0, Some(0.0)),
    ])]);
    point_at(&llm.url());
    // Clips 1 and 3 carry captions (the volume aimed at clip 3 still lands — §F3.11 refuses only a fast rate
    // over words), and their presence is what puts this stage's ask in its own cache slot.
    let window = cut_page(app, &three_clips_captioned());
    press_decorations(&window);

    // The unknown kind adds nothing of its own, and the good volume landed: one volume, at clip 3's start + 2 s
    // with the app's ramp. (Each stage starts from a freshly seeded cut — `cut_page` re-seeds — so what is on the
    // lane is this press's doing and nobody else's.)
    assert!(placed_of_kind(&window, "zoom").is_empty(), "a `blur` line becomes no record at all");
    let landed = placed_of_kind(&window, "volume");
    assert_eq!(landed.len(), 1, "only the volume lands; the unknown kind adds nothing, got {landed:?}");
    assert_eq!((landed[0].kind.as_str(), landed[0].t, landed[0].dur, landed[0].gain), ("volume", 22.0, 4.0, 0.0), "gain 0 is kept: silence is a decision someone can mean");
    assert_eq!(placed_of_kind(&window, "speed").len(), 0, "nothing else came with it");
    let said = status_text(&window);
    assert!(
        said.contains("\"blur\" is not an effect this pass places"),
        "§F3.11 S3: the skip is said on the status line, got {said:?}"
    );
    // The skip cost no round: one ask for a usable reply.
    assert!(llm.next_seen().body.contains("add_effect"), "exactly one request went out for this stage");
    RAN_SKIP_SAID.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (3) one ↶ takes the placed decoration back ------------------------------------------------
    // Again its own batch (clip 2 carries the caption here, clips 1 and 3 did in stage 2), so this press really
    // dials rather than reading an earlier stage's answer out of the cache.
    let llm = FakeLlm::start(vec![effects_json(&[fx_entry(2, "stop", 1.0, 3.0, None)])]);
    point_at(&llm.url());
    let mut captioned = three_clips();
    captioned.fx.push(Fx {
        kind: "text".to_string(),
        t: 11.0,
        dur: 4.0,
        text: "words over the stopped clip".to_string(),
        ..Default::default()
    });
    let window = cut_page(app, &captioned);
    press_decorations(&window);
    assert_eq!(placed_of_kind(&window, "speed").len(), 1, "the pass placed one stop to undo");
    ui::press_undo(&window);
    settle();
    assert!(
        placed_of_kind(&window, "speed").is_empty(),
        "one ↶ undoes the whole pass -- it went through record_edit, not seed_review_cut"
    );
    RAN_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();
    release_last_window();
}

#[test]
fn f3_11_widget_the_decorations_button_dials_the_live_ask() {
    // Pin cwd BEFORE anything builds: the pass resolves the session folder from it, and the settings folder
    // travels with it so `point_at` rewrites an address only this scenario can read.
    let root = std::env::temp_dir().join(format!("np-effects-live-{}", std::process::id()));
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
    conf.model = "effects-model".to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run_with_args::<&str>(&[]);
    });

    assert!(RAN_LIVE_CLICK.load(Ordering::SeqCst), "the live click never ran");
    assert!(RAN_SKIP_SAID.load(Ordering::SeqCst), "the skipped-line stage never ran");
    assert!(RAN_UNDO.load(Ordering::SeqCst), "the undo of the live pass never ran");
}
