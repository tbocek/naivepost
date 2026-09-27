// §06-effects#1-record — creating a record: a new effect carries exactly its kind's columns of §1's table.
//
// The reading side of §1 lives in `tests/cut_six_effects_record.rs`; this file is the other direction — what a
// record looks like when it is MADE. The rule under test is that `fx_record::blank` fills nothing beyond time,
// length and kind, so every field the table marks `–` for a kind costs no key in `cut.json`, and the fields the
// table DOES give read back as their kind's default (a caption with no box → lower third, a drawing → middle).

use naivepost::cut::{self, Cut, EffectKind, Fx};
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

/// A throwaway project holding one cut file, written by hand so §1's legacy spellings arrive the way an old
/// project writes them — through `cut::load`, where the migration happens.
fn with_cut(tag: &str, text: &str) -> (std::path::PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-fxrec-create-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(dir.join("cut/cut.json"), text).unwrap();
    (root, Tree::new(&dir).unwrap())
}

fn keys_of(fx: &Fx) -> Vec<String> {
    serde_json::to_value(fx)
        .expect("an Fx serialises")
        .as_object()
        .expect("an Fx is an object")
        .keys()
        .cloned()
        .collect()
}

/// §06-effects#1-record — a freshly recorded effect holds exactly its kind's columns: no key for any field
/// the table marks `–`, and `t`/`kind` always there.
#[test]
fn sec_06_effects_1_record_s12_a_blank_record_carries_only_its_kinds_columns() {
    for kind in KINDS {
        assert!(
            rec::recorded_keys_are_the_kinds_own(kind, 2.0),
            "{kind:?}: a fresh record must not carry a column §1 denies it"
        );
    }

    // Spot-checked against the table by hand, so a change to `uses` alone cannot pass both halves.
    let zoom = keys_of(&rec::blank(EffectKind::Zoom, 1.0, 2.0));
    for denied in ["wf", "snd", "gain", "lane", "text", "src"] {
        assert!(!zoom.iter().any(|k| k == denied), "a zoom has no {denied} row: {zoom:?}");
    }
    assert!(zoom.iter().any(|k| k == "dur"), "and it has the length row: {zoom:?}");

    let volume = keys_of(&rec::blank(EffectKind::Volume, 1.0, 2.0));
    for denied in ["cx", "cy", "hf", "wf", "cam", "stay", "rate", "snd"] {
        assert!(!volume.iter().any(|k| k == denied), "a volume has no {denied} row: {volume:?}");
    }
    assert!(volume.iter().any(|k| k == "dur"));

    // A label gets fades nothing at all (§1's `–` across the fade row) but keeps its name.
    let label = keys_of(&rec::blank(EffectKind::Label, 1.0, 2.0));
    for denied in ["trans", "tout", "ease", "cx", "cy", "hf", "wf", "stay", "rate", "snd", "gain", "src", "cam", "lane"] {
        assert!(!label.iter().any(|k| k == denied), "a label has no {denied} row: {label:?}");
    }
}

/// §06-effects#1-record — a new record is linear (`ease` stores "" and writes no key) and holds no box, which
/// is what puts a bare caption in the lower third and a bare drawing mid-frame.
#[test]
fn sec_06_effects_1_record_s13_a_new_record_is_linear_and_holds_no_box() {
    let caption = rec::blank(EffectKind::Text, 5.0, 2.0);
    assert!(rec::ease_is_linear(&caption), "linear is the default");
    assert!(rec::keeps_the_old_bytes(&caption), "and linear costs no `ease` key");
    assert!(!caption.has_box(), "nothing was placed, so there is no box");
    // ...which is exactly §1's last clause: a model-proposed caption carries no box and lands in the lower third.
    assert_eq!(caption.centre(), (0.5, 0.78));

    let drawing = rec::blank(EffectKind::Svg, 5.0, 2.0);
    assert!(!drawing.has_box());
    assert_eq!(drawing.centre(), (0.5, 0.5), "a drawing with no box is middle");

    // A bare zoom pulls back afterwards rather than staying reframed (§1's `stay`: false = pull back).
    let zoom = rec::blank(EffectKind::Zoom, 5.0, 2.0);
    assert!(!zoom.stay);
}

/// §06-effects#1-record — recording appends one record and says which kind went in.
#[test]
fn sec_06_effects_1_record_s14_record_into_appends_one_and_the_status_names_the_kind() {
    let mut cut = Cut::default();
    let made = rec::record_into(&mut cut, EffectKind::Volume, 12.0, rec::NEW_EFFECT_SECONDS);
    assert_eq!(cut.fx.len(), 1);
    assert_eq!(made.t, 12.0);
    assert_eq!(made.dur, rec::NEW_EFFECT_SECONDS);
    assert_eq!(made.effect_kind(), Some(EffectKind::Volume));
    // Gain 0 on a brand-new volume IS silence (§1: "linear gain, 0 = silence") — the hand turns it up next.
    assert!(rec::gain_is_silence(&made));
    // Empty lane = the whole bed (§1's `lane (new)`).
    assert!(rec::rides_whole_bed(&made));

    assert_eq!(
        rec::recorded_status(EffectKind::Volume),
        "Volume recorded \u{2014} \u{21b6} Undo takes it back"
    );

    // A second recording adds one and leaves the first exactly as it was.
    let again = rec::record_into(&mut cut, EffectKind::Text, 20.0, 3.0);
    assert_eq!(cut.fx.len(), 2);
    assert_eq!(again.kind, "text");
    assert_eq!(cut.fx[0], made, "the earlier record is untouched");
}

/// §06-effects#1-record — `cam (new)`: a zoom names the row it was framed on, and a kind with no `cam` row
/// has no such key at all.
#[test]
fn sec_06_effects_1_record_s15_a_recorded_zoom_names_the_row_it_was_framed_on() {
    let zoom = rec::blank(EffectKind::Zoom, 3.0, 2.0);
    // Row 0 is a real answer, not "unset": the key is skipped at 0 while the reading still says row 0.
    assert_eq!(rec::camera_row(&zoom), Some(0));
    assert!(!keys_of(&zoom).iter().any(|k| k == "cam"), "row 0 costs no key");

    let second_row = Fx { cam: 1, ..zoom.clone() };
    assert_eq!(rec::camera_row(&second_row), Some(1));
    assert!(keys_of(&second_row).iter().any(|k| k == "cam"), "a non-default row is written");

    // A caption is not framed on a camera: the field is absent, not zero.
    let caption = rec::blank(EffectKind::Text, 3.0, 2.0);
    assert!(!keys_of(&caption).iter().any(|k| k == "cam"));
    assert_eq!(rec::camera_row(&caption), None);
}

/// §06-effects#1-record — the kind spelled into the file reads back as the same kind, so a recorded effect is
/// never something the cut cannot hold; `"stop"` is Speed, not a seventh kind.
#[test]
fn sec_06_effects_1_record_s16_the_six_kinds_spell_their_way_back() {
    for kind in KINDS {
        let made = rec::blank(kind, 0.0, 1.0);
        // `EffectKind::parse` is private to the crate, so the round trip is read back through `effect_kind`,
        // which is parse on the stored string — the same door `uses` reads its column set through.
        assert_eq!(
            made.effect_kind(),
            Some(kind),
            "{} round-trips through the record's own kind reader",
            made.kind
        );
    }
    // Never written as "stop": §1 reads a stop as a rate of nought on a speed.
    let speed = rec::blank(EffectKind::Speed, 0.0, 1.0);
    assert_eq!(speed.kind, "speed");
    let stop_like = Fx { kind: "stop".into(), t: 0.0, dur: 1.0, ..Default::default() };
    assert_eq!(stop_like.effect_kind(), Some(EffectKind::Speed));
    assert_ne!(speed.kind, "stop");
}

/// §06-effects#1-record — legacy spellings beside a new record: `view` → zoom and `mute` → `snd: "mute"`,
/// read once on load, while the record written today comes back unchanged.
#[test]
fn sec_06_effects_1_record_s17_an_old_record_still_reads_after_the_new_one() {
    let fresh = rec::blank(EffectKind::Zoom, 30.0, 4.0);
    let fresh_json = serde_json::to_string(&fresh).unwrap();
    let text = format!(
        "{{\"segs\": [], \"fx\": [{{\"kind\": \"view\", \"t\": 4.0, \"dur\": 2.0, \"mute\": true}}, {fresh_json}]}}"
    );
    let (_root, tree) = with_cut("legacy", &text);
    let loaded = cut::load(&tree).expect("the cut loads");
    assert_eq!(loaded.fx.len(), 2);

    let old = &loaded.fx[0];
    assert_eq!(old.effect_kind(), Some(EffectKind::Zoom), "`view` became a zoom");
    assert_eq!(old.snd, "mute", "`mute` became the sound answer");
    assert!(!old.mute, "and the legacy flag is cleared on the way through");

    let now = &loaded.fx[1];
    assert_eq!(*now, fresh, "the record written today reads back byte-for-byte the same thing");
}
