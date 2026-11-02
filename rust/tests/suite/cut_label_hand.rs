//! F3.7 Label by hand — `spec/06-effects.md` F3.7, with §A.7 and §A.8 of `spec/inventory/effects.md`.
//!
//! A label is the one effect that changes nothing in the picture, so every assertion here is a string or a
//! number: where its moment comes from (S1), what the press leaves unnamed (S2), the form's two fields (S3),
//! the name refusal and the 0.4 s length floor (S4), and the fact that the mark IS drawn on the lane while the
//! render never sees it and the narration brief reads it as MARKED (S5). Strings are compared whole, since a
//! reworded refusal is a different refusal; floats go through a tolerance because 0.4 and 0.2 are not
//! representable in binary, and every expected number below is derived from the rule it tests rather than read
//! off a run. No widget and no display: the page's wire is proven in `tests/suite/label_form_widgets.rs`.

use naivepost::cut_speed;
use naivepost::fx_label::{self, Pressed};
use naivepost::fx_volume;

use crate::common;
use common::assert_close;

/// The form as Apply would receive it: a named moment at 12 s over `dur` seconds.
fn form(name: &str, dur: f64) -> fx_label::Form {
    fx_label::Form {
        t: 12.0,
        name: name.into(),
        dur,
    }
}

// --- S1: a band, a line, or neither -------------------------------------------------------------------------------

/// S1 (`band or line..+2 s → neither → "click a track or mark a stretch first — a label names a moment, so it
/// needs one"`): the marked band wins, then the red line, then the refusal. The floor is
/// [`cut_speed::MIN_MARKED_SECONDS`]'s, borrowed because it belongs to every effect's press; under it a band is
/// a click that slipped and the line answers instead of refusing.
#[test]
fn f3_7_s1_a_band_or_a_line_and_neither_refuses() {
    let band = fx_label::press(Some((30.0, 47.5)), None);
    assert_eq!(band, Pressed::Stretched { t: 30.0, dur: 17.5 });

    // A right-to-left drag is the same stretch, so the record never has to carry a negative length.
    assert_eq!(fx_label::press(Some((47.5, 30.0)), None), band);

    // The floor itself counts as a band. 0.2 is not representable in binary, so the width is built from the
    // bound rather than written as `30.2 - 30.0`, which would be a hair under it and get thrown away.
    let (from, to) = (30.0, 30.0 + cut_speed::MIN_MARKED_SECONDS);
    assert_eq!(
        fx_label::press(Some((from, to)), None),
        Pressed::Stretched {
            t: from,
            dur: to - from
        }
    );

    // Under the floor it is not a band: with a line the line answers …
    assert_eq!(
        fx_label::press(Some((30.0, 30.1)), Some(12.0)),
        Pressed::MarkAtLine
    );
    // … and without one there is no moment to name.
    assert_eq!(fx_label::press(Some((30.0, 30.1)), None), Pressed::Refused);

    // Only a line: the mark starts where the red line is.
    assert_eq!(fx_label::press(None, Some(12.0)), Pressed::MarkAtLine);
    assert_eq!(
        fx_label::press(None, Some(0.0)),
        Pressed::MarkAtLine,
        "the first second is a line like any other"
    );

    // Neither — the refusal whole. §F3.7 lists one sentence per button, so a label's must not be volume's or
    // speed's with the noun swapped at print time; asserted apart or they would quietly become one string.
    assert_eq!(fx_label::press(None, None), Pressed::Refused);
    assert_eq!(
        fx_label::NO_SECONDS,
        "click a track or mark a stretch first \u{2014} a label names a moment, so it needs one"
    );
    assert_ne!(
        fx_label::NO_SECONDS,
        fx_volume::NO_SECONDS,
        "a label asks in a label's words"
    );
    assert_ne!(
        fx_label::NO_SECONDS,
        cut_speed::NO_SECONDS,
        "...and in neither of the other two's"
    );
}

// --- S2: the first record ------------------------------------------------------------------------------------------

/// S1/S2 (`line..+2 s`, empty name not yet answered): what lands before the form is touched — the band's or the
/// line's seconds with NO NAME, and nothing that would put a fade, a box or a lane on the record.
#[test]
fn f3_7_s2_the_first_mark_is_unnamed_and_two_seconds_wide() {
    // P.policy.effectDefaultSeconds ("stop/speed/volume/label 2") is where LINE_SECONDS comes from.
    assert_close("a line gives two seconds", fx_label::LINE_SECONDS, 2.0);

    let band = fx_label::initial(Pressed::Stretched { t: 30.0, dur: 17.5 })
        .expect("a band marks something");
    assert_eq!(band.kind, "label");
    assert_close("the band's own start", band.t, 30.0);
    assert_close("the band's own length", band.dur, 17.5);
    // The name is the question the form still has to answer. Leaving it blank is what makes S4's refusal real:
    // had `initial` invented a placeholder, an unnamed mark would reach the lane.
    assert_eq!(band.text, "", "no name yet -- the form asks for it");

    // The line branch: same record, starting at the second someone named. `initial` cannot know the playhead, so
    // it leaves 0.0 for the thin page to replace; `initial_at_line` is the same rule with the line supplied.
    let at = fx_label::initial_at_line(12.0);
    assert_eq!(at.kind, "label");
    assert_close("at the line", at.t, 12.0);
    assert_close("P.policy.effectDefaultSeconds", at.dur, 2.0);
    assert_eq!(at.text, "");

    // Nothing else is written: no fades, no ease, no box, no stay, no rate, no gain, no lane.
    assert_eq!((at.trans, at.tout, at.ease.as_str()), (0.0, 0.0, ""));
    assert_eq!((at.cx, at.cy, at.hf, at.wf), (None, None, None, None));
    assert_eq!(
        (at.stay, at.rate, at.snd.as_str(), at.gain),
        (false, 0.0, "", 0.0)
    );
    assert_eq!(at.lane, "", "an unnamed mark owns no lane either");

    // The refusal marks nothing.
    assert_eq!(fx_label::initial(Pressed::Refused), None);
}

// --- S3: the form ---------------------------------------------------------------------------------------------------

// --- S4: the name refusal and the length floor ------------------------------------------------------------------------

/// S2/S4 (`empty name not placed`, `dur ≥ 0.4`): the two ways Apply can say no, and the truncation over the
/// 10-char cap. The name is checked BEFORE the length because the name is the reason the effect exists.
#[test]
fn f3_7_s4_the_name_floor_and_the_length_floor() {
    // Empty name — refused with §A.7's sentence whole, in both spellings of "empty".
    assert_eq!(
        fx_label::NO_NAME,
        "type a name and it is marked \u{2014} nothing is placed until then"
    );
    assert_eq!(
        fx_label::apply(&form("", 3.0)),
        Err(fx_label::NO_NAME.to_string())
    );
    assert_eq!(
        fx_label::apply(&form("   ", 3.0)),
        Err(fx_label::NO_NAME.to_string()),
        "whitespace is not a name either -- it would draw an empty tag"
    );
    // And the name refusal outranks the length refusal: an unnamed AND too-short mark is reported as unnamed,
    // because naming it is the step that makes the rest worth fixing.
    assert_eq!(
        fx_label::apply(&form("", 0.1)),
        Err(fx_label::NO_NAME.to_string())
    );

    // Length floor: 0.4 taken, one tenth under refused naming both numbers. Built from the constant so the
    // accepted case is not undone by 0.4's binary representation.
    assert!(fx_label::apply(&form("reveal", fx_label::MIN_SECONDS)).is_ok());
    assert_eq!(
        fx_label::apply(&form("reveal", 0.3)),
        Err(
            "a label of 0.30 s is shorter than the shortest one this form takes (0.4) s"
                .to_string()
        )
    );

    // Over the cap the name is TRUNCATED, not refused: the cap is the tag's display budget. Counted in CHARS,
    // not bytes, so a multi-byte name is cut at the character boundary rather than mid-glyph.
    let long = fx_label::apply(&form("the final boss fight", 3.0)).unwrap();
    assert_eq!(long.text.chars().count(), fx_label::NAME_MAX_CHARS);
    assert_eq!(long.text, "the final ");
    let unicode = fx_label::apply(&form("★ boss fight ★", 3.0)).unwrap();
    assert_eq!(
        unicode.text.chars().count(),
        fx_label::NAME_MAX_CHARS,
        "cut at characters, not bytes"
    );
    assert!(!unicode.text.ends_with('\u{FFFD}'), "never a broken glyph");
    // Exactly ten passes through untouched ("ten chars!" is ten characters, not eleven).
    assert_eq!(
        fx_label::apply(&form("ten chars!", 3.0)).unwrap().text,
        "ten chars!"
    );

    // What a good apply writes, and what it does NOT: the name lands in `text` (the shared key), the seconds pass
    // through, and no fades / box / lane appear.
    let ok = fx_label::apply(&form("boss fight", 3.0)).unwrap();
    assert_eq!(
        (ok.kind.as_str(), ok.text.as_str()),
        ("label", "boss fight")
    );
    assert_close("the form's own second", ok.t, 12.0);
    assert_close("the form's own length", ok.dur, 3.0);
    assert_eq!(
        (ok.trans, ok.tout, ok.ease.as_str()),
        (0.0, 0.0, ""),
        "a label fades nothing"
    );
    assert_eq!(
        (ok.cx, ok.cy, ok.hf, ok.wf),
        (None, None, None, None),
        "and draws no box"
    );
    assert_eq!(ok.lane, "");
}

// --- S5: marked, drawn on the lane, never rendered ---------------------------------------------------------------------
