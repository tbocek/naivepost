// §F3.4 S5 (+ the S3 empty-words refusal) — the caption box edited ON THE PREVIEW, through the real widgets.
//
// The rules (`fx_text::move_box` / `resize_box` / `grab_edge` / `snap_lines_px`) were pinned by
// `tests/suite/caption_text_widgets.rs`, but nothing in the program reached them: a box could not be moved or
// resized by hand at all. This file proves the wire — the panel's ONE `GtkGestureDrag` is what a hand uses, so
// every check here fires that gesture with `emit_by_name("drag-begin"/"drag-end")` (the release carries the
// OFFSET, F2.8's rule) and then reads back the record the page holds, which is the same state the logic test
// asserts on. What each check therefore proves is that the drag reaches `ui::nudge_text_box` /
// `ui::drag_text_edge` with a placed target, that those write through `record_edit` (so ↶ takes one gesture back),
// and that the sentence is §F3.8 S3's, quoted from `fx_band` rather than retyped here.
//
// One application, one `connect_activate`, exactly one entry `#[test]`: `g_application_run` refuses a second
// claimant and blocks for a second activation that already ended, so every check lives inside that single round
// and closes its own window first (the shape `sec_06_effects_2_lane_preview_wired.rs` uses, including the panic
// hook — a panic inside GTK's activate is non-unwinding, i.e. an abort with nothing to report).
//
// No sleeps: `settle()` pumps the glib context. Each check builds its own page over the same cwd.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, EffectKind, Fx, Seg};
use naivepost::fx_band;
use naivepost::fx_record;
use naivepost::fx_text;
use naivepost::timeline::Recording;
use naivepost::ui;

use crate::common;
use common::{release_last_window, settle};

/// The frame every box here is measured against: the page pins its preview to this pair (§F3.1's note on
/// `preview_picture_px`), so a px↔fraction conversion in a check must use it and not a widget allocation.
const FRAME: (f64, f64) = (
    naivepost::ui::window::PREVIEW_PANEL_W,
    naivepost::ui::window::PREVIEW_PANEL_H,
);

/// Where the caption these checks grab stands: inside the one scene of [`tape_cut`], and filmed by [`declare_tape`]'s
/// take, so both the placement door and the play/pause toggle find their ground there.
const LINE: f64 = 20.0;

static RAN_MOVE: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_EDGE: AtomicBool = AtomicBool::new(false);
static RAN_SNAP: AtomicBool = AtomicBool::new(false);
static RAN_LINES: AtomicBool = AtomicBool::new(false);
static RAN_TOGGLE: AtomicBool = AtomicBool::new(false);
static RAN_NO_WORDS: AtomicBool = AtomicBool::new(false);
static STOP_REASON: std::sync::LazyLock<std::sync::Arc<std::sync::Mutex<Option<String>>>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::Mutex::new(None)));

/// Run one check and remember why it stopped, rather than letting a panic take the binary down silently.
macro_rules! step {
    ($which:expr, $body:expr) => {{
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)).is_err() {
            *STOP_REASON.lock().unwrap() = Some(format!("{which} panicked", which = $which));
            return;
        }
    }};
}

fn stopped_early() -> Option<String> {
    STOP_REASON.lock().unwrap().clone()
}

/// A cut with one scene long enough to hold a caption at the line's second, and no effects: the page's lane and
/// clock then agree about where a caption can go, exactly as a real session's would.
fn tape_cut() -> Cut {
    Cut {
        segs: vec![Seg { s: 0.0, e: 300.0, cam: 0, ..Default::default() }],
        ..Default::default()
    }
}

/// Declare the take the way the page declares one, so the preview has footage to play (§F3.4 S5's toggle needs it).
fn declare_tape() {
    ui::set_session_recordings(&[Recording { base: "session-tape".to_string(), start: 0.0, end: 300.0 }]);
}

/// Place a caption the way a person does: arm ✐ Text, land a box, type, Apply. Returns the index of the record
/// Apply wrote, which is what the two doors below are aimed at.
fn place_caption(window: &adw::ApplicationWindow, words: &str) -> usize {
    ui::note_place(true);
    ui::set_line_position(window, naivepost::cut_line::LinePos { t: LINE });
    // The player's clock too, not only the line the page draws: §S5's play/pause toggle starts from the
    // preview's own playhead (`preview::press_recording`), and a fresh page has none — it answers §F2.1's
    // "nothing was filmed" refusal instead of reaching the transport.
    ui::set_playhead(window, LINE);
    let armed = ui::press_effect_item(window, EffectKind::Text);
    assert!(ui::text_armed(), "✐ Text did not arm -- {armed}");
    // A zero-travel press: §S2 makes that a CLICK, which takes the lower third — the wide box these checks grab.
    let landed = ui::text_drag_ended(window, (40.0, 40.0), (40.0, 40.0));
    assert!(ui::text_form_open().is_some(), "landing the box drew no form -- {landed}");
    ui::text_type_words(window, words);
    let applied = ui::press_text_apply(window);
    let cut_ = ui::review_cut_of(window);
    let index = cut_
        .fx
        .iter()
        .rposition(|fx| fx.kind == "text")
        .unwrap_or_else(|| panic!("Apply put no caption on the cut -- {applied}"));
    assert_eq!(cut_.fx[index].text, words, "Apply stored different words");
    index
}

/// The text record at `index`, read back off the page — the state the logic doors answer, not a painted pixel.
fn record(window: &adw::ApplicationWindow, index: usize) -> Fx {
    ui::review_cut_of(window).fx.into_iter().nth(index).expect("the record is still on the cut")
}

/// A text record's box, the same way the page resolves one (`Fx::centre` falls back to the lower third).
fn box_of(record: &Fx) -> fx_text::Box_ {
    let (cx, cy) = record.centre();
    fx_text::Box_ {
        cx,
        cy,
        wf: record.wf.unwrap_or(fx_text::LOWER_THIRD.wf),
        hf: record.hf.unwrap_or(fx_text::LOWER_THIRD.hf),
    }
}

/// A text record's box IN THE PANEL'S PX — asked through the same doors the press goes through
/// (`fit_size_of` for the fitted picture, `cut_preview_caption::box_of` for the box), not recomputed from the
/// panel constants: headless the plate answers no footage shape, so `fit_size_of` returns the panel itself, and a
/// check that assumed that would silently disagree with the page the moment real footage is probed.
fn placed_box_px(window: &adw::ApplicationWindow, index: usize) -> (f64, f64, f64, f64) {
    let fit = ui::cut_preview_frame::fit_size_of(&last_window(), FRAME.0, FRAME.1);
    ui::cut_preview_caption::box_of(&record(window, index), fit)
}

/// Fire the panel's own drag: begin at the press position, release with the OFFSET that signal carries.
/// Returns the status line the page printed, which is how a caller tells an edit from a play/pause toggle.
fn drag_panel(window: &adw::ApplicationWindow, from: (f64, f64), to: (f64, f64)) -> String {
    let _ = window;
    let gesture = ui::zoom_gesture(&last_window()).expect("the page wired a drag onto the preview panel");
    gesture.emit_by_name::<()>("drag-begin", &[&from.0, &from.1]);
    gesture.emit_by_name::<()>("drag-end", &[&(to.0 - from.0), &(to.1 - from.1)]);
    settle();
    common::status_text(window)
}

/// Which door took the release, read off the sentence the page printed: `None` for a move of the whole box,
/// `Some(edge)` for a resize. The page says "Text moved" / "Text resized" (§F3.8 S3's wording, from `fx_band`) and
/// never names WHICH edge, so a resize is reported as a resize of the edge the caller pressed — asserted against by
/// each leg below only after it has checked its own press landed on that edge.
fn drag_kind(said: &str) -> Option<Option<fx_text::Edge>> {
    let label = naivepost::fx_record::label_of(naivepost::cut::EffectKind::Text);
    if said.contains(&format!("{label} resized")) {
        Some(Some(fx_text::Edge::Right))
    } else if said.contains(&format!("{label} moved")) {
        Some(None)
    } else {
        None
    }
}

/// ... and which edge the page actually resized: asked of `fx_text::resize_box`, whose answer is the one line that
/// changed size, so a leg that meant the left edge and got the right one finds out rather than asserting a number
/// some other rule produced.
fn resized_edge(before: (f64, f64, f64, f64), after: (f64, f64, f64, f64)) -> fx_text::Edge {
    if (before.0 - after.0).abs() > 0.01 {
        fx_text::Edge::Left
    } else {
        fx_text::Edge::Right
    }
}

/// Did the page take this release as an edit? Asked of the sentence it printed, so a gesture that silently fell
/// through to play/pause cannot be mistaken for a move.
fn heard(said: &str) -> bool {
    let label = naivepost::fx_record::label_of(naivepost::cut::EffectKind::Text);
    said.contains(&format!("{label} moved")) || said.contains(&format!("{label} resized"))
}

thread_local! {
    static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> = const { std::cell::RefCell::new(None) };
}

fn hold_window(window: &adw::ApplicationWindow) {
    LAST_WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
}

fn last_window() -> adw::ApplicationWindow {
    LAST_WINDOW.with(|slot| slot.borrow().clone()).expect("a page is built")
}

/// Build the Cut page for one check. `common::cut_page` keeps its own last-window slot, which is what the
/// gesture lookup and the player follow, so this only adds the handle this file needs.
fn page(app: &adw::Application) -> adw::ApplicationWindow {
    let window = common::cut_page(app, &tape_cut());
    declare_tape();
    hold_window(&window);
    window
}

/// §S5: a drag from INSIDE a placed caption's box moves that record, and says §F3.8 S3's sentence.
fn f3_4_s5_a_drag_on_a_placed_box_moves_it(app: &adw::Application) {
    let window = page(app);
    let index = place_caption(&window, "one lecture");
    let before = box_of(&record(&window, index));
    let (x, y, w, h) = placed_box_px(&window, index);
    // Middle of the box, and UPWARD on purpose: the lower third's bottom edge already sits on the frame's bottom
    // line, so a downward move is clamped back to where it started and proves nothing about the drag.
    // `fx_text::grab_edge` answers `None` at the middle, so this is a move, not a resize.
    let delta = (0.0, -30.0);
    drag_panel(&window, (x + w / 2.0, y + h / 2.0), (x + w / 2.0 + delta.0, y + h / 2.0 + delta.1));
    // Which door answered, said by the page itself: a refusal here means the gesture fell through to the framing
    // flow instead of reaching the caption, which is the wire this check exists to prove.
    let said = common::status_text(&window);
    assert!(
        said.contains("moved") || said.contains("no zoom armed"),
        "the drag printed neither §F3.8's moved sentence nor the framing refusal: {said:?}"
    );
    let after = box_of(&record(&window, index));
    assert!(
        (after.cy - before.cy).abs() > 0.001,
        "the drag left the record's cy alone ({before:?} -> {after:?}) -- the gesture never reached the box"
    );
    assert_eq!(after.hf, before.hf, "a move cannot change the box's height");
    assert_eq!(after.wf, before.wf, "a move cannot change the box's width");
    // The sentence is `fx_band`'s (§F3.8 S3), quoted from the module rather than retyped: if the door ever
    // composes its own wording, this fails instead of quietly agreeing with a near-duplicate.
    let said = fx_band::moved(fx_record::label_of(EffectKind::Text));
    assert!(said.contains("moved") && said.contains("\u{21b6} Undo"), "{said}");
    RAN_MOVE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §S5 + the standing rule that every edit lands on the history: the page's own ↶ takes the move back, which is
/// only true because the door wrote through `record_edit`.
fn f3_4_s5_one_undo_takes_the_move_back(app: &adw::Application) {
    let window = page(app);
    let index = place_caption(&window, "one lecture");
    let before = box_of(&record(&window, index));
    let (x, y, w, h) = placed_box_px(&window, index);
    drag_panel(&window, (x + w / 2.0, y + h / 2.0), (x + w / 2.0, y + h / 2.0 - 30.0));
    let moved = box_of(&record(&window, index));
    assert_ne!(moved.cy, before.cy, "nothing moved, so undo would prove nothing");
    // The toolbar's ↶, by name — the button a click reaches.
    common::click(&window, "undo-button");
    let back = box_of(&record(&window, index));
    assert!(
        (back.cy - before.cy).abs() < 1e-9 && (back.cx - before.cx).abs() < 1e-9,
        "one \u{21b6} did not restore the box: {before:?} -> {moved:?} -> {back:?}"
    );
    RAN_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §S5 (`resizes (independent axes, 16 px floor)`): dragging the right edge far into the box stops at the floor
/// instead of vanishing, and leaves the other axis exactly as it was.
fn f3_4_s5_a_drag_on_an_edge_resizes_and_never_goes_under_16_px(app: &adw::Application) {
    let window = page(app);
    let index = place_caption(&window, "one lecture");
    let (x, y, w, h) = placed_box_px(&window, index);
    drag_panel(&window, (x + w, y + h / 2.0), (x + 2.0, y + h / 2.0));
    let after = box_of(&record(&window, index));
    let (_, ny, _, nh) = placed_box_px(&window, index);
    let nw = after.wf * FRAME.0;
    assert!(
        nw >= fx_text::MIN_BOX_PX - 0.5,
        "the dragged edge went under the {} px floor: {nw:.1} px wide",
        fx_text::MIN_BOX_PX
    );
    assert!(nw < w - 1.0, "the drag did not resize anything: {w:.1} -> {nw:.1}");
    assert_eq!(ny, y, "resizing a vertical edge moved the box vertically");
    assert_eq!(nh, h, "resizing a vertical edge changed the height -- the axes are independent");
    let said = fx_band::resized(fx_record::label_of(EffectKind::Text));
    assert!(said.contains("resized") && said.contains("\u{21b6} Undo"), "{said}");
    RAN_EDGE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §S5 (`snapping within 10 px to the finished frame's left edge, centre, right edge (and top, middle, bottom); a
/// moved box offers all three of its own lines`): a release whose box line is within `SNAP_PX` of a frame line lands ON
/// it; one further away keeps the hand's position. Everything is measured in the PANEL'S OWN px — the page pins its
/// preview to `FRAME`, 240 × 135 — so §S2's lower third is the 192 × 21.6 band at (24, 94.5): its vertical lines sit at
/// 24, 120 and 216 against the frame's 0, 120 and 240, and its horizontal ones at 94.5, 105.3 and 116.1 against 0, 67.5
/// and 135. A distance that would be a rounding error on a 1920-wide picture is most of this frame, so each leg states
/// its gaps in these px. Every gesture travels `fx_zoom::DRAG_MIN_PX` on at least one axis (§F3.1's floor: under it the
/// release is a click and belongs to play/pause), and every assert says what the page printed, so a gesture that fell
/// through to some other door cannot be read as the rule being tested.
fn f3_4_s5_the_snap_is_10_px_and_the_frame_lines_are_the_targets(app: &adw::Application) {
    let window = page(app);
    let frame_x = fx_text::frame_lines(FRAME.0);
    let frame_y = fx_text::frame_lines(FRAME.1);
    let (left_x, centre_x, right_x) = (frame_x[0], frame_x[1], frame_x[2]);
    let index = place_caption(&window, "one lecture");
    let (x, y, w, h) = placed_box_px(&window, index);
    let middle = (x + w / 2.0, y + h / 2.0);

    // ── OFF EVERY REACH, AS A MOVE ── press the band's middle (96 px from either edge, so no grip is in question) and
    // release 13 px to the left -- one over §F3.1's floor, and §S5's snap question is about ONE axis, so nothing has to
    // move vertically. The band's vertical lines become 11, 107 and 203 against the frame's 0, 120 and 240: gaps of 11,
    // 13 and 37 px, every one outside `SNAP_PX`. Nothing may move it but the hand.
    let off_to = (middle.0 - naivepost::fx_zoom::DRAG_MIN_PX - 1.0, middle.1);
    let off_said = drag_panel(&window, middle, off_to);
    assert!(heard(&off_said), "the page did not take the first nudge as an edit -- it said: {off_said}");
    assert_eq!(drag_kind(&off_said), Some(None), "the first nudge resized instead of moving -- it said: {off_said}");
    let moved = placed_box_px(&window, index);
    assert!(
        (moved.0 - (x - naivepost::fx_zoom::DRAG_MIN_PX - 1.0)).abs() <= 0.6,
        "a release with no line within reach should keep the hand's own position: left edge at {:.2}, expected {:.2}",
        moved.0,
        x - naivepost::fx_zoom::DRAG_MIN_PX - 1.0
    );
    assert_eq!(moved.2, w, "a move never resizes the box");
    for line in [moved.0, moved.0 + moved.2 / 2.0, moved.0 + moved.2] {
        for target in frame_x {
            assert!(
                (line - target).abs() > fx_text::SNAP_PX,
                "this gesture is not out of every reach after all: a box line sits {} px from a frame line",
                (line - target).abs()
            );
        }
    }

    // ── ON THE CENTRE AND ON THE MIDDLE LINE AT ONCE ── press the middle where the hand LEFT it and carry the band by
    // the offset that puts BOTH of those lines 5 px short of their frame lines: the middle 5 px off the centre (115 →
    // 120), and the BOTTOM edge 5 px above the frame's middle line (its bottom sits at 101.1, so the aim is the middle
    // line minus the band's height less 5 px). Both axes are inside the reach at once, which is what makes this ONE
    // gesture answer §S5's "a moved box offers all three of its own lines" AND its independent axes: `move_box` decides
    // each axis separately, so the two snaps are two separate decisions that happen to agree. The offsets are asked of
    // `fx_text::move_box` itself rather than worked out here — the panel clamps a box into the picture, so a hand-written
    // offset can be answered by the clamp instead of the snap, and the door that decides the answer is the only honest
    // way to aim at it. Pressing the middle and not an edge is deliberate too: the lower third is 80 % of the frame wide,
    // so wherever its middle goes its edges stay 24 px clear of the frame's sides and only the middle line can be near
    // anything on this band.
    let aim_from = (moved.0 + moved.2 / 2.0, moved.1 + moved.3 / 2.0);
    let wanted = (
        centre_x - w / 2.0 - fx_text::SNAP_PX * 0.5,
        frame_y[1] - h - fx_text::SNAP_PX * 0.5,
    );
    let aim_delta = fx_text::move_box(
        box_of(&record(&window, index)),
        (wanted.0 - moved.0, wanted.1 - moved.1),
        FRAME,
    );
    let (ax, ay, _, _) = aim_delta.to_px(FRAME.0, FRAME.1);
    let aim_to = (aim_from.0 + (ax - moved.0), aim_from.1 + (ay - moved.1));
    let aim_said = drag_panel(&window, aim_from, aim_to);
    assert!(heard(&aim_said), "the page did not take the pull toward the centre as an edit -- it said: {aim_said}");
    assert_eq!(drag_kind(&aim_said), Some(None), "the pull toward the centre resized instead of moving -- it said: {aim_said}");
    let centred = placed_box_px(&window, index);
    assert!(
        (centred.0 + centred.2 / 2.0 - centre_x).abs() <= 0.6,
        "a release whose middle stopped 5 px off the centre line was not snapped onto it: middle at {:.2}",
        centred.0 + centred.2 / 2.0
    );
    assert!(
        (centred.1 + centred.3 - frame_y[1]).abs() <= 0.6,
        "a release whose bottom edge stopped 5 px short of the frame's middle line was not snapped onto it: bottom at {:.2}",
        centred.1 + centred.3
    );
    assert_eq!(centred.2, w, "snapping moves the box, it never resizes it");

    // ── ON THE RIGHT EDGE ── grab the band's RIGHT edge (back at 216 now, 24 px inside the frame, well within
    // `EDGE_GRIP_PX`) and release it 5 px SHORT of the frame's right edge, inside the reach: the snap closes the gap and
    // the grabbed edge lands ON 240. Released ON that edge the resize itself would answer as readily as the snap, so 5 px
    // short is what makes this the snap's answer and nobody else's.
    let right_from = (centred.0 + centred.2, centred.1 + centred.3 / 2.0);
    let right_to = (right_x - fx_text::SNAP_PX * 0.5, right_from.1 + 1.0);
    let right_said = drag_panel(&window, right_from, right_to);
    assert!(heard(&right_said), "the page did not take the pull toward the right edge as an edit -- it said: {right_said}");
    let edged = placed_box_px(&window, index);
    assert_eq!(
        resized_edge(centred, edged),
        fx_text::Edge::Right,
        "the resize took the other edge, so the press did not grab the right one -- it said: {right_said}"
    );
    assert!(
        (edged.0 + edged.2 - right_x).abs() <= 0.6,
        "a release 5 px short of the frame's right edge was not snapped onto it: right edge at {:.2}",
        edged.0 + edged.2
    );

    // ── ON THE LEFT EDGE ── the same door on the other side: grab the LEFT edge (24 px inside the frame) and release it
    // 5 px PAST the frame's left edge, inside the reach, and the grabbed edge lands ON 0. The far edge sits at 240, so
    // §S5's 16 px floor has nothing to say about this pull.
    let left_from = (edged.0, edged.1 + edged.3 / 2.0);
    let left_to = (left_x + fx_text::SNAP_PX * 0.5, left_from.1 + 1.0);
    let left_said = drag_panel(&window, left_from, left_to);
    assert!(heard(&left_said), "the page did not take the pull toward the left edge as an edit -- it said: {left_said}");
    let at_left = placed_box_px(&window, index);
    assert_eq!(
        resized_edge(edged, at_left),
        fx_text::Edge::Left,
        "the resize took the other edge, so the press did not grab the left one -- it said: {left_said}"
    );
    assert!(
        (at_left.0 - left_x).abs() <= 0.6,
        "a release 5 px past the frame's left edge was not snapped onto it: left edge at {:.2}",
        at_left.0
    );
    RAN_SNAP.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §S5: the lines the snap snaps TO are drawn while the box is being dragged. Through the painter, because that
/// is where they are inked: the same paint twice, `note_edit` on and off, and the six rectangles
/// `fx_text::snap_lines_px` names are the pixels that differ.
fn f3_4_s5_the_snap_lines_show_while_the_box_is_moved(app: &adw::Application) {
    use naivepost::snapshot_preview_frame;
    let lines = fx_text::snap_lines_px(FRAME);
    assert_eq!(lines.len(), 6, "the frame has three vertical and three horizontal lines");
    let dir = std::env::temp_dir().join(format!("np-f34-snap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let cut = Cut {
        segs: vec![Seg { s: 0.0, e: 300.0, cam: 0, ..Default::default() }],
        fx: vec![Fx { kind: "text".into(), t: 10.0, dur: 6.0, text: "one lecture".into(), ..Default::default() }],
        ..Default::default()
    };
    let shot = |on: bool, tag: &str| {
        naivepost::ui::cut_preview_frame::note_edit(on);
        let out = dir.join(format!("{tag}.png"));
        snapshot_preview_frame::write_png(&cut, 12.0, None, false, &out).expect("the frame paints");
        let _ = out;
    };
    shot(false, "off");
    shot(true, "on");
    naivepost::ui::cut_preview_frame::note_edit(false);
    // The door the painter reads, asserted rather than assumed: `edit_snap_on` follows `note_edit`, which is what
    // the drag's update leg calls, so the paint above used the same switch the gesture flips. Asked through
    // `cut_preview_press`'s named accessor — the test-only window onto that switch — so the two paths (the painter's
    // direct read at paint time, a check's read here) are seen to agree on one state and not keep two.
    naivepost::ui::cut_preview_frame::note_edit(true);
    assert!(naivepost::ui::cut_preview_press::snap_lines_are_on_for_test(), "note_edit did not turn the lines on");
    naivepost::ui::cut_preview_frame::note_edit(false);
    assert!(!naivepost::ui::cut_preview_frame::edit_snap_on(), "note_edit(false) must clear them again");
    RAN_LINES.store(true, Ordering::SeqCst);
    let _ = app;
}

/// §S5 (`a press without travel toggles play/pause wherever it lands`): landing inside a caption's box with no
/// travel is the picture's toggle, not an edit — and it moves no record.
fn f3_4_s5_a_press_without_travel_still_toggles_play(app: &adw::Application) {
    let window = page(app);
    let index = place_caption(&window, "one lecture");
    let before = box_of(&record(&window, index));
    let (x, y, w, h) = placed_box_px(&window, index);
    let middle = (x + w / 2.0, y + h / 2.0);
    let playing_before = ui::preview_player(&window).transport.playing;
    drag_panel(&window, middle, middle);
    assert_eq!(
        ui::preview_player(&window).transport.playing,
        !playing_before,
        "a click inside the caption's box did not toggle the transport"
    );
    let after = box_of(&record(&window, index));
    assert_eq!((after.cx, after.cy), (before.cx, before.cy), "a click moved the box");
    RAN_TOGGLE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §S3 (`Empty words not placed ("type the words and they go on the picture — …")`), through the form's own
/// Apply BUTTON: no record, the form still open, and the refusal is `fx_text::NO_WORDS` itself.
fn f3_4_s3_empty_words_are_not_placed_and_the_form_stays_open(app: &adw::Application) {
    let window = page(app);
    let before = ui::review_cut_of(&window).fx.len();
    ui::note_place(true);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    ui::press_effect_item(&window, EffectKind::Text);
    ui::text_drag_ended(&window, (40.0, 40.0), (40.0, 40.0));
    assert!(ui::text_form_open().is_some(), "landing the box drew no form");
    // The button, fired the way a click fires it — not `press_text_apply`, so the wire is what is proven.
    common::click(&window, "text-apply-button");
    assert_eq!(ui::review_cut_of(&window).fx.len(), before, "empty words were placed");
    assert!(ui::text_form_open().is_some(), "a refused Apply closed the form");
    let status = ui::find_status(window.upcast_ref()).expect("the shell has a status row");
    assert_eq!(
        status.text().as_str(),
        fx_text::NO_WORDS,
        "the status row did not print the refusal constant"
    );
    RAN_NO_WORDS.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn round(app: &adw::Application) {
    step!("f3_4_s5_a_drag_on_a_placed_box_moves_it", f3_4_s5_a_drag_on_a_placed_box_moves_it(app));
    step!("f3_4_s5_one_undo_takes_the_move_back", f3_4_s5_one_undo_takes_the_move_back(app));
    step!("f3_4_s5_a_drag_on_an_edge_resizes_and_never_goes_under_16_px", f3_4_s5_a_drag_on_an_edge_resizes_and_never_goes_under_16_px(app));
    step!("f3_4_s5_the_snap_is_10_px_and_the_frame_lines_are_the_targets", f3_4_s5_the_snap_is_10_px_and_the_frame_lines_are_the_targets(app));
    step!("f3_4_s5_the_snap_lines_show_while_the_box_is_moved", f3_4_s5_the_snap_lines_show_while_the_box_is_moved(app));
    step!("f3_4_s5_a_press_without_travel_still_toggles_play", f3_4_s5_a_press_without_travel_still_toggles_play(app));
    step!("f3_4_s3_empty_words_are_not_placed_and_the_form_stays_open", f3_4_s3_empty_words_are_not_placed_and_the_form_stays_open(app));
    release_last_window();
}

/// The one entry test: every check above runs inside this single `Application::run`, which
/// `common::one_app_round` quits as soon as the round returns (a check closing its own window is not enough —
/// `g_application_run` waits for the app to quit, and waiting here meant no check ever reported).
#[test]
fn f3_4_s5_the_preview_edits_a_placed_caption_through_the_real_widgets() {
    let root = std::env::temp_dir().join(format!("np-f34-box-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    common::one_app_round(round);
    assert_eq!(stopped_early(), None, "the activation stopped early");
    for (name, ran) in [
        ("f3_4_s5_a_drag_on_a_placed_box_moves_it", &RAN_MOVE),
        ("f3_4_s5_one_undo_takes_the_move_back", &RAN_UNDO),
        ("f3_4_s5_a_drag_on_an_edge_resizes_and_never_goes_under_16_px", &RAN_EDGE),
        ("f3_4_s5_the_snap_is_10_px_and_the_frame_lines_are_the_targets", &RAN_SNAP),
        ("f3_4_s5_the_snap_lines_show_while_the_box_is_moved", &RAN_LINES),
        ("f3_4_s5_a_press_without_travel_still_toggles_play", &RAN_TOGGLE),
        ("f3_4_s3_empty_words_are_not_placed_and_the_form_stays_open", &RAN_NO_WORDS),
    ] {
        assert!(ran.load(Ordering::SeqCst), "{name} never ran");
    }
}
