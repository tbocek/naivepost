// F3.4 S4 — the preview side of "fitted: the largest size where ≤ 12 lines fit, floor 7 pt · bold sans, white
// with a dark edge · PREVIEW = RENDER".
//
// `fx_text::fit` is one arithmetic layout precisely so both halves can agree, and §S4 says they must be THE SAME
// FUNCTION. The render half has had that fitter since its own round; the preview painted captions at a fixed 13 px
// in one unwrapped line, which meant the promise had no preview side at all. These checks pin the door the painter
// now goes through: `ui::cut_preview_caption`, whose size and lines ARE `fx_text::fit`'s, whose box is the record's
// own output-frame fractions (LOWER_THIRD when none are stored), and whose edge is the dilation §S4 describes rather
// than a shadow of someone's choosing.
//
// Logic-side on purpose: the drawing itself is looked at through `just snapshot 06-preview`, and the wire — a real
// press on the panel reaching these doors — is `f3_4_s5_a_press_on_a_caption_toggles_play` further down, which fires
// the page's own gesture rather than calling a rule.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;

use naivepost::cut::{Cut, EffectKind, Fx};
use naivepost::fx_text;
use naivepost::ui;

use crate::common;
use common::settle;

static RAN_TOGGLE: AtomicBool = AtomicBool::new(false);
static RAN_CLEAR: AtomicBool = AtomicBool::new(false);

/// The panel's fitted picture size. No ffprobe runs headless, so `fit_size_of` answers the panel itself — which
/// is also what its own `set_size_request` pins it to (`window.rs`: `PREVIEW_PANEL_W`/`PREVIEW_PANEL_H`, spelled
/// out here rather than reached through the private `ui::window` constants).
const FIT: (f64, f64) = (240.0, 135.0);

/// A caption record: words, three seconds, and optionally a box the hand drew.
fn caption(words: &str, box_: Option<(f64, f64, f64, f64)>) -> Fx {
    let mut effect = Fx {
        kind: "text".into(),
        t: 10.0,
        dur: 3.0,
        text: words.to_string(),
        ..Default::default()
    };
    if let Some((cx, cy, wf, hf)) = box_ {
        effect.cx = Some(cx);
        effect.cy = Some(cy);
        effect.wf = Some(wf);
        effect.hf = Some(hf);
    }
    effect
}

/// §F3.4 S4 (`preview = render`): the preview's `(size, lines)` IS `fx_text::fit`'s answer for the same box — not
/// an equal-looking copy of it. Longer words mean smaller type in the same box, and a hopeless box still gets the
/// 7 pt floor rather than vanishing.
#[test]
fn f3_4_s4_the_preview_uses_the_shared_fitter() {
    // A working directory of its own: the app keeps `session.naivepost` there, and test files run side by side.
    common::pin_private_cwd();
    let words = "Ethereum in one lecture, covering gas, wallets and Layer 2 rollups";
    let effect = caption(words, None);
    let (box_px, size, lines) = ui::cut_preview_caption::layout(&effect, FIT);
    let (want_size, want_lines) = fx_text::fit(words, box_px.2, box_px.3);
    assert_eq!(size, want_size, "the preview's size is the shared fitter's, to the last decimal");
    assert_eq!(lines, want_lines, "and so are its lines");
    assert!(size >= fx_text::MIN_POINTS, "never under the floor: {size}");
    assert!(!lines.is_empty(), "a caption with words has lines");
    assert!(
        lines.len() <= fx_text::MAX_LINES,
        "{} lines is over §S4's twelve",
        lines.len()
    );

    // The whole point of fitting: more words in the same box means smaller type, never an overflow.
    let longer = caption(&format!("{words}, and then a second clause that goes on for a good while"), None);
    let (_, long_size, _) = ui::cut_preview_caption::layout(&longer, FIT);
    assert!(long_size < size, "a longer caption shrinks to fit: {long_size} vs {size}");

    // A box too small for the floor: `fit` clamps its search to what the box can hold at all (`lo = MIN_POINTS.min(hi)`)
    // rather than answering 0, so a hopeless box still gets a size and overflows visibly. The panel's own height is
    // generous, so this asks about a box measured in units where 7 pt simply cannot sit.
    let tiny = caption(words, Some((0.5, 0.5, 1.0, 1.0)));
    let (_, tiny_size, _) = ui::cut_preview_caption::layout(&tiny, (6.0, 4.0));
    assert!(tiny_size > 0.0, "a hopeless box still answers a size, never 0");
    assert!(tiny_size <= fx_text::MIN_POINTS, "capped by the box rather than claiming the floor it cannot hold");

    // No words, nowhere to draw: `fit` answers 0 and the painter draws nothing rather than a zero-sized glyph run.
    let (_, empty_size, empty_lines) = ui::cut_preview_caption::layout(&caption("   ", None), FIT);
    assert_eq!(empty_size, 0.0);
    assert!(empty_lines.is_empty());
}

/// §F3.4 S2 + S4: the words sit in the box the RECORD carries, measured against the OUTPUT frame (§06#1 gives a
/// text `cx, cy, wf, hf` as output-frame fractions), and a record with no box gets the lower third.
#[test]
fn f3_4_s4_the_preview_lays_the_words_in_the_stored_box() {
    common::pin_private_cwd();
    let stored = caption("hello", Some((0.4, 0.3, 0.5, 0.2)));
    let (x, y, w, h) = ui::cut_preview_caption::box_of(&stored, FIT);
    assert!((w - 0.5 * FIT.0).abs() < 1e-9, "the stored width is a fraction of the output frame");
    assert!((h - 0.2 * FIT.1).abs() < 1e-9, "and so is its height");
    assert!((x - (0.4 * FIT.0 - w / 2.0)).abs() < 1e-9, "the centre is where the record says");
    assert!((y - (0.3 * FIT.1 - h / 2.0)).abs() < 1e-9, "on both axes");

    // Nothing stored: the lower third, which is §S2's click default and `Fx::centre`'s answer for a text.
    let bare = caption("hello", None);
    let (bx, by, bw, bh) = ui::cut_preview_caption::box_of(&bare, FIT);
    let lower = fx_text::Box_ { cx: fx_text::LOWER_THIRD.cx, cy: fx_text::LOWER_THIRD.cy, wf: fx_text::LOWER_THIRD.wf, hf: fx_text::LOWER_THIRD.hf };
    assert_eq!((bx, by, bw, bh), lower.to_px(FIT.0, FIT.1), "a boxless caption takes LOWER_THIRD");
    assert!(by > FIT.1 / 2.0, "which is low on the frame: y {by} of {}", FIT.1);
}

/// §F3.4 S4 (`dark dilated edge`): the numbers the draw path reads are §10's, so changing a row shows up here
/// instead of only in a screenshot nobody took.
#[test]
fn f3_4_s4_the_edge_is_dilated_not_shadowed() {
    common::pin_private_cwd();
    // A dilation scales with the type; a shadow would be a fixed offset. And it never rounds away to nothing.
    assert_eq!(fx_text::edge_radius(20.0), 20.0 * fx_text::EDGE_RADIUS_EM);
    assert_eq!(fx_text::edge_radius(4.0), 1.0, "under a pixel the edge would vanish between samples");
    // The render strokes across its path, so twice the radius: one constant for both halves' thickness.
    assert_eq!(fx_text::render_stroke_width(20.0), fx_text::edge_radius(20.0) * 2.0);
    assert_eq!(fx_text::EDGE_ALPHA, 0.85, "not fully opaque, or the edge reads as a second set of words");
    assert_eq!(fx_text::EDGE_STEPS, 16, "sixteen directions: eight was faceted on diagonals");
    assert!(fx_text::preview_is_the_render(), "one layout function, so there is nothing to keep in step");
}

// --- the wire: a press on the picture ---------------------------------------------------------------------------

/// The Cut page with ONE caption already on the cut, its seconds covering the placed line, plus the tape the
/// transport needs to have something to play. Built through the page's own doors, exactly as the other wire tests.
fn page_with_a_caption(app: &adw::Application) -> adw::ApplicationWindow {
    let mut seeded = Cut::default();
    seeded.segs.push(naivepost::cut::Seg { s: 0.0, e: 120.0, ..Default::default() });
    seeded.fx.push(caption("Ethereum in one lecture", None));
    // Declared AFTER the window is built: `set_session_recordings` writes into the newest per-window slot, and
    // `build_window` pushes that slot itself — declaring before it exists is what makes `cut_folds.rs` panic with
    // "a session was declared before any window existed". The window's own rescan fills nothing for this fixture,
    // so the list written here is what `filmed_runs` sees, which is what lets ▶ start instead of refusing.
    let window = common::cut_page_settled_first(app, &seeded);
    ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "session-tape".into(),
        start: 0.0,
        end: 300.0,
    }]);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 11.0 });
    settle();
    window
}

// A GTK app needs a display — `gtk_init` aborts the process where there is none, rather than returning an error —
// so this file's widget checks are run under xvfb exactly as `just test` runs them; a bare `cargo test --test <this>`
// in a shell without one dies inside GTK and proves nothing.

/// Fire the panel's OWN button-1 gesture with zero travel: the toolkit's two signals, begin at the point and end
/// with a zero offset, which is what a click is to GTK. Nothing here calls a rule.
fn press_the_panel(window: &adw::ApplicationWindow, at: (f64, f64)) {
    let gesture = ui::zoom_gesture(window).expect("the page wired a drag onto the preview panel");
    gesture.emit_by_name::<()>("drag-begin", &[&at.0, &at.1]);
    gesture.emit_by_name::<()>("drag-end", &[&0.0f64, &0.0f64]);
    settle();
}

/// The middle of the caption's box in panel px — the point §S5 says must not be swallowed by the box.
fn caption_centre(window: &adw::ApplicationWindow) -> (f64, f64) {
    let cut_ = ui::review_cut_of(window);
    let effect = cut_
        .fx
        .iter()
        .find(|fx| fx.effect_kind() == Some(EffectKind::Text))
        .expect("the page holds the caption this check pressed on");
    let (x, y, w, h) = ui::cut_preview_caption::box_of(effect, FIT);
    (x + w / 2.0, y + h / 2.0)
}

/// The page's gesture, fired twice inside the caption's box: play, then pause. Run inside one application; a
/// failing check is recorded rather than allowed to abort the process from inside `connect_activate` (a panic there
/// is a non-unwinding abort, which kills the whole test binary and hides which assertion broke).
fn toggle_check(app: &adw::Application) -> Result<(), String> {
    let window = page_with_a_caption(app);
    if ui::preview_player(&window).transport.playing {
        return Err("a fresh preview is not playing".into());
    }
    press_the_panel(&window, caption_centre(&window));
    if !ui::preview_player(&window).transport.playing {
        return Err(format!(
            "a click inside the caption's box never reached the transport \u{2014} status was {:?}, clock {}",
            common::status_text(&window),
            naivepost::preview::clock(ui::preview_player(&window).playhead)
        ));
    }
    // The tick only runs while a real player process exists (`cut_record_tick`'s own guard), and none does in here,
    // so the line has to be put back on filmed ground by hand before the second click: the first one lands it at
    // second 300, past the end of the tape, where §F2.1's S2 refuses to start a preview rather than pretend.
    ui::set_playhead(&window, 11.0);
    press_the_panel(&window, caption_centre(&window));
    if ui::preview_player(&window).transport.playing {
        return Err("the same click again did not pause it \u{2014} toggle, not restart".into());
    }
    Ok(())
}

/// The same press clear of every overlay box stays the framing question it was: no play, no pause.
fn clear_check(app: &adw::Application) -> Result<(), String> {
    let window = page_with_a_caption(app);
    // Top-left corner: outside the lower third, so nothing here belongs to F3.4 S5.
    press_the_panel(&window, (2.0, 2.0));
    if ui::preview_player(&window).transport.playing {
        return Err("a press clear of every overlay box started the preview \u{2014} it must stay §F3.1's".into());
    }
    Ok(())
}

fn round(app: &adw::Application) {
    for (flag, what, check) in [
        (&RAN_TOGGLE, "the press-on-the-caption check", toggle_check as fn(&adw::Application) -> Result<(), String>),
        (&RAN_CLEAR, "the press-clear-of-a-box check", clear_check as fn(&adw::Application) -> Result<(), String>),
    ] {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(app)));
        match outcome {
            Ok(Ok(())) => flag.store(true, Ordering::SeqCst),
            Ok(Err(reason)) => PANIC.with(|slot| *slot.borrow_mut() = Some(format!("{what}: {reason}"))),
            Err(_) => PANIC.with(|slot| *slot.borrow_mut() = Some(format!("{what}: panicked"))),
        }
    }
}

thread_local! {
    static PANIC: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

#[test]
fn f3_4_s5_a_press_without_travel_on_a_caption_toggles_play() {
    common::pin_private_cwd();
    common::one_app_round(round);
    let trouble = PANIC.with(|slot| slot.borrow().clone());
    if let Some(reason) = trouble {
        panic!("{reason}");
    }
    assert!(RAN_TOGGLE.load(Ordering::SeqCst), "the press-on-the-caption check never ran");
    assert!(RAN_CLEAR.load(Ordering::SeqCst), "the press-clear-of-a-box check never ran");
}
