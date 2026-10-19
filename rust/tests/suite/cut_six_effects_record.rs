// §06-effects#1-record — spec/06-effects.md §1, the effect record's field table.
//
// §1 is a table with six columns, so most of this file walks it: for each kind, exactly its own fields are read
// and nothing else. The rest are the readings the table states outright — which frame a box is measured against,
// what an empty `ease`/`snd`/`lane` answers, when a rate is a stop and when a gain is silence — plus the legacy
// spellings §1's last paragraph settles. Everything is plain data; no widget and no display.

use naivepost::cut::{self, EffectKind, Fx};
use naivepost::fx_record as rec;
use naivepost::layout::Tree;

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
    Fx { kind: kind.into(), t, ..Default::default() }
}

/// A throwaway project holding one cut file, so §1's legacy spellings are read the way an old project writes
/// them — through `cut::load`, which is where the migration happens.
fn with_cut(tag: &str, text: &str) -> (std::path::PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-fxrecord-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(dir.join("cut/cut.json"), text).unwrap();
    (root, Tree::new(&dir).unwrap())
}

/// §06-effects#1-record — the table's first row: `t, dur` for all six kinds, `dur` being the total length
/// including the fades, i.e. the width of the bar.
#[test]
fn sec_06_effects_1_record_s1_time_and_length() {
    for kind in KINDS {
        assert!(rec::uses(kind, rec::Field::Dur), "{kind:?} has a length");
    }
    // The bar is `dur` wide and `dur` alone — the fades are inside it, not added to it.
    let glide = Fx { kind: "zoom".into(), t: 30.0, dur: 5.0, trans: 1.0, tout: 2.0, ..Default::default() };
    assert_eq!(glide.spans(), (30.0, 35.0));
    let hard = Fx { kind: "label".into(), t: 4.0, dur: 2.0, ..Default::default() };
    assert_eq!(hard.spans(), (4.0, 6.0), "a label has a length too");
    // And an effect with no length covers no time at all — a moment, not a stretch.
    assert_eq!(fx("label", 9.0).spans(), (9.0, 9.0));
}

/// §06-effects#1-record — `trans, tout, ease`: glide in/out for a zoom, ramp in/out for a speed, fade in/out for
/// a text, an svg and a volume, and no fades at all on a label. `ease` stores "" for linear.
#[test]
fn sec_06_effects_1_record_s2_fades_and_ease() {
    use rec::Field;
    for kind in KINDS {
        let faded = !matches!(kind, EffectKind::Label);
        assert_eq!(rec::uses(kind, Field::Trans), faded, "{kind:?}");
        assert_eq!(rec::uses(kind, Field::Tout), faded, "{kind:?}");
        assert_eq!(rec::uses(kind, Field::Ease), faded, "{kind:?}");
    }

    // "" IS linear, and an empty ease writes no key — which is what keeps an old file byte-identical.
    assert!(rec::ease_is_linear(&fx("text", 1.0)), "the default is linear");
    assert!(rec::keeps_the_old_bytes(&fx("text", 1.0)));
    let shaped = Fx { ease: "quad".into(), ..fx("text", 1.0) };
    assert!(!rec::ease_is_linear(&shaped));
    assert!(!rec::keeps_the_old_bytes(&shaped), "a named shape is written");
    // A curve this build has never heard of survives: dropping it would re-time a fade nobody asked to change.
    assert!(rec::ease_survives("zigzag"), "an unknown shape is kept");
    assert!(!rec::ease_survives(""), "and linear is not a name");
}

/// §06-effects#1-record — `cx, cy, hf` and `wf`: source fractions on a zoom, output fractions on a text and an
/// svg, nothing on the other three; no box at all falls back to its kind's default (text: lower third, svg:
/// middle), which is where a model-proposed caption lands.
#[test]
fn sec_06_effects_1_record_s3_boxes_and_their_defaults() {
    use rec::Field;
    for kind in KINDS {
        let boxy = matches!(kind, EffectKind::Zoom | EffectKind::Text | EffectKind::Svg);
        let width = matches!(kind, EffectKind::Text | EffectKind::Svg);
        assert_eq!(rec::uses(kind, Field::Cx), boxy, "{kind:?}");
        assert_eq!(rec::uses(kind, Field::Cy), boxy, "{kind:?}");
        assert_eq!(rec::uses(kind, Field::Hf), boxy, "{kind:?}");
        assert_eq!(rec::uses(kind, Field::Wf), width, "{kind:?}");
    }

    // Which frame the fractions are read off: only a zoom looks at the picture it reframes. A zoom has no `wf`
    // for the same reason — its width follows the aspect of that source frame.
    assert!(rec::fractions_read_off_source_frame(EffectKind::Zoom));
    for other in [EffectKind::Speed, EffectKind::Text, EffectKind::Svg, EffectKind::Volume, EffectKind::Label] {
        assert!(!rec::fractions_read_off_source_frame(other), "{other:?}");
    }

    // No box at all: not a corner and no height, but the kind's own default.
    let caption = fx("text", 2.0);
    assert!(!caption.has_box());
    assert_eq!(caption.centre(), (0.5, 0.78), "a text with no box is a lower third");
    // Which is where the model's captions go: it proposes words and carries no box.
    let proposed = Fx { dur: 3.0, text: "the reveal".into(), ..fx("text", 2.0) };
    assert!(!proposed.has_box());
    assert_eq!(proposed.centre(), (0.5, 0.78));
    let drawing = fx("svg", 2.0);
    assert_eq!(drawing.centre(), (0.5, 0.5), "a drawing with no box is centred");

    // A box someone placed wins over the default, and one written fraction counts as a box.
    let placed = Fx { cx: Some(0.2), ..fx("text", 2.0) };
    assert!(placed.has_box());
    assert_eq!(placed.centre(), (0.2, 0.78), "the other two stay the default");
}

/// §06-effects#1-record — `stay`: pull back (false) or reframe until the next zoom (true), on a zoom only.
#[test]
fn sec_06_effects_1_record_s4_stay() {
    for kind in KINDS {
        assert_eq!(rec::uses(kind, rec::Field::Stay), matches!(kind, EffectKind::Zoom), "{kind:?}");
    }
    // The default is the pull-back: an effect that says nothing about holding returns the frame afterwards.
    assert!(!fx("zoom", 3.0).stay);
    assert!(Fx { stay: true, ..fx("zoom", 3.0) }.stay);
}

/// §06-effects#1-record — `rate, snd` on a speed only; rate 1 is the clip's own clock and 0 is a stop, and the
/// sound answer is "" / pitch / own / scene / mute.
#[test]
fn sec_06_effects_1_record_s5_rate_and_sound_answer() {
    use rec::Field;
    for kind in KINDS {
        assert_eq!(rec::uses(kind, Field::Rate), matches!(kind, EffectKind::Speed), "{kind:?}");
        assert_eq!(rec::uses(kind, Field::Snd), matches!(kind, EffectKind::Speed), "{kind:?}");
    }

    // A stop is a rate of nought, not a seventh kind: the picture holds and its sound is answered separately.
    let stop = Fx { dur: 2.0, rate: 0.0, ..fx("speed", 8.0) };
    assert!(rec::is_stop(&stop));
    assert!(!rec::own_clock(&stop), "a stop is not the clip running at itself");
    let own = Fx { dur: 2.0, rate: 1.0, ..fx("speed", 8.0) };
    assert!(rec::own_clock(&own));
    assert!(!rec::is_stop(&own));
    let half = Fx { dur: 2.0, rate: 0.5, ..fx("speed", 8.0) };
    assert!(!rec::is_stop(&half) && !rec::own_clock(&half), "any other rate is neither");
    // The same nought on another kind is not a stop — it has no rate to read.
    assert!(!rec::is_stop(&fx("zoom", 8.0)));

    // §1's four answers, in the table's order; "" is the absence of one.
    assert_eq!(rec::SND_ANSWERS, ["pitch", "own", "scene", "mute"]);
    let snd = |answer: &str| Fx { snd: answer.into(), ..fx("speed", 8.0) };
    assert_eq!(rec::snd_of(&snd("")), rec::Snd::Default);
    assert_eq!(rec::snd_of(&snd("pitch")), rec::Snd::Pitch);
    assert_eq!(rec::snd_of(&snd("own")), rec::Snd::Own);
    assert_eq!(rec::snd_of(&snd("scene")), rec::Snd::Scene);
    assert_eq!(rec::snd_of(&snd("mute")), rec::Snd::Mute);
    // An answer this build does not know behaves as the empty one — the clip's own sound, never silence.
    assert_eq!(rec::snd_of(&snd("whisper")), rec::Snd::Default);
    assert!(rec::snd_is_known(""), "the empty answer is known");
    for answer in rec::SND_ANSWERS {
        assert!(rec::snd_is_known(answer), "{answer}");
    }
    assert!(!rec::snd_is_known("whisper"));
}

/// §06-effects#1-record — `gain` on a volume only: linear, with 0 meaning silence; the ceiling is playbin's own.
#[test]
fn sec_06_effects_1_record_s6_gain() {
    for kind in KINDS {
        assert_eq!(rec::uses(kind, rec::Field::Gain), matches!(kind, EffectKind::Volume), "{kind:?}");
    }
    // P.eng.maxGain is the ceiling and this only reads it — 10 is what the volume property will take.
    assert_eq!(rec::gain_ceiling(), 10.0);

    let silence = Fx { dur: 2.0, gain: 0.0, ..fx("volume", 6.0) };
    assert!(rec::gain_is_silence(&silence), "0 is silence, not \"unset\"");
    assert!(!rec::gain_is_silence(&Fx { dur: 2.0, gain: 1.0, ..fx("volume", 6.0) }));
    assert!(!rec::gain_is_silence(&Fx { dur: 2.0, gain: 2.0, ..fx("volume", 6.0) }), "a boost");
}

/// §06-effects#1-record — `text / src`: the words of a caption and the name of a label share one key; an svg's
/// file has its own.
#[test]
fn sec_06_effects_1_record_s7_words_and_drawing() {
    use rec::Field;
    for kind in KINDS {
        let wordy = matches!(kind, EffectKind::Text | EffectKind::Label);
        assert_eq!(rec::uses(kind, Field::Words), wordy, "{kind:?}");
        assert_eq!(rec::uses(kind, Field::Source), matches!(kind, EffectKind::Svg), "{kind:?}");
    }

    // One key, two meanings — the kind says which.
    let caption = Fx { text: "the reveal".into(), ..fx("text", 2.0) };
    let marked = Fx { text: "boss fight".into(), ..fx("label", 40.0) };
    assert_eq!(rec::words(&caption), "the reveal");
    assert_eq!(rec::words(&marked), "boss fight");
    // A path is a path only on a drawing, so a caption that looks like one stays words.
    let lookalike = Fx { text: "assets/tier.svg".into(), ..fx("text", 2.0) };
    assert_eq!(rec::words(&lookalike), "assets/tier.svg");
    assert_eq!(rec::drawing(&lookalike), "", "and it is not the drawing's file");
    let drawn = Fx { src: "assets/tier.svg".into(), ..fx("svg", 2.0) };
    assert_eq!(rec::drawing(&drawn), "assets/tier.svg");
}

/// §06-effects#1-record — `cam (new)`: the camera row a zoom was framed on, and nothing for any other kind.
#[test]
fn sec_06_effects_1_record_s8_camera_row() {
    for kind in KINDS {
        assert_eq!(rec::uses(kind, rec::Field::Cam), matches!(kind, EffectKind::Zoom), "{kind:?}");
    }
    // Row 0 is a real row and not "unset" — the key is simply skipped when it holds.
    assert_eq!(rec::camera_row(&fx("zoom", 3.0)), Some(0));
    assert_eq!(rec::camera_row(&Fx { cam: 2, ..fx("zoom", 3.0) }), Some(2));
    for other in [EffectKind::Speed, EffectKind::Text, EffectKind::Svg, EffectKind::Volume, EffectKind::Label] {
        let name = match other {
            EffectKind::Speed => "speed",
            EffectKind::Text => "text",
            EffectKind::Svg => "svg",
            EffectKind::Volume => "volume",
            _ => "label",
        };
        assert_eq!(rec::camera_row(&Fx { cam: 2, ..fx(name, 3.0) }), None, "{other:?} is not framed on a camera");
    }
}

/// §06-effects#1-record — `lane (new)` on a volume: "" is the whole bed, a lane's name that lane only.
#[test]
fn sec_06_effects_1_record_s9_lane() {
    for kind in KINDS {
        assert_eq!(rec::uses(kind, rec::Field::Lane), matches!(kind, EffectKind::Volume), "{kind:?}");
    }
    let bed = Fx { dur: 2.0, gain: 0.5, ..fx("volume", 6.0) };
    assert!(rec::rides_whole_bed(&bed), "an empty lane is the whole bed");
    assert_eq!(bed.lane(), None);

    let named = Fx { dur: 2.0, gain: 0.5, lane: "mic".into(), ..fx("volume", 6.0) };
    assert!(!rec::rides_whole_bed(&named), "a named lane is that lane only");
    assert_eq!(named.lane(), Some("mic"));

    // A kind with no `lane` row rides nothing, not even the bed: an empty string there means nothing.
    assert!(!rec::rides_whole_bed(&fx("text", 6.0)));
}

/// §06-effects#1-record — legacy spellings: `view` → zoom, `mute` → `snd: "mute"`, and an old file comes back
/// byte-identical because a linear ease writes no key and neither spelling is ever written again.
#[test]
fn sec_06_effects_1_record_s10_legacy_spellings() {
    let (_root, tree) = with_cut(
        "legacy",
        r#"{"segs":[{"s":0,"e":30}],"fx":[
            {"kind":"view","t":30,"dur":5,"cx":0.3,"cy":0.5,"hf":0.6},
            {"kind":"speed","t":40,"dur":2,"rate":0,"mute":true},
            {"kind":"text","t":1,"dur":3,"text":"a caption"}]}"#,
    );
    let loaded = cut::load(&tree).expect("an old file loads");

    // A "view" was a region the camera kept: the same record under the name this build uses.
    assert_eq!(loaded.fx[0].kind, "zoom");
    assert_eq!(loaded.fx[0].effect_kind(), Some(EffectKind::Zoom));
    assert_eq!(rec::camera_row(&loaded.fx[0]), Some(0), "framed on the row that was default then");
    // The stop's old silence tick became an answer, so every rate can now be asked about its sound.
    assert_eq!(loaded.fx[1].snd, "mute");
    assert_eq!(rec::snd_of(&loaded.fx[1]), rec::Snd::Mute);
    assert!(!loaded.fx[1].mute);
    // A caption written with no box is still a lower third after the round trip.
    assert_eq!(loaded.fx[2].centre(), (0.5, 0.78));

    // Read once: neither old spelling is written again, and a linear ease writes no key at all.
    cut::save(&loaded, &tree).expect("the migrated cut saves");
    let text = std::fs::read_to_string(tree.cut_json()).unwrap();
    assert!(!text.contains("\"view\""), "{text}");
    assert!(!text.contains("\"mute\":"), "the legacy tick is gone: {text}");
    assert!(text.contains("\"snd\":\"mute\"") || text.contains("\"snd\": \"mute\""), "{text}");
    assert!(!text.contains("\"ease\""), "linear stays unwritten: {text}");

    // And saving again changes nothing: the file this build wrote is stable.
    let again = cut::load(&tree).expect("the saved cut loads");
    cut::save(&again, &tree).expect("and saves");
    assert_eq!(std::fs::read_to_string(tree.cut_json()).unwrap(), text);
}

/// §06-effects#1-record — the table as a whole: six kinds by fifteen rows (the two ends of `trans, tout, ease`
/// and of `rate, snd` are one answer each), so a kind cannot gain or lose a field without this noticing. The
/// per-row assertions above say *which* answer is right; this one says there are no others.
#[test]
fn sec_06_effects_1_record_s11_the_table_has_no_other_answers() {
    // What each kind reads, written out as the table's own columns.
    let expected: [(EffectKind, &[rec::Field]); 6] = [
        (EffectKind::Zoom, &[rec::Field::Dur, rec::Field::Trans, rec::Field::Tout, rec::Field::Ease,
            rec::Field::Cx, rec::Field::Cy, rec::Field::Hf, rec::Field::Stay, rec::Field::Cam]),
        (EffectKind::Speed, &[rec::Field::Dur, rec::Field::Trans, rec::Field::Tout, rec::Field::Ease,
            rec::Field::Rate, rec::Field::Snd]),
        (EffectKind::Text, &[rec::Field::Dur, rec::Field::Trans, rec::Field::Tout, rec::Field::Ease,
            rec::Field::Cx, rec::Field::Cy, rec::Field::Hf, rec::Field::Wf, rec::Field::Words]),
        (EffectKind::Svg, &[rec::Field::Dur, rec::Field::Trans, rec::Field::Tout, rec::Field::Ease,
            rec::Field::Cx, rec::Field::Cy, rec::Field::Hf, rec::Field::Wf, rec::Field::Source]),
        (EffectKind::Volume, &[rec::Field::Dur, rec::Field::Trans, rec::Field::Tout, rec::Field::Ease,
            rec::Field::Gain, rec::Field::Lane]),
        (EffectKind::Label, &[rec::Field::Dur, rec::Field::Words]),
    ];
    for (kind, read) in expected {
        for field in FIELDS {
            assert_eq!(rec::uses(kind, field), read.contains(&field), "{kind:?} / {field:?}");
        }
    }
    // A `–` row is only "never read": a file may carry the key and this build keeps it untouched.
    let odd = Fx { kind: "label".into(), t: 1.0, gain: 3.0, cx: Some(0.4), ..Default::default() };
    assert_eq!(odd.gain, 3.0, "a field the kind does not read is still kept");
    assert_eq!(odd.effect_kind(), Some(EffectKind::Label));
}
