//! §06-effects#4-render: the render's ASSEMBLY step. [`crate::render_fx`] says each effect in filter text; this puts them in §4's order for one clip, asks for the inputs they read (a still, a generated title sheet, the user's drawing) and writes the files ffmpeg opens. It decides nothing about filters, so a test can read back which builders ran via [`for_clip`] without an encoder.
//! # What §4 leaves to be decided here
//! - **Where the pieces go in the graph.** `render_fx::picture_order()` and `render_fx::audio_order()` fix the order; [`for_clip`] answers the effect halves and [`crate::produce_render`] splices them at those seams.
//! - **A generated title must exist as a file.** §4 says "each from a looped input", so the SVG from `render_fx::title_svg` is WRITTEN to scratch first ([`Built::title_files`]) for `-loop 1`.
//! - **Box fractions become pixels** by [`box_px`]: text defaults to a lower third and svg to the middle; size falls back to each kind's shipped band.

use std::path::{Path, PathBuf};

use crate::cut::{EffectKind, Fx};
use crate::produce_render::Clip;
use crate::render_fx;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Built {
    /// Picture filters in §4's order: camera, frozen spans, then whatever is printed on top; the caller puts them around the burned-subtitle filter.
    pub video: Vec<String>,
    pub audio: Vec<String>,
    pub lane_gains: Vec<(String, String)>,
    pub overlays: Vec<render_fx::Overlay>,
    /// Extra inputs the filters read, as `(kind, path)` in the order after `-i` #0 ([`crate::produce_render::encode_command`]).
    pub inputs: Vec<(String, Option<String>)>,
    pub logs: Vec<String>,
    /// Files the encode reads but the program must create first: `(path, contents)`; today only §4's generated title sheets.
    pub title_files: Vec<(PathBuf, String)>,
}

/// One clip's effects as render filters, extra inputs and written files. `fx` is read through `fx_for` as `plan_clips` does; `src` is the footage shape a camera rectangle is measured against; `scratch` is `produce/clips/` (§3, cleared every run).
pub fn for_clip(clip: &Clip, fx: &[Fx], src: (i32, i32), scratch: &Path) -> Built {
    let mut built = Built::default();
    let drawn: Vec<Fx> = fx
        .iter()
        .filter(|e| render_fx::is_rendered(&e.kind))
        .cloned()
        .collect();
    let footage = footage_length(clip);

    // The camera: §4's `> 10× capped and logged` runs BEFORE the chain is built, so the chain carries what was actually rendered.
    let samples = render_fx::camera_samples(&drawn, clip.seg_s, footage);
    let (samples, capped) = render_fx::cap_zoom(&samples, src, clip.frame, clip.no);
    if let Some(line) = capped {
        built.logs.push(line);
    }
    // A parked camera that looks at the full frame needs no crop+scale (§4's static branch is a CROP).
    if !samples.is_empty() && (!render_fx::is_static(&samples) || !fills_frame(&samples[0])) {
        let rate = if clip.rate > 0.0 { clip.rate } else { 1.0 };
        let on_clip_clock: Vec<render_fx::Sample> = samples
            .iter()
            .map(|sample| render_fx::Sample {
                t: (sample.t - clip.seg_s) / rate,
                ..sample.clone()
            })
            .collect();
        built.video.extend(render_fx::camera_chain(
            &on_clip_clock,
            clip.frame,
            clip.frame,
        ));
    }

    for stop in drawn.iter().filter(|e| crate::fx_record::is_stop(e)) {
        let Some((window, fades)) = on_clip(stop, clip) else {
            continue;
        };
        built
            .overlays
            .push(render_fx::still_overlay(window, fades, clip.frame));
        built
            .inputs
            .push(("still".to_string(), Some(format!("{}", stop.t))));
    }

    for (index, effect) in drawn
        .iter()
        .filter(|e| {
            matches!(
                e.effect_kind(),
                Some(EffectKind::Text) | Some(EffectKind::Svg)
            )
        })
        .enumerate()
    {
        let Some((window, fades)) = on_clip(effect, clip) else {
            continue;
        };
        let box_px = box_px(effect, clip.frame);
        match effect.effect_kind() {
            Some(EffectKind::Text) => {
                let sheet = scratch.join(title_name(clip.no, index));
                built.title_files.push((
                    sheet.clone(),
                    render_fx::title_svg(&effect.text, box_px.2, box_px.3),
                ));
                built
                    .inputs
                    .push(("text".to_string(), Some(sheet.display().to_string())));
            }
            Some(EffectKind::Svg) => {
                // An empty path is a drawing whose file went away: say so and draw nothing rather than lose the clip.
                if effect.src.trim().is_empty() {
                    built.logs.push(missing_drawing_log(clip.no));
                    continue;
                }
                built
                    .inputs
                    .push(("svg".to_string(), Some(effect.src.clone())));
            }
            _ => {}
        }
        built
            .overlays
            .push(render_fx::overlay(window, fades, box_px));
    }

    // `snd` is stored on the speed effect; a stop carries rate 0 and its own sound, so the first sped effect with a spelling wins.
    if let Some(speed) = drawn.iter().find(|e| {
        e.effect_kind() == Some(EffectKind::Speed)
            && !crate::fx_record::is_stop(e)
            && !e.snd.is_empty()
    }) {
        built.audio.extend(sound_of(clip, speed));
    }

    // Volume: §4's `after the lane mix, before the narration` is the CALLER's seam (`produce_render::audio_chain`). Cues come in seconds already divided by the clip's rate.
    let cues = render_fx::all_gain_cues(&drawn, clip.seg_s, clip.rate, clip.on_screen);
    // §9's lane reach: a cue naming a lane runs BEFORE the lane mix, the rest after it. A card hears no lane, so a named cue over one rides its own sound (§9 MUST row).
    let (before, after): (Vec<_>, Vec<_>) = cues
        .into_iter()
        .partition(|cue| cue.before_the_mix() && clip.source.is_empty());
    for cue in &before {
        built.lane_gains.extend(
            render_fx::gain_filters(std::slice::from_ref(cue))
                .into_iter()
                .map(|filter| (cue.lane.clone(), filter)),
        );
    }
    built.audio.extend(render_fx::gain_filters(&after));
    built
}

/// §4 (`a planned 1× read head with 0.15 s dips at run ends for own/scene`): sound held at 1× while the picture runs ahead is read on its own clock; only the fades at the run ends reach the graph, [`crate::fx_lane::SOUND_DIP_SECONDS`] each, via [`render_fx::read_head`]. Pitch and mute use their own filters and replace the seam dip.
fn sound_of(clip: &Clip, speed: &Fx) -> Vec<String> {
    match speed.snd.as_str() {
        // §8 bullet 4: the head opens where the planner said (`clip.read_head`), and the 0.15 s dip is split half to each side of the join.
        "own" | "scene" => match clip.read_head {
            None => render_fx::sound_filters(clip.rate, ""),
            Some(_) => {
                let half = crate::effect_details::dip_each_side();
                vec![
                    format!("afade=t=in:st=0:d={half}"),
                    format!(
                        "afade=t=out:st={:.3}:d={half}",
                        (clip.on_screen - half).max(0.0)
                    ),
                ]
            }
        },
        other => render_fx::sound_filters(clip.rate, other),
    }
}

/// §1 box fractions as pixels: `(x, y, w, h)` of the OUTPUT frame, positioned by centre. Defaults via [`Fx::centre`]; an undrawn size falls back to the kind's shipped band (0.8×0.16 caption, 0.6×0.6 drawing). Even sides for 4:2:0.
pub fn box_px(effect: &Fx, frame: (i32, i32)) -> (i32, i32, i32, i32) {
    let (cx, cy) = effect.centre();
    let fallback = match effect.effect_kind() {
        Some(EffectKind::Text) => (
            crate::fx_text::LOWER_THIRD.wf,
            crate::fx_text::LOWER_THIRD.hf,
        ),
        _ => (crate::fx_svg::MIDDLE.wf, crate::fx_svg::MIDDLE.hf),
    };
    let wf = effect
        .wf
        .unwrap_or(fallback.0)
        .clamp(crate::fx_text::MIN_WIDTH_FRACTION, 1.0);
    let hf = effect
        .hf
        .unwrap_or(fallback.1)
        .clamp(crate::fx_text::MIN_HEIGHT_FRACTION, 1.0);
    let w = even((wf * frame.0 as f64).round() as i32);
    let h = even((hf * frame.1 as f64).round() as i32);
    let x = ((cx * frame.0 as f64) - w as f64 / 2.0).max(0.0).round() as i32;
    let y = ((cy * frame.1 as f64) - h as f64 / 2.0).max(0.0).round() as i32;
    (x.min(frame.0 - w).max(0), y.min(frame.1 - h).max(0), w, h)
}

/// Where an effect shows inside one clip on the clip's own clock (window and fades), or `None` when it does not reach this clip. A fade belongs only to the clip holding that end of the band.
fn on_clip(effect: &Fx, clip: &Clip) -> Option<((f64, f64), (f64, f64))> {
    let rate = if clip.rate > 0.0 { clip.rate } else { 1.0 };
    let start = (effect.t - clip.seg_s) / rate;
    let end = start + effect.dur.max(0.0) / rate;
    let (from, to) = (start.max(0.0), end.min(clip.on_screen));
    if to <= from {
        return None;
    }
    let fade_in = if start >= 0.0 {
        effect.trans.max(0.0)
    } else {
        0.0
    };
    let fade_out = if end <= clip.on_screen {
        effect.tout.max(0.0)
    } else {
        0.0
    };
    Some(((from, to), (fade_in, fade_out)))
}

fn footage_length(clip: &Clip) -> f64 {
    let rate = if clip.rate > 0.0 { clip.rate } else { 1.0 };
    let played = if (rate - 1.0).abs() > f64::EPSILON {
        clip.on_screen * rate
    } else {
        clip.on_screen
    };
    played.max(0.0)
}

fn fills_frame(sample: &render_fx::Sample) -> bool {
    sample.hf >= 1.0 - 1e-9
}

/// Round up to a whole even pixel: 4:2:0 refuses a chroma sample on an odd edge (as [`crate::render_fx::frame_box`]).
pub(crate) fn even(v: i32) -> i32 {
    v + v % 2
}

/// Scratch name of a clip's title sheet, numbered by position so two captions in one clip do not overwrite each other.
fn title_name(no: usize, index: usize) -> String {
    format!("c{no:03}_t{index}.svg")
}

pub fn missing_drawing_log(no: usize) -> String {
    format!("clip {no}: its svg drawing is not there any more \u{2014} nothing is drawn over it")
}
