//! F3.7 Label by hand — `spec/06-effects.md` F3.7, and `spec/inventory/effects.md` §A.7 / §A.8.

use crate::cut::Fx;
use crate::cut_speed;
use crate::tools;

// --- S1: where the mark goes ---------------------------------------------------------------------------------------

/// F3.7 S1 (`neither → "click a track or mark a stretch first — a label names a moment, so it needs one"`):
pub const NO_SECONDS: &str =
    "click a track or mark a stretch first \u{2014} a label names a moment, so it needs one";

/// F3.7 S2 (`empty name not placed ("type a name and it is marked — nothing is placed until then")`): §A.7
/// verbatim.
pub const NO_NAME: &str = "type a name and it is marked \u{2014} nothing is placed until then";

/// F3.7 S1 (`line..+2 s`): **P.policy.effectDefaultSeconds**, whose row reads "stop/speed/volume/label 2" —
/// the same §10 row `fx_volume::LINE_SECONDS` and [`cut_speed::STOP_SECONDS`] read at their own kinds'
/// values.
pub const LINE_SECONDS: f64 = 2.0;

/// F3.7 S3/S4 (`dur ≥ 0.4`): the label form's typed floor.
pub const MIN_SECONDS: f64 = 0.4;

/// F3.7 S3 (`Name (10 chars`): the cap §A.7 puts on the field.
pub const NAME_MAX_CHARS: usize = 10;

/// F3.7 S1: what the flowchart decides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pressed {
    /// A marked stretch: the mark covers the band's seconds.
    Stretched {
        t: f64,
        dur: f64,
    },
    MarkAtLine,
    Refused,
}

/// F3.7 S1 (`a selection, or a line?
pub fn press(selection: Option<(f64, f64)>, line: Option<f64>) -> Pressed {
    if let Some((t, dur)) = cut_speed::marked_stretch(selection) {
        return Pressed::Stretched { t, dur };
    }
    match line {
        Some(_) => Pressed::MarkAtLine,
        None => Pressed::Refused,
    }
}

/// F3.7 S1/S2: the record a press puts down BEFORE the name is given — the band's seconds or two from the line,
/// with `words` empty because the name is the question the form still has to answer.
pub fn initial(pressed: Pressed) -> Option<Fx> {
    let (t, dur) = match pressed {
        Pressed::Stretched { t, dur } => (t, dur),
        // `t` comes from the caller's red line; 0.0 here is the placeholder the thin page replaces, as in
        // `fx_volume::initial`, since this module is not given a playhead it cannot invent one.
        Pressed::MarkAtLine => (0.0, LINE_SECONDS),
        Pressed::Refused => return None,
    };
    Some(Fx {
        kind: "label".into(),
        t,
        dur,
        ..Default::default()
    })
}

/// F3.7 S1: the same record at a second someone named — the `line..+2 s` branch with its line supplied, so the
/// rule is testable without a playhead.
pub fn initial_at_line(line: f64) -> Fx {
    let mut fx = initial(Pressed::MarkAtLine).expect("a line is never refused");
    fx.t = line;
    fx
}

// --- S3: the form --------------------------------------------------------------------------------------------------

/// F3.7 S3 (`Form "Label at m:ss"`): titled by the ONE moment it marks, so unlike `fx_volume::form_title`'s
/// `a – b` pair this takes only the start.
pub fn form_title(t: f64) -> String {
    format!("Label at {}", tools::mm_ss(t))
}

/// F3.7 S3 (`Name · Length (s)`): the fields, in §A.7's order.
pub const FORM_FIELDS: [&str; 2] = ["Name", "Length (s)"];

/// F3.7 S3 (§A.7's Name tooltip, whole): the examples are the point — they say a label is a PERSONAL name for
/// a moment, not a filename or a slug — and the middle clause is the promise that placing one is safe.
pub const NAME_HELP: &str = "what you call this moment -- \"the reveal\", \"boss fight\". It changes nothing in \
                            the video: it is written into the brief the narration writer is given\u{2026}";

/// F3.7 S3 (§A.7's Length wording, `the stretch a clip must overlap`): the length of a label is not how long
/// a sound lasts, it is how wide the marked moment is drawn and therefore how far a kept clip has to reach to
/// carry it.
pub const LENGTH_HELP: &str =
    "how wide the marked moment is \u{2014} the stretch a clip must overlap";

/// F3.7 S3: what the form holds when Apply is pressed — the name as typed (in characters, the field's unit) and
/// the seconds as typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub t: f64,
    pub name: String,
    pub dur: f64,
}

/// F3.7 S3/S4: the record the form writes back.
pub fn apply(form: &Form) -> Result<Fx, String> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err(NO_NAME.to_string());
    }
    if form.dur < MIN_SECONDS {
        return Err(format!(
            "a label of {:.2} s is shorter than the shortest one this form takes ({}) s",
            form.dur, MIN_SECONDS
        ));
    }
    let clipped: String = name.chars().take(NAME_MAX_CHARS).collect();
    Ok(Fx {
        kind: "label".into(),
        t: form.t,
        dur: form.dur,
        text: clipped,
        ..Default::default()
    })
}

// --- S5: what the page says, and what happens to the mark -------------------------------------------------------------

/// F3.7 S5 (`placed "… — nothing changes in the video; the narration is told about it"`): the second half of
/// §A.7's placed sentence, whole.
pub const NOTHING_CHANGES: &str = "nothing changes in the video; the narration is told about it";

/// F3.7 S5 + §A.8 (`"label “…” at m:ss"`): the mark as the lane and the status line name it.
pub fn label(fx: &Fx) -> String {
    format!(
        "label \u{201c}{}\u{201d} at {}",
        fx.text,
        tools::mm_ss(fx.t)
    )
}

/// F3.7 S5: what the page says after a label is placed.
pub fn placed_status(fx: &Fx) -> String {
    format!("{} \u{2014} {NOTHING_CHANGES}", label(fx))
}
