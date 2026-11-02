//! F3.6 Volume by hand — `spec/06-effects.md` F3.6, steps S1–S5, and `spec/inventory/effects.md` §A.6.

use crate::cut::Fx;
use crate::cut_hear;
use crate::cut_speed;
use crate::fx_record;
use crate::fx_zoom;
use crate::tools;

// --- S1: where the effect goes --------------------------------------------------------------------------------------

pub const NO_SECONDS: &str =
    "click a track or mark a stretch first \u{2014} volume needs seconds to work on";

pub const DEFAULT_GAIN: f64 = 2.0;

pub const FADE_SECONDS: f64 = 0.25;

pub const LINE_SECONDS: f64 = 2.0;

/// F3.6 S3 (`Length ≥ 0.1`): `// effects.volumeMinSeconds`, §F's `fxMinDur 0.1` at volume's value — the shortest thing
/// worth placing, in §A.6's terms: audible, and still a band wide enough for the lane to draw and be grabbed again.
pub const MIN_SECONDS: f64 = 0.1;

/// F3.6 S3 (`Volume % (0..1000)`): the top of the percent field, DERIVED from playbin's ceiling rather than written as
/// a literal 1000 — if the ceiling moves, the form's top moves with it, which is the only way they cannot drift apart.
pub fn max_percent() -> f64 {
    fx_record::gain_ceiling() * 100.0
}

/// F3.6 S1: what the flowchart decides. `LoudAtLine` rather than speed's `StopAtLine`: a volume does not stop
/// anything, it makes two seconds louder or quieter from where the line is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pressed {
    /// A marked stretch: the effect's seconds are the band's seconds.
    Stretched { t: f64, dur: f64 },
    /// No band worth the name, but a line: two seconds start there.
    LoudAtLine,
    /// Neither — [`NO_SECONDS`].
    Refused,
}

pub fn press(selection: Option<(f64, f64)>, line: Option<f64>) -> Pressed {
    if let Some((t, dur)) = cut_speed::marked_stretch(selection) {
        return Pressed::Stretched { t, dur };
    }
    match line {
        Some(_) => Pressed::LoudAtLine,
        None => Pressed::Refused,
    }
}

pub fn initial(pressed: Pressed) -> Option<Fx> {
    let (t, dur) = match pressed {
        Pressed::Stretched { t, dur } => (t, dur),
        Pressed::LoudAtLine => (0.0, LINE_SECONDS),
        Pressed::Refused => return None,
    };
    Some(Fx {
        kind: "volume".into(),
        t,
        dur,
        gain: DEFAULT_GAIN,
        trans: FADE_SECONDS,
        tout: FADE_SECONDS,
        ..Default::default()
    })
}

/// F3.6 S1: the same record at a second someone named — the `line..+2 s` branch with its line supplied, so the rule
/// is testable without a playhead.
pub fn initial_at_line(line: f64) -> Fx {
    let mut fx = initial(Pressed::LoudAtLine).expect("a line is never refused");
    fx.t = line;
    fx
}

// --- S3: the form --------------------------------------------------------------------------------------------------

pub fn form_title(t: f64, dur: f64) -> String {
    format!(
        "Volume {} \u{2013} {}",
        tools::mm_ss(t),
        tools::mm_ss(t + dur)
    )
}

pub const FORM_FIELDS: [&str; 5] = [
    "Volume %",
    "Length (s)",
    "Fade in (s)",
    "Fade out (s)",
    "Curve",
];

pub const VOLUME_HELP: &str =
    "up to 1000 for a passage recorded too quietly to hear \u{2014} though a passage lifted \
                               that far brings its hiss up with it";

pub const VOLUME_HELP_FULL: &str = "how loud these seconds are played, against how they were recorded. 100 is \
                                    untouched, 50 half as loud, 0 silent, and up to 1000 for a passage recorded too \
                                    quietly to hear \u{2014} though a passage lifted that far brings its hiss up with it";

pub const FADE_IN_HELP: &str = "0 is the hard step, which on a big change is audible as a click";

/// F3.6 S3 (`Fade out`): §A.6's wording — the way back to what was recorded, on the same terms as the way in.
pub const FADE_OUT_HELP: &str =
    "how long it takes to come back to the recorded loudness at the end, on the same terms";

/// F3.6 S3: what the form holds when Apply is pressed — the percent as typed (the field's unit, not the record's),
/// the seconds and fades as typed, and the curve's display name.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub t: f64,
    /// The field's own unit: 100 is untouched. [`gain_of_percent`] turns it into what the record stores.
    pub percent: f64,
    pub dur: f64,
    pub trans: f64,
    pub tout: f64,
    /// The curve's display name; [`fx_zoom::curve_stored`] decides what gets written.
    pub curve: String,
    /// §1's new column, carried in by the page rather than typed: `""` is the whole bed, a lane's name that lane
    /// only. Read off the press's seconds by [`lane_for_press`] and checked against the cut by [`lane_answer`].
    pub lane: String,
}

/// F3.6 S3: the percent a stored gain is worth — 100 for untouched, 0 for silence, [`max_percent`] at the ceiling.
pub fn percent_of(gain: f64) -> f64 {
    gain.clamp(0.0, fx_record::gain_ceiling()) * 100.0
}

pub fn gain_of_percent(percent: f64) -> f64 {
    (percent.clamp(0.0, max_percent()) / 100.0).min(fx_record::gain_ceiling())
}

pub fn apply(form: &Form) -> Result<Fx, String> {
    if form.dur < MIN_SECONDS {
        return Err(format!(
            "a volume change of {:.2} s is shorter than the shortest one this form takes ({}) s",
            form.dur, MIN_SECONDS
        ));
    }
    let (trans, tout) = cut_speed::clamp_fades(form.trans, form.tout, form.dur);
    Ok(Fx {
        kind: "volume".into(),
        t: form.t,
        dur: form.dur,
        trans,
        tout,
        gain: gain_of_percent(form.percent),
        ease: fx_zoom::curve_stored(&form.curve).to_string(),
        lane: form.lane.clone(),
        ..Default::default()
    })
}

/// §06-effects#1-record (`lane … "" = the whole bed; a lane's name = that lane only`): which lane a press works on.
pub fn lane_for_press(lanes: &[crate::cut::Lane], t: f64, dur: f64) -> String {
    let inside = |lane: &crate::cut::Lane| lane.at <= t && t + dur <= lane.at + lane.dur;
    let mut names = lanes
        .iter()
        .filter(|lane| inside(lane))
        .map(|lane| lane.name.clone());
    match (names.next(), names.next()) {
        (Some(one), None) => one,
        _ => String::new(),
    }
}

pub fn lane_answer(lanes: &[crate::cut::Lane], named: &str) -> String {
    if lanes.iter().any(|lane| lane.name == named) {
        named.to_string()
    } else {
        String::new()
    }
}

// --- S5: what the page says -----------------------------------------------------------------------------------------

pub const PICTURE_UNTOUCHED: &str = "the picture is untouched";

pub fn label(fx: &Fx) -> String {
    format!(
        "{} {} at {} for {:.1}s",
        tools::mm_ss(fx.t),
        verb(fx.gain),
        percent_label(fx),
        fx.dur
    )
}

/// F3.6 S5: the verb [`label`] uses — the same three readings §A.6 gives the field.
pub fn verb(gain: f64) -> &'static str {
    match gain.clamp(0.0, fx_record::gain_ceiling()) {
        0.0 => "silent",
        g if g < 1.0 => "quieter",
        _ => "louder",
    }
}

/// F3.6 S5 (`label the percentage`): a gain as the whole percent the lane prints. Rounded, because a lane bar is read
/// at a glance and `200.0 %` is not a thing anyone says.
pub fn percent_label(fx: &Fx) -> String {
    format!("{:.0}%", percent_of(fx.gain))
}

/// F3.6 S5 (`"… — the picture is untouched"`): what the page says after a volume is placed.
pub fn placed_status(fx: &Fx) -> String {
    format!("{} \u{2014} {PICTURE_UNTOUCHED}", label(fx))
}

// --- S5: heard, not seen ----------------------------------------------------------------------------------------------

pub fn heard_while_paused(fx: &[Fx], t: f64) -> f64 {
    if cut_hear::bed_silenced_under(fx, t) {
        return 0.0;
    }
    cut_hear::gain_under(fx, t)
}
