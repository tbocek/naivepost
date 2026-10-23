// §06-effects.md §3 Flows — the WIRE half. The flows start at a user action, so every check here presses the
// real run-bar button (✐ Captions / ⏩ Speeds / ✨ Decorations) and reads the result out of the SAME state the
// logic tests read: `ui::review_cut_of(&window).fx`, which is what `cut_suggest_passes::run_after_cut` counts
// when it answers §S3's effect line. `tests/suite/sec_06_effects_3_flows_order.rs` proves the rules with answers
// handed in as text; `f3_9_s*`/`f3_10_*`/`f3_11_*` prove each pass on its own. What is proven here is the seam:
// the click dials the address the settings hold, the answered work lands on the cut, and a pass the policy
// switched off neither greys-in nor places anything — the wire agreeing with `cut_suggest_passes::order`'s filter.
//
// The replies arrive over HTTP from the loopback fake (`captions_wire_harness.rs`), pointed at through the
// settings conf, so NO live server is dialled and no canned reply stands where the program would telephone.
//
// One app run holds all the stages; the process-wide env (cwd, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`) is pinned
// once before any window is built, because each pass resolves "this project" through
// `startup::session_dir(current_dir())` and leaving cwd at `rust/` writes a stray `rust/session.naivepost/`.

#![allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning

#[path = "captions_wire_harness.rs"]
mod wire;

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Fx;
use naivepost::ui;
use serde_json::json;

use crate::common;
use common::{cut_page, release_last_window, settle, wait_until, widget_in};

use wire::{add_captions_json, add_effects_json, point_at, set_speeds_json, FakeLlm};

static RAN_CAPTIONS: AtomicBool = AtomicBool::new(false);
static RAN_SPEEDS: AtomicBool = AtomicBool::new(false);
static RAN_DECORATIONS: AtomicBool = AtomicBool::new(false);
static RAN_OFF_GATE: AtomicBool = AtomicBool::new(false);

/// Three kept runs of ten seconds each: clip 1 = 0–10, clip 2 = 10–20, clip 3 = 20–30, so every second a pass
/// writes down is checkable by hand.
/// Press one of the three run-bar job buttons by its stable widget name. Emitted by name because a named widget
/// is the decision taken in its spirit: the handler under test is the shell's own, not a seam a test opened for
/// itself.
fn press(window: &adw::ApplicationWindow, name: &str) {
    let button = widget_in(window, name).expect("the pass's button is on the run-bar row");
    button
        .clone()
        .downcast::<gtk::Button>()
        .expect("it is a button")
        .emit_by_name::<()>("clicked", &[]);
}

/// The pass's effects on the review cut, by §06 kind. Reading the cut rather than the status line is the point:
/// this is the state `cut_suggest_passes::run_after_cut` counts, and it is what the render will read.
fn placed_of_kind(window: &adw::ApplicationWindow, kind: &str) -> Vec<Fx> {
    ui::review_cut_of(window)
        .fx
        .iter()
        .filter(|fx| fx.kind == kind)
        .cloned()
        .collect()
}

/// The `set_clip_speed` CALL §3.7's tool row spells for this job: since §3's answer on this item the answer IS
/// a call list, so a prose `{"speeds": […]}` document is no longer an answer (see `sec_06_effects_3_flows_s1_plain`).
fn speeds_json(speeds: &[(u32, f64)]) -> String {
    set_speeds_json(speeds)
}

/// The `add_effect` CALLS §3.7's tool row spells for this job, in the same answering shape.
fn effects_json(entries: &[serde_json::Value]) -> String {
    add_effects_json(entries)
}

/// The wire for one pass: press the button and WAIT for the effect kind to appear — on the fx list the pass
/// writes — rather than for a count of turns, because the reply arrives from another thread.
fn wait_for_kind(window: &adw::ApplicationWindow, button: &str, kind: &str) -> Vec<Fx> {
    press(window, button);
    let mut landed: Vec<Fx> = Vec::new();
    wait_until(&format!("a {kind} effect placed by {button}"), || {
        landed = placed_of_kind(window, kind);
        !landed.is_empty()
    });
    landed
}

fn round(app: &adw::Application) {
    // --- ✐ Captions: the caption the fake answered with is on the cut the shell holds ----------------
    // A caption over CLIP 1 at +1..+3 inside it: the model answers in clip-relative offsets (§F3.9) and the
    // app turns them into session seconds, so the record reads t = 1.0, dur = 2.0.
    let llm = FakeLlm::start(vec![add_captions_json(&[(1, 1.0, 3.0, "the reveal")])]);
    point_at(&llm.url());
    let window = cut_page(app, &common::three_clips_cut());
    assert!(
        placed_of_kind(&window, "text").is_empty(),
        "the seeded cut holds no caption to confuse the count"
    );
    let captions = wait_for_kind(&window, "captions-pass-button", "text");
    assert_eq!(
        captions.len(),
        1,
        "the answered caption landed: {captions:?}"
    );
    assert_eq!(
        (captions[0].t, captions[0].dur, captions[0].text.as_str()),
        (1.0, 2.0, "the reveal")
    );
    // The press really dialled the address the settings hold, offering this job's tool.
    let seen = llm.next_seen();
    assert_eq!(
        (seen.method.as_str(), seen.path.as_str()),
        ("POST", "/v1/chat/completions")
    );
    assert!(
        seen.body.contains("add_caption"),
        "the captions tool was offered: the body names it"
    );
    // The pass's own sentence reached the window log -- the same seam `run_after_cut` copies into the run's log.
    wait_until("the captions pass's line in the window log", || {
        log_said(|line| line.contains("caption"))
    });
    RAN_CAPTIONS.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- ⏩ Speeds: the rate it was answered with is the effect on the cut ---------------------------
    // Clip 2, at rate 2.0: the effect covers the WHOLE clip, in session seconds, at the answered rate.
    let llm = FakeLlm::start(vec![speeds_json(&[(2, 2.0)])]);
    point_at(&llm.url());
    let window = cut_page(app, &common::three_clips_cut());
    let speeds = wait_for_kind(&window, "speeds-pass-button", "speed");
    assert_eq!(
        speeds.len(),
        1,
        "the answered rate became one speed effect: {speeds:?}"
    );
    assert_eq!(
        (speeds[0].t, speeds[0].dur, speeds[0].rate),
        (10.0, 10.0, 2.0),
        "over clip 2, at the answered rate"
    );
    assert!(
        llm.next_seen().body.contains("set_clip_speed"),
        "the speeds tool was offered"
    );
    wait_until("the speed pass's line in the window log", || {
        log_said(|line| line.contains("speed"))
    });
    RAN_SPEEDS.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- ✨ Decorations: the answered zoom lands with the APP's §F3.11 defaults, not the model's ----
    // The answer names only the span: §F3.11 keeps the box the app's own — centred, `ZOOM_HEIGHT` tall,
    // glides min(1, d/3) → 1 s over this 3 s span.
    let llm = FakeLlm::start(vec![effects_json(&[
        json!({"clip": 2, "kind": "zoom", "start": 1.0, "end": 4.0}),
    ])]);
    point_at(&llm.url());
    let window = cut_page(app, &common::three_clips_cut());
    let zooms = wait_for_kind(&window, "decorations-pass-button", "zoom");
    assert_eq!(
        zooms.len(),
        1,
        "the answered zoom became one lane effect: {zooms:?}"
    );
    assert_eq!(
        (zooms[0].t, zooms[0].dur),
        (11.0, 3.0),
        "the answered span, in session seconds"
    );
    assert_eq!(
        (zooms[0].cx, zooms[0].cy),
        (Some(0.5), Some(0.5)),
        "centred: this pass sends no box"
    );
    assert_eq!(
        zooms[0].hf,
        Some(naivepost::cut_effects_pass::ZOOM_HEIGHT),
        "at 60 % of the frame height"
    );
    assert_eq!(
        (zooms[0].trans, zooms[0].tout),
        (1.0, 1.0),
        "glides min(1, d/3) with d = 3"
    );
    assert!(
        llm.next_seen().body.contains("add_effect"),
        "the decoration tool was offered"
    );
    RAN_DECORATIONS.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- a pass the policy switched off: greyed, and a forced press places nothing -------------------
    // Three windows, one per switch, because each pass reads its own field: the grey-out and the refusal are
    // the SAME predicate the run-after-the-cut order filters on (`policy::pass_runs`).
    struct Gate {
        set: fn(&adw::ApplicationWindow, bool),
        refresh: fn(&adw::ApplicationWindow),
        button: fn(&adw::ApplicationWindow) -> Option<gtk::Button>,
        run: fn(&adw::ApplicationWindow) -> String,
        name: &'static str,
        kind: &'static str,
        says: &'static str,
    }
    let gates = [
        Gate {
            set: ui::set_captions_pass,
            refresh: ui::refresh_captions_gate,
            button: ui::captions_pass_button,
            run: ui::run_captions_pass,
            name: "captions-pass-button",
            kind: "text",
            says: "captions",
        },
        Gate {
            set: ui::set_speeds_pass,
            refresh: ui::refresh_speeds_gate,
            button: ui::speeds_pass_button,
            run: ui::run_speeds_pass,
            name: "speeds-pass-button",
            kind: "speed",
            says: "speed",
        },
        Gate {
            set: ui::set_decorations_pass,
            refresh: naivepost::ui::window::refresh_decorations_gate,
            button: ui::decorations_pass_button,
            run: ui::run_effects_pass,
            name: "decorations-pass-button",
            kind: "zoom",
            says: "decoration",
        },
    ];
    for gate in gates {
        // A live answer waits behind each gate: were the press to get through, work WOULD land, so "nothing
        // changed" is a real verdict about the gate rather than about an unreachable model.
        let llm = FakeLlm::start(vec![match gate.kind {
            "text" => add_captions_json(&[(1, 1.0, 3.0, "would have landed")]),
            "speed" => speeds_json(&[(1, 2.0)]),
            _ => effects_json(&[json!({"clip": 1, "kind": "zoom", "start": 1.0, "end": 4.0})]),
        }]);
        point_at(&llm.url());
        let window = cut_page(app, &common::three_clips_cut());
        (gate.set)(&window, false);
        (gate.refresh)(&window);
        let button = (gate.button)(&window).expect("the pass's button is on the run-bar row");
        assert!(
            !button.is_sensitive(),
            "{} off greys its control",
            gate.says
        );
        assert_eq!(
            ui::review_cut_of(&window).fx.len(),
            0,
            "the seeded cut holds nothing to begin with"
        );
        // GTK swallows a click on an insensitive control, so the emission is the honest test of the wire; the
        // direct call proves the pass itself refuses too, which is what `order` relies on.
        button.emit_by_name::<()>("clicked", &[]);
        settle();
        let refused = (gate.run)(&window);
        assert!(
            refused.contains("off"),
            "the pass refuses its own press, got {refused}"
        );
        assert_eq!(
            ui::review_cut_of(&window).fx.len(),
            0,
            "and a pass that is off places nothing"
        );
        assert!(
            placed_of_kind(&window, gate.kind).is_empty(),
            "no {} landed while off",
            gate.kind
        );
        // Nothing was dialled: a 1 s timeout on the fake's channel is the whole wait, and an off pass that
        // phoned would leave a request behind. §S2b's rule is "never called", not "called and told nothing".
        assert!(
            llm.seen_within(std::time::Duration::from_millis(250))
                .is_none(),
            "an off pass never dials the model -- it is not called and told to return nothing"
        );
        // Back on: the grey is never sticky, so the gate tracks the field the pass reads.
        (gate.set)(&window, true);
        (gate.refresh)(&window);
        assert!(
            button.is_sensitive(),
            "{} back on re-enables its control",
            gate.says
        );
        window.close();
        settle();
    }
    RAN_OFF_GATE.store(true, Ordering::SeqCst);
    release_last_window();
}

#[test]
fn sec_06_effects_3_flows_s3_pressing_captions_places_through_the_same_state_the_pass_reports() {
    // One app run holds every stage: the env is process-wide, so it is pinned once, before any window exists.
    pin_scenario_env("flows-wired");
    run_app_once(round);
    assert!(
        RAN_CAPTIONS.load(Ordering::SeqCst),
        "the ✐ Captions stage never ran"
    );
    assert!(
        RAN_SPEEDS.load(Ordering::SeqCst),
        "the ⏩ Speeds stage never ran"
    );
    assert!(
        RAN_DECORATIONS.load(Ordering::SeqCst),
        "the ✨ Decorations stage never ran"
    );
    assert!(
        RAN_OFF_GATE.load(Ordering::SeqCst),
        "the pass-off gate stage never ran"
    );
}

/// Any window-log line matching `says` — the program's own log seam (`ui::window_logs`), scoped by the word
/// the pass's sentence contains rather than by the status line, which several parts of the page also write.
fn log_said(says: impl Fn(&str) -> bool) -> bool {
    ui::window_logs().iter().any(|line| says(line))
}

/// Pin cwd, settings and data under one temp root: the passes resolve the session folder from cwd, and the
/// LLM address travels in the settings file, so a stage's `point_at` rewrites an address only this scenario reads.
fn pin_scenario_env(tag: &str) {
    let root = std::env::temp_dir().join(format!("np-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("scenario config folder");
    std::fs::create_dir_all(root.join("data")).expect("scenario data folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let paths =
        naivepost::settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "flows-model".to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written");
}

fn run_app_once(round: fn(&adw::Application)) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run_with_args::<&str>(&[]);
    });
}
