//! F3.1 Zoom by hand — `spec/06-effects.md` F3.1, steps S1–S5 and the camera path after them.
//!
//! ✚ Effect ▾ → ⊕ Zoom arms a drag; a box drawn on the preview becomes a zoom at the red line (or over the
//! marked stretch) with its defaults; the form that opens is where its seconds and its ending are settled. Every
//! one of those is arithmetic or a string, so all of it lives here and the widget layer will only forward presses.
//!
//! `rust/src/ui/window.rs` renders only the Prepare page, so there is no Cut page to compare with
//! `spec/img/06-zoom.png`: that image was read for the form's field ORDER and for the preview plate's wording,
//! and `spec/inventory/effects.md` §A.1 for the arm text and the camera path. Nothing here draws anything.

use crate::cut::{Cut, EffectKind, Fx};
use crate::fx_record;
use crate::tools;

// --- S1: arming -----------------------------------------------------------------------------------------------

/// F3.1 S1 (`a line?` → no): ✚ Zoom's own refusal. It is a different sentence from
/// [`crate::cut_copy::NO_LINE_YET`] (⧉ Paste), [`crate::cut_insert::NO_LINE_YET`] (⧉ Insert) and
/// [`crate::cut_copy::NO_LANE_LINE_YET`] (⇲ Lane) — four buttons asking for the same missing thing in the words of
/// what each one does.
pub const NO_LINE: &str = "click a track first \u{2014} the effect needs a moment to happen at";


/// F3.1 S1: what pressing ⊕ Zoom did.
#[derive(Debug, Clone, PartialEq)]
pub enum Press {
    Armed,
    Disarmed,
    Refused(&'static str),
}

/// F3.1 S1 (`a line?` … `same entry again disarms`, spec/inventory/effects.md §A.1): the dropdown is a toggle, so
/// arming an arm that is already out takes it back rather than arming a second one — an effect can only be drawn
/// once, and a menu that armed twice would leave someone looking for the second box.
pub fn arm(line: Option<f64>, already_armed: bool) -> Press {
    let Some(_) = line else {
        return Press::Refused(NO_LINE);
    };
    if already_armed {
        return Press::Disarmed;
    }
    Press::Armed
}

/// F3.1 S1 (`Status/panel text`): the arm's words, up to and including the shape clause — spec/inventory/effects.md
/// §A.1 spells them, and the trailing space is the join with [`arm_tail`].
///
/// **The contradiction, and what this tree does.** These words promise two things F3.1 S2 refuses: "The box keeps
/// the cut's shape" and "let go near the full width or height to snap to it". S2 says the gesture is a free
/// rectangle and that "neither happens", and `spec/img/06-zoom.png`'s caption repeats it ("A free rectangle, not
/// the cut's shape the prototype's words promised"). The clause stays in the string because F3.1 S1 quotes this
/// panel text as the thing to show — this tree pins quoted UI strings whole — while the BEHAVIOUR follows S2: see
/// [`free_rectangle`], [`keeps_the_cut_shape`] and [`snaps_to_full_size`]. One comment, so neither half gets
/// "fixed" into matching the other.
pub const ARM_WORDS: &str = "Drag a box on the video: the picture zooms there and comes back out on its own, \
                             or stays on it \u{2014} the form that opens is where that is said. The box keeps \
                             the cut's shape; let go near the full width or height to snap to it. ";

/// F3.1 S1: the second half of the panel text — where the zoom will start, and for how long until the form says
/// otherwise. The seconds it names are [`DEFAULT_SECONDS`], so the sentence cannot drift from the default it
/// describes. The red line is named rather than printed: spec/inventory/effects.md §A.1 spells this half with no
/// stamp, and F3.1 S1 quotes it the same way ("It starts at the red line and runs 3 s…").
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

/// F3.1 S1 (`Camera layer goes down; whole source visible`): while a box is being drawn the framing has to be
/// out of the way, or someone draws a box over a crop and gets a crop of a crop.
pub fn whole_source_shown(armed: bool) -> bool {
    armed
}

/// F3.1 S1 (`same entry again disarms`; Esc too — spec/inventory/cut.md §E's key list drops "arm"): whether the
/// arm was there to lose, so the page only says it was cancelled when something was cancelled.
pub fn disarm(armed: bool) -> bool {
    armed
}

// --- S2: the drag ---------------------------------------------------------------------------------------------

/// F3.1 S2 (`Drag a box on the preview (< 12 px ignored)`; `// effects.dragMinPx` — §10 gives this no `P.` id and
/// spec/inventory/effects.md §F lists it as "tiny drag 12"): below this a press is a click, not a box.
pub const DRAG_MIN_PX: f64 = 12.0;

/// F3.1 S2: did the drag describe a box? Measured on both axes, because a 200 × 4 px swipe across the picture is
/// someone flicking at the preview and not a framing.
pub fn drag_counts(width_px: f64, height_px: f64) -> bool {
    width_px >= DRAG_MIN_PX && height_px >= DRAG_MIN_PX
}

/// F3.1 S2: a rectangle in preview pixels — the box as drawn, before it becomes fractions of the source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// F3.1 S2 (`the gesture is a free rectangle`): the box from press to release. Either drag direction works, and
/// nothing else is done to it — no aspect lock, no snap to full width or height (see [`ARM_WORDS`] for why the
/// panel text says otherwise). The prototype took this same free-rectangle path for every drawing drag: zoom,
/// text and svg alike.
pub fn free_rectangle(from: (f64, f64), to: (f64, f64)) -> Option<Rect> {
    let (x0, y0) = (from.0.min(to.0), from.1.min(to.1));
    let (w, h) = ((from.0 - to.0).abs(), (from.1 - to.1).abs());
    if !drag_counts(w, h) {
        return None;
    }
    Some(Rect { x: x0, y: y0, w, h })
}

/// F3.1 S2: the release does not snap a near-full box to full. `false` is the behaviour; it is a function so the
/// answer can be asserted instead of argued with the panel text.
pub fn snaps_to_full_size() -> bool {
    false
}

/// F3.1 S2: and the box does not inherit the cut's aspect either — a free rectangle keeps whatever shape was drawn.
pub fn keeps_the_cut_shape() -> bool {
    false
}

/// F3.1 S2 → §06#1 (`cx, cy, hf … fractions of the SOURCE frame (width follows the aspect)`): the box as the
/// record stores it. Height is the fraction that matters and width follows the source's own aspect, so a wide box
/// on a tall source is not a wide zoom.
///
/// A box may hang off the source — someone drags past an edge — and `hf` then exceeds 1, which is allowed: §A.1
/// says the rect is normalised against the source, may exceed the frame, and the render pads black.
pub fn to_fractions(rect: Rect, source_w: f64, source_h: f64) -> (f64, f64, f64) {
    let (cx, cy) = (
        (rect.x + rect.w / 2.0) / source_w,
        (rect.y + rect.h / 2.0) / source_h,
    );
    (cx, cy, rect.h / source_h)
}

/// F3.1 S2 (`Rect clamp hf ∈ [0.02, 12]`; `// effects.rectHfMin` — §A.1's number, no §10 row): the smallest
/// region worth a zoom. Below it the camera is inside one pixel and the render has nothing to show.
pub const HF_MIN: f64 = 0.02;

/// F3.1 S2 (`hf ∈ [0.02, 12]`; `// effects.rectHfMax`): and the largest — past twelve source-heights the frame is
/// a black border around a video.
pub const HF_MAX: f64 = 12.0;

/// F3.1 S2 (`centre ∈ [−2, 3]`; `// effects.rectCentreMin`): how far off-frame a zoom may be pointed before it is
/// taken as a mistake.
pub const CENTRE_MIN: f64 = -2.0;

/// F3.1 S2 (`centre ∈ [−2, 3]`; `// effects.rectCentreMax`).
pub const CENTRE_MAX: f64 = 3.0;

/// F3.1 S2: the clamp §A.1 gives a rect, applied on the way in so no later step has to guard against it. Only the
/// three numbers a zoom stores are clamped — a box's own pixels are what was drawn.
pub fn clamp_rect(hf: f64, cx: f64, cy: f64) -> (f64, f64, f64) {
    (
        hf.clamp(HF_MIN, HF_MAX),
        cx.clamp(CENTRE_MIN, CENTRE_MAX),
        cy.clamp(CENTRE_MIN, CENTRE_MAX),
    )
}

// --- S3: the defaults ------------------------------------------------------------------------------------------

/// F3.1 S3 (`glide 1 s in and out`; `// P.policy.effectDefaultFades`, whose row reads zoom 1): a camera that
/// arrives without gliding is a jump cut, so the default moves.
pub const GLIDE_SECONDS: f64 = 1.0;

/// F3.1 S3 (`3 s, or the marked stretch`; `// P.policy.effectDefaultSeconds`, whose row reads zoom/text/svg 3):
/// long enough to read a region and come back off it.
pub const DEFAULT_SECONDS: f64 = 3.0;

/// F3.1 S3 (`stay when an aspect is set`): has this cut been given a shape? An empty `aspect` IS the source's own
/// shape, i.e. no shape chosen.
///
/// [`crate::cut_screen::ASPECT_DEFAULT`] must NOT be consulted here: its `"16:9"` is what the readout prints for
/// an unset aspect, not an aspect anyone chose, and reading it would make every project start out wanting a
/// staying zoom.
pub fn aspect_is_set(cut: &Cut) -> bool {
    !cut.aspect.is_empty()
}

/// F3.1 S3 (`and no staying zoom exists yet`): is the frame already settled by a zoom that stays? Asked through the
/// typed kind, so a text with `stay` left in it from an older file says nothing about the camera.
pub fn a_staying_zoom_exists(fx: &[Fx]) -> bool {
    fx.iter()
        .any(|effect| effect.effect_kind() == Some(EffectKind::Zoom) && effect.stay)
}

/// F3.1 S3 (`stay when an aspect is set and no staying zoom exists yet`): the first zoom into a shaped video
/// reframes it, because a 9:16 video whose camera still shows the whole wide frame has bars nobody asked for; once
/// one staying zoom exists the next one should be a passing close-up instead.
pub fn stays_by_default(aspect_set: bool, staying_exists: bool) -> bool {
    aspect_set && !staying_exists
}

/// F3.1 S3 (`then no glides`): a staying camera has nowhere to glide back to, so its fades are nought — and the
/// form greys the fade-out field for exactly that reason.
pub fn fades(stay: bool) -> (f64, f64) {
    if stay {
        return (0.0, 0.0);
    }
    (GLIDE_SECONDS, GLIDE_SECONDS)
}

/// F3.1 S3 (`length 3 s (or the marked stretch)`): where the zoom goes and for how long — a marked stretch is what
/// the person pointed at, so it outranks both the line and the default length.
pub fn place(marked: Option<(f64, f64)>, line: f64) -> (f64, f64) {
    match marked {
        Some((from, to)) => (from, to - from),
        None => (line, DEFAULT_SECONDS),
    }
}

/// F3.1 S3: the record a finished drag creates — the box as clamped fractions, the defaults for everything the
/// form has not been shown yet, and the row the zoom was drawn on (§06#1's `cam`).
///
/// `ease` stays [`fx_record::LINEAR_EASE`], which writes no key: a zoom placed by hand is linear, and linear is
/// empty so an old file comes back byte-identical. `wf` stays `None` — §06#1's table gives a zoom no width field.
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

/// F3.1 S4 (`Form "Zoom at m:ss"`): the window's title, named for the second it belongs to — a zoom is placed at
/// the red line and its form must say which line it has.
pub fn form_title(t: f64) -> String {
    format!("Zoom at {}", tools::mm_ss(t))
}

/// F3.1 S4: the form's fields, in the order `spec/img/06-zoom.png` shows them — Length, then the ending, then the
/// two fades, then the curve. Order is the sentence a person reads, not a layout detail.
pub const FORM_FIELDS: [&str; 5] = ["Length (s)", "At the end", "Fade in (s)", "Fade out (s)", "Curve"];

/// F3.1 S4 (`At the end: Pull back / Stay on it`): the two endings, and they are a radio pair — one zoom, one
/// ending. `end_choices` puts the chosen one first so the widget has nothing to decide.
pub const PULL_BACK: &str = "Pull back";
pub const STAY_ON_IT: &str = "Stay on it";

pub fn end_choices(stay: bool) -> [&'static str; 2] {
    if stay {
        [STAY_ON_IT, PULL_BACK]
    } else {
        [PULL_BACK, STAY_ON_IT]
    }
}

/// F3.1 S4 (`Fade out (s) (greyed when staying: "A camera that stays has no way back, so no fade out.")`): the
/// greying and its reason are one rule — a field is greyed only because there is a sentence saying why.
pub fn fade_out_greyed(stay: bool) -> bool {
    stay
}

/// F3.1 S4: that sentence, verbatim (spec/inventory/effects.md §A.1).
pub const NO_WAY_BACK: &str = "A camera that stays has no way back, so no fade out.";

/// F3.1 S4 (`Curve (Linear)`): the shapes on offer. One, because one is implemented — §A.1's own help text says
/// "Straight is all there is so far…", and a menu of curves that all do the same thing would be a lie about the
/// renderer.
pub const CURVE_CHOICES: [&str; 1] = ["Linear"];

/// F3.1 S4: what the curve stores. "Linear" is [`fx_record::LINEAR_EASE`] — empty, so a hand-placed zoom writes no
/// `ease` key and an old file stays byte-identical (§06#1). A name from a newer build is kept untouched rather
/// than flattened to linear by a build that cannot draw it yet.
pub fn curve_stored(name: &str) -> &str {
    if name == CURVE_CHOICES[0] {
        return fx_record::LINEAR_EASE;
    }
    name
}

// The field help, verbatim from spec/inventory/effects.md §A.1 — each is the tooltip its field shows, so they are
// one string per field rather than a table that can drift a row.
/// F3.1 S4: Length's help.
pub const LENGTH_HELP: &str = "how long the camera move lasts altogether, fades included";
/// F3.1 S4: Pull back's help.
pub const PULL_BACK_HELP: &str = "A passing close-up: the picture closes in, holds for its seconds and opens back \
                                  out on its own, leaving the rest of the video framed as it was.";
/// F3.1 S4: Stay on it's help.
pub const STAY_ON_IT_HELP: &str = "A reframing: from here on the finished video shows this region. This is how a \
                                   vertical short is made out of widescreen footage \u{2014} say where the action \
                                   is, and say it again when it moves.";
/// F3.1 S4: Fade in's help.
pub const FADE_IN_HELP: &str = concat!(
    "how long the camera takes to arrive: 0 cuts straight to the region, ",
    "1 glides over a second"
);
/// F3.1 S4: Fade out's help — replaced by [`NO_WAY_BACK`] once staying is chosen.
pub const FADE_OUT_HELP: &str = "how long it takes to come back off the region again: 0 cuts straight back";
/// F3.1 S4: Curve's help.
pub const CURVE_HELP: &str = "the shape both fades travel in. Straight is all there is so far\u{2026}";

/// F3.1 S4: the form's footer, read off `spec/img/06-zoom.png` (image-derived — §A.1 does not spell it). It says
/// what the form does while it is open: every keystroke is kept, and one ↶ takes the whole edit back.
pub const FORM_FOOTER: &str = "Kept as you type \u{2014} \u{21b6} Undo takes the whole edit back.";

/// F3.1 S2 → S3: the form a finished drag opens, carrying every default §S3 gives rather than a blank sheet.
///
/// The box arrives in picture pixels and leaves as source fractions through [`to_fractions`], clamped by
/// [`clamp_rect`] — no number is composed here. `at` is the line the arm was holding (S1 refuses without one), and
/// the length comes from [`place`], so a marked stretch outranks both the line and [`DEFAULT_SECONDS`] exactly as
/// §F3.1 S3 says. `stay` is [`stays_by_default`] read against this cut's own list, and the fades follow from
/// [`fades`] — which answers nought for a staying camera, so the form's fade-out field greys itself rather than
/// being told to.
///
/// `marked` is handed in because nothing in the window knows the marked stretch yet (no live marking seam exists in
/// `src/ui/window.rs` today); the UI passes `None`, and the marked branch is proven logic-side only until F1.x's
/// marking reaches the Cut page.
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

/// F3.1 S4 (`Apply: dur ≥ 0.4`): `// effects.zoomFloorSeconds` — §06's own §6 lists this as a form floor ("zoom
/// 0.4") with no `P.` row in §10. [`crate::cut_insert::seconds_accepts`] is the sibling for an insert, at that
/// flow's own second-long floor; the two floors differ because a camera move under 0.4 s is not a move.
pub const MIN_SECONDS: f64 = 0.4;

/// F3.1 S4: what the form holds when Apply is pressed — the seconds as typed, the ending as chosen, and the box
/// the drag left behind (in source fractions, already clamped).
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
/// Below [`MIN_SECONDS`] it refuses, naming the floor. §F3.1 gives no words for this refusal — a form that greys
/// Apply below the floor should never reach here — so this sentence is ours, in the shape this tree's other
/// refusals use: what is wrong, then the bound.
///
/// The fades are then settled the way §A.1 says and the prototype's `clampFades` (gui/cut_fx.go) does it: staying
/// means no glides at all; otherwise neither fade may be negative, and when the two together overrun the length
/// they share it in proportion rather than one of them losing everything — a move that arrives in 0.8 s out of a
/// 1.2 s glide still arrives smoothly.
pub fn apply(form: &Form) -> Result<Fx, String> {
    if form.dur < MIN_SECONDS {
        return Err(format!(
            "a camera move of {:.2} s is shorter than the shortest one this form takes ({MIN_SECONDS} s)",
            form.dur
        ));
    }
    let mut effect = new_zoom((form.cx, form.cy, form.hf), form.at, form.dur, form.stay, form.row);
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

/// F3.1 S5 (`"<label> — ↶ Undo takes it back"`): the `<label>` §F3.1 leaves open. Three labels exist for one zoom
/// and they are not interchangeable: §A.1's lane bar reads the bare `X.Xs`, the preview plate (see [`zoom_plate`])
/// is a whole sentence, and this is the subject of the status line — what was done, in three words.
pub fn label(t: f64) -> String {
    format!("zoom at {}", tools::mm_ss(t))
}

/// F3.1 S5: the status after a zoom is placed. A staying zoom says what it changes, because "zoom at 0:30" does not
/// tell someone that every second after 0:30 is now framed differently. Both shapes are §A.1's; §F3.1 S5 abbreviates
/// the second one with "…".
pub fn placed_status(t: f64, stay: bool) -> String {
    let name = label(t);
    if stay {
        return format!("{name} \u{2014} the video shows this region from here on; \u{21b6} Undo takes it back");
    }
    format!("{name} \u{2014} \u{21b6} Undo takes it back")
}

/// F3.1 (the rule after S5): the status when a press in the settled preview grabbed the zoom in force and moved it.
/// Same subject, one more word, because what happened is different: nothing was placed, an existing framing moved.
pub fn re_frame_status(t: f64) -> String {
    format!("{} re-framed \u{2014} \u{21b6} Undo takes it back", label(t))
}

/// F3.1 (the rule after S5): the plate drawn beside the box on the preview, as `spec/img/06-zoom.png` shows it —
/// "zoom at 1:21 for 3.0s (1.0s in, 1.0s out)". This is the ZOOM's plate; [`crate::fx_lane`] owns the sound-debt
/// plate ("sound X s behind"), and the two must not be merged into one generic plate function — they name
/// different things and appear on different effects.
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

/// F3.1 (the rule after S5: `Nothing armed or held, paused preview, camera settled`): all three have to hold before
/// a press on the picture means anything about framing. On a moving or playing preview the same press is someone
/// reaching for the playhead, and answering it by re-framing would be the worst possible guess.
///
/// `box_under_pointer` is which zoom's box was pressed, or `None` when the press fell clear of every box — which
/// draws a new one rather than taking a zoom nobody touched.
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

/// F3.1 (the paragraph after S5): where the camera is — a rect in source fractions, the same three numbers a zoom
/// stores.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub cx: f64,
    pub cy: f64,
    pub hf: f64,
}

/// F3.1 (`from the centred full-fill slice`): the frame with no zoom on it — the whole source, centred. Every glide
/// starts here unless a staying zoom has replaced it.
pub const SETTLED: Camera = Camera { cx: 0.5, cy: 0.5, hf: 1.0 };

/// F3.1: the zooms, in the order §A.1 walks them — by `t`, since "walk zooms by t" is what makes a lane out of
/// effects stored in any order.
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
/// §A.1's walk, in order: start from [`SETTLED`]; for each zoom in time order, glide from wherever the camera was
/// at that zoom's own start to its rect across `trans`, hold while it is fully visible, and — unless it stays —
/// glide back to the settled rect across `tout`; a staying zoom becomes the new settled rect. Nothing reaches
/// backwards, so a second before the first zoom is simply the whole source. The three cases fall out of that walk
/// in this order: an arrival (inside `trans`) outranks everything — "fade-in wins any overlap", and of two
/// arrivals the earlier `t` decides, the same earliest-answer-wins rule as [`crate::fx_lane::sound_answer_at`],
/// because a camera cannot be arriving twice; then the last zoom covering the second, held or gliding back out
/// across `tout`; then nothing, which is the settled frame.
///
/// The ramps themselves are the ones §A.1 describes and [`fx_lane`] draws the overlay from — same `trans`/`tout`
/// inside `dur`, read here as a fraction of each leg rather than as an alpha.
pub fn camera_at(fx: &[Fx], t: f64) -> Camera {
    let zooms = zooms_in_order(fx);
    // Where each zoom starts, as the settled frame standing just before it: every earlier zoom has finished by
    // then (nothing glides into a second that is not its own), and only a staying one left anything behind.
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
        let effect = *zooms.get(index).expect("the filter just took it from this list");
        let share = (t - effect.t) / effect.trans;
        return mix(start_of(index), rect_of(effect), share);
    }

    // The last zoom covering this second decides what is held and what glides back — the walk has already
    // passed through anything that began earlier.
    let covering = (0..zooms.len())
        .filter(|&i| {
            let (from, to) = zooms[i].spans();
            t >= from && t < to
        })
        .max_by(|a, b| zooms[*a].t.total_cmp(&zooms[*b].t));

    match covering {
        Some(index) => {
            let effect = *zooms.get(index).expect("the filter just took it from this list");
            // Gliding back out unless it stays — a staying camera has no way back, which is why its fade-out is
            // nought and this branch never runs for it.
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

/// F3.1: the frame a staying zoom leaves behind — [`camera_at`]'s own settled state, published for the next round's
/// reader (F3.2 asks whether a staying zoom holds the whole frame) and for anyone who wants the settled framing
/// without asking what is visible at a second.
pub fn settled_after(fx: &[Fx], t: f64) -> Camera {
    let mut settled = SETTLED;
    for effect in zooms_in_order(fx) {
        if effect.stay && effect.spans().0 <= t {
            settled = rect_of(effect);
        }
    }
    settled
}

/// F3.1: a zoom's own rect, clamped as it is read (§A.1's clamp covers a hand-edited file as well as a fresh
/// drag) and falling back to the whole source where a fraction was left out.
fn rect_of(effect: &Fx) -> Camera {
    let (hf, cx, cy) = clamp_rect(
        effect.hf.unwrap_or(SETTLED.hf),
        effect.cx.unwrap_or(SETTLED.cx),
        effect.cy.unwrap_or(SETTLED.cy),
    );
    Camera { cx, cy, hf }
}

/// F3.1: `share` of the way from one camera to another, each number travelling on its own — a glide that moved x, y
/// and height by one shared distance would arc instead of arriving.
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
