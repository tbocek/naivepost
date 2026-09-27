//! F3.7 Label by hand — `spec/06-effects.md` F3.7, with §A.7 and §A.8 of `spec/inventory/effects.md`.
//!
//! A label is the one effect that changes nothing in the picture, so every assertion here is a string or a
//! number: where its moment comes from (S1), what the press leaves unnamed (S2), the form's two fields (S3),
//! the name refusal and the 0.4 s length floor (S4), and the fact that the mark IS drawn on the lane while the
//! render never sees it and the narration brief reads it as MARKED (S5). Strings are compared whole, since a
//! reworded refusal is a different refusal; floats go through a tolerance because 0.4 and 0.2 are not
//! representable in binary, and every expected number below is derived from the rule it tests rather than read
//! off a run. No widget and no display: the page's wire is proven in `tests/label_form_widgets.rs`.

use naivepost::cut::{EffectKind, Fx};
use naivepost::cut_speed;
use naivepost::fx_label::{self, Pressed};
use naivepost::fx_lane;
use naivepost::fx_record::{self, Field};
use naivepost::fx_volume;
use naivepost::narrate_pass;
use naivepost::render_fx;
use naivepost::tools;

/// Floats from a rule, never `assert_eq!`: 0.4 is not exactly representable, so a bound built by addition is a
/// hair off the literal and must be compared with slack.
const EPS: f64 = 1e-9;

fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < EPS, "{asked}: got {got}, want {want}");
}

/// A placed label: the name lives in `text`, the same field a caption's words use (§1's note on the shared key).
fn marked(name: &str, t: f64, dur: f64) -> Fx {
    Fx { kind: "label".into(), t, dur, text: name.into(), ..Default::default() }
}

/// The form as Apply would receive it: a named moment at 12 s over `dur` seconds.
fn form(name: &str, dur: f64) -> fx_label::Form {
    fx_label::Form { t: 12.0, name: name.into(), dur }
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
    assert_eq!(fx_label::press(Some((from, to)), None), Pressed::Stretched { t: from, dur: to - from });

    // Under the floor it is not a band: with a line the line answers …
    assert_eq!(fx_label::press(Some((30.0, 30.1)), Some(12.0)), Pressed::MarkAtLine);
    // … and without one there is no moment to name.
    assert_eq!(fx_label::press(Some((30.0, 30.1)), None), Pressed::Refused);

    // Only a line: the mark starts where the red line is.
    assert_eq!(fx_label::press(None, Some(12.0)), Pressed::MarkAtLine);
    assert_eq!(fx_label::press(None, Some(0.0)), Pressed::MarkAtLine, "the first second is a line like any other");

    // Neither — the refusal whole. §F3.7 lists one sentence per button, so a label's must not be volume's or
    // speed's with the noun swapped at print time; asserted apart or they would quietly become one string.
    assert_eq!(fx_label::press(None, None), Pressed::Refused);
    assert_eq!(
        fx_label::NO_SECONDS,
        "click a track or mark a stretch first \u{2014} a label names a moment, so it needs one"
    );
    assert_ne!(fx_label::NO_SECONDS, fx_volume::NO_SECONDS, "a label asks in a label's words");
    assert_ne!(fx_label::NO_SECONDS, cut_speed::NO_SECONDS, "...and in neither of the other two's");
}

// --- S2: the first record ------------------------------------------------------------------------------------------

/// S1/S2 (`line..+2 s`, empty name not yet answered): what lands before the form is touched — the band's or the
/// line's seconds with NO NAME, and nothing that would put a fade, a box or a lane on the record.
#[test]
fn f3_7_s2_the_first_mark_is_unnamed_and_two_seconds_wide() {
    // P.policy.effectDefaultSeconds ("stop/speed/volume/label 2") is where LINE_SECONDS comes from.
    assert_close("a line gives two seconds", fx_label::LINE_SECONDS, 2.0);

    let band = fx_label::initial(Pressed::Stretched { t: 30.0, dur: 17.5 }).expect("a band marks something");
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
    assert_eq!((at.stay, at.rate, at.snd.as_str(), at.gain), (false, 0.0, "", 0.0));
    assert_eq!(at.lane, "", "an unnamed mark owns no lane either");

    // The refusal marks nothing.
    assert_eq!(fx_label::initial(Pressed::Refused), None);
}

// --- S3: the form ---------------------------------------------------------------------------------------------------

/// S3 (`Form "Label at m:ss"` · Name (10 chars; tooltip "what you call this moment -- the reveal, boss
/// fight. …") · Length (s) = the stretch a clip must overlap): the title's ONE moment, the two fields in
/// §A.7's order, and the help that says a label is safe to place.
#[test]
fn f3_7_s3_the_form_and_its_two_fields() {
    // Asserted through tools::mm_ss rather than spelled only once: mm_ss pads, so the title reads
    // `Label at 00:12`. A label takes only the start -- unlike volume's `a – b` pair -- because it names a
    // MOMENT, not a span.
    assert_eq!(fx_label::form_title(12.0), format!("Label at {}", tools::mm_ss(12.0)));
    assert_eq!(fx_label::form_title(12.0), "Label at 00:12");
    assert!(
        !fx_label::form_title(12.0).contains('\u{2013}'),
        "no en dash: this title has one time in it, not a range"
    );

    // Two fields, in §A.7's order, name first: it is the whole answer, the length only bounds what it covers.
    assert_eq!(fx_label::FORM_FIELDS, ["Name", "Length (s)"]);

    // The Name tooltip is §A.7's sentence whole, examples included -- they are what say a label is a person's
    // own word for a moment rather than a slug.
    assert_eq!(
        fx_label::NAME_HELP,
        "what you call this moment -- \"the reveal\", \"boss fight\". It changes nothing in the video: it is \
         written into the brief the narration writer is given\u{2026}"
    );
    assert!(fx_label::NAME_HELP.contains("brief"), "the tooltip tells the truth about where the name goes");
    assert!(fx_label::NAME_HELP.contains("changes nothing"), "...and that placing one is safe");

    // The Length help is §A.7's own phrasing of what the number means.
    assert!(
        fx_label::LENGTH_HELP.contains("the stretch a clip must overlap"),
        "the length field says what the number means: {}",
        fx_label::LENGTH_HELP
    );

    // The cap §A.7 puts on the field.
    assert_eq!(fx_label::NAME_MAX_CHARS, 10);
    // effects.labelMinSeconds: bare prefix, no `P.` row in §10 (params.rs says so on purpose).
    assert_close("effects.labelMinSeconds", fx_label::MIN_SECONDS, 0.4);
}

// --- S4: the name refusal and the length floor ------------------------------------------------------------------------

/// S2/S4 (`empty name not placed`, `dur ≥ 0.4`): the two ways Apply can say no, and the truncation over the
/// 10-char cap. The name is checked BEFORE the length because the name is the reason the effect exists.
#[test]
fn f3_7_s4_the_name_floor_and_the_length_floor() {
    // Empty name — refused with §A.7's sentence whole, in both spellings of "empty".
    assert_eq!(fx_label::NO_NAME, "type a name and it is marked \u{2014} nothing is placed until then");
    assert_eq!(fx_label::apply(&form("", 3.0)), Err(fx_label::NO_NAME.to_string()));
    assert_eq!(
        fx_label::apply(&form("   ", 3.0)),
        Err(fx_label::NO_NAME.to_string()),
        "whitespace is not a name either -- it would draw an empty tag"
    );
    // And the name refusal outranks the length refusal: an unnamed AND too-short mark is reported as unnamed,
    // because naming it is the step that makes the rest worth fixing.
    assert_eq!(fx_label::apply(&form("", 0.1)), Err(fx_label::NO_NAME.to_string()));

    // Length floor: 0.4 taken, one tenth under refused naming both numbers. Built from the constant so the
    // accepted case is not undone by 0.4's binary representation.
    assert!(fx_label::apply(&form("reveal", fx_label::MIN_SECONDS)).is_ok());
    assert_eq!(
        fx_label::apply(&form("reveal", 0.3)),
        Err("a label of 0.30 s is shorter than the shortest one this form takes (0.4) s".to_string())
    );

    // Over the cap the name is TRUNCATED, not refused: the cap is the tag's display budget. Counted in CHARS,
    // not bytes, so a multi-byte name is cut at the character boundary rather than mid-glyph.
    let long = fx_label::apply(&form("the final boss fight", 3.0)).unwrap();
    assert_eq!(long.text.chars().count(), fx_label::NAME_MAX_CHARS);
    assert_eq!(long.text, "the final ");
    let unicode = fx_label::apply(&form("★ boss fight ★", 3.0)).unwrap();
    assert_eq!(unicode.text.chars().count(), fx_label::NAME_MAX_CHARS, "cut at characters, not bytes");
    assert!(!unicode.text.ends_with('\u{FFFD}'), "never a broken glyph");
    // Exactly ten passes through untouched ("ten chars!" is ten characters, not eleven).
    assert_eq!(fx_label::apply(&form("ten chars!", 3.0)).unwrap().text, "ten chars!");

    // What a good apply writes, and what it does NOT: the name lands in `text` (the shared key), the seconds pass
    // through, and no fades / box / lane appear.
    let ok = fx_label::apply(&form("boss fight", 3.0)).unwrap();
    assert_eq!((ok.kind.as_str(), ok.text.as_str()), ("label", "boss fight"));
    assert_close("the form's own second", ok.t, 12.0);
    assert_close("the form's own length", ok.dur, 3.0);
    assert_eq!((ok.trans, ok.tout, ok.ease.as_str()), (0.0, 0.0, ""), "a label fades nothing");
    assert_eq!((ok.cx, ok.cy, ok.hf, ok.wf), (None, None, None, None), "and draws no box");
    assert_eq!(ok.lane, "");
}

// --- S5: marked, drawn on the lane, never rendered ---------------------------------------------------------------------

/// S5 (`placed "… — nothing changes in the video; the narration is told about it"` · `Drawn as a tag; never
/// rendered; narration brief lists it as MARKED`): the three places a mark shows up afterwards, each proven
/// through the module that owns it rather than restated here.
#[test]
fn f3_7_s5_placed_it_is_marked_never_rendered() {
    let tag = marked("boss fight", 12.0, 3.0);

    // The status whole, including §A.7's em dash and both halves of its reassurance.
    assert_eq!(fx_label::NOTHING_CHANGES, "nothing changes in the video; the narration is told about it");
    assert_eq!(fx_label::placed_status(&tag), format!("{} \u{2014} {}", fx_label::label(&tag), fx_label::NOTHING_CHANGES));
    assert!(fx_label::placed_status(&tag).contains("boss fight"), "the name is what the page repeats back");
    assert!(fx_label::placed_status(&tag).contains("nothing changes in the video"));
    assert!(fx_label::placed_status(&tag).contains("the narration is told about it"));

    // §A.8's label sentence: `label "…" at m:ss` -- the name and the moment, and no duration where the other
    // kinds' sentences carry one, because a label's width is a drawing detail rather than a reading action.
    assert_eq!(fx_label::label(&tag), "label \u{201c}boss fight\u{201d} at 00:12");
    assert!(!fx_label::label(&tag).contains("for "), "§A.8 gives the label sentence no duration clause");

    // Never rendered: delegated to the render's own decision, so the two cannot drift apart.
    assert!(fx_label::never_rendered(&tag), "a label contributes no filter to any clip");
    assert!(!render_fx::is_rendered("label"), "...which is `render_fx::is_rendered` answering false");
    assert!(render_fx::is_rendered("zoom"), "and a kind that IS rendered proves the delegation works");

    // Drawn as a tag on the lane even while paused -- the opposite of a volume, which is heard and not seen.
    assert!(fx_label::drawn_as_a_tag(EffectKind::Label), "§A.7's grey-white tag, tick and dashed line");
    assert!(fx_lane::drawn_paused(EffectKind::Label));
    assert!(fx_volume::gain_has_no_visual(), "a volume has no visual at all; a label has one, and no render");

    // §1's table read at the label's row: the name and the length yes, everything else no. This is why the form
    // has two fields and not five.
    assert!(fx_record::uses(EffectKind::Label, Field::Dur));
    assert!(fx_record::uses(EffectKind::Label, Field::Words), "the name shares the caption's `text` key");
    for denied in [Field::Trans, Field::Tout, Field::Ease, Field::Cx, Field::Cy, Field::Hf, Field::Wf, Field::Gain] {
        assert!(!fx_record::uses(EffectKind::Label, denied), "a label owns no {denied:?}");
    }

    // The narration brief lists it as MARKED -- proven by building a brief with one label on it, through the
    // function that writes the brief rather than by quoting the match arm.
    let segs = vec![naivepost::cut::Seg { s: 0.0, e: 60.0, ..Default::default() }];
    let brief = narrate_pass::brief(&segs, &[], &[tag.clone()], "Narrator 1");
    assert!(brief.contains("MARKED"), "the brief carries the mark: {brief}");
    assert!(brief.contains("boss fight"), "...with the name the human typed: {brief}");
    assert!(!brief.contains("CAPTION"), "and a label is not filed as a caption");
}
