//! F1.5 S4 — numbers are said as words to the aligner, and folded back into the written token.
//!
//! Measured on 08-dex 18-41-59 against qwen3-aligner: asked with "0,81" the aligner gave the number 80 ms where it is
//! said for 1.5 s, and "45" took the "USDC" after it; asked with "null Komma acht eins" the same window timed both
//! to the sound. These checks hold the request text and the fold, never a server.

use std::fs;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::common;
use naivepost::align;
use naivepost::layout::Tree;
use naivepost::requests::{self, Chunk, Word};
use naivepost::spoken_numbers::{merge, spell};

fn spoken(text: &str, language: &str) -> String {
    spell(text, language).0
}

#[test]
fn f1_5_s4_german_numbers_are_said_as_words() {
    assert_eq!(
        spoken("zahlt rund 45 USDC und nimmt 0,184 Ethers.", "de"),
        "zahlt rund fünfundvierzig USDC und nimmt null Komma eins acht vier Ethers."
    );
    assert_eq!(spoken("1", "de"), "eins");
    assert_eq!(spoken("21", "de"), "einundzwanzig");
    assert_eq!(spoken("101", "de"), "hunderteins");
    assert_eq!(spoken("100", "de"), "hundert");
    assert_eq!(spoken("2.100", "de"), "zweitausendhundert");
    assert_eq!(spoken("245", "de"), "zweihundertfünfundvierzig");
    assert_eq!(spoken("1000", "de"), "tausend");
    assert_eq!(spoken("1.300", "de"), "tausenddreihundert");
    // years are said as years
    assert_eq!(spoken("1997.", "de"), "neunzehnhundertsiebenundneunzig.");
    assert_eq!(spoken("1901", "de"), "neunzehnhunderteins");
    // a version's dots are "Punkt", not a thousands group
    assert_eq!(spoken("0.9.0", "de"), "null Punkt neun Punkt null");
    assert_eq!(spoken("0.8-Reihe", "de"), "null Punkt acht Reihe");
    assert_eq!(spoken("2026", "de"), "zweitausendsechsundzwanzig");
    assert_eq!(spoken("1.000.000", "de"), "eine Million");
    assert_eq!(
        spoken("3.500.000", "de"),
        "drei Millionen fünfhunderttausend"
    );
    assert_eq!(spoken("10%", "de"), "zehn Prozent");
    assert_eq!(spoken("(5€)", "de"), "(fünf Euro)");
    assert_eq!(spoken("ERC20", "de"), "ERC zwanzig");
    assert_eq!(spoken("007", "de"), "null null sieben");
}

#[test]
fn f1_5_s4_english_numbers_are_said_as_words() {
    assert_eq!(spoken("about 0.5 ETH", "en"), "about zero point five ETH");
    assert_eq!(spoken("1,250", "en"), "one thousand two hundred fifty");
    assert_eq!(spoken("$5", "en"), "five dollars");
    assert_eq!(spoken("x402", "eng"), "x four hundred two");
    assert_eq!(spoken("2023,", "en"), "twenty twenty three,");
    assert_eq!(spoken("2005", "en"), "two thousand five");
    assert_eq!(spoken("1905", "en"), "nineteen oh five");
    assert_eq!(spoken("1900", "en"), "nineteen hundred");
    assert_eq!(spoken("14th", "en"), "fourteenth");
    assert_eq!(spoken("1st", "en"), "first");
    assert_eq!(spoken("2nd", "en"), "second");
    assert_eq!(spoken("23rd", "en"), "twenty third");
    assert_eq!(spoken("20th", "en"), "twentieth");
    assert_eq!(spoken("90s:", "en"), "nineties:");
    assert_eq!(spoken("1990s", "en"), "nineteen nineties");
}

#[test]
fn f1_5_s4_text_without_digits_or_in_another_language_is_asked_as_written() {
    let text = "Guten Tag, ich begrüße Sie.";
    assert_eq!(spoken(text, "de"), text);
    // French is not spelled: the aligner gets what the ASR wrote.
    assert_eq!(spoken("il y a 45 ans", "fr"), "il y a 45 ans");
}

fn w(word: &str, from: u64, to: u64) -> Word {
    Word {
        word: word.into(),
        start_sample: from,
        end_sample: to,
    }
}

#[test]
fn f1_5_s4_the_spoken_words_fold_back_into_the_written_token() {
    let (_, tokens) = spell("bei 245 USDC und 0,81 Ethers", "de");
    let answer = vec![
        w("bei", 0, 10),
        w("zweihundertfünfundvierzig", 10, 40),
        w("usdc", 40, 50),
        w("und", 50, 55),
        w("null", 55, 60),
        w("komma", 60, 66),
        w("acht", 66, 72),
        w("eins", 72, 90),
        w("ethers", 95, 110),
    ];
    let folded = merge(answer, &tokens);
    // One word per written token, named as the aligner names the written token ("0,81" → "081").
    assert_eq!(
        folded,
        vec![
            w("bei", 0, 10),
            w("245", 10, 40),
            w("usdc", 40, 50),
            w("und", 50, 55),
            w("081", 55, 90),
            w("ethers", 95, 110),
        ]
    );
}

#[test]
fn f1_5_s4_an_answer_that_does_not_follow_the_spoken_words_loses_nothing() {
    let (_, tokens) = spell("rund 45 USDC", "de");
    // The aligner split the number somewhere unexpected and added a word nobody asked for.
    let answer = vec![
        w("rund", 0, 10),
        w("fünfund", 10, 20),
        w("vierzig", 20, 30),
        w("usdc", 30, 40),
    ];
    let folded = merge(answer.clone(), &tokens);
    assert_eq!(folded.len(), 4, "{folded:?}");
    assert_eq!(folded[0], answer[0]);
    assert_eq!(folded[3], answer[3]);
}

/// Through the stage: the aligner is asked with the spelled text and `words.aligned.json` holds the written tokens.
#[test]
fn f1_5_s4_the_stage_asks_with_words_and_writes_the_written_tokens() {
    let root = common::temp_dir_created("spoken-numbers");
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a project");
    let source = "mic.wav";
    fs::create_dir_all(tree.input_dir(source)).unwrap();

    let transcript = "und 0,81 Ethers";
    let chunks = vec![Chunk {
        s: 0.0,
        e: 30.0,
        text: transcript.into(),
    }];
    let asr_words = vec![w("und", 0, 1), w("081", 1, 2), w("ethers", 2, 3)];
    let asked: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = asked.clone();
    let answer: Value = json!({"words": [
        {"word": "und", "start": 1.0, "end": 1.3},
        {"word": "null", "start": 1.3, "end": 1.5},
        {"word": "Komma", "start": 1.5, "end": 1.8},
        {"word": "acht", "start": 1.8, "end": 2.2},
        {"word": "eins", "start": 2.2, "end": 2.8},
        {"word": "Ethers", "start": 3.2, "end": 3.9},
    ]});
    let outcome = align::run(
        &tree,
        source,
        30.0,
        "de",
        &[],
        &chunks,
        &asr_words,
        transcript,
        &["qwen3-aligner".to_string()],
        |_model, _clip, text| {
            seen.lock().unwrap().push(text.to_string());
            Ok(answer.clone())
        },
    )
    .expect("aligned");
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        ["und null Komma acht eins Ethers"]
    );
    let written = requests::read_aligned(&tree, source)
        .unwrap()
        .expect("written")
        .words;
    let names: Vec<&str> = written.iter().map(|w| w.word.as_str()).collect();
    assert_eq!(names, ["und", "081", "ethers"]);
    // The number runs from its first spoken word's start to its last one's end.
    assert_eq!(written[1].start_sample, (1.3 * 16000.0_f64).round() as u64);
    assert_eq!(written[1].end_sample, (2.8 * 16000.0_f64).round() as u64);
    assert_eq!(outcome.words.len(), 3);
}

/// The folded word is named as the aligner names the written token: apostrophes kept, other punctuation dropped.
#[test]
fn f1_5_s4_the_folded_word_keeps_the_apostrophe_the_aligner_keeps() {
    let (text, tokens) = spell("im März '26 wurden", "de");
    assert_eq!(text, "im März 'sechsundzwanzig wurden");
    let answer = vec![
        w("im", 0, 1),
        w("märz", 1, 2),
        w("'sechsundzwanzig", 2, 9),
        w("wurden", 9, 12),
    ];
    let names: Vec<String> = merge(answer, &tokens).into_iter().map(|w| w.word).collect();
    assert_eq!(names, ["im", "märz", "'26", "wurden"]);
}
