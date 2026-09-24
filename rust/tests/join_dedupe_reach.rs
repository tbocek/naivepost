// §10-parameters · P.eng.joinReachWords — how many kept words either side of a cut are compared for a
// doubled saying (3; prototype `joinReach`, gui/textedit.go). The pass is `textedit::dedupe_joins`: at
// each run of dropped words it takes the kept words nearest the seam on both sides, capped at the reach,
// and if the earlier copy's tail matches the later copy's head it drops the earlier one and notes it.
// The LATER copy stays, because what was said last is what the speaker meant to keep.
//
// What this file pins: the value and its single catalogue row; one doubled word across the cut; the
// longest match winning up to exactly the reach with one note only; the cap leaving a four-word repeat
// alone for the retake matcher; and the scan's edges plus the neighbours that share a number but not a
// rule.
//
// Word times are one second per word here: only the head word's start second reaches the note, so the
// shape of the clock does not matter to any assertion below.

use naivepost::hand_edit;
use naivepost::params;
use naivepost::tools::textedit;

/// Words with one second each, so word `i` starts at second `i`.
fn fixture(words: &[&str]) -> (Vec<String>, Vec<(f64, f64)>) {
    let w: Vec<String> = words.iter().map(|s| s.to_string()).collect();
    let t: Vec<(f64, f64)> = (0..words.len())
        .map(|i| (i as f64, i as f64 + 1.0))
        .collect();
    (w, t)
}

fn kept(v: &[bool]) -> Vec<bool> {
    v.to_vec()
}

/// Every list that carries rows, chained across all five pages so uniqueness is checked against the
/// whole catalogue rather than one page (`tests/hands_off.rs`'s `row()` makes the same choice).
fn all_rows() -> Vec<params::Param> {
    params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .collect()
}

/// S1: `P.eng.joinReachWords` = 3, held by `textedit::JOIN_REACH_WORDS`, catalogued once under the
/// family §10 files it in.
#[test]
fn p_eng_joinreachwords_s1_the_reach_is_3_and_catalogued_once() {
    // P.eng.joinReachWords — "dedupe across a cut".
    assert_eq!(textedit::JOIN_REACH_WORDS, 3);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.joinReachWords")
        .expect("the join reach must be catalogued for Prepare, whose dedupe reads it");
    assert_eq!(row.from, "tools::textedit::JOIN_REACH_WORDS", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "3");
    assert_eq!(row.spelled.parse::<usize>().unwrap(), 3);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.joinReachWords"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.joinReachWords").count(),
        1,
        "P.eng.joinReachWords catalogued more than once"
    );
}

/// S2: one doubled word across the cut — the earlier copy goes, the later stays, and the note is the
/// existing string stamped with the earlier copy's own second.
#[test]
fn p_eng_joinreachwords_s2_one_doubled_word_across_the_cut() {
    // "so we start again | start again now": the speaker restarted the sentence at "start", the model
    // dropped the middle stumble, and both copies of "start again" are kept. The near side's tail
    // ["start","again"] equals the far side's head ["start","again"], so the earlier copy goes and the
    // later one stays — the case the keep-mask cannot see, because the model kept both sayings.
    let (w, t) = fixture(&["so", "we", "start", "again", "gone", "start", "again", "now"]);
    let mut k = kept(&[true, true, true, true, false, true, true, true]);
    let notes = textedit::dedupe_joins(&w, &t, &mut k);

    assert_eq!(notes.len(), 1, "one join, one note: {notes:?}");
    // The two words of the earlier copy (indices 2 and 3) go with the run; the later copy at 5 and 6
    // is untouched — what was said last is what the speaker meant to keep.
    assert_eq!(&k[..4], &[true, true, false, false], "the earlier copy's tail is dropped");
    assert!(k[5] && k[6], "the later copy stays: what was said last is what the speaker meant to keep");
    // The wording is `dedupe_note`'s, not a copy of it, and the second is the EARLIER copy's head
    // (word 2 → second 2.0), which is what the prototype stamps.
    assert_eq!(notes[0], textedit::dedupe_note(t[2].0, "start again"));
    assert!(notes[0].starts_with("2: "), "stamped at the earlier copy's start: {}", notes[0]);
    // Untouched elsewhere.
    assert_eq!(&k, &[true, true, false, false, false, true, true, true], "the whole earlier copy goes");
}

/// S3: three words doubled across the cut — the longest-first loop matches at n = the full reach, all
/// three earlier words go, and still only ONE note comes out.
#[test]
fn p_eng_joinreachwords_s3_a_whole_reach_of_words_can_go_as_one_note() {
    // x / a / b / c | dropped run | a / b / c / y
    let (w, t) = fixture(&["x", "a", "b", "c", "gone", "a", "b", "c", "y"]);
    let mut k = kept(&[true, true, true, true, false, true, true, true, true]);
    let notes = textedit::dedupe_joins(&w, &t, &mut k);

    assert_eq!(notes.len(), 1, "the break after a match means one note per join: {notes:?}");
    assert_eq!(&k, &[true, false, false, false, false, true, true, true, true], "all three earlier words go, x untouched");
    assert!(notes[0].contains("\"a b c\""), "the note names all three words: {}", notes[0]);
    assert!(notes[0].starts_with("1: "), "stamped at the first dropped word's second: {}", notes[0]);
}

/// S4: the cap. Only [`JOIN_REACH_WORDS`] kept words are offered from either side, so a longer repeat
/// either lines up inside the window or is left for the retake matcher (F1.9) entirely.
#[test]
fn p_eng_joinreachwords_s4_only_the_reach_is_compared() {
    // (a) the earlier copy ends "... x A B C D", the later begins "B C D E": `before` is capped at the
    // last three kept words (A? no — B C D), so the match happens inside the window and exactly those
    // three drop while everything further back keeps its place.
    let (w, t) = fixture(&["x", "A", "B", "C", "D", "gone", "B", "C", "D", "E"]);
    let mut k = kept(&[true, true, true, true, true, false, true, true, true, true]);
    let notes = textedit::dedupe_joins(&w, &t, &mut k);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(&k, &[true, true, false, false, false, false, true, true, true, true], "three dropped, x and A still kept");
    assert!(notes[0].contains("\"B C D\""), "{}", notes[0]);

    // (b) an exact FOUR-word repeat: `before` = [B,C,D] (capped) against `after` = [A,B,C]. No n lines
    // up, so nothing drops and no note is written. A four-word repeat is a retake's shape, not a join's
    // own stumble, and the cap is precisely what leaves it to the matcher that knows how to find one.
    let (w2, t2) = fixture(&["A", "B", "C", "D", "gone", "A", "B", "C", "D"]);
    let mut k2 = kept(&[true, true, true, true, false, true, true, true, true]);
    let notes2 = textedit::dedupe_joins(&w2, &t2, &mut k2);
    assert!(notes2.is_empty(), "a four-word repeat is beyond the reach: {notes2:?}");
    assert_eq!(&k2, &[true, true, true, true, false, true, true, true, true], "nothing changed");
}

/// S5: the scan's edges, the word-equality it inherits, and the neighbours that share its number.
#[test]
fn p_eng_joinreachwords_s5_scan_edges_inherited_word_equality_and_neighbours() {
    // (i) eight kept words before the run: the reach still offers only three, so the note can never
    // name more than the reach even when the near side is long.
    let (w, t) = fixture(&["one", "two", "three", "four", "five", "six", "seven", "go", "gone", "go", "on"]);
    let mut k = kept(&[true, true, true, true, true, true, true, true, false, true, true]);
    let notes = textedit::dedupe_joins(&w, &t, &mut k);
    assert_eq!(notes.len(), 1, "{notes:?}");
    let named = notes[0].split('"').nth(1).unwrap_or("");
    assert!(
        named.split_whitespace().count() <= textedit::JOIN_REACH_WORDS,
        "a long near side still yields at most the reach: {named:?}"
    );
    assert_eq!(named, "go", "only the doubled word itself went: {named:?}");

    // (ii) a drop run with nothing kept after it ends the scan: no far side, no join.
    let (w2, t2) = fixture(&["kept", "gone", "also gone"]);
    let mut k2 = kept(&[true, false, false]);
    assert!(textedit::dedupe_joins(&w2, &t2, &mut k2).is_empty(), "no far side: nothing to compare");
    assert_eq!(&k2, &[true, false, false], "and nothing dropped");

    // (iii) the dedupe compares words with the retake matcher's equality (`same_word`), not plain ==:
    // a respelled pair of three-plus letters still counts as the same word...
    let (w3, t3) = fixture(&["we", "start", "gone", "starts", "here"]);
    let mut k3 = kept(&[true, true, false, true, true]);
    let notes3 = textedit::dedupe_joins(&w3, &t3, &mut k3);
    assert_eq!(notes3.len(), 1, "\"start\"/\"starts\" is one word to the matcher: {notes3:?}");
    assert!(!k3[1], "the earlier copy goes");
    // ...while two-letter words are held to exactness, so "to"/"too" (one edit apart) is NOT the same
    // word and the join is left alone.
    let (w4, t4) = fixture(&["next", "to", "gone", "too", "far"]);
    let mut k4 = kept(&[true, true, false, true, true]);
    assert!(textedit::dedupe_joins(&w4, &t4, &mut k4).is_empty(), "\"to\" and \"too\" are different words");
    assert_eq!(&k4, &[true, true, false, true, true], "nothing dropped");

    // (iv) neighbours with different numbers and different ids.
    // P.eng.keepReachWords (200): how far back the hand-edit match looks for the answer's next word.
    assert_eq!(hand_edit::KEEP_REACH, 200);
    // P.machine.seamReachWords (140): how many words each side of a join is SHOWN to the model.
    assert_eq!(textedit::SEAM_REACH_WORDS, 140);
    assert_ne!(textedit::JOIN_REACH_WORDS, hand_edit::KEEP_REACH, "a dedupe window is not the match's look-back");
    assert_ne!(textedit::JOIN_REACH_WORDS, textedit::SEAM_REACH_WORDS, "what is compared for a doubling is not what is shown for the answer");

    // P.eng.repeatSkip also equals 3, but it is the fuzzy repeat matcher's step-over (how many words of
    // the later take may be skipped while hunting the next tail word) — a different rule in a different
    // pass, so there is nothing to assert between it and this reach beyond noting the coincidence.
    assert_eq!(
        params::find("P.eng.repeatSkip").map(|r| r.from),
        Some("prepare_decisions::TAIL_SKIP_MAX"),
    );
}
