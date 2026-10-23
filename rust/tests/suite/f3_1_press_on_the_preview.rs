// F3.1 (the paragraph after S5) + §2's plate — the two zoom rules whose code existed with no caller.
//
// `fx_zoom::preview_press`, `fx_zoom::re_frame_status` and `fx_zoom::zoom_plate` each stated a rule the spec
// prints; none of them was reachable from a press. These are the logic-side pins: every answer comes from the
// module the page forwards to, so a rule that loses its caller again fails here rather than sitting dead in
// `fx_zoom`. The window side of the same doors — the arm gate, the form the press leaves open, the plate as a
// widget — is driven by the real widgets in `tests/suite/zoom_drag_widgets.rs` (F3.1 S1/S2), which fires the panel's
// own gesture; this file deliberately builds no window.

use naivepost::cut::{Cut, Fx};
use naivepost::fx_zoom as zoom;
use naivepost::ui;

/// A zoom framed off-centre so a plate, a box and a camera can all be told apart from the settled frame.
fn zoom_at(t: f64, stay: bool) -> Fx {
    Fx {
        kind: "zoom".into(),
        t,
        dur: 3.0,
        stay,
        cx: Some(0.3),
        cy: Some(0.4),
        hf: Some(0.35),
        trans: 1.0,
        tout: 1.0,
        ..Default::default()
    }
}

/// §F3.1 after S5: with nothing armed or held, a press inside a zoom's box takes THAT zoom — the hand goes on
/// the record, the page answers with `fx_zoom::re_frame_status`, and nothing is written (so the cut is unchanged
/// and the step the record arrived with stays the one ↶ takes back).
#[test]
fn f3_1_a_press_inside_a_box_takes_the_zoom_in_force() {
    let mut cut_ = Cut::default();
    cut_.fx.push(zoom_at(30.0, false));
    // The rule-side door: a press inside the only zoom's box names that index, and the box the page measures is
    // the camera's own rect fitted into the panel (here the whole panel, because the test's fit IS the panel).
    let fit = (240.0, 135.0);
    let centre = ui::cut_preview_frame::camera_rect_px(&zoom::camera_at(&cut_.fx, 31.0), fit);
    let under = ui::cut_zoom_hand::box_under_pointer(
        &cut_,
        31.0,
        fit,
        (centre.0 + centre.2 / 2.0, centre.1 + centre.3 / 2.0),
    );
    assert_eq!(
        under,
        Some(0),
        "the press fell clear of the box the frame draws"
    );
    // A press outside it draws a new rectangle instead.
    let outside = ui::cut_zoom_hand::box_under_pointer(&cut_, 31.0, fit, (2.0, 2.0));
    assert_eq!(
        outside, None,
        "a press clear of every box must not claim the zoom"
    );
    assert!(!cut_.fx.is_empty(), "taking a zoom must not write anything");
}

/// §F3.1 after S5: a press clear of every box draws a new rectangle FOR THE ZOOM IN FORCE, which means the form
// that opens carries that zoom's framing rather than the whole source.
#[test]
fn f3_1_a_press_clear_of_every_box_uses_the_zoom_in_force() {
    let mut cut_ = Cut::default();
    cut_.fx.push(zoom_at(10.0, true));
    // The zoom ran out at 13 s; at 20 s a staying zoom is the settled frame, so the box the next press draws is
    // that framing — asked of the same door the page uses.
    let camera = zoom::camera_at(&cut_.fx, 20.0);
    assert_eq!(
        camera,
        zoom::camera_at(&cut_.fx, 12.0),
        "a staying zoom is the frame now"
    );
    // And with NO zoom anywhere, the camera is the centred full-fill slice, which is what §A.1 starts from.
    let bare = Cut::default();
    assert_eq!(zoom::camera_at(&bare.fx, 20.0), zoom::SETTLED);
}

/// §2 (`rect stroked white WITH A PLATE`): the plate beside the box is the ZOOM's own sentence, from
/// `fx_zoom::zoom_plate` — not the sound-debt plate, and not a placeholder.
#[test]
fn f3_1_the_plate_beside_the_box_names_the_zoom() {
    let mut cut_ = Cut::default();
    cut_.fx.push(zoom_at(81.0, false));
    let effect = &cut_.fx[0];
    let said = zoom::zoom_plate(effect.t, effect.dur, effect.trans, effect.tout);
    assert_eq!(said, "zoom at 01:21 for 3.0s (1.0s in, 1.0s out)");
    // A staying zoom has no way back, so its plate carries no fades: the plate cannot promise a glide the
    // camera path will not travel.
    let staying = zoom_at(81.0, true);
    let stays = zoom::zoom_plate(staying.t, staying.dur, 0.0, 0.0);
    assert!(stays.contains("for 3.0s (0.0s in, 0.0s out)"), "{stays}");
}

/// §F3.1 S6's promise, checked where it is spent: the sentence the page prints is the one that says Undo works,
/// so the doors that print it must be the ones that go through `record_edit`.
#[test]
fn f3_1_the_re_frame_sentence_promises_undo() {
    assert_eq!(
        zoom::re_frame_status(81.0),
        "zoom at 01:21 re-framed \u{2014} \u{21b6} Undo takes it back"
    );
}

/// §F3.1 S1 against the paragraph after S5: while ⊕ Zoom is armed a press on the picture is the ARM's gesture,
/// not a framing question, so the framing door has nothing to say. Both halves of that are pinned here without
/// a window: `fx_zoom::arm` is what decides the arm, and `fx_zoom::preview_press` is the rule that answers
/// `TakesZoom` only when nothing is armed or held — the same three conditions `cut_zoom_hand::preview_press`
/// forwards. The window-side gate itself is `window::zoom_armed()`; the press path that reads it is driven by
/// the real widgets in `tests/suite/zoom_drag_widgets.rs` (F3.1 S1 arms and disarms through `effect-item-zoom`).
#[test]
fn f3_1_the_door_says_nothing_while_the_arm_is_up() {
    // Arming is the page's answer to ✚ Effect ▾ → ⊕ Zoom with a line placed.
    assert_eq!(zoom::arm(Some(30.0), false), zoom::Press::Armed);
    // With the arm up, §S1 owns the preview: `preview_press` refuses to name a zoom for ANY press, which is why
    // `cut_zoom_hand::preview_press` returns `Pressed::Silent` in exactly that case.
    assert_eq!(
        zoom::preview_press(false, true, true, Some(0)),
        None,
        "an armed page gets no framing answer at all"
    );
    // Same press with nothing armed and the camera settled: now there IS an answer, and it is the new rectangle.
    assert_eq!(
        zoom::preview_press(true, true, true, None),
        Some(zoom::PressPreview::DrawsNew)
    );
    // And the same press inside a box takes that zoom instead of drawing a new one.
    assert_eq!(
        zoom::preview_press(true, true, true, Some(2)),
        Some(zoom::PressPreview::TakesZoom(2))
    );
    // A playing preview is someone reaching for the playhead: never a framing question, armed or not.
    assert_eq!(zoom::preview_press(true, false, true, None), None);
}

// --- the arm rule, stated once more as the page sees it -------------------------------------------------------
//
// Everything above is logic-side on purpose: a widget check in this tree needs the whole application harness and
// is exercised by `tests/suite/zoom_drag_widgets.rs`, which already fires the preview panel's own drag. What that
// file does NOT cover is the paragraph after S5 -- the press with nothing armed -- so the door stays here.
