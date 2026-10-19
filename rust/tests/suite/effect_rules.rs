// §06-effects#7-rules — spec/06-effects.md §7's standing rules for the effects page, one test per clause in the
// order §7 writes them.
//
// Most of these rules already live in the module whose behaviour they bound; what this file pins is that they are
// still true and stated in exactly one place, so a later round cannot quietly break one while passing another.
// Everything here is plain data: no widget is built and no display is needed.

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_clamp;
use naivepost::cut_hear;
use naivepost::cut_select;
use naivepost::cut_speed;
use naivepost::cut_speed_pass;
use naivepost::cut_trim;
use naivepost::effect_rules;
use naivepost::fx_record::{self, Field};
use naivepost::fx_svg;
use naivepost::fx_text;
use naivepost::fx_zoom;
use naivepost::render_fx;
use naivepost::tools::clips::Clips;
use naivepost::tools::cutpass;
use naivepost::{cut_cards, cut as cut_mod, fx_lane};

/// Floats from arithmetic, never `assert_eq!` on a difference.
fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < 1e-9, "{asked}: got {got}, want {want}");
}

/// The kinds §7 talks about, all six in one list.
fn one_of_each() -> Vec<Fx> {
    ["zoom", "text", "svg", "speed", "volume", "label"]
        .iter()
        .enumerate()
        .map(|(i, kind)| Fx { kind: (*kind).to_string(), t: i as f64 * 10.0, dur: 2.0, ..Default::default() })
        .collect()
}

fn speed(t: f64, dur: f64, rate: f64) -> Fx {
    Fx { kind: "speed".into(), t, dur, rate, ..Default::default() }
}

/// A speed with a sound answer of its own (§7's `sound answers do not merge`).
fn speed_snd(t: f64, dur: f64, rate: f64, snd: &str) -> Fx {
    Fx { kind: "speed".into(), t, dur, rate, snd: snd.into(), ..Default::default() }
}

fn volume(t: f64, dur: f64, gain: f64) -> Fx {
    Fx { kind: "volume".into(), t, dur, gain, ..Default::default() }
}

/// The JSON one of the tools answered with.
fn json(reply: &str) -> serde_json::Value {
    serde_json::from_str(reply).expect("a tool always answers JSON")
}

// --- clause 1: one list, one owner ---------------------------------------------------------------------------

/// §06-effects#7-rules (`One list, one owner`): the kinds have one order and it belongs to the lane, and the lane
/// packs that one list — nothing hands it a per-kind list to keep in step.
#[test]
fn sec_06_effects_7_rules_s1_one_list_one_owner() {
    let order = fx_lane::MENU_ORDER;
    assert_eq!(order.len(), 6);
    for kind in order {
        assert_eq!(order.iter().filter(|other| **other == kind).count(), 1, "{kind:?} twice");
    }

    let fx = one_of_each();
    let rows = fx_lane::rows_for_effects(&fx);
    assert_eq!(rows.len(), fx.len(), "every effect is placed on a row");
    // One pass over that one list: the depth is the deepest row used, §2's `min one row`.
    assert_eq!(fx_lane::row_count(&fx), 1 + rows.iter().max().unwrap());

    // The same six kinds in the lane's order are the six the record knows about.
    for kind in order {
        assert!(fx_record::uses(kind, Field::Dur), "{kind:?} is a kind with a length");
    }
}

// --- clause 2: nothing reaches backwards ----------------------------------------------------------------------

/// §06-effects#7-rules (`nothing reaches backwards`): an effect answers at and after its own seconds. Stated as
/// equality with the no-effect answer, so the rule is "the same as if it were not there", not a literal.
#[test]
fn sec_06_effects_7_rules_s2_nothing_reaches_backwards() {
    let zoom = Fx { kind: "zoom".into(), t: 10.0, dur: 3.0, hf: Some(0.5), ..Default::default() };
    let later = zoom.clone();
    assert_eq!(
        fx_zoom::camera_at(&[zoom], 5.0),
        fx_zoom::camera_at(&[], 5.0),
        "a zoom at 10 s has not begun to move the camera at 5"
    );
    assert_ne!(
        fx_zoom::camera_at(&[later], 11.0),
        fx_zoom::camera_at(&[], 11.0),
        "and it does move it inside its own seconds"
    );

    assert_eq!(cut_speed::rate_at(&[speed(10.0, 3.0, 2.0)], 5.0), 1.0);
    assert_eq!(cut_speed::rate_at(&[speed(10.0, 3.0, 2.0)], 11.0), 2.0);
    // A volume too: silence before its band, and after it again.
    assert_eq!(cut_hear::gain_under(&[volume(10.0, 3.0, 4.0)], 5.0), 1.0);
}

// --- clause 3: dur is the bar for every kind --------------------------------------------------------------------

/// §06-effects#7-rules (`dur is the bar for every kind`): length is the one field no kind may drop, while the fields
/// that belong to one kind's picture are false for the kinds that draw nothing of that sort.
#[test]
fn sec_06_effects_7_rules_s3_dur_is_the_bar_for_every_kind() {
    for kind in fx_lane::MENU_ORDER {
        assert!(fx_record::uses(kind, Field::Dur), "{kind:?} has no length to be a band by");
    }
    // Rate belongs to a speed (and a stop's 0), gain to a volume — and to neither of the others.
    for kind in fx_lane::MENU_ORDER {
        let rate = fx_record::uses(kind, Field::Rate);
        let gain = fx_record::uses(kind, Field::Gain);
        assert_eq!(rate, kind == cut_mod::EffectKind::Speed, "{kind:?}'s rate field");
        assert_eq!(gain, kind == cut_mod::EffectKind::Volume, "{kind:?}'s gain field");
    }
}

// --- clause 4: one fade rule -------------------------------------------------------------------------------------

/// §06-effects#7-rules (`one fade rule (both fades ≥ 0, together ≤ dur, trimmed proportionally)`): the same
/// `clamp_fades` every kind's form routes through, and a real text form obeying it.
#[test]
fn sec_06_effects_7_rules_s4_one_fade_rule_for_every_kind() {
    // A negative fade is no fade.
    assert_eq!(cut_speed::clamp_fades(-1.0, 0.3, 2.0), (0.0, 0.3));
    assert_eq!(cut_speed::clamp_fades(0.3, -1.0, 2.0), (0.3, 0.0));

    // Overrunning the band: shared in proportion, and never more than the band.
    let (fin, fout) = cut_speed::clamp_fades(4.0, 2.0, 3.0);
    assert_close("the longer fade keeps its share", fin / fout, 4.0 / 2.0);
    assert!(fin + fout <= 3.0 + 1e-9, "{} + {} overran 3 s", fin, fout);

    // What fits is untouched — a rule that trimmed in the ordinary case would not be one fade rule, it would be a tax.
    assert_eq!(cut_speed::clamp_fades(0.3, 0.3, 2.0), (0.3, 0.3));

    // And the same rule out of a form: the record holds fades that fit its own band.
    let form = fx_text::Form {
        t: 4.0,
        dur: 1.0,
        trans: 0.8,
        tout: 0.6,
        curve: "Linear".into(),
        words: "hello".into(),
        on: fx_text::LOWER_THIRD,
    };
    let fx = fx_text::apply(&form).expect("words and a length");
    assert!(fx.trans + fx.tout <= fx.dur + 1e-9, "{fx:?} fades overran the band");
}

// --- clause 5: rates average, gains multiply, sound answers do not merge -----------------------------------------

/// §06-effects#7-rules (`rates average, gains multiply, sound answers do not merge`).
#[test]
fn sec_06_effects_7_rules_s5_rates_average_gains_multiply_sound_answers_do_not_merge() {
    // Rates: the arithmetic mean over the seconds both cover.
    let spans = cut_speed::rate_spans(&[speed(0.0, 10.0, 2.0), speed(0.0, 20.0, 4.0)]);
    let overlap = spans.iter().find(|s| s.from < 10.0).expect("the shared seconds");
    assert_close("two rates average", overlap.rate, (2.0 + 4.0) / 2.0);

    // Gains: the product, capped at playbin's ceiling — P.eng.maxGain.
    let fx = [volume(0.0, 10.0, 2.0), volume(0.0, 10.0, 3.0)];
    assert_close("two gains multiply", cut_hear::gain_under(&fx, 5.0), 2.0 * 3.0);
    let loud = [volume(0.0, 10.0, 6.0), volume(0.0, 10.0, 4.0)];
    assert_eq!(cut_hear::gain_under(&loud, 5.0), cut_hear::MAX_GAIN);

    // Sound answers: two stretches at one rate that hear different things stay two decisions —
    // P.policy.speedGapSeconds is the bound that would otherwise have folded them.
    let apart = cut_speed_pass::merge(&[speed_snd(0.0, 10.0, 2.0, ""), speed_snd(12.0, 10.0, 2.0, "own")]);
    assert_eq!(apart.len(), 2, "{apart:?}: one pitch and one 1× read do not merge");
    let together = cut_speed_pass::merge(&[speed(0.0, 10.0, 2.0), speed(12.0, 10.0, 2.0)]);
    assert_eq!(together.len(), 1, "{together:?}: the same decision nearer than the gap is one stretch");
}

// --- clause 6: a stop is not a rate -------------------------------------------------------------------------------

/// §06-effects#7-rules (`a stop is not a rate`): rate 0 means a held frame, and it never reaches the render as a
/// division by zero, nor gets folded into a neighbouring stretch.
#[test]
fn sec_06_effects_7_rules_s6_a_stop_is_not_a_rate() {
    let stop = speed(3.0, 2.0, 0.0);
    assert!(fx_record::is_stop(&stop));
    assert!(!fx_record::is_stop(&speed(3.0, 2.0, 1.0)), "as filmed is not a stop");
    assert!(!fx_record::is_stop(&volume(3.0, 2.0, 0.0)), "only a speed's 0 is one");

    // A held frame costs its seconds at 1× rather than dividing by 0 anywhere.
    assert_eq!(cut_speed::applied_rate(0.0), 1.0);
    assert_eq!(render_fx::setpts(cut_speed::applied_rate(0.0)), None, "a stop writes no setpts");

    let merged = cut_speed_pass::merge(&[speed(0.0, 10.0, 2.0), stop]);
    assert_eq!(merged.len(), 2, "{merged:?}: a stop is never folded into a stretch");
}

// --- clause 7: staircases are built whole or not at all -----------------------------------------------------------

/// §06-effects#7-rules (`staircases are built whole or not at all`): `ramps` answers the fades asked for when every
/// stair pays P.eng.minClipSeconds on screen, and (0, 0) — no staircase at all — when one of them would not.
#[test]
fn sec_06_effects_7_rules_s7_staircases_are_built_whole_or_not_at_all() {
    // A band long enough for a ×2 ramp at the stair price keeps its ramps.
    assert_eq!(cut_speed::ramps(2.0, 10.0, 1.0, 1.0), (1.0, 1.0));
    // The same ramps on a band where a stair of the climb renders under the shortest clip the render makes: dropped
    // whole rather than encoded as missing footage. Derived from that floor, not from a magic duration.
    let too_short = cutpass::MIN_CLIP_SECONDS;
    assert_eq!(cut_speed::ramps(4.0, too_short, 1.0, 1.0), (0.0, 0.0));
    // One more case at the ceiling of the same rule: a ×4 over two seconds cannot be built either.
    assert_eq!(cut_speed::ramps(4.0, 2.0, 1.0, 1.0), (0.0, 0.0));
}

// --- clause 8: slivers are healed, not dropped ---------------------------------------------------------------------

/// §06-effects#7-rules (`slivers are healed, not dropped`): a stretch two effects slice out of each other is given
/// to its longer neighbour, and a short stretch with nothing beside it stands.
#[test]
fn sec_06_effects_7_rules_s8_slivers_are_healed_not_dropped() {
    // Two speeds whose boundary leaves a tenth of a second at the slower rate: 0.05 s on screen.
    let spans = cut_speed::rate_spans(&[speed(0.0, 1.0, 2.0), speed(0.95, 3.0, 4.0)]);
    let healed = cut_speed::heal(spans.clone());
    assert!(!healed.is_empty(), "nothing was dropped");
    for span in &healed {
        let on_screen = (span.to - span.from) / cut_speed::applied_rate(span.rate);
        assert!(on_screen >= cutpass::MIN_CLIP_SECONDS - 1e-9, "{span:?} is still a sliver");
    }
    // The seconds are all still accounted for: healed into a neighbour, not deleted.
    let covered = |list: &[cut_speed::Span]| -> f64 { list.iter().map(|s| s.to - s.from).sum::<f64>() };
    assert!(covered(&healed) + 1e-9 >= covered(&spans), "{healed:?} lost seconds from {spans:?}");

    // A lone short stretch has no neighbour to heal into and is left alone (the §F3.3 stop case).
    let alone = cut_speed::rate_spans(&[speed(4.0, 0.2, 0.0)]);
    assert_eq!(cut_speed::heal(alone.clone()), alone);
}

// --- clause 9: only speed touches the clip list ---------------------------------------------------------------------

/// §06-effects#7-rules (`only speed touches the clip list`): an effect of any other kind lands on the lane and
/// leaves the clip's own rate alone; ⏩ is the one verb that writes a rate.
#[test]
fn sec_06_effects_7_rules_s9_only_speed_touches_the_clip_list() {
    let mut batch = Clips::new(&[(1, 0.0, 10.0)]);
    // The kinds this pass may place — the model's own list (§F3.11's tool) — none of which is a rate.
    for kind in ["zoom", "stop", "volume"] {
        let gain = (kind == "volume").then_some(2.0);
        let reply = json(&batch.add_effect(1, kind, 1.0, 3.0, gain, None));
        assert!(reply["error"].is_null(), "{kind}: {reply}");
    }
    assert_eq!(batch.clips()[0].rate, 1.0, "no effect of another kind moved the clip's rate");

    let reply = json(&batch.set_clip_speed(1, 2.0));
    assert_eq!(batch.clips()[0].rate, 2.0);
    // The clip's own seconds over the rate: what the timeline has to draw.
    assert_close("on screen", reply["on_screen"].as_f64().unwrap(), 10.0 / 2.0);
}

// --- clause 10: camera windows have no width, overlay boxes do --------------------------------------------------------

/// §06-effects#7-rules (`camera windows have no width, overlay boxes do`): a window's width is the frame's aspect
/// read off its height; an overlay's box carries its own.
#[test]
fn sec_06_effects_7_rules_s10_camera_windows_have_no_width_overlay_boxes_do() {
    use cut_mod::EffectKind;
    assert!(!fx_record::uses(EffectKind::Zoom, Field::Wf), "a window has no width of its own");
    assert!(fx_record::uses(EffectKind::Text, Field::Wf));
    assert!(fx_record::uses(EffectKind::Svg, Field::Wf));
    // The fractions a zoom stores are off the source frame, which is what makes one height enough.
    assert!(fx_record::fractions_read_off_source_frame(EffectKind::Zoom));

    let window = Fx { kind: "zoom".into(), t: 0.0, dur: 2.0, cx: Some(0.5), cy: Some(0.5), hf: Some(0.5), ..Default::default() };
    assert!(window.wf.is_none(), "and the record holds none");
    // An overlay's box, by contrast, cannot be built without one: `wf` is a plain field of it.
    let box_ = fx_text::LOWER_THIRD;
    assert!(box_.wf > 0.0 && box_.hf > 0.0, "{box_:?}");
}

// --- clause 11: the box belongs to the picture not the form -------------------------------------------------------------

/// §06-effects#7-rules (`the box belongs to the picture not the form`): wherever a box arrives from it is held
/// inside the frame and above the floors, and holding an already-inside box changes nothing.
#[test]
fn sec_06_effects_7_rules_s11_the_box_belongs_to_the_picture_not_the_form() {
    let outside = fx_text::Box_ { cx: 2.0, cy: -1.0, wf: 0.4, hf: 0.2 };
    let inside = outside.clamp();
    assert!(inside.cx - inside.wf / 2.0 >= -1e-9 && inside.cx + inside.wf / 2.0 <= 1.0 + 1e-9, "{inside:?}");
    assert!(inside.cy - inside.hf / 2.0 >= -1e-9 && inside.cy + inside.hf / 2.0 <= 1.0 + 1e-9, "{inside:?}");

    // Bigger than the frame comes back the size of the frame; smaller than grabbable comes up to the floor.
    let huge = fx_text::Box_ { cx: 0.5, cy: 0.5, wf: 4.0, hf: 4.0 }.clamp();
    assert_eq!((huge.wf, huge.hf), (1.0, 1.0));
    let tiny = fx_text::Box_ { cx: 0.5, cy: 0.5, wf: 0.001, hf: 0.001 }.clamp();
    assert_eq!((tiny.wf, tiny.hf), (fx_text::MIN_WIDTH_FRACTION, fx_text::MIN_HEIGHT_FRACTION));

    // Idempotent: the picture is the bound, so a second pass has nothing left to do.
    assert_eq!(inside.clamp(), inside);
}

// --- clause 12: forms find effects by value ----------------------------------------------------------------------------

/// §06-effects#7-rules (`forms find effects by value`): ✎ Edit re-finds its effect by kind and start, within a
/// millisecond, and says so when it cannot.
#[test]
fn sec_06_effects_7_rules_s12_forms_find_effects_by_value() {
    let fx = [Fx { kind: "zoom".into(), t: 10.0, dur: 2.0, ..Default::default() }, speed(10.0, 2.0, 2.0)];
    // Found by kind and second, the millisecond of float slop included.
    assert_eq!(effect_rules::find_by_value(&fx, "zoom", 10.0), Some(0));
    assert_eq!(effect_rules::find_by_value(&fx, "zoom", 10.0 + effect_rules::REACH_SECONDS / 2.0), Some(0));
    assert_eq!(effect_rules::find_by_value(&fx, "speed", 10.0), Some(1), "the second is not enough on its own");

    // A different kind at that second, or the right kind elsewhere, is not the effect the form opened on.
    assert_eq!(effect_rules::find_by_value(&fx, "text", 10.0), None);
    assert_eq!(effect_rules::find_by_value(&fx, "zoom", 10.5), None, "a nudge has moved it out from under us");

    // And what the page says: nothing was changed, which is the promise a stale index would have broken.
    assert!(effect_rules::GONE.contains('\u{2014}'), "{}", effect_rules::GONE);
    assert!(effect_rules::GONE.ends_with("nothing was changed"), "{}", effect_rules::GONE);
}

// --- clause 13: one Undo per visit, empty effects never placed, a click is not a drag -------------------------------------

/// §06-effects#7-rules (`one Undo per visit, drag or hold`, `empty effects are never placed`, `a click is not a
/// drag`): the three bounds on what a gesture may cost and what a form may place.
#[test]
fn sec_06_effects_7_rules_s13_one_undo_per_visit_empty_never_placed_and_a_click_is_not_a_drag() {
    // One Undo per gesture: the first move pushes, every further one of the same gesture does not.
    assert!(cut_trim::pushes_undo(true, false));
    assert!(!cut_trim::pushes_undo(true, true), "the second move of one drag is not a second edit");
    assert!(!cut_trim::pushes_undo(false, false), "a press that moved nothing owes nothing back");

    // A click: under the slop and on the same row.
    assert!(cut_trim::is_click(0.0, false));
    assert!(cut_trim::is_click(cut_trim::ROW_TRAVEL_PX - 1.0, false));
    assert!(!cut_trim::is_click(cut_trim::ROW_TRAVEL_PX, false), "travel is a drag");

    // Empty effects: no words, no file, no gain.
    let blank = |words: &str| fx_text::Form {
        t: 1.0,
        dur: 2.0,
        trans: 0.3,
        tout: 0.3,
        curve: "Linear".into(),
        words: words.into(),
        on: fx_text::LOWER_THIRD,
    };
    assert_eq!(fx_text::apply(&blank("   ")), Err(fx_text::NO_WORDS.to_string()));
    let no_file = fx_svg::Form {
        t: 1.0,
        dur: 2.0,
        trans: 0.3,
        tout: 0.3,
        curve: "Linear".into(),
        file: "  ".into(),
        on: fx_text::LOWER_THIRD,
    };
    assert_eq!(fx_svg::apply(&no_file), Err(fx_svg::NO_FILE.to_string()));

    let mut batch = Clips::new(&[(1, 0.0, 10.0)]);
    let refusal = json(&batch.add_effect(1, "volume", 1.0, 3.0, Some(1.0), None));
    assert_eq!(refusal["error"], "a volume gain of 1 changes nothing -- leave the audio alone");
}

// --- clause 14: framing is done paused, and the hit order on the picture ----------------------------------------------

/// §06-effects#7-rules (`framing is done paused`; `hit order on the picture: held box → held camera → texts
/// top-down → the framing zoom`).
#[test]
fn sec_06_effects_7_rules_s14_framing_is_done_paused_and_hit_order() {
    // Framing only while nothing is armed and the clock is stopped.
    assert_eq!(fx_zoom::preview_press(true, true, true, Some(2)), Some(fx_zoom::PressPreview::TakesZoom(2)));
    assert_eq!(fx_zoom::preview_press(false, true, true, Some(0)), None, "armed");
    assert_eq!(fx_zoom::preview_press(true, false, true, Some(0)), None, "playing");

    use effect_rules::{Press, Hit};
    let armed = [(3usize, 0.4f64)];
    assert_eq!(effect_rules::grab(Press::Armed, Some(1), Some(2), &armed, Some(9)), None);
    assert_eq!(effect_rules::grab(Press::Playing, Some(1), Some(2), &armed, Some(9)), None);

    // The order itself: box over camera over text over the framing rect.
    assert_eq!(effect_rules::grab(Press::Idle, Some(1), Some(2), &armed, Some(9)), Some(Hit::HeldBox(1)));
    assert_eq!(effect_rules::grab(Press::Idle, None, Some(2), &armed, Some(9)), Some(Hit::HeldCamera(2)));

    // Texts top-down — given out of order so the answer proves it is by y and not by arrival.
    let two = [(7usize, 0.7f64), (4, 0.2)];
    assert_eq!(effect_rules::grab(Press::Idle, None, None, &two, Some(9)), Some(Hit::Text(4)));
    // The framing rect only when the pointer holds nothing and no effect is in hand.
    assert_eq!(effect_rules::grab(Press::Idle, None, None, &[], Some(9)), Some(Hit::FramingZoom(9)));
}

// --- clause 15: snapping is pixel-constant and nudging unsnapped ---------------------------------------------------------

/// §06-effects#7-rules (`snapping is pixel-constant and nudging unsnapped`): a drag asks in pixels, so the answer
/// does not change with the zoom; a nudge asks in seconds and gets them.
#[test]
fn sec_06_effects_7_rules_s15_snapping_is_pixel_constant_and_nudging_is_unsnapped() {
    // `snap_to` is given a reach in pixels and no pixels-per-second: one call, the same answer at any zoom —
    // effects.snapPx on the picture, layout.snapPx (cut_select::SNAP_PX) on the lane.
    let lines = fx_text::frame_lines(800.0);
    assert_eq!(fx_text::snap_to(397.0, &lines, fx_text::SNAP_PX), 400.0);
    assert_eq!(fx_text::snap_to(397.0, &lines, fx_text::SNAP_PX / 2.0), 400.0);
    assert_eq!(fx_text::snap_to(300.0, &lines, fx_text::SNAP_PX), 300.0, "out of reach: where the hand left it");
    assert_eq!(cut_select::SNAP_PX, 8.0);

    // A nudge is not a drag: no marks are offered, so the second asked for is the second got — even with a mark
    // sitting under it that P.policy.snapToleranceSeconds would have pulled a dragged band onto.
    let marks = [12.0];
    assert_eq!(effect_rules::nudge(11.96, 0.04), 12.0 - 0.0);
    assert_close("a nudge lands where it was aimed", effect_rules::nudge(11.9, 0.05), 11.95);
    let band = cut_select::Selection { start: 11.9, end: 13.9, scope: cut_select::Scope::Footage { row: 0 } };
    let dragged = cut_select::move_band(&band, 11.96, &marks, 100.0);
    assert_eq!(dragged.start, 12.0, "the same second, dragged, is snapped onto the mark");
}

// --- clause 16: effects cannot leave the timeline --------------------------------------------------------------------------

/// §06-effects#7-rules (`effects cannot leave the timeline`): a span is clamped into what exists, and a span that
/// clamps to nothing is refused naming the length it was asked against.
#[test]
fn sec_06_effects_7_rules_s16_effects_cannot_leave_the_timeline() {
    let mut batch = Clips::new(&[(1, 0.0, 10.0)]);
    let reply = json(&batch.add_effect(1, "zoom", -3.0, 25.0, None, None));
    assert_eq!(reply["clamped"], serde_json::json!(true), "{reply}");
    assert_eq!(reply["span"], serde_json::json!([0.0, 10.0]), "{reply}");

    // Wholely outside: refused, and the refusal says the clip's own length so the caller can see its arithmetic.
    let gone = json(&batch.add_effect(1, "zoom", 20.0, 30.0, None, None));
    let reason = gone["error"].as_str().expect("a refusal names a reason");
    assert!(reason.contains("10.00 s"), "{reason}");

    // And the lane's own drag will not answer a start before the first second.
    let band = cut_select::Selection { start: 1.0, end: 3.0, scope: cut_select::Scope::Footage { row: 0 } };
    assert_eq!(cut_select::move_band(&band, -50.0, &[], 100.0).start, 0.0);
}

// --- clause 17: the lane draws what the render does -------------------------------------------------------------------------

/// §06-effects#7-rules (`the lane draws what the render does`): one record read twice — the fades the form wrote are
/// the fades the render's filters carry, so a fade trimmed for the band is trimmed the same way in both.
#[test]
fn sec_06_effects_7_rules_s17_the_lane_draws_what_the_render_does() {
    let form = fx_text::Form {
        t: 4.0,
        dur: 1.0,
        trans: 0.8,
        tout: 0.6,
        curve: "Linear".into(),
        words: "hello".into(),
        on: fx_text::LOWER_THIRD,
    };
    let record = fx_text::apply(&form).expect("a caption");
    let end = record.t + record.dur;
    let chain = render_fx::overlay_chain("movie", (record.t, end), (record.trans, record.tout), (0, 0, 640, 128));

    // The fade out starts where the band ends minus the record's own fade, and the enable window is the band.
    let expected_out = format!("fade=t=out:st={}:d={}:alpha=1", end - record.tout, record.tout);
    assert!(chain.contains(&expected_out), "{chain:?} should carry {expected_out}");
    let expected_window = format!(
        "overlay=0:0:eof_action=pass:enable=between(t\\,{}\\,{})",
        record.t, end
    );
    assert!(chain.contains(&expected_window), "{chain:?}");

    // The label is deliberately different: read by the briefs, never painted.
    assert!(!render_fx::is_rendered("label"));
    assert!(render_fx::is_rendered("text"));
}

// --- clause 18: static card files are finished pictures ----------------------------------------------------------------------

/// §06-effects#7-rules (`static card files are finished pictures`): a card is drawn at the render's frame size and
/// every animation starts at 0 and waits, so the file opened in a browser is the finished badge.
#[test]
fn sec_06_effects_7_rules_s18_static_card_files_are_finished_pictures() {
    assert!(cut_cards::is_frame_size(cut_cards::CANVAS));
    let (w, h) = cut_cards::CANVAS;
    assert!(!cut_cards::is_frame_size((w / 2, h / 2)), "a half-size file is not a card");

    // Waits at the start and reaches the end: the state with nothing run yet is the finished one.
    assert!(cut_cards::starts_at_zero_and_waits(&[0.0, 0.4, 1.0]));
    assert!(!cut_cards::starts_at_zero_and_waits(&[0.25, 1.0]), "caught mid-move when nothing has run");
}

// --- clause 19: a path with ? is not a file -------------------------------------------------------------------------------------

/// §06-effects#7-rules (`a path with ? is not a file`): the query a card carries is its declared inputs, split off
/// before the path is resolved and escaped on the way in so a value cannot forge either.
#[test]
fn sec_06_effects_7_rules_s19_a_path_with_a_question_mark_is_not_a_file() {
    let seg = Seg { ins: "project:assets/tier.svg?title=Hi%3F".into(), ..Default::default() };
    let asset = seg.insert_asset(std::path::Path::new("/root"), std::path::Path::new("/root/p.naivepost"))
        .expect("an insert names an asset");
    assert!(asset.to_string_lossy().ends_with("tier.svg"), "{asset:?}: the query is not part of the file");

    let pairs = seg.insert_query().expect("the query survives as parameters");
    assert_eq!(pairs, vec![("title".to_string(), "Hi%3F".to_string())], "values verbatim");

    // And a value cannot become a second parameter, or a second file.
    let escaped = cut_cards::esc("A?B&C=D");
    assert!(!escaped.contains('?') && !escaped.contains('&') && !escaped.contains('='), "{escaped}");
    assert!(escaped.starts_with('A') && escaped.ends_with('D'), "{escaped}");
}

// --- clause 20: suggested effects are clamped against the cut as applied -------------------------------------------------------------

/// §06-effects#7-rules (`suggested effects are clamped against the cut as applied`): the clamp runs over the cut as
/// it now stands — a band across footage the final cut does not keep is trimmed to what it does, and an effect with
/// nothing under it is dropped, with the number logged. Floor: P.eng.effectMinSurvivingSeconds.
#[test]
fn sec_06_effects_7_rules_s20_suggested_effects_are_clamped_against_the_cut_as_applied() {
    let scene = |s: f64, e: f64| Seg { s, e, ..Default::default() };
    let card = Seg { ins: "project:assets/tier.svg".into(), dur: 5.0, ..Default::default() };

    let mut cut = Cut::default();
    // Ten seconds of footage, a card, ten more seconds: the middle is not footage at all.
    cut.segs = vec![scene(0.0, 10.0), card, scene(20.0, 30.0)];
    cut.fx = vec![
        Fx { kind: "zoom".into(), t: 8.0, dur: 9.0, ..Default::default() },
        Fx { kind: "text".into(), t: 12.0, dur: 1.0, ..Default::default() },
    ];

    let clamped = cut_clamp::clamp_to_cut(&cut);
    assert_eq!(clamped.kept.len(), 1, "{:?}", clamped.kept);
    let kept = &clamped.kept[0];
    // Trimmed to the seconds that are still footage rather than dropped for reaching into the card.
    assert_eq!((kept.t, kept.dur), (8.0, 2.0), "{kept:?}");
    assert_eq!(clamped.dropped, 1);
    assert_eq!(cut_clamp::log_line(clamped.dropped), ">>> 1 effect(s) pointed at footage the final cut does not keep \u{2014} dropped");

    // The floor is §10's: a band trimmed under it is a drop, not a stub.
    assert_eq!(cut_clamp::MIN_SURVIVING_SECONDS, 1.0);
}
