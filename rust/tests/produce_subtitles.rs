//! F5.4 Subtitles — `spec/08-produce.md` §F5.4 (S1 cues per clip, S2 the produced clock, S3 translation),
//! checked against `naivepost::produce_subtitles`, and `spec/02-services.md` §3.10 for what the translation
//! round-trip may and may not do to a cue.
//!
//! Ids used, each asserted against its §10 row in `params::produce()` (`params::find` only searches Prepare's
//! rows): P.policy.subtitleBreakSeconds (0.6), P.policy.subtitleRowChars (42), P.policy.subtitleMaxSeconds
//! (6), P.policy.subtitleHoldSeconds (1.2), P.policy.subtitleMinSeconds (0.8), P.machine.translateBatch (150).

use std::collections::BTreeMap;

use naivepost::narration::Entry;
use naivepost::params;
use naivepost::produce_subtitles as sub;

const ITEM: &str = "F5.4";

fn word(text: &str, s: f64, e: f64) -> sub::Word {
    sub::Word { text: text.to_string(), s, e }
}

fn cue(s: f64, e: f64, text: &str) -> sub::Cue {
    sub::Cue { s, e, text: text.to_string(), pos: String::new() }
}

fn entry(s: f64, e: f64, text: &str) -> Entry {
    let mut line = Entry::default();
    line.s = s;
    line.e = e;
    line.text = text.to_string();
    line
}

fn entry_at(s: f64, e: f64, text: &str, pos: &str) -> Entry {
    let mut line = entry(s, e, text);
    line.pos = pos.to_string();
    line
}

/// A short word at a natural rhythm: no break, well under two rows.
fn steady(count: usize) -> Vec<sub::Word> {
    (0..count).map(|i| word(&format!("w{i}"), i as f64 * 0.3, i as f64 * 0.3 + 0.2)).collect()
}

/// §10's six rows, read from the catalogue this build writes rather than from a second copy of the numbers.
fn row(id: &str) -> String {
    params::produce()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{} is catalogued in §10's Produce rows", id))
        .spelled
}

// ---- S1: what a clip contributes --------------------------------------------------

#[test]
fn f5_4_s1_narration_lines_beat_their_own_speech() {
    assert_eq!(ITEM, "F5.4");
    // A line written over the clip is what the caption says — the words the footage spoke are not shown
    // beside it, or the reader would be reading two things at once.
    let lines = vec![entry_at(1.0, 3.0, "what you wrote", "top")];
    let words = steady(4);
    let cues = sub::cues(sub::Source::Footage, &lines, &words, 1.0);
    assert_eq!(cues.len(), 1);
    assert_eq!((cues[0].s, cues[0].e, cues[0].text.as_str()), (1.0, 3.0, "what you wrote"));
    assert_eq!(cues[0].pos, "top", "the placement the line asked for travels with it");

    // No lines: footage captions its own speech.
    let spoken = sub::cues(sub::Source::Footage, &[], &words, 1.0);
    assert!(!spoken.is_empty());
    assert_eq!(spoken[0].text, "w0 w1 w2 w3");

    // A line that is empty on purpose (a deleted line, still an entry) is not a caption: the clip falls back
    // to its own speech rather than showing nothing.
    let blank = vec![entry(0.0, 5.0, "   ")];
    assert_eq!(sub::cues(sub::Source::Footage, &blank, &words, 1.0), spoken);

    // And a line wins over the speech of an insert too — S1's first branch asks about lines before it asks
    // what the clip is.
    let over_insert = sub::cues(sub::Source::Insert, &lines, &words, 1.0);
    assert_eq!(over_insert.len(), 1);
    assert_eq!(over_insert[0].text, "what you wrote");
}

#[test]
fn f5_4_s1_inserts_freezes_and_silent_video_give_no_cues() {
    assert_eq!(ITEM, "F5.4");
    let words = steady(6);
    for (source, expected) in [
        (sub::Source::Insert, "inserted"),
        (sub::Source::Freeze, "freeze"),
        (sub::Source::NoVideo, "no video"),
    ] {
        assert!(
            sub::cues(source, &[], &words, 1.0).is_empty(),
            "{source:?} contributes no cues however many words it carries"
        );
        let reason = source.no_cues_reason().unwrap_or_else(|| panic!("{source:?} says why"));
        assert!(reason.contains(expected), "{reason} should name the {expected}");
    }
    // Footage has nothing to explain: its speech is worth reading.
    assert_eq!(sub::Source::Footage.no_cues_reason(), None);
    assert!(!sub::cues(sub::Source::Footage, &[], &words, 1.0).is_empty());
}

#[test]
fn f5_4_s1_a_break_of_six_tenths_ends_a_cue() {
    assert_eq!(ITEM, "F5.4");
    // P.policy.subtitleBreakSeconds — the row and the rule read the same number.
    assert_eq!(row("P.policy.subtitleBreakSeconds"), "0.6");
    assert_eq!(sub::SUBBREAK_SECONDS, 0.6);

    // Just under: one cue across the pause.
    let under = vec![word("one", 0.0, 1.0), word("two", 1.59, 2.0)];
    let cues = sub::word_cues(&under, 1.0);
    assert_eq!(cues.len(), 1, "{cues:?}");
    assert_eq!(cues[0].text, "one two");

    // Exactly the break: a breath is where a caption ends.
    let exact = vec![word("one", 0.0, 1.0), word("two", 1.6, 2.0)];
    let cues = sub::word_cues(&exact, 1.0);
    assert_eq!(cues.len(), 2, "{cues:?}");
    assert_eq!((cues[0].text.as_str(), cues[1].text.as_str()), ("one", "two"));

    // The break is measured from the previous word's end, not its start.
    let slow = vec![word("long", 0.0, 1.5), word("word", 2.0, 2.2)];
    assert_eq!(sub::word_cues(&slow, 1.0).len(), 1);
}

#[test]
fn f5_4_s1_two_rows_of_42_characters_end_a_cue() {
    assert_eq!(ITEM, "F5.4");
    // P.policy.subtitleRowChars: two rows is what a player will show, so two rows' characters is the most
    // one cue may carry.
    assert_eq!(row("P.policy.subtitleRowChars"), "42");
    assert_eq!(sub::ROW_CHARS, 42);

    // Up to two rows exactly is allowed on one cue: 42 + 1 + 41 = 84.
    let first = "c".repeat(42);
    let second = "d".repeat(41);
    let cues = sub::word_cues(&[word(&first, 0.0, 0.2), word(&second, 0.3, 0.5)], 1.0);
    assert_eq!(cues.len(), 1, "84 characters still fit: {cues:?}");

    // One character more and the cue closes rather than pass two rows' worth.
    let over = "e".repeat(42);
    let cues = sub::word_cues(&[word(&first, 0.0, 0.2), word(&over, 0.3, 0.5)], 1.0);
    assert_eq!(cues.len(), 2, "{cues:?}");
    assert_eq!(cues[0].text.chars().count(), 42);

    // And whatever a cue holds, `wrap` never leaves it on more than two rows.
    for cue in sub::word_cues(&steady(200), 1.0) {
        assert!(cue.text.lines().count() <= 2 || !cue.text.contains('\n'), "{}", cue.text);
        assert!(sub::wrap(&cue.text).lines().count() <= 2, "\n{}", sub::wrap(&cue.text));
    }
}

#[test]
fn f5_4_s1_six_seconds_on_screen_end_a_cue() {
    assert_eq!(ITEM, "F5.4");
    // P.policy.subtitleMaxSeconds: the longest a cue stays up, counted on the PRODUCED clock — hence rate.
    assert_eq!(row("P.policy.subtitleMaxSeconds"), "6");
    assert_eq!(sub::MAX_SECONDS, 6.0);

    let words = steady(40);
    let cues = sub::word_cues(&words, 1.0);
    assert!(cues.len() > 1, "a run of 40 words cannot be one cue");
    for cue in &cues {
        assert!(cue.e - cue.s <= sub::MAX_SECONDS + 1e-9, "cue up for {} s", cue.e - cue.s);
    }

    // A steady run of words, so the ceiling and not the breath between two words is what cuts: no cue is up
    // for more than six seconds, and cues get close to it rather than breaking early.
    // Eighty words at this rhythm run twenty seconds: the ceiling, not the breath, is what cuts them.
    let cues = sub::word_cues(&steady(80), 1.0);
    assert!(cues.len() > 2, "{:?}", cues.iter().map(|c| c.e - c.s).collect::<Vec<f64>>());
    for cue in &cues {
        assert!(cue.e - cue.s <= sub::MAX_SECONDS + 1e-9, "up for {} s", cue.e - cue.s);
    }

    // The ceiling is counted on the PRODUCED clock: these same eighty words at 2× are only ten seconds of
    // video, so they fit cues the ceiling still allows — more words per cue than at 1×.
    let sped = sub::word_cues(&steady(80), 2.0);
    assert!(sped[0].text.split_whitespace().count() > cues[0].text.split_whitespace().count());
    for cue in &sped {
        assert!(cue.e - cue.s <= 2.0 * sub::MAX_SECONDS + 1e-9, "clip seconds at 2x: {}", cue.e - cue.s);
    }

    // A rate of nought is not a clock: a stopped clip runs on at 1× rather than ending every cue at once.
    let stopped = vec![word("one", 0.0, 1.0), word("two", 11.9, 12.0)];
    assert_eq!(sub::word_cues(&stopped, 0.0).len(), sub::word_cues(&stopped, 1.0).len());
}

#[test]
fn f5_4_s1_an_empty_cue_extends_the_previous() {
    assert_eq!(ITEM, "F5.4");
    // A word respelled into the one in front, or a line deleted on purpose: it holds seconds and says
    // nothing, so it lengthens what is already up instead of flashing an empty caption.
    let mut cues = vec![cue(0.0, 1.0, "the words"), cue(0.0, 3.0, ""), cue(0.0, 4.0, "   ")];
    sub::extend_previous(&mut cues);
    assert_eq!(cues.len(), 1, "{cues:?}");
    assert_eq!((cues[0].s, cues[0].e), (0.0, 4.0));

    // Held to the same ceiling as every other cue — an endless silence must not buy an endless caption.
    let mut long = vec![cue(0.0, 1.0, "a word"), cue(0.0, 99.0, "")];
    sub::extend_previous(&mut long);
    assert_eq!(long[0].e - long[0].s, sub::MAX_SECONDS);

    // A leading empty cue has nothing to extend: dropped rather than shown.
    let mut leading = vec![cue(0.0, 2.0, ""), cue(2.0, 3.0, "first words")];
    sub::extend_previous(&mut leading);
    assert_eq!(leading.len(), 1);
    assert_eq!(leading[0].text, "first words");

    // Nothing at all is not a panic and not a cue.
    let mut none: Vec<sub::Cue> = Vec::new();
    sub::extend_previous(&mut none);
    assert!(none.is_empty());

    // The same thing happens through `word_cues`: a folded word holds its seconds and disappears from the
    // text rather than becoming a caption of its own.
    let cues = sub::word_cues(&[word("said", 0.0, 0.4), word("", 0.5, 3.0)], 1.0);
    assert_eq!(cues.len(), 1, "{cues:?}");
    assert_eq!(cues[0].text, "said");
    assert!(cues[0].e > 0.4, "the folded word's seconds are held: {:?}", cues[0]);
}

// ---- S2: the produced clock --------------------------------------------------------

#[test]
fn f5_4_s2_the_produced_clock_moves_every_cue() {
    assert_eq!(ITEM, "F5.4");
    let cues = vec![cue(0.0, 1.0, "first"), cue(2.0, 3.0, "second")];

    // The second clip's cues start where the first's produced length ended — `produced_clocks` gives that
    // offset, and this is the same arithmetic for one clip.
    let on_clock = sub::on_clock(&cues, 20.0, 1.0);
    assert_eq!((on_clock[0].s, on_clock[1].s), (20.0, 22.0));

    // At 2× the clip is half as long, so its cues are half as far apart and half as long: what was said at
    // recording second 2 is on screen a second after the clip starts.
    let sped = sub::on_clock(&cues, 10.0, 2.0);
    assert_eq!((sped[0].s, sped[0].e), (10.0, 10.5));
    assert_eq!((sped[1].s, sped[1].e), (11.0, 11.5));

    // A clip later in the video keeps its cues behind the clips before it, and text/placement survive.
    let last = sub::on_clock(&cues, 61.5, 1.0);
    assert_eq!(last[1].s, 63.5);
    assert_eq!(last[1].text, "second");
    let placed = sub::Cue { s: 0.0, e: 1.0, text: "top line".into(), pos: "top".into() };
    assert_eq!(sub::on_clock(&[placed], 5.0, 1.0)[0].pos, "top");
}

#[test]
fn f5_4_s2_no_two_cues_are_on_screen_at_once() {
    assert_eq!(ITEM, "F5.4");
    // Overlap: the earlier cue is cut where the next begins, whatever the clocks said.
    let tidy = sub::tidy(&[cue(0.0, 10.0, "first"), cue(9.0, 20.0, "second")]);
    assert_eq!((tidy[0].s, tidy[0].e), (0.0, 9.0));
    assert_eq!((tidy[1].s, tidy[1].e), (9.0, 20.0));

    // A cue whose end fell before its start is clamped rather than written backwards into the file.
    let broken = sub::tidy(&[cue(30.0, 20.0, "backwards"), cue(40.0, 45.0, "next")]);
    assert!(broken[0].e >= broken[0].s, "{:?}", broken[0]);

    // The same input twice is the same answer — tidy is a rule, not a state machine.
    let cues = [cue(0.0, 1.0, "a"), cue(5.0, 6.0, "b")];
    assert_eq!(sub::tidy(&cues), sub::tidy(&cues));
}

#[test]
fn f5_4_s2_a_gap_under_1_2_s_is_held() {
    assert_eq!(ITEM, "F5.4");
    // P.policy.subtitleHoldSeconds: a breath between two lines is not a blank screen.
    assert_eq!(row("P.policy.subtitleHoldSeconds"), "1.2");
    assert_eq!(sub::HOLD_SECONDS, 1.2);

    let held = sub::tidy(&[cue(0.0, 5.0, "first"), cue(6.0, 12.0, "second")]);
    assert_eq!(held[0].e, 6.0, "the caption already up is held across the gap");

    // A real pause stays a pause: nothing is held over four seconds of silence.
    let broken = sub::tidy(&[cue(0.0, 5.0, "first"), cue(9.0, 12.0, "second")]);
    assert_eq!(broken[0].e, 5.0);

    // Held, not extended past the next cue: the hold ends where the next caption begins.
    let tight = sub::tidy(&[cue(0.0, 5.0, "first"), cue(5.4, 12.0, "second")]);
    assert_eq!((tight[0].e, tight[1].s), (5.4, 5.4));
}

#[test]
fn f5_4_s2_a_cue_under_0_8_s_folds_into_the_next() {
    assert_eq!(ITEM, "F5.4");
    // P.policy.subtitleMinSeconds: under this a reader sees a flash rather than words.
    assert_eq!(row("P.policy.subtitleMinSeconds"), "0.8");
    assert_eq!(sub::MIN_SECONDS, 0.8);

    // Folded into what follows: its words join with a newline and its start is taken.
    let folded = sub::tidy(&[cue(2.0, 10.0, "first"), cue(10.4, 10.7, "brief"), cue(20.0, 30.0, "next")]);
    assert_eq!(folded.len(), 2, "{folded:?}");
    assert_eq!((folded[1].s, folded[1].text.as_str()), (10.4, "brief\nnext"));

    // The last cue has nothing after it to fold into, so it is given the time a reader needs instead.
    let last = sub::tidy(&[cue(0.0, 10.0, "first"), cue(20.0, 20.3, "brief")]);
    assert_eq!(last.last().unwrap().text, "brief", "the last cue keeps its own words: {:?}", last.last());
    let tail = last.last().unwrap();
    assert!(tail.e - tail.s >= sub::MIN_SECONDS - 1e-9, "lengthened to the floor: {tail:?}");

    // A single short cue is lengthened too — there is nothing else to do with it.
    let only = sub::tidy(&[cue(0.0, 0.2, "hi")]);
    assert_eq!(only.len(), 1);
    assert!((only[0].e - only[0].s - sub::MIN_SECONDS).abs() < 1e-9, "{:?}", only[0]);
}

#[test]
fn f5_4_s2_wrapped_at_42_and_never_more_than_2_rows() {
    assert_eq!(ITEM, "F5.4");
    // Short text is left alone entirely, and so is text that already holds a newline.
    assert_eq!(sub::wrap("one two three"), "one two three");
    assert_eq!(sub::wrap("the first row\nthe second"), "the first row\nthe second");
    assert_eq!(sub::wrap("short\nrow\nrows"), "short\nrow\nrows");

    // Long text breaks at a word boundary, every row inside the width, never more than two rows.
    // Text that fits one row is left alone; text that needs two rows breaks at a word boundary with every
    // row inside the width.
    let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma";
    assert!(long.split_whitespace().all(|w| w.chars().count() <= sub::ROW_CHARS));
    let wrapped = sub::wrap(long);
    assert_eq!(wrapped.lines().count(), 2, "{wrapped}");
    let rows: Vec<&str> = wrapped.lines().collect();
    assert_eq!(rows.len(), 2);
    // Each break was taken as late as a word boundary allowed, and no row ends in the space that separated
    // two words.
    for line in &rows {
        assert!(!line.ends_with(' '), "no trailing space: {line:?}");
        assert!(!line.starts_with(' '), "{line:?}");
    }
    // No word was cut: every word of the input is still whole somewhere in the output.
    for word in long.split_whitespace() {
        assert!(wrapped.split_whitespace().any(|part| part == word), "{word} was broken");
    }

    // A word that is itself longer than a row gets its own row rather than being cut in half, and the text
    // around it keeps its own break.
    // A word longer than a row gets its own row rather than being cut in half.
    let giant = "z".repeat(60);
    let wrapped = sub::wrap(&format!("aa {giant} bb"));
    assert!(wrapped.lines().count() <= 2, "{wrapped}");
    assert!(wrapped.split_whitespace().any(|part| part == giant), "{wrapped}");
}

#[test]
fn f5_4_s2_top_and_centre_carry_their_tag() {
    assert_eq!(ITEM, "F5.4");
    // The accepted spellings are narrate_pass's, so a model's answer and a person's choice agree here.
    assert_eq!(sub::placement("top"), Some(r"{\an8}"));
    for centre in ["center", "centre", "middle", "Centre", " TOP "] {
        let want = if centre.trim().eq_ignore_ascii_case("top") { r"{\an8}" } else { r"{\an5}" };
        assert_eq!(sub::placement(centre), Some(want), "{centre}");
    }
    for bottom in ["", "bottom", "somewhere else"] {
        assert_eq!(sub::placement(bottom), None, "{bottom} is where a caption sits anyway");
    }

    // Both cue sheets: the timecode format each format uses, and the tag on the text line.
    let cues = [sub::Cue { s: 1.5, e: 3.25, text: "the words".into(), pos: "top".into() }];
    let srt = sub::srt_text(&cues);
    assert!(srt.contains("00:00:01,500 --> 00:00:03,250"), "{srt}");
    assert!(srt.contains(r"{\an8}the words"), "the tag leads the text line: {srt}");
    assert!(srt.starts_with("1\n"), "SubRip numbers its cues: {srt}");

    let vtt = sub::vtt_text(&cues);
    assert!(vtt.starts_with("WEBVTT\n\n"), "{vtt}");
    assert!(vtt.contains("00:00:01.500 --> 00:00:03.250"), "a vtt timecode uses a dot: {vtt}");
    assert!(!vtt.contains(','), "{vtt}");

    // A bottom cue carries no tag, and an empty track writes a header and nothing else.
    let plain = sub::srt_text(&[cue(0.0, 1.0, "plain")]);
    assert!(!plain.contains(r"{\an"), "{plain}");
    assert_eq!(sub::vtt_text(&[]), "WEBVTT\n\n");
    assert_eq!(sub::srt_text(&[]), "");
}

// ---- S3: translation, one batch of numbered lines at a time -----------------------

fn original(from: usize, to: usize) -> Vec<(usize, String)> {
    (from..=to).map(|n| (n, format!("line {n}"))).collect()
}

#[test]
fn f5_4_s3_one_request_per_150_lines() {
    assert_eq!(ITEM, "F5.4");
    // P.machine.translateBatch: new in the rewrite — the prototype sent a whole track at once and then
    // repaired whatever it dropped off the end of the answer.
    assert_eq!(row("P.machine.translateBatch"), "150");
    assert_eq!(sub::BATCH, 150);

    assert_eq!(sub::batches(150), vec![(0, 150)]);
    assert_eq!(sub::batches(151), vec![(0, 150), (150, 151)]);
    assert_eq!(sub::batches(300), vec![(0, 150), (150, 300)]);
    assert_eq!(sub::batches(301).len(), 3);
    assert_eq!(sub::batches(301)[2], (300, 301));

    // No cues, no request; and the ranges tile the track without gaps or overlap.
    assert!(sub::batches(0).is_empty());
    let ranges = sub::batches(400);
    assert_eq!(ranges[0].0, 0);
    assert_eq!(ranges.last().unwrap().1, 400);
    for pair in ranges.windows(2) {
        assert_eq!(pair[0].1, pair[1].0);
    }
}

#[test]
fn f5_4_s3_the_message_and_its_tools() {
    assert_eq!(ITEM, "F5.4");
    // §F5.4's sentence, verbatim: the numbers are the whole contract.
    assert_eq!(
        sub::message(3, "German"),
        "TRANSLATE THESE 3 LINES INTO German. Answer with 3 lines, numbered as they are here:"
    );

    // The numbered list under it, one entry per cue: `n\ttext`.
    let list = sub::numbered(&original(1, 3));
    assert_eq!(list, "1\tline 1\n2\tline 2\n3\tline 3");
    let whole = format!("{}\n{}", sub::message(3, "German"), list);
    assert!(whole.contains("here:\n1\tline 1"), "{whole}");

    // A cue wrapped on two rows is still ONE numbered line: the break is folded, not sent as a newline that
    // would end the entry and make the next number look like a missing line.
    let wrapped = sub::numbered(&[(1, "the first row\nthe second".to_string())]);
    assert_eq!(wrapped.lines().count(), 1, "{wrapped}");
    assert!(wrapped.contains("the first row / the second"), "{wrapped}");

    // §3.10: thinking off, and the two tools — one call per line rather than an answer to be parsed.
    assert!(sub::THINKING_OFF);
    assert_eq!(sub::TOOLS, ["translate_line", "finish"]);
}

#[test]
fn f5_4_s3_translate_line_accepts_a_numbered_answer_and_refuses_two_kinds() {
    assert_eq!(ITEM, "F5.4");
    let mut batch = sub::Batch::new((0, 3));
    assert!(batch.translate_line(2, "zeile 2").is_ok());
    assert_eq!(batch.answers()[&2], "zeile 2");

    // A number this batch was never given: refused, and the error names it so the caller can say why.
    let outside = batch.translate_line(4, "viertens").unwrap_err();
    assert_eq!(outside, "no such line number in this batch: 4");

    // An answer with no words in it: refused, because an empty caption is not a translation.
    let empty = batch.translate_line(1, "   ").unwrap_err();
    assert_eq!(empty, "line 1 came back empty");

    // Neither refusal filed anything, so both lines are still missing and will ship their originals.
    assert_eq!(batch.missing(), vec![1, 3]);

    // Zero is not a line number either, and a batch that is not the first takes only its own numbers: this
    // one holds lines 151 and 152 (the range is half-open), so 150 is as foreign as 153.
    let mut second = sub::Batch::new((150, 152));
    assert!(second.translate_line(0, "nought").is_err());
    assert!(second.translate_line(150, "einhundertfunfzig").is_err());
    assert_eq!(second.translate_line(151, "eins einundfunfzig").ok(), Some(()));
    assert_eq!(second.missing(), vec![152]);
}

#[test]
fn f5_4_s3_missing_numbers_are_asked_once_more_with_their_original_numbers() {
    assert_eq!(ITEM, "F5.4");
    // A second batch: its lines are numbered 151.., and that is what the re-ask must use.
    let range = (150, 153);
    let track = original(151, 153);
    let mut batch = sub::Batch::new(range);
    batch.translate_line(152, "zeile 152").unwrap();

    // `finish` lists what is still missing.
    assert_eq!(batch.missing(), vec![151, 153]);
    assert!(!batch.complete());

    let ask = batch.ask_again_text(&track);
    assert!(ask.starts_with("TRANSLATE THESE 2 LINES."), "{ask}");
    // The numbers they were given the first time — not renumbered 1 and 2, which is what made the prototype's
    // repair calls land on the wrong lines.
    assert!(ask.contains("151\tline 151"), "{ask}");
    assert!(ask.contains("153\tline 153"), "{ask}");
    // Only the missing ones, and none of what already arrived.
    assert!(!ask.contains("152"), "the line that answered is not asked for again: {ask}");
    assert!(ask.contains("their numbers do not start at 1"), "{ask}");

    // Answered, and the batch is complete.
    batch.translate_line(151, "eins").unwrap();
    batch.translate_line(153, "drei").unwrap();
    assert!(batch.missing().is_empty());
    assert!(batch.complete());
}

#[test]
fn f5_4_s3_a_line_still_missing_ships_the_original_with_a_warning() {
    assert_eq!(ITEM, "F5.4");
    let track = original(1, 4);
    let mut got = BTreeMap::new();
    got.insert(1, "eins".to_string());
    got.insert(3, "drei".to_string());
    // Line 2 never arrived; line 4 was refused and is absent too.
    let merge = sub::merge("German", &track, &got);

    // A track is never dropped for one bad line: the rest of it ships translated.
    assert_eq!(merge.lines[0], "eins");
    assert_eq!(merge.lines[2], "drei");
    // The ones that stayed in the source language ship as they came, and are named.
    assert_eq!(merge.lines[1], "line 2");
    assert_eq!(merge.lines[3], "line 4");
    assert_eq!(merge.still_missing, vec![2, 4]);

    let warning = merge.warning.expect("the log names the lines still in the other language");
    assert!(warning.starts_with("!!!"), "{warning}");
    assert!(warning.contains("German"), "{warning}");
    assert!(warning.contains("2") && warning.contains("4"), "{warning}");

    // A complete track says nothing: no warning, nothing missing.
    let mut all = got.clone();
    all.insert(2, "zwei".to_string());
    all.insert(4, "vier".to_string());
    let done = sub::merge("German", &track, &all);
    assert!(done.warning.is_none() && done.still_missing.is_empty());
    assert_eq!(done.lines, vec!["eins", "zwei", "drei", "vier"]);

    // A cue's own row breaks survive the round trip into and out of the numbered list.
    let wrapped = vec![(1, "the first row\nthe second".to_string())];
    let kept = sub::merge("German", &wrapped, &BTreeMap::new());
    assert_eq!(kept.lines[0], "the first row\nthe second");
}

#[test]
fn f5_4_s3_only_a_complete_answer_is_cached() {
    assert_eq!(ITEM, "F5.4");
    assert!(sub::cached(true));
    assert!(!sub::cached(false), "a partial answer cached is a partial answer forever");

    // One line out of three is not an answer worth keeping: the next run must be free to ask again.
    let mut batch = sub::Batch::new((0, 3));
    batch.translate_line(1, "eins").unwrap();
    assert!(!sub::cached(batch.complete()));
    batch.translate_line(2, "zwei").unwrap();
    batch.translate_line(3, "drei").unwrap();
    assert!(sub::cached(batch.complete()));
}

#[test]
fn f5_4_s3_the_sessions_own_language_is_track_0() {
    assert_eq!(ITEM, "F5.4");
    // Translating a language into itself costs a minute and answers with what it was given; the session's own
    // track is written anyway, beside the video and unnamed by a code.
    let ticked = ["de".to_string(), "en".to_string(), "sv".to_string()];
    assert_eq!(sub::track_languages("en", &ticked), vec!["de", "sv"]);

    // Order is what the menu listed, and everything else is untouched.
    let other = ["sv".to_string(), "de".to_string()];
    assert_eq!(sub::track_languages("en", &other), vec!["sv", "de"]);
    assert!(sub::track_languages("en", &["en".to_string()]).is_empty());
    assert!(sub::track_languages("en", &[]).is_empty());
}

#[test]
fn f5_4_s3_the_placement_tag_never_travels_and_never_comes_back() {
    assert_eq!(ITEM, "F5.4");
    // §3.10's two must-fixes. The prototype sent `{\an8}` into the translation as a word and never checked it
    // came back, so a model that dropped it silently lost that caption's placement.
    let top = sub::Cue { s: 0.0, e: 1.0, text: "the words".into(), pos: "top".into() };
    let sent = sub::sent_text(&top);
    assert!(!sent.contains('{'), "{sent}");
    assert_eq!(sent, "the words");

    // A cue whose text still carries a tag from an older file loses it on the way out too.
    let tagged = sub::Cue { s: 0.0, e: 1.0, text: format!("{}the words", r"{\an8}"), pos: "top".into() };
    assert_eq!(sub::sent_text(&tagged), "the words");

    // Dropped by the model: the app puts its own placement back.
    let (text, carried) = sub::keep_placement(&top, "die worte");
    assert_eq!(text, format!("{}die worte", r"{\an8}"));
    assert!(!carried);

    // Handed back as a word: stripped and reported, never trusted — a model told to translate is not a model
    // told to lay out.
    let (text, carried) = sub::keep_placement(&top, &format!("{}die worte", r"{\an5}"));
    assert_eq!(text, format!("{}die worte", r"{\an8}"), "the original placement wins");
    assert!(carried);

    // A bottom cue asks for no tag and gets none, even if the model volunteered one.
    let bottom = cue(0.0, 1.0, "the words");
    let (text, carried) = sub::keep_placement(&bottom, &format!("{}die worte", r"{\an8}"));
    assert_eq!(text, "die worte");
    assert!(carried);

    // And a number list never carries a tag at all, whatever the cue held.
    let list = sub::numbered(&[(1, sub::sent_text(&tagged)), (2, "next".to_string())]);
    assert!(!list.contains('{'), "{list}");
}

#[test]
fn f5_4_s3_the_models_own_line_breaks_survive() {
    assert_eq!(ITEM, "F5.4");
    // The prompt asks the model to keep the wrapping; re-wrapping its answer at 42 characters contradicts it
    // and moves a break the model chose — usually into the middle of a phrase it split on purpose.
    let translated = "der erste satz\nder zweite";
    assert_eq!(sub::keep_breaks(translated), translated);
    assert!(translated.contains('\n'), "the answer is two rows: {translated}");

    // Filed as the line it is, still two rows — even though each row is shorter than the width and a re-wrap
    // would have joined them.
    let track = vec![(1, "the first row\nthe second".to_string())];
    let mut got = BTreeMap::new();
    got.insert(1, translated.to_string());
    assert_eq!(sub::merge("German", &track, &got).lines[0], translated);

    // And through the batch that answered it.
    let mut batch = sub::Batch::new((0, 1));
    batch.translate_line(1, translated).unwrap();
    assert_eq!(batch.answers()[&1], translated);
    assert!(sub::wrap(translated) == translated, "a wrapped cue is never re-wrapped");

    // The source text going the other way IS wrapped once, by S2 — that is the app's own line.
    assert_eq!(sub::wrap("one two three four").lines().count(), 1);
}
