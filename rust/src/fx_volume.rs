//! F3.6 Volume by hand — `spec/06-effects.md` F3.6, steps S1–S5, and `spec/inventory/effects.md` §A.6.
//!
//! ❝ A volume is the one effect with nothing to point at: §A.6 opens "No box, no drag", because there is nowhere on a
//! frame to indicate a sound. So this module has no geometry at all — what it owns is where the seconds come from
//! (a marked band, else two seconds off the red line), how the percent field and the stored linear gain are the same
//! number read two ways, and what the page says afterwards. Everything arithmetic about loudness is already
//! [`crate::cut_hear`]'s: overlapping gains multiply, playbin's ceiling caps the result, and a zero gain is silence.
//! `rust/src/ui/window.rs` renders it for real: 🔊 Volume in the ✚ Effect dropdown calls
//! `ui::press_volume_item`, `show_volume_form` draws the five fields, and `press_volume_apply` puts the record
//! on the cut through `record_edit`. What this module owns is where the seconds come from (a marked band, else two
//! seconds off the red line), how the percent field and the stored linear gain are the same number read two ways,
//! and what the page says afterwards. Everything arithmetic about loudness is already [`crate::cut_hear`]'s:
//! overlapping gains multiply, playbin's ceiling caps the result, and a zero gain is silence. §A.6's "No box, no
//! drag" holds in code: there is no geometry here for a drag to write.

use crate::cut::{EffectKind, Fx};
use crate::cut_hear;
use crate::cut_speed;
use crate::fx_lane;
use crate::fx_record;
use crate::fx_zoom;
use crate::tools;

// --- S1: where the effect goes --------------------------------------------------------------------------------------

/// F3.6 S1 (`neither → "click a track or mark a stretch first — volume needs seconds to work on"`): §A.6 verbatim,
/// and this button's own sentence rather than an import of [`cut_speed::NO_SECONDS`] — that one says "speed needs
/// seconds to work on", and its doc comment already notes it says the same shape here with "volume". A volume is
/// placed over seconds, so what it lacks when nothing is marked is time, not a track.
pub const NO_SECONDS: &str =
    "click a track or mark a stretch first \u{2014} volume needs seconds to work on";

/// F3.6 S1 (`defaults gain 200 %`): `// effects.defaultGain` — §F's "gain 2" has no `P.` row in §10, so the id takes
/// the bare prefix of the rule that uses it, exactly as [`cut_speed::DEFAULT_RATE`] does. Twice as loud is the one ask
/// worth a default: it is what someone reaches for when a lav mic was too quiet, and it is reversible by ear in a
/// second. §A.6's "defaults gain 2".
pub const DEFAULT_GAIN: f64 = 2.0;

/// F3.6 S1 (`ramps 0.25 s`): **P.policy.effectDefaultFades**, whose §10 row reads "volume 0.25". A gain that arrives
/// on one sample is a click (§A.6's parenthetical), which is why a volume — the only effect that changes nothing but
/// loudness — still starts with fades rather than a step.
pub const FADE_SECONDS: f64 = 0.25;

/// F3.6 S1 (`selection or line..+2 s`): **P.policy.effectDefaultSeconds**, whose row reads "stop/speed/volume/label
/// 2". It is the same §10 row [`cut_speed::STOP_SECONDS`] and [`fx_zoom::DEFAULT_SECONDS`] read at their own kinds'
/// values — two seconds for a volume because that is long enough to hear what the loudness does and short enough not
/// to strand a passage nobody meant to raise.
pub const LINE_SECONDS: f64 = 2.0;

/// F3.6 S3 (`Length ≥ 0.1`): `// effects.volumeMinSeconds`, §F's `fxMinDur 0.1` at volume's value — the shortest thing
/// worth placing, in §A.6's terms: audible, and still a band wide enough for the lane to draw and be grabbed again.
pub const MIN_SECONDS: f64 = 0.1;

/// F3.6 S3 (`Volume % (0..1000)`): the top of the percent field, DERIVED from playbin's ceiling rather than written as
/// a literal 1000 — if the ceiling moves, the form's top moves with it, which is the only way they cannot drift apart.
/// A function because [`fx_record::gain_ceiling`] is where that ceiling is named, and a const cannot call it; the
/// field reads its bound here so `cut_hear::MAX_GAIN` stays the one place 10 is written.
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

/// F3.6 S1 (`a selection, or a line? … neither → refusal`): marked band first, then the line, then the refusal —
/// `cut_speed::press`'s order, since both effects are placed over seconds and §A.6 says volume is "placed like a
/// volume" i.e. over the marked stretch or a couple of seconds from the red line. A band under
/// [`cut_speed::MIN_MARKED_SECONDS`] is not a band at all (that floor belongs to every effect's press, which is why
/// it is borrowed rather than restated) and falls through to the line instead of refusing. Ends are ordered because a
/// right-to-left drag is the same stretch.
pub fn press(selection: Option<(f64, f64)>, line: Option<f64>) -> Pressed {
    if let Some((a, b)) = selection {
        let (from, to) = (a.min(b), a.max(b));
        // Compared as "not below the floor" rather than "at or above it": 0.2 is not representable in binary, so a
        // band dragged to exactly the floor can come out one ulp under and be thrown away as a click that slipped.
        // The trap is documented at [`cut_speed::press`]'s own test of the same bound.
        if to - from > cut_speed::MIN_MARKED_SECONDS
            || (to - from - cut_speed::MIN_MARKED_SECONDS).abs() < 1e-9
        {
            return Pressed::Stretched { t: from, dur: to - from };
        }
    }
    match line {
        Some(_) => Pressed::LoudAtLine,
        None => Pressed::Refused,
    }
}

/// F3.6 S1/S2: the record a press puts down before the form is touched — the band's seconds or two from the line, at
/// twice as loud with quarter-second ramps. No box and no lane, which is what makes it ride the whole bed
/// ([`fx_record::rides_whole_bed`]) rather than one row: §A.6's "No box, no drag", and a volume named for no lane
/// changes everything you hear.
pub fn initial(pressed: Pressed) -> Option<Fx> {
    let (t, dur) = match pressed {
        Pressed::Stretched { t, dur } => (t, dur),
        // `t` comes from the caller's red line; 0.0 here is the placeholder the thin page replaces, as in
        // `cut_speed::initial`, since this module is not given a playhead it cannot invent one.
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

/// F3.6 S3 (`Form "Volume a – b"`): titled by the seconds it is about, since a volume is placed over a stretch and
/// its form has to say which. The dash is §F3.6's en dash, not a hyphen. Through [`tools::mm_ss`], which pads — so it
/// reads `Volume 00:12 – 00:16`, matching every other status line on the page rather than the prototype's `0:12`.
pub fn form_title(t: f64, dur: f64) -> String {
    format!("Volume {} \u{2013} {}", tools::mm_ss(t), tools::mm_ss(t + dur))
}

/// F3.6 S3 (`Volume % … Length (s) … Fade in … Fade out … Curve`): the fields, in the order §A.6's sentence reads
/// them. Order is the sentence a person reads, not a layout detail — as in [`cut_speed::FORM_FIELDS`] and
/// [`crate::fx_text::FORM_FIELDS`]. The percent comes first because it is the only answer that says what happens to
/// the sound; everything after it is when.
pub const FORM_FIELDS: [&str; 5] =
    ["Volume %", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"];

/// F3.6 S3 (`"…up to 1000 for a passage recorded too quietly to hear — though a passage lifted that far brings its
/// hiss up with it"`): §F3.6's own quote, the part after the ellipsis. The 1000 in it is [`max_percent`], i.e. playbin
/// by another name, which is why this is one sentence and not a number someone has to keep agreeing with the ceiling.
pub const VOLUME_HELP: &str = "up to 1000 for a passage recorded too quietly to hear \u{2014} though a passage lifted \
                               that far brings its hiss up with it";

/// F3.6 S3 (§A.6's fuller wording of the same tooltip): the three readings a person actually uses, in percent because
/// "180 % is a loudness and 1.8 is arithmetic homework" — and because silence and ten times are easier to mean in
/// percent than as factors.
pub const VOLUME_HELP_FULL: &str = "how loud these seconds are played, against how they were recorded. 100 is \
                                    untouched, 50 half as loud, 0 silent, and up to 1000 for a passage recorded too \
                                    quietly to hear \u{2014} though a passage lifted that far brings its hiss up with it";

/// F3.6 S3 (`Fade in ("0 is the hard step, which on a big change is audible as a click")`): verbatim. Zero is allowed
/// and explained rather than forbidden — §A.6 says the ramp exists because a step is a click, and someone who wants
/// the click can have it.
pub const FADE_IN_HELP: &str = "0 is the hard step, which on a big change is audible as a click";

/// F3.6 S3 (`Fade out`): §A.6's wording — the way back to what was recorded, on the same terms as the way in.
pub const FADE_OUT_HELP: &str = "how long it takes to come back to the recorded loudness at the end, on the same terms";

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
}

/// F3.6 S3: the percent a stored gain is worth — 100 for untouched, 0 for silence, [`max_percent`] at the ceiling.
pub fn percent_of(gain: f64) -> f64 {
    gain.clamp(0.0, fx_record::gain_ceiling()) * 100.0
}

/// F3.6 S3: and back. One number read two ways, so the field can be loudness while the file stays a linear multiplier
/// — [`cut_hear::gain_under`]'s arithmetic is in factors, and a percent in `cut.json` would make every reader of the
/// record do the division. Outside the field's range it clamps rather than refusing: 1500 % typed by hand means "as
/// loud as this goes", not an error to explain.
pub fn gain_of_percent(percent: f64) -> f64 {
    (percent.clamp(0.0, max_percent()) / 100.0).min(fx_record::gain_ceiling())
}

/// F3.6 S3/S4 (`Volume % 0..1000 · Length ≥ 0.1`): the record the form writes back. A length under [`MIN_SECONDS`] is
/// refused naming its floor, in the shape [`fx_text::apply`] and `cut_speed::apply` use — what is wrong, then the
/// bound. The percent is clamped by [`gain_of_percent`], the two fades are shared out by
/// [`cut_speed::clamp_fades`] (this is the fourth form reading §A.2's arithmetic, which is the point of it being one
/// function), and the curve goes through [`fx_zoom::curve_stored`] so Linear writes no `ease` key and an untouched
/// file re-saves byte-identically (§06#1).
///
/// Nothing else on [`Fx`] is written: a volume has no box, no words, no file and no camera row — §1's table marks all
/// of those `–` for it, and [`fx_record::uses`] is what says so.
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
        ..Default::default()
    })
}

// --- S5: what the page says -----------------------------------------------------------------------------------------

/// F3.6 S5 (`status "… — the picture is untouched"`): the second half of §F3.6's status, and the sentence that tells
/// someone why this effect is safe to try — a volume changes what you hear and no pixel. There is deliberately no
/// `↶ Undo takes it back` tail here: that suffix belongs to [`cut_speed::placed_status`]'s own §A.3 wording, not to
/// this flow's quote.
pub const PICTURE_UNTOUCHED: &str = "the picture is untouched";

/// F3.6 S5 (§A.6 `label the percentage`): the effect as the lane and the status line name it — `00:12 louder at 200%
/// for 4.0s`. Percent rather than a factor, because that is the unit the form asked in and the unit a person hears
/// in. The three verbs are §A.6's own readings: silence at nought, quieter under untouched, louder from untouched up
/// (a volume at exactly 100 % does nothing, but it was still *meant* as a raising of the voice — and it is the label,
/// not a gain rule).
///
/// The fades are left out of the label, following [`cut_speed::label`]'s decision rather than the prototype's: two
/// labels in one status line have to read alike, and speed's does not carry them either.
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

/// F3.6 S5 (`applied in the preview even while paused`): the loudness of a session second, asked exactly as the
/// playing preview asks it — because it is a function of the second, not of whether frames are moving. §A.6's
/// "Preview gain applies even paused" is that fact stated from the other side: park the line in the middle of a
/// raised passage and what you hear must be what will play, or the fades cannot be judged without playing.
///
/// [`cut_hear::gain_under`] is the one rule (overlapping gains multiply, capped at playbin's ceiling); this is volume's
/// name for it, so the preview and this flow cannot grow separate answers.
pub fn heard_while_paused(fx: &[Fx], t: f64) -> f64 {
    cut_hear::gain_under(fx, t)
}

/// F3.6 S5 (`No box, no drag` + §06#2's paused-preview rule): a volume has no visual at all. [`fx_lane::drawn_paused`]
/// already says so — §2 lists nothing to draw for it, and inventing one would put a shape on the picture that the
/// render does not contain. `true` here means what §A.6 means: heard, never seen.
pub fn gain_has_no_visual() -> bool {
    !fx_lane::drawn_paused(EffectKind::Volume)
}
