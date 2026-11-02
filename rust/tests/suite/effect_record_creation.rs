// §06-effects#1-record — creating a record: a new effect carries exactly its kind's columns of §1's table.
//
// The reading side of §1 lives in `tests/suite/cut_six_effects_record.rs`; this file is the other direction — what a
// record looks like when it is MADE. The rule under test is that `fx_record::blank` fills nothing beyond time,
// length and kind, so every field the table marks `–` for a kind costs no key in `cut.json`, and the fields the
// table DOES give read back as their kind's default (a caption with no box → lower third, a drawing → middle).

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

/// A throwaway project holding one cut file, written by hand so §1's legacy spellings arrive the way an old
/// project writes them — through `cut::load`, where the migration happens.
fn with_cut(tag: &str, text: &str) -> (std::path::PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!(
        "naivepost-fxrec-create-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(dir.join("cut/cut.json"), text).unwrap();
    (root, Tree::new(&dir).unwrap())
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
    let stop_like = Fx {
        kind: "stop".into(),
        t: 0.0,
        dur: 1.0,
        ..Default::default()
    };
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
    assert_eq!(
        old.effect_kind(),
        Some(EffectKind::Zoom),
        "`view` became a zoom"
    );
    assert_eq!(old.snd, "mute", "`mute` became the sound answer");
    assert!(
        !old.mute,
        "and the legacy flag is cleared on the way through"
    );

    let now = &loaded.fx[1];
    assert_eq!(
        *now, fresh,
        "the record written today reads back byte-for-byte the same thing"
    );
}
