//! §06-effects#4-render: the render's half of every effect (`spec/06-effects.md` §4; its six bullets are read as S1..S6 and the frame-box rule as S7). Turns an effect record into filter TEXT and the arithmetic behind it; never runs ffmpeg ([`crate::subprocess`]) or touches a window.
//! Three §4 values live once below and are catalogued by `params::cut()` / `params::engineering()`: the animated camera grid, the `zoompan` depth cap, and the resolution tier.

use crate::cut::{EffectKind, Fx};
use crate::fx_lane;
use crate::fx_record;
use crate::fx_text;
use crate::fx_zoom;
use crate::produce_fx_chain::even;

/// §4 (`the resolution tier names the short side`): 1080p is 1080 on the SHORT edge (9:16 is 1080×1920), the one number that survives a change of shape.
pub const TIER_SHORT_SIDE: i32 = 1080;

/// S7 (`no aspect → the footage's own`): `cut.aspect` empty or `"source"` (both spellings arrive here).
pub fn frame_box(aspect: &str, src_w: i32, src_h: i32) -> (i32, i32) {
    if aspect.is_empty() || aspect == crate::fx_aspect::SOURCE {
        return (src_w, src_h);
    }
    // A string that is not `w:h` behaves like no aspect; a stray word in settings should not stop a render.
    let Some((w, h)) = ratio(aspect) else {
        return (src_w, src_h);
    };
    let short_side = TIER_SHORT_SIDE;
    if w <= h {
        // Tall shape: width is the tier's edge; a square lands here too (`tier_other` returns the short side for equal legs).
        return (even(short_side), even(tier_other(short_side, w, h)));
    }
    // Even sides: all offered encoders are 4:2:0.
    (even(tier_other(short_side, h, w)), even(short_side))
}

fn ratio(aspect: &str) -> Option<(i32, i32)> {
    let (w, h) = aspect.split_once(':')?;
    let w: i32 = w.trim().parse().ok()?;
    let h: i32 = h.trim().parse().ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

fn tier_other(short_side: i32, short_leg: i32, long_leg: i32) -> i32 {
    ((short_side as i64 * long_leg as i64) / short_leg.max(1) as i64) as i32
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub from: f64,
    pub to: f64,
    pub rate: f64,
    pub setpts: Option<String>,
}

/// S1 (`the middle carries the rate (setpts=PTS/rate)`): a stretch at 1× needs no filter.
pub fn setpts(rate: f64) -> Option<String> {
    if rate == 1.0 {
        return None;
    }
    Some(format!("setpts=PTS/{rate}"))
}

/// S1 (`sound by atempo chain`): tempo held, rates beyond one `atempo` (below 0.5, above 2 on old builds) reached by chaining halves and doublings.
pub fn atempo_chain(rate: f64) -> String {
    if rate <= 0.0 || rate == 1.0 {
        return String::new();
    }
    let mut rest = rate;
    let mut parts: Vec<String> = Vec::new();
    while rest < 0.5 {
        parts.push("atempo=0.5".into());
        rest *= 2.0;
    }
    while rest > 2.0 {
        parts.push("atempo=2".into());
        rest /= 2.0;
    }
    if rest != 1.0 {
        parts.push(format!("atempo={rest}"));
    }
    parts.join(",")
}

/// S1 (`asetrate when pitched`): the sound goes with the picture's clock by moving the sample rate and resampling back to 48 kHz.
pub fn asetrate_chain(rate: f64) -> String {
    if rate <= 0.0 || rate == 1.0 {
        return String::new();
    }
    format!(",asetrate=48000*{rate},aresample=48000")
}

/// S1 (`a mute expression when silent`): silence the whole bed, as the preview does (§06-effects §9 MUST row).
pub fn mute_expr(spans: &[(f64, f64)]) -> String {
    let windows: Vec<String> = spans
        .iter()
        .map(|(from, to)| format!("between(t\\,{from}\\,{to})"))
        .collect();
    format!("volume=0:enable='{}'", windows.join("+"))
}

/// S1 (`a planned 1× read head with 0.15 s dips at run ends`): `(in, out)` fade windows in the run's seconds, [`fx_lane::SOUND_DIP_SECONDS`] each.
pub fn read_head(run: (f64, f64)) -> (f64, f64) {
    let dip = fx_lane::SOUND_DIP_SECONDS;
    let len = (run.1 - run.0).max(0.0);
    // A run shorter than two dips shares what it has.
    let share = (len / 2.0).min(dip);
    (run.0 + share, run.1 - share)
}

/// S1: the sound of one sped stretch, one of §4's four answers by `snd` ([`crate::cut_speed`]: `""`, `"pitch"`, `"own"`, `"scene"`, `"mute"`).
pub fn sound_filters(rate: f64, snd: &str) -> Vec<String> {
    match snd {
        "pitch" => vec![asetrate_chain(rate).trim_start_matches(',').to_string()],
        "mute" => vec![mute_expr(&[(0.0, f64::MAX)])],
        // The sound stays at 1× while the picture runs on: only the dip windows are named (§4 "planned").
        "own" | "scene" => {
            let (in_at, out_at) = read_head((0.0, 1.0));
            vec![format!("read head at 1x: dips {in_at}..{out_at}")]
        }
        _ => {
            let chain = atempo_chain(rate);
            if chain.is_empty() {
                return Vec::new();
            }
            vec![chain]
        }
    }
}

/// S2 (`a still of the frame at t … overlaid with alpha fades for the frozen spans; footage runs on at 1×`): the freeze is not a rate; giving it one would make slow motion, which is why [`crate::cut_speed`] refuses to clamp it.
pub fn still_overlay(frozen: (f64, f64), fades: (f64, f64), frame: (i32, i32)) -> Overlay {
    let mut built = overlay(frozen, fades, (0, 0, frame.0, frame.1));
    built.prep.insert(
        0,
        format!("trim=end_frame=1,loop=loop=-1:size=1,setpts=N/{OVERLAY_FPS}/TB"),
    );
    built
}

/// §4 (`at fixed fps (default 30)`): the grid an animated camera is sampled on; the render's default rate, same as the preview.
pub const ZOOM_GRID_FPS: f64 = 30.0;

/// §4 (`> 10× capped`): how deep `zoompan` goes before it blurs to one pixel; deeper samples are pulled back and the log says so.
pub const ZOOM_DEPTH_CAP: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub t: f64,
    pub cx: f64,
    pub cy: f64,
    pub hf: f64,
}

/// §4 (`sampled at clip ends and every zoom's breakpoints, straight lines between`): a zoom has four moments (arriving, parked, letting go, gone); missing one makes a hold drift.
pub fn camera_samples(fx: &[Fx], sess_s: f64, span: f64) -> Vec<Sample> {
    let mut at = vec![sess_s, sess_s + span];
    for zoom in fx
        .iter()
        .filter(|e| e.effect_kind() == Some(EffectKind::Zoom))
    {
        let (tin, tout) = glides(zoom);
        let end = zoom.t + zoom.dur.max(0.0);
        at.extend([zoom.t, zoom.t + tin]);
        if !zoom.stay {
            at.extend([end - tout, end]);
        }
    }
    at.retain(|t| *t >= sess_s - 1e-6 && *t <= sess_s + span + 1e-6);
    at.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    at.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    at.iter()
        .map(|t| {
            let mut frame = fx_zoom::camera_at(fx, *t);
            // A staying zoom parks on its rect for good (§A.1), so a later sample is that rect; `camera_at` knows this only up to the zoom's end. Render and preview read the same walk.
            if let Some(last) = fx
                .iter()
                .filter(|e| e.effect_kind() == Some(EffectKind::Zoom) && e.stay && *t >= e.t)
                .max_by(|a, b| a.t.total_cmp(&b.t))
            {
                frame = fx_zoom::Camera {
                    cx: last.cx.unwrap_or(frame.cx),
                    cy: last.cy.unwrap_or(frame.cy),
                    hf: last.hf.unwrap_or(frame.hf),
                };
            }
            Sample {
                t: *t,
                cx: frame.cx,
                cy: frame.cy,
                hf: frame.hf,
            }
        })
        .collect()
}

fn glides(zoom: &Fx) -> (f64, f64) {
    let dur = zoom.dur.max(0.0);
    let tin = zoom.trans.max(0.0).min(dur);
    (tin, zoom.tout.max(0.0).min(dur - tin))
}

pub fn is_static(samples: &[Sample]) -> bool {
    samples
        .get(1)
        .is_none_or(|first| samples.iter().all(|s| near(s, first)))
}

fn near(a: &Sample, b: &Sample) -> bool {
    (a.cx - b.cx).abs() < 1e-9 && (a.cy - b.cy).abs() < 1e-9 && (a.hf - b.hf).abs() < 1e-9
}

/// §4: the filters that realise the camera. A still camera is two; a moving one pads black where a rectangle reaches past the source, crops to the union of every rectangle grown to the output shape, and runs `zoompan` on piecewise-linear expressions of input time.
pub fn camera_chain(samples: &[Sample], src: (i32, i32), frame: (i32, i32)) -> Vec<String> {
    if samples.is_empty() {
        return vec![format!("scale={}:{}", frame.0, frame.1)];
    }
    let out_a = frame.0 as f64 / frame.1 as f64;
    if is_static(samples) {
        let (x, y, w, h) = rect_px(&samples[0], src, out_a);
        return vec![
            format!(
                "crop={}:{}:{}:{}",
                even_f(w),
                even_f(h),
                clip_int(x, src.0 - even_f(w)),
                clip_int(y, src.1 - even_f(h))
            ),
            format!("scale={}:{}", frame.0, frame.1),
        ];
    }
    let mut chain = Vec::new();
    if let Some((w, h, l, t)) = pad_box(samples, src, out_a) {
        chain.push(format!("pad={w}:{h}:{l}:{t}:color=black"));
    }
    let ts: Vec<f64> = samples.iter().map(|s| s.t).collect();
    let along = |pick: fn(&Sample) -> f64| {
        let vs: Vec<f64> = samples.iter().map(pick).collect();
        piece_expr(&ts, &vs)
    };
    // `it` runs on the stream's own clock, so `t` is from the clip's start. d=1: one frame out per frame in.
    chain.push(format!(
        "zoompan=z=1/({hf}):x=({cx})*iw-iw/zoom/2:y=({cy})*ih-ih/zoom/2:d=1:s={w}x{h}:fps={ZOOM_GRID_FPS}",
        hf = along(|s| s.hf),
        cx = along(|s| s.cx),
        cy = along(|s| s.cy),
        w = frame.0,
        h = frame.1,
    ));
    chain
}

fn rect_px(s: &Sample, src: (i32, i32), out_a: f64) -> (f64, f64, f64, f64) {
    let h = s.hf * src.1 as f64;
    let w = h * out_a;
    (
        s.cx * src.0 as f64 - w / 2.0,
        s.cy * src.1 as f64 - h / 2.0,
        w,
        h,
    )
}

/// §4 (`pad black as needed`): only when a rectangle reaches past the source, else every frame is rescaled for nothing.
fn pad_box(samples: &[Sample], src: (i32, i32), out_a: f64) -> Option<(i32, i32, i32, i32)> {
    let mut left = 0.0_f64;
    let mut top = 0.0_f64;
    let mut right = 0.0_f64;
    let mut bottom = 0.0_f64;
    for s in samples {
        let (x, y, w, h) = rect_px(s, src, out_a);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + w);
        bottom = bottom.max(y + h);
    }
    if left >= 0.0 && top >= 0.0 && right <= src.0 as f64 && bottom <= src.1 as f64 {
        return None;
    }
    let l = (-left).max(0.0).ceil() as i32;
    let t = (-top).max(0.0).ceil() as i32;
    let r = (right - src.0 as f64).max(0.0).ceil() as i32;
    let b = (bottom - src.1 as f64).max(0.0).ceil() as i32;
    Some((even(src.0 + l + r), even(src.1 + t + b), even(l), even(t)))
}

fn even_f(v: f64) -> i32 {
    let n = v.round() as i32;
    even(n)
}

fn clip_int(v: f64, max: i32) -> i32 {
    (v.round() as i32).clamp(0, max.max(0))
}

/// §4 (`piecewise-linear expressions`): a value over input time as nested `if`s, flat past either end; exact because the camera is a straight lerp between breakpoints, as in the preview.
pub fn piece_expr(ts: &[f64], vs: &[f64]) -> String {
    let n = ts.len().min(vs.len());
    if n == 0 {
        return "0".into();
    }
    let mut expr = format!("{:.4}", vs[n - 1]);
    for i in (0..n - 1).rev() {
        let (t0, t1, v0, v1) = (ts[i], ts[i + 1], vs[i], vs[i + 1]);
        let seg = if t1 - t0 < 1e-4 || (v1 - v0).abs() < 1e-9 {
            format!("{v1:.4}")
        } else {
            format!(
                "{v0:.4}+{:.4}*clip((it-{t0:.4})/{:.4}\\,0\\,1)",
                v1 - v0,
                t1 - t0
            )
        };
        expr = format!("if(lt(it\\,{t1:.4})\\,{seg}\\,{expr})");
    }
    expr
}

/// §4: how deep the moving window goes, in box heights per window (`zoompan`'s measure); the deepest is read against the cap.
pub fn zoom_depths(samples: &[Sample], src: (i32, i32), frame: (i32, i32)) -> Vec<f64> {
    let out_a = frame.0 as f64 / frame.1 as f64;
    let box_h = union_height(samples, src, out_a);
    samples
        .iter()
        .map(|s| {
            let (_, _, _, h) = rect_px(s, src, out_a);
            box_h / h.max(1.0)
        })
        .collect()
}

/// The union of every rectangle's vertical reach, the box `zoompan` works inside (grown by [`camera_chain`]).
fn union_height(samples: &[Sample], src: (i32, i32), out_a: f64) -> f64 {
    let mut top = f64::MAX;
    let mut bottom = f64::MIN;
    for s in samples {
        let (_, y, _, h) = rect_px(s, src, out_a);
        top = top.min(y);
        bottom = bottom.max(y + h);
    }
    (bottom - top).max(1.0)
}

/// §4 (`> 10× capped and logged`): samples with nothing deeper than [`ZOOM_DEPTH_CAP`], and the log line when pulled back. Only too-deep windows are raised, so shallow frames are kept. Wording is the prototype's.
pub fn cap_zoom(
    samples: &[Sample],
    src: (i32, i32),
    frame: (i32, i32),
    clip_no: usize,
) -> (Vec<Sample>, Option<String>) {
    let depths = zoom_depths(samples, src, frame);
    let deepest = depths.iter().fold(0.0_f64, |a, b| a.max(*b));
    if deepest <= ZOOM_DEPTH_CAP {
        return (samples.to_vec(), None);
    }
    let box_h = union_height(samples, src, frame.0 as f64 / frame.1 as f64);
    let capped: Vec<Sample> = samples
        .iter()
        .zip(depths.iter())
        .map(|(s, depth)| {
            if *depth <= ZOOM_DEPTH_CAP {
                return *s;
            }
            Sample {
                t: s.t,
                cx: s.cx,
                cy: s.cy,
                hf: box_h / ZOOM_DEPTH_CAP / src.1.max(1) as f64,
            }
        })
        .collect();
    (capped, Some(depth_log(clip_no)))
}

pub fn depth_log(clip_no: usize) -> String {
    format!("clip {clip_no}: the zoom goes deeper than ffmpeg's 10× \u{2014} it is rendered at 10×")
}

pub const OVERLAY_FPS: u32 = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub prep: Vec<String>,
    pub place: String,
}

/// §4: one title or drawing with alpha fades and an enable window. A zero-length fade gets no filter (`d=0` means 25 frames); `shortest` because the input loops.
pub fn overlay(enable: (f64, f64), fades: (f64, f64), box_px: (i32, i32, i32, i32)) -> Overlay {
    let mut prep = vec![
        "format=rgba".to_string(),
        format!("scale={}:{}", box_px.2, box_px.3),
    ];
    if fades.0 > 0.0 {
        prep.push(format!("fade=t=in:st={}:d={}:alpha=1", enable.0, fades.0));
    }
    if fades.1 > 0.0 {
        prep.push(format!(
            "fade=t=out:st={}:d={}:alpha=1",
            enable.1 - fades.1,
            fades.1
        ));
    }
    Overlay {
        prep,
        place: format!(
            "overlay={}:{}:shortest=1:enable=between(t\\,{}\\,{})",
            box_px.0, box_px.1, enable.0, enable.1
        ),
    }
}

/// §4 (`title: generated transparent frame-sized SVG, black round-joined stroke then white fill`) and F3.4 S4 (`same function for preview and render`): the sheet is laid out by [`crate::fx_text`] (size, line breaks, baselines, edge thickness), as the preview ([`crate::ui::cut_preview_caption`]), so wrapping matches. `w`/`h` are the BOX in px ([`crate::produce_fx_chain::box_px`]); `overlay_chain` scales the sheet into it. Stroke is written before fill (SVG paints in document order), or the letters eat their edge.
pub fn title_svg(words: &str, w: i32, h: i32) -> String {
    // Colours as words: `#` starts a fragment in SVG.
    let (size, lines) = fx_text::fit(words, w as f64, h as f64);
    // Every number below comes from `fx_text`, shared with the preview. Bold sans-serif (`cairo::FontWeight::Bold`, §S4) on both halves.
    let mut texts = String::new();
    if size > 0.0 {
        let ys = fx_text::baselines(0.0, h as f64, size, lines.len());
        for (line, y) in lines.iter().zip(ys.iter()) {
            if line.is_empty() {
                continue;
            }
            texts.push_str(&format!(
                "<text x=\"{cx}\" y=\"{y:.3}\">{}</text>",
                crate::cut_cards::xml_escape(line),
                cx = w / 2
            ));
        }
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\
         <g font-family=\"sans-serif\" font-weight=\"bold\" font-size=\"{size:.3}\" text-anchor=\"middle\" \
         paint-order=\"stroke\" stroke-linejoin=\"round\" stroke=\"black\" stroke-width=\"{stroke:.3}\" fill=\"white\">\
         {texts}</g></svg>",
        stroke = fx_text::render_stroke_width(size),
    )
}

#[derive(Debug, Clone, PartialEq)]
/// §9 (`A volume SHOULD carry `lane`: "" the whole bed (today), or one lane's name, gained before the mix`): a cue's reach and graph place as ONE pair. A naming cue acts on one recording before the mix; an unnamed one is the bed, after it. The render (`render_fx::all_gain_cues`) and preview (`cut_hear::mix_at`) both take their halves from here.
pub struct Cue {
    pub from: f64,
    pub to: f64,
    pub gain: f64,
    pub fin: f64,
    pub fout: f64,
    pub lane: String,
}

impl Cue {
    /// [`crate::effect_overlap::gained_before_the_mix`]: a cue naming one lane runs before the lanes are summed.
    pub fn before_the_mix(&self) -> bool {
        crate::effect_overlap::gained_before_the_mix(&self.as_effect())
    }

    /// [`crate::effect_overlap::gain_reaches`]: does this cue touch the lane named `lane`? Asked per clip, so a duck on a lane nobody hears is not work.
    pub fn reaches(&self, lane: &str) -> bool {
        crate::effect_overlap::gain_reaches(&self.as_effect(), lane)
    }

    /// [`crate::effect_overlap::gain_reaches_the_clip`]: whether it acts on a clip hearing exactly `heard`; a card's own sound is gained (a whole-bed cue needs no heard lane).
    pub fn reaches_the_clip(&self, heard: &[String]) -> bool {
        crate::effect_overlap::gain_reaches_the_clip(&self.as_effect(), heard)
    }

    /// The cue as the record it came from, so §9's reach predicates have one wording in plan and encode.
    pub fn as_effect(&self) -> Fx {
        Fx {
            kind: "volume".to_string(),
            t: self.from,
            dur: (self.to - self.from).max(0.0),
            gain: self.gain,
            lane: self.lane.clone(),
            ..Default::default()
        }
    }
}

/// Every volume cue, including lane-naming ones; [`gain_cues`] is the bed's half, split via [`Cue::before_the_mix`].
pub fn all_gain_cues(fx: &[Fx], sess_s: f64, rate: f64, length: f64) -> Vec<Cue> {
    if length <= 0.0 || rate <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for effect in fx
        .iter()
        .filter(|e| e.effect_kind() == Some(EffectKind::Volume))
    {
        let start = (effect.t - sess_s) / rate;
        let dur = effect.dur.max(0.0) / rate;
        let (from, to) = (start.clamp(0.0, length), (start + dur).clamp(0.0, length));
        if to - from < 1.0 / ZOOM_GRID_FPS {
            continue;
        }
        let fin = if start >= 0.0 {
            (effect.trans.max(0.0) / rate).min(to - from)
        } else {
            0.0
        };
        out.push(Cue {
            from,
            to,
            gain: effect.gain.clamp(0.0, fx_record::gain_ceiling()),
            fin,
            fout: if start + dur <= length {
                (effect.tout.max(0.0) / rate).min(to - from - fin)
            } else {
                0.0
            },
            lane: effect.lane().unwrap_or_default().to_string(),
        });
    }
    out
}

/// S5: one cue as an expression in the clip's seconds: the gain across the band, 1 elsewhere, a ramp over each fade. The multiplier carries an explicit sign (`1-0.5000*…`, never `1+-0.5000*…`).
pub fn gain_expr(cue: &Cue) -> String {
    let g = cue.gain.clamp(0.0, fx_record::gain_ceiling());
    let mut ramps = Vec::new();
    if cue.fin > 0.0 {
        ramps.push(format!("(t-{:.3})/{:.3}", cue.from, cue.fin));
    }
    if cue.fout > 0.0 {
        ramps.push(format!("({:.3}-t)/{:.3}", cue.to, cue.fout));
    }
    let value = match ramps.len() {
        1 => format!("1{:+.4}*min(1,{})", g - 1.0, ramps[0]),
        2 => format!("1{:+.4}*min(1,min({},{}))", g - 1.0, ramps[0], ramps[1]),
        _ => format!("{:.4}", g),
    };
    format!("if(between(t,{:.3},{:.3}),{},1)", cue.from, cue.to, value)
}

/// S5 (`per cue, in turn`): cues applied one after another, so overlapping ones multiply, as in the preview's mix.
pub fn gain_filters(cues: &[Cue]) -> Vec<String> {
    cues.iter()
        .map(|cue| format!("volume=volume='{}':eval=frame", gain_expr(cue)))
        .collect()
}

/// §4 (`Label: nothing`): a label marks a moment for the narration brief and is never drawn; its filter is an empty string.
pub fn is_rendered(kind: &str) -> bool {
    if kind == "label" {
        return false;
    }
    // The kinds §4 gives a filter to, written out rather than via `EffectKind::parse` (which would accept a label).
    matches!(kind, "zoom" | "speed" | "stop" | "text" | "svg" | "volume")
}
