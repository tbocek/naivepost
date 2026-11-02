//! §06-effects#2-the-lane-and-the-preview — `spec/06-effects.md` §2.

use crate::cut::{EffectKind, Fx};
use crate::cut_screen;
use crate::fx_record;
use crate::timeline;

/// §06-effects#2-the-lane-and-the-preview — a lane colour: the three components cairo takes, 0..1 each.
pub type Rgb = (f64, f64, f64);

/// §06-effects#2-the-lane-and-the-preview (`Colours: zoom …, staying zoom …, speed/stop …`): a kind's bar
/// colour, verbatim from §2's list.
pub fn bar_colour(kind: EffectKind, staying: bool) -> Rgb {
    match kind {
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

/// §06-effects#2-the-lane-and-the-preview: does this kind mean something different when it stays?
pub fn staying_is_a_different_effect(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Zoom)
}

/// §06-effects#2-the-lane-and-the-preview (`// effects.washAlpha` — §10 gives this no `P.` id): the alpha a bar's
/// colour is laid down at.
pub const WASH_ALPHA: f64 = 0.4;

/// §06-effects#2-the-lane-and-the-preview (`// effects.labelTagAlpha`, inventory/effects.md §A.7: "grey-white tag
/// (fill 0.22, tick, dashed line)"): a label changes nothing in the video, so its bar is paler than every real
/// effect's — a solid block would read as though something happened there.
pub const LABEL_TAG_ALPHA: f64 = 0.22;

/// The alpha this kind's bar is washed on: the label's paler tag, everyone else's ordinary wash.
pub fn bar_wash_alpha(kind: EffectKind) -> f64 {
    match kind {
        EffectKind::Label => LABEL_TAG_ALPHA,
        _ => WASH_ALPHA,
    }
}

// --- the Effect menu -----------------------------------------------------------------------------------------

/// §06-effects#2-the-lane-and-the-preview — the Effect dropdown's rows, top to bottom, as
/// `spec/img/06-effect-menu.png` shows them: Zoom, Text, SVG, Speed, Volume, Label.
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
/// so it is drawn on the lane without claiming a row of its own.
pub const PACK_MIN_SECONDS: f64 = 0.4;

/// One effect's place in the lane: the row it is drawn on, and whether it claims that row's seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Packed {
    pub row: usize,
    pub reserves_a_row: bool,
}

/// §06-effects#2-the-lane-and-the-preview (`Rows: first-fit by seconds`, `span floor 0.4 s`): the lane's
/// packing, one entry per effect, in the order given.
pub fn pack(fx: &[Fx]) -> Vec<Packed> {
    let mut taken: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut packed = Vec::with_capacity(fx.len());

    for effect in fx {
        if effect.dur < PACK_MIN_SECONDS {
            packed.push(Packed {
                row: 0,
                reserves_a_row: false,
            });
            continue;
        }
        let span = effect.spans();
        let free = taken
            .iter()
            .position(|busy| !busy.iter().any(|held| timeline::spans_overlap(*held, span)));
        let row = free.unwrap_or_else(|| {
            taken.push(Vec::new());
            taken.len() - 1
        });
        taken[row].push(span);
        packed.push(Packed {
            row,
            reserves_a_row: true,
        });
    }
    packed
}

/// §06-effects#2-the-lane-and-the-preview (`Rows: first-fit by seconds`): the row each effect is drawn on, in
/// the order the effects were given — [`pack`]'s rows, kept as its own name because the band's painters and
/// press readers (`cut_tracks::rows_of`, `cut_line_press`, `cut_select_press`) ask for just this.
pub fn rows_for_effects(fx: &[Fx]) -> Vec<usize> {
    pack(fx).iter().map(|placed| placed.row).collect()
}

/// §06-effects#2-the-lane-and-the-preview (`lane one row deep even when empty`): how many rows the lane shows.
pub fn row_count(fx: &[Fx]) -> usize {
    // One pass over the rows this packing produced; `max(1)` is §2's "one row deep even when empty".
    let rows = rows_for_effects(fx);
    1 + rows.iter().copied().max().unwrap_or(0)
}

/// §06-effects#2-the-lane-and-the-preview: the lane's height.
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
/// colour.
pub const HELD_OUTLINE: [f64; 3] = [0.62, 0.44, 0.86];

/// §06-effects#2-the-lane-and-the-preview (`visible overlays at real alpha`): how visible an effect is at the
/// session second `t` — nought outside its span, ramping in across `trans`, out across `tout`, full between.
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
/// something the paused preview draws as it will be seen?
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

// --- preview and render, deliberately different --------------------------------------------------------------------

/// §06-effects#2-the-lane-and-the-preview (`the two 1× sound answers cannot be previewed`; inventory/effects.md
/// §A.3): `own` (1× to the effect's end) and `scene` (1× to the scene's end) both need the sound running at a
/// different clock from the picture, which is what a preview with one player cannot do.
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
/// until there is room.
pub const PLATE_MIN_PX: f64 = 60.0;

/// §06-effects#2-the-lane-and-the-preview (`Debt = dur − Σ on-screen seconds (positive: sound behind)`): how far
/// the sound is out of step with the picture.
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

// --- the two surfaces as values ------------------------------------------------------------------------------
//
// The lane and the preview each need several of the rules above at once. Composing them here keeps the view down
// to one call per surface, so no colour, alpha, row or threshold can be re-derived inside a draw callback --
// which is where a rule goes wrong unnoticed, because nothing outside the callback can see it.

/// One bar in the lane: which effect it is, which row it got, and the colour it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    /// Index into the effect list, so the view can name the widget `fx-bar-<index>` and read the record back.
    pub index: usize,
    pub row: usize,
    pub kind: EffectKind,
    /// A staying zoom is a different bar (§2 gives it its own colour).
    pub staying: bool,
    pub colour: Rgb,
    /// Whether this bar claims its row for its seconds.
    pub reserves_a_row: bool,
}

/// §06-effects#2-the-lane-and-the-preview: the lane as one value.
pub fn lane_layout(fx: &[Fx]) -> Vec<Bar> {
    let packed = pack(fx);
    fx.iter()
        .zip(packed)
        .enumerate()
        .filter_map(|(index, (effect, placed))| {
            let kind = effect.effect_kind()?;
            // Staying is a zoom's question: §06#1's table gives no other kind a `stay` field, so a stray
            // `stay: true` on a caption cannot make it the orange bar.
            let staying = fx_record::uses(kind, fx_record::Field::Stay) && effect.stay;
            Some(Bar {
                index,
                row: placed.row,
                kind,
                staying,
                colour: bar_colour(kind, staying),
                reserves_a_row: placed.reserves_a_row,
            })
        })
        .collect()
}

/// What the paused preview shows for one frame: the dim outside the camera rect, the rect's stroke, and every
/// overlay with the alpha its fades give it right now.
#[derive(Debug, Clone, PartialEq)]
pub struct PausedScene {
    pub dim_alpha: f64,
    pub rect_stroke_px: f64,
    /// `(index, kind, alpha)` for each overlay the paused frame draws.
    pub overlays: Vec<(usize, EffectKind, f64)>,
    pub held_full: bool,
    pub held_outline_dashed: bool,
    pub box_dashed: bool,
}

/// §06-effects#2-the-lane-and-the-preview: the paused frame.
pub fn paused_scene(
    fx: &[Fx],
    t: f64,
    held_index: Option<usize>,
    drawing_box: bool,
) -> PausedScene {
    let overlays = fx
        .iter()
        .enumerate()
        .filter(|(index, effect)| {
            Some(*index) != held_index && effect.effect_kind().is_some_and(drawn_paused)
        })
        .map(|(index, effect)| {
            (
                index,
                effect.effect_kind().expect("filtered above"),
                visibility_at(effect, t),
            )
        })
        .collect();
    PausedScene {
        dim_alpha: DIM_ALPHA,
        rect_stroke_px: RECT_STROKE_PX,
        overlays,
        held_full: held_drawn_full(held_index.is_some()),
        held_outline_dashed: held_outline_dashed(held_index.is_some()),
        box_dashed: box_being_drawn_dashed(drawing_box),
    }
}

/// §06-effects#2-the-lane-and-the-preview: the playing frame — titles only, at their real alpha.
pub fn playing_scene(fx: &[Fx], t: f64) -> Vec<(usize, EffectKind, f64)> {
    fx.iter()
        .enumerate()
        .filter(|(_, effect)| effect.effect_kind().is_some_and(drawn_while_playing))
        .map(|(index, effect)| {
            (
                index,
                effect.effect_kind().expect("filtered above"),
                visibility_at(effect, t),
            )
        })
        .collect()
}

/// Does this bar earn the dashed debt tail?
pub fn debt_tail_for(fx: &[Fx], index: usize, on_screen: f64, scene_runs_on: bool) -> bool {
    let Some(effect) = fx.get(index) else {
        return false;
    };
    draws_debt_tail(&effect.snd, debt(effect.dur, on_screen), scene_runs_on)
}

/// The plate's words, if there is room for the plate.
pub fn plate_for(debt: f64, width_px: f64) -> Option<String> {
    if !plate_shown(width_px) {
        return None;
    }
    let text = plate_text(debt);
    (!text.is_empty()).then_some(text)
}

/// §06-effects#2-the-lane-and-the-preview (`"sound X s behind|ahead"`): the plate's words.
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

/// §06-effects#2-the-lane-and-the-preview (`"sound X s behind|ahead" plate once wider than 60 px`): the room
/// the plate has to speak in.
pub fn room_px(allocated: f64, requested: f64) -> f64 {
    allocated.max(requested)
}

/// §06-effects#2-the-lane-and-the-preview (the debt tail and its plate): how far the sound under the red line
/// is from where the picture is.
pub fn plate_debt_at(fx: &[Fx], t: f64) -> f64 {
    let Some(winner) = fx
        .iter()
        .filter(|effect| {
            effect
                .effect_kind()
                .is_some_and(|kind| fx_record::uses(kind, fx_record::Field::Snd))
        })
        .filter(|effect| !effect.snd.is_empty())
        .filter(|effect| {
            let (from, to) = effect.spans();
            t >= from && t < to
        })
        .min_by(|a, b| a.t.total_cmp(&b.t))
    else {
        return 0.0;
    };
    debt(winner.dur, (t - winner.t).clamp(0.0, winner.dur))
}

/// §06-effects#2-the-lane-and-the-preview (`sndDip 0.15 s fades at the rejoin`, inventory/effects.md §A.3): the
/// fade in and back out where delayed sound rejoins the picture — short enough not to hear, long enough that the
/// rejoin does not click.
pub const SOUND_DIP_SECONDS: f64 = 0.15;

/// `P.eng.effectMinSeconds` (spec/10-parameters.md §5.1: 0.1, "shortest an effect band may be dragged down
/// to"; prototype `fxMinDur` in go/cut_fx.go).
pub const MIN_BAND_SECONDS: f64 = 0.1;

/// §06-effects#7-rules (`effects cannot leave the timeline`) plus `P.eng.effectMinSeconds`: where a band lands
/// when one of its ends is dragged to `to`.
pub fn drag_end(t: f64, dur: f64, end: bool, to: f64) -> (f64, f64) {
    if end {
        (t, (to - t).max(MIN_BAND_SECONDS))
    } else {
        // Leading end: the stop holds and the START is what moves.
        let stop = t + dur;
        let start = to.min(stop - MIN_BAND_SECONDS).max(0.0);
        (start, stop - start)
    }
}
