// F3.11 Decorations proposed by the model — the WIRE: the pass's own door reaches the rules in
// `naivepost::cut_effects_pass`, and what they answer lands on the cut, the lane,
// the status line and the log instead of staying inside a callback. The trigger is §S2b's:
// `cut_suggest_land` → `cut_suggest_passes::run_after_cut` asks this door LAST after the cut lands, filtered
// by P.policy.decorationsPass — F3.11 names no control.
//
// `rust/tests/suite/cut_effects_proposed.rs` proves the rules themselves (the one-call brief that names the captions
// and the rates, the whole-reply rejection for a clip outside the list, the per-call skip of an unknown kind or
// an empty span, the app's defaults per kind, gain 1 ignored / 0 kept, the two lines and the two-round budget);
// this file proves the Cut page can actually reach them. Before this round `grep -rn cut_effects_pass src/ui/`
// returned nothing at all: the module was complete and unreachable, which is what a green logic suite cannot see.
//
// The window trick is copied from `speeds_pass_widgets.rs`, which is the shape that terminates here: a window per
// block, closed and drained between blocks, and a `settle()` that never blocks. A closed GTK window is not
// destroyed, so `release_last_window()` drops the previous one before the next build takes the widget names.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx};
use naivepost::cut_effects_pass::{self as pass, Call};
use naivepost::ui;

use crate::common;
use common::{cut_page, settle, status_text, widget_in};

static RAN_DOOR_JOBS: AtomicBool = AtomicBool::new(false);
static RAN_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_PLACED: AtomicBool = AtomicBool::new(false);
static RAN_LANE: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_GATE: AtomicBool = AtomicBool::new(false);
static RAN_NO_ANSWER: AtomicBool = AtomicBool::new(false);

/// Every `fx-bar-*` name under the effects lane. Which kinds earn a bar is `fx_lane::paused_scene`'s decision
/// (a volume is heard, not drawn), so this collects what the view drew rather than tallying a number by hand.
fn lane_bar_names(window: &adw::ApplicationWindow) -> Vec<String> {
    fn gather(node: &gtk::Widget, out: &mut Vec<String>) {
        let name = node.widget_name().to_string();
        if name.starts_with("fx-bar-") {
            out.push(name);
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                gather(&current, out);
                cursor = current.next_sibling();
            }
        }
    }
    let Some(lane) = widget_in(window, "effects-lane") else {
        return Vec::new();
    };
    let mut names = Vec::new();
    gather(&lane, &mut names);
    names
}

// F3.11 has no control to find by name any more: the passes are jobs of the run after the cut, so each block
// drives the program's own door (`run_effects_pass`, `run_after_cut`) instead of emitting a click.

/// One `add_effect` call as §3.7's tool receives it: a clip number, a kind, two offsets inside that clip, and
/// volume's gain when it has one.
fn call(clip: u32, kind: &str, start: f64, end: f64, gain: Option<f64>) -> Call {
    Call {
        clip,
        kind: kind.to_string(),
        start,
        end,
        gain,
    }
}

/// Every fx of `kind` in a cut, so a count can be read without caring where each landed.
fn kinds<'a>(cut_: &'a Cut, kind: &str) -> Vec<&'a Fx> {
    cut_.fx.iter().filter(|fx| fx.kind == kind).collect()
}

// --- the round ----------------------------------------------------------------------------------------

fn decorations_round(app: &adw::Application) {
    // --- (1) the trigger: the pass runs as a job of the run after the cut, in §S2b's order ---------
    // No control starts it: F3.11's trigger is the run after the cut, so
    // what is proven here is the door `cut_suggest_land` walks: `run_after_cut` over the policy's switches.
    // Decorations is asked LAST — captions, then speeds, then decorations — and with no endpoint wired each
    // pass gets no calls, so nothing is invented into the cut while all three are still reached.
    let window = cut_page(app, &common::three_clips_cut());
    assert!(
        ui::review_cut_of(&window).fx.is_empty(),
        "the seeded cut holds nothing to confuse the count"
    );
    let lines =
        naivepost::cut_suggest_passes::run_after_cut(&window, &ui::session_policy(&window), |_| {});
    assert!(
        lines.is_empty(),
        "with no model dialled no pass claims to have placed anything -- got {lines:?}"
    );
    assert_eq!(
        status_text(&window),
        "nothing proposed \u{2014} the cut stands as it is",
        "the last pass asked (decorations) is the one whose line the run ends on"
    );
    RAN_DOOR_JOBS.store(true, Ordering::SeqCst);
    eprintln!("DECO block 1 (row) done");
    window.close();
    settle();

    // --- (2) the pass answers, and invents nothing when no model was dialled ----------------------
    // A page with no endpoint wired makes NO calls, and "nothing was proposed" is a USABLE reply
    // (`Reply::usable()` true, no fault), so the honest line is the empty-result one; `pass::no_answer()`
    // belongs to block (6), where a reply actually faults twice.
    let window = cut_page(app, &common::three_clips_cut());
    let before = ui::review_cut_of(&window).fx.len();
    let said = ui::run_effects_pass(&window);
    settle();
    eprintln!("DECO block 2 (no model) done: {said}");
    assert_eq!(
        status_text(&window),
        "nothing proposed \u{2014} the cut stands as it is",
        "the pass run with no model dialled never ran"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before,
        "nothing was written when nothing was proposed"
    );
    RAN_CLICK.store(true, Ordering::SeqCst);
    eprintln!("DECO block 2 (no model) done");
    window.close();
    settle();

    // --- (3) a scripted reply lands with the app's defaults, and the skips are SAID --------------
    // A 3 s zoom on clip 1 (§F3.11's 2–4 s), a stop on clip 2, a ducking volume on clip 3, plus a volume
    // whose gain means "no change" and a kind this build does not know.
    let window = cut_page(app, &common::three_clips_cut());
    let before = ui::review_cut_of(&window).fx.len();
    let said = ui::run_effects_pass_with_reply(
        &window,
        &[
            call(1, "zoom", 2.0, 5.0, None),
            call(2, "stop", 1.0, 3.0, None),
            call(3, "volume", 0.0, 4.0, Some(0.5)),
            call(3, "volume", 5.0, 8.0, Some(1.0)),
            call(1, "blur", 0.0, 2.0, None),
        ],
    );
    settle();
    let placed_line = pass::placed(3);
    assert!(
        said.starts_with(&placed_line),
        "three decorations landed and the door said so -- got {said:?}"
    );
    // The skipped calls ride on the SAME line: a user counting bars should see why there are three, not five.
    assert!(
        said.len() > placed_line.len(),
        "the skipped gain-1 / unknown-kind calls are SAID, not silently dropped -- got {said:?}"
    );
    let cut_after = ui::review_cut_of(&window);
    assert_eq!(
        cut_after.fx.len(),
        before + 3,
        "exactly three fx were written"
    );
    // S4: a proposed zoom is centred, at ZOOM_HEIGHT of the frame, gliding min(1, d/3).
    let zoom = kinds(&cut_after, "zoom").remove(0);
    assert_eq!(
        zoom.cx,
        Some(0.5),
        "a proposed zoom is centred horizontally"
    );
    assert_eq!(zoom.cy, Some(0.5), "a proposed zoom is centred vertically");
    assert_eq!(
        zoom.hf,
        Some(pass::ZOOM_HEIGHT),
        "…at the pass's own frame-height share"
    );
    assert!(
        (zoom.t - 2.0).abs() < 1e-6,
        "clip 1's offset 2.0 lands at session 2.0"
    );
    assert!(
        (zoom.dur - 3.0).abs() < 1e-6,
        "the 3 s span the model asked for"
    );
    assert!(
        (zoom.trans - pass::glide(3.0)).abs() < 1e-9 && (zoom.tout - pass::glide(3.0)).abs() < 1e-9,
        "the zoom carries S4's glide"
    );
    // S4: a stop is a speed whose rate is 0, on its clip's own session seconds, faded min(0.3, d/4).
    let stops = kinds(&cut_after, "speed");
    assert_eq!(stops.len(), 1, "the stop became a speed effect");
    assert_eq!(
        stops[0].rate, 0.0,
        "a stop holds the picture still -- rate 0"
    );
    assert!(
        (stops[0].t - 11.0).abs() < 1e-6,
        "clip 2 starts at 10.0, offset 1.0 lands at 11.0"
    );
    assert!(
        (stops[0].trans - pass::stop_fade(2.0)).abs() < 1e-9,
        "the stop carries S4's fade"
    );
    // S4/S5: the gain the model sent is the gain that lands; gain 1 wrote nothing at all.
    let vols = kinds(&cut_after, "volume");
    assert_eq!(
        vols.len(),
        1,
        "only the ducking volume was written -- gain 1 is ignored, not stored"
    );
    assert_eq!(
        vols[0].gain, 0.5,
        "the gain the model sent is the gain that lands"
    );
    assert!(
        !cut_after.fx.iter().any(|fx| fx.kind == "blur"),
        "an unknown kind never becomes an effect"
    );
    RAN_PLACED.store(true, Ordering::SeqCst);
    eprintln!("DECO block 3 (placed) done");

    // --- (4) the lane drew them ---------------------------------------------------------------
    let drawn = lane_bar_names(&window);
    assert!(
        !drawn.is_empty(),
        "the lane drew the placed decorations -- got {drawn:?}"
    );
    RAN_LANE.store(true, Ordering::SeqCst);
    eprintln!("DECO block 4 (lane) done");
    window.close();
    settle();

    // --- (5) one ↶ takes the whole pass back ---------------------------------------------------
    let window = cut_page(app, &common::three_clips_cut());
    let said = ui::run_effects_pass_with_reply(&window, &[call(1, "zoom", 2.0, 5.0, None)]);
    settle();
    assert!(
        said.starts_with(&pass::placed(1)),
        "the pass reported its result, got {said:?}"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        1,
        "the decoration is on the cut before the undo"
    );
    ui::press_undo(&window);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "one ↶ undoes the whole pass -- it went through record_edit, not seed_review_cut"
    );
    RAN_UNDO.store(true, Ordering::SeqCst);
    eprintln!("DECO block 5 (undo) done");
    window.close();
    settle();

    // --- (6) the gate: decorationsPass off makes the pass refuse and place nothing ----------------
    // No control greys out any more — F3.11 is a job of the run after the cut, not a button — so what the
    // switch owns is the refusal itself, the same predicate `cut_suggest_passes::order` filters on.
    let window = cut_page(app, &common::three_clips_cut());
    ui::set_decorations_pass_for_test(&window, false);
    let refused = ui::run_effects_pass(&window);
    assert!(
        refused.contains("decorations are off"),
        "the pass itself refuses, not only the paint -- got {refused:?}"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "a gated pass writes nothing at all"
    );
    // The gate sits in the shared body, so the scripted-reply door cannot get around it either.
    let still_refused =
        ui::run_effects_pass_with_reply(&window, &[call(1, "zoom", 1.0, 4.0, None)]);
    assert!(
        still_refused.contains("decorations are off"),
        "nor does the with_reply seam, got {still_refused:?}"
    );
    ui::set_decorations_pass_for_test(&window, true);
    let reopened = ui::run_effects_pass_with_reply(&window, &[call(1, "zoom", 1.0, 4.0, None)]);
    assert!(
        !reopened.contains("decorations are off"),
        "switching the policy back on lets the pass run again, got {reopened:?}"
    );
    RAN_GATE.store(true, Ordering::SeqCst);
    eprintln!("DECO block 6 (gate) done");
    window.close();
    settle();

    // --- (7) a clip outside the batch loses the WHOLE reply, twice and no more ------------------
    let window = cut_page(app, &common::three_clips_cut());
    let said = ui::run_effects_pass_with_reply(
        &window,
        &[
            call(1, "zoom", 1.0, 4.0, None),
            call(99, "volume", 0.0, 2.0, Some(0.5)),
        ],
    );
    settle();
    assert_eq!(
        said,
        pass::no_answer(),
        "the second round failed too, so the pass gives up with its own line -- got {said:?}"
    );
    assert_eq!(
        said, "!!! effects: no usable answer -- the cut stands without them",
        "ASCII -- and all, matching the spec's bytes"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "not even the good clip 1 zoom survived the rejected reply"
    );
    // The retry budget is F3.9's: one rejection owes one retry, a third ask never happens.
    assert!(
        pass::rejected("x").starts_with(">>> effects rejected:"),
        "the between-rounds line exists"
    );
    assert!(
        !naivepost::cut_captions::retries(2),
        "two rounds is the whole budget -- no third is owed"
    );
    RAN_NO_ANSWER.store(true, Ordering::SeqCst);
    eprintln!("DECO block 7 (no answer) done");
    window.close();
    settle();
}

#[test]
fn f3_11_widget_the_decorations_pass_door_places_what_it_is_answered() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-deco-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(decorations_round);
        app.run_with_args::<&str>(&[]);
    });

    assert!(
        RAN_DOOR_JOBS.load(Ordering::SeqCst),
        "the post-cut job block never ran"
    );
    assert!(
        RAN_CLICK.load(Ordering::SeqCst),
        "the run-through-the-door check never ran"
    );
    assert!(
        RAN_PLACED.load(Ordering::SeqCst),
        "the scripted-reply placement never ran"
    );
    assert!(
        RAN_LANE.load(Ordering::SeqCst),
        "the effects-lane block never ran"
    );
    assert!(
        RAN_UNDO.load(Ordering::SeqCst),
        "the undo of the pass never ran"
    );
    assert!(RAN_GATE.load(Ordering::SeqCst), "the gate never ran");
    assert!(
        RAN_NO_ANSWER.load(Ordering::SeqCst),
        "the two-round give-up never ran"
    );
}
