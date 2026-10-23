//! §05-cut#7-rules bullet 6 — `Suggest replaces the footage half and the effects, keeps inserts, and sets
//! the base`: the WIRE through the landing door.
//!
//! [`naivepost::suggest::apply`] (the walk-back) and [`naivepost::cut::History::suggested`] (the base) already
//! have their own suites; what this file proves is that the page's landing seam actually reaches both halves of
//! §7's sentence. The insert-preserving half had no program caller at all before this round — `suggested_cut`
//! rebuilt the list by hand while [`naivepost::cut::keep_inserts`] sat uncalled — so a re-suggest could have
//! dropped a title card and every test would still have passed.
//!
//! Pure rule + pure door: nothing here builds a window, because `suggested_cut` reads the cut it lands onto from
//! its argument and the recorder slot only matters to the write, which `sec_05_cut_7_rules_s7_*` covers.

use crate::common;
use common::fx;
use naivepost::cut::{self, Cut, Seg};
use naivepost::layout;
use naivepost::suggest::Applied;
use naivepost::ui::cut_suggest_land;

/// A footage scene on camera row 0.
fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        cam: 0,
        ..Default::default()
    }
}

/// A hand-placed card: `ins` names the file it plays, which is what makes it not footage (§7: `inserts are
/// files`).
fn card(s: f64, e: f64, file: &str) -> Seg {
    Seg {
        s,
        e,
        ins: file.into(),
        ..Default::default()
    }
}

/// The cut on screen before the suggestion: two scenes of footage, one card in the middle of them, and a zoom
/// placed against those seconds.
fn cut_before() -> Cut {
    Cut {
        segs: vec![
            seg(0.0, 10.0),
            card(10.0, 12.0, "title.png"),
            seg(12.0, 30.0),
        ],
        fx: vec![fx("zoom", 2.0, 3.0)],
        ..Default::default()
    }
}

#[test]
fn sec_05_cut_7_rules_s6_a_resuggest_keeps_the_cards_and_moves_the_base() {
    let before = cut_before();
    // The model's answer for a talk it re-cut completely: none of these seconds overlap the old footage, and
    // its effects are its own. No kept inserts — the cards must survive from `before`, not from the reply.
    let answer = Applied {
        segs: vec![(40.0, 55.0), (60.0, 71.5)],
        kept_inserts: Vec::new(),
        effects: vec![fx("speed", 41.0, 2.0)],
        log: Vec::new(),
    };

    let landed = cut_suggest_land::suggested_cut(&before, &answer);

    // (a) the card is still there, unchanged: nobody filmed it, so nothing in the answer can be its second
    //     version.
    let cards: Vec<&Seg> = landed.segs.iter().filter(|s| !s.ins.is_empty()).collect();
    assert_eq!(
        cards,
        vec![&card(10.0, 12.0, "title.png")],
        "§7's `keeps inserts`: the hand card survives a re-suggest exactly as it was — {:?}",
        landed.segs
    );

    // (b) all of the model's footage arrived.
    let footage: Vec<(f64, f64)> = landed
        .segs
        .iter()
        .filter(|s| s.ins.is_empty())
        .map(|s| (s.s, s.e))
        .collect();
    assert_eq!(
        footage,
        vec![(40.0, 55.0), (60.0, 71.5)],
        "§7's `replaces the footage half`: every suggested scene is present, in order — {footage:?}"
    );

    // (c) none of the old footage remains.
    for old in [(0.0, 10.0), (12.0, 30.0)] {
        assert!(
            !footage.contains(&old),
            "the scene the model did not choose is gone from the footage half: {old:?} still in {footage:?}"
        );
    }

    // Time order survives the merge: `keep_inserts` sorts, so the card sits between the scenes it belongs to
    // rather than being appended after them.
    let starts: Vec<f64> = landed.segs.iter().map(|s| s.s).collect();
    let mut sorted = starts.clone();
    sorted.sort_by(f64::total_cmp);
    assert_eq!(
        starts, sorted,
        "the landed list stays in time order: {starts:?}"
    );

    // (d) the effects go with the footage half: the answer's list, not the old zoom.
    assert_eq!(
        landed.fx, answer.effects,
        "§7's `and the effects`: the suggestion's decorations replace the ones placed against scenes that \
         are no longer kept"
    );

    // (e) `sets the base`: after the landing moves the base, Revert returns the SUGGESTION, not whatever was
    //     on disk an hour ago — and a hand edit made after the suggestion is what Revert drops.
    let mut history = cut::History::open(&before);
    history.note_change(&before);
    history.suggested(&landed);
    let back = history.revert();
    assert_eq!(
        back.segs, landed.segs,
        "Revert lands on the suggestion's segments"
    );
    assert_eq!(back.fx, landed.fx, "…with the suggestion's effects");
    assert!(
        history.base_is_the_screen(&landed),
        "the base IS the last suggestion (§2), so the page says `nothing to revert` right after one"
    );
    assert!(
        !history.base_is_the_screen(&before),
        "and the pre-suggestion cut is no longer the base: {:?}",
        before.segs
    );
}

/// The `Applied` a no-model words cut hands the same door: spans only, no kept inserts and no effects.
fn plain_answer(spans: &[(f64, f64)]) -> Applied {
    Applied {
        segs: spans.to_vec(),
        kept_inserts: Vec::new(),
        effects: Vec::new(),
        log: Vec::new(),
    }
}

/// §7's translate, in one test: the strip is a WINDOW drawn under a translate, so the painter's offset and the
/// press's offset must cancel exactly. If either side stopped using the scroll, this is what catches it.
#[test]
fn sec_05_cut_7_rules_s8_the_strip_is_a_window_under_a_translate() {
    use naivepost::ui::cut_strip_view;

    let pps = cut_strip_view::pps();
    assert!(
        pps > 0.0,
        "the strip draws at some zoom, or nothing here means anything: {pps}"
    );

    // At zero scroll the two conversions are the plain divide / multiply — the old behaviour, still correct there.
    cut_strip_view::set_scroll_for_test(0.0);
    assert_eq!(
        cut_strip_view::scroll_px(),
        0.0,
        "scroll starts at the head of the tape"
    );
    assert!(
        (cut_strip_view::seconds_at(120.0) - 120.0 / pps).abs() < 1e-9,
        "unscrolled, a pixel is just pixels over pps"
    );

    // Scrolled by 100 px: the second under a pixel is further into the session by exactly `100 / pps`.
    let scrolled = cut_strip_view::set_scroll_for_test(100.0);
    assert!(
        (scrolled - 100.0).abs() < 1e-9,
        "the scroll took: {scrolled}"
    );
    let delta = cut_strip_view::seconds_at(120.0) - 120.0 / pps;
    assert!(
        (delta - 100.0 / pps).abs() < 1e-9,
        "a 100 px scroll moves the answer by 100/pps seconds: got {delta}, want {}",
        100.0 / pps
    );

    // The paint offset is the negative of the press offset: they cancel, which is the wire that keeps a click on
    // the clip it drew. Take the pixel a press answered about, put its own tape-x back through the painter's
    // function, and the widget pixel comes home.
    for px in [0.0, 37.0, 120.0, 900.0] {
        let tape = cut_strip_view::seconds_at(px) * pps;
        assert!(
            (cut_strip_view::tape_x_for_test(tape) - px).abs() < 1e-6,
            "press → tape → paint round-trips the pixel: {px} -> {tape} -> {}",
            cut_strip_view::tape_x_for_test(tape)
        );
    }

    // What a scrolled window actually paints: the ruler's first tick, which sat at widget-x 0 unscrolled, is
    // now `scroll` px off the left edge — that is the translate doing its job. (The strip clips with cairo's own
    // `translate`, so there is no per-box visibility helper to poke.)
    let scroll = cut_strip_view::set_scroll_for_test(100.0);
    assert!(
        (cut_strip_view::tape_x_for_test(0.0) + scroll).abs() < 1e-9,
        "the tape's x=0 sits {} px left of the widget's left edge",
        scroll
    );

    // One scroll drives both surfaces once the strip scrolls with the view: while the placeholder answers zero,
    // a non-zero offset must still round-trip, and the number it reports is the one the painter used.
    assert!(
        (cut_strip_view::scroll_px() - 100.0).abs() < 1e-9,
        "the painter and the press read the same scroll: {}",
        cut_strip_view::scroll_px()
    );
    cut_strip_view::set_scroll_for_test(0.0);
}

/// §7's last clause: `the page is rebuilt only when Prepare's output changed`. The guard lives in
/// [`naivepost::ui::cut_rebuild`]; this proves the identity it asks about behaves — including the two states a
/// naive "does the file exist" check would collapse into one.
#[test]
fn sec_05_cut_7_rules_s8_the_page_rebuilds_only_when_prepare_output_changed() {
    use naivepost::ui::cut_rebuild;

    let tree = session_tree();
    // No output at all yet: the first look IS a change (there is nothing drawn), and the next one is not, so a
    // project opened before its first Prepare does not rebuild on every beat.
    cut_rebuild::forget();
    assert!(
        cut_rebuild::fingerprint(&tree).is_none(),
        "no session.tsv means no fingerprint: {:?}",
        tree.session_tsv()
    );
    assert!(
        cut_rebuild::output_changed(&tree),
        "absent output counts as changed once"
    );
    assert!(
        !cut_rebuild::output_changed(&tree),
        "and unchanged after that — nothing has appeared"
    );

    // Prepare writes its merged timeline: size + mtime both move off "absent", so the page rebuilds once.
    write_session(&tree, b"t\tfile\n0.0\tcam0.mp4\n");
    assert!(
        cut_rebuild::output_changed(&tree),
        "output appeared, so the rows must be laid out again"
    );
    assert!(
        !cut_rebuild::output_changed(&tree),
        "the same bytes again are not a change: re-probing every footage file for nothing"
    );

    // A real Prepare run rewrites the file. Size alone is enough to notice; a same-size rewrite needs the mtime,
    // so this appends (size moves) rather than relying on a clock tick a test may not have.
    write_session(&tree, b"t\tfile\n0.0\tcam0.mp4\n12.5\tmic.wav\n");
    assert!(
        cut_rebuild::output_changed(&tree),
        "a longer session.tsv is a new output: {:?}",
        cut_rebuild::fingerprint(&tree)
    );
    assert!(!cut_rebuild::output_changed(&tree), "and it settles again");

    // Output vanishing (a rescan that dropped the transcript) is also a change — the page has something to undo.
    std::fs::remove_file(tree.session_tsv()).expect("session.tsv removed");
    assert!(
        cut_rebuild::output_changed(&tree),
        "output disappearing is not 'unchanged'"
    );
    assert!(
        !cut_rebuild::output_changed(&tree),
        "back to steady absence"
    );

    // And an open of another folder forgets everything, so a byte-identical file elsewhere still rebuilds.
    write_session(&tree, b"t\tfile\n0.0\tcam0.mp4\n");
    assert!(cut_rebuild::output_changed(&tree), "it came back");
    assert!(!cut_rebuild::output_changed(&tree), "steady");
    cut_rebuild::forget();
    assert!(
        cut_rebuild::output_changed(&tree),
        "after a folder is taken over, the same bytes count as new output"
    );
}

/// A temp `.naivepost` folder to ask files inside about.
fn session_tree() -> layout::Tree {
    let root = std::env::temp_dir().join(format!("np-rebuild-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root");
    layout::Tree::new(naivepost::startup::session_dir(&root)).expect("a .naivepost folder")
}

/// Put bytes at Prepare's own output path, creating `transcript/` the way the writer does.
fn write_session(tree: &layout::Tree, bytes: &[u8]) {
    let path = tree.session_tsv();
    std::fs::create_dir_all(path.parent().expect("transcript/")).expect("transcript dir");
    std::fs::write(&path, bytes).expect("session.tsv written");
}

#[test]
fn sec_05_cut_7_rules_s6_b_a_words_cut_carries_the_cards_too() {
    // §S2a's no-model cut goes through the same door with an empty `kept_inserts` and no effects: a words cut
    // proposes scenes, so it has no claim on a card either — and, per §7, it replaces the effects too rather
    // than leaving decorations pointed at scenes it removed.
    let before = cut_before();
    let landed = cut_suggest_land::suggested_cut(&before, &plain_answer(&[(1.0, 9.0)]));
    assert!(
        landed.segs.iter().any(|s| s.ins == "title.png"),
        "the card survives a words cut as it survives a model one: {:?}",
        landed.segs
    );
    assert_eq!(
        landed
            .segs
            .iter()
            .filter(|s| s.ins.is_empty())
            .map(|s| (s.s, s.e))
            .collect::<Vec<_>>(),
        vec![(1.0, 9.0)],
        "the words cut's spans are the whole footage half"
    );
    assert!(
        landed.fx.is_empty(),
        "effects go with the footage half even when the proposal names none: {:?}",
        landed.fx
    );
}
