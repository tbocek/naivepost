//! F1.13 The session's word list (spec/04-prepare.md) — the one list retakes, joins, `final.txt` and
//! subtitles read. Tested on [`naivepost::word_list`]: gluing, the stray rule, both dressings, and the
//! join pass matching its answer back against these tokens. Times never change; only the written form.

use naivepost::params;
use naivepost::tools::mm_ss;
use naivepost::word_list::{self as words, Token, Word};

const SRC: &str = "lecture.mkv";

fn token(word: &str, start: f64, end: f64) -> Token {
    Token { word: word.into(), start, end }
}

/// A word already glued, for the rules that take the list rather than the tokens.
fn word(source: &str, match_word: &str, start: f64, end: f64) -> Word {
    Word {
        source: source.into(),
        match_word: match_word.into(),
        written: match_word.into(),
        start,
        end,
        stray: false,
    }
}

fn glued_from(pairs: &[(&str, f64, f64)]) -> Vec<Word> {
    let tokens: Vec<Token> = pairs.iter().map(|(w, s, e)| token(w, *s, *e)).collect();
    words::glued(SRC, &tokens)
}

/// A loudness lookup over a hand-built envelope: `(start, end, peak)` buckets, `None` outside them.
fn envelope(buckets: &[(f64, f64, i32)]) -> impl Fn(&str, f64, f64) -> Option<i32> {
    let buckets: Vec<(f64, f64, i32)> = buckets.to_vec();
    move |_source: &str, start: f64, end: f64| {
        buckets
            .iter()
            .filter(|(from, to, _)| start < *to && end > *from)
            .map(|(_, _, peak)| *peak)
            .max()
    }
}

// --- S1: tokens become words -----------------------------------------------------------------------

#[test]
fn f1_13_s1_tokens_are_glued_into_words_on_the_session_clock() {
    // F1.13: a leading space anywhere means piece tokens, so the spaces say where a word starts.
    let list = glued_from(&[(" wa", 0.0, 0.2), ("lle", 0.2, 0.4), (" t", 0.5, 0.6), ("hi", 0.7, 0.8)]);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].written, "walle");
    // The leading space starts the next word and is not part of it; the pieces after it join on.
    assert_eq!(list[1].written, "thi");
    assert_eq!(list[1].match_word, "thi");

    // A token of nothing but space is where a word begins, even mid-piece — drop it without remembering
    // and the next piece welds onto the word before.
    let spaced = glued_from(&[("hal", 0.0, 0.2), (" ", 0.21, 0.22), ("lo", 0.3, 0.4)]);
    assert_eq!(words::written(&spaced), vec!["hal", "lo"]);
    // A glued word spans its first piece's start to its last piece's end.
    assert_eq!((list[0].start, list[0].end), (0.0, 0.4));
    // No leading space anywhere: whole words, taken as they stand.
    let whole = glued_from(&[("Welcome", 0.0, 0.3), ("back", 0.35, 0.6)]);
    assert_eq!(whole.len(), 2);
    assert_eq!(words::written(&whole), vec!["Welcome", "back"]);

    // match_word is the bare lowercase form every pass matches against; written keeps the token's text.
    assert_eq!(whole[0].match_word, "welcome");
    assert_eq!(whole[0].written, "Welcome");

    // Punctuation is text and not sound: it rides the word before WITHOUT moving that word's end — a
    // comma's stamp lands in the silence after the sentence.
    let punct = glued_from(&[("hello", 0.0, 0.4), (",", 0.45, 0.46), ("there", 1.0, 1.3)]);
    assert_eq!(punct.len(), 2);
    assert_eq!(punct[0].written, "hello,");
    assert_eq!(punct[0].end, 0.4, "the comma must not stretch the word in front of it");
    assert_eq!(punct[1].match_word, "there");

    // A full stop on a glued word too, and a piece-word's match form follows the same rule.
    let pieces = glued_from(&[
        ("hel", 0.0, 0.2),
        ("lo", 0.2, 0.3),
        (".", 0.34, 0.35),
        (" there", 1.0, 1.2),
    ]);
    assert_eq!(pieces.len(), 2);
    assert_eq!(pieces[0].written, "hello.");
    assert_eq!(pieces[0].match_word, "hello");
    assert_eq!(pieces[0].end, 0.3);

    // Punctuation with no word before it belongs to no sound and is dropped.
    let leading = glued_from(&[("-", 0.0, 0.1), ("start", 0.2, 0.4)]);
    assert_eq!(words::written(&leading), vec!["start"]);

    // bare() is the whole rule: punctuation-only is empty, case and marks go.
    assert_eq!(words::bare("RSA-260"), "rsa260");
    assert_eq!(words::bare("  …  "), "");
}

// --- S2: a word on almost no sound ------------------------------------------------------------------

#[test]
fn f1_13_s2_a_word_on_almost_no_sound_is_timed_at_the_end_of_the_word_before() {
    // P.machine.strayWordRatio = 10, P.machine.strayWordGapSeconds = 1 (F1.13).
    assert_eq!(words::STRAY_RATIO, 10);
    assert_eq!(words::STRAY_GAP_SECONDS, 1.0);

    // The ETH lecture: "Geld." matched to a breath 3.4 s after "viel", its loudest moment a tenth of what
    // that recording's words reach.
    let mut list = glued_from(&[
        ("Und", 10.0, 10.2),
        ("viel", 10.3, 10.6),
        ("Geld.", 14.0, 14.1),
        ("hier", 20.0, 20.3),
    ]);
    // The median is over every word of this recording: peaks 120, 120, 10 and 130 put the middle at 120,
    // and "Geld." peaks at a tenth of that.
    let loud = envelope(&[(9.0, 13.9, 120), (13.9, 14.2, 10), (19.5, 21.0, 130)]);
    let logs = words::retimed(&mut list, loud);

    let geld = &list[2];
    assert!(geld.stray, "the quiet word well after its neighbour is the stray");
    // Timed at the end of the word before: start == end == that end.
    assert_eq!((geld.start, geld.end), (10.6, 10.6));
    assert_eq!(geld.match_word, "geld", "matching never changes");
    assert_eq!(geld.written, "Geld.", "only the time moved, not the spelling");

    // The one line the spec asks for, quoted exactly.
    let want = format!(
        ">>> words: \u{201c}Geld.\u{201d} was placed on almost no sound, well after the word before it \
         -- timed at the end of that word instead ({})",
        mm_ss(10.6)
    );
    assert_eq!(logs, vec![want], "one line per retimed word, and (m:ss) off the new time");

    // A soft word immediately after another is ordinary speech: same quiet, gap under 1 s.
    let mut together = glued_from(&[("a", 0.0, 0.2), ("b", 0.25, 0.4)]);
    assert!(words::retimed(&mut together, envelope(&[(0.0, 1.0, 100), (0.2, 0.5, 8)])).is_empty());
    assert!(!together[1].stray);
    assert_eq!((together[1].start, together[1].end), (0.25, 0.4));

    // A loud word after a long gap is just a word after a pause.
    let mut after_pause = glued_from(&[("a", 0.0, 0.2), ("b", 5.0, 5.2)]);
    assert!(words::retimed(&mut after_pause, envelope(&[(0.0, 6.0, 100)])).is_empty());
    assert!(!after_pause[1].stray);

    // Exactly at the ratio is not under it: peak * STRAY_RATIO >= median leaves the word alone.
    let mut at_ratio = glued_from(&[("a", 0.0, 0.2), ("b", 5.0, 5.2)]);
    let peaks = envelope(&[(0.0, 1.0, 100), (4.9, 6.0, 10)]);
    assert!(words::retimed(&mut at_ratio, peaks).is_empty(), "10 * 10 == median 100");
    assert!(!at_ratio[1].stray);

    // No envelope for this recording: nothing can be called a stray, and nothing is said.
    let mut blind = glued_from(&[("a", 0.0, 0.2), ("b", 5.0, 5.2)]);
    assert!(words::retimed(&mut blind, |_s, _a, _b| None).is_empty());
    assert!(!blind.iter().any(|word| word.stray));

    // The same three words with the last one unmeasured: absence of sound under a word proves nothing, so
    // nothing moves -- and that word is not in the median either.
    let mut blind_word = glued_from(&[("a", 0.0, 0.2), ("b", 0.3, 0.5), ("c", 6.0, 6.2)]);
    assert!(words::retimed(&mut blind_word, envelope(&[(0.0, 1.0, 100), (0.25, 0.6, 90)])).is_empty());
    assert!(!blind_word[2].stray);

    // The median is over the words that DO have a peak: one unmeasured word neither lifts it nor drags it
    // down, and an unmeasured word is never itself retimed.
    let mut partly = glued_from(&[("a", 0.0, 0.2), ("b", 0.3, 0.5), ("c", 6.0, 6.2)]);
    let logs = words::retimed(&mut partly, envelope(&[(0.0, 1.0, 100), (0.25, 0.6, 90), (6.0, 6.3, 8)]));
    assert_eq!(logs.len(), 1, "peaks 100 and 90 put the median at 90; 8 is under a tenth of that");
    assert!(partly[2].stray);
    assert_eq!((partly[2].start, partly[2].end), (0.5, 0.5));

    // Both recordings are judged against their own median: a quiet room and a loud one side by side.
    let mut two = vec![word("a.mkv", "one", 0.0, 0.2), word("a.mkv", "two", 4.0, 4.2)];
    two.push(word("b.mkv", "one", 0.0, 0.2));
    two.push(word("b.mkv", "two", 4.0, 4.2));
    let by_source = move |source: &str, start: f64, end: f64| match source {
        // a.mkv words are all loud; b.mkv's second word is a tenth of its own median.
        "a.mkv" => Some(100),
        _ if start > 3.0 => Some(9),
        _ => Some(100),
    };
    let logs = words::retimed(&mut two, by_source);
    assert_eq!(logs.len(), 1, "only the recording with a quiet word says anything");
    assert!(!two[1].stray && two[3].stray);
}

// --- S3: the raw transcript's case and punctuation ---------------------------------------------------

#[test]
fn f1_13_s3_the_raw_transcript_dresses_the_bare_words() {
    // P.eng.dressReachWords = 8 (F1.13).
    assert_eq!(words::DRESS_REACH_WORDS, 8);

    let mut list = vec![
        word(SRC, "welcome", 0.0, 0.3),
        word(SRC, "back", 0.35, 0.5),
        word(SRC, "this", 0.6, 0.7),
        word(SRC, "is", 0.75, 0.8),
        word(SRC, "a", 0.85, 0.9),
        word(SRC, "weekly", 1.0, 1.2),
        word(SRC, "summary", 1.3, 1.6),
    ];
    words::dressed(&mut list, "Welcome back. This is a weekly summary");
    assert_eq!(
        words::written(&list),
        vec!["Welcome", "back.", "This", "is", "a", "weekly", "summary"]
    );

    // A word the ASR heard twice: the second saying looks eight fields ahead and finds nothing, so it
    // keeps what was heard rather than borrowing a later sentence's capital.
    let mut repeat = vec![word(SRC, "the", 0.0, 0.2), word(SRC, "end", 5.0, 5.2)];
    words::dressed(&mut repeat, "the quick brown fox jumps over the lazy dog end");
    // Reach is counted in fields from the cursor: `end` sits at index 8, one past the eighth lookahead.
    assert_eq!(words::written(&repeat), vec!["the", "end"]);

    // Inside the reach it does get dressed: the same list with the word one field nearer.
    let mut near = vec![word(SRC, "the", 0.0, 0.2), word(SRC, "end", 5.0, 5.2)];
    words::dressed(&mut near, "the quick brown fox jumps over the lazy end");
    assert_eq!(words::written(&near), vec!["the", "end"]);
    let mut inside = vec![word(SRC, "lazy", 5.0, 5.2)];
    words::dressed(&mut inside, "the quick brown fox jumps over the lazy end");
    assert_eq!(words::written(&inside), vec!["lazy"]);

    // A word the transcript has no counterpart for keeps the bare form: a caption missing a capital is a
    // smaller fault than one missing a word.
    let mut unknown = vec![word(SRC, "hm", 0.0, 0.2)];
    words::dressed(&mut unknown, "nothing of the sort");
    assert_eq!(unknown[0].written, "hm");

    // Per recording: each walks its own transcript, and one recording's cursor cannot spend another's.
    let mut both = vec![
        word("a.wav", "hallo", 0.0, 0.2),
        word("b.wav", "hallo", 0.0, 0.2),
    ];
    // Each recording is dressed from its own transcript and keeps its own cursor -- a slice per recording
    // is what the driver hands over (f1_13_s6 drives the whole thing).
    // One call dresses the list it is handed from one transcript; the driver asks per recording
    // (f1_13_s6), where each starts again from its own cursor.
    words::dressed(&mut both, "Hallo.");
    assert_eq!((both[0].written.as_str(), both[1].written.as_str()), ("Hallo.", "Hallo."));

    // Times never change here — only the written form.
    assert_eq!((list[0].start, list[0].end), (0.0, 0.3));
}

// --- S4: the fix pass's spelling ---------------------------------------------------------------------

#[test]
fn f1_13_s4_the_fix_pass_spelling_is_printed_on_the_first_word_of_a_run() {
    // P.eng.respellRunReachWords = 6 (F1.13). The written form carries a space, so it shares "rsa" with
    // what was heard -- enough for the walk to believe the pairing. One written
    // field against four spoken ones resyncs on neither side of the window, so the walk takes them as one
    // word: the respelling lands on the first and the rest of the run is emptied.
    assert_eq!(words::RESPPELL_REACH_WORDS, 6);

    let mut list = vec![
        word(SRC, "rsa", 0.0, 0.2),
        word(SRC, "two", 0.3, 0.4),
        word(SRC, "hundred", 0.5, 0.7),
        word(SRC, "and", 0.8, 0.9),
        word(SRC, "sixty", 1.0, 1.2),
    ];
    let before: Vec<(f64, f64)> = list.iter().map(|w| (w.start, w.end)).collect();

    assert!(words::respelled(&mut list, &["rsa", "260"]));
    // "260" resyncs two words into the run, so the fold prints it on the first word of what is left and
    // empties the words behind it. Every word keeps its own seconds.
    assert_eq!(list[0].written, "rsa");
    assert_eq!(&words::written(&list)[1..], &["260", "", "", ""]);
    let after: Vec<(f64, f64)> = list.iter().map(|w| (w.start, w.end)).collect();
    assert_eq!(before, after, "times never change");
    // Matching is untouched, so the fold cannot break a pass that reads the bare forms.
    assert_eq!(list[1].match_word, "two");

    // Resync at exactly the window and one past it.
    let mut at_window = vec![word(SRC, "one", 0.0, 0.2), word(SRC, "x", 0.3, 0.4)];
    for _ in 0..5 {
        at_window.push(word(SRC, "filler", 0.5, 0.6));
    }
    at_window.push(word(SRC, "found", 0.7, 0.8));
    // fixed[6] == "Found" is RESPPELL_REACH_WORDS fields ahead of the mismatch: within reach.
    let mut edge = at_window.clone();
    assert!(words::respelled(&mut edge, &["one", "a", "b", "c", "d", "e", "Found"]));
    assert_eq!(edge[1].written, "a b c d e");

    let mut past = vec![word(SRC, "one", 0.0, 0.2), word(SRC, "x", 0.3, 0.4)];
    for _ in 0..6 {
        past.push(word(SRC, "filler", 0.5, 0.6));
    }
    past.push(word(SRC, "found", 0.7, 0.8));
    let mut edge = past.clone();
    assert!(words::respelled(&mut edge, &["one", "a", "b", "c", "d", "e", "f", "Found"]));
    // Past the window the walk takes them as the same word anyway and the pairing is positional.
    assert_eq!(edge[1].written, "a");

    // A run that resyncs inside the window folds onto its first word: three spoken words against two
    // written, agreeing again on "sixty".
    let mut folded_run = vec![
        word(SRC, "rsa", 0.0, 0.2),
        word(SRC, "two", 0.3, 0.4),
        word(SRC, "hundred", 0.5, 0.7),
        word(SRC, "and", 0.8, 0.9),
        word(SRC, "sixty", 1.0, 1.2),
    ];
    assert!(words::respelled(&mut folded_run, &["rsa", "260", "sixty"]));
    assert_eq!(folded_run[0].written, "rsa");
    assert_eq!(folded_run[1].written, "260");
    assert_eq!(&words::written(&folded_run)[2..4], &["", ""], "a folded word keeps seconds, not text");
    assert_eq!(folded_run[4].written, "sixty");

    // Fixed words the recogniser never heard ride on the next word that has seconds.
    let mut extra = vec![word(SRC, "the", 0.0, 0.2), word(SRC, "system", 0.3, 0.5)];
    assert!(words::respelled(&mut extra, &["the", "brand", "new", "system"]));
    assert_eq!(extra[1].written, "brand new system");
    assert_eq!(extra[0].written, "the");

    // Leftover fixed words go onto the last word that has a time.
    let mut tail = vec![word(SRC, "one", 0.0, 0.2)];
    assert!(words::respelled(&mut tail, &["one", "two", "three"]));
    assert_eq!(tail[0].written, "one two three");

    // Leftover words when the line ran out are emptied: the fold is already printed on their first.
    let mut folded = vec![word(SRC, "twenty", 0.0, 0.2), word(SRC, "four", 0.3, 0.4)];
    assert!(words::respelled(&mut folded, &["twenty"]));
    assert_eq!(words::written(&folded), vec!["twenty", ""]);

    // A line with no word in common is left alone entirely — and says so by returning false.
    let mut apart = vec![word(SRC, "alpha", 0.0, 0.2), word(SRC, "beta", 0.3, 0.4)];
    let was = words::written(&apart);
    assert!(!words::respelled(&mut apart, &["gamma", "delta"]));
    assert_eq!(words::written(&apart), was, "the wrong words on these seconds is worse than none");

    // One word in common is enough to believe the pairing.
    let mut shared = vec![word(SRC, "alpha", 0.0, 0.2), word(SRC, "beta", 0.3, 0.4)];
    assert!(words::respelled(&mut shared, &["Alpha", "delta"]));
    assert_eq!(words::written(&shared), vec!["Alpha", "delta"]);

    // Nothing to walk: an empty line touches nothing.
    let mut none = vec![word(SRC, "alpha", 0.0, 0.2)];
    assert!(!words::respelled(&mut none, &[]));
    assert_eq!(none[0].written, "alpha");
}

// --- S5: the join pass reads this spelling -----------------------------------------------------------

#[test]
fn f1_13_s5_the_join_pass_matches_its_answer_against_these_tokens() {
    // tools::textedit::Join is shown word_list::written() and matches what the model sends back with
    // joins_token/owns: "a word printed as two owns both; survives if either does" (F1.13).
    let mut list = vec![
        word(SRC, "rsa", 0.0, 0.2),
        word(SRC, "two", 0.3, 0.4),
        word(SRC, "hundred", 0.5, 0.7),
        word(SRC, "sixty", 0.8, 1.0),
    ];
    assert!(words::respelled(&mut list, &["rsa", "260"]));

    // written() is the list in order, empties and all — a reader that wants text filters them out.
    assert_eq!(words::written(&list), vec!["rsa", "260", "", ""]);

    // A word printed as two owns both pieces: the printed spelling is split at its own seam — a hyphen or
    // a space — and either half matches.
    assert!(words::joins_token("RSA-260", "rsa"));
    assert!(words::joins_token("RSA-260", "260"), "the tail piece of a folded word still owns it");
    assert!(words::joins_token("rsa", "RSA-260"));
    assert!(words::joins_token("RSA 260", "260"));
    // An unrelated word, a fragment from the middle of one, and either side empty, are no match.
    assert!(!words::joins_token("rsa", "sixty"));
    assert!(!words::joins_token("walle", "all"));
    assert!(!words::joins_token("", "rsa"));
    assert!(!words::joins_token("rsa", ""));

    // Which word of the list a piece belongs to: the fold's first word, never one of the emptied ones.
    assert_eq!(words::owns(&list, "rsa"), Some(0));
    assert_eq!(words::owns(&list, "two"), Some(1), "the fold left its text on that word");
    assert_eq!(words::owns(&list, "sixty"), None, "its seconds are still there; its text is not");

    // Punctuation the model did not repeat is no obstacle.
    let punct = vec![word(SRC, "hello", 0.0, 0.2)];
    assert_eq!(words::owns(&punct, "hello,"), Some(0));
}

// --- S6: one list for the whole session --------------------------------------------------------------

#[test]
fn f1_13_s6_one_list_for_the_whole_session_and_its_parameters() {
    // F1.13: retakes, joins, final.txt and subtitles all read this one list. Two recordings at once — a
    // piece-token camera and a whole-word microphone, an envelope making one stray, a transcript to dress
    // from and one fixed line to re-dress from.
    let camera = vec![
        token(" hal", 10.0, 10.2),
        token("lo", 10.2, 10.4),
        token(" welt", 14.0, 14.1),
    ];
    let mic = vec![token("and", 0.0, 0.2), token("here", 0.3, 0.5)];
    let sources = vec![(format!("camera.mkv"), camera), (format!("mic.wav"), mic)];

    let loud = |source: &str, start: f64, end: f64| match source {
        "camera.mkv" if start > 13.9 => Some(8),
        "camera.mkv" => Some(120),
        _ => None,
    };
    let transcripts = |source: &str| match source {
        "camera.mkv" => Some("Hallo Welt".to_string()),
        _ => Some("And here".to_string()),
    };
    // The fixed line covers the camera's first word only; everything else is a skipped (EVENT/blank) row.
    let lines = |source: &str, start: f64, _end: f64| match source {
        "camera.mkv" if start < 11.0 => Some(vec!["Hallo".to_string(), "Welt".to_string()]),
        _ => None,
    };

    let (list, logs) = words::list(&sources, loud, transcripts, lines);

    // One list, in the order the recordings were handed over: two glued camera words, then the mic's two.
    assert_eq!(list.len(), 4);
    assert_eq!(list[0].source, "camera.mkv");
    assert_eq!(list[2].source, "mic.wav");

    // The stray rule is the only place a time moved: "Welt" on a breath after "Hallo".
    let welt = &list[1];
    assert!(welt.stray);
    assert_eq!((welt.start, welt.end), (10.4, 10.4));
    assert_eq!(logs.len(), 1, "the only thing this session says about itself");
    assert!(logs[0].starts_with(">>> words: \u{201c}"), "{}", logs[0]);

    // Dressing and respelling changed writing only: every other word keeps the seconds it was glued with.
    assert_eq!((list[0].start, list[0].end), (10.0, 10.4));
    assert_eq!(list[0].written, "Hallo");
    assert_eq!((list[2].start, list[2].end), (0.0, 0.2));
    assert_eq!(list[2].written, "And", "dressed from its own recording's transcript");
    assert_eq!(list[3].written, "here");

    // The four parameters F1.13 cites are in the catalogue §4 keeps, spelled as §10 spells them.
    assert_eq!(params::find("P.machine.strayWordRatio").unwrap().spelled, "10");
    assert_eq!(params::find("P.machine.strayWordGapSeconds").unwrap().spelled, "1");
    assert_eq!(params::find("P.eng.dressReachWords").unwrap().spelled, "8");
    assert_eq!(params::find("P.eng.respellRunReachWords").unwrap().spelled, "6");
}
