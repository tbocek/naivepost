//! F2.5 (Hush and mix — what the preview hears) — `spec/05-cut.md` F2.5.
//!
//! Sound overlaps whenever two sources were recording at once, and every one of them is a lane: the
//! camera's own sound, a separate recording, each extra ticked track of a multi-track file, a second
//! camera. Every lane is heard by default; the scene under the line decides what that scene hears, and
//! a gutter switch turns a lane off for the whole cut (F2.10). This module is that decision — the five
//! branches of F2.5's diagram as plain functions: which scene decides, what the footage's own sound
//! does, where each lane sits on the clock and when it is started, how rate and gain follow the effects
//! under the line, and the one preview volume every preview shares.
//!
//! No UI lives here either: the Cut page's preview-volume control forwards to [`PreviewVolume`] through
//! `ui::set_preview_volume` / `ui::preview_volume` (`rust/src/ui/window.rs`), and each tick asks this
//! module for one answer with [`mix_at`] rather than deciding anything itself. What still lands with the
//! lane rounds (F2.8/F2.10/F2.11) is the picture of it — the lanes drawn under the line, their speaker
//! badges and gutter switches, and the GStreamer pipelines these numbers will drive. Until then the mix
//! is computed and stored, not played: what is testable is the arithmetic those pipelines are handed,
//! which lanes start, at what second, stopping where, and how loud.

use crate::cut::{Cut, Fx, Lane, Seg};

/// F2.5 S5 (`P.eng.maxGain`, `spec/10-parameters.md`: "volume effect ceiling (playbin's own)"): the
/// loudest any gain — an effect's own, or the mix of slider and effect — may be asked for, because that
/// is what the volume property will take. Catalogued as `P.eng.maxGain` in [`crate::params`].
pub const MAX_GAIN: f64 = 10.0;

// --- S1: the scene under the line decides -------------------------------------------------------------

/// F2.5 S1: the scene whose hearing answers for the session second `t`, or `None` for a second the cut
/// removed — a removed stretch has no say about sound, so it decides nothing and the caller keeps the
/// previous answer rather than inventing one.
///
/// The first match is the only match: the cut's segments are disjoint (§3's own rule that two clips of
/// one camera touching are one clip), so there is never a second scene to break a tie between.
pub fn scene_at(segs: &[Seg], t: f64) -> Option<usize> {
    segs.iter().position(|seg| t >= seg.s && t < seg.e)
}

/// F2.5 S1: the lanes this scene does not hear — its `quiet` list, which is what a speaker badge writes
/// and what [`lane_start`] refuses to start.
///
/// Empty for an insert (`ins` non-empty: spliced or overwriting, both bring their own sound or replace the
/// lane they were laid in, and neither is a question about which lanes are heard), so a list left on one
/// cannot silence a lane by a scene that has no say over any.
pub fn hush(scene: &Seg) -> &[String] {
    if !scene.ins.is_empty() {
        return &[];
    }
    &scene.quiet
}

// --- S2: the footage's own sound ----------------------------------------------------------------------

/// F2.5 S2 (`the footage's own sound: muted by property when the scene silences it`): is the picture's
/// own sound heard in this scene?
///
/// Muted BY PROPERTY, never by stopping a pipeline: the answer changes every tick as the line moves, and
/// tearing a stream down and back up per tick is a hitch per tick (`spec/inventory/cut.md`: "the
/// footage's own sound uses the mute property"). The same flag also answers for an insert, whose `mute`
/// is its own "play it silent" tick (F2.7).
pub fn footage_sound_heard(scene: &Seg) -> bool {
    !scene.mute
}

// --- S3: each separate recording, its own pipeline, in sync ------------------------------------------

/// F2.5 S3: the file second the session second `t` lands on for this lane — `t - at + off`.
///
/// This IS the sync: a lane is placed by the time stamp in its name (F2.8 slides it by changing `at`),
/// and every pipeline is driven by the same transport, seeked to its own second of the same instant. No
/// lane ever asks where another one is.
pub fn lane_seconds(lane: &Lane, t: f64) -> f64 {
    t - lane.at + lane.off
}

/// F2.5 S3: does this lane's file have anything at the session second `t`? `file_seconds` is
/// [`lane_seconds`] for the same `t`, passed in so a caller that already computed it does not do it twice.
///
/// A lane with `dur` runs from `off` to `off + dur`; without one it is its whole file, which is what §1
/// writes for a separate recording, so it runs from `off` on. A seek outside that is silence, not the
/// wrong minute played quietly under the picture — which is why this is asked before anything is started.
pub fn lane_covers(lane: &Lane, file_seconds: f64, t: f64) -> bool {
    if lane.dur > 0.0 {
        return file_seconds >= lane.off && file_seconds < lane.off + lane.dur;
    }
    t >= lane.at
}

/// F2.5 S3/S4: what one tick asks of one lane's pipeline. The three answers are the whole state a lane
/// can be in, and two of them look the same from outside — silence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LaneStart {
    /// Start it here, in the lane's own clock, and stop at `stop` (0 = no stop).
    Seek { from: f64, stop: f64 },
    /// The scene under the line silences this lane: never started, so it holds no stream at all.
    Silenced,
    /// The file has nothing at this second. A lane with nothing to play and a lane the scene does not
    /// hear are the same answer — held, not seeked — so they are one state, not two.
    Nothing,
}

/// F2.5 S3 + S4: start this lane for the session second `t` under `scene`, or refuse to.
///
/// Asked in this order on purpose: a silenced lane is not started even where its file has something
/// ("never started when silenced"), and a lane outside its file is not started either — seeking a
/// pipeline to a second past its end is a start from the file's first second, which is audible.
pub fn lane_start(lane: &Lane, scene: &Seg, t: f64) -> LaneStart {
    if !scene.hears(&lane.name) {
        return LaneStart::Silenced;
    }
    let from = lane_seconds(lane, t);
    if !lane_covers(lane, from, t) {
        return LaneStart::Nothing;
    }
    LaneStart::Seek { from, stop: stop_at_boundary(scene, lane, t) }
}

/// F2.5 S4 (`a lane started under a scene boundary is seeked, with a stop at the boundary`): where a
/// running lane's seek stops, in the lane's own clock — the scene's end, written as a file second. A
/// scene that runs past the lane's own end yields 0, which is "no stop": nothing of the lane's falls
/// silent at that boundary, so the seek may run to the end of what the file has.
///
/// The scene's END is the boundary because that is the second the answer expires at: the hearing is the
/// scene's, so it holds only until the scene does, and a stop on the seek is what makes the lane fall
/// silent there without waiting for a tick to notice (a timer-driven hush lands up to a tick late). The
/// next tick re-places the lane under whatever scene the line has reached. `t` is taken for symmetry with
/// [`lane_start`]; the boundary is the scene's, not the line's, so it does not read it.
pub fn stop_at_boundary(scene: &Seg, lane: &Lane, _t: f64) -> f64 {
    let stop = scene.e - lane.at + lane.off;
    if lane.dur > 0.0 && stop >= lane.off + lane.dur {
        return 0.0;
    }
    stop.max(0.0)
}

// --- S5: rate and gain follow the effects under the line ---------------------------------------------

/// F2.5 S5: how much of `fx` is doing its thing at `t`, as 0..1 — 0 outside the effect's seconds, 1
/// across the middle, a straight ramp over `trans` in and `tout` out. A fade of 0 either side is a hard
/// cut, which is what §1 says a zero transition means.
///
/// Each fade is clamped to what is left of the effect (`fin` to its length, `fout` to what `fin` did not
/// take), so an effect whose two fades add up past its own band still reads 1 in the middle rather than
/// going negative there — and an effect of no length covers no second at all.
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

/// F2.5 S5: how loud the session second `t` is, as a linear gain — 1 with no volume effect over it, and
/// every covering volume effect MULTIPLIED in (`g *= 1 + (gain - 1) × envelope`). Two gains are two
/// things done to the same sound, so they multiply; rates average instead, which is [`rate_under`]'s
/// business. The result never leaves `0..=MAX_GAIN`, because that is what the property will take.
///
/// This is the one gain rule for the seconds under the line: the preview reads it per tick and the render
/// reads the same numbers per clip (`spec/08-produce.md`), so a boosted stretch played at the slider's
/// full travel still lands inside what the pipeline accepts.
pub fn gain_under(fx: &[Fx], t: f64) -> f64 {
    let mut gain = 1.0;
    for effect in fx {
        if effect.effect_kind() != Some(crate::cut::EffectKind::Volume) {
            continue;
        }
        let asked = effect.gain.clamp(0.0, MAX_GAIN);
        gain *= 1.0 + (asked - 1.0) * envelope(effect, t);
    }
    gain.clamp(0.0, MAX_GAIN)
}

/// F2.5 S5: the rate the PREVIEW runs at `t` — the first speed effect covering `t`, flat across its
/// seconds, or 1 with none over it. Two decisions the prototype makes and this follows:
///
/// - A stop (`rate` 0) reads 1: a frozen stretch has no observable rate, the still covers it, and ×0 is
///   not a clip any encoder can build, so the footage under it runs on at full speed.
/// - Overlapping speeds take the FIRST here, where the render averages them — the preview is not asked to
///   agree with the render about two effects over one second, only to be playable; and it stays flat over
///   a ramp because a rate change is a flushing seek and a ramp's stairs ten times a second hang the
///   window.
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

// --- S6: one preview volume, shared by every preview ---------------------------------------------------

/// F2.5 S6: how loud the players are when nothing has been said. A number, not a percentage: the slider
/// is 0..100 and the property is 0..1, and the two meet in [`PreviewVolume::set_percent`].
pub const VOLUME_DEFAULT: f64 = 1.0;

/// F2.5 S6: what the control says about itself, `spec/inventory/cut.md` verbatim — the sentence that
/// tells a person why moving it changed nothing they rendered.
pub const VOLUME_TIP: &str = "preview volume — the players only; nothing that is rendered, and the same setting wherever it is shown";

/// F2.5 S6: the word beside the slider, so the trough is identifiable without hovering. The inventory
/// asks for "icon + 0..100, 120 px, shared tooltip" (`spec/inventory/cut.md` §D item 2) and the
/// prototype draws a speaker glyph there; this one spells the icon as a WORD because the headless
/// container has no icon theme, where a themed icon renders as a blank box — a word always draws.
pub const VOLUME_LABEL: &str = "preview volume";

/// F2.5 S6: the slider's 0..100 held to what a gain means. Below nought is silence, not a phase flip,
/// and above full travel is full travel.
pub fn clamp_volume(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// F2.5 S6 (`one preview volume, shared by every preview`): the app's one loudness setting.
///
/// ONE number for the whole app, held once and mirrored by every slider that is shown — never copied into
/// a preview, because two copies are two answers and the tooltip promises one. It changes no rendered
/// byte: the render mixes what each scene hears at recorded levels (`spec/08-produce.md`).
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
    /// The setting nobody has touched.
    pub fn new() -> Self {
        PreviewVolume { value: VOLUME_DEFAULT }
    }

    /// A slider's value — 0..100, step 1 — written as the one number. Every other slider mirrors this
    /// afterwards; none of them owns a copy.
    pub fn set_percent(&mut self, percent: f64) {
        self.value = clamp_volume(percent / 100.0);
    }

    /// The same value as a slider's position, for building or syncing one.
    pub fn percent(&self) -> f64 {
        self.value * 100.0
    }

    /// The same value as a gain: what every preview multiplies its sound by.
    pub fn value(&self) -> f64 {
        self.value
    }
}

/// F2.5 S6: the gain one pipeline is asked for — the room's slider and the cut's own say over the seconds
/// under the line, multiplied and held to what the property will take ([`MAX_GAIN`]).
pub fn mix_gain(volume: f64, fx_gain: f64) -> f64 {
    (clamp_volume(volume) * fx_gain).clamp(0.0, MAX_GAIN)
}

// --- the one answer a tick asks for ---------------------------------------------------------------------

/// F2.5: everything one tick of the preview asks its players to do, in one value. The page reads this and
/// forwards it; it decides none of the parts.
///
/// `lane_starts` keeps the lane's name with its answer because the caller addresses a pipeline by name —
/// two lanes of the same file are two pipelines and must not be told "started" without being told which.
/// `Vec` rather than a fixed array because the lane count is the project's, not a constant; that costs
/// `Copy`, so `Mix` derives `Clone` only, and callers who want the old cheap copy clone the vec once per
/// tick instead of copying five function results.
#[derive(Debug, Clone, PartialEq)]
pub struct Mix {
    /// The footage's own sound at this second: muted BY PROPERTY, never by stopping a stream (S2).
    pub footage_muted: bool,
    /// Each lane's answer under the scene that decides here, in the order the lanes were given (S3/S4).
    pub lane_starts: Vec<(String, LaneStart)>,
    /// The gain every pipeline is asked for: slider × the cut's own (S5 + S6), capped at [`MAX_GAIN`].
    pub gain: f64,
    /// The rate the preview runs at (S5): the first speed effect over `t`, or 1.
    pub rate: f64,
}

/// F2.5: the whole per-tick answer, composed in the spec's order — the scene under the line decides, the
/// footage's own sound is muted by property, each separate recording is started or refused under that
/// scene, and rate and gain follow the effects under the line.
///
/// A second the cut removed answers with no new say: `scene_at` returns `None` there, and a removed
/// stretch has no hearing of its own, so nothing here changes what the caller already had —
/// `footage_muted` stays false and no lane is refused, rather than inventing a silence nobody set.
/// `volume` is the app's ONE preview volume ([`PreviewVolume::value`]), never a per-preview copy.
pub fn mix_at(segs: &[Seg], lanes: &[Lane], fx: &[Fx], t: f64, volume: f64) -> Mix {
    let Some(scene) = scene_at(segs, t) else {
        return Mix {
            footage_muted: false,
            lane_starts: Vec::new(),
            gain: mix_gain(volume, gain_under(fx, t)),
            rate: rate_under(fx, t),
        };
    };
    let deciding = &segs[scene];
    Mix {
        footage_muted: !footage_sound_heard(deciding),
        lane_starts: lanes
            .iter()
            .map(|lane| (lane.name.clone(), lane_start(lane, deciding, t)))
            .collect(),
        gain: mix_gain(volume, gain_under(fx, t)),
        rate: rate_under(fx, t),
    }
}

// --- S7: saying what it did ----------------------------------------------------------------------------

/// F2.5 S7: a session second the way the Cut page writes one — `0:40`, `12:05`, minutes counting past an
/// hour without padding.
///
/// Not [`crate::tools::mm_ss`]: that is the padded form a model is quoted its own number in (§3.6), while
/// a status line and a tooltip read a moment the way a tape header does. Two spellings on purpose, one
/// per audience — but within the Cut page this is the only one, so a badge and a status never disagree.
pub fn scene_clock(t: f64) -> String {
    let whole = t.max(0.0) as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// F2.5 S7 (and `spec/05-cut.md` F2.10 S2's wording): the status a speaker badge leaves behind — "2026-09-16
/// 17-26-20 is silent in the scene at 0:40", or heard when the press turned the lane back on.
///
/// F2.10's badges call this rather than spelling the sentence again, so the two flows cannot drift apart;
/// the gutter switch's own arithmetic (a lane off for the whole cut) stays F2.10's round.
pub fn hush_status(base: &str, heard: bool, scene_start: f64) -> String {
    let word = if heard { "heard in" } else { "silent in" };
    format!("{base} is {word} the scene at {}", scene_clock(scene_start))
}

// --- F2.10 S2: the badges and the gutter switch ----------------------------------------------------------

/// F2.10 S2 (`🔈 speaker badge per lane: does this scene hear that lane`): flip it, and return what the page
/// says. `None` for a scene index the cut does not hold — a press aimed at nothing changes nothing; an insert
/// answers with what is already true and changes nothing either, since [`hush`] reads it as hearing everything
/// and a list written there would silence nothing while looking as if it did.
///
/// `quiet` is treated as a SET (`spec/10-parameters.md` §5 rule 10: "compared as a set; fresh slices per
/// toggle"): the list is rebuilt rather than poked, so re-silencing a lane that was already listed cannot
/// duplicate it and un-listing one leaves no hole. An insert's `quiet` is not this badge's business — [`hush`]
/// answers empty for one, and writing to it would silence a lane through a scene that has no say over any — so
/// an insert reports the change as made on nothing: its hearing is F2.5 S1's rule.
pub fn toggle_heard(cut: &mut Cut, scene: usize, lane: &str) -> Option<String> {
    let seg = cut.segs.get_mut(scene)?;
    // An insert's hearing is F2.5 S1's rule and this badge does not move it, so the answer it reports is the
    // one already true — nothing about the scene changes.
    if !seg.ins.is_empty() {
        return Some(hush_status(lane, seg.hears(lane), seg.s));
    }
    let heard = !seg.hears(lane);
    // A fresh slice, built from what is still silent: the pressed lane in or out, everything else as it was.
    let silenced: Vec<String> = if heard { vec![] } else { vec![lane.to_string()] };
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

/// Is every one of these lanes silent in EVERY kept scene — the state a row's switch sits in before a press
/// brings it back on? A scene that says nothing about a lane hears it, so silence has to be LISTED for this to
/// hold; one unsaid lane anywhere makes the row heard.
pub fn all_silent(cut: &Cut, lanes: &[&str]) -> bool {
    let speaks: Vec<&Seg> = cut.segs.iter().filter(|seg| seg.ins.is_empty()).collect();
    // Every kept scene has to LIST every one of the row's recordings. A scene that says nothing about a lane
    // hears it, so silence is only ever what was written down — which is also why a recording no scene was cut
    // with reads as heard rather than silent: nothing lists it.
    !speaks.is_empty() && lanes.iter().all(|lane| speaks.iter().all(|seg| !seg.hears(lane)))
}

/// F2.10 S2 (`the gutter switch toggles a lane for the whole cut`): does ANY scene still hear any of these
/// lanes? A row's switch stands for every recording on that row, so it reads as OFF only when none of them is
/// heard anywhere — a half-silenced row still reads as heard, and pressing it finishes the job.
pub fn lane_is_heard_anywhere(cut: &Cut, lanes: &[&str]) -> bool {
    // An insert is not a scene with a say about lanes — it brings its own sound — so it can neither make a row
    // heard nor keep one silent. `any` rather than `all`: one scene still hearing the row is enough to read the
    // switch as ON, which is what lets a half-silenced row finish its job on the next press.
    cut.segs
        .iter()
        .any(|seg| seg.ins.is_empty() && lanes.iter().any(|lane| seg.hears(lane)))
}

/// F2.10 S2 (`the gutter switch toggles a lane for the whole cut`): silence these lanes in every scene that
/// could hear them, or bring them all back, and say which happened. `lanes` is every recording on the row the
/// switch stands for; `name` is what the status calls them.
///
/// Three answers, per `spec/inventory/cut.md` §A: nothing to cut with yet ("… is in no scene yet — cut something
/// first") when the cut holds no scene at all, then off and on with how many scenes moved. The direction is read
/// from [`lane_is_heard_anywhere`] — a row half-silenced by hand goes fully silent rather than flipping back on.
/// An insert's hearing is F2.5 S1's rule and stays as it was: [`hush`] answers empty for one, and its own sound
/// replaces the lane it was laid in rather than choosing which lanes are heard.
pub fn toggle_lane_all(cut: &mut Cut, lanes: &[&str], name: &str) -> String {
    // Every kept scene answers for a lane, including one that says nothing about it yet: silence means LISTING
    // the lane there too. An insert is the exception (`an insert brings its own sound; no scene silences it`).
    let speaks = |seg: &Seg| seg.ins.is_empty();
    if cut.segs.is_empty() {
        return format!("{name} is in no scene yet \u{2014} cut something first");
    }
    // The switch reads as ON while any scene still hears any of the row's recordings — `off if ANY of them is
    // still heard anywhere, so a half-silenced row finishes the job` — and only a row silent in every scene that
    // could speak for it comes back on.
    let on = !all_silent(cut, lanes);
    let mut changed = 0usize;
    for seg in cut.segs.iter_mut() {
        if !speaks(seg) {
            continue;
        }
        // A fresh slice again (rule 10), and only the row's own lanes move: another camera keeps its say.
        // Pressing an ON switch silences the row; pressing an OFF one clears it back to hearing everything.
        let silenced: Vec<String> = if on { lanes.iter().map(|lane| lane.to_string()).collect() } else { vec![] };
        let quiet: Vec<String> = seg
            .quiet
            .iter()
            .filter(|held| !lanes.iter().any(|lane| held.as_str() == *lane))
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
