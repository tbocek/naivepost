//! F3.7 Label by hand — `spec/06-effects.md` F3.7, and `spec/inventory/effects.md` §A.7 / §A.8.
//!
//! A label is the one effect that changes NOTHING: §A.7 opens "Changes nothing; read by the narration brief as
//! MARKED", so this module owns only where the moment comes from (a marked band, else two seconds off the red
//! line), what a name has to be before anything may be placed, and what the page says afterwards. The storage
//! side already exists and is not restated here: `fx_record::uses` grants a label exactly `Dur` and `Words`
//! (§1's table), `render_fx::is_rendered("label")` is false so it never reaches a filter, and
//! `narrate_pass::brief` prints it as `MARKED`. What was missing, and what this module supplies, is the form
//! and the two refusals in front of it.
//!
//! Wording note: §F3.7 in `spec/06-effects.md` elides every sentence with "…"; §A.7 spells them out, so §A.7
//! is what these constants hold verbatim.

use crate::cut::{EffectKind, Fx};
use crate::cut_speed;
use crate::fx_lane;
use crate::fx_volume;
use crate::render_fx;
use crate::tools;

// --- S1: where the mark goes ---------------------------------------------------------------------------------------

/// F3.7 S1 (`neither → "click a track or mark a stretch first — a label names a moment, so it needs one"`):
/// §A.7 verbatim, and this button's own sentence rather than a borrowed one — a label is placed AT a moment, so
/// what it lacks when nothing is marked is a moment. Asserted apart from [`fx_volume::NO_SECONDS`] and
/// [`cut_speed::NO_SECONDS`] below so the three cannot quietly collapse into one string with the noun swapped.
pub const NO_SECONDS: &str =
    "click a track or mark a stretch first \u{2014} a label names a moment, so it needs one";

/// F3.7 S2 (`empty name not placed ("type a name and it is marked — nothing is placed until then")`): §A.7
/// verbatim. This is the only effect whose Apply can be refused for want of an ANSWER rather than for want of
/// seconds: an unnamed mark says nothing to the narration writer, so it is not worth putting on the lane.
pub const NO_NAME: &str = "type a name and it is marked \u{2014} nothing is placed until then";

/// F3.7 S1 (`line..+2 s`): **P.policy.effectDefaultSeconds**, whose row reads "stop/speed/volume/label 2" —
/// the same §10 row [`fx_volume::LINE_SECONDS`] and [`cut_speed::STOP_SECONDS`] read at their own kinds'
/// values. Two seconds because that is the shortest span a clip can be asked to overlap and still be findable.
pub const LINE_SECONDS: f64 = 2.0;

/// F3.7 S3/S4 (`dur ≥ 0.4`): the label form's typed floor. §10 gives this number no `P.` row on purpose
/// (`params.rs`, see the note above `effects()`), so it takes a bare prefix — `effects.labelMinSeconds` — and
/// lives only here, uncatalogued: nothing tunes it, the form just refuses under it. It is higher than volume's
/// 0.1 s because a tag has to be wide enough to draw its tick and its dashed line (§A.7's lane row).
pub const MIN_SECONDS: f64 = 0.4;

/// F3.7 S3 (`Name (10 chars`): the cap §A.7 puts on the field. Over the limit the name is TRUNCATED rather
/// than refused — the cap is a display budget for the lane's tag, and refusing someone their own word for being
/// long is not this app's style; they can shorten it and re-apply if the truncated form is wrong.
pub const NAME_MAX_CHARS: usize = 10;

/// F3.7 S1: what the flowchart decides. `MarkAtLine` rather than volume's `LoudAtLine`: a label marks the
/// moment the line stands on, it does not make anything louder from there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pressed {
    /// A marked stretch: the mark covers the band's seconds.
    Stretched { t: f64, dur: f64 },
    /// No band worth the name, but a line: the mark starts there.
    MarkAtLine,
    /// Neither — [`NO_SECONDS`].
    Refused,
}

/// F3.7 S1 (`a selection, or a line? … neither → refusal`): marked band first, then the line, then the
/// refusal — the order every seconds-placed effect uses, since §A.7 gives a label the same "band or line..+2 s"
/// shape a volume has. A band under [`cut_speed::MIN_MARKED_SECONDS`] is not a band at all (that floor belongs
/// to every effect's press, which is why it is borrowed rather than restated) and falls through to the line
/// instead of refusing. Ends are ordered because a right-to-left drag is the same stretch.
pub fn press(selection: Option<(f64, f64)>, line: Option<f64>) -> Pressed {
    if let Some((a, b)) = selection {
        let (from, to) = (a.min(b), a.max(b));
        // Compared as "not below the floor" rather than "at or above it": 0.2 is not representable in binary, so
        // a band dragged to exactly the floor can come out one ulp under and be thrown away as a slipped click.
        // The trap is documented at [`cut_speed::press`]'s own test of the same bound.
        if to - from > cut_speed::MIN_MARKED_SECONDS
            || (to - from - cut_speed::MIN_MARKED_SECONDS).abs() < 1e-9
        {
            return Pressed::Stretched { t: from, dur: to - from };
        }
    }
    match line {
        Some(_) => Pressed::MarkAtLine,
        None => Pressed::Refused,
    }
}

/// F3.7 S1/S2: the record a press puts down BEFORE the name is given — the band's seconds or two from the line,
/// with `words` empty because the name is the question the form still has to answer. An unnamed mark is not
/// placeable ([`NO_NAME`]), which is why `initial` leaves the name blank rather than inventing one: inventing it
/// here would put a word on the lane that nobody typed.
pub fn initial(pressed: Pressed) -> Option<Fx> {
    let (t, dur) = match pressed {
        Pressed::Stretched { t, dur } => (t, dur),
        // `t` comes from the caller's red line; 0.0 here is the placeholder the thin page replaces, as in
        // `fx_volume::initial`, since this module is not given a playhead it cannot invent one.
        Pressed::MarkAtLine => (0.0, LINE_SECONDS),
        Pressed::Refused => return None,
    };
    Some(Fx { kind: "label".into(), t, dur, ..Default::default() })
}

/// F3.7 S1: the same record at a second someone named — the `line..+2 s` branch with its line supplied, so the
/// rule is testable without a playhead.
pub fn initial_at_line(line: f64) -> Fx {
    let mut fx = initial(Pressed::MarkAtLine).expect("a line is never refused");
    fx.t = line;
    fx
}

// --- S3: the form --------------------------------------------------------------------------------------------------

/// F3.7 S3 (`Form "Label at m:ss"`): titled by the ONE moment it marks, so unlike [`fx_volume::form_title`]'s
/// `a – b` pair this takes only the start. Through [`tools::mm_ss`], which pads — so it reads
/// `Label at 00:12`, matching every other status line on the page.
pub fn form_title(t: f64) -> String {
    format!("Label at {}", tools::mm_ss(t))
}

/// F3.7 S3 (`Name · Length (s)`): the fields, in §A.7's order. Only two, because a label has nothing else to
/// be: no fades (it fades nothing), no curve, no box, no gain. Order is the sentence a person reads — the name
/// first because it is the whole answer, the length after because it only bounds what the name covers.
pub const FORM_FIELDS: [&str; 2] = ["Name", "Length (s)"];

/// F3.7 S3 (§A.7's Name tooltip, whole): the examples are the point — they say a label is a PERSONAL name for
/// a moment, not a filename or a slug — and the middle clause is the promise that placing one is safe.
pub const NAME_HELP: &str = "what you call this moment -- \"the reveal\", \"boss fight\". It changes nothing in \
                            the video: it is written into the brief the narration writer is given\u{2026}";

/// F3.7 S3 (§A.7's Length wording, `the stretch a clip must overlap`): the length of a label is not how long
/// a sound lasts, it is how wide the marked moment is drawn and therefore how far a kept clip has to reach to
/// carry it. Kept short and factual because the field itself says the unit.
pub const LENGTH_HELP: &str = "how wide the marked moment is \u{2014} the stretch a clip must overlap";

/// F3.7 S3: what the form holds when Apply is pressed — the name as typed (in characters, the field's unit) and
/// the seconds as typed. There is no fade or curve here because a label has none: §1's table denies them, so a
/// field for one would ask a question the record cannot answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub t: f64,
    pub name: String,
    pub dur: f64,
}

/// F3.7 S3/S4: the record the form writes back. The name is checked FIRST and the length second, in that order,
/// because the name is the reason the effect exists: a 3-second mark with no name tells the writer nothing,
/// while a named mark that is slightly short is at least a real instruction someone can widen.
///
/// * empty or whitespace-only name → [`NO_NAME`] verbatim, nothing placed;
/// * `dur` under [`MIN_SECONDS`] → refused naming its floor, in the shape [`fx_volume::apply`] uses (what is
///   wrong, then the bound);
/// * otherwise the name is trimmed and cut to [`NAME_MAX_CHARS`] characters.
///
/// Nothing else on [`Fx`] is written: no fades, no ease, no box, no lane — `fx_record::uses` says a label owns
/// `Dur` and `Words` only, and writing more would put keys in the file that §1 marks `–`.
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
/// §A.7's placed sentence, whole. Both halves matter: the first reassures that no pixel moves, the second says
/// where the mark DOES go, so "changes nothing" is never mistaken for "does nothing".
pub const NOTHING_CHANGES: &str = "nothing changes in the video; the narration is told about it";

/// F3.7 S5 + §A.8 (`"label “…” at m:ss"`): the mark as the lane and the status line name it. §A.8's label
/// sentence carries no duration where the others do, because a label's width is a drawing detail rather than
/// something a reader of the list acts on; the seconds and the name are the whole entry.
pub fn label(fx: &Fx) -> String {
    format!("label \u{201c}{}\u{201d} at {}", fx.text, tools::mm_ss(fx.t))
}

/// F3.7 S5: what the page says after a label is placed.
pub fn placed_status(fx: &Fx) -> String {
    format!("{} \u{2014} {NOTHING_CHANGES}", label(fx))
}

/// F3.7 S5 (`never rendered`): the render's own answer to a label, delegated to the one place that decides it
/// ([`render_fx::is_rendered`], whose comment records that the mark survives in the record precisely because
/// this flow exists) rather than restated as a second `kind == "label"` test that could drift.
pub fn never_rendered(fx: &Fx) -> bool {
    !render_fx::is_rendered(&fx.kind)
}

/// F3.7 S5 (`Drawn as a tag`): a label IS drawn — on the effects lane, as a grey-white tag with its tick and
/// dashed line (§A.7's Lane row) — even while paused, which is what makes it visible to be judged. Pinned
/// against [`fx_volume::gain_has_no_visual`] in the test: heard-and-not-seen versus seen-and-not-rendered are
/// opposite conditions, and both kinds satisfy exactly one of them.
pub fn drawn_as_a_tag(kind: EffectKind) -> bool {
    fx_lane::drawn_paused(kind)
}
