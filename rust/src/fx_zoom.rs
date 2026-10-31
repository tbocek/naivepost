//! F3.1 Zoom by hand — `spec/06-effects.md` F3.1, steps S1–S5 and the camera path after them.
//!
//! ✚ Effect ▾ → ⊕ Zoom arms a drag; a box drawn on the preview becomes a zoom at the red line (or over the marked stretch) with its defaults; the form settles seconds and ending.
//! All of it is arithmetic or strings and lives here; the widget layer only forwards presses.

use crate::cut::{Cut, EffectKind, Fx};
use crate::fx_record;
use crate::tools;

// --- S1: arming -----------------------------------------------------------------------------------------------

/// F3.1 S1 (`a line?` → no): ✚ Zoom's own refusal, worded apart from [`crate::cut_copy::NO_LINE_YET`], [`crate::cut_insert::NO_LINE_YET`] and [`crate::cut_copy::NO_LANE_LINE_YET`].
pub const NO_LINE: &str = "click a track first \u{2014} the effect needs a moment to happen at";

/// F3.1 S1: what pressing ⊕ Zoom did.
#[derive(Debug, Clone, PartialEq)]
pub enum Press {
    Armed,
    Disarmed,
    Refused(&'static str),
}

/// F3.1 S1 (`same entry again disarms`, spec/inventory/effects.md §A.1): the dropdown is a toggle; arming an arm already out takes it back.
pub fn arm(line: Option<f64>, already_armed: bool) -> Press {
    let Some(_) = line else {
        return Press::Refused(NO_LINE);
    };
    if already_armed {
        return Press::Disarmed;
    }
    Press::Armed
}

/// F3.1 S1 (`Status/panel text`): the arm's words up to the shape clause (spec/inventory/effects.md §A.1); the trailing space joins [`arm_tail`].
///
/// **The contradiction.** These words promise "The box keeps the cut's shape" and a snap to full width or height, which F3.1 S2 refuses
/// (`spec/img/06-zoom.png`: "A free rectangle"). The clause stays because this tree pins quoted UI strings whole, while the BEHAVIOUR follows S2:
/// see [`free_rectangle`], [`keeps_the_cut_shape`] and [`snaps_to_full_size`]. Neither half should be "fixed" to match the other.
pub const ARM_WORDS: &str = "Drag a box on the video: the picture zooms there and comes back out on its own, \
                             or stays on it \u{2014} the form that opens is where that is said. The box keeps \
                             the cut's shape; let go near the full width or height to snap to it. ";

/// F3.1 S1: the second half of the panel text. The seconds it names are [`DEFAULT_SECONDS`]; the red line is named, not printed (§A.1).
pub fn arm_tail(marked: Option<(f64, f64)>) -> String {
    match marked {
        None => format!(
            "It starts at the red line and runs {} s; the form that opens says how long.",
            DEFAULT_SECONDS as i64
        ),
        Some((from, to)) => format!(
            concat!(
                "It covers the marked stretch \u{2014} {} \u{2013} {}, {:.1} s \u{2014} ",
                "which the form that opens can change."
            ),
            tools::mm_ss(from),
            tools::mm_ss(to),
            to - from
        ),
    }
}

/// F3.1 S1 (`Camera layer goes down; whole source visible`): while a box is drawn the framing is out of the way.
pub fn whole_source_shown(armed: bool) -> bool {
    armed
}

/// F3.1 S1 (`same entry again disarms`; Esc too — spec/inventory/cut.md §E): whether the arm was there to lose.
pub fn disarm(armed: bool) -> bool {
    armed
}

// --- S2: the drag ---------------------------------------------------------------------------------------------

/// F3.1 S2 (`Drag a box on the preview (< 12 px ignored)`; `// effects.dragMinPx`, spec/inventory/effects.md §F "tiny drag 12"): below this a press is a click.
pub const DRAG_MIN_PX: f64 = 12.0;

/// F3.1 S2: did the drag describe a box? Both axes must clear it: a 200 × 4 px swipe is a flick.
pub fn drag_counts(width_px: f64, height_px: f64) -> bool {
    width_px >= DRAG_MIN_PX && height_px >= DRAG_MIN_PX
}

/// The same floor for moving an ALREADY-DRAWN box (§F3.4 S5): one axis travels, so either axis clearing `DRAG_MIN_PX` counts, unlike [`drag_counts`]' both-axes rule.
pub fn travel_one_axis(dx_px: f64, dy_px: f64) -> bool {
    dx_px >= DRAG_MIN_PX || dy_px >= DRAG_MIN_PX
}

/// F3.1 S2: a rectangle in preview pixels — the box as drawn, before it becomes fractions of the source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// F3.1 S2 (`the gesture is a free rectangle`): the box from press to release, either direction, no aspect lock or snap (see [`ARM_WORDS`]).
pub fn free_rectangle(from: (f64, f64), to: (f64, f64)) -> Option<Rect> {
    let (x0, y0) = (from.0.min(to.0), from.1.min(to.1));
    let (w, h) = ((from.0 - to.0).abs(), (from.1 - to.1).abs());
    if !drag_counts(w, h) {
        return None;
    }
    Some(Rect { x: x0, y: y0, w, h })
}

/// F3.1 S2: the release does not snap a near-full box to full; a function so the answer can be asserted.
pub fn snaps_to_full_size() -> bool {
    false
}

/// F3.1 S2: the box does not inherit the cut's aspect either.
pub fn keeps_the_cut_shape() -> bool {
    false
}

/// F3.1 S2 → §06#1 (`cx, cy, hf … fractions of the SOURCE frame (width follows the aspect)`): the box as the record stores it.
///
/// A box may hang off the source, so `hf` can exceed 1: §A.1 normalises against the source and the render pads black.
pub fn to_fractions(rect: Rect, source_w: f64, source_h: f64) -> (f64, f64, f64) {
    let (cx, cy) = (
        (rect.x + rect.w / 2.0) / source_w,
        (rect.y + rect.h / 2.0) / source_h,
    );
    (cx, cy, rect.h / source_h)
}

/// F3.1 S2 (`Rect clamp hf ∈ [0.02, 12]`; `// effects.rectHfMin` — §A.1's number, no §10 row): the smallest region worth a zoom.
pub const HF_MIN: f64 = 0.02;

/// F3.1 S2 (`hf ∈ [0.02, 12]`; `// effects.rectHfMax`): the largest.
pub const HF_MAX: f64 = 12.0;

/// F3.1 S2 (`centre ∈ [−2, 3]`; `// effects.rectCentreMin`): how far off-frame a zoom may point.
pub const CENTRE_MIN: f64 = -2.0;

/// F3.1 S2 (`centre ∈ [−2, 3]`; `// effects.rectCentreMax`).
pub const CENTRE_MAX: f64 = 3.0;

/// F3.1 S2: the clamp §A.1 gives a rect, applied on the way in; only the three stored numbers are clamped.
pub fn clamp_rect(hf: f64, cx: f64, cy: f64) -> (f64, f64, f64) {
    (
        hf.clamp(HF_MIN, HF_MAX),
        cx.clamp(CENTRE_MIN, CENTRE_MAX),
        cy.clamp(CENTRE_MIN, CENTRE_MAX),
    )
}

// --- S3: the defaults ------------------------------------------------------------------------------------------

/// F3.1 S3 (`glide 1 s in and out`; `// P.policy.effectDefaultFades`, whose row reads zoom 1).
pub const GLIDE_SECONDS: f64 = 1.0;

/// F3.1 S3 (`3 s, or the marked stretch`; `// P.policy.effectDefaultSeconds`, whose row reads zoom/text/svg 3).
pub const DEFAULT_SECONDS: f64 = 3.0;

/// F3.1 S3 (`stay when an aspect is set`): has this cut been given a shape? An empty `aspect` is the source's own shape.
///
/// Not [`crate::cut_screen::ASPECT_DEFAULT`]: its `"16:9"` is only what the readout prints, and would make every project want a staying zoom.
pub fn aspect_is_set(cut: &Cut) -> bool {
    !cut.aspect.is_empty()
}

/// F3.1 S3 (`and no staying zoom exists yet`): is the frame already settled by a staying zoom? Asked through the typed kind, so a stale `stay` on a text doesn't count.
pub fn a_staying_zoom_exists(fx: &[Fx]) -> bool {
    fx.iter()
        .any(|effect| effect.effect_kind() == Some(EffectKind::Zoom) && effect.stay)
}

/// F3.1 S3 (`stay when an aspect is set and no staying zoom exists yet`): the first zoom into a shaped video reframes it; later ones are passing close-ups.
pub fn stays_by_default(aspect_set: bool, staying_exists: bool) -> bool {
    aspect_set && !staying_exists
}

/// F3.1 S3 (`then no glides`): a staying camera has nowhere to glide back to; the form greys the fade-out for that reason.
pub fn fades(stay: bool) -> (f64, f64) {
    if stay {
        return (0.0, 0.0);
    }
    (GLIDE_SECONDS, GLIDE_SECONDS)
}

/// F3.1 S3 (`length 3 s (or the marked stretch)`): where the zoom goes and how long; a marked stretch outranks the line and the default.
pub fn place(marked: Option<(f64, f64)>, line: f64) -> (f64, f64) {
    match marked {
        Some((from, to)) => (from, to - from),
        None => (line, DEFAULT_SECONDS),
    }
}

/// F3.1 S3: the record a finished drag creates: clamped fractions, defaults for the rest, and the row it was drawn on (§06#1's `cam`).
///
/// `ease` stays [`fx_record::LINEAR_EASE`] (no key written, so old files come back byte-identical); `wf` stays `None` (§06#1).
pub fn new_zoom(centre: (f64, f64, f64), at: f64, dur: f64, stay: bool, row: usize) -> Fx {
    let (hf, cx, cy) = clamp_rect(centre.2, centre.0, centre.1);
    let (trans, tout) = fades(stay);
    Fx {
        kind: "zoom".into(),
        t: at,
        dur,
        trans,
        tout,
        stay,
        cx: Some(cx),
        cy: Some(cy),
        hf: Some(hf),
        cam: row as i32,
        ..Default::default()
    }
}

// --- S4: the form ---------------------------------------------------------------------------------------------

/// F3.1 S4 (`Form "Zoom at m:ss"`): the window's title, named for its second.
pub fn form_title(t: f64) -> String {
    format!("Zoom at {}", tools::mm_ss(t))
}

/// F3.1 S4: the form's fields in the order `spec/img/06-zoom.png` shows them: Length, ending, two fades, curve.
pub const FORM_FIELDS: [&str; 5] = [
    "Length (s)",
    "At the end",
    "Fade in (s)",
    "Fade out (s)",
    "Curve",
];

/// F3.1 S4 (`At the end: Pull back / Stay on it`): the two endings, a radio pair; `end_choices` puts the chosen one first.
pub const PULL_BACK: &str = "Pull back";
pub const STAY_ON_IT: &str = "Stay on it";

pub fn end_choices(stay: bool) -> [&'static str; 2] {
    if stay {
        [STAY_ON_IT, PULL_BACK]
    } else {
        [PULL_BACK, STAY_ON_IT]
    }
}

/// F3.1 S4 (greyed when staying): the greying and its reason ([`NO_WAY_BACK`]) are one rule.
pub fn fade_out_greyed(stay: bool) -> bool {
    stay
}

/// F3.1 S4: that sentence, verbatim (spec/inventory/effects.md §A.1).
pub const NO_WAY_BACK: &str = "A camera that stays has no way back, so no fade out.";

/// F3.1 S4 (`Curve (Linear)`): one shape, because one is implemented (§A.1: "Straight is all there is so far…").
pub const CURVE_CHOICES: [&str; 1] = ["Linear"];

/// F3.1 S4: what the curve stores. "Linear" is [`fx_record::LINEAR_EASE`] (empty: no `ease` key, §06#1); a newer build's name is kept untouched.
pub fn curve_stored(name: &str) -> &str {
    if name == CURVE_CHOICES[0] {
        return fx_record::LINEAR_EASE;
    }
    name
}

// The field help, verbatim from spec/inventory/effects.md §A.1: each is its field's tooltip.
pub const LENGTH_HELP: &str = "how long the camera move lasts altogether, fades included";
pub const PULL_BACK_HELP: &str =
    "A passing close-up: the picture closes in, holds for its seconds and opens back \
                                  out on its own, leaving the rest of the video framed as it was.";
pub const STAY_ON_IT_HELP: &str = "A reframing: from here on the finished video shows this region. This is how a \
                                   vertical short is made out of widescreen footage \u{2014} say where the action \
                                   is, and say it again when it moves.";
pub const FADE_IN_HELP: &str = concat!(
    "how long the camera takes to arrive: 0 cuts straight to the region, ",
    "1 glides over a second"
);
/// F3.1 S4: Fade out's help — replaced by [`NO_WAY_BACK`] once staying is chosen.
pub const FADE_OUT_HELP: &str =
    "how long it takes to come back off the region again: 0 cuts straight back";
pub const CURVE_HELP: &str =
    "the shape both fades travel in. Straight is all there is so far\u{2026}";

/// F3.1 S4: the form's footer, read off `spec/img/06-zoom.png` (§A.1 does not spell it).
pub const FORM_FOOTER: &str = "Kept as you type \u{2014} \u{21b6} Undo takes the whole edit back.";

/// F3.1 S2 → S3: the form a finished drag opens, carrying every default §S3 gives rather than a blank sheet.
///
/// The box arrives in picture pixels and leaves as source fractions ([`to_fractions`], [`clamp_rect`]). `at` is the arm's line, the length
/// comes from [`place`], `stay` from [`stays_by_default`] read against this cut, and the fades from [`fades`].
///
/// `marked` is handed in because the window has no marking seam yet (the UI passes `None`); the marked branch is proven logic-side only.
pub fn form_from_drag(
    box_: Rect,
    source_w: f64,
    source_h: f64,
    at: f64,
    marked: Option<(f64, f64)>,
    fx: &[Fx],
    aspect_set: bool,
    row: usize,
) -> Form {
    let (_line, dur) = place(marked, at);
    let stay = stays_by_default(aspect_set, a_staying_zoom_exists(fx));
    let (trans, tout) = fades(stay);
    let (cx, cy, hf) = to_fractions(box_, source_w, source_h);
    let (hf, cx, cy) = clamp_rect(hf, cx, cy);
    Form {
        at,
        dur,
        stay,
        trans,
        tout,
        curve: CURVE_CHOICES[0].to_string(),
        cx,
        cy,
        hf,
        row,
    }
}

/// F3.1 S4 (`Apply: dur ≥ 0.4`): `// effects.zoomFloorSeconds` — §06 §6's form floor, no `P.` row in §10.
/// [`crate::cut_insert::seconds_accepts`] is the insert's sibling, with its own floor.
pub const MIN_SECONDS: f64 = 0.4;

/// F3.1 S4: what the form holds when Apply is pressed: seconds as typed, ending as chosen, and the clamped box in source fractions.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub at: f64,
    pub dur: f64,
    pub stay: bool,
    pub trans: f64,
    pub tout: f64,
    /// The curve's display name; [`curve_stored`] decides what gets written.
    pub curve: String,
    pub cx: f64,
    pub cy: f64,
    pub hf: f64,
    pub row: usize,
}

/// F3.1 S4 (`Apply: dur ≥ 0.4, clampFades`): the record the form writes back.
///
/// Below [`MIN_SECONDS`] it refuses, naming the floor (the sentence is ours; §F3.1 gives none).
///
/// Fades follow §A.1 and the prototype's `clampFades` (go/cut_fx.go): staying means no glides; otherwise neither is negative, and an overrun
/// is shared in proportion rather than one fade losing everything.
pub fn apply(form: &Form) -> Result<Fx, String> {
    if form.dur < MIN_SECONDS {
        return Err(format!(
            "a camera move of {:.2} s is shorter than the shortest one this form takes ({MIN_SECONDS} s)",
            form.dur
        ));
    }
    let mut effect = new_zoom(
        (form.cx, form.cy, form.hf),
        form.at,
        form.dur,
        form.stay,
        form.row,
    );
    if !form.stay {
        let trans = form.trans.max(0.0);
        let tout = form.tout.max(0.0);
        let sum = trans + tout;
        let (trans, tout) = if sum > form.dur && sum > 0.0 {
            let share = form.dur / sum;
            (trans * share, tout * share)
        } else {
            (trans, tout)
        };
        effect.trans = trans;
        effect.tout = tout;
    }
    effect.ease = curve_stored(&form.curve).to_string();
    Ok(effect)
}

// --- S5: what the page says -------------------------------------------------------------------------------------

/// F3.1 S5 (`"<label> — ↶ Undo takes it back"`): the `<label>` §F3.1 leaves open: the status line's subject, distinct from the lane bar's `X.Xs` and [`zoom_plate`].
pub fn label(t: f64) -> String {
    format!("zoom at {}", tools::mm_ss(t))
}

/// F3.1 S5: the status after a zoom is placed; a staying zoom says what it changes (§A.1 spells both shapes).
pub fn placed_status(t: f64, stay: bool) -> String {
    let name = label(t);
    if stay {
        return format!(
            "{name} \u{2014} the video shows this region from here on; \u{21b6} Undo takes it back"
        );
    }
    format!("{name} \u{2014} \u{21b6} Undo takes it back")
}

/// F3.1 (the rule after S5): the status when a press in the settled preview grabbed the zoom in force and moved it.
pub fn re_frame_status(t: f64) -> String {
    format!(
        "{} re-framed \u{2014} \u{21b6} Undo takes it back",
        label(t)
    )
}

/// F3.1 (the rule after S5): the plate drawn beside the box, as `spec/img/06-zoom.png` shows it: "zoom at 1:21 for 3.0s (1.0s in, 1.0s out)".
/// [`crate::fx_lane`] owns the sound-debt plate; keep the two separate.
pub fn zoom_plate(t: f64, dur: f64, trans: f64, tout: f64) -> String {
    format!(
        "{} for {:.1}s ({:.1}s in, {:.1}s out)",
        label(t),
        dur,
        trans,
        tout
    )
}

/// F3.1 (the rule after S5): what a press on the preview did — took the zoom in force, or started a new box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PressPreview {
    TakesZoom(usize),
    DrawsNew,
}

/// F3.1 (the rule after S5: `Nothing armed or held, paused preview, camera settled`): all three must hold, or a press is reaching for the playhead.
///
/// `box_under_pointer` is the pressed zoom's box, or `None` for a press clear of every box, which draws a new one.
pub fn preview_press(
    nothing_armed_or_held: bool,
    paused: bool,
    camera_settled: bool,
    box_under_pointer: Option<usize>,
) -> Option<PressPreview> {
    if !(nothing_armed_or_held && paused && camera_settled) {
        return None;
    }
    Some(match box_under_pointer {
        Some(index) => PressPreview::TakesZoom(index),
        None => PressPreview::DrawsNew,
    })
}

// --- the camera path --------------------------------------------------------------------------------------------

/// F3.1 (the paragraph after S5): where the camera is, a rect in source fractions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub cx: f64,
    pub cy: f64,
    pub hf: f64,
}

/// F3.1 (`from the centred full-fill slice`): the frame with no zoom on it; every glide starts here unless a staying zoom replaced it.
pub const SETTLED: Camera = Camera {
    cx: 0.5,
    cy: 0.5,
    hf: 1.0,
};

/// F3.1: the zooms in time order (§A.1's "walk zooms by t").
fn zooms_in_order(fx: &[Fx]) -> Vec<&Fx> {
    let mut zooms: Vec<&Fx> = fx
        .iter()
        .filter(|effect| effect.effect_kind() == Some(EffectKind::Zoom))
        .collect();
    zooms.sort_by(|a, b| a.t.total_cmp(&b.t));
    zooms
}

/// F3.1 (`Camera path`): where the camera is at the session second `t`.
///
/// §A.1's walk: start from [`SETTLED`]; each zoom in time order glides from the camera at its start to its rect across `trans`, holds,
/// then glides back across `tout` unless it stays, in which case its rect becomes the settled one. Cases in order: an arrival (inside `trans`)
/// outranks everything ("fade-in wins any overlap"; of two the earlier `t`, as [`crate::fx_lane::sound_answer_at`]); then the last zoom covering the second;
/// then the settled frame.
///
/// The ramps are §A.1's, as `fx_lane` draws them, read here as a fraction of each leg rather than an alpha.
pub fn camera_at(fx: &[Fx], t: f64) -> Camera {
    let zooms = zooms_in_order(fx);
    // The settled frame standing just before each zoom: earlier zooms have finished, and only a staying one left anything.
    let start_of = |index: usize| -> Camera {
        let mut frame = SETTLED;
        for effect in zooms[..index].iter().filter(|z| z.stay) {
            frame = rect_of(effect);
        }
        frame
    };

    // Fade-in wins any overlap, so an arrival is answered before anything else is considered.
    let arrival = (0..zooms.len())
        .filter(|&i| {
            let effect = zooms[i];
            effect.trans > 0.0 && t >= effect.t && t < effect.t + effect.trans
        })
        .min_by(|a, b| zooms[*a].t.total_cmp(&zooms[*b].t));
    if let Some(index) = arrival {
        let effect = *zooms
            .get(index)
            .expect("the filter just took it from this list");
        let share = (t - effect.t) / effect.trans;
        return mix(start_of(index), rect_of(effect), share);
    }

    // The last zoom covering this second decides what is held and what glides back.
    let covering = (0..zooms.len())
        .filter(|&i| {
            let (from, to) = zooms[i].spans();
            t >= from && t < to
        })
        .max_by(|a, b| zooms[*a].t.total_cmp(&zooms[*b].t));

    match covering {
        Some(index) => {
            let effect = *zooms
                .get(index)
                .expect("the filter just took it from this list");
            // A staying camera has no way back, so its fade-out is nought and this never runs for it.
            if !effect.stay && effect.tout > 0.0 && t > effect.spans().1 - effect.tout {
                let share = (effect.spans().1 - t) / effect.tout;
                return mix(rect_of(effect), start_of(index), share);
            }
            rect_of(effect)
        }
        // Nothing covers it: the settled frame, which a staying zoom may have replaced.
        None => {
            let mut frame = SETTLED;
            for effect in zooms.iter().filter(|z| z.stay && z.spans().1 <= t) {
                frame = rect_of(effect);
            }
            frame
        }
    }
}

/// F3.1: the frame a staying zoom leaves behind, published for F3.2 (does a staying zoom hold the whole frame?).
pub fn settled_after(fx: &[Fx], t: f64) -> Camera {
    let mut settled = SETTLED;
    for effect in zooms_in_order(fx) {
        if effect.stay && effect.spans().0 <= t {
            settled = rect_of(effect);
        }
    }
    settled
}

/// F3.1: a zoom's own rect, clamped as it is read (§A.1's clamp covers hand-edited files) and defaulting to the whole source for a missing fraction.
/// Public because `zoom_box_edit_on_release` reads a placed zoom's box through the same door as `camera_at`.
pub fn rect_of(effect: &Fx) -> Camera {
    let (hf, cx, cy) = clamp_rect(
        effect.hf.unwrap_or(SETTLED.hf),
        effect.cx.unwrap_or(SETTLED.cx),
        effect.cy.unwrap_or(SETTLED.cy),
    );
    Camera { cx, cy, hf }
}

/// F3.1: `share` of the way from one camera to another, each number on its own (one shared distance would arc).
fn mix(from: Camera, to: Camera, share: f64) -> Camera {
    let step = |a: f64, b: f64| a + (b - a) * share;
    let camera = Camera {
        cx: step(from.cx, to.cx),
        cy: step(from.cy, to.cy),
        hf: step(from.hf, to.hf),
    };
    // The clamp the rect was given applies to the path too: a mid-glide number is still a rect.
    let (hf, cx, cy) = clamp_rect(camera.hf, camera.cx, camera.cy);
    Camera { cx, cy, hf }
}
