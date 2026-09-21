//! §06-effects#2-the-lane-and-the-preview — `spec/06-effects.md` §2.
//!
//! The lane's colours, how its bars are packed into rows, and what the preview draws for each effect: all of it
//! is a rule about numbers, so it lives here as plain functions rather than inside a draw callback. The Cut page
//! is still drawn only for Prepare in `rust/src/ui/window.rs` (as with F2.1–F2.12), so there are no widgets of
//! this lane to compare yet; the three images §2 shows (`spec/img/06-lane.png`, `06-preview.png`,
//! `06-effect-menu.png`) were read here for the Effect menu's ORDER and for the colours, not for a widget diff.
//!
//! Two ramps in this tree look alike and must not be merged: [`crate::cut_hear::envelope`] is a volume effect's
//! GAIN over the seconds under it, while [`visibility_at`] below is an overlay's ALPHA.

use crate::cut::{EffectKind, Fx};
use crate::cut_screen;
use crate::fx_record;
use crate::timeline;

/// §06-effects#2-the-lane-and-the-preview — a lane colour: the three components cairo takes, 0..1 each.
pub type Rgb = (f64, f64, f64);

/// §06-effects#2-the-lane-and-the-preview (`Colours: zoom …, staying zoom …, speed/stop …`): a kind's bar
/// colour, verbatim from §2's list. Only a zoom has a second form — a reframing that stays is a different thing
/// to see, so it is a different colour rather than an outline or a badge.
pub fn bar_colour(kind: EffectKind, staying: bool) -> Rgb {
    match kind {
        // The staying zoom's orange against the normal cyan: same record, opposite intentions.
        EffectKind::Zoom if staying => (0.95, 0.62, 0.15),
        EffectKind::Zoom => (0.25, 0.72, 0.82),
        // inventory/effects.md §A draws this at alpha 0.4 — the bar is a wash over the lane, not a solid block.
        EffectKind::Speed => (0.92, 0.42, 0.6),
        EffectKind::Text => (0.6, 0.55, 0.95),
        EffectKind::Svg => (0.4, 0.8, 0.5),
        EffectKind::Volume => (0.95, 0.85, 0.2),
        // §2 says "grey-white" without a number; this near-white is ours — the point is that it separates from
        // the lane's dark ground, since a label changes nothing in the video and must not read as important.
        EffectKind::Label => (0.92, 0.92, 0.92),
    }
}

/// §06-effects#2-the-lane-and-the-preview: does this kind mean something different when it stays? Only a zoom:
/// every other kind's `stay` is never read (§06#1's table), so no other colour could change.
pub fn staying_is_a_different_effect(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Zoom)
}

// --- the Effect menu -----------------------------------------------------------------------------------------

/// §06-effects#2-the-lane-and-the-preview — the Effect dropdown's rows, top to bottom, as
/// `spec/img/06-effect-menu.png` shows them: Zoom, Text, SVG, Speed, Volume, Label. That is NOT
/// [`EffectKind`]'s declaration order (which groups speed with the time kinds), so the menu cannot be built by
/// iterating that enum.
pub const MENU_ORDER: [EffectKind; 6] = [
    EffectKind::Zoom,
    EffectKind::Text,
    EffectKind::Svg,
    EffectKind::Speed,
    EffectKind::Volume,
    EffectKind::Label,
];

// --- the lane's rows ------------------------------------------------------------------------------------------

/// §06-effects#2-the-lane-and-the-preview (`span floor 0.4 s`; `// effects.packMinSeconds` — §10 gives this no
/// `P.` id, and inventory/effects.md §F calls it fxPackMin): below this a bar is too narrow to be a row's owner,
/// so it is drawn on the lane without claiming a row of its own. Packing everything would make a 0.1 s label
/// push three real effects apart.
pub const PACK_MIN_SECONDS: f64 = 0.4;

/// §06-effects#2-the-lane-and-the-preview (`Rows: first-fit by seconds`): the row each effect is drawn on, in
/// the order the effects were given. An effect takes the first row nothing already on it overlaps; a new row
/// appears only when every existing one is busy at those seconds.
///
/// Packed by SECONDS and not by pixels, so the answer does not change when the timeline is zoomed: what fits
/// beside an effect at 4 px/s still fits at 240 px/s. [`timeline::rows_for`] is the sibling for recordings on
/// rows — same first-fit, different thing being packed (an effect lane has no pins and no camera rows).
pub fn rows_for_effects(fx: &[Fx]) -> Vec<usize> {
    // What each row is busy with, as the spans already placed on it.
    let mut taken: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut rows = Vec::with_capacity(fx.len());

    for effect in fx {
        let span = effect.spans();
        // Under the floor: drawn at the lane's top, reserving nothing.
        if span.1 - span.0 < PACK_MIN_SECONDS {
            rows.push(0);
            continue;
        }
        let free = taken.iter().position(|busy| {
            !busy
                .iter()
                .any(|held| timeline::spans_overlap(*held, span))
        });
        let row = free.unwrap_or_else(|| {
            taken.push(Vec::new());
            taken.len() - 1
        });
        taken[row].push(span);
        rows.push(row);
    }
    rows
}

/// §06-effects#2-the-lane-and-the-preview (`lane one row deep even when empty`): how many rows the lane shows.
/// Never nought — an empty lane still costs one row, because a lane that disappears when its last effect is
/// deleted moves every row under it, and a person reaches for a bar by where it was.
pub fn row_count(fx: &[Fx]) -> usize {
    // One pass over the rows this packing produced; `max(1)` is §2's "one row deep even when empty".
    let rows = rows_for_effects(fx);
    1 + rows.iter().copied().max().unwrap_or(0)
}

/// §06-effects#2-the-lane-and-the-preview: the lane's height. `// layout.effectRowPx`, read from
/// [`cut_screen::EFFECT_ROW_PX`] rather than repeated — the row height is the timeline's number and this only
/// multiplies it, so a zoom of the lane that changes one cannot leave the other behind.
pub fn lane_height_px(rows: usize) -> f64 {
    rows.max(1) as f64 * cut_screen::EFFECT_ROW_PX
}

// --- the preview while paused ---------------------------------------------------------------------------------

/// §06-effects#2-the-lane-and-the-preview (`outside the camera rect dimmed black 0.45`): how far the picture
/// outside the framed region is darkened — enough to read the framing, not enough to lose what is there.
pub const DIM_ALPHA: f64 = 0.45;

/// §06-effects#2-the-lane-and-the-preview (`rect stroked white with a plate`; inventory/effects.md §A.1 gives
/// the width): the framed region's outline, thin so it reads as a guide rather than as part of the picture.
pub const RECT_STROKE_PX: f64 = 1.5;

/// §06-effects#2-the-lane-and-the-preview (`the held one full with a dashed violet outline`): the outline's
/// colour. "Violet" is §2's word; these components are ours, in the family an audio insert is drawn in, so the
/// two "this is not footage yet" marks agree.
pub const HELD_OUTLINE: [f64; 3] = [0.62, 0.44, 0.86];

/// §06-effects#2-the-lane-and-the-preview (`visible overlays at real alpha`): how visible an effect is at the
/// session second `t` — nought outside its span, ramping in across `trans`, out across `tout`, full between.
/// Both fades live INSIDE `dur` (§06#1), so a long-enough fade simply never reaches full visibility.
///
/// This is an overlay's ALPHA; [`crate::cut_hear::envelope`] is a volume effect's GAIN. They look like the same
/// ramp and are not: one is what you see through, the other is how loud the bed is.
pub fn visibility_at(fx: &Fx, t: f64) -> f64 {
    let (from, to) = fx.spans();
    if t < from || t >= to {
        return 0.0;
    }
    // A kind with no fades — a label, per §06#1's table — is simply there for its seconds.
    let Some(kind) = fx.effect_kind() else {
        return 1.0;
    };
    if !fx_record::uses(kind, fx_record::Field::Ease) {
        return 1.0;
    }
    // A fade of nought either side is a hard cut: full from the first frame, gone after the last.
    if t < from + fx.trans {
        return (t - from) / fx.trans;
    }
    if t > to - fx.tout && fx.tout > 0.0 {
        return (to - t) / fx.tout;
    }
    1.0
}

/// §06-effects#2-the-lane-and-the-preview (`visible overlays at real alpha`): is this kind an overlay, i.e.
/// something the paused preview draws as it will be seen? A text, a drawing and a label are; a zoom's rect and
/// a rate's mask are guides rather than overlays (the first has its own line above, the second only matters when
/// something is playing), and a volume is heard — §2 lists no visual for it, so drawing one would be inventing.
pub fn drawn_paused(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Text | EffectKind::Svg | EffectKind::Label)
}

/// §06-effects#2-the-lane-and-the-preview (`the held one full`): the overlay in hand is shown at full alpha even
/// where its fades say half, because it is being judged, not watched.
pub fn held_drawn_full(held: bool) -> bool {
    held
}

/// §06-effects#2-the-lane-and-the-preview (`with a dashed violet outline`): and only the held one gets the
/// outline — an outline on everything would be no signal at all.
pub fn held_outline_dashed(held: bool) -> bool {
    held
}

/// §06-effects#2-the-lane-and-the-preview (`a box being drawn dashed`): a box still under the hand is dashed, so
/// an unfinished one cannot be mistaken for a placed one.
pub fn box_being_drawn_dashed(drawing: bool) -> bool {
    drawing
}

// --- the preview while playing ----------------------------------------------------------------------------------

/// §06-effects#2-the-lane-and-the-preview (`only the black mask over what the finished frame hides`): a zoom is
/// the only kind that contributes that mask while playing — its framing is why anything is hidden at all.
pub fn mask_only_while_playing(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Zoom)
}

/// §06-effects#2-the-lane-and-the-preview (`plus titles`): what survives of the paused overlay list while
/// playing — a caption and a drawing, since they are in the finished frame; the rest is either the mask above or
/// something with no visual at all.
pub fn drawn_while_playing(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Text | EffectKind::Svg)
}

/// §06-effects#2-the-lane-and-the-preview (`camera layer on the smoothed live clock`): the camera layer follows
/// [`crate::preview::live_clock`] rather than the playhead, which only moves once a tick — the mask and the
/// picture it covers would otherwise drift apart between ticks and the framing would shimmer.
pub fn camera_clock_is_smoothed() -> bool {
    true
}

/// §06-effects#2-the-lane-and-the-preview (`A stop's still: rendered from the scene's own camera, fitted on the
/// footage's transform`): a held frame borrows its framing from where it stands and never invents one, so it is
/// placed by the zoom that was already there. Only a stop does this — a rate of 1 is not a still.
pub fn still_come_from_scene_camera(fx: &Fx) -> bool {
    fx_record::is_stop(fx)
}

// --- preview and render, deliberately different --------------------------------------------------------------------

/// §06-effects#2-the-lane-and-the-preview (`preview runs speed at one flat rate`): [`crate::cut_hear::rate_under`]
/// answers the rate for the second under the line and nothing else. A rate change is a flushing seek, so following
/// a ramp would mean seeking per frame — the preview would spend its time seeking instead of playing.
pub fn previews_flat_rate() -> bool {
    true
}

/// §06-effects#2-the-lane-and-the-preview (`render follows ramps and averages overlaps`): the other half of the
/// difference, stated here so the two can never be "unified" by someone who found only one of them.
pub fn render_follows_ramps() -> bool {
    true
}

/// §06-effects#2-the-lane-and-the-preview (`the two 1× sound answers cannot be previewed`; inventory/effects.md
/// §A.3): `own` (1× to the effect's end) and `scene` (1× to the scene's end) both need the sound running at a
/// different clock from the picture, which is what a preview with one player cannot do. The empty answer,
/// `pitch` and `mute` all can be.
pub fn previewable_sound(snd: &str) -> bool {
    !matches!(snd, "own" | "scene")
}

/// §06-effects#2-the-lane-and-the-preview (`mute mutes the preview`, inventory/effects.md §A.3): of the five
/// answers this is the one the preview can honour exactly.
pub fn mute_silences_the_preview(snd: &str) -> bool {
    snd == "mute"
}

// --- the debt tail -------------------------------------------------------------------------------------------------

/// §06-effects#2-the-lane-and-the-preview (`tail suppressed under 0.05 s of debt`;
/// `// effects.debtTailMinSeconds` — no `P.` id in §10): a debt shorter than this is a rounding difference, and
/// a tail drawn for it would be longer than the thing it explains.
pub const DEBT_TAIL_MIN_SECONDS: f64 = 0.05;

/// §06-effects#2-the-lane-and-the-preview (`"sound X s behind|ahead" plate once wider than 60 px`;
/// `// effects.plateMinPx`): a plate narrower than the words in it is an unreadable smudge, so the number waits
/// until there is room. inventory/effects.md §A.3 writes this bound as "≥ 60 px"; §2's own wording is "once
/// wider than", which is what [`plate_shown`] implements — the two differ at exactly 60.0 and nowhere else.
pub const PLATE_MIN_PX: f64 = 60.0;

/// §06-effects#2-the-lane-and-the-preview (`Debt = dur − Σ on-screen seconds (positive: sound behind)`): how far
/// the sound is out of step with the picture. Positive means the sound still has seconds left to play after the
/// picture moved on — it is BEHIND.
pub fn debt(dur: f64, on_screen: f64) -> f64 {
    dur - on_screen
}

/// §06-effects#2-the-lane-and-the-preview (`Only "1× to the scene's end" draws the dashed debt tail ("1× to the
/// effect's end" closes its gap on the effect's last frame)`): a tail is drawn for `scene` only, only when the
/// debt is worth a line, and only while the scene actually runs on — a tail reaching past the cut would point at
/// seconds that are not in the video.
pub fn draws_debt_tail(snd: &str, debt: f64, scene_runs_on: bool) -> bool {
    snd == "scene" && debt.abs() >= DEBT_TAIL_MIN_SECONDS && scene_runs_on
}

/// §06-effects#2-the-lane-and-the-preview (`"sound X s behind|ahead"`): the plate's words. One tenth of a second
/// is the finest thing worth telling someone to wait for; an exact zero has nothing to say, and a plate reading
/// "sound 0.0 s" would be a bug dressed as information.
pub fn plate_text(debt: f64) -> String {
    if debt == 0.0 {
        return String::new();
    }
    let seconds = debt.abs();
    let side = if debt > 0.0 { "behind" } else { "ahead" };
    format!("sound {seconds:.1} s {side}")
}

/// §06-effects#2-the-lane-and-the-preview (`plate once wider than 60 px`): is the tail wide enough for its plate?
pub fn plate_shown(width_px: f64) -> bool {
    width_px > PLATE_MIN_PX
}

/// §06-effects#2-the-lane-and-the-preview (`The earliest covering effect's answer wins`, inventory/effects.md
/// §A.3): what the speed under `t` does with its sound — the first effect that both covers the second and named
/// an answer, in the order they were placed. An empty `snd` is no answer and does not silence a later one; a
/// non-speed says nothing about sound at all (§06#1's table gives it no `snd`).
pub fn sound_answer_at(fx: &[Fx], t: f64) -> Option<String> {
    fx.iter()
        .filter(|effect| effect.effect_kind() == Some(EffectKind::Speed))
        .filter(|effect| !effect.snd.is_empty())
        .filter(|effect| {
            let (from, to) = effect.spans();
            t >= from && t < to
        })
        .min_by(|a, b| a.t.total_cmp(&b.t))
        .map(|effect| effect.snd.clone())
}

/// §06-effects#2-the-lane-and-the-preview (`sndDip 0.15 s fades at the rejoin`, inventory/effects.md §A.3): the
/// fade in and back out where delayed sound rejoins the picture — short enough not to hear, long enough that the
/// rejoin does not click. `// P.eng.soundDipSeconds`, whose row lives in spec/10-parameters.md; this is the
/// effects side of it, so the number sits with the rule that fades.
pub const SOUND_DIP_SECONDS: f64 = 0.15;

/// §06-effects#2-the-lane-and-the-preview: the rejoin's dip, read as a function so a caller cannot pick its own.
pub fn rejoin_dip() -> f64 {
    SOUND_DIP_SECONDS
}
