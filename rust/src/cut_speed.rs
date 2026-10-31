//! F3.3 Speed and stop by hand — `spec/06-effects.md` F3.3, steps S1–S4 and the arithmetic after them.

use crate::cut::{EffectKind, Fx, Seg};
use crate::fx_lane;
use crate::fx_record;
use crate::fx_zoom;
use crate::tools::{self, cutpass};

/// P.eng.minClipSeconds.
const MIN_CLIP_SECONDS: f64 = cutpass::MIN_CLIP_SECONDS;

pub const MIN_MARKED_SECONDS: f64 = 0.2;

pub const NO_SECONDS: &str =
    "click a track or mark a stretch first \u{2014} speed needs seconds to work on";

pub const DEFAULT_RATE: f64 = 0.5;

pub const STOP_SECONDS: f64 = 2.0;

pub const STOP_FADE_SECONDS: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pressed {
    /// A marked stretch: the effect's seconds are the band's seconds.
    Stretched { t: f64, dur: f64 },
    /// No band worth the name, but a line: a stop goes there.
    StopAtLine,
    /// Neither — [`NO_SECONDS`].
    Refused,
}

pub fn press(selection: Option<(f64, f64)>, line: Option<f64>) -> Pressed {
    if let Some((a, b)) = selection {
        let (from, to) = (a.min(b), a.max(b));
        // `to - from == MIN_MARKED_SECONDS` is tested as "not below it" rather than "at or above it": 0.2 is not
        // representable in binary, so the width of a band dragged to exactly the floor can come out one ulp under it
        // and would be thrown away as a click that slipped.
        if to - from > MIN_MARKED_SECONDS || (to - from - MIN_MARKED_SECONDS).abs() < 1e-9 {
            return Pressed::Stretched {
                t: from,
                dur: to - from,
            };
        }
    }
    match line {
        Some(_) => Pressed::StopAtLine,
        None => Pressed::Refused,
    }
}

pub fn initial(pressed: Pressed) -> Option<Fx> {
    match pressed {
        Pressed::Stretched { t, dur } => Some(Fx {
            kind: "speed".into(),
            t,
            dur,
            rate: DEFAULT_RATE,
            ..Default::default()
        }),
        Pressed::StopAtLine => Some(Fx {
            kind: "speed".into(),
            t: 0.0,
            dur: STOP_SECONDS,
            trans: STOP_FADE_SECONDS,
            tout: STOP_FADE_SECONDS,
            rate: 0.0,
            ..Default::default()
        }),
        Pressed::Refused => None,
    }
}

pub fn form_title(t: f64, dur: f64) -> String {
    format!(
        "Speed {} \u{2013} {}",
        tools::mm_ss(t),
        tools::mm_ss(t + dur)
    )
}

pub const RATES: [f64; 11] = [0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0, 8.0, 20.0, 100.0];

pub const CUSTOM: &str = "Custom\u{2026}";

fn short_number(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    let mut text = format!("{value}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text.trim_end_matches('.').to_string()
}

pub fn rate_label(rate: f64) -> String {
    match rate {
        0.0 => format!("\u{00d7}{} \u{2014} stop", short_number(rate)),
        1.0 => format!("\u{00d7}{} \u{2014} as filmed", short_number(rate)),
        _ => format!("\u{00d7}{}", short_number(rate)),
    }
}

pub fn rate_index(rate: f64) -> usize {
    RATES
        .iter()
        .position(|listed| (listed - rate).abs() < 1e-9)
        .unwrap_or(RATES.len())
}

pub const FORM_FIELDS: [&str; 6] = [
    "Speed \u{00d7}",
    "Sound",
    "Length (s)",
    "Fade in (s)",
    "Fade out (s)",
    "Curve",
];

pub const SOUND_CHOICES: [&str; 5] = [
    "With the picture",
    "With the picture, pitched",
    "1\u{00d7} to the effect's end",
    "1\u{00d7} to the scene's end",
    "Silent",
];

pub fn sound_stored(choice: &str) -> &'static str {
    match choice {
        _ if choice == SOUND_CHOICES[1] => "pitch",
        _ if choice == SOUND_CHOICES[2] => "own",
        _ if choice == SOUND_CHOICES[3] => "scene",
        _ if choice == SOUND_CHOICES[4] => "mute",
        // Including the first row and anything unrecognised: no answer, the clip's own sound.
        _ => "",
    }
}

pub fn fade_in_help() -> String {
    format!(
        "how long it takes to get there instead of snapping to it: the clock ramping up to the rate, or the still \
         fading on over the moving picture. 0 changes on one frame. A ramp needs about {}s of footage for every \
         \u{00d7} of the rate \u{2014} {}s at \u{00d7}4 \u{2014} because each step of it has to last long enough \
         to render; under that it is treated as 0.",
        short_number(RAMP_STEP_SECONDS),
        short_number(4.0 * RAMP_STEP_SECONDS),
    )
}

const STOP_SOUND_NOTE: &str =
    "A stop's footage runs on at 1\u{00d7} under the held frame, so every answer but \
                               Silent sounds the same here.";

pub fn cost_note(snd: &str, rate: f64, dur: f64) -> String {
    if rate <= 0.0 {
        return STOP_SOUND_NOTE.to_string();
    }
    if snd != "own" && snd != "scene" {
        return String::new();
    }
    let on_screen = dur / rate;
    let gap = fx_lane::debt(dur, on_screen);
    if gap.abs() < fx_lane::DEBT_TAIL_MIN_SECONDS {
        return String::new();
    }
    if gap > 0.0 {
        format!(
            "{:.0} s on screen: the sound ends {:.0} s behind the picture, and going back in sync skips those \
             seconds.",
            on_screen, gap
        )
    } else {
        format!(
            "{:.0} s on screen: the sound runs {:.0} s ahead of the picture, and going back in sync plays those \
             seconds again.",
            on_screen,
            -gap
        )
    }
}

pub const MIN_RATE: f64 = 0.05;

pub const MAX_RATE: f64 = 100.0;

pub const RAMP_STEP_SECONDS: f64 = 0.6;

pub fn clamp_speed(rate: f64, dur: f64) -> (f64, f64) {
    let dur = dur.max(MIN_CLIP_SECONDS);
    let mut rate = rate.clamp(MIN_RATE, MAX_RATE);
    if dur / rate < MIN_CLIP_SECONDS {
        rate = dur / MIN_CLIP_SECONDS;
    }
    (rate, dur)
}

/// F3.3 S2/S3.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub t: f64,
    pub dur: f64,
    pub rate: f64,
    pub trans: f64,
    pub tout: f64,
    /// The curve's display name; [`fx_zoom::curve_stored`] decides what gets written.
    pub curve: String,
    pub snd: &'static str,
}

pub fn apply(form: &Form) -> Fx {
    let (rate, dur) = if form.rate <= 0.0 {
        (0.0, form.dur.max(MIN_CLIP_SECONDS))
    } else {
        clamp_speed(form.rate, form.dur)
    };
    let (trans, tout) = clamp_fades(form.trans, form.tout, dur);
    Fx {
        kind: "speed".into(),
        t: form.t,
        dur,
        trans,
        tout,
        rate,
        snd: form.snd.to_string(),
        // Reused from the zoom round rather than copied: "Linear" is LINEAR_EASE (empty, so no `ease` key and an
        // untouched file stays byte-identical) and a curve name this build does not know survives untouched.
        ease: fx_zoom::curve_stored(&form.curve).to_string(),
        ..Default::default()
    }
}

pub fn clamp_fades(trans: f64, tout: f64, dur: f64) -> (f64, f64) {
    let (trans, tout) = (trans.max(0.0), tout.max(0.0));
    let sum = trans + tout;
    if sum > dur && sum > 0.0 {
        let share = dur / sum;
        return (trans * share, tout * share);
    }
    (trans, tout)
}

pub fn label(fx: &Fx) -> String {
    if fx_record::is_stop(fx) {
        return format!("stop at {} for {:.1}s", tools::mm_ss(fx.t), fx.dur);
    }
    let verb = if fx.rate > 1.0 { "sped up" } else { "slowed" };
    format!(
        "{} {verb} \u{00d7}{} for {:.1}s",
        tools::mm_ss(fx.t),
        short_number(fx.rate),
        fx.dur
    )
}

const PLAYS_AT_RATE: &str =
    "the footage plays at that rate there and the cut gets longer or shorter to match";
const STANDS_STILL: &str = "the picture stands still there while the clock runs";

pub fn placed_status(fx: &Fx) -> String {
    let what = if fx_record::is_stop(fx) {
        STANDS_STILL
    } else {
        PLAYS_AT_RATE
    };
    format!("{} \u{2014} {what}; \u{21b6} Undo takes it back", label(fx))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub from: f64,
    pub to: f64,
    pub rate: f64,
}

impl Span {
    /// How long this stretch comes out on screen, which is the length the render measures it by.
    fn on_screen(&self) -> f64 {
        (self.to - self.from) / applied_rate(self.rate)
    }
}

pub fn applied_rate(rate: f64) -> f64 {
    if rate <= 0.0 {
        return 1.0;
    }
    rate
}

fn stair_fits(seconds: f64, from: f64, to: f64, n: usize) -> bool {
    let (lo, hi) = (from.min(to), from.max(to));
    if lo <= 0.0 || n < 1 || seconds <= 0.0 {
        return false;
    }
    let n = n as f64;
    let fastest = lo * (hi / lo).powf((n - 0.5) / n);
    (seconds / n) / fastest >= RAMP_STEP_SECONDS
}

pub fn stair_count(seconds: f64, from: f64, to: f64) -> usize {
    let mut n = 1;
    while stair_fits(seconds, from, to, n + 1) {
        n += 1;
    }
    n
}

pub fn stairs_in(t: f64, dur: f64, rate: f64, inp: f64, out: f64) -> Vec<Span> {
    // One ramp's stairs, appended in order. Kept a free fn with the destination passed in rather than a closure over
    // `steps`, which would be two mutable borrows of the same Vec.
    fn ramp(steps: &mut Vec<Span>, at: f64, length: f64, from: f64, to: f64) {
        let n = stair_count(length, from, to);
        for i in 0..n {
            let u = (i as f64 + 0.5) / n as f64;
            steps.push(Span {
                from: at + length * (i as f64) / n as f64,
                to: at + length * (i as f64 + 1.0) / n as f64,
                // from·(to/from)^u: the geometric middle of the stair.
                rate: from * (to / from).powf(u),
            });
        }
    }

    let mut steps: Vec<Span> = Vec::new();
    if inp > 0.0 {
        ramp(&mut steps, t, inp, 1.0, rate);
    }
    if dur - inp - out > 0.0 {
        steps.push(Span {
            from: t + inp,
            to: t + dur - out,
            rate,
        });
    }
    if out > 0.0 {
        ramp(&mut steps, t + dur - out, out, rate, 1.0);
    }
    steps
}

pub fn ramps(rate: f64, dur: f64, trans: f64, tout: f64) -> (f64, f64) {
    let mut inp = trans.max(0.0);
    let mut out = tout.max(0.0);
    if inp <= 0.0 && out <= 0.0 {
        return (0.0, 0.0);
    }
    let least = RAMP_STEP_SECONDS * rate.max(1.0).sqrt();
    if inp < least {
        inp = 0.0;
    }
    if out < least {
        out = 0.0;
    }
    if inp + out > dur && inp + out > 0.0 {
        let share = dur / (inp + out);
        inp *= share;
        out *= share;
        // The share-out can leave a ramp under the price again.
        if inp < least {
            inp = 0.0;
        }
        if out < least {
            out = 0.0;
        }
    }
    // A flat middle that renders under the floor is absorbed by the ramps.
    let mid = dur - inp - out;
    if mid > 0.0 && rate > 0.0 && mid / rate < MIN_CLIP_SECONDS && inp + out > 0.0 {
        let share = dur / (inp + out);
        inp *= share;
        out *= share;
    }
    if stairs_in(0.0, dur, rate, inp, out)
        .iter()
        .any(|step| step.on_screen() < MIN_CLIP_SECONDS - 1e-9)
    {
        return (0.0, 0.0);
    }
    (inp, out)
}

pub fn stairs(rate: f64, dur: f64, trans: f64, tout: f64) -> Vec<Span> {
    if rate <= 0.0 || dur <= 0.0 {
        return Vec::new();
    }
    let (inp, out) = ramps(rate, dur, trans, tout);
    if inp <= 0.0 && out <= 0.0 {
        return vec![Span {
            from: 0.0,
            to: dur,
            rate,
        }];
    }
    stairs_in(0.0, dur, rate, inp, out)
}

pub fn steps_of(fx: &Fx) -> Vec<Span> {
    if fx.effect_kind() != Some(EffectKind::Speed) || fx.dur <= 0.0 {
        return Vec::new();
    }
    let (from, _to) = fx.spans();
    if fx.rate <= 0.0 {
        return vec![Span {
            from,
            to: from + fx.dur,
            rate: 0.0,
        }];
    }
    let (rate, _) = clamp_speed(fx.rate, fx.dur);
    stairs(rate, fx.dur, fx.trans, fx.tout)
        .into_iter()
        .map(|mut step| {
            step.from += from;
            step.to += from;
            step
        })
        .collect()
}

pub fn render_stairs(fx: &[Fx], seg: &Seg) -> Vec<Span> {
    rate_spans(fx)
        .into_iter()
        .map(|mut span| {
            span.from = span.from.max(seg.s);
            span.to = span.to.min(seg.e);
            span
        })
        .filter(|span| span.to > span.from)
        .collect()
}

pub fn rate_spans(fx: &[Fx]) -> Vec<Span> {
    let all: Vec<Span> = fx.iter().flat_map(steps_of).collect();
    if all.is_empty() {
        return Vec::new();
    }
    let mut cuts: Vec<f64> = Vec::with_capacity(all.len() * 2);
    for step in &all {
        cuts.push(step.from);
        cuts.push(step.to);
    }
    cuts.sort_by(f64::total_cmp);

    let mut out: Vec<Span> = Vec::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if to - from < 1e-9 {
            continue;
        }
        let mid = (from + to) / 2.0;
        let covering: Vec<&Span> = all.iter().filter(|s| mid >= s.from && mid < s.to).collect();
        if covering.is_empty() {
            continue; // plain footage between two effects
        }
        let mean = covering.iter().map(|s| s.rate).sum::<f64>() / covering.len() as f64;
        out.push(Span {
            from,
            to,
            rate: mean,
        });
    }
    heal(join(out))
}

fn join(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for step in spans {
        match out.last_mut() {
            Some(last)
                if (last.to - step.from).abs() < 1e-9 && (last.rate - step.rate).abs() < 1e-9 =>
            {
                last.to = step.to;
            }
            _ => out.push(step),
        }
    }
    out
}

fn span_cuts(spans: &[Span], i: usize) -> bool {
    let left = if i > 0 {
        applied_rate(spans[i - 1].rate)
    } else {
        1.0
    };
    let right = if i + 1 < spans.len() {
        applied_rate(spans[i + 1].rate)
    } else {
        1.0
    };
    let here = applied_rate(spans[i].rate);
    (here - left).abs() > 1e-9 || (here - right).abs() > 1e-9
}

pub fn heal(mut spans: Vec<Span>) -> Vec<Span> {
    for _ in 0..spans.len() + 1 {
        if spans.is_empty() {
            return spans;
        }
        let thin = spans.iter().enumerate().filter(|(i, step)| {
            step.on_screen() < MIN_CLIP_SECONDS - 1e-9 && span_cuts(&spans, *i)
        });
        let Some((k, _)) = thin.min_by(|a, b| a.1.on_screen().total_cmp(&b.1.on_screen())) else {
            return spans;
        };
        spans = absorb(spans, k);
    }
    spans
}

fn absorb(mut spans: Vec<Span>, k: usize) -> Vec<Span> {
    if spans.len() == 1 {
        return Vec::new();
    }
    if k == 0 {
        spans[1].from = spans[0].from;
        return join(spans[1..].to_vec());
    }
    if k == spans.len() - 1 {
        spans[k - 1].to = spans[k].to;
        spans.truncate(k);
        return join(spans);
    }
    if spans[k - 1].on_screen() >= spans[k + 1].on_screen() {
        spans[k - 1].to = spans[k].to;
    } else {
        spans[k + 1].from = spans[k].from;
    }
    let mut rest = Vec::with_capacity(spans.len() - 1);
    rest.extend(spans[..k].iter().copied());
    rest.extend(spans[k + 1..].iter().copied());
    join(rest)
}

pub fn rate_at(fx: &[Fx], t: f64) -> f64 {
    rate_spans(fx)
        .into_iter()
        .find(|step| t >= step.from && t < step.to)
        .map(|step| applied_rate(step.rate))
        .unwrap_or(1.0)
}
