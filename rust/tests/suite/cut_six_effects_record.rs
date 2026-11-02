// §06-effects#1-record — spec/06-effects.md §1, the effect record's field table.
//
// §1 is a table with six columns, so most of this file walks it: for each kind, exactly its own fields are read
// and nothing else. The rest are the readings the table states outright — which frame a box is measured against,
// what an empty `ease`/`snd`/`lane` answers, when a rate is a stop and when a gain is silence — plus the legacy
// spellings §1's last paragraph settles. Everything is plain data; no widget and no display.

use naivepost::cut::{EffectKind, Fx};
use naivepost::fx_record as rec;

/// Every kind this build knows — the six columns of §1's table.
const KINDS: [EffectKind; 6] = [
    EffectKind::Zoom,
    EffectKind::Speed,
    EffectKind::Text,
    EffectKind::Svg,
    EffectKind::Volume,
    EffectKind::Label,
];

/// §06-effects#1-record — every row of §1's table except `t`, which needs no row: an effect that happens at no
/// second is not an effect. Walked by s11 below, so the table cannot lose a column unnoticed.
const FIELDS: [rec::Field; 16] = [
    rec::Field::Dur,
    rec::Field::Trans,
    rec::Field::Tout,
    rec::Field::Ease,
    rec::Field::Cx,
    rec::Field::Cy,
    rec::Field::Hf,
    rec::Field::Wf,
    rec::Field::Stay,
    rec::Field::Rate,
    rec::Field::Snd,
    rec::Field::Gain,
    rec::Field::Words,
    rec::Field::Source,
    rec::Field::Cam,
    rec::Field::Lane,
];

fn fx(kind: &str, t: f64) -> Fx {
    Fx {
        kind: kind.into(),
        t,
        ..Default::default()
    }
}

/// §06-effects#1-record — the table's first row: `t, dur` for all six kinds, `dur` being the total length
/// including the fades, i.e. the width of the bar.
#[test]
fn sec_06_effects_1_record_s1_time_and_length() {
    for kind in KINDS {
        assert!(rec::uses(kind, rec::Field::Dur), "{kind:?} has a length");
    }
    // The bar is `dur` wide and `dur` alone — the fades are inside it, not added to it.
    let glide = Fx {
        kind: "zoom".into(),
        t: 30.0,
        dur: 5.0,
        trans: 1.0,
        tout: 2.0,
        ..Default::default()
    };
    assert_eq!(glide.spans(), (30.0, 35.0));
    let hard = Fx {
        kind: "label".into(),
        t: 4.0,
        dur: 2.0,
        ..Default::default()
    };
    assert_eq!(hard.spans(), (4.0, 6.0), "a label has a length too");
    // And an effect with no length covers no time at all — a moment, not a stretch.
    assert_eq!(fx("label", 9.0).spans(), (9.0, 9.0));
}

/// §06-effects#1-record — `stay`: pull back (false) or reframe until the next zoom (true), on a zoom only.
#[test]
fn sec_06_effects_1_record_s4_stay() {
    for kind in KINDS {
        assert_eq!(
            rec::uses(kind, rec::Field::Stay),
            matches!(kind, EffectKind::Zoom),
            "{kind:?}"
        );
    }
    // The default is the pull-back: an effect that says nothing about holding returns the frame afterwards.
    assert!(!fx("zoom", 3.0).stay);
    assert!(
        Fx {
            stay: true,
            ..fx("zoom", 3.0)
        }
        .stay
    );
}

/// §06-effects#1-record — `text / src`: the words of a caption and the name of a label share one key; an svg's
/// file has its own.
#[test]
fn sec_06_effects_1_record_s7_words_and_drawing() {
    use rec::Field;
    for kind in KINDS {
        let wordy = matches!(kind, EffectKind::Text | EffectKind::Label);
        assert_eq!(rec::uses(kind, Field::Words), wordy, "{kind:?}");
        assert_eq!(
            rec::uses(kind, Field::Source),
            matches!(kind, EffectKind::Svg),
            "{kind:?}"
        );
    }

    // One key, two meanings — the kind says which.
    let caption = Fx {
        text: "the reveal".into(),
        ..fx("text", 2.0)
    };
    let marked = Fx {
        text: "boss fight".into(),
        ..fx("label", 40.0)
    };
    assert_eq!(rec::words(&caption), "the reveal");
    assert_eq!(rec::words(&marked), "boss fight");
    // A path is a path only on a drawing, so a caption that looks like one stays words.
    let lookalike = Fx {
        text: "assets/tier.svg".into(),
        ..fx("text", 2.0)
    };
    assert_eq!(rec::words(&lookalike), "assets/tier.svg");
    assert_eq!(
        rec::drawing(&lookalike),
        "",
        "and it is not the drawing's file"
    );
    let drawn = Fx {
        src: "assets/tier.svg".into(),
        ..fx("svg", 2.0)
    };
    assert_eq!(rec::drawing(&drawn), "assets/tier.svg");
}

/// §06-effects#1-record — `lane (new)` on a volume: "" is the whole bed, a lane's name that lane only.
#[test]
fn sec_06_effects_1_record_s9_lane() {
    for kind in KINDS {
        assert_eq!(
            rec::uses(kind, rec::Field::Lane),
            matches!(kind, EffectKind::Volume),
            "{kind:?}"
        );
    }
    let bed = Fx {
        dur: 2.0,
        gain: 0.5,
        ..fx("volume", 6.0)
    };
    assert!(rec::rides_whole_bed(&bed), "an empty lane is the whole bed");
    assert_eq!(bed.lane(), None);

    let named = Fx {
        dur: 2.0,
        gain: 0.5,
        lane: "mic".into(),
        ..fx("volume", 6.0)
    };
    assert!(
        !rec::rides_whole_bed(&named),
        "a named lane is that lane only"
    );
    assert_eq!(named.lane(), Some("mic"));

    // A kind with no `lane` row rides nothing, not even the bed: an empty string there means nothing.
    assert!(!rec::rides_whole_bed(&fx("text", 6.0)));
}

/// §06-effects#1-record — the table as a whole: six kinds by fifteen rows (the two ends of `trans, tout, ease`
/// and of `rate, snd` are one answer each), so a kind cannot gain or lose a field without this noticing. The
/// per-row assertions above say *which* answer is right; this one says there are no others.
#[test]
fn sec_06_effects_1_record_s11_the_table_has_no_other_answers() {
    // What each kind reads, written out as the table's own columns.
    let expected: [(EffectKind, &[rec::Field]); 6] = [
        (
            EffectKind::Zoom,
            &[
                rec::Field::Dur,
                rec::Field::Trans,
                rec::Field::Tout,
                rec::Field::Ease,
                rec::Field::Cx,
                rec::Field::Cy,
                rec::Field::Hf,
                rec::Field::Stay,
                rec::Field::Cam,
            ],
        ),
        (
            EffectKind::Speed,
            &[
                rec::Field::Dur,
                rec::Field::Trans,
                rec::Field::Tout,
                rec::Field::Ease,
                rec::Field::Rate,
                rec::Field::Snd,
            ],
        ),
        (
            EffectKind::Text,
            &[
                rec::Field::Dur,
                rec::Field::Trans,
                rec::Field::Tout,
                rec::Field::Ease,
                rec::Field::Cx,
                rec::Field::Cy,
                rec::Field::Hf,
                rec::Field::Wf,
                rec::Field::Words,
            ],
        ),
        (
            EffectKind::Svg,
            &[
                rec::Field::Dur,
                rec::Field::Trans,
                rec::Field::Tout,
                rec::Field::Ease,
                rec::Field::Cx,
                rec::Field::Cy,
                rec::Field::Hf,
                rec::Field::Wf,
                rec::Field::Source,
            ],
        ),
        (
            EffectKind::Volume,
            &[
                rec::Field::Dur,
                rec::Field::Trans,
                rec::Field::Tout,
                rec::Field::Ease,
                rec::Field::Gain,
                rec::Field::Lane,
            ],
        ),
        (EffectKind::Label, &[rec::Field::Dur, rec::Field::Words]),
    ];
    for (kind, read) in expected {
        for field in FIELDS {
            assert_eq!(
                rec::uses(kind, field),
                read.contains(&field),
                "{kind:?} / {field:?}"
            );
        }
    }
    // A `–` row is only "never read": a file may carry the key and this build keeps it untouched.
    let odd = Fx {
        kind: "label".into(),
        t: 1.0,
        gain: 3.0,
        cx: Some(0.4),
        ..Default::default()
    };
    assert_eq!(
        odd.gain, 3.0,
        "a field the kind does not read is still kept"
    );
    assert_eq!(odd.effect_kind(), Some(EffectKind::Label));
}
