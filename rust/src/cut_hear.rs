//! F2.5 (Hush and mix — what the preview hears) — `spec/05-cut.md` F2.5.
//!
//! Every sound source is a lane, heard by default; the scene under the line decides what that scene
//! hears, and a gutter switch turns a lane off for the whole cut (F2.10). The five branches of F2.5's
//! diagram are plain functions here, and [`mix_at`] answers a whole tick. The mix is computed, not
//! played: what is testable is the arithmetic the pipelines are handed.

use crate::cut::{Cut, Fx, Lane, Seg};

/// F2.5 S5 (`P.eng.maxGain`): the loudest any gain may be asked for; what the volume property will take.
pub const MAX_GAIN: f64 = 10.0;

/// F2.5 S1: the scene whose hearing answers for session second `t`, or `None` for a removed second
/// (the caller keeps its previous answer).
pub fn scene_at(segs: &[Seg], t: f64) -> Option<usize> {
    segs.iter().position(|seg| t >= seg.s && t < seg.e)
}

/// F2.5 S1: the lanes this scene does not hear: its `quiet` list.
pub fn hush(scene: &Seg) -> &[String] {
    if !scene.ins.is_empty() {
        return &[];
    }
    &scene.quiet
}

/// F2.5 S2: is the picture's own sound heard in this scene?
pub fn footage_sound_heard(scene: &Seg) -> bool {
    !scene.mute
}

/// F2.5 S3: the file second the session second `t` lands on for this lane — `t - at + off`.
pub fn lane_seconds(lane: &Lane, t: f64) -> f64 {
    t - lane.at + lane.off
}

/// F2.5 S3: does this lane's file have anything at session second `t`? `file_seconds` is
/// [`lane_seconds`] for the same `t`.
pub fn lane_covers(lane: &Lane, file_seconds: f64, t: f64) -> bool {
    if lane.dur > 0.0 {
        return file_seconds >= lane.off && file_seconds < lane.off + lane.dur;
    }
    t >= lane.at
}

/// F2.5 S3/S4: what one tick asks of one lane's pipeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LaneStart {
    /// Start it here, in the lane's own clock, and stop at `stop` (0 = no stop).
    Seek { from: f64, stop: f64 },
    /// The scene under the line silences this lane: never started.
    Silenced,
    /// The file has nothing at this second. Same answer as silenced (held, not seeked), kept as one state.
    Nothing,
}

/// F2.5 S3 + S4: start this lane for the session second `t` under `scene`, or refuse to.
///
/// Order matters: seeking a pipeline past its file's end restarts from the first second, which is audible.
pub fn lane_start(lane: &Lane, scene: &Seg, t: f64) -> LaneStart {
    if !scene.hears(&lane.name) {
        return LaneStart::Silenced;
    }
    let from = lane_seconds(lane, t);
    if !lane_covers(lane, from, t) {
        return LaneStart::Nothing;
    }
    LaneStart::Seek {
        from,
        stop: stop_at_boundary(scene, lane, t),
    }
}

/// F2.5 S4: where a running lane's seek stops, in the lane's own clock: the scene's end as a file second,
/// or 0 (no stop) when the scene outlasts the lane.
pub fn stop_at_boundary(scene: &Seg, lane: &Lane, _t: f64) -> f64 {
    let stop = scene.e - lane.at + lane.off;
    if lane.dur > 0.0 && stop >= lane.off + lane.dur {
        return 0.0;
    }
    stop.max(0.0)
}

/// F2.5 S5: how much of `fx` is doing its thing at `t`, as 0..1, with a straight ramp over `trans` in
/// and `tout` out; a fade of 0 is a hard cut.
///
/// Each fade is clamped to what is left of the effect, so overlong fades still read 1 in the middle.
pub fn envelope(fx: &Fx, t: f64) -> f64 {
    let (from, to) = fx.spans();
    if t < from || t >= to {
        return 0.0;
    }
    let length = to - from;
    let fin = fx.trans.clamp(0.0, length);
    let fout = fx.tout.clamp(0.0, length - fin);
    let mut alpha = 1.0;
    if fin > 0.0 && t < from + fin {
        alpha = (t - from) / fin;
    }
    if fout > 0.0 && t > to - fout {
        alpha = alpha.min((to - t) / fout);
    }
    alpha.clamp(0.0, 1.0)
}

/// F2.5 S5: how loud second `t` is, as a linear gain: every covering volume effect is MULTIPLIED in
/// (`g *= 1 + (gain - 1) × envelope`), never leaving `0..=MAX_GAIN`. Rates average instead ([`rate_under`]).
pub fn gain_under(fx: &[Fx], t: f64) -> f64 {
    let mut gain = 1.0;
    for effect in fx {
        if effect.effect_kind() != Some(crate::cut::EffectKind::Volume) {
            continue;
        }
        let asked = effect.gain.clamp(0.0, crate::fx_record::gain_ceiling());
        gain *= 1.0 + (asked - 1.0) * envelope(effect, t);
    }
    gain.clamp(0.0, crate::fx_record::gain_ceiling())
}

/// F2.5 S5: the rate the PREVIEW runs at `t`: the first speed effect covering it, flat, or 1.
///
/// - A stop (`rate` 0) reads 1: ×0 is not a clip any encoder can build.
/// - Overlapping speeds take the FIRST here (the render averages); it stays flat over a ramp because a
///   rate change is a flushing seek and a ramp's stairs ten times a second hang the window.
///
/// [`crate::cut::Seg::rate`] is the render planner's answer (F5.2) and is not asked here.
pub fn rate_under(fx: &[Fx], t: f64) -> f64 {
    for effect in fx {
        if effect.effect_kind() != Some(crate::cut::EffectKind::Speed) {
            continue;
        }
        let (from, to) = effect.spans();
        if t >= from && t < to {
            return if effect.rate == 0.0 { 1.0 } else { effect.rate };
        }
    }
    1.0
}

/// §06-effects#2 (`mute mutes the preview`): is the FIRST Speed covering `t` one whose sound answer is
/// `mute`? Rule: [`crate::fx_lane::mute_silences_the_preview`]. It silences THE WHOLE BED, separate
/// recordings included, as [`crate::render_fx::mute_expr`] does in the render.
/// The first Speed covering `t`, if any: the one walk [`rate_under`] and [`bed_silenced_under`] share (§A.3).
fn first_covering_speed(fx: &[Fx], t: f64) -> Option<&Fx> {
    fx.iter().find(|effect| {
        effect.effect_kind() == Some(crate::cut::EffectKind::Speed) && {
            let (from, to) = effect.spans();
            t >= from && t < to
        }
    })
}

pub fn bed_silenced_under(fx: &[Fx], t: f64) -> bool {
    first_covering_speed(fx, t)
        .is_some_and(|effect| crate::fx_lane::mute_silences_the_preview(&effect.snd))
}

/// §06-effects#2 (`the two 1× sound answers cannot be previewed`): is the FIRST Speed covering `t` one
/// whose sound needs a clock of its own? Rule: [`crate::fx_lane::previewable_sound`]; lets the preview
/// say it is faking.
pub fn sound_needs_two_clocks_under(fx: &[Fx], t: f64) -> bool {
    first_covering_speed(fx, t)
        .is_some_and(|effect| !crate::fx_lane::previewable_sound(&effect.snd))
}

/// F2.5 S6: how loud the players are when nothing has been said. The slider is 0..100, the property 0..1.
pub const VOLUME_DEFAULT: f64 = 1.0;

/// F2.5 S6: the control's tooltip, `spec/inventory/cut.md` verbatim.
pub const VOLUME_TIP: &str = "preview volume — the players only; nothing that is rendered, and the same setting wherever it is shown";

/// F2.5 S6: the slider's 0..100 held to what a gain means.
pub fn clamp_volume(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// F2.5 S6: the app's one loudness setting, mirrored by every slider, never copied into a preview.
///
/// It changes no rendered byte: the render mixes at recorded levels (`spec/08-produce.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewVolume {
    value: f64,
}

impl Default for PreviewVolume {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewVolume {
    pub fn new() -> Self {
        PreviewVolume {
            value: VOLUME_DEFAULT,
        }
    }

    /// A slider's value (0..100, step 1) written as the one number.
    pub fn set_percent(&mut self, percent: f64) {
        self.value = clamp_volume(percent / 100.0);
    }

    pub fn percent(&self) -> f64 {
        self.value * 100.0
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

/// F2.5 S6: the gain one pipeline is asked for: slider and cut's own, multiplied and held to [`MAX_GAIN`].
pub fn mix_gain(volume: f64, fx_gain: f64) -> f64 {
    (clamp_volume(volume) * fx_gain).clamp(0.0, crate::fx_record::gain_ceiling())
}

/// F2.5: everything one tick of the preview asks its players to do, in one value.
///
/// `lane_starts` keeps the lane's name because the caller addresses a pipeline by name. `Vec` costs
/// `Copy`, so `Mix` derives `Clone` only.
#[derive(Debug, Clone, PartialEq)]
pub struct Mix {
    /// The footage's own sound at this second: muted BY PROPERTY, never by stopping a stream (S2).
    pub footage_muted: bool,
    /// Each lane's answer under the scene that decides here (S3/S4).
    pub lane_starts: Vec<(String, LaneStart)>,
    /// The gain every pipeline is asked for: slider × the cut's own (S5 + S6), capped at [`MAX_GAIN`].
    pub gain: f64,
    /// The rate the preview runs at (S5): the first speed effect over `t`, or 1.
    pub rate: f64,
    /// Whether the first speed effect over `t` asked for silence ([`bed_silenced_under`]); `gain` alone
    /// cannot tell that from a slider turned to zero.
    pub bed_silenced: bool,
    /// Whether the first speed effect over `t` asked for sound on its own clock
    /// ([`sound_needs_two_clocks_under`]); the preview cannot give it, so the caller owes a sentence.
    pub sound_needs_two_clocks: bool,
}

/// F2.5: the whole per-tick answer, composed in the spec's order.
///
/// A removed second (`scene_at` is `None`) changes nothing: `footage_muted` stays false and no lane is
/// refused. `volume` is the app's ONE preview volume ([`PreviewVolume::value`]).
pub fn mix_at(segs: &[Seg], lanes: &[Lane], fx: &[Fx], t: f64, volume: f64) -> Mix {
    // The whole bed goes silent together via the gain. `footage_muted` is NOT set: it is the scene's own
    // mute property (S2) and `preview_sound::report_mute_change` logs per change of it.
    let silenced = bed_silenced_under(fx, t);
    let two_clocks = sound_needs_two_clocks_under(fx, t);
    let bed_gain = |volume: f64| -> f64 {
        if silenced {
            0.0
        } else {
            mix_gain(volume, gain_under(fx, t))
        }
    };
    let Some(scene) = scene_at(segs, t) else {
        return Mix {
            footage_muted: false,
            lane_starts: Vec::new(),
            gain: bed_gain(volume),
            rate: rate_under(fx, t),
            bed_silenced: silenced,
            sound_needs_two_clocks: two_clocks,
        };
    };
    let deciding = &segs[scene];
    Mix {
        footage_muted: !footage_sound_heard(deciding),
        lane_starts: lanes
            .iter()
            .map(|lane| (lane.name.clone(), lane_start(lane, deciding, t)))
            .collect(),
        gain: bed_gain(volume),
        rate: rate_under(fx, t),
        bed_silenced: silenced,
        sound_needs_two_clocks: two_clocks,
    }
}

/// F2.5 S7: a session second the way the Cut page writes one — `0:40`, `12:05`.
///
/// Not [`crate::tools::mm_ss`], the padded form a model is quoted; the two spellings are on purpose.
pub fn scene_clock(t: f64) -> String {
    let whole = t.max(0.0) as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// F2.5 S7 (F2.10 S2's wording): the status a speaker badge leaves behind, e.g. "2026-09-16
/// 17-26-20 is silent in the scene at 0:40".
pub fn hush_status(base: &str, heard: bool, scene_start: f64) -> String {
    let word = if heard { "heard in" } else { "silent in" };
    format!("{base} is {word} the scene at {}", scene_clock(scene_start))
}

/// F2.10 S2: flip a scene's hearing of a lane and return the page's status. `None` for an unknown scene
/// index; an insert answers with what is already true and changes nothing.
///
/// `quiet` is a SET (`spec/10-parameters.md` §5 rule 10): rebuilt, not poked, so no duplicates or holes.
pub fn toggle_heard(cut: &mut Cut, scene: usize, lane: &str) -> Option<String> {
    let seg = cut.segs.get_mut(scene)?;
    // An insert's hearing is F2.5 S1's rule; report what is already true.
    if !seg.ins.is_empty() {
        return Some(hush_status(lane, seg.hears(lane), seg.s));
    }
    let heard = !seg.hears(lane);
    // A fresh slice: the pressed lane in or out, everything else as it was.
    let silenced: Vec<String> = if heard {
        vec![]
    } else {
        vec![lane.to_string()]
    };
    let quiet: Vec<String> = seg
        .quiet
        .iter()
        .filter(|held| held.as_str() != lane)
        .cloned()
        .chain(silenced)
        .collect();
    seg.quiet = quiet;
    Some(hush_status(lane, heard, seg.s))
}

/// Is every one of these lanes silent in EVERY kept scene? Silence has to be LISTED; one unsaid lane
/// anywhere makes the row heard.
pub fn all_silent(cut: &Cut, lanes: &[&str]) -> bool {
    let speaks: Vec<&Seg> = cut.segs.iter().filter(|seg| seg.ins.is_empty()).collect();
    // Every kept scene has to LIST every recording of the row; a recording no scene was cut with reads as heard.
    !speaks.is_empty()
        && lanes
            .iter()
            .all(|lane| speaks.iter().all(|seg| !seg.hears(lane)))
}

/// F2.10 S2: does ANY scene still hear any of these lanes? A half-silenced row still reads as heard.
pub fn lane_is_heard_anywhere(cut: &Cut, lanes: &[&str]) -> bool {
    // An insert has no say about lanes. `any`, not `all`: one hearing scene keeps the switch ON.
    cut.segs
        .iter()
        .any(|seg| seg.ins.is_empty() && lanes.iter().any(|lane| seg.hears(lane)))
}

/// §05-cut §1's per-row sound switch: does any kept scene shown from `row` still play that row's own
/// footage sound? That is the scene's `mute` property (S2), not a listed lane.
pub fn row_is_heard_anywhere(cut: &Cut, row: i32) -> bool {
    cut.segs
        .iter()
        .any(|seg| seg.ins.is_empty() && seg.cam == row && footage_sound_heard(seg))
}

/// §05-cut §1's per-row sound switch, pressed: silence the footage's own sound in every kept scene shown
/// from `row`, or bring it back, and say which happened (answers as [`toggle_lane_all`]).
pub fn toggle_row_all(cut: &mut Cut, row: i32, name: &str) -> String {
    let from_row = |seg: &Seg| seg.ins.is_empty() && seg.cam == row;
    if !cut.segs.iter().any(from_row) {
        return format!("{name} is in no scene yet \u{2014} cut something first");
    }
    let on = row_is_heard_anywhere(cut, row);
    let mut changed = 0usize;
    for seg in cut.segs.iter_mut().filter(|seg| from_row(seg)) {
        if seg.mute != on {
            seg.mute = on;
            changed += 1;
        }
    }
    if on {
        format!("{name} sound off for the whole cut \u{2014} {changed} scene(s) changed")
    } else {
        format!("{name} sound is on for the whole cut \u{2014} every scene plays it ({changed} changed)")
    }
}

/// F2.10 S2: silence these lanes in every scene that could hear them, or bring them back, and say which
/// happened. `lanes` is every recording on the row; `name` is what the status calls them.
///
/// Three answers per `spec/inventory/cut.md` §A: nothing to cut with yet, then off and on with how many
/// scenes moved. The direction is read from [`lane_is_heard_anywhere`], so a half-silenced row goes fully
/// silent. An insert's hearing stays F2.5 S1's rule.
pub fn toggle_lane_all(cut: &mut Cut, lanes: &[&str], name: &str) -> String {
    // Every kept scene answers for a lane, including one that says nothing yet. Inserts are the exception.
    let speaks = |seg: &Seg| seg.ins.is_empty();
    if cut.segs.is_empty() {
        return format!("{name} is in no scene yet \u{2014} cut something first");
    }
    // ON while any scene still hears any of the row's recordings.
    let on = !all_silent(cut, lanes);
    let mut changed = 0usize;
    for seg in cut.segs.iter_mut() {
        if !speaks(seg) {
            continue;
        }
        // A fresh slice again (rule 10); only the row's own lanes move.
        let silenced: Vec<String> = if on {
            lanes.iter().map(|lane| lane.to_string()).collect()
        } else {
            vec![]
        };
        let quiet: Vec<String> = seg
            .quiet
            .iter()
            .filter(|held| !lanes.contains(&held.as_str()))
            .cloned()
            .chain(silenced)
            .collect();
        if quiet != seg.quiet {
            changed += 1;
        }
        seg.quiet = quiet;
    }
    if on {
        format!("{name} off for the whole cut \u{2014} {changed} scene(s) changed")
    } else {
        format!("{name} is on for the whole cut \u{2014} every scene hears it ({changed} changed)")
    }
}
