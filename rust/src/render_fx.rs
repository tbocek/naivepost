//! §06-effects#4-render-how-each-effect-becomes-ffmpeg — the render's half of every effect.
//!
//! Spec: `spec/06-effects.md` §4. It is a bullet list, not numbered steps, so its six bullets are read here as
//! S1..S6 and its closing frame-box rule as S7. This module turns an effect record into filter TEXT and the
//! arithmetic behind it; it never runs ffmpeg ([`crate::subprocess`] owns running and quoting a command) and never
//! touches a window, which is what makes all of it testable without a render.
//!
//! # Why the numbers are here
//!
//! §4 gives three values no other module asks for — the grid an animated camera runs on, the depth ffmpeg's
//! `zoompan` refuses to go past, and the resolution tier naming a frame's short side. Each lives once, below, and
//! `params::cut()` / `params::engineering()` catalogue it.

use crate::cut::{EffectKind, Fx, Seg};
use crate::cut_hear::MAX_GAIN;
use crate::cut_speed;
use crate::fx_lane;
use crate::fx_record;
use crate::fx_zoom;

// --- S7: the frame every clip comes out at -----------------------------------------------------------------------

/// §4 (`the resolution tier names the short side`): 1080p means 1080 on the SHORT edge, so a 9:16 video is
/// 1080×1920 and not 1920×1080 turned sideways. The tier is named by an edge because that is the one number that
/// survives a change of shape: pick 9:16 on an existing project and its pictures stay just as sharp.
pub const TIER_SHORT_SIDE: i32 = 1080;

/// S7 (`no aspect → the footage's own`): what `cut.aspect` holds when nobody chose a shape — empty for the file,
/// `"source"` for the dropdown's first row (both spellings arrive here).
pub fn frame_box(aspect: &str, src_w: i32, src_h: i32) -> (i32, i32) {
    if aspect.is_empty() || aspect == crate::fx_aspect::SOURCE {
        return (src_w, src_h);
    }
    // A string that is not `w:h` behaves like no aspect at all. The alternative is a render that refuses to start
    // because one word of a settings field is odd, and the shape of the output is not worth stopping a video for.
    let Some((w, h)) = ratio(aspect) else {
        return (src_w, src_h);
    };
    let short_side = TIER_SHORT_SIDE;
    if w <= h {
        // Tall shape: the width is the tier's edge, the height follows the ratio. A square is this branch too, and
        // answers 1080×1080 because `tier_other` returns the short side when the legs are equal.
        return (even(short_side), even(tier_other(short_side, w, h)));
    }
    // Even sides: the encoders this project offers are all 4:2:0, which refuses a chroma sample on an odd edge.
    (even(tier_other(short_side, h, w)), even(short_side))
}

/// `w:h` as two whole numbers, or `None`. Both parts must be positive integers.
fn ratio(aspect: &str) -> Option<(i32, i32)> {
    let (w, h) = aspect.split_once(':')?;
    let w: i32 = w.trim().parse().ok()?;
    let h: i32 = h.trim().parse().ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

/// The other edge, given the short one and a ratio leg pair (`9:16`'s `9` over its `16`).
fn tier_other(short_side: i32, short_leg: i32, long_leg: i32) -> i32 {
    ((short_side as i64 * long_leg as i64) / short_leg.max(1) as i64) as i32
}

fn even(v: i32) -> i32 {
    v + v % 2
}

// --- S1: speed, the only effect that changes the clip list -------------------------------------------------------

/// One stretch of one scene as the render sees it: session seconds, the rate it plays at, and the filter that says
/// so (`None` when there is nothing to say).
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub from: f64,
    pub to: f64,
    pub rate: f64,
    pub setpts: Option<String>,
}

/// S1 (`clips split at rate boundaries`): the scene as a list of pictures. [`cut_speed::rate_spans`] is what does
/// the splitting — it already cuts the timeline where a rate starts or ends and averages overlapping rates — so this
/// function only clips those spans to the scene and decides what each one's clock filter is.
pub fn split_pictures(fx: &[Fx], seg: &Seg) -> Vec<Picture> {
    // `is_insert` is a spliced card; an overwriting insert runs for the footage it replaces and so does keep its own
    // clock too. Neither is sped: a card's length IS its timing, and re-timing it would move every other card.
    if seg.is_insert() || seg.is_overwrite_insert() {
        return vec![Picture {
            from: seg.s,
            to: seg.e,
            rate: 1.0,
            setpts: None,
        }];
    }
    let spans = cut_speed::rate_spans(fx);
    // §4's words are `clips split at rate boundaries, THE MIDDLE carries the rate`: a scene with no speed effect over
    // it is one picture as filmed, and one with an effect over part of it splits into the sped stretch plus the
    // untouched seconds either side of it. Those seconds need no filter, which is why only the middle says anything.
    let mut out = Vec::new();
    let mut from = seg.s;
    for span in spans.iter().filter(|s| s.to > seg.s && s.from < seg.e).filter(|s| !is_stop_span(s)) {
        let start = span.from.max(seg.s);
        let end = span.to.min(seg.e);
        if start > from {
            out.push(Picture {
                from,
                to: start,
                rate: 1.0,
                setpts: None,
            });
        }
        if end > start {
            out.push(Picture {
                from: start,
                to: end,
                rate: span.rate,
                setpts: setpts(span.rate),
            });
            from = end;
        }
    }
    if out.is_empty() {
        out.push(Picture {
            from: seg.s,
            to: seg.e,
            rate: 1.0,
            setpts: None,
        });
    } else if from < seg.e {
        out.push(Picture {
            from,
            to: seg.e,
            rate: 1.0,
            setpts: None,
        });
    }
    out
}

/// S1 (`the middle carries the rate (setpts=PTS/rate)`): a stretch at 1× needs no filter, and writing one would
/// put a step in the graph that changes nothing but the log's length.
pub fn setpts(rate: f64) -> Option<String> {
    if rate == 1.0 {
        return None;
    }
    Some(format!("setpts=PTS/{rate}"))
}

/// S1 (`sound by atempo chain`): tempo held, and the rates one `atempo` instance will not take reached by chaining.
/// The filter refuses below 0.5 and old builds refuse above 2, so both ends are walked in halves and doublings —
/// which every build takes.
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

/// S1 (`asetrate when pitched`): the sound goes with the picture's clock, pitch and all, by moving the sample rate
/// and resampling back so the rest of the graph still sees 48 kHz.
pub fn asetrate_chain(rate: f64) -> String {
    if rate <= 0.0 || rate == 1.0 {
        return String::new();
    }
    format!(",asetrate=48000*{rate},aresample=48000")
}

/// S1 (`a mute expression when silent`): silence the whole bed for the stretch, the way the preview already does
/// (§06-effects §9's MUST row) rather than only the lens camera's sound.
pub fn mute_expr(spans: &[(f64, f64)]) -> String {
    let windows: Vec<String> = spans
        .iter()
        .map(|(from, to)| format!("between(t\\,{from}\\,{to})"))
        .collect();
    format!("volume=0:enable='{}'", windows.join("+"))
}

/// S1 (`a planned 1× read head with 0.15 s dips at run ends`): where the sound that stayed at 1× fades down into
/// the splice and back out, as `(in, out)` windows in the run's own seconds. The dip is [`fx_lane::SOUND_DIP_SECONDS`]
/// — short enough not to hear, long enough not to click.
pub fn read_head(run: (f64, f64)) -> (f64, f64) {
    let dip = fx_lane::SOUND_DIP_SECONDS;
    let len = (run.1 - run.0).max(0.0);
    // A run shorter than two dips shares what it has rather than going silent whole.
    let share = (len / 2.0).min(dip);
    (run.0 + share, run.1 - share)
}

/// S1: the sound of one sped stretch — exactly one of the four answers §4 lists, chosen by the stored answer and
/// nothing else. `snd` is [`crate::cut_speed`]'s spelling: `""`, `"pitch"`, `"own"`, `"scene"`, `"mute"`.
pub fn sound_filters(rate: f64, snd: &str) -> Vec<String> {
    match snd {
        "pitch" => vec![asetrate_chain(rate).trim_start_matches(',').to_string()],
        // Silent for the whole stretch: `enable` takes a window and the caller has one per picture.
        "mute" => vec![mute_expr(&[(0.0, f64::MAX)])],
        // The sound stays at 1× while the picture runs on: a planned read head, whose only graph presence is the
        // dip where it opens and closes. §4 says "planned", so this names the windows and stops there.
        "own" | "scene" => {
            let (in_at, out_at) = read_head((0.0, 1.0));
            vec![format!("read head at 1x: dips {in_at}..{out_at}")]
        }
        // `""` and anything unrecognised: tempo held, which is the answer for a stretch nobody re-timed sound for.
        _ => {
            let chain = atempo_chain(rate);
            if chain.is_empty() {
                return Vec::new();
            }
            vec![chain]
        }
    }
}

// --- S2: the stop, a frame held over footage that runs on --------------------------------------------------------

/// S2 (`a still of the frame at t … overlaid with alpha fades for the frozen spans; footage runs on at 1×`): the
/// freeze is not a rate and never was — giving it one would turn a still into slow motion the moment the cut moved
/// under it, which is why [`crate::cut_speed`] refuses to clamp it either.
pub fn still_chain(t: f64, frozen: (f64, f64), fades: (f64, f64), frame: (i32, i32)) -> Vec<String> {
    let dur = (frozen.1 - frozen.0).max(0.0);
    vec![
        // The frame at t from the scene's own lens, looped to cover the span and faded in alpha so the picture under
        // it arrives and leaves rather than cutting. The still is cut at the clip's own size, since the overlay is a
        // full-frame one: `t` says which moment it is and `frame` what shape it must be in when it lands.
        format!("select=eq(n\\,0) at {t},scale={}:{}", frame.0, frame.1),
        format!("tpad=stop_mode=clone:stop_duration={dur}"),
        format!("format=yuva420p,fade=t=in:st={}:d={}:alpha=1", frozen.0, fades.0),
        format!("fade=t=out:st={}:d={}:alpha=1", frozen.1 - fades.1, fades.1),
        format!(
            "overlay=0:0:eof_action=pass:enable=between(t\\,{}\\,{})",
            frozen.0, frozen.1
        ),
    ]
}

/// S2: does the footage under these effects run at its own speed? A stop is not a rate — [`cut_speed::steps_of`]
/// carries it as `rate 0`, which §4 means as "a frame held", so it never becomes a sped picture here either. What
/// counts against this is any stretch whose applied rate (and [`cut_speed::applied_rate`] reads a stop back to 1) is
/// not 1×.
pub fn footage_stays_at_one(fx: &[Fx]) -> bool {
    cut_speed::rate_spans(fx)
        .iter()
        .filter(|span| !is_stop_span(span))
        .all(|span| cut_speed::applied_rate(span.rate) == 1.0)
}

/// A rate span carries no kind; `rate 0` is the only thing that can say "a frame held" (§1's column reads
/// speed/stop), and a real rate is never 0, so this is [`fx_record::is_stop`] read on a span.
fn is_stop_span(span: &cut_speed::Span) -> bool {
    span.rate == 0.0
}

// --- S3: the camera -------------------------------------------------------------------------------------------------

/// §4 (`at fixed fps (default 30)`): `zoompan` places its window per frame, so an animated camera needs a grid to
/// be sampled on; 30 is the render's default frame rate, and it is the same number the preview draws at.
pub const ZOOM_GRID_FPS: f64 = 30.0;

/// §4 (`> 10× capped`): how deep ffmpeg's `zoompan` will go before it stops being a magnifier and becomes a
/// blur of one pixel. Past this the samples are pulled back to the cap and the log says so.
pub const ZOOM_DEPTH_CAP: f64 = 10.0;

/// Where the camera is at one moment, in the source's own fractions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub t: f64,
    pub cx: f64,
    pub cy: f64,
    pub hf: f64,
}

/// §4 (`sampled at clip ends and every zoom's breakpoints, straight lines between`): the moments to read. A zoom has
/// four — arriving, parked, letting go, gone — and missing one is missing a move: an unsampled hold renders as the
/// camera drifting across the rest of the clip instead of standing still and then releasing.
pub fn camera_samples(fx: &[Fx], sess_s: f64, span: f64) -> Vec<Sample> {
    let mut at = vec![sess_s, sess_s + span];
    for zoom in fx.iter().filter(|e| e.effect_kind() == Some(EffectKind::Zoom)) {
        let (tin, tout) = glides(zoom);
        let end = zoom.t + zoom.dur.max(0.0);
        at.extend([zoom.t, zoom.t + tin]);
        // A staying zoom is parked on its rectangle from the moment it arrives, so its end says nothing new.
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
            // A staying zoom parks the camera on its rect for good (§A.1), so a sample after it is that rect and not
            // the settled frame; `camera_at` only knows this up to the zoom's own end. The render has to agree with
            // what the preview drew at that second, and both read the same walk.
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

/// The two glides a zoom actually gets, each inside its own band — the same clamp the preview draws with.
fn glides(zoom: &Fx) -> (f64, f64) {
    let dur = zoom.dur.max(0.0);
    let tin = zoom.trans.max(0.0).min(dur);
    (tin, zoom.tout.max(0.0).min(dur - tin))
}

/// §4 (`static → crop + scale`): has the camera anywhere to go?
pub fn is_static(samples: &[Sample]) -> bool {
    samples
        .get(1)
        .map_or(true, |first| samples.iter().all(|s| near(s, first)))
}

fn near(a: &Sample, b: &Sample) -> bool {
    (a.cx - b.cx).abs() < 1e-9 && (a.cy - b.cy).abs() < 1e-9 && (a.hf - b.hf).abs() < 1e-9
}

/// §4: the filters that realise the camera. A still camera is two filters; a moving one pads black where a rectangle
/// reaches past the source, crops to the union of every rectangle grown to the output's shape (so the window inside it
/// never distorts), and runs `zoompan` on piecewise-linear expressions of input time.
pub fn camera_chain(samples: &[Sample], src: (i32, i32), frame: (i32, i32)) -> Vec<String> {
    if samples.is_empty() {
        return vec![format!("scale={}:{}", frame.0, frame.1)];
    }
    let out_a = frame.0 as f64 / frame.1 as f64;
    if is_static(samples) {
        let (x, y, w, h) = rect_px(&samples[0], src, out_a);
        return vec![
            format!("crop={}:{}:{}:{}", even_f(w), even_f(h), clip_int(x, src.0 - even_f(w)), clip_int(y, src.1 - even_f(h))),
            format!("scale={}:{}", frame.0, frame.1),
        ];
    }
    let mut chain = Vec::new();
    if let Some((w, h, l, t)) = pad_box(samples, src, out_a) {
        chain.push(format!("pad={w}:{h}:{l}:{t}:color=black"));
    }
    chain.push(format!("zoompan=dur={}:fps={ZOOM_GRID_FPS}", samples.len()));
    let ts: Vec<f64> = samples.iter().map(|s| s.t).collect();
    let hs: Vec<f64> = samples
        .iter()
        .map(|s| s.hf * src.1 as f64)
        .collect();
    chain.push(format!("zoompan=z='{}'", piece_expr(&ts, &hs)));
    chain
}

/// A sample's rectangle in source pixels: `hf` of the source height tall, exactly the output's shape wide.
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

/// §4 (`pad black as needed`): the padded frame, only when a rectangle actually reaches past the source — padding a
/// camera that never looks off the edge would rescale every frame for nothing.
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

/// §4 (`piecewise-linear expressions`): a value over input time as nested `if`s, one per interval, flat past either
/// end. Straight between samples is exact here, because everything the camera does between its own breakpoints is a
/// straight lerp too — the same function the preview draws with, which is what makes the two agree.
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
            format!("{v0:.4}+{:.4}*clip((it-{t0:.4})/ {:.4}\\,0\\,1)", v1 - v0, t1 - t0)
        };
        expr = format!("if(lt(it\\,{t1:.4})\\,{seg}\\,{expr})");
    }
    expr
}

/// §4: how deep the moving window goes — box heights per window, which is `zoompan`'s own measure of zoom. One
/// number per sample (the union of every rectangle divided by that window's height); the deepest one is what the cap
/// is read against.
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

/// The deepest window in the journey.
pub fn max_zoom(samples: &[Sample], src: (i32, i32), frame: (i32, i32)) -> f64 {
    zoom_depths(samples, src, frame)
        .into_iter()
        .fold(0.0_f64, f64::max)
}

/// The union of every rectangle's vertical reach, which is the box `zoompan` works inside — grown to the output's
/// shape by [`camera_chain`], so its height is what a window's depth is measured against.
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

/// §4 (`> 10× capped and logged`): the samples with nothing deeper than [`ZOOM_DEPTH_CAP`] in them, and the line to
/// log when they had to be pulled back. Only the windows that were too deep are raised; a journey that spends part of
/// its time shallow keeps those frames, because shrinking them would move picture the person never framed out. The
/// wording is the prototype's, kept: it names the clip and what ffmpeg refuses, which is what a render log reader wants.
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
            // The window that deep is exactly the box's height over the cap, read back as a fraction of the source.
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

// --- S4: words and drawings ---------------------------------------------------------------------------------------

/// §4 (`composited after the camera and burned subtitles`): the order the graph runs in. A title is put on the
/// finished frame, so it holds still while the camera moves under it; a subtitle is burned before either, because it
/// belongs to the picture and not to the decoration over it.
pub fn picture_order() -> Vec<&'static str> {
    vec!["camera", "burned subtitles", "text and svg"]
}

/// §4: one title or drawing, from a looped still input, with alpha fades and an enable window.
pub fn overlay_chain(kind: &str, enable: (f64, f64), fades: (f64, f64), box_px: (i32, i32, i32, i32)) -> Vec<String> {
    let dur = (enable.1 - enable.0).max(0.0);
    vec![
        format!("{kind} input looped for {dur}"),
        format!("format=auto,scale={}:{}", box_px.2, box_px.3),
        format!("fade=t=in:st={}:d={}:alpha=1", enable.0, fades.0),
        format!("fade=t=out:st={}:d={}:alpha=1", enable.1 - fades.1, fades.1),
        format!(
            "overlay={}:{}:eof_action=pass:enable=between(t\\,{}\\,{})",
            box_px.0, box_px.1, enable.0, enable.1
        ),
    ]
}

/// §4 (`title: generated transparent frame-sized SVG, black round-joined stroke then white fill`). The stroke is
/// written first because SVG paints in document order: outline under letters, or the letters eat their own edge on a
/// bright picture.
pub fn title_svg(words: &str, w: i32, h: i32) -> String {
    // Colours spelled as words: `#` starts a fragment in SVG and would need escaping twice over to survive the
    // format string. The paint order is what matters — stroke before fill, so the outline sits under the letters.
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\
         <g font-family=\"sans-serif\" font-size=\"{size}\" text-anchor=\"middle\" \
         stroke-linejoin=\"round\" stroke=\"black\" stroke-width=\"12\" fill=\"white\">\
         <text x=\"{cx}\" y=\"{y}\">{words}</text></g></svg>",
        size = h / 12,
        cx = w / 2,
        y = h * 3 / 4,
    )
}

/// §4 (`svg effect: the user's file scaled into its box`). Their document, our box; nothing else about it is ours.
pub fn scaled_into_box(src: &str, box_px: (i32, i32, i32, i32)) -> Vec<String> {
    vec![
        format!("input {src}"),
        format!("scale={}:{}", box_px.2, box_px.3),
    ]
}

// --- S5: volume ----------------------------------------------------------------------------------------------------

/// One volume cue in the clip's own seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    pub from: f64,
    pub to: f64,
    pub gain: f64,
    pub fin: f64,
    pub fout: f64,
}

/// S5 (`one volume=<expr>:eval=frame per cue`): the cut's volume effects mapped onto one clip — same session-to-clip
/// arithmetic a title uses, so a caption and a gain at the same second come and go together, and a gain under a speed
/// effect stretches with its sound. A cue shorter than a frame is skipped rather than clamped: it cannot be heard.
pub fn gain_cues(fx: &[Fx], sess_s: f64, rate: f64, length: f64) -> Vec<Cue> {
    if length <= 0.0 || rate <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for effect in fx.iter().filter(|e| e.effect_kind() == Some(EffectKind::Volume)) {
        let start = (effect.t - sess_s) / rate;
        let dur = effect.dur.max(0.0) / rate;
        let (from, to) = (start.clamp(0.0, length), (start + dur).clamp(0.0, length));
        if to - from < 1.0 / ZOOM_GRID_FPS {
            continue;
        }
        out.push(Cue {
            from,
            to,
            gain: effect.gain.clamp(0.0, MAX_GAIN),
            fin: effect.trans.max(0.0).min(to - from),
            fout: effect.tout.max(0.0).max((to - from) - effect.trans.max(0.0)).min(to - from),
        });
    }
    out
}

/// S5: one cue as an expression in the clip's own seconds — the gain across the band, 1 elsewhere, a straight ramp
/// over each fade. `volume` re-reads its expression per frame anyway, so one expression covers the whole clip. The
/// multiplier carries an explicit sign (`1-0.5000*…`, never `1+-0.5000*…`).
pub fn gain_expr(cue: &Cue) -> String {
    let g = cue.gain.clamp(0.0, MAX_GAIN);
    let mut ramps = Vec::new();
    if cue.fin > 0.0 {
        ramps.push(format!("(t-{:.3})/{:.3}", cue.from, cue.fin));
    }
    if cue.fout > 0.0 {
        ramps.push(format!("({:.3}-t)/{:.3}", cue.to, cue.fout));
    }
    let value = match ramps.len() {
        1 => format!("1{:+.4}*min(1,{})", g - 1.0, ramps[0]),
        2 => format!(
            "1{:+.4}*min(1,min({},{}))",
            g - 1.0,
            ramps[0],
            ramps[1]
        ),
        _ => format!("{:.4}", g),
    };
    format!(
        "if(between(t,{:.3},{:.3}),{},1)",
        cue.from, cue.to, value
    )
}

/// S5 (`per cue, in turn`): every cue of one clip applied one after another, so two that overlap multiply — which is
/// exactly what the preview's mix says they do. Returns the filter text and the label it leaves on.
pub fn gain_chain(cues: &[Cue], in_label: &str) -> String {
    let mut fc = String::new();
    let mut label = in_label.to_string();
    for (k, cue) in cues.iter().enumerate() {
        let out = format!("gv{k}");
        fc += &format!(
            "[{label}]volume=volume='{}':eval=frame[{out}];",
            gain_expr(cue)
        );
        label = out;
    }
    fc
}

/// S5 (`after the lane mix, before the narration`): where the volume cues sit in the audio graph. The lanes are
/// already summed by the time a cue runs, so a duck under the narration multiplies with them instead of replacing
/// one lane's level.
pub fn audio_order() -> Vec<&'static str> {
    vec!["lane mix", "volume cues", "narration"]
}

// --- S6: the label -------------------------------------------------------------------------------------------------

/// §4 (`Label: nothing`): a label marks a moment for the narration brief and is never drawn, so it contributes no
/// filter to any clip. It is kept in the record because F3.7's whole point is that the mark survives; the render's
/// answer to it is an empty string.
pub fn is_rendered(kind: &str) -> bool {
    if kind == "label" {
        return false;
    }
    // The kinds §4 gives a filter to. Written out rather than asked of `EffectKind::parse`, which would also answer
    // true for a label and needs no new public surface for this one question.
    matches!(kind, "zoom" | "speed" | "stop" | "text" | "svg" | "volume")
}

/// The filter text one clip carries for its effects — the single place that assembles them, so S6's answer cannot be
/// forgotten: a label never reaches any of the per-kind builders below.
pub fn clip_filters(fx: &[Fx], seg: &Seg, src: (i32, i32), frame: (i32, i32)) -> Vec<String> {
    let drawn: Vec<Fx> = fx.iter().filter(|e| is_rendered(&e.kind)).cloned().collect();
    let mut chain = Vec::new();
    for picture in split_pictures(&drawn, seg) {
        if let Some(step) = picture.setpts {
            chain.push(step);
        }
    }
    // A camera nobody moved needs no filters: the clip is already the right size and shape, and a crop-and-scale pair
    // that takes the whole frame would only give the encoder something to resample.
    let (samples, _) = cap_zoom(&camera_samples(&drawn, seg.s, seg.e - seg.s), src, frame, 1);
    if !is_static(&samples) {
        chain.extend(camera_chain(&samples, src, frame));
    }
    for effect in drawn
        .iter()
        .filter(|e| matches!(e.effect_kind(), Some(EffectKind::Text) | Some(EffectKind::Svg)))
    {
        chain.extend(overlay_chain(
            &effect.kind,
            (effect.t - seg.s, effect.t + effect.dur - seg.s),
            (effect.trans, effect.tout),
            (0, 0, frame.0, frame.1),
        ));
    }
    let cues = gain_cues(&drawn, seg.s, 1.0, seg.e - seg.s);
    if !cues.is_empty() {
        chain.push(gain_chain(&cues, "snd"));
    }
    // A stop is not a rate and appears here only as the overlay that holds its frame.
    for effect in drawn.iter().filter(|e| fx_record::is_stop(e)) {
        chain.extend(still_chain(
            effect.t,
            (effect.t - seg.s, effect.t + effect.dur - seg.s),
            (effect.trans, effect.tout),
            frame,
        ));
    }
    chain
}
