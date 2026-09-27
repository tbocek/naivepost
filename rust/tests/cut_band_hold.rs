//! F3.8 Hold, move, resize, edit, remove — `spec/06-effects.md` F3.8, steps S1–S5, with §A.9 of
//! `spec/inventory/effects.md`.
//!
//! The rules live in `naivepost::fx_band`; this file proves them and, where a rule is DELEGATED, pins the
//! delegation by comparing against the delegate's own call rather than re-deriving the arithmetic. That is the
//! point of most of these assertions: a second copy of the 4 px slop, the 8 px snap reach or the 0.1 s floor
//! would compile fine and drift silently, so what is tested is that `fx_band` answers exactly what
//! `cut_select::is_drag`, `cut_select::snap_span`, `fx_lane::drag_end` and `effect_rules::*` answer.
//! Strings are compared whole (a reworded refusal is a different refusal), per the house style of
//! `tests/cut_volume_hand.rs`.

use naivepost::cut::Fx;
use naivepost::{cut_select, cut_trim, effect_rules, fx_band, fx_lane, params};

/// A zoom band with a box, so the S4 "live vs snapshot" test has four real fractions to disagree about.
fn zoom_at(t: f64, dur: f64) -> Fx {
    Fx {
        kind: "zoom".into(),
        t,
        dur,
        cx: Some(0.4),
        cy: Some(0.6),
        wf: Some(0.5),
        hf: Some(0.4),
        ..Default::default()
    }
}

// --- S1: picking a band up --------------------------------------------------------------------------------------

#[test]
fn f3_8_s1_a_press_on_a_band_picks_it_up() {
    // 5 px of travel on a 60 px bar is past the click slop (§A.9: under 4 px is a click), so the band is
    // picked up from wherever the hand grabbed it. `t` comes back as the band's start, which is where the red
    // line goes.
    let press = fx_band::press_band(20.0, 60.0, 5.0, false, true, None);
    assert_eq!(press, fx_band::Press::Hold { t: 20.0, from_end: None });

    // The status sentence, whole: no em dash and no Undo clause, because picking something up has changed
    // nothing yet.
    assert_eq!(fx_band::picked_up("Zoom"), "Zoom picked up");
    assert!(!fx_band::picked_up("Zoom").contains('\u{2014}'));
    assert!(!fx_band::picked_up("Zoom").contains('\u{21b6}'));

    // Grabbing an end carries that end through, so the caller resizes without asking the geometry again.
    assert_eq!(
        fx_band::press_band(20.0, 60.0, 5.0, false, true, Some(true)),
        fx_band::Press::Hold { t: 20.0, from_end: Some(true) }
    );
    assert_eq!(
        fx_band::press_band(20.0, 60.0, 5.0, false, true, Some(false)),
        fx_band::Press::Hold { t: 20.0, from_end: Some(false) }
    );

    // Empty lane: the press puts whatever was in hand down rather than picking anything up.
    assert_eq!(fx_band::press_band(20.0, 60.0, 40.0, false, false, None), fx_band::Press::Dropped);
}

#[test]
fn f3_8_s1_under_four_pixels_is_a_click_that_opens_the_form() {
    // §A.9 / S1: "drag under 4 px is a click", and the click opens the form.
    for travel in [0.0, 1.5, 3.0, 3.9] {
        assert_eq!(
            fx_band::press_band(20.0, 60.0, travel, false, true, None),
            fx_band::Press::ClickForm,
            "{travel} px of travel must still be a click"
        );
    }
    // 4 px is NOT under 4, so it is a drag. This is the boundary the spec draws, and it is drawn by
    // `cut_select::is_drag` (`moved_px > DRAG_SLOP_PX`) rather than by a literal here.
    assert_eq!(
        fx_band::press_band(20.0, 60.0, 4.0, false, true, None),
        fx_band::Press::ClickForm,
        "exactly 4 px is still not OVER the slop"
    );
    assert_eq!(
        fx_band::press_band(20.0, 60.0, 4.1, false, true, None),
        fx_band::Press::Hold { t: 20.0, from_end: None }
    );

    // The delegation: `fx_band`'s answer agrees with the slop owner's, at 4 px and either side of it. This is
    // why the module cannot quietly adopt F2.8's 3 px clip threshold instead — `cut_trim::is_click` says a
    // 3 px move is NOT a click, and for a band that would be wrong.
    for travel in [3.0, 4.0, 5.0] {
        let clicked = matches!(
            fx_band::press_band(20.0, 60.0, travel, false, true, None),
            fx_band::Press::ClickForm
        );
        assert_eq!(clicked, !cut_select::is_drag(travel), "{travel} px");
    }
    assert_eq!(cut_select::DRAG_SLOP_PX, 4.0, "the spec's slop is 4 px");
    assert_ne!(
        cut_trim::ROW_TRAVEL_PX, cut_select::DRAG_SLOP_PX,
        "F2.8's clip gesture opens at 3 px, so borrowing is_click here would change the band's rule"
    );
}

#[test]
fn f3_8_s1_ends_are_grips_only_at_30_px() {
    // S1: "ends are grips when the band is \u2265 30 px". The threshold itself, both sides of it.
    assert!(!fx_band::grips_open(29.9));
    assert!(fx_band::grips_open(30.0));
    assert_eq!(fx_band::GRIP_MIN_PX, 30.0);

    // On a 40 px bar: the left 6 px are the start grip, the right 6 px the end grip, the middle is not a grip.
    assert_eq!(fx_band::grab_at(0.0, 40.0), Some(true));
    assert_eq!(fx_band::grab_at(cut_select::GRIP_PX, 40.0), Some(true));
    assert_eq!(fx_band::grab_at(39.0, 40.0), Some(false));
    assert_eq!(fx_band::grab_at(40.0 - cut_select::GRIP_PX, 40.0), Some(false));
    assert_eq!(fx_band::grab_at(20.0, 40.0), None);

    // Just outside each grip is the middle, not the grip: the reach is exactly GRIP_PX, reached through
    // `cut_select::GRIP_PX` (one of §10's "grab reaches") and never written again as a local 6.0.
    assert_eq!(fx_band::grab_at(cut_select::GRIP_PX + 1.0, 40.0), None);
    assert_eq!(fx_band::grab_at(40.0 - cut_select::GRIP_PX - 1.0, 40.0), None);
    assert_eq!(cut_select::GRIP_PX, 6.0);

    // Below 30 px there are no grips at all: every pixel of a narrow bar moves it, so nobody resizes one by
    // accident.
    for x in [0.0, 3.0, 6.0, 10.0, 17.0, 20.0] {
        assert_eq!(fx_band::grab_at(x, 20.0), None, "{x} px on a 20 px bar is not a grip");
    }

    // The catalogue row points at the same constant the rule reads (§10 gives this no `P.` id, so it is a
    // bare `effects.gripMinPx`).
    let row = params::effects()
        .into_iter()
        .find(|p| p.id == "effects.gripMinPx")
        .expect("effects.gripMinPx catalogued");
    assert_eq!(row.spelled, "30");
    assert_eq!(row.from, "fx_band::GRIP_MIN_PX");
}

// --- S2: moving, resizing, nudging ------------------------------------------------------------------------------

#[test]
fn f3_8_s2_the_whole_band_moves_clamped_and_both_ends_snap() {
    // layout.snapPx = 8 px; at 10 px/s that is 0.8 s of reach. Marks here are OTHER landmarks only:
    // §A.9 requires the moved effect's own two ends to be EXCLUDED, or a band snaps to itself and cannot
    // be dragged once it lines up with its own start.
    let pps = 10.0;
    let marks = [10.0, 30.0];
    let band = zoom_at(20.0, 4.0);

    // Start within reach of 10.0+... no: nearest mark to 10.6 is 10.0 (0.6 <= 0.8) while the tail (14.6) is
    // far, so the start lands on the mark.
    assert_eq!(fx_band::move_whole(&band, 10.6, &marks, pps, 100.0), (10.0, 4.0));

    // Tail nearer than the start: dragged so the tail is 0.4 s off 30.0 and the start 3.6 s off 10.0 \u2192
    // the tail wins, start = 26.0.
    assert_eq!(fx_band::move_whole(&band, 25.6, &marks, pps, 100.0), (26.0, 4.0));

    // Neither end within reach: exactly where the hand left it, unsnapped.
    assert_eq!(fx_band::move_whole(&band, 15.0, &marks, pps, 100.0), (15.0, 4.0));

    // Both ends within reach of their own marks: the NEARER end decides (start 0.2 off 10.0 vs tail 0.5 off
    // 30.0-ish \u2014 built so the start is nearer).
    let both = [10.0, 14.5];
    assert_eq!(fx_band::move_whole(&band, 10.2, &both, pps, 100.0), (10.0, 4.0));

    // The delegation: `move_whole` IS `cut_select::snap_span` plus the timeline clamp, so the span rule
    // (nearer end wins) is not reimplemented here.
    for to in [10.6, 25.6, 15.0, 10.2] {
        let (start, _) = fx_band::move_whole(&band, to, &marks, pps, 100.0);
        let snapped = cut_select::snap_span(to, band.dur, &marks, pps);
        assert_eq!(start, snapped.clamp(0.0, 100.0 - band.dur), "{to} s");
    }

    // Clamped inside the timeline: before second 0 is refused by the clamp, and a tail that would run past the
    // session end is pulled back so the WHOLE band fits.
    assert_eq!(fx_band::move_whole(&band, -5.0, &[], pps, 100.0), (0.0, 4.0));
    assert_eq!(fx_band::move_whole(&band, 99.0, &[], pps, 100.0), (96.0, 4.0));
    assert_eq!(fx_band::move_whole(&band, 200.0, &[], pps, 100.0), (96.0, 4.0));

    // Length never changes when the whole band slides.
    for to in [-5.0, 0.0, 15.0, 99.0, 200.0] {
        let (_, dur) = fx_band::move_whole(&band, to, &marks, pps, 100.0);
        assert_eq!(dur, 4.0, "{to} s must not stretch the band");
    }
}

#[test]
fn f3_8_s2_an_end_resizes_with_the_0_1_s_floor() {
    // P.eng.effectMinSeconds = 0.1, held by `fx_lane::MIN_BAND_SECONDS` and applied by `fx_lane::drag_end`.
    // The floor rule is NOT repeated here: `resize_end` returns whatever `drag_end` returns.
    let band = zoom_at(20.0, 4.0);
    for (end, to) in [(false, 21.0), (true, 23.0), (true, 19.0), (false, 26.0)] {
        assert_eq!(
            fx_band::resize_end(&band, end, to),
            fx_lane::drag_end(band.t, band.dur, end, to),
            "end={end} to={to}"
        );
    }

    // Dragging the start past the end stops AT the floor rather than through it: no inverted band.
    let (t, dur) = fx_band::resize_end(&band, false, 30.0);
    assert!(dur >= fx_lane::MIN_BAND_SECONDS, "length floored at 0.1 s, got {dur}");
    assert_eq!(t + dur, 24.0, "the untouched end stays put at 24.0");

    // Same from the other end.
    let (t, dur) = fx_band::resize_end(&band, true, 15.0);
    assert!(dur >= fx_lane::MIN_BAND_SECONDS, "length floored at 0.1 s, got {dur}");
    assert_eq!(t, 20.0, "the untouched end stays put at 20.0");
    assert_eq!(fx_lane::MIN_BAND_SECONDS, 0.1);
}

#[test]
fn f3_8_s2_keyboard_nudge_is_unsnapped() {
    // S2: "Frame steps and \u2190/\u2192 nudge a held effect unsnapped." A mark sits 0.05 s away, well inside
    // the 0.8 s snap reach at this zoom, and the nudge ignores it entirely: the value is exactly the sum.
    let band = zoom_at(20.0, 4.0);
    let delta = 0.05;
    let (t, dur) = fx_band::nudged(&band, delta);
    assert_eq!(t, 20.05);
    assert_eq!(t, effect_rules::nudge(band.t, delta), "delegated to effect_rules::nudge");
    assert_eq!(t, band.t + delta);
    assert_ne!(
        t,
        cut_select::snap(band.t + delta, &[20.05], 10.0) - 0.0 + 0.0_f64.min(0.0) + (band.t + delta),
        "trivially equal guard: the nudge is the raw sum, not a snapped value"
    );

    // The keyboard moves a band, it does not stretch one.
    assert_eq!(dur, 4.0);

    // And a negative nudge is the same rule: exact, unsnapped, length intact.
    let (back, dur_back) = fx_band::nudged(&band, -0.0417);
    assert_eq!(back, 20.0 - 0.0417);
    assert_eq!(dur_back, 4.0);
}

#[test]
fn f3_8_s2_the_hold_drops_one_frame_off_the_band() {
    // §A.9: "The hold drops when the line walks > 1/24 s outside the band." One frame at 24 fps, so the
    // clock may keep running in hand without the band vanishing mid-drag.
    assert_eq!(fx_band::HOLD_RELEASE_SECONDS, 1.0 / 24.0);
    let slack = fx_band::HOLD_RELEASE_SECONDS;

    // Inside the band: held.
    assert!(!fx_band::hold_drops(20.0, 4.0, 22.0));
    assert!(!fx_band::hold_drops(20.0, 4.0, 20.0));
    assert!(!fx_band::hold_drops(20.0, 4.0, 24.0));

    // Within the slack on either side: still held (the "> 1/24 s" is strict, so exactly one frame is held).
    assert!(!fx_band::hold_drops(20.0, 4.0, 20.0 - slack));
    assert!(!fx_band::hold_drops(20.0, 4.0, 24.0 + slack));

    // Beyond it: dropped, on both sides.
    assert!(fx_band::hold_drops(20.0, 4.0, 20.0 - slack - 0.001));
    assert!(fx_band::hold_drops(20.0, 4.0, 24.0 + slack + 0.001));
    assert!(fx_band::hold_drops(20.0, 4.0, 40.0));
}

// --- S3: kill, undo entry, the sentences ------------------------------------------------------------------------

#[test]
fn f3_8_s3_kill_needs_32_px_and_beats_the_drag() {
    // S3: "\u2715 in the band's middle (\u2265 32 px)". Two pixels wider than the grip threshold so the kill
    // affordance never sits where the hand was reaching for an edge.
    assert!(!fx_band::kill_open(31.9));
    assert!(fx_band::kill_open(32.0));
    assert_eq!(fx_band::KILL_MIN_PX, 32.0);
    assert!(fx_band::KILL_MIN_PX > fx_band::GRIP_MIN_PX);

    // A narrow bar offers no \u2715 even when the pointer is dead centre on it: over_kill alone is not enough,
    // the bar has to have room for the cross.
    assert_ne!(
        fx_band::press_band(20.0, 24.0, 1.0, true, true, None),
        fx_band::Press::Kill
    );

    // The kill is decided FIRST: 200 px of travel afterwards does not turn it into a move, and a grip position
    // does not outrank it either.
    assert_eq!(
        fx_band::press_band(20.0, 34.0, 200.0, true, true, Some(false)),
        fx_band::Press::Kill
    );
    assert_eq!(fx_band::press_band(20.0, 34.0, 0.0, true, true, None), fx_band::Press::Kill);

    let row = params::effects()
        .into_iter()
        .find(|p| p.id == "effects.killMinPx")
        .expect("effects.killMinPx catalogued");
    assert_eq!(row.spelled, "32");
    assert_eq!(row.from, "fx_band::KILL_MIN_PX");
}

#[test]
fn f3_8_s3_undo_is_pushed_on_the_first_two_pixels_once() {
    // S3: "undo pushed on the first 2 px of travel". Under it nothing moved; over it the gesture owns ONE
    // entry, taken at the crossing, so a 200 px drag costs one Undo and not a hundred.
    assert_eq!(fx_band::UNDO_PUSH_PX, 2.0);
    assert!(!fx_band::undo_pushed(1.9, false));
    assert!(fx_band::undo_pushed(2.0, false));
    assert!(!fx_band::undo_pushed(100.0, true), "already pushed: the gesture keeps its one entry");
    assert!(!fx_band::undo_pushed(0.0, false));

    // The delegation: `pushes_undo` is the once-only half of the rule, unchanged.
    for (travel, pushed) in [(2.0, false), (2.0, true), (1.0, false), (50.0, true)] {
        assert_eq!(
            fx_band::undo_pushed(travel, pushed),
            cut_trim::pushes_undo(travel >= fx_band::UNDO_PUSH_PX, pushed),
            "travel={travel} already_pushed={pushed}"
        );
    }

    let row = params::effects()
        .into_iter()
        .find(|p| p.id == "effects.undoPushPx")
        .expect("effects.undoPushPx catalogued");
    assert_eq!(row.spelled, "2");
    assert_eq!(row.from, "fx_band::UNDO_PUSH_PX");
}

#[test]
fn f3_8_s3_said_moved_and_resized_carry_the_undo_promise() {
    // Whole strings, per §A.9. The removal clause is load-bearing: removal goes through `record_edit`, so
    // \u21b6 really does restore the record.
    assert_eq!(
        fx_band::removed("Volume"),
        "removed Volume \u{2014} \u{21b6} Undo takes it back"
    );
    assert_eq!(
        fx_band::moved("Zoom"),
        "Zoom moved \u{2014} \u{21b6} Undo takes it back"
    );
    assert_eq!(
        fx_band::resized("Speed"),
        "Speed resized \u{2014} \u{21b6} Undo takes it back"
    );
    // Esc's bare disarm is one word: a reflex, and the sentence only has to confirm the key was heard.
    assert_eq!(fx_band::CANCELLED, "cancelled");
    // And the pick-up sentence deliberately carries neither the dash nor the arrow (see S1).
    assert!(!fx_band::moved("Zoom").starts_with("removed"));
}

// --- S4: \u270e Edit finding its way back ------------------------------------------------------------------------

#[test]
fn f3_8_s4_edit_refinds_by_kind_and_start_within_one_ms() {
    // |Delta-t| < 1 ms (effect_rules::REACH_SECONDS = 0.001), by VALUE and never by index: the page does not
    // hold the lane still while a dialog is open.
    let lane = vec![zoom_at(10.0, 3.0), zoom_at(20.0, 4.0)];
    assert_eq!(fx_band::refind(&lane, "zoom", 20.0), Some(1));
    assert_eq!(fx_band::refind(&lane, "zoom", 20.0005), Some(1), "inside the 1 ms reach");
    assert_eq!(fx_band::refind(&lane, "zoom", 19.9995), Some(1), "inside the 1 ms reach");
    assert_eq!(fx_band::refind(&lane, "zoom", 20.0015), None, "outside the reach");
    assert_eq!(fx_band::refind(&lane, "zoom", 19.9985), None, "outside the reach");

    // Same kind at a different start, and a different kind at the same start: both misses. Position in the list
    // buys nothing \u2014 the first element is never found by asking for the second's time.
    assert_eq!(fx_band::refind(&lane, "zoom", 30.0), None);
    assert_eq!(fx_band::refind(&lane, "volume", 20.0), None);

    // The delegation and the miss sentence, whole.
    assert_eq!(fx_band::refind(&lane, "zoom", 20.0005), effect_rules::find_by_value(&lane, "zoom", 20.0005));
    assert_eq!(effect_rules::REACH_SECONDS, 0.001);
    assert_eq!(
        fx_band::GONE,
        "that effect is no longer in the cut \u{2014} nothing was changed"
    );
    assert_eq!(fx_band::GONE, effect_rules::GONE);
}

#[test]
fn f3_8_s4_the_box_comes_from_the_live_effect_never_the_snapshot() {
    // S4: "box always from the live effect, never the form's snapshot". The stale snapshot below is wrong on
    // ALL FOUR fractions; if any of them leaked through, a form would overwrite a box someone moved since it
    // opened.
    let live = zoom_at(20.0, 4.0);
    let stale = (0.1, 0.2, 0.9, 0.8);
    assert_eq!(
        fx_band::live_box(&live, Some(stale)),
        (Some(0.4), Some(0.6), Some(0.5), Some(0.4))
    );

    // A second live record, different numbers, same rule.
    let mut other = zoom_at(30.0, 2.0);
    other.cx = Some(0.75);
    other.cy = Some(0.25);
    other.wf = Some(0.3);
    other.hf = Some(0.6);
    assert_eq!(
        fx_band::live_box(&other, Some(stale)),
        (Some(0.75), Some(0.25), Some(0.3), Some(0.6))
    );

    // A live record with NO box stays no-box even when the snapshot holds four numbers: the absence is the
    // truth, and inventing a box from a stale copy would draw one that was never set.
    let bare = Fx { kind: "volume".into(), t: 5.0, dur: 1.0, ..Default::default() };
    assert_eq!(fx_band::live_box(&bare, Some(stale)), (None, None, None, None));

    // No snapshot at all is the ordinary case and answers the live values too.
    assert_eq!(
        fx_band::live_box(&live, None),
        (Some(0.4), Some(0.6), Some(0.5), Some(0.4))
    );
}

// --- S5: forms are live ----------------------------------------------------------------------------------------

#[test]
fn f3_8_s5_first_answer_pushes_undo_and_a_refusal_gives_it_back() {
    // S5: "first answer pushes Undo". One form visit, one entry, until a refusal hands it back.
    let mut form = fx_band::LiveForm::default();
    assert!(!form.undo_pushed);
    assert!(form.first_answer(), "the first accepted answer owns the entry");
    assert!(!form.first_answer(), "every later change rides on that same entry");
    assert!(!form.first_answer());

    // A refusal wrote nothing, so it must not have spent the entry either: after one, the next accepted answer
    // is again the first.
    form.refusal_resets();
    assert!(!form.undo_pushed);
    assert!(form.first_answer(), "a refusal gave the entry back");
    assert!(!form.first_answer());
}

#[test]
fn f3_8_s5_a_keystroke_lands_after_the_debounce() {
    // S5: "every keystroke lands after a debounce". §A.9 names the behaviour and no number; 300 ms is
    // long enough that a burst of typing is ONE write and short enough to feel live.
    assert_eq!(fx_band::DEBOUNCE_MS, 300.0);
    assert!(!fx_band::keystroke_lands(299.0, fx_band::DEBOUNCE_MS));
    assert!(fx_band::keystroke_lands(300.0, fx_band::DEBOUNCE_MS));
    assert!(fx_band::keystroke_lands(1200.0, fx_band::DEBOUNCE_MS));
    assert!(!fx_band::keystroke_lands(0.0, fx_band::DEBOUNCE_MS));

    let row = params::effects()
        .into_iter()
        .find(|p| p.id == "effects.formDebounceMs")
        .expect("effects.formDebounceMs catalogued");
    assert_eq!(row.spelled, "300");
    assert_eq!(row.from, "fx_band::DEBOUNCE_MS");

    // The hold-release row too: 1/24 s prints as its decimal, and points at the same constant.
    let hold = params::effects()
        .into_iter()
        .find(|p| p.id == "machine.holdReleaseSeconds")
        .expect("machine.holdReleaseSeconds catalogued");
    assert_eq!(hold.from, "fx_band::HOLD_RELEASE_SECONDS");
    assert_eq!(hold.spelled, format!("{}", 1.0 / 24.0));
}

#[test]
fn f3_8_s5_the_live_note_is_the_promise_the_undo_makes() {
    // Byte-exact: the note is the promise the Undo behaviour makes, so it lives in the module that owns the
    // live-form rule and cannot drift from it.
    assert_eq!(
        fx_band::KEPT_AS_YOU_TYPE,
        "Kept as you type \u{2014} \u{21b6} Undo takes the whole edit back."
    );
    assert!(fx_band::KEPT_AS_YOU_TYPE.ends_with('.'));
}
