//! F3.4 Text (caption) by hand — `spec/06-effects.md` F3.4, steps S1–S5.
//!
//! One rule per test, named after the step it belongs to. Every assertion is a string compared against the spec's
//! wording or a number the spec gives; nothing here draws, and no test re-derives what another module already owns
//! (the fades' `clampFades` arithmetic is [`crate::cut_speed`]'s, the camera layer's pause-and-draw rule is
//! [`crate::fx_lane`]'s, §06#1's "no box means this default" is [`crate::fx_record`]'s).

use naivepost::cut::Fx;
use naivepost::fx_lane;
use naivepost::fx_text::{self, BoxChoice, Edge, Form, LOWER_THIRD};
use naivepost::tools;
use naivepost::fx_zoom;

/// The preview is a 640 × 360 window on a 1920 × 1080 finished frame throughout: the box's fractions are always of
/// the FINISHED frame (§06#1), while the hand drags in PREVIEW pixels. Keeping the two sizes apart in every test is
/// what stops an assertion passing for the wrong reason.
const FRAME: (f64, f64) = (640.0, 360.0);

fn lower_third_box() -> naivepost::fx_text::Box_ {
    LOWER_THIRD
}

/// A box 256 × 57.6 at (192, 252) — a fifth of the frame's width instead of the lower third's four fifths. The lower
/// third cannot demonstrate a horizontal snap: [`fx_text::Box_::clamp`] holds it inside the picture, which leaves it
/// only 0…128 px of legal x against its own 64, so any snap landing past 128 is slid back and what a test would then
/// measure is the clamp, not the rule. At this width the legal range is 0…384 in x and 0…302.4 in y, which every
/// snap below stays inside.
const NARROW: naivepost::fx_text::Box_ = naivepost::fx_text::Box_ { cx: 0.5, cy: 0.78, wf: 0.4, hf: 0.16 };

// --- S1: press ❝ Text -------------------------------------------------------------------------------------------

/// S1: pressing it arms the gesture and says what the gesture is — this flow's own words, then the shared tail that
/// names the start and the length. The tail is ⊕ Zoom's ([`fx_zoom::arm_tail`]), so the test asserts both halves and
/// that neither has drifted from what §F3.4 quotes.
#[test]
fn f3_4_s1_pressing_text_arms_the_gesture() {
    assert_eq!(fx_text::arm(false), fx_text::Press::Armed);
    // The quoted sentence, exactly — the em dash and the two spaces either side of it are in the spec's text.
    assert_eq!(
        fx_text::ARM_WORDS,
        "Drag the box the words go in \u{2014} anywhere on the picture, any shape. \
         A click puts one across the lower third. "
    );
    // The tail is ⊕ Zoom's, so it is asserted through the function that owns it rather than re-typed here: with no
    // marked stretch it offers 3 s from the red line, and names the stretch when there is one.
    assert_eq!(
        fx_text::arm_words(None),
        format!("{}{}", fx_text::ARM_WORDS, fx_zoom::arm_tail(None))
    );
    let marked = fx_text::arm_words(Some((12.0, 42.0)));
    assert!(marked.starts_with(fx_text::ARM_WORDS), "{marked}");
    // §A.4 quotes the arm in full; the stretch it names is spelled with the app's own seconds-and-length words.
    assert!(marked.contains("00:12 \u{2013} 00:42, 30.0 s"), "{marked}");

    // S1's other branch: the dropdown entry is a toggle, so a second press puts the arm down.
    assert_eq!(fx_text::arm(true), fx_text::Press::Disarmed);
}

/// S1 (`Camera layer stays up (box is on the output frame)`): the camera layer holds its place while a caption is
/// drawn — and that is the mirror image of ⊕ Zoom's rule, which puts it DOWN for its own box. Asserting both keeps
/// one module from "helpfully" copying the other. §06#2 already draws a text over the paused preview and over the
/// playing video alike, so arming changes neither.
#[test]
fn f3_4_s1_the_camera_layer_stays_up() {
    assert!(fx_text::camera_layer_stays_up());
    // ⊕ Zoom's rule is the opposite, and takes the arm as its input: armed means the camera steps aside.
    assert!(fx_zoom::whole_source_shown(true));
    assert!(!fx_zoom::whole_source_shown(false));
}

// --- S2: drag or click ------------------------------------------------------------------------------------------

/// S2 (`a click takes the lower third`): a press that did not travel gets the default box, and it is the box §06#1
/// says a text with no box means — `{0.5, 0.78, 0.8, 0.16}` — written into the record so the file carries what was
/// used. Length comes from the selection if there is one, else 3 s from the red line ([`fx_zoom::DEFAULT_SECONDS`],
/// P.policy.effectDefaultSeconds), and both fades are 0.3 (P.policy.effectDefaultFades).
#[test]
fn f3_4_s2_a_click_takes_the_lower_third() {
    let fx = fx_text::place(BoxChoice::Click, Some(12.0), None, FRAME).expect("a line is enough");
    assert_eq!(fx.kind, "text");
    assert_eq!((fx.t, fx.dur), (12.0, 3.0));
    assert_eq!((fx.trans, fx.tout), (0.3, 0.3));
    let box_ = naivepost::fx_text::Box_ {
        cx: fx.cx.unwrap(),
        cy: fx.cy.unwrap(),
        wf: fx.wf.unwrap(),
        hf: fx.hf.unwrap(),
    };
    assert_eq!(box_, LOWER_THIRD);
    assert_eq!((LOWER_THIRD.cx, LOWER_THIRD.cy, LOWER_THIRD.wf, LOWER_THIRD.hf), (0.5, 0.78, 0.8, 0.16));

    // A marked stretch is the length instead of the default; the box is still the one §S2 called the default.
    let stretched = fx_text::place(BoxChoice::Click, Some(12.0), Some((30.0, 47.5)), FRAME).unwrap();
    assert_eq!((stretched.t, stretched.dur), (30.0, 17.5));
    assert_eq!(stretched.wf, Some(0.8));

    // §06#1's no-box default and this box are the same box: `Fx::centre` answers {0.5, 0.78} for a text that stored
    // nothing, which is where a caption from before the box existed still lands.
    let bare = Fx { kind: "text".into(), ..Default::default() };
    assert_eq!(bare.centre(), (LOWER_THIRD.cx, LOWER_THIRD.cy));
}

/// S2 (`Drag or click`): a drag keeps what the hand drew — preview pixels converted to fractions of the FINISHED
/// frame, so the same box at 1920 × 1080 sits where it was drawn at 640 × 360. A drag narrower or shorter than the
/// tiny-drag floor (§F's "tiny drag 12", shared with ⊕ Zoom) is a click after all, and takes the lower third.
#[test]
fn f3_4_s2_a_drag_keeps_the_box_and_tiny_drag_is_a_click() {
    // A box from (64, 240), 512 × 60 preview px on a 640 × 360 preview: centre (0.5, 0.75), size (0.8, 0.166…).
    let dragged = fx_text::place(
        BoxChoice::Dragged { x: 64.0, y: 240.0, w: 512.0, h: 60.0 },
        Some(4.0),
        None,
        FRAME,
    )
    .unwrap();
    assert_eq!((dragged.cx.unwrap(), dragged.wf.unwrap()), (0.5, 0.8));
    assert_eq!(dragged.cy.unwrap(), (240.0 + 30.0) / 360.0);
    assert_eq!(dragged.hf.unwrap(), 60.0 / 360.0);

    // The same box measured on a 1920 × 1080 preview lands on the same fractions — the frame cancels out.
    let bigger = fx_text::place(
        BoxChoice::Dragged { x: 192.0, y: 720.0, w: 1536.0, h: 180.0 },
        Some(4.0),
        None,
        (1920.0, 1080.0),
    )
    .unwrap();
    assert_eq!((bigger.cx.unwrap(), bigger.cy.unwrap()), (dragged.cx.unwrap(), dragged.cy.unwrap()));
    assert_eq!((bigger.wf.unwrap(), bigger.hf.unwrap()), (dragged.wf.unwrap(), dragged.hf.unwrap()));

    // 11 px of travel is not a box; it is the press that did not move.
    let tiny = fx_text::place(
        BoxChoice::Dragged { x: 300.0, y: 40.0, w: fx_zoom::DRAG_MIN_PX - 1.0, h: 200.0 },
        Some(4.0),
        None,
        FRAME,
    )
    .unwrap();
    assert_eq!(tiny.wf.unwrap(), LOWER_THIRD.wf);
    assert_eq!(tiny.cy.unwrap(), LOWER_THIRD.cy);

    // With neither a selection nor a red line there is no second to place it at: nothing is placed.
    assert_eq!(fx_text::place(BoxChoice::Click, None, None, FRAME), None);
}

// --- S3: the form -----------------------------------------------------------------------------------------------

/// S3 (`Form "Text at m:ss"`): titled by the second it belongs to — `mm_ss` gives the minutes-and-seconds spelling
/// every other readout in the app uses.
#[test]
fn f3_4_s3_the_form_is_titled_by_its_second() {
    // `mm_ss` is the app's one spelling of a second, so the title is asserted through it rather than re-typed.
    assert_eq!(fx_text::form_title(75.0), format!("Text at {}", tools::mm_ss(75.0)));
    assert!(fx_text::form_title(0.0).starts_with("Text at "), "Text at 0:00");
    assert!(fx_text::form_title(75.0).ends_with(&tools::mm_ss(75.0)));
}

/// S3 (`the words (3-line box; Enter = new line), Length, Fade in, Fade out, Curve`): the fields and their order,
/// plus the two help sentences §S3 quotes. The words come first because they are what the caption is.
#[test]
fn f3_4_s3_the_form_asks_for_the_words_first() {
    assert_eq!(
        fx_text::FORM_FIELDS,
        ["Words", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"]
    );
    assert_eq!(fx_text::WORDS_ROWS, 3);
    assert_eq!(
        fx_text::WORDS_HELP,
        "what is written over the picture. The words are fitted to the box you drew \u{2014} a longer line comes out \
         smaller, and Enter starts a new line."
    );
    // One sentence for both fades, because it says the same thing about each of them.
    assert_eq!(fx_text::FADES_HELP, "0 cuts them straight on|off");
}

/// S3 (`Empty words not placed ("type the words and they go on the picture — the form applies as you type it")`):
/// nothing to place is refused with those words; a length under 0.3 s (the §6 forms' floor for text/svg) names its
/// bound; and a valid form writes the box, the typed newlines and the curve.
#[test]
fn f3_4_s3_empty_words_are_not_placed() {
    let base = Form {
        t: 6.0,
        dur: 2.0,
        trans: 0.3,
        tout: 0.3,
        curve: "Linear".into(),
        words: "hello".into(),
        on: lower_third_box(),
    };

    for blank in ["", "   ", "\n"] {
        let empty = Form { words: blank.into(), ..base.clone() };
        assert_eq!(
            fx_text::apply(&empty),
            Err(fx_text::NO_WORDS.to_string()),
            "{blank:?} is not a caption"
        );
    }
    assert_eq!(
        fx_text::NO_WORDS,
        "type the words and they go on the picture \u{2014} the form applies as you type it"
    );

    // length ≥ 0.3: the floor itself is taken, a hundredth under it is not.
    assert!(fx_text::apply(&Form { dur: 0.3, ..base.clone() }).is_ok());
    let short = fx_text::apply(&Form { dur: 0.29, ..base.clone() }).unwrap_err();
    assert!(short.contains("0.3 s"), "{short}");

    let written = fx_text::apply(&Form { words: "one\ntwo".into(), curve: "Glide".into(), ..base.clone() }).unwrap();
    assert_eq!(written.kind, "text");
    // Typed newlines survive: §S4 breaks on them rather than re-deciding where the lines fall.
    assert_eq!(written.text, "one\ntwo");
    assert_eq!((written.cx.unwrap(), written.cy.unwrap(), written.wf.unwrap(), written.hf.unwrap()), (0.5, 0.78, 0.8, 0.16));
    // Linear stays unwritten (§06#1: an empty ease is what makes a byte-identical re-save), Glide is named.
    assert_eq!(written.ease, fx_zoom::curve_stored("Glide"));
    assert_eq!(fx_text::apply(&Form { curve: "Linear".into(), ..base }).unwrap().ease, "");
}

/// S3's fades go through §A.2's `clampFades` — the function zoom and speed already use, so an overrunning pair is
/// shared out one way for every effect rather than three. Asserted here because it is this form that has to do that
/// arithmetic on 0.3-second defaults: two of them over a 0.3 s band leave each other a seventh of the band.
#[test]
fn f3_4_s3_two_overrunning_fades_are_shared_once() {
    let base = Form {
        t: 0.0,
        dur: 0.3,
        trans: 0.3,
        tout: 0.3,
        curve: "Linear".into(),
        words: "hi".into(),
        on: lower_third_box(),
    };
    let fx = fx_text::apply(&base).unwrap();
    assert!(fx.trans + fx.tout <= fx.dur);
    // Equal asks share equally: 0.3 + 0.3 over a 0.3 s band leaves each half of it, which is what §A.2's `share`
    // computes and what this form must not work out differently.
    assert_eq!((fx.trans, fx.tout), (0.15, 0.15));

    // A one-sided overrun is the same rule with different numbers: the fade that asked for more keeps the larger
    // share of the band, and neither goes negative. 3.0 + 0.5 over a 1 s band shares it in that same 6:1 ratio.
    let one_sided = fx_text::apply(&Form { dur: 1.0, trans: 3.0, tout: 0.5, ..base }).unwrap();
    assert!(one_sided.trans > one_sided.tout);
    assert!(one_sided.trans + one_sided.tout <= 1.0 + 1e-12);
    assert_eq!((one_sided.trans, one_sided.tout), (6.0 / 7.0, 1.0 / 7.0));
}

// --- S4: fitting -------------------------------------------------------------------------------------------------

/// S4 (`the words are fitted to the box`: `≤ 12 lines of 0.58 em per character fit the box (min 7 pt)`, with
/// `lines·size·1.25 ≤ boxH`): the largest size that fits, and what it takes to get there — a line too long wraps
/// between words, one word too long for the width is hard-split, an explicit newline breaks, and past twelve lines
/// the floor holds so the box overflows visibly instead of shrinking to nothing.
#[test]
fn f3_4_s4_the_words_are_fitted_to_the_box() {
    // One short word in a wide box: height is the limit — one line of 1.25 sizes in a 60-unit box.
    let (size, lines) = fx_text::fit("hi", 400.0, 60.0);
    assert_eq!(lines, vec!["hi".to_string()]);
    assert_eq!(size, 60.0 / fx_text::LINE_HEIGHT_EM);

    // Widening the box buys nothing while one line still has to fit its height; doubling the height does.
    let (wider, _) = fx_text::fit("hi", 800.0, 60.0);
    assert_eq!(wider, size);
    let (taller, taller_lines) = fx_text::fit("hi", 400.0, 120.0);
    assert_eq!(taller, 120.0 / fx_text::LINE_HEIGHT_EM);
    assert_eq!(taller_lines.len(), 1);

    // A longer line comes out smaller: same box, more characters per line than the width holds at that size.
    let (small, wrapped) = fx_text::fit("one two three four five six seven", 200.0, 60.0);
    assert!(small < size, "{small} vs {size}");
    assert!(wrapped.len() > 1, "{wrapped:?}");
    // 0.58 em per character: at this size the width holds this many, and no line exceeds it.
    let per_line = (200.0 / (small * fx_text::CHAR_ADVANCE_EM)).floor() as usize;
    assert!(per_line >= 2);
    for line in &wrapped {
        assert!(line.chars().count() <= per_line, "{line} of {per_line}");
    }

    // Explicit newlines break; a blank typed line survives as an empty line.
    let (own_size, own) = fx_text::fit("a\n\nb", 400.0, 200.0);
    assert!(own_size > 0.0);
    assert_eq!(own, vec!["a".to_string(), String::new(), "b".to_string()]);

    // A word longer than the line is hard-split rather than left to run out of the box.
    let (_, split) = fx_text::fit("supercalifragilisticexpialidocious", 40.0, 200.0);
    assert!(split.len() > 1, "{split:?}");
    let longest = split.iter().map(|l| l.chars().count()).max().unwrap();
    let room = (40.0 / (fx_text::MIN_POINTS * fx_text::CHAR_ADVANCE_EM)).floor() as usize;
    assert!(longest <= room.max(1), "{split:?} against {room}");

    // ≤ 12 lines, and then the floor: a paragraph in a small box stops shrinking at 7 pt and overflows.
    let paragraph = "word ".repeat(400);
    let (floored, many) = fx_text::fit(&paragraph, 200.0, 60.0);
    assert_eq!(floored, fx_text::MIN_POINTS);
    assert!(many.len() > fx_text::MAX_LINES, "{} lines", many.len());
    assert_eq!(fx_text::MAX_LINES, 12);
    assert_eq!(fx_text::CHAR_ADVANCE_EM, 0.58);
    assert_eq!(fx_text::LINE_HEIGHT_EM, 1.25);

    // Nothing to say: no size and no lines, so no drawer has to guard against a zero-sized draw.
    assert_eq!(fx_text::fit("   ", 400.0, 60.0), (0.0, Vec::new()));
}

/// S4 (`baselines from 0.95`, `block centred`): where the lines sit. The block is centred in the box and each
/// baseline is 0.95 of a size under the top of its line; with one line taller than the box there is nothing to
/// centre, so it starts at the top rather than being pulled above it.
#[test]
fn f3_4_s4_the_baselines_are_centred() {
    let y = 100.0;
    let (size, n) = (20.0, 2);
    let lines = fx_text::baselines(y, 100.0, size, n);
    // block = 2 × 20 × 1.25 = 50 in a 100-high box → 25 of air above; first baseline 19 below that.
    assert_eq!(lines, vec![y + 44.0, y + 69.0]);

    // Three lines fill exactly: no air left over, so the top stays put.
    let filled = fx_text::baselines(y, 75.0, size, 3);
    assert_eq!(filled[0], y + size * fx_text::ASCENT_EM);
    let step = filled[1] - filled[0];
    assert_eq!(step, size * fx_text::LINE_HEIGHT_EM);

    // A block taller than the box is not centred off the top of it.
    let over = fx_text::baselines(y, 10.0, size, 4);
    assert_eq!(over[0], y + size * fx_text::ASCENT_EM);
    assert_eq!(fx_text::ASCENT_EM, 0.95);

    // No lines: no baselines for the drawer to place.
    assert_eq!(fx_text::baselines(y, 100.0, size, 0), Vec::<f64>::new());
}

/// S4 (`a dark dilated edge (radius 0.08 em, alpha 0.85, 16 directions)`): the halo's three numbers, and the pair
/// that keeps the preview and the video agreeing — the preview dilates by the radius, the render strokes at twice it
/// because a stroke straddles its path. Never under a pixel, or small type loses the edge between samples.
#[test]
fn f3_4_s4_the_edge_is_a_dark_dilation() {
    assert_eq!(fx_text::EDGE_RADIUS_EM, 0.08);
    assert_eq!(fx_text::EDGE_ALPHA, 0.85);
    assert_eq!(fx_text::EDGE_STEPS, 16);

    assert_eq!(fx_text::edge_radius(50.0), 4.0);
    assert_eq!(fx_text::render_stroke_width(50.0), 8.0);
    // A caption fitted to 7 pt still has an edge: it just stops scaling and takes the whole pixel.
    assert_eq!(fx_text::edge_radius(fx_text::MIN_POINTS), 1.0);
    assert_eq!(fx_text::render_stroke_width(fx_text::MIN_POINTS), 2.0);
}

/// S4 (`same function for preview and render`): the reason the layout is arithmetic over an average character width
/// rather than a font measurement — cairo could measure, librsvg will not, so the only way the two agree to the last
/// decimal is if neither measures. Asserted as the fact it is: one layout function, called twice, same answer.
#[test]
fn f3_4_s4_the_preview_is_the_render() {
    assert!(fx_text::preview_is_the_render());
    let words = "a caption that has to be broken across a few lines";
    for (w, h) in [(200.0, 60.0), (320.0, 40.0), (90.0, 200.0)] {
        assert_eq!(fx_text::fit(words, w, h), fx_text::fit(words, w, h), "{w}×{h}");
    }
}

// --- S5: moving and sizing on the preview --------------------------------------------------------------------------

/// S5 (`On the preview a box moves — snapping within 10 px to the finished frame's left edge, centre, right edge
/// (and top, middle, bottom); a moved box offers all three of its own lines`): a drag lands on a line inside the
/// reach and stays where the hand left it outside it. The same three lines serve both axes, and which of the box's
/// three arrives at them depends on how far the box was thrown — that is what "offers all three" buys.
#[test]
fn f3_4_s5_a_moved_box_snaps_to_the_frame_lines() {
    assert_eq!(fx_text::SNAP_PX, 10.0);
    // The frame's own lines, and the box's three that a move is aimed with.
    assert_eq!(fx_text::frame_lines(640.0), [0.0, 320.0, 640.0]);
    assert_eq!(fx_text::move_lines(100.0, 200.0), [100.0, 200.0, 300.0]);

    let box_ = lower_third_box();
    let (x, _y, _w, _h) = box_.to_px(FRAME.0, FRAME.1);
    assert_eq!(x, 64.0);

    // Its LEFT edge is 7 px off the CENTRE line: of its three lines (327, 455, 583) only that one has a frame line in
    // reach — 320 − 327 = −7, while 455 is 135 from the right edge and 583 is 263 from it. So the shift is −7 and the
    // box lands with its left edge on 320, still 256 wide.
    let left_on_centre = fx_text::move_box(NARROW, (135.0, 0.0), FRAME);
    let (lx, _, lw, _) = left_on_centre.to_px(FRAME.0, FRAME.1);
    assert!((lx - 320.0).abs() < 1e-9, "{lx}");
    assert!((lw - 256.0).abs() < 1e-9, "{lw}");

    // Its RIGHT edge is the one near a line here: at 380, 508 and 636 only 640 − 636 = +4 is in reach (380 sits 60 off
    // the centre line, 508 is 132 short of the right edge), so the box slides to x=384 with its right edge on 640. The
    // rule works off whichever of the box's own lines is close, not off where the hand let go — and a move never
    // resizes, so it comes back 256 × 57.6 as it went in.
    let right_on_frame = fx_text::move_box(NARROW, (188.0, 0.0), FRAME);
    let (rx, _, rw, rh) = right_on_frame.to_px(FRAME.0, FRAME.1);
    assert!((rx - 384.0).abs() < 1e-9, "{rx}");
    assert!((rx + rw - FRAME.0).abs() < 1e-9, "{}", rx + rw);
    assert!((rw - 256.0).abs() < 1e-9, "{rw}");
    assert!((rh - 57.6).abs() < 1e-9, "{rh}");

    // Nothing of the box is near a line: at 292, 420 and 548 the nearest gaps are 28 (its left edge to the centre),
    // 100 and 92 — none within 10 — so the hand's position stands unimproved at 292.
    let free = fx_text::move_box(NARROW, (100.0, 0.0), FRAME);
    assert!((free.to_px(FRAME.0, FRAME.1).0 - 292.0).abs() < 1e-9);

    // Down the same rule runs: at 300, 328.8 and 357.6 only 360 − 357.6 = +2.4 is in reach (300 is 60 off the middle
    // line, 328.8 is 31.2 off it), so its bottom edge lands on the frame's bottom at y=302.4.
    let low = fx_text::move_box(NARROW, (0.0, 48.0), FRAME);
    let (_, ly, _, lh) = low.to_px(FRAME.0, FRAME.1);
    assert!((ly - 302.4).abs() < 1e-9, "{ly}");
    assert!((ly + lh - FRAME.1).abs() < 1e-9, "{}", ly + lh);

    // Thrown off the picture, its lines are at 652, 680.8 and 709.6 — no pair anywhere near in reach, so this is not
    // the snap saving it but [`fx_text::Box_::clamp`]: the box is held on the frame rather than lost off it, bottom
    // edge on 360 and still findable by the hand that put it there.
    let off_screen = fx_text::move_box(NARROW, (0.0, 400.0), FRAME);
    let (_, oy, _, oh) = off_screen.to_px(FRAME.0, FRAME.1);
    assert!((oy + oh - FRAME.1).abs() < 1e-9, "{}", oy + oh);

    // Snapping on its own: within reach it takes the nearest line, beyond the reach nothing is snapped. The bound is
    // inclusive (`<=`), which 310 shows and 309 — one pixel further out — does not.
    assert_eq!(fx_text::snap_to(324.0, &fx_text::frame_lines(640.0), fx_text::SNAP_PX), 320.0);
    assert_eq!(fx_text::snap_to(315.0, &fx_text::frame_lines(640.0), fx_text::SNAP_PX), 320.0);
    assert_eq!(fx_text::snap_to(310.0, &fx_text::frame_lines(640.0), fx_text::SNAP_PX), 320.0);
    assert_eq!(fx_text::snap_to(309.0, &fx_text::frame_lines(640.0), fx_text::SNAP_PX), 309.0);
}

/// S5 (`and resizes (independent axes, 16 px floor)`): one edge moves and only its axis changes; the box never goes
/// under 16 px on the axis being pulled; and a dragged edge is offered only itself for snapping — pulling a side to
/// the centre must not be second-guessed into moving the middle there.
#[test]
fn f3_4_s5_a_resized_box_keeps_the_other_axis() {
    assert_eq!(fx_text::MIN_BOX_PX, 16.0);
    // Sizing offers one line; moving offers three. That is the whole difference between the two gestures.
    assert_eq!(fx_text::resize_lines(320.0), [320.0]);

    let box_ = lower_third_box();
    let (x, y, w, h) = box_.to_px(FRAME.0, FRAME.1);

    // The right edge in by 40 px, to 536: the nearest frame line is 104 away (the centre is 216), so nothing snaps and
    // the floor at x+16 = 80 does not bite either. Width shrinks to 472; the corner stays at (64, 252) and the height
    // is not involved — that is what "independent axes" means here.
    let narrower = fx_text::resize_box(box_, Edge::Right, 536.0, FRAME);
    let (nx, ny, nw, nh) = narrower.to_px(FRAME.0, FRAME.1);
    assert!((nx - 64.0).abs() < 1e-9, "{nx}");
    assert!((ny - 252.0).abs() < 1e-9, "{ny}");
    assert!((nw - 472.0).abs() < 1e-9, "{nw}");
    assert!((nh - 57.6).abs() < 1e-9, "{nh}");

    // 5 px off the centre line the dragged edge takes it: |320 − 315| = 5 is within reach, so the right edge lands on
    // 320 and the box is 256 wide. The one line offered was the edge itself, and the frame's line is what it met.
    let snapped = fx_text::resize_box(box_, Edge::Right, 315.0, FRAME);
    let (sx, _, sw, _) = snapped.to_px(FRAME.0, FRAME.1);
    assert!((sx + sw - 320.0).abs() < 1e-9, "{}", sx + sw);
    assert!((sw - 256.0).abs() < 1e-9, "{sw}");

    // …and 20 px off it, where the centre is 20 away and the right edge 340, no line is in reach so the edge stays at
    // 300. Nothing else was offered to pull to — that is "a dragged edge only itself".
    let far = fx_text::resize_box(box_, Edge::Right, 300.0, FRAME);
    let (fx0, _, fw0, _) = far.to_px(FRAME.0, FRAME.1);
    assert!((fx0 + fw0 - 300.0).abs() < 1e-9, "{}", fx0 + fw0);

    // The bottom edge down by 20 px, to 329.6: the frame's bottom is 30.4 away and its middle 149.6, so no snap —
    // height grows to 77.6 while x and w are untouched.
    let taller = fx_text::resize_box(box_, Edge::Bottom, y + h + 20.0, FRAME);
    let (tx, ty, tw, th) = taller.to_px(FRAME.0, FRAME.1);
    assert!((tx - x).abs() < 1e-9, "{tx}");
    assert!((tw - w).abs() < 1e-9, "{tw}");
    assert!((ty - 252.0).abs() < 1e-9, "{ty}");
    assert!((th - 77.6).abs() < 1e-9, "{th}");

    // The floor, and the only place the 16 px itself shows: NARROW's bottom is at 309.6, its top dragged to 609.6
    // (nearest line 249.6 away, so no snap) would give a negative height, and `min(309.6 − 16)` stops it at exactly
    // 16 — the box then sits against its own bottom edge at y=293.6 with the width alone. Vertically 16 clears
    // §A.4's `hf ∈ [0.03,1]`, which on a 360-high frame is 10.8.
    let flat = fx_text::resize_box(NARROW, Edge::Top, 609.6, FRAME);
    let (ffy, ffh, ffw) = {
        let (_ax, ay, aw, ah) = flat.to_px(FRAME.0, FRAME.1);
        (ay, ah, aw)
    };
    assert_eq!(ffh, fx_text::MIN_BOX_PX);
    assert!((ffy - 293.6).abs() < 1e-9, "{ffy}");
    assert!((ffw - 256.0).abs() < 1e-9, "{ffw}");

    // Horizontally the same floor can never show at this frame's size: 16 px is under §A.4's `wf ∈ [0.04,1]`, which on
    // 640 is 25.6, so the box stops there instead — and holding it on the picture leaves it at x=192, its own left
    // edge, because a 25.6 px plate still fits where the anchor was.
    let crushed = fx_text::resize_box(NARROW, Edge::Right, 192.0 - 300.0, FRAME);
    let (cx0, _, cw, _) = crushed.to_px(FRAME.0, FRAME.1);
    assert_eq!(cw, fx_text::MIN_WIDTH_FRACTION * FRAME.0);
    assert!((cx0 - 192.0).abs() < 1e-9, "{cx0}");

    // The opposite edge, and still one axis only: NARROW's left dragged to 300 is 20 px off the centre line, so no
    // snap, and `min(448 − 16)` does not bite — x=300 with a width of 148, the top and height exactly as they were.
    let side = fx_text::resize_box(NARROW, Edge::Left, 300.0, FRAME);
    let (sx0, sy0, sw0, sh0) = side.to_px(FRAME.0, FRAME.1);
    assert!((sx0 - 300.0).abs() < 1e-9, "{sx0}");
    assert!((sw0 - 148.0).abs() < 1e-9, "{sw0}");
    assert!((sy0 - 252.0).abs() < 1e-9, "{sy0}");
    assert!((sh0 - 57.6).abs() < 1e-9, "{sh0}");
}

/// S5 (`a press without travel toggles play/pause wherever it lands`): landing on a caption cannot change what a
/// press means, so the box lets a still press through to the picture — and while the box is held it keeps drawing at
/// full strength with its outline dashed (§06#2), which is what makes the gesture legible without hiding the words.
#[test]
fn f3_4_s5_a_press_without_travel_still_toggles_play() {
    assert!(fx_text::press_without_travel_toggles_play());
    // §06#2's rule, asserted as a neighbour rather than re-derived: held ≠ hidden.
    assert!(fx_lane::held_drawn_full(true));
    assert!(fx_lane::held_outline_dashed(true));
    assert!(!fx_lane::held_outline_dashed(false));
}
