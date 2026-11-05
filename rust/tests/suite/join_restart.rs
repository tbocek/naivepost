//! F1.10 — a restart across a cut: the take after the cut opens with words said already shortly before it, so the
//! first attempt goes from where those words were said first. 09-dao (an untouched autocut) kept the start of an
//! abandoned attempt and then played the new take saying it all again.

use naivepost::tools::textedit::{restart_joins, RESTART_MIN_WORDS, RESTART_REACH_WORDS};

/// Words, their times (one second each) and a kept mask; `gone` marks the dropped run (the cut).
fn scene(text: &str, gone: std::ops::Range<usize>) -> (Vec<String>, Vec<(f64, f64)>, Vec<bool>) {
    let words: Vec<String> = text.split_whitespace().map(String::from).collect();
    let times = (0..words.len()).map(|i| (i as f64, i as f64 + 0.5)).collect();
    let kept = (0..words.len()).map(|i| !gone.contains(&i)).collect();
    (words, times, kept)
}

fn kept_text(words: &[String], kept: &[bool]) -> String {
    words.iter().zip(kept).filter(|(_, k)| **k).map(|(w, _)| w.as_str()).collect::<Vec<_>>().join(" ")
}

#[test]
fn f1_10_a_restart_across_a_cut_drops_the_first_attempt() {
    // 09-dao 20-37-43 → 20-39-45: the join model dropped "den Token ... bekommen." and kept the rest of the attempt.
    let first = "Und hier die entsprechende Studie dazu. Und das dritte Problem: Stimmen kann man kaufen. Das beste \
                 Beispiel sind die Curve Wars. Bei Curve entscheiden die Besitzer von";
    let dropped = "den Token, welche Pools als Belohnung bekommen.";
    let again = "Und das dritte Problem: Stimmen kann man kaufen. Das beste Beispiel sind die Curve Wars. Curve ist eine DEX";
    let n_first = first.split_whitespace().count();
    let n_dropped = dropped.split_whitespace().count();
    let (words, times, mut kept) = scene(&format!("{first} {dropped} {again}"), n_first..n_first + n_dropped);
    let notes = restart_joins(&words, &times, &mut kept);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("started over after the cut"), "{notes:?}");
    assert_eq!(
        kept_text(&words, &kept),
        format!("Und hier die entsprechende Studie dazu. {again}"),
        "the first attempt goes from its first word; the sentence before it and the new take stay"
    );
}

#[test]
fn f1_10_a_short_shared_opening_is_not_a_restart() {
    // "Und das ist" opens many sentences: four words in common are not enough.
    assert!(RESTART_MIN_WORDS >= 5);
    let (words, times, mut kept) = scene("Und das ist der Preis heute weg gut Und das ist der Wert", 6..8);
    let before = kept.clone();
    assert!(restart_joins(&words, &times, &mut kept).is_empty());
    assert_eq!(kept, before);
}

#[test]
fn f1_10_a_repeat_further_back_than_the_reach_is_left_alone() {
    let opening = "Stimmen kann man kaufen sagt man";
    let filler: Vec<String> = (0..RESTART_REACH_WORDS + 5).map(|i| format!("w{i}")).collect();
    let text = format!("{opening} {} weg {opening}", filler.join(" "));
    let n = text.split_whitespace().count();
    let cut_at = n - opening.split_whitespace().count() - 1;
    let (words, times, mut kept) = scene(&text, cut_at..cut_at + 1);
    assert!(restart_joins(&words, &times, &mut kept).is_empty());
}
