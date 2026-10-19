// F3.1 Zoom by hand — spec/06-effects.md F3.1, steps S1–S4.
//
// The arm, the drag and the defaults are all arithmetic or a quoted sentence, so each is pinned here against the
// function that answers it. Strings are compared whole: this tree treats a reworded refusal as a broken refusal.
// No widget and no display — `rust/src/ui/window.rs` still renders only Prepare, so there is nothing to click yet.

use naivepost::cut::{Cut, Fx};
use naivepost::cut_screen;
use naivepost::fx_zoom as zoom;
use naivepost::tools;

/// A box on a 1920 × 1080 source: the picture this tree's screenshots are made of.
const SOURCE: (f64, f64) = (1920.0, 1080.0);

fn zoom_at(t: f64, stay: bool) -> Fx {
    Fx { kind: "zoom".into(), t, dur: 3.0, stay, ..Default::default() }
}

/// F3.1 S1 — `Effect ▾ → ⊕ Zoom` arms a drag, refuses without a line, and disarms when pressed again.
#[test]
fn f3_1_s1_armed_or_refused() {
    // No red line: the effect has nowhere to happen.
    assert_eq!(zoom::arm(None, false), zoom::Press::Refused(NO_LINE_SENTENCE));
    assert_eq!(NO_LINE_SENTENCE, "click a track first \u{2014} the effect needs a moment to happen at");
    // A refusal is not an arm: pressing again while refused stays refused rather than arming twice.
    assert_eq!(zoom::arm(None, true), zoom::Press::Refused(NO_LINE_SENTENCE));

    // A line arms; the same entry again takes it back (§A.1: "same kind twice disarms").
    assert_eq!(zoom::arm(Some(30.0), false), zoom::Press::Armed);
    assert_eq!(zoom::arm(Some(30.0), true), zoom::Press::Disarmed);

    // The panel text, whole: it opens with the gesture and ends on the shape/snap clause plus the join space.
    assert!(zoom::ARM_WORDS.starts_with("Drag a box on the video:"), "{}", zoom::ARM_WORDS);
    assert!(
        zoom::ARM_WORDS.ends_with("let go near the full width or height to snap to it. "),
        "the trailing space is the join: {:?}",
        zoom::ARM_WORDS
    );

    // The two tails, verbatim — and the unmarked one names the default length rather than a copy of it.
    let at_line = zoom::arm_tail(None);
    assert_eq!(at_line, "It starts at the red line and runs 3 s; the form that opens says how long.");
    assert!(at_line.contains(&format!("{} s", zoom::DEFAULT_SECONDS as i64)), "{at_line}");
    let marked = zoom::arm_tail(Some((81.0, 91.5)));
    assert_eq!(
        marked,
        format!(
            concat!(
                "It covers the marked stretch \u{2014} {} \u{2013} {}, {:.1} s \u{2014} ",
                "which the form that opens can change."
            ),
            tools::mm_ss(81.0),
            tools::mm_ss(91.5),
            10.5
        )
    );

    // Arming puts the camera layer down so the whole source shows; disarming gives the framing back.
    assert!(zoom::whole_source_shown(true));
    assert!(!zoom::whole_source_shown(false));
    // And Esc/cancel only says anything when there was an arm to lose.
    assert!(zoom::disarm(true));
    assert!(!zoom::disarm(false));
}

/// The same sentence through the module, so a test cannot pass against its own copy of the wording.
const NO_LINE_SENTENCE: &str = "click a track first \u{2014} the effect needs a moment to happen at";

/// F3.1 S2 — `Drag a box on the preview (< 12 px ignored)`, and the gesture is a free rectangle: no shape lock,
/// no snap, either drag direction.
#[test]
fn f3_1_s2_a_free_rectangle() {
    // The floor, measured on both axes.
    assert_eq!(zoom::DRAG_MIN_PX, 12.0);
    assert!(!zoom::drag_counts(11.9, 400.0), "too narrow");
    assert!(!zoom::drag_counts(400.0, 11.9), "too flat — a swipe, not a box");
    assert!(!zoom::drag_counts(4.0, 4.0));
    assert!(zoom::drag_counts(12.0, 12.0), "exactly the floor counts");

    // Either direction describes the same box.
    let forward = zoom::free_rectangle((100.0, 60.0), (300.0, 260.0)).expect("a real drag");
    let backward = zoom::free_rectangle((300.0, 260.0), (100.0, 60.0)).expect("the same drag backwards");
    assert_eq!(forward, backward);
    assert_eq!((forward.x, forward.y, forward.w, forward.h), (100.0, 60.0, 200.0, 200.0));
    assert_eq!(zoom::free_rectangle((100.0, 60.0), (105.0, 300.0)), None, "under the floor on width");

    // Free: an off-shape box stays off-shape rather than being squared to the cut's aspect.
    let wide = zoom::free_rectangle((0.0, 0.0), (960.0, 200.0)).expect("a real drag");
    assert_eq!(wide.w / wide.h, 4.8, "the drawn shape survives: {wide:?}");
    assert!(!zoom::keeps_the_cut_shape());
    // And no snap: 95 % of the width stays 95 %, it is not pulled to full.
    let near_full = zoom::free_rectangle((0.0, 0.0), (SOURCE.0 * 0.95, SOURCE.1), ).expect("a real drag");
    assert_eq!(near_full.w, SOURCE.0 * 0.95);
    assert!(!zoom::snaps_to_full_size());

    // Fractions of the SOURCE frame: centre and height only — the width follows the source's aspect (§06#1).
    let (cx, cy, hf) = zoom::to_fractions(forward, SOURCE.0, SOURCE.1);
    assert!((cx - (200.0 / 1920.0)).abs() < 1e-12, "{cx}");
    assert!((cy - (160.0 / 1080.0)).abs() < 1e-12, "{cy}");
    assert!((hf - (200.0 / 1080.0)).abs() < 1e-12, "{hf}");
    // A box hanging off the bottom is allowed: hf past 1, and §A.1 pads black.
    let off_bottom = zoom::Rect { x: 0.0, y: 0.0, w: 400.0, h: SOURCE.1 * 2.0 };
    let (_, _, tall) = zoom::to_fractions(off_bottom, SOURCE.0, SOURCE.1);
    assert_eq!(tall, 2.0);

    // The clamp §A.1 gives a rect.
    assert_eq!(zoom::clamp_rect(0.01, 0.5, 0.5), (zoom::HF_MIN, 0.5, 0.5));
    assert_eq!((zoom::HF_MIN, zoom::HF_MAX), (0.02, 12.0));
    assert_eq!(zoom::clamp_rect(13.0, 0.5, 0.5), (zoom::HF_MAX, 0.5, 0.5));
    assert_eq!((zoom::CENTRE_MIN, zoom::CENTRE_MAX), (-2.0, 3.0));
    assert_eq!(zoom::clamp_rect(1.0, -5.0, 4.0), (1.0, -2.0, 3.0));
    // In range, untouched — a clamp that moves a good rect is a bug.
    assert_eq!(zoom::clamp_rect(0.6, 0.3, 0.7), (0.6, 0.3, 0.7));
}

/// F3.1 S3 — `glide 1 s in and out · 3 s, or the marked stretch · stay when an aspect is set and no staying zoom
/// exists yet (then no glides)`.
#[test]
fn f3_1_s3_defaults() {
    // P.policy.effectDefaultFades (zoom 1) and P.policy.effectDefaultSeconds (zoom/text/svg 3).
    assert_eq!(zoom::GLIDE_SECONDS, 1.0);
    assert_eq!(zoom::DEFAULT_SECONDS, 3.0);

    // An empty aspect IS the source's own shape: no shape chosen. The readout's fallback is not a choice.
    assert!(!zoom::aspect_is_set(&Cut::default()));
    assert!(zoom::aspect_is_set(&Cut { aspect: "9:16".into(), ..Default::default() }));
    // cut_screen prints "16:9" for an unset aspect; that string is what the readout shows, not what was chosen,
    // so a cut holding it still counts as unshaped here.
    assert_ne!(cut_screen::ASPECT_DEFAULT, "");
    let fallback = Cut { aspect: String::new(), ..Default::default() };
    assert!(!zoom::aspect_is_set(&fallback), "the readout would print {} for this", cut_screen::ASPECT_DEFAULT);

    // Only a staying zoom settles the frame; a pull-back zoom and every other kind say nothing.
    let pulled = [zoom_at(10.0, false), Fx { kind: "text".into(), t: 12.0, stay: true, ..Default::default() }];
    assert!(!zoom::a_staying_zoom_exists(&pulled));
    assert!(zoom::a_staying_zoom_exists(&[zoom_at(10.0, false), zoom_at(40.0, true)]));
    assert!(!zoom::a_staying_zoom_exists(&[]));

    // The default is a staying zoom exactly once: an aspect set and nothing staying yet.
    assert!(zoom::stays_by_default(true, false));
    assert!(!zoom::stays_by_default(true, true), "one reframing is enough");
    assert!(!zoom::stays_by_default(false, false), "no shape chosen, so no reason to reframe");
    assert!(!zoom::stays_by_default(false, true));

    // Glides in and out — unless it stays, which has no way back.
    assert_eq!(zoom::fades(false), (1.0, 1.0));
    assert_eq!(zoom::fades(true), (0.0, 0.0));

    // The marked stretch outranks both the line and the default length.
    assert_eq!(zoom::place(None, 30.0), (30.0, 3.0));
    assert_eq!(zoom::place(Some((81.0, 91.5)), 30.0), (81.0, 10.5));

    // The record a finished drag leaves behind.
    let placed = zoom::new_zoom((0.25, 0.4, 0.6), 30.0, 3.0, false, 1);
    assert_eq!((placed.kind.as_str(), placed.t, placed.dur), ("zoom", 30.0, 3.0));
    assert_eq!((placed.trans, placed.tout), (1.0, 1.0));
    assert_eq!((placed.cx, placed.cy, placed.hf), (Some(0.25), Some(0.4), Some(0.6)));
    assert_eq!(placed.cam, 1);
    assert!(!placed.stay);
    // §06#1: a zoom has no width field, and linear ease writes no key.
    assert_eq!(placed.wf, None);
    assert_eq!(placed.ease, naivepost::fx_record::LINEAR_EASE);
    // Staying arrives with no glides.
    let staying = zoom::new_zoom((0.5, 0.5, 1.0), 30.0, 2.0, true, 0);
    assert!((staying.trans + staying.tout) == 0.0);
    // And a wild box is clamped on the way in rather than stored as drawn. `new_zoom` takes the tuple in §06#1's
    // own order (cx, cy, hf), so 9.0 and −9.0 are the two centres and 40.0 the height.
    let wild = zoom::new_zoom((9.0, -9.0, 40.0), 30.0, 2.0, false, 0);
    assert_eq!((wild.cx, wild.cy, wild.hf), (Some(3.0), Some(-2.0), Some(12.0)));}

/// F3.1 S3 — a finished drag is already a valid effect before the form is opened.
#[test]
fn f3_1_s3b_a_finished_drag_is_already_valid() {
    // The form is where the defaults get argued with: length, ending, two fades and a curve.
    assert_eq!(zoom::DEFAULT_SECONDS, 3.0, "the form opens on what S3 defaulted");
    assert!(!zoom::a_staying_zoom_exists(&[]), "and on no staying zoom yet");

    // A hand-placed box plus the defaults is already a valid effect before the form is touched.
    let box_ = zoom::free_rectangle((480.0, 270.0), (1200.0, 810.0)).expect("a real drag");
    let centre = zoom::to_fractions(box_, SOURCE.0, SOURCE.1);
    let (at, dur) = zoom::place(None, 81.0);
    let effect = zoom::new_zoom(centre, at, dur, zoom::stays_by_default(false, false), 0);
    assert_eq!((effect.t, effect.dur), (81.0, 3.0));
    assert!(effect.hf.unwrap() > 0.0 && effect.hf.unwrap() < 1.0, "a box inside the frame: {:?}", effect.hf);
    // The whole gesture leaves one zoom on the lane and no staying frame behind it.
    let after = [effect.clone()];
    assert!(!zoom::a_staying_zoom_exists(&after));
    assert!(zoom::a_staying_zoom_exists(&[effect, zoom_at(90.0, true)]));
}

/// F3.1 S4 — the form: its title, its five fields in the image's order, the greyed fade-out and its reason, the
/// single curve, every help string, the footer, and what Apply writes back.
#[test]
fn f3_1_s4_the_form() {
    // The title names the second it belongs to — the stamps come from the same fn the page prints.
    assert_eq!(zoom::form_title(81.0), format!("Zoom at {}", tools::mm_ss(81.0)));
    assert_eq!(zoom::form_title(0.0), format!("Zoom at {}", tools::mm_ss(0.0)));

    // The fields, in the order the image reads them.
    assert_eq!(zoom::FORM_FIELDS, ["Length (s)", "At the end", "Fade in (s)", "Fade out (s)", "Curve"]);
    // A radio pair with the chosen ending first — never a checkbox that could show both.
    assert_eq!(zoom::end_choices(false), [zoom::PULL_BACK, zoom::STAY_ON_IT]);
    assert_eq!(zoom::end_choices(true), [zoom::STAY_ON_IT, zoom::PULL_BACK]);
    assert_eq!((zoom::PULL_BACK, zoom::STAY_ON_IT), ("Pull back", "Stay on it"));

    // The greying and its reason are one rule.
    assert!(zoom::fade_out_greyed(true));
    assert!(!zoom::fade_out_greyed(false));
    assert_eq!(zoom::NO_WAY_BACK, "A camera that stays has no way back, so no fade out.");

    // One curve, and it stores nothing.
    assert_eq!(zoom::CURVE_CHOICES, ["Linear"]);
    assert_eq!(zoom::curve_stored("Linear"), naivepost::fx_record::LINEAR_EASE);
    assert_eq!(zoom::curve_stored("Linear"), "");
    // A shape from a newer build is kept rather than flattened to linear.
    assert_eq!(zoom::curve_stored("easeInOut"), "easeInOut");

    // Every help string, whole — these are the tooltips, not paraphrases of them.
    assert_eq!(zoom::LENGTH_HELP, "how long the camera move lasts altogether, fades included");
    assert_eq!(
        zoom::PULL_BACK_HELP,
        "A passing close-up: the picture closes in, holds for its seconds and opens back out on its own, \
         leaving the rest of the video framed as it was."
    );
    assert_eq!(
        zoom::STAY_ON_IT_HELP,
        "A reframing: from here on the finished video shows this region. This is how a vertical short is made \
         out of widescreen footage \u{2014} say where the action is, and say it again when it moves."
    );
    assert_eq!(
        zoom::FADE_IN_HELP,
        "how long the camera takes to arrive: 0 cuts straight to the region, 1 glides over a second"
    );
    assert_eq!(zoom::FADE_OUT_HELP, "how long it takes to come back off the region again: 0 cuts straight back");
    assert_eq!(
        zoom::CURVE_HELP,
        "the shape both fades travel in. Straight is all there is so far\u{2026}"
    );
    // The footer is read off spec/img/06-zoom.png; §A.1 does not spell it.
    assert_eq!(
        zoom::FORM_FOOTER,
        "Kept as you type \u{2014} \u{21b6} Undo takes the whole edit back."
    );

    // Apply: the floor first — 0.4 s is a camera move, anything shorter is a twitch.
    assert_eq!(zoom::MIN_SECONDS, 0.4);
    let short = zoom::Form {
        at: 81.0,
        dur: 0.39,
        stay: false,
        trans: 1.0,
        tout: 1.0,
        curve: "Linear".into(),
        cx: 0.5,
        cy: 0.5,
        hf: 0.6,
        row: 0,
    };
    let refused = zoom::apply(&short).expect_err("under the floor is refused");
    assert!(refused.contains("0.4"), "the refusal names the bound: {refused}");
    assert!(refused.contains("0.39"), "and what was typed: {refused}");

    let at_floor = zoom::Form { dur: 0.4, ..short.clone() };
    let kept = zoom::apply(&at_floor).expect("exactly the floor is a move");
    assert_eq!(kept.dur, 0.4);
    // A camera that stays has no fades to clamp — and none to grey out either.
    let staying = zoom::Form { stay: true, trans: 1.0, tout: 1.0, dur: 3.0, ..short.clone() };
    let held = zoom::apply(&staying).expect("a staying zoom is fine at 3 s");
    assert_eq!((held.trans, held.tout), (0.0, 0.0));
    assert!(held.stay);

    // Fades inside the length are kept as typed; fades overrunning it share it in proportion.
    let roomy = zoom::Form { dur: 2.0, trans: 1.0, tout: 1.0, ..short.clone() };
    let moved = zoom::apply(&roomy).expect("two seconds hold two one-second glides");
    assert_eq!((moved.trans, moved.tout), (1.0, 1.0));
    let tight = zoom::Form { dur: 1.0, trans: 1.0, tout: 1.0, ..short.clone() };
    let shared = zoom::apply(&tight).expect("a move whose fades overrun still applies");
    assert_eq!((shared.trans, shared.tout), (0.5, 0.5));
    // A negative fade is raised rather than played backwards.
    let wrong = zoom::Form { dur: 4.0, trans: -1.0, tout: 2.0, ..short.clone() };
    let fixed = zoom::apply(&wrong).expect("a negative fade is not a refusal");
    assert_eq!((fixed.trans, fixed.tout), (0.0, 2.0));

    // After every Apply the two fades fit inside the length — §A.1's clampFades in one sentence.
    for form in [at_floor.clone(), staying, roomy.clone(), tight, wrong] {
        let effect = zoom::apply(&form).expect("these all clear the floor");
        assert!(effect.trans + effect.tout <= form.dur + 1e-9, "{:?}", (effect.trans, effect.tout, form.dur));
    }

    // The record keeps the box, the second and the row it was drawn on.
    let placed = zoom::apply(&roomy).expect("a good form");
    assert_eq!((placed.kind.as_str(), placed.t, placed.cam), ("zoom", 81.0, 0));
    assert_eq!(placed.ease, "", "Linear writes no key");
}

/// F3.1 S5 — the status line: what was done, and for a staying zoom what it changes.
#[test]
fn f3_1_s5_the_status() {
    let stamp = tools::mm_ss(81.0);
    // A pull-back says only that it happened and can be taken back.
    assert_eq!(zoom::placed_status(81.0, false), format!("zoom at {stamp} \u{2014} \u{21b6} Undo takes it back"));
    // A staying zoom says what it does to the rest of the video.
    assert_eq!(
        zoom::placed_status(81.0, true),
        format!("zoom at {stamp} \u{2014} the video shows this region from here on; \u{21b6} Undo takes it back")
    );
    // The subject is shared with the re-frame sentence and the plate — one name for one zoom.
    assert_eq!(zoom::label(81.0), format!("zoom at {stamp}"));
    assert!(zoom::placed_status(81.0, false).starts_with(&zoom::label(81.0)));

    // The plate as spec/img/06-zoom.png draws it: the seconds and both glides.
    assert_eq!(
        zoom::zoom_plate(81.0, 3.0, 1.0, 1.0),
        format!("zoom at {} for 3.0s (1.0s in, 1.0s out)", stamp)
    );
    // A staying zoom's plate says no glides because it has none.
    assert_eq!(
        zoom::zoom_plate(81.0, 2.0, 0.0, 0.0),
        format!("zoom at {} for 2.0s (0.0s in, 0.0s out)", stamp)
    );
}

/// F3.1 (the rule after S5): a press on the settled preview takes the zoom in force, or draws a new box — and only
/// when nothing is armed or held, the preview is paused and the camera has settled.
#[test]
fn f3_1_s6_re_framing() {
    // All three preconditions hold: a box taken, or a box drawn clear of every one.
    assert_eq!(
        zoom::preview_press(true, true, true, Some(2)),
        Some(zoom::PressPreview::TakesZoom(2))
    );
    assert_eq!(zoom::preview_press(true, true, true, None), Some(zoom::PressPreview::DrawsNew));

    // Any one precondition failing means the press is not about framing at all.
    assert_eq!(zoom::preview_press(false, true, true, Some(0)), None, "something armed or held");
    assert_eq!(zoom::preview_press(true, false, true, Some(0)), None, "playing");
    assert_eq!(zoom::preview_press(true, true, false, Some(0)), None, "the camera is mid-glide");
    assert_eq!(zoom::preview_press(false, false, false, None), None, "and all three at once");

    // The status a re-frame leaves, sharing its subject with the placement sentence.
    let stamp = tools::mm_ss(81.0);
    assert_eq!(
        zoom::re_frame_status(81.0),
        format!("zoom at {stamp} re-framed \u{2014} \u{21b6} Undo takes it back")
    );
    assert!(zoom::re_frame_status(81.0).starts_with(&zoom::label(81.0)));
}

/// F3.1 (the paragraph after S5): the camera path — from the centred full-fill slice, gliding to each zoom's rect,
/// holding, gliding back unless staying, with a staying zoom becoming the new settled frame and fade-in winning
/// overlaps. Nothing reaches backwards.
#[test]
fn f3_1_s7_camera_path() {
    let close = |at: f64, dur: f64| Fx {
        kind: "zoom".into(),
        t: at,
        dur,
        trans: 1.0,
        tout: 1.0,
        cx: Some(0.25),
        cy: Some(0.4),
        hf: Some(0.5),
        ..Default::default()
    };
    let rect = zoom::Camera { cx: 0.25, cy: 0.4, hf: 0.5 };

    // No zooms, and a second before the first one: the whole source. Nothing reaches backwards.
    assert_eq!(zoom::camera_at(&[], 10.0), zoom::SETTLED);
    let one = [close(30.0, 6.0)];
    assert_eq!(zoom::camera_at(&one, 29.9), zoom::SETTLED);
    assert_eq!((zoom::SETTLED.cx, zoom::SETTLED.cy, zoom::SETTLED.hf), (0.5, 0.5, 1.0));

    // Gliding in: the midpoint of the arrival is halfway between where it started and its rect.
    let mid = zoom::camera_at(&one, 30.5);
    assert_eq!(mid.cx, (0.5 + 0.25) / 2.0, "x halfway");
    assert_eq!(mid.cy, (0.5 + 0.4) / 2.0);
    assert!(mid.hf > rect.hf && mid.hf < zoom::SETTLED.hf, "height between: {}", mid.hf);
    // Held at full visibility.
    assert_eq!(zoom::camera_at(&one, 33.0), rect);
    // Gliding back out: halfway between the rect and the settled frame again.
    let back = zoom::camera_at(&one, 35.5);
    assert_eq!(back.cx, (0.25 + 0.5) / 2.0);
    assert!(back.hf > rect.hf && back.hf < zoom::SETTLED.hf);
    // After it, the frame is the whole source again.
    assert_eq!(zoom::camera_at(&one, 40.0), zoom::SETTLED);

    // A staying zoom holds: its own seconds, and every second after with nothing else on the lane.
    let held = [close(30.0, 6.0)];
    let mut stays = held[0].clone();
    stays.stay = true;
    stays.tout = 0.0;
    let stays = [stays];
    assert_eq!(zoom::camera_at(&stays, 33.0), rect);
    assert_eq!(zoom::camera_at(&stays, 90.0), rect, "a staying zoom is the frame now");
    assert_eq!(zoom::settled_after(&stays, 40.0), rect);
    assert_eq!(zoom::settled_after(&one, 40.0), zoom::SETTLED, "a pull-back leaves nothing behind");

    // A zoom after a staying one glides FROM the staying rect, not from the whole source.
    let mut next = close(60.0, 6.0);
    next.cx = Some(0.8);
    next.cy = Some(0.8);
    next.hf = Some(0.25);
    let pair = [stays[0].clone(), next.clone()];
    let from_staying = zoom::camera_at(&pair, 60.5);
    assert_eq!(from_staying.cx, (rect.cx + 0.8) / 2.0, "it started on the staying rect");
    assert_eq!(from_staying.hf, (rect.hf + 0.25) / 2.0);

    // A non-zoom effect changes nothing about the camera.
    let caption = Fx {
        kind: "text".into(),
        t: 30.0,
        dur: 6.0,
        cx: Some(0.1),
        cy: Some(0.1),
        hf: Some(0.2),
        ..Default::default()
    };
    let text = [caption];
    assert_eq!(zoom::camera_at(&text, 33.0), zoom::SETTLED);

    // Overlap: fade-in wins, so the arriving zoom decides a second another zoom also covers.
    let mut other = close(33.0, 6.0);
    other.cx = Some(0.9);
    other.cy = Some(0.9);
    other.hf = Some(0.2);
    let both = [close(30.0, 6.0), other.clone()];
    let arriving = zoom::camera_at(&both, 33.5);
    assert!(arriving.cx > rect.cx && arriving.cx < other.cx.unwrap(), "travelling to the new rect: {arriving:?}");
    // And two arrivals at one second go to the earlier one.
    let mut early = close(40.0, 4.0);
    early.hf = Some(0.3);
    let mut late = close(40.5, 4.0);
    late.hf = Some(0.9);
    assert!(zoom::camera_at(&[late.clone(), early.clone()], 40.6).hf < zoom::SETTLED.hf);

    // A rect pushed past both clamps still leaves the path inside them.
    let mut huge = close(10.0, 6.0);
    huge.hf = Some(zoom::HF_MAX * 3.0);
    huge.cx = Some(zoom::CENTRE_MAX + 5.0);
    assert_eq!(zoom::camera_at(&[huge.clone()], 13.0).hf, zoom::HF_MAX, "held");
    let mut tiny = close(10.0, 6.0);
    tiny.hf = Some(zoom::HF_MIN / 10.0);
    assert_eq!(zoom::camera_at(&[tiny.clone()], 13.0).hf, zoom::HF_MIN);
    let glide_out = zoom::camera_at(&[huge], 15.5);
    assert!(glide_out.hf <= zoom::HF_MAX && glide_out.hf >= zoom::SETTLED.hf, "{glide_out:?}");
    assert!(zoom::camera_at(&[tiny], 10.5).hf >= zoom::HF_MIN);
}

/// F3.1 S3 — the form a finished drag opens already carries the defaults: nothing in it is blank, and every number
/// is one this file already owns (`place`, `stays_by_default`, `fades`, `to_fractions`/`clamp_rect`).
#[test]
fn f3_1_s3c_form_from_drag_carries_the_defaults() {
    // A box on the source's own picture, armed at the line, with no marked stretch and no shape chosen.
    let box_ = zoom::Rect { x: 480.0, y: 270.0, w: 960.0, h: 540.0 };
    let none_marked = Cut::default();

    let plain = zoom::form_from_drag(box_, SOURCE.0, SOURCE.1, 81.0, None, &none_marked.fx, false, 0);
    // P.policy.effectDefaultSeconds (zoom/text/svg 3) — the length the arm words promised.
    assert_eq!(plain.dur, zoom::DEFAULT_SECONDS);
    assert_eq!(plain.dur, 3.0);
    // P.policy.effectDefaultFades (zoom 1): a glide in and out.
    assert_eq!((plain.trans, plain.tout), (1.0, 1.0));
    assert!(!plain.stay, "no shape chosen means nothing to reframe into");
    // The line it was armed at, not the box's position or midnight.
    assert_eq!(plain.at, 81.0);
    assert_eq!(plain.curve, zoom::CURVE_CHOICES[0]);
    // Half the source in each direction: the centre lands mid-frame, hf half the height.
    assert!((plain.cx - 0.5).abs() < 1e-9, "{}", plain.cx);
    assert!((plain.cy - 0.5).abs() < 1e-9, "{}", plain.cy);
    assert!((plain.hf - 0.5).abs() < 1e-9, "{}", plain.hf);
    assert_eq!(plain.row, 0, "the row the box was drawn on (§06#1's `cam`)");

    // The marked stretch outranks the default length — same rule `place` answers, seen through the form.
    let marked = zoom::form_from_drag(box_, SOURCE.0, SOURCE.1, 81.0, Some((8.0, 18.0)), &none_marked.fx, false, 2);
    assert_eq!(marked.dur, 10.0, "a 10 s marked stretch, not 3 s");
    assert_eq!(marked.at, 81.0, "it still happens at the line the arm held");
    assert_eq!(marked.row, 2);

    // Aspect set and no staying zoom yet → stay, and a camera that stays has no glides.
    let shaped = Cut { aspect: "9:16".to_string(), ..Default::default() };
    let first_into_shape = zoom::form_from_drag(box_, SOURCE.0, SOURCE.1, 81.0, None, &shaped.fx, true, 0);
    assert!(first_into_shape.stay, "the first zoom into a shaped video reframes it");
    assert_eq!((first_into_shape.trans, first_into_shape.tout), (0.0, 0.0), "then no glides");

    // A staying zoom already exists → the next one passes instead of stacking reframings.
    let already_settled = vec![zoom_at(12.0, true)];
    let later = zoom::form_from_drag(box_, SOURCE.0, SOURCE.1, 81.0, None, &already_settled, true, 0);
    assert!(!later.stay, "one reframing is enough; this one is a passing close-up");
    assert_eq!((later.trans, later.tout), (1.0, 1.0), "and it glides both ways again");

    // A box dragged past the clamp still arrives inside it -- the form cannot open on an unrenderable rect.
    let enormous = zoom::Rect { x: 0.0, y: 0.0, w: SOURCE.0 * 20.0, h: SOURCE.1 * 20.0 };
    let clamped = zoom::form_from_drag(enormous, SOURCE.0, SOURCE.1, 81.0, None, &none_marked.fx, false, 0);
    assert_eq!(clamped.hf, zoom::HF_MAX, "hf clamped to // effects.rectHfMax");
    assert!(clamped.cx <= zoom::CENTRE_MAX && clamped.cy <= zoom::CENTRE_MAX);
}

/// F3.1 S4 (`Form "Zoom at m:ss"`): the heading names the second the zoom belongs to, so the form says which line
/// it was opened for. Distinct from [`zoom::label`], which is lower-case because it is the subject of a status
/// sentence rather than a window title.
#[test]
fn f3_1_s4b_the_form_title_names_the_second() {
    assert_eq!(zoom::form_title(81.5), "Zoom at 01:21");
    assert_eq!(zoom::form_title(0.0), "Zoom at 00:00");
    assert_eq!(zoom::form_title(3599.0), "Zoom at 59:59");
    // The two spellings differ on purpose: the title is a heading, the label is a sentence's subject.
    assert_ne!(zoom::form_title(81.5), zoom::label(81.5));
    assert_eq!(zoom::label(81.5), format!("zoom at {}", tools::mm_ss(81.5)));
}
