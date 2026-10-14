//! §06-effects#4-render (`how each effect becomes ffmpeg`) — the render's ASSEMBLY step.
//!
//! [`crate::render_fx`] already knows how to say each single effect in filter text: the crop-and-scale pair for a
//! parked camera, `zoompan` on piecewise expressions for a moving one, the still that covers a stop, the looped
//! title input with its alpha fades, one `volume=<expr>:eval=frame` per cue. None of it used to be CALLED from
//! anywhere: the encode chain carried only `setpts`, `atempo` and a placeholder per volume cue, so a rendered video
//! came out with no zoom, no freeze, no caption and no duck in it while every rule still passed its own test.
//!
//! This file is the missing caller. It decides NOTHING about filters — every string comes from `render_fx` — it only
//! puts them in §4's order for one clip, asks for the inputs those filters read (a still, a generated title sheet,
//! the user's drawing), and writes the files ffmpeg will open. That split is what makes it checkable: a test can hand
//! a clip's effects to [`for_clip`] and read back which builders ran, without an encoder.
//!
//! # What §4 leaves to be decided here, and how
//!
//! - **Where the pieces go in the graph.** `render_fx::picture_order()` says camera → burned subtitles → text/svg,
//!   and `render_fx::audio_order()` says lane mix → volume cues → narration. [`for_clip`] answers the halves of those
//!   two orders that belong to effects; [`crate::produce_render`] splices them into its own chain at exactly those
//!   seams, so neither file lists the order twice.
//! - **A generated title has to exist as a file.** §4 says "each from a looped input", and an input is a path. So the
//!   SVG `render_fx::title_svg` builds is WRITTEN into the scratch folder before the command is built — see
//!   [`Built::title_files`], which the caller writes to disk. A title that was only ever a string could not be fed to
//!   `-loop 1`.
//! - **The box fractions become pixels** by [`box_px`]: §1 gives text a lower-third default and svg a middle default,
//!   `Fx::centre` already answers the centre, and the width/height fall back to each kind's shipped band.

use std::path::{Path, PathBuf};

use crate::cut::{EffectKind, Fx};
use crate::produce_render::Clip;
use crate::render_fx;

/// Everything one clip's effects add to its encode, in the four shapes the caller needs them in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Built {
    /// Filter text for the picture, in §4's order among themselves: the camera first, then the frozen spans, then
    /// whatever is printed on top. The caller puts these around the burned-subtitle filter.
    pub video: Vec<String>,
    /// Filter text for the sound: the sped-sound answer and the volume cues, in `render_fx::audio_order()`'s order.
    pub audio: Vec<String>,
    /// Extra inputs the filters above read, as `(kind, path)` pairs in the order they must appear after `-i` #0 —
    /// the same shape [`crate::produce_render::encode_command`] takes.
    pub inputs: Vec<(String, Option<String>)>,
    /// Lines the run should print: the > 10× cap, a drawing whose file went missing. Naming them here keeps the
    /// reason next to the decision rather than in the caller's head.
    pub logs: Vec<String>,
    /// Files this clip's encode reads but the program has to create first: `(path, contents)`. §4's generated title
    /// sheets are the whole list today — nothing else the render invents needs writing.
    pub title_files: Vec<(PathBuf, String)>,
}

/// One clip's effects as the render's filters, its extra inputs and its written files.
///
/// `fx` is what the segment carries (the caller reads it through the same `fx_for` door `plan_clips` uses, so a clip
/// and its filters cannot disagree); `src` is the footage's own shape, which is what a camera rectangle is measured
/// against; `scratch` is `produce/clips/`, where a generated title sheet belongs (§3 lists it as scratch, cleared
/// every run).
pub fn for_clip(clip: &Clip, fx: &[Fx], src: (i32, i32), scratch: &Path) -> Built {
    let mut built = Built::default();
    // A label is §4's "nothing": filtered out before any builder sees it, so it cannot reach a filter by accident.
    let drawn: Vec<Fx> = fx.iter().filter(|e| render_fx::is_rendered(&e.kind)).cloned().collect();
    let footage = footage_length(clip);

    // --- the camera: sampled, capped, then either crop+scale or zoompan -------------------------------------
    // §4's `> 10× capped and logged` runs BEFORE the chain is built, because the log names the depth ffmpeg refused
    // and the chain has to carry what was actually rendered, not what was asked for.
    let samples = render_fx::camera_samples(&drawn, clip.seg_s, footage);
    let (samples, capped) = render_fx::cap_zoom(&samples, src, clip.frame, clip.no);
    if let Some(line) = capped {
        built.logs.push(line);
    }
    // Nothing moved and nothing cropped: the clip is already its frame's size and shape, and a crop+scale pair over
    // the whole picture would only give the encoder something to resample. §4's static branch is a CROP, so a parked
    // camera that looks anywhere but at the full frame does get its pair.
    if !samples.is_empty() && (!render_fx::is_static(&samples) || !fills_frame(&samples[0])) {
        built.video.extend(render_fx::camera_chain(&samples, src, clip.frame));
    }

    // --- the stop: a still over footage that runs on ----------------------------------------------------------------
    for stop in drawn.iter().filter(|e| crate::fx_record::is_stop(e)) {
        let from = (stop.t - clip.seg_s).max(0.0);
        let to = from + stop.dur.max(0.0);
        built.video.extend(render_fx::still_chain(
            stop.t,
            (from, to),
            (stop.trans.max(0.0), stop.tout.max(0.0)),
            clip.frame,
        ));
        // The frame at t from THIS scene's lens, extracted by the caller's ffmpeg from the same input the clip
        // reads. Naming it under the scratch folder keeps it out of the project and out of the next run (§3).
        built.inputs.push((
            "still".to_string(),
            Some(scratch.join(still_name(clip.no, stop.t)).display().to_string()),
        ));
    }

    // --- words and drawings: after the camera and the burned cues -------------------------------------------------
    for (index, effect) in drawn
        .iter()
        .filter(|e| matches!(e.effect_kind(), Some(EffectKind::Text) | Some(EffectKind::Svg)))
        .enumerate()
    {
        let from = (effect.t - clip.seg_s).max(0.0);
        let window = (from, from + effect.dur.max(0.0));
        let fades = (effect.trans.max(0.0), effect.tout.max(0.0));
        let box_px = box_px(effect, clip.frame);
        match effect.effect_kind() {
            Some(EffectKind::Text) => {
                let sheet = scratch.join(title_name(clip.no, index));
                built.title_files.push((
                    sheet.clone(),
                    render_fx::title_svg(&effect.text, box_px.2, box_px.3),
                ));
                built.inputs.push(("text".to_string(), Some(sheet.display().to_string())));
            }
            Some(EffectKind::Svg) => {
                // Their document, our box. An empty path is a drawing whose file went away: say so and draw nothing
                // rather than hand ffmpeg an input it cannot open and lose the whole clip over it.
                if effect.src.trim().is_empty() {
                    built.logs.push(missing_drawing_log(clip.no));
                    continue;
                }
                built.video.extend(render_fx::scaled_into_box(&effect.src, box_px));
                built.inputs.push(("svg".to_string(), Some(effect.src.clone())));
            }
            _ => {}
        }
        built.video.extend(render_fx::overlay_chain(
            match effect.effect_kind() {
                Some(EffectKind::Svg) => "svg",
                _ => "title",
            },
            window,
            fades,
            box_px,
        ));
    }

    // --- the sound of a re-timed stretch ----------------------------------------------------------------------------
    // `snd` is stored on the speed effect itself; a stop carries rate 0 and its own sound answer, so the first
    // sped effect with a spelling wins and a clip nobody re-timed gets nothing from here.
    if let Some(speed) = drawn
        .iter()
        .find(|e| e.effect_kind() == Some(EffectKind::Speed) && !crate::fx_record::is_stop(e) && !e.snd.is_empty())
    {
        built.audio.extend(sound_of(clip, speed, footage));
    }

    // --- volume: one filter per cue, in turn ------------------------------------------------------------------------
    // §4's `after the lane mix, before the narration` is the CALLER's seam (see produce_render::audio_chain); this
    // only answers what goes there. The cues come back in seconds already divided by the clip's rate, so a duck under
    // a 2× stretch moves and narrows with the sound it rides.
    let cues = render_fx::all_gain_cues(&drawn, clip.seg_s, clip.rate, clip.on_screen);
    // §9's lane reach: a cue naming a lane runs BEFORE the lane mix, where that one
    // recording can still be taken apart, and the rest ride after it. One list per side
    // is what keeps the order honest — a lane-named duck appended to the summed bed would
    // have no lane to act on any more.
    let (before, after): (Vec<_>, Vec<_>) = cues
        .into_iter()
        .partition(|cue| cue.before_the_mix());
    for cue in &before {
        built.audio.push(render_fx::gain_chain(
            std::slice::from_ref(cue),
            &cue.lane,
        ));
    }
    if !after.is_empty() {
        built.audio.push(render_fx::gain_chain(&after, "snd"));
    }
    built
}

/// §4 (`a planned 1× read head with 0.15 s dips at run ends for own/scene`): the sound that stayed at 1× while the
/// picture ran ahead is read from the recording on its own clock, and the only thing that reaches the graph is where
/// it fades in and out — two real `afade`s at the run's ends, [`crate::fx_lane::SOUND_DIP_SECONDS`] apiece, computed
/// through [`render_fx::read_head`] so the dip length lives in one place. Pitch and mute answer with their own
/// filters instead, and both replace the plain seam dip rather than stacking on it.
fn sound_of(clip: &Clip, speed: &Fx, footage: f64) -> Vec<String> {
    match speed.snd.as_str() {
        "own" | "scene" => {
            let (in_at, out_at) = render_fx::read_head((0.0, footage));
            let dip = crate::fx_lane::SOUND_DIP_SECONDS;
            vec![
                format!("afade=t=in:st={in_at:.3}:d={dip}"),
                format!("afade=t=out:st={out_at:.3}:d={dip}"),
            ]
        }
        other => render_fx::sound_filters(clip.rate, other),
    }
}

/// §1's box fractions as pixels: `(x, y, w, h)` of the OUTPUT frame, positioned by its centre. Text defaults to the
/// lower third and svg to mid-frame — both already answered by [`Fx::centre`] — and a size nobody drew falls back to
/// that kind's shipped band (0.8×0.16 for a caption, 0.6×0.6 for a drawing, the prototype's numbers). Even sides
/// again: the overlay lands on a 4:2:0 picture.
pub fn box_px(effect: &Fx, frame: (i32, i32)) -> (i32, i32, i32, i32) {
    let (cx, cy) = effect.centre();
    let fallback = match effect.effect_kind() {
        Some(EffectKind::Text) => (crate::fx_text::LOWER_THIRD.wf, crate::fx_text::LOWER_THIRD.hf),
        _ => (crate::fx_svg::MIDDLE.wf, crate::fx_svg::MIDDLE.hf),
    };
    let wf = effect.wf.unwrap_or(fallback.0).clamp(crate::fx_text::MIN_WIDTH_FRACTION, 1.0);
    let hf = effect.hf.unwrap_or(fallback.1).clamp(crate::fx_text::MIN_HEIGHT_FRACTION, 1.0);
    let w = even((wf * frame.0 as f64).round() as i32);
    let h = even((hf * frame.1 as f64).round() as i32);    let x = ((cx * frame.0 as f64) - w as f64 / 2.0).max(0.0).round() as i32;
    let y = ((cy * frame.1 as f64) - h as f64 / 2.0).max(0.0).round() as i32;
    (x.min(frame.0 - w).max(0), y.min(frame.1 - h).max(0), w, h)
}

/// Seconds of FOOTAGE the clip plays through, which is what a camera walk and a read head are measured on — as
/// opposed to [`Clip::on_screen`], the shorter time it spends in the finished file. A rated clip shows `dur / rate`
/// seconds of film; a stop is not a rate (§4 means "a frame held") so it divides by 1 like the footage it covers.
fn footage_length(clip: &Clip) -> f64 {
    let rate = if clip.rate > 0.0 { clip.rate } else { 1.0 };
    let played = if (rate - 1.0).abs() > f64::EPSILON {
        clip.on_screen * rate
    } else {
        clip.on_screen
    };
    played.max(0.0)
}

/// Does this camera take the whole source? Then there is nothing to crop and nothing to scale beyond the frame box
/// the caller already applied.
fn fills_frame(sample: &render_fx::Sample) -> bool {
    sample.hf >= 1.0 - 1e-9
}

/// Round up to a whole even pixel: the overlay lands on a 4:2:0 picture, which refuses a chroma sample on an odd
/// edge — the same reason [`crate::render_fx::frame_box`] answers even sides.
fn even(v: i32) -> i32 {
    v + v % 2
}

/// The scratch name of one clip's generated title sheet. Numbered by position, not by second: two captions in the
/// same clip must not overwrite each other, and §3 lists these as scratch that is cleared every run.
fn title_name(no: usize, index: usize) -> String {
    format!("c{no:03}_t{index}.svg")
}

/// The scratch name of one clip's frozen frame. Same reasoning as [`title_name`].
fn still_name(no: usize, at: f64) -> String {
    format!("c{no:03}_f{at:.3}.png")
}

/// §4's drawing that is not there: named and skipped, in keeping with `Failure is specific and local`. Losing the
/// decoration costs less than losing the clip.
pub fn missing_drawing_log(no: usize) -> String {
    format!("clip {no}: its svg drawing is not there any more \u{2014} nothing is drawn over it")
}
