//! F3.3 Speed and stop by hand — `spec/06-effects.md` F3.3, steps S1–S4 and the arithmetic after them.
//!
//! ⏩ Speed turns a marked stretch into a clock and a bare red line into a stop; the form settles the rate, what
//! its sound does, and the two ramps. Every one of those answers is arithmetic or a quoted sentence, so all of it
//! lives here. The widget layer forwards and holds no rule: `rust/src/ui/window.rs` routes the dropdown's ⏩ Speed
//! row through `press_speed_item` (→ [`press`] / [`initial`], which opens the form but records nothing), draws the
//! "Speed a – b" fields from this module's own lists, and applies them in `press_speed_apply` (→ [`apply`] +
//! `record_edit`, so one ↶ takes the speed back).
//!
//! Two readings of a speed exist on purpose and this module holds the render's: overlapping rates AVERAGE per span
//! and ramps become stairs. The preview keeps its own flat, first-covering-effect reading in
//! [`crate::cut_hear::rate_under`] — see [`rate_at`] for why they stay apart.

use crate::cut::{EffectKind, Fx};
use crate::fx_lane;
use crate::fx_record;
use crate::fx_zoom;
use crate::tools::{self, cutpass};

/// P.eng.minClipSeconds — the shortest clip the render makes and, §10 says so outright, "also the speed clamp
/// floor". Held once by [`cutpass`], which is why this module borrows it instead of writing 0.5 a second time.
const MIN_CLIP_SECONDS: f64 = cutpass::MIN_CLIP_SECONDS;

// --- S1: where the effect goes -------------------------------------------------------------------------------

/// F3.3 S1 (`a selection ≥ 0.2 s?`): a fifth of a second under the pointer is a click that slipped, not a
/// dragged band. §10 files this bound as `P.eng.effectMinSelectionSeconds` (0.2, "timeline needed under the band
/// before ⏩ Speed treats it as a chosen stretch"; prototype `fxMinSel`, gui/cut_fx.go), and §06 §6 names the
/// same bound "the 0.2 s floor under which a band is not a marked stretch" — two ids for one rule, so both rows
/// in [`crate::params::effects`] read this one constant and neither spelling can drift from the other or from
/// the rule. This is NOT [`crate::cut_select::MIN_SECONDS`] (0.04), which asks whether a remainder of footage
/// is worth keeping; this asks whether the hand meant to drag.
pub const MIN_MARKED_SECONDS: f64 = 0.2;

/// F3.3 S1 (`neither → "click a track or mark a stretch first — speed needs seconds to work on"`): ⏩ Speed's own
/// refusal, the fifth sentence for the same missing thing — [`crate::fx_zoom::NO_LINE`] (⊕ Zoom),
/// [`crate::cut_insert::NO_LINE_YET`] (⧉ Insert), [`crate::cut_copy::NO_LINE_YET`] (⧉ Paste) and
/// [`crate::cut_copy::NO_LANE_LINE_YET`] (⇲ Lane) each ask in the words of what their button does. F3.6 (🔊 Volume)
/// says the same shape with "volume".
pub const NO_SECONDS: &str =
    "click a track or mark a stretch first \u{2014} speed needs seconds to work on";

/// F3.3 S1 (`t and dur from it · rate 0.5`): the rate a marked stretch is offered at — half speed, the one case
/// worth a default because it is the difference between "watch it again" and "skip it". §06 §6 lists it as "default
/// rate 0.5" with no `P.` row in §10 (`// effects.defaultRate`), so `params::cut()` catalogues it under that name.
pub const DEFAULT_RATE: f64 = 0.5;

/// F3.3 S1 (`only a line → a stop at the line, 2 s`): P.policy.effectDefaultSeconds, whose row reads "stop/speed/
/// volume/label 2" — long enough to read as a hold, short enough not to strand anyone.
pub const STOP_SECONDS: f64 = 2.0;

/// F3.3 S1 (`2 s · 0.5 s fades`): P.policy.effectDefaultFades, whose row reads "stop 0.5". A stop is faded on and
/// off rather than cut in, because a frame that arrives on one sample is a jump.
pub const STOP_FADE_SECONDS: f64 = 0.5;

/// F3.3 S1: what pressing ⏩ Speed did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pressed {
    /// A marked stretch: the effect's seconds are the band's seconds.
    Stretched { t: f64, dur: f64 },
    /// No band worth the name, but a line: a stop goes there.
    StopAtLine,
    /// Neither — [`NO_SECONDS`].
    Refused,
}

/// F3.3 S1: the decision the flowchart draws, in its own order — marked band first, then the line, then the
/// refusal. A band under [`MIN_MARKED_SECONDS`] is not a band at all, so it falls through to the line rather than
/// placing a fifth-of-a-second speed nobody meant. Ends are ordered because a right-to-left drag is the same stretch.
pub fn press(selection: Option<(f64, f64)>, line: Option<f64>) -> Pressed {
    if let Some((a, b)) = selection {
        let (from, to) = (a.min(b), a.max(b));
        // `to - from == MIN_MARKED_SECONDS` is tested as "not below it" rather than "at or above it": 0.2 is not
        // representable in binary, so the width of a band dragged to exactly the floor can come out one ulp under it
        // and would be thrown away as a click that slipped.
        if to - from > MIN_MARKED_SECONDS || (to - from - MIN_MARKED_SECONDS).abs() < 1e-9 {
            return Pressed::Stretched { t: from, dur: to - from };
        }
    }
    match line {
        Some(_) => Pressed::StopAtLine,
        None => Pressed::Refused,
    }
}

/// F3.3 S1: the effect the press opens the form with — a half-speed stretch over the marked seconds, or a stop at
/// the line faded on and off. `ease` stays empty — [`fx_record::LINEAR_EASE`] — so nothing is written for a curve
/// nobody chose (§06#1).
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

// --- S2: the form -----------------------------------------------------------------------------------------------

/// F3.3 S2 (`Form "Speed a – b"`): titled by the seconds it is about, since a speed is placed over a stretch and
/// its form must say which. The dash is §F3.3's en dash, not a hyphen.
pub fn form_title(t: f64, dur: f64) -> String {
    format!("Speed {} \u{2013} {}", tools::mm_ss(t), tools::mm_ss(t + dur))
}

/// F3.3 S2 (`Speed × (×0 — stop, …, ×100, Custom…)`; `// effects.speedChoices`): the rates worth a row. A list and
/// not an entry, because the useful slow rates are a handful that sit close together while the fast ones run from
/// "trim the dead air a bit" to a hundred — so anything not on it is typed instead of rounded to a neighbour.
pub const RATES: [f64; 11] = [0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0, 8.0, 20.0, 100.0];

/// F3.3 S2 (`Custom…`): the row after the list, and the only one with a typed box beside it — shown beside every
/// pick it would be two answers to one question sitting side by side disagreeing.
pub const CUSTOM: &str = "Custom\u{2026}";

/// F3.3 S2: a rate as short as it can be without lying. The one decimal the other fields use is too coarse for a
/// rate — a quarter speed printed ×0.5 would be a different speed.
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

/// F3.3 S2: the two rows that need a word rather than a number — `×0 — stop` and `×1 — as filmed`. The rest are
/// their own number, since ×4 needs no explanation and ×20 should not be dressed up.
pub fn rate_label(rate: f64) -> String {
    match rate {
        0.0 => format!("\u{00d7}{} \u{2014} stop", short_number(rate)),
        1.0 => format!("\u{00d7}{} \u{2014} as filmed", short_number(rate)),
        _ => format!("\u{00d7}{}", short_number(rate)),
    }
}

/// F3.3 S2: which row the dropdown opens on. A rate that is not listed is Custom, keeping its own number in the
/// box rather than silently becoming the nearest listed one — someone who typed ×3 asked for ×3.
pub fn rate_index(rate: f64) -> usize {
    RATES
        .iter()
        .position(|listed| (listed - rate).abs() < 1e-9)
        .unwrap_or(RATES.len())
}

/// F3.3 S2 (`Speed × · Sound · Length · Fade in · Fade out · Curve`): the fields, in the order the form reads them.
/// Order is the sentence a person reads, not a layout detail — as in [`crate::fx_zoom::FORM_FIELDS`].
pub const FORM_FIELDS: [&str; 6] =
    ["Speed \u{00d7}", "Sound", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"];

/// F3.3 S2 (`Sound (With the picture / With the picture, pitched / 1× to the effect's end / 1× to the scene's end
/// / Silent)`, inventory/effects.md §A.3): the five answers in the order they are offered. Short, because a
/// dropdown is as wide as the longest thing in it and this one shares its line with two other questions — what each
/// answer COSTS is [`cost_note`], which is the part worth reading twice.
pub const SOUND_CHOICES: [&str; 5] = [
    "With the picture",
    "With the picture, pitched",
    "1\u{00d7} to the effect's end",
    "1\u{00d7} to the scene's end",
    "Silent",
];

/// F3.3 S2: what the chosen answer is stored as. The two lists are ONE list read twice, so an index into either is
/// an index into the other, and these five keys are exactly the ones [`fx_record::snd_of`] reads (`""` is
/// [`fx_record::Snd::Default`] — the absence of an answer rather than one of them). An unknown word in a file reads
/// back as `""`, not as silence: guessing that a spelling this build does not know means "hush it" would take sound
/// out of somebody's video.
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

/// F3.3 S2 (`Fade in (s) ("…A ramp needs about 0.6s of footage for every × of the rate…")`): the help says what a
/// ramp costs because the alternative is someone typing 0.4 and wondering why nothing ramped. The numbers are read
/// off [`RAMP_STEP_SECONDS`] so the sentence cannot drift from the bound it describes.
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

/// F3.3 S2: what a stop does with its sound — the sentence for every rate 0, whatever was chosen. The footage under
/// a held frame runs on at 1× (§F3.3's last clause), so four of the five answers are the same sound.
const STOP_SOUND_NOTE: &str = "A stop's footage runs on at 1\u{00d7} under the held frame, so every answer but \
                               Silent sounds the same here.";

/// F3.3 S2 (`a cost note ("N s on screen: the sound ends N s behind the picture, and going back in sync skips those
/// seconds.")`): the line under the Sound dropdown — the arithmetic nobody should be asked to do while choosing.
/// Only the two 1× answers put the sound on a clock of its own; `""` and `pitch` take it with the picture and
/// `mute` drops them, so they have no cost to state. The gap is [`fx_lane::debt`] — the same number the lane's tail
/// is drawn to — printed whole seconds because this is a sentence, not a measurement.
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

// --- S3: Apply -----------------------------------------------------------------------------------------------

/// F3.3 S3 (`clampSpeed: rate into P.eng.minRate … maxRate`): the slowest a hand may put the clock to. Below this
/// the render is holding a frame rather than slowing one, which is what a stop is for.
pub const MIN_RATE: f64 = 0.05;

/// F3.3 S3 (`rate ∈ [P.eng.minRate 0.05, P.eng.maxRate 100]`): and the fastest.
///
/// This is NOT [`cutpass::MAX_RATE`] (4), which is `P.policy.maxSpeedRate` — the suggest pass's own ceiling, built
/// on how much footage a target can swallow. That pass only ever shortens; an effect placed by hand may slow footage
/// down or run to a hundred over seconds someone marked. The two numbers answer different questions and must not be
/// unified because they happen to share the word "rate".
pub const MAX_RATE: f64 = 100.0;

/// F3.3 (`each stair ≥ P.eng.rampStepSeconds (0.6) on screen`): one stair of a speed ramp, measured on the FINISHED
/// video and not in footage — at ×8 a stair reads eight seconds of footage to last one on screen. Measured in
/// footage, every stair of a fast ramp fell under the render's floor and its seconds went missing.
pub const RAMP_STEP_SECONDS: f64 = 0.6;

/// F3.3 S3 (`clampSpeed`): a rate inside [`MIN_RATE`]…[`MAX_RATE`] over a stretch that still lasts
/// [`MIN_CLIP_SECONDS`] on screen once the rate has had it. When those two fight it is the RATE that gives way —
/// the seconds are what the person marked and can see, and moving a band would edit something they did not ask for.
pub fn clamp_speed(rate: f64, dur: f64) -> (f64, f64) {
    let dur = dur.max(MIN_CLIP_SECONDS);
    let mut rate = rate.clamp(MIN_RATE, MAX_RATE);
    if dur / rate < MIN_CLIP_SECONDS {
        rate = dur / MIN_CLIP_SECONDS;
    }
    (rate, dur)
}

/// F3.3 S2/S3: what the form holds when Apply is pressed — the seconds and the rate as typed, the two fades, the
/// curve's display name and the sound answer [`sound_stored`] produced.
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

/// F3.3 S3 (`Apply: rate 0 → stop with dur ≥ 0.5 (no clampSpeed); else clampSpeed; clampFades`): the record the form
/// writes back.
///
/// A stop is NOT clamped: [`clamp_speed`] would hand it a rate, and a still has no rate to give way — its fades
/// belong inside the band the way a title's do. So the length floor is taken directly and the rate stays 0.
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

/// F3.3 S3 (`clampFades`): neither fade may be negative, and when the two together overrun the band they share it in
/// proportion rather than one of them losing everything — a ramp that arrives in 0.8 s out of a 1.2 s glide still
/// arrives smoothly.
///
/// This is the same share-out [`crate::fx_zoom`] does inside its own `apply`, and deliberately not one function with
/// a flag: a staying zoom zeroes its fades first and a stop never does, so folding them together would put a bool
/// parameter in every call site to say which kind is asking.
pub fn clamp_fades(trans: f64, tout: f64, dur: f64) -> (f64, f64) {
    let (trans, tout) = (trans.max(0.0), tout.max(0.0));
    let sum = trans + tout;
    if sum > dur && sum > 0.0 {
        let share = dur / sum;
        return (trans * share, tout * share);
    }
    (trans, tout)
}

// --- S4: what the page says -----------------------------------------------------------------------------------

/// F3.3 S4: the effect's own name in the status line and on the lane — `stop at m:ss for X.Xs` for a still,
/// `m:ss slowed ×0.5 for X.Xs` for a clock. The verb changes because "×2" and "slowed ×2" are not the same news.
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

/// F3.3 S4: the two sentences, one per branch — a rate changes the footage's clock and therefore the cut's length;
/// a stop holds the picture while the clock runs on, so it costs the video nothing.
const PLAYS_AT_RATE: &str = "the footage plays at that rate there and the cut gets longer or shorter to match";
const STANDS_STILL: &str = "the picture stands still there while the clock runs";

/// F3.3 S4 (`Status "… — the footage plays at that rate there …; ↶ Undo takes it back" / "… — the picture stands
/// still there while the clock runs…"`) — the whole line, in §A.2's order: what was placed, what it does, and that
/// one ↶ undoes it.
pub fn placed_status(fx: &Fx) -> String {
    let what = if fx_record::is_stop(fx) { STANDS_STILL } else { PLAYS_AT_RATE };
    format!("{} \u{2014} {what}; \u{21b6} Undo takes it back", label(fx))
}

// --- the arithmetic after S4 -----------------------------------------------------------------------------------

/// §F3.3 (`overlapping rates average per span`): one stretch of the cut's seconds and the rate its covering effects
/// agreed on. `rate` is the MEAN — a stop contributes 0 to it, which is how "a stop under a ×2 comes out ×1" falls
/// out instead of being a special case.
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

/// §F3.3 (`footage under a still runs at 1×`): the rate the footage actually runs at. A frozen stretch has no
/// observable rate — the still covers it — and ×0 is not a clip any encoder can build, so what runs underneath runs
/// at its own speed. The mean keeps the 0 in [`Span::rate`]; this is what plays.
pub fn applied_rate(rate: f64) -> f64 {
    if rate <= 0.0 {
        return 1.0;
    }
    rate
}

/// §F3.3 (`each stair ≥ P.eng.rampStepSeconds (0.6) on screen`): do all `n` stairs of a ramp clear the step? Asked
/// by measuring the FASTEST of them — the last stair going up, the first coming down, the same rate either way — at
/// its own middle (`lo·(hi/lo)^((n−½)/n)`). Judged at the top instead, an eight-second ramp to ×8 gets one stair
/// where it can afford two.
fn stair_fits(seconds: f64, from: f64, to: f64, n: usize) -> bool {
    let (lo, hi) = (from.min(to), from.max(to));
    if lo <= 0.0 || n < 1 || seconds <= 0.0 {
        return false;
    }
    let n = n as f64;
    let fastest = lo * (hi / lo).powf((n - 0.5) / n);
    (seconds / n) / fastest >= RAMP_STEP_SECONDS
}

/// §F3.3: how many stairs a ramp gets — as many as the render keeps whole, at least one.
pub fn stair_count(seconds: f64, from: f64, to: f64) -> usize {
    let mut n = 1;
    while stair_fits(seconds, from, to, n + 1) {
        n += 1;
    }
    n
}

/// §F3.3's staircase: the constant-rate stretches one effect becomes. Rates climb geometrically, each stair at the
/// rate of its own middle (the count is [`stair_count`], set by the fastest end); between them the flat middle runs
/// at the effect's own rate; a ramp of nought seconds contributes nothing. Split from [`ramps`] because `ramps` has
/// to look at the stairs a pair of ramps would make before it can say whether those ramps happen at all, and a
/// function that called `ramps` to answer that would be asking itself.
fn stairs_in(t: f64, dur: f64, rate: f64, inp: f64, out: f64) -> Vec<Span> {
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
        steps.push(Span { from: t + inp, to: t + dur - out, rate });
    }
    if out > 0.0 {
        ramp(&mut steps, t + dur - out, out, rate, 1.0);
    }
    steps
}

/// §F3.3 (`A ramp needs about 0.6s of footage for every × of the rate … under that it is treated as 0`): the seconds
/// an effect actually spends ramping each way.
///
/// The price of one stair at this rate is `RAMP_STEP_SECONDS · √max(1, rate)`: a single stair of a ramp to ×R runs
/// at √R, the geometric middle of 1 and R, so that is the rate its length is measured against — and a ramp that
/// cannot pay it gets none. Ramps asked to cover more than the effect holds share it in proportion (the flat middle
/// disappearing between them), and a flat middle too short to render is worse than no flat middle at all: the ramps
/// grow to meet where it was rather than the render dropping it and those seconds going missing.
///
/// Last, whole-or-nothing: if ANY stair of the pair would render under [`MIN_CLIP_SECONDS`] the ramps are dropped. A
/// dropped stair does not play at the wrong speed, it takes its footage out of the video — so a staircase that
/// cannot be built entirely is not built at all and the effect falls back to the plain change of speed it would have
/// been without ramps, which [`clamp_speed`] has already made long enough to render.
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

/// §F3.3 (`ramps are geometric staircases … built whole or not at all`): the stairs an effect renders as, over its
/// own seconds. With no ramps it is one flat stretch at the effect's rate — which is also what a pair of ramps that
/// cannot be built whole comes back to.
pub fn stairs(rate: f64, dur: f64, trans: f64, tout: f64) -> Vec<Span> {
    if rate <= 0.0 || dur <= 0.0 {
        return Vec::new();
    }
    let (inp, out) = ramps(rate, dur, trans, tout);
    if inp <= 0.0 && out <= 0.0 {
        return vec![Span { from: 0.0, to: dur, rate }];
    }
    stairs_in(0.0, dur, rate, inp, out)
}

/// §F3.3: what one effect contributes to the cut's clock. A stop is one flat ×0 over its band with no stairs — there
/// is no ramping into and out of standing still. A non-speed says nothing about the clock (§06#1 gives it no `rate`)
/// and a band of nought seconds contributes nothing.
pub fn steps_of(fx: &Fx) -> Vec<Span> {
    if fx.effect_kind() != Some(EffectKind::Speed) || fx.dur <= 0.0 {
        return Vec::new();
    }
    let (from, _to) = fx.spans();
    if fx.rate <= 0.0 {
        return vec![Span { from, to: from + fx.dur, rate: 0.0 }];
    }
    stairs(fx.rate, fx.dur, fx.trans, fx.tout)
        .into_iter()
        .map(|mut step| {
            step.from += from;
            step.to += from;
            step
        })
        .collect()
}

/// §F3.3 (`overlapping rates average per span`): the whole cut's speed as flat stretches — every boundary any speed
/// effect draws, and between two boundaries the arithmetic mean of the rates covering it. Seconds no effect covers
/// are not in the list at all: they run at ×1 by simply being left alone. Equal neighbours are then merged (a
/// boundary two effects happen to share must not become a cut in the video) and thin stretches healed ([`heal`]).
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
        out.push(Span { from, to, rate: mean });
    }
    heal(join(out))
}

/// §F3.3 (`merges equal neighbours`): neighbours that ask for the same thing are one stretch.
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

/// §F3.3 (`spans rendering under 0.5 s heal into the longer neighbour`): is this stretch a cut in the finished video
/// at all? One that runs at the same speed as both its neighbours — and the footage outside the list runs at ×1 —
/// makes no clip of its own, so it has no length for the render to find too short. This is what keeps a very short
/// stop from being healed away: on its own it splits nothing.
fn span_cuts(spans: &[Span], i: usize) -> bool {
    let left = if i > 0 { applied_rate(spans[i - 1].rate) } else { 1.0 };
    let right = if i + 1 < spans.len() { applied_rate(spans[i + 1].rate) } else { 1.0 };
    let here = applied_rate(spans[i].rate);
    (here - left).abs() > 1e-9 || (here - right).abs() > 1e-9
}

/// §F3.3 (`spans rendering under 0.5 s heal into the longer neighbour`): give away every stretch the render would
/// drop. Two bands overlapping by a tenth of a second slice a sliver under the floor out of each other, and such a
/// clip is not encoded — slightly the wrong speed for a tenth of a second beats footage going missing. The thinnest
/// one goes first, so several can heal in one pass; its seconds go to the LONGER neighbour, which is the one least
/// changed by taking them, and a stretch that stands alone is simply dropped.
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

/// §F3.3: hand stretch `k`'s seconds to the longer neighbour, or drop it when it stands alone.
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

/// §F3.3: the clock the footage at session second `t` runs on — the averaged, ramped answer, or ×1 where nothing
/// covers it (a frozen second included: [`applied_rate`] says what plays, and under a still that is the footage's
/// own speed so the still can fade off onto it).
///
/// This is the RENDER's reading and it deliberately differs from [`crate::cut_hear::rate_under`], which takes the
/// FIRST covering effect flat because a rate change is a flushing seek and following a ramp ten times a second hangs
/// the preview window. Both halves of that difference are pinned by [`crate::fx_lane`] —
/// [`crate::fx_lane::previews_flat_rate`] and [`crate::fx_lane::render_follows_ramps`] — and neither is this file's
/// to unify.
pub fn rate_at(fx: &[Fx], t: f64) -> f64 {
    rate_spans(fx)
        .into_iter()
        .find(|step| t >= step.from && t < step.to)
        .map(|step| applied_rate(step.rate))
        .unwrap_or(1.0)
}
