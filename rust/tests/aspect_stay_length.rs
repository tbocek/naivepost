// §10-parameters · P.eng.aspectStaySeconds — the length of the staying zoom an aspect change places (1.0;
// prototype `cut_fx.go` 1.0). The rule already existed: `fx_aspect::HOLD_SECONDS`, consumed once by
// `fx_aspect::choose` when a shape is picked on footage nobody has framed yet. What this file pins is that the
// §10 row points at that rule rather than at a copy of the number, that the 1.0 s lands in exactly one of F3.2's
// three branches and not in the other two, and that it stays distinct from the hand-placed zoom's 3 s default.
//
// Status lines are compared whole, as `tests/cut_aspect_ratio.rs` does: this tree treats a reworded status line
// as a broken one.

use naivepost::cut::Cut;
use naivepost::{fx_aspect, fx_zoom, params};

/// Every list that carries rows. All five are chained because the bare `effects.aspectHoldSeconds` row lives in
/// `cut()` while §10's `P.` row must be found exactly once across the whole catalogue, not merely per page
/// (`tests/hands_off.rs`'s `row()` makes the same choice for the same reason).
fn anywhere(id: &str) -> params::Param {
    let mut found = params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
    found.pop().unwrap()
}

/// A number §10 writes with a decimal point compared as a number: `params::num` trims the trailing zero, so
/// comparing strings would pin a spelling neither side chose.
fn number_anywhere(id: &str) -> f64 {
    anywhere(id)
        .spelled
        .parse()
        .unwrap_or_else(|_| panic!("{id} spells a number"))
}

/// S1: `P.eng.aspectStaySeconds` = 1.0, held by `fx_aspect::HOLD_SECONDS`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_aspectstayseconds_s1_the_stay_is_1_0_and_catalogued_once() {
    // P.eng.aspectStaySeconds — "length of the staying zoom an aspect change places".
    assert_eq!(fx_aspect::HOLD_SECONDS, 1.0);
    assert_eq!(number_anywhere("P.eng.aspectStaySeconds"), 1.0);

    // The row names the constant its rule reads, not a copy of the number.
    assert_eq!(anywhere("P.eng.aspectStaySeconds").from, "fx_aspect::HOLD_SECONDS");

    // §10 §5.1 files it among the engineering constants, so the prefix must answer to Eng.
    assert_eq!(params::family("P.eng.aspectStaySeconds"), params::Family::Eng);

    // And the row sits in the Cut page's list, where F3.2's own bare row also lives.
    assert!(
        params::cut()
            .iter()
            .any(|row| row.id == "P.eng.aspectStaySeconds"),
        "the aspect's stay belongs to §05-cut#6's list, beside effects.aspectHoldSeconds"
    );
}

/// S2: the branch that DOES place the hold — a shape picked with no staying zoom on the lane. This is where
/// P.eng.aspectStaySeconds's 1.0 s is actually consumed: it becomes the new zoom's `dur`.
#[test]
fn p_eng_aspectstayseconds_s2_a_shape_with_no_staying_zoom_places_one_second_of_hold() {
    let cut = Cut::default();
    assert!(!fx_zoom::a_staying_zoom_exists(&cut.fx), "the fixture must really have nothing staying");

    let chosen = fx_aspect::choose(&cut, "9:16");
    let zoom = chosen.zoom.clone().expect("this branch places the framing zoom");
    assert_eq!(zoom.kind, "zoom");
    assert_eq!(zoom.t, 0.0, "at 0:00, the first second of the session");
    // The value under test: the hold is exactly §10's second, not some rounded stand-in.
    assert_eq!(zoom.dur, fx_aspect::HOLD_SECONDS);
    assert_eq!(zoom.dur, 1.0);
    assert!(zoom.stay, "a reframing stays; that is what makes it a camera move rather than an event");

    // A staying camera has nowhere to pull back to, so it gets no fades — [`fx_zoom::fades`] answers (0, 0)
    // for `stay`, and this zoom goes through it rather than setting the fields itself.
    assert_eq!((zoom.trans, zoom.tout), fx_zoom::fades(true));
    assert_eq!((zoom.trans, zoom.tout), (0.0, 0.0));

    // The whole frame, centred: `fx_zoom::SETTLED` read as a rect, so this cannot drift from what F3.1's
    // camera path calls the settled frame.
    assert_eq!(zoom.cx, Some(fx_zoom::SETTLED.cx));
    assert_eq!(zoom.cy, Some(fx_zoom::SETTLED.cy));
    assert_eq!(zoom.hf, Some(fx_zoom::SETTLED.hf));
    assert_eq!((zoom.cx, zoom.cy, zoom.hf), (Some(0.5), Some(0.5), Some(1.0)));
    assert_eq!(zoom.cam, 0, "no row asked for: the pick owns no lane of its own");

    // The line the page prints, whole.
    assert_eq!(chosen.aspect, "9:16");
    assert_eq!(
        chosen.status,
        "aspect 9:16 \u{2014} a \u{2295} zoom at 0:00 holds the whole frame, centred"
    );
}

/// S3: the branch that does NOT place a hold — a staying zoom is already on the lane, so the shape changes only
/// the shape. Asserting the fixture really had one keeps this branch from silently becoming S2's.
#[test]
fn p_eng_aspectstayseconds_s3_an_existing_staying_zoom_means_no_second_hold() {
    let mut cut = Cut::default();
    // A staying zoom someone placed by hand, at 12 s for 4 s — deliberately not 1.0 s, so a wrongly-added
    // second hold could not hide behind matching lengths.
    cut.fx.push(fx_zoom::new_zoom((0.4, 0.5, 0.6), 12.0, 4.0, true, 0));
    assert!(fx_zoom::a_staying_zoom_exists(&cut.fx), "the fixture must exercise THIS branch");
    let before = cut.fx.len();

    let chosen = fx_aspect::choose(&cut, "9:16");
    assert_eq!(chosen.zoom, None, "the lane already decides the framing; nothing is added");
    assert_eq!(chosen.aspect, "9:16", "the shape itself is still stored");
    assert_eq!(
        chosen.status,
        "aspect 9:16 \u{2014} the zooms on the lane decide the framing"
    );

    // And the existing zoom is untouched: its 4 s is not overwritten by the aspect's 1.0 s.
    assert_eq!(cut.fx.len(), before);
    assert_eq!(cut.fx[0].dur, 4.0);
    assert_ne!(cut.fx[0].dur, fx_aspect::HOLD_SECONDS);
}

/// S4: `source` asks for nothing, so no hold is placed either — the 1.0 s belongs to a shape, not to a report.
#[test]
fn p_eng_aspectstayseconds_s4_source_places_no_hold_at_all() {
    let cut = Cut::default();
    let chosen = fx_aspect::choose(&cut, fx_aspect::SOURCE);
    assert_eq!(chosen.zoom, None, "nothing is added for the footage's own shape");
    assert_eq!(chosen.aspect, "", "unset is stored empty, not as the word \"source\"");
    assert_eq!(
        chosen.status,
        "aspect: the source's own \u{2014} the video comes out the shape it was filmed"
    );

    // Through `apply` too: the effect list stays empty, so no stray 1.0 s zoom appears for a pick that asked
    // for nothing.
    let mut cut = Cut::default();
    fx_aspect::apply(&mut cut, fx_aspect::SOURCE);
    assert!(cut.fx.is_empty(), "a pick of `source` must not leave a hold lying on the lane");
    assert_eq!(cut.aspect, "");
}

/// S5: one constant, two rows — and the shape's hold is NOT the hand-placed zoom's default length.
#[test]
fn p_eng_aspectstayseconds_s5_one_constant_two_rows_and_not_the_hand_placed_default() {
    // §10's `P.` row and F3.2's bare row read ONE constant, so the two spellings cannot drift apart.
    assert_eq!(
        number_anywhere("P.eng.aspectStaySeconds"),
        number_anywhere("effects.aspectHoldSeconds")
    );
    assert_eq!(
        anywhere("P.eng.aspectStaySeconds").from,
        anywhere("effects.aspectHoldSeconds").from,
        "both rows must point at the same constant, not two copies of 1.0"
    );

    // P.policy.effectDefaultSeconds ("zoom 3") is fx_zoom::DEFAULT_SECONDS — a zoom someone placed by hand and
    // named. The shape's hold is a third of that because it exists to give the camera something to settle on
    // from the first frame, not to be watched. That row spells a whole family rather than a number ("zoom 3;
    // stop/speed/volume/label 2"), so the zoom cell is matched in its own words.
    assert_eq!(fx_zoom::DEFAULT_SECONDS, 3.0);
    assert_ne!(
        fx_aspect::HOLD_SECONDS, fx_zoom::DEFAULT_SECONDS,
        "an aspect's hold must not quietly become a hand-placed zoom's length"
    );
    let family = anywhere("P.policy.effectDefaultSeconds").spelled;
    assert_eq!(family, "zoom/text/svg 3; stop/speed/volume/label 2");
    assert!(
        !family.contains(&format!("zoom {}", fx_aspect::HOLD_SECONDS)),
        "the default-length family must not claim the aspect's hold as a zoom default: {family}"
    );

    // The shape and its 1.0 s hold arrive in the same call, which is what makes them one Undo step: the page
    // pushes history once around `apply`, not once per field.
    let mut cut = Cut::default();
    let status = fx_aspect::apply(&mut cut, "1:1");
    assert_eq!(cut.aspect, "1:1");
    assert_eq!(cut.fx.len(), 1, "the shape brought its hold with it");
    assert_eq!(cut.fx[0].t, 0.0);
    assert_eq!(cut.fx[0].dur, fx_aspect::HOLD_SECONDS);
    assert!(cut.fx[0].stay);
    assert!(status.contains("0:00"), "the line names the second the hold starts: {status}");
    assert_eq!(
        status,
        "aspect 1:1 \u{2014} a \u{2295} zoom at 0:00 holds the whole frame, centred"
    );
}
