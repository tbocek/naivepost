//! F3.2 Aspect ratio — `spec/06-effects.md` F3.2, checked against [`naivepost::fx_aspect`].
//!
//! The flow has one decision (is a zoom already staying?) and three sentences, so what these tests pin is which
//! sentence goes with which branch, what the pick stores, and that the zoom a shape brings arrives in the same undo
//! step as the shape. Strings are compared whole: this tree treats a reworded status line as a broken one. No
//! widget and no display — `rust/src/ui/window.rs` still renders only Prepare, so there is no dropdown to click yet.

use naivepost::cut::{Cut, Fx, History};
use naivepost::cut_screen;
use naivepost::fx_aspect as aspect;
use naivepost::fx_zoom;
use naivepost::params;

/// A zoom on the lane, staying or pulling back.
fn zoom_at(t: f64, stay: bool) -> Fx {
    Fx { kind: "zoom".into(), t, dur: 3.0, stay, ..Default::default() }
}

/// F3.2 (`Dropdown source / 9:16 / 1:1 / 4:5 / 16:9`) — the five shapes in the dropdown's own order, and the one
/// sentence the dropdown carries.
#[test]
fn f3_2_s1_the_dropdown_offers_five_shapes_and_says_what_one_is() {
    assert_eq!(aspect::ASPECTS, ["source", "9:16", "1:1", "4:5", "16:9"]);
    assert_eq!(aspect::SOURCE, "source");
    // The list starts where the flowchart starts: `source` is the first entry, not an afterthought.
    assert_eq!(aspect::ASPECTS[0], aspect::SOURCE);

    // The tooltip, whole — spec/inventory/effects.md §B spells it and F3.2 quotes it back.
    assert_eq!(
        aspect::DROPDOWN_HELP,
        concat!(
            "the shape of the finished video \u{2014} source is the footage's own, 9:16 is a vertical short. ",
            "The whole frame fits inside it (bars either side) until \u{25AD} View frames a region; ",
            "the outline on the preview is what the finished video shows"
        )
    );

    // Every shape but source is stored as its own name, and source is stored as nothing at all.
    for shape in aspect::ASPECTS {
        let want = if shape == aspect::SOURCE { "" } else { shape };
        assert_eq!(aspect::stored(shape), want, "{shape}");
    }
}

/// F3.2 (`source` → "aspect: the source's own — the video comes out the shape it was filmed") — the report branch,
/// which adds nothing and takes nothing away.
#[test]
fn f3_2_s2_source_is_the_footages_own_shape() {
    let chosen = aspect::choose(&Cut::default(), aspect::SOURCE);
    assert_eq!(chosen.aspect, "", "source is stored as no key at all (§10: `aspect (source)`)");
    assert_eq!(chosen.zoom, None, "the footage's own shape needs no reframing");
    assert_eq!(
        chosen.status,
        "aspect: the source's own \u{2014} the video comes out the shape it was filmed"
    );

    // A cut that was shaped and framed, asked to go back to source: the aspect goes, the zooms stay. Nothing is
    // deleted by a pick that says nothing about them (spec/00-principles.md: nothing is deleted, only marked).
    let framed = Cut {
        aspect: "9:16".into(),
        fx: vec![zoom_at(0.0, true), zoom_at(40.0, false)],
        ..Default::default()
    };
    let mut cut = framed.clone();
    let status = aspect::apply(&mut cut, aspect::SOURCE);
    assert_eq!(status, chosen.status);
    assert_eq!(cut.aspect, "");
    assert_eq!(cut.fx, framed.fx, "choosing a shape is not a way to remove someone's effects");
    // And the readout falls back to the shape it prints for an unset aspect — which is a reading, not a choice.
    assert_eq!(cut_screen::ASPECT_DEFAULT, "16:9");
    assert!(!fx_zoom::aspect_is_set(&cut));
}

/// F3.2 (non-source, `a staying zoom already?` → no) — the pick brings a staying zoom at 0:00 holding the whole
/// frame for one second, and says so.
#[test]
fn f3_2_s3_a_shape_with_no_staying_zoom_gets_one_at_0_00() {
    // P.policy.effectDefaultSeconds is 3 s for a zoom; this hold is F3.2's own second.
    assert_eq!(aspect::HOLD_SECONDS, 1.0);

    let chosen = aspect::choose(&Cut::default(), "9:16");
    assert_eq!(chosen.aspect, "9:16", "what was picked is what the file holds");
    let zoom = chosen.zoom.clone().expect("no staying zoom yet, so the shape needs one");
    assert_eq!(zoom.kind, "zoom");
    assert_eq!((zoom.t, zoom.dur), (0.0, aspect::HOLD_SECONDS));
    assert!(zoom.stay, "a reframing, not an event that pulls back out");
    // `stay` is what leaves it with no glides — the same rule F3.1's form greys fade-out for.
    assert_eq!((zoom.trans, zoom.tout), (0.0, 0.0));
    assert_eq!(fx_zoom::fades(true), (0.0, 0.0));
    // The whole frame, centred: §F3.2's word `centred`, and no camera of its own (§F3.2 names none).
    assert_eq!((zoom.cx, zoom.cy, zoom.hf), (Some(0.5), Some(0.5), Some(1.0)));
    assert_eq!(zoom.cam, 0);
    // Linear: an empty `ease` writes no key, so a cut with a shape and no hand-drawn curve stays byte-identical.
    assert_eq!(zoom.ease, "");

    assert_eq!(
        chosen.status,
        "aspect 9:16 \u{2014} a \u{2295} zoom at 0:00 holds the whole frame, centred"
    );

    // The other three shapes take the same branch with their own word in the sentence.
    for shape in ["1:1", "4:5", "16:9"] {
        let picked = aspect::choose(&Cut::default(), shape);
        assert_eq!(picked.aspect, shape);
        assert!(picked.zoom.is_some(), "{shape} with nothing staying still has bars to fill");
        assert_eq!(
            picked.status,
            format!("aspect {shape} \u{2014} a \u{2295} zoom at 0:00 holds the whole frame, centred")
        );
    }

    // `apply` is the same answer written into the cut.
    let mut cut = Cut::default();
    let status = aspect::apply(&mut cut, "9:16");
    assert_eq!(status, chosen.status);
    assert_eq!(cut.aspect, "9:16");
    assert_eq!(cut.fx.len(), 1);
    assert_eq!(cut.fx[0], chosen.zoom.unwrap());
}

/// F3.2 (non-source, `a staying zoom already?` → yes) — the zooms on the lane decide the framing, so no second
/// reframing is added. Only a staying ZOOM counts: the question is asked through the typed kind.
#[test]
fn f3_2_s4_a_staying_zoom_already_decides_the_framing() {
    let framed = Cut { aspect: String::new(), fx: vec![zoom_at(12.0, true)], ..Default::default() };
    let chosen = aspect::choose(&framed, "9:16");
    assert_eq!(chosen.aspect, "9:16", "the shape is still what was picked");
    assert_eq!(chosen.zoom, None, "a video already framed by a zoom is not reframed twice");
    assert_eq!(chosen.status, "aspect 9:16 \u{2014} the zooms on the lane decide the framing");

    // A pull-back zoom leaves the frame where it was, so it does not answer for the framing.
    let pulled = Cut { fx: vec![zoom_at(12.0, false)], ..Default::default() };
    assert!(!fx_zoom::a_staying_zoom_exists(&pulled.fx));
    assert!(aspect::choose(&pulled, "9:16").zoom.is_some());

    // Neither does a staying text: `stay` means something else on every other kind.
    let staying_text = Cut {
        fx: vec![Fx { kind: "text".into(), t: 12.0, stay: true, ..Default::default() }],
        ..Default::default()
    };
    assert!(!fx_zoom::a_staying_zoom_exists(&staying_text.fx));
    assert!(aspect::choose(&staying_text, "9:16").zoom.is_some());

    // Nothing on the lane at all is the same branch as a pull-back zoom.
    assert!(aspect::choose(&Cut::default(), "4:5").zoom.is_some());
}

/// F3.2 (`same Undo step`) — the aspect and the zoom it brings are one state of the cut, so ↶ takes both back at
/// once. Also the seam to F3.1 S3: the zoom this flow places is the `staying zoom exists` that stops the next hand
/// zoom staying by default.
#[test]
fn f3_2_s5_aspect_and_zoom_are_one_undo_step() {
    let before = Cut::default();
    let mut history = History::open(&before);
    let depth_before = history.depth();

    let mut cut = before.clone();
    let status = aspect::apply(&mut cut, "9:16");
    assert_eq!(status, "aspect 9:16 \u{2014} a \u{2295} zoom at 0:00 holds the whole frame, centred");
    // One push for the whole pick: the page records an edit around `apply`, not one per field it touched.
    history.push(&cut);
    assert_eq!(history.depth(), depth_before + 1, "the aspect and its zoom are one step, not two");

    assert!(history.can_undo());
    let back = history.undo().expect("one step to take back");
    let mut restored = cut.clone();
    back.restore(&mut restored);
    assert_eq!(restored.aspect, "", "the shape went");
    assert_eq!(restored.fx.len(), before.fx.len(), "and the zoom it brought went with it");

    // The readout says what was chosen (§05-cut#1's Aspect ratio row reads the cut, not a default).
    let rows = cut_screen::idle_readouts(64, &cut.aspect, 0.0, None, &cut, 0.0);
    let value = |label: &str| rows.iter().find(|row| row.label == label).unwrap().value.clone();
    assert_eq!(value("Aspect ratio"), "9:16");

    // F3.1 S3's default asks two questions and this pick answers both: an aspect is set, and a staying zoom now
    // exists — so the next zoom someone draws is a passing close-up rather than a second reframing.
    assert!(fx_zoom::aspect_is_set(&cut));
    assert!(fx_zoom::a_staying_zoom_exists(&cut.fx));
    assert!(!fx_zoom::stays_by_default(fx_zoom::aspect_is_set(&cut), fx_zoom::a_staying_zoom_exists(&cut.fx)));
    // Before the pick, with no shape, it was nought for the other reason; after a hand zoom that stays, again nought.
    assert!(!fx_zoom::stays_by_default(false, false));
}

/// F3.2 — the one number this flow holds is catalogued once under its bare name, beside §10's `P.` row for it.
#[test]
fn f3_2_s6_the_hold_is_catalogued_once() {
    // effects.aspectHoldSeconds — F3.2's own bare name, catalogued once inside `cut()`; §10 now files the same
    // value as `P.eng.aspectStaySeconds`, rowed beside it and reading the same constant (pinned in
    // tests/aspect_stay_length.rs). This id keeps Family::Other: the bare name has no `P.` prefix to inherit.
    let listed: Vec<&str> = params::cut().iter().map(|row| row.id).collect();
    assert_eq!(listed.iter().filter(|id| **id == "effects.aspectHoldSeconds").count(), 1);

    let row = params::cut().into_iter().find(|row| row.id == "effects.aspectHoldSeconds").unwrap();
    assert_eq!(row.spelled.parse::<f64>().unwrap(), aspect::HOLD_SECONDS);
    assert_eq!(row.from, "fx_aspect::HOLD_SECONDS", "a row must name the constant its rule reads");
    assert_eq!(params::family("effects.aspectHoldSeconds"), params::Family::Other);
}
