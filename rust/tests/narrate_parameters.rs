// §07-narrate#4-parameters-used — spec/07-narrate.md §4, checked against naivepost::params::narrate() and the
// owners of every value it names.
//
// §4 is a list of ids rather than of numbers, so what these tests pin is that the two cannot drift: every id §4
// names either has a row built from the constant its rule reads — exercised through that rule, not just looked up
// — or is absent because no rule in this tree reads it yet, which is asserted too, so a number arriving with
// nothing behind it is caught here rather than shipping. Same shape as §06-effects#6's tests.

use naivepost::cut::Seg;
use naivepost::narrate_data::{self, Take};
use naivepost::narrate_pass as pass;
use naivepost::narrate_preview as preview;
use naivepost::narrate_screen as screen;
use naivepost::narrate_tts as tts;
use naivepost::narration::{self, Entry};
use naivepost::params;
use naivepost::shell;
use naivepost::textfmt::SessionLine;

fn narrate_rows() -> Vec<params::Param> {
    params::narrate()
}

/// Every id the catalogue carries, from all four sections: what a row's home is when §10 puts it in another
/// chapter's table.
fn all_sections() -> Vec<String> {
    params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .map(|param| param.id.to_string())
        .collect()
}

fn row(id: &str) -> params::Param {
    narrate_rows()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| {
            let ids: Vec<&str> = narrate_rows().iter().map(|param| param.id).collect();
            panic!("{id} is in §07-narrate §4's list and so must be catalogued; this section holds {ids:?}")
        })
}

/// A number §4 writes with a decimal point compared as a number: `params::num` trims the trailing zero, so
/// comparing strings would pin a spelling neither side chose.
fn number(id: &str) -> f64 {
    row(id).spelled.parse().unwrap_or_else(|_| panic!("{id} spells a number"))
}

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

fn line(at: f64, text: &str) -> Entry {
    Entry {
        s: 10.0,
        e: 20.0,
        at,
        text: text.into(),
        ..Default::default()
    }
}

fn row_at(start: f64, end: f64, text: &str) -> SessionLine {
    SessionLine {
        start,
        end,
        source: "2026-09-16 17-26-20".into(),
        who: "ANN".into(),
        text: text.into(),
    }
}

// --- S1: the word budget the narration call prints ------------------------------------------------------------

/// §07-narrate#4-parameters-used — the three ids of §4's word budget, exercised through the arithmetic that
/// prints a clip's word ceiling and through the heading that states it.
#[test]
fn sec_07_narrate_4_parameters_used_s1_the_word_budget_the_narration_call_prints() {
    // P.policy.narrationMinWords — the floor, so a two-second clip is still offered a sentence.
    assert_eq!(pass::word_ceiling(2.0), 8);
    assert_eq!(pass::word_ceiling(0.0), 8, "a clip with no seconds still gets the floor");

    // P.policy.narrationWordsPerSecond — between the ends it is 0.75 words per on-screen second, and the product
    // is truncated rather than rounded: 13.3 s affords 9.99 words, which is nine words and not ten.
    assert_eq!(pass::word_ceiling(13.333), 9);
    assert_eq!(pass::word_ceiling(14.0), 10);
    assert_eq!(pass::word_ceiling(20.0), 15);

    // P.policy.narrationMaxWords — the ceiling, so a ten-minute clip is not offered an essay.
    assert_eq!(pass::word_ceiling(100.0), 30);
    assert_eq!(pass::word_ceiling(10_000.0), 30, "the ceiling holds however long the clip is");

    // Catalogued from the constants the arithmetic reads, so a row and a rule cannot drift.
    assert_eq!(row("P.policy.narrationMinWords").spelled, "8");
    assert_eq!(
        row("P.policy.narrationMinWords").from,
        "narrate_pass::MIN_WORDS"
    );
    assert_eq!(row("P.policy.narrationMaxWords").spelled, "30");
    assert_eq!(row("P.policy.narrationMaxWords").from, "narrate_pass::MAX_WORDS");
    assert_eq!(number("P.policy.narrationWordsPerSecond"), 0.75);
    assert_eq!(
        row("P.policy.narrationWordsPerSecond").from,
        "narrate_pass::WORDS_PER_SECOND"
    );

    // And the heading the model reads states the same budget: one number, one place it is computed.
    let short = pass::clip_heading(1, &seg(0.0, 2.0));
    assert!(short.contains("at most 8 words"), "{short}");
    let long = pass::clip_heading(1, &seg(0.0, 600.0));
    assert!(long.contains("at most 30 words"), "{long}");
}

// --- S2: how far the call looks, and what language it speaks ---------------------------------------------------

/// §07-narrate#4-parameters-used — the ± context window that decides which transcript rows join a clip's brief,
/// and the language the speech request carries.
#[test]
fn sec_07_narrate_4_parameters_used_s2_how_far_the_call_looks_and_what_language_it_speaks() {
    // P.machine.narrationContextSeconds — §10:60 spells it "±4 s", and the brief is where it is read.
    assert_eq!(number("P.machine.narrationContextSeconds"), 4.0);
    assert_eq!(
        row("P.machine.narrationContextSeconds").from,
        "narrate_pass::CONTEXT_SECONDS"
    );

    let clips = [seg(20.0, 30.0)];
    let rows = [
        row_at(24.0, 26.0, "inside the clip"),
        // Ending 3 s before the clip is inside ±4 s; ending 5 s before is outside it.
        row_at(15.0, 17.0, "three seconds ahead of it"),
        row_at(13.0, 15.0, "five seconds ahead of it"),
    ];
    let brief = pass::brief(&clips, &rows, &[], "own");
    assert!(brief.contains("inside the clip"), "{brief}");
    assert!(
        brief.contains("three seconds ahead of it"),
        "a row ending 3 s before a clip is within ±4 s: {brief}"
    );
    assert!(
        !brief.contains("five seconds ahead of it"),
        "a row ending 5 s before a clip is outside ±4 s: {brief}"
    );
    // The window's own edge, half-open as the code reads it: `row.end <= seg.s - 4` drops the row, so a row
    // ending exactly four seconds out is already gone.
    let edge = [row_at(14.0, 16.0, "exactly at the edge")];
    assert!(
        !pass::brief(&clips, &edge, &[], "own").contains("exactly at the edge"),
        "the window excludes its own far edge"
    );
    // The user message is built from the same brief, so it cannot reach further.
    let user = pass::user_message("context", &clips, &rows, &[], "own");
    assert!(user.contains("three seconds ahead of it"));
    assert!(!user.contains("five seconds ahead of it"));

    // P.policy.ttsLanguage — §10:91 gives no number: the project's language is the value, and the fallback is
    // the prototype's hard-coded "en" for a project that names none.
    assert_eq!(tts::language(""), tts::LANGUAGE_FALLBACK);
    assert_eq!(tts::LANGUAGE_FALLBACK, "en");
    assert_eq!(tts::language("  pl  "), "pl", "a real language is never overridden");
    assert_eq!(row("P.policy.ttsLanguage").spelled, "project language");
    assert_eq!(row("P.policy.ttsLanguage").from, tts::LANGUAGE_SOURCE);
}

// --- S3: the seconds the page and the preview wait with --------------------------------------------------------

/// §07-narrate#4-parameters-used — the tail that ends a line's estimate, the ceiling the preview holds at, and
/// the audition lead-in ▶ seeks back by.
#[test]
fn sec_07_narrate_4_parameters_used_s3_the_seconds_the_page_and_preview_wait_with() {
    // P.eng.narrationTailSeconds, P.eng.narrationMaxExtendSeconds, P.eng.narrationRunInSeconds.
    assert_eq!(number("P.eng.narrationTailSeconds"), 0.2);
    assert_eq!(
        row("P.eng.narrationTailSeconds").from,
        "narrate_screen::SPEECH_TAIL_SECONDS"
    );
    assert_eq!(number("P.eng.narrationMaxExtendSeconds"), 4.0);
    assert_eq!(
        row("P.eng.narrationMaxExtendSeconds").from,
        "narrate_preview::MAX_EXTEND_SECONDS"
    );
    assert_eq!(number("P.eng.narrationRunInSeconds"), 3.0);
    assert_eq!(
        row("P.eng.narrationRunInSeconds").from,
        "narrate_screen::AUDITION_LEAD_SECONDS"
    );

    // P.eng.speechCharsPerSecond is catalogued under the id §07-narrate#1-screen chose; §4 names the same value.
    assert_eq!(number("narrate.speechCharsPerSecond"), 15.0);
    assert_eq!(
        row("narrate.speechCharsPerSecond").from,
        "narrate_screen::SPEECH_CHARS_PER_SECOND"
    );

    // The lead-in: three seconds of picture before the line, never back past the clip's start.
    let entries = [line(10.0, "a line ten seconds into its clip")];
    assert_eq!(preview::lead_in(0, &entries), 17.0);
    let early = [line(1.0, "a line at the clip's head")];
    assert_eq!(preview::lead_in(0, &early), 10.0, "the clip's start rather than a second before it");

    // The estimate is 15 characters a second plus its tail because no take exists to measure yet — 30 chars
    // over 15 is 2 s of speech, and the tail counts since "speaking until mm:ss" names the line's end. So ＋
    // inside those seconds is refused with the second to add after, and accepted once they have run out.
    let clips = [seg(10.0, 20.0)];
    let spoken = [line(0.0, &"x".repeat(30))];
    let refused = screen::add_at_playhead(11.5, &clips, &spoken);
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.contains("a line is speaking here until")),
        "＋ 1.5 s into a 2.2 s line has to be refused: {refused:?}"
    );
    let accepted = screen::add_at_playhead(12.5, &clips, &spoken);
    assert_eq!(accepted, Ok(10.0), "past the estimate the clip is free again");
}

// --- S4: the take floor and the alpha that rides with a take ---------------------------------------------------

/// §07-narrate#4-parameters-used — the shortest take ＋ accepts, and the emotion alpha F4.4 sends with a take.
#[test]
fn sec_07_narrate_4_parameters_used_s4_the_take_floor_and_the_alpha_that_rides_with_a_take() {
    // P.eng.takeMinSeconds — asked before the take is made, and again by the clean-up that stores it.
    assert!(screen::can_make_take((0.0, 0.4)));
    assert!(!screen::can_make_take((0.0, 0.39)));
    let kept = narrate_data::clean_takes(&[Take { s: 0.0, e: 5.0 }, Take { s: 8.0, e: 8.3 }]);
    assert_eq!(kept, vec![Take { s: 0.0, e: 5.0 }], "the short one never reaches disk");
    assert_eq!(number("P.eng.takeMinSeconds"), 0.4);
    assert_eq!(row("P.eng.takeMinSeconds").from, "narrate_screen::TAKE_MIN_SECONDS");

    // P.eng.emotionAlpha — the alpha rides with a take whose emotion is a name the engine has to interpret; an
    // exact vector is not diluted, so that path sends 1.
    assert_eq!(tts::options("", 9)["emotion_alpha"], "0.85");
    assert_eq!(tts::options("wonderful", 9)["emotion_alpha"], tts::EMOTION_ALPHA);
    assert_eq!(
        tts::options("angry=1", 9)["emotion_alpha"],
        tts::VECTOR_ALPHA,
        "an exact weight is not diluted"
    );
    assert_eq!(row("P.eng.emotionAlpha").spelled, "0.85");
    assert_eq!(row("P.eng.emotionAlpha").from, "narrate_tts::EMOTION_ALPHA");
}

// --- S5: §4's engineering sentence ----------------------------------------------------------------------------

/// §07-narrate#4-parameters-used — "tick 100 ms, seek debounce 120 ms, autosave 400 ms, band geometry, cache key
/// format (frozen)". The first three and the geometry have no `P.` row to inherit and say so by their prefix; the
/// key format is a format rather than a number, so it is pinned as one.
#[test]
fn sec_07_narrate_4_parameters_used_s5_the_engineering_sentence_tick_debounce_autosave_and_geometry() {
    // narrate.tickMs / narrate.seekDebounceMs / narrate.autosaveMs — §10:133's prose line and §07's own.
    assert_eq!(row("narrate.tickMs").spelled, "100");
    assert_eq!(row("narrate.tickMs").from, "narrate_screen::TICK_MS");
    assert_eq!(row("narrate.seekDebounceMs").spelled, "120");
    assert_eq!(
        row("narrate.seekDebounceMs").from,
        "narrate_screen::SEEK_DEBOUNCE_MS"
    );
    assert_eq!(row("narrate.autosaveMs").spelled, "400");
    assert_eq!(row("narrate.autosaveMs").from, "shell::NARRATION_AUTOSAVE");
    assert_eq!(shell::NARRATION_AUTOSAVE.as_millis().to_string(), "400");

    // The band's geometry: §10 lists these by area, so one row per number and no `P.` prefix.
    for (id, spelled, from) in [
        ("narrate.bandRulerPx", "12", "narrate_screen::BAND_RULER_PX"),
        ("narrate.bandLanePx", "56", "narrate_screen::BAND_LANE_PX"),
        ("narrate.bandMaxPps", "200", "narrate_screen::BAND_MAX_PPS"),
        ("narrate.clickSlopPx", "3", "narrate_screen::CLICK_SLOP_PX"),
        ("narrate.takeLabelPx", "34", "narrate_screen::TAKE_LABEL_MIN_PX"),
    ] {
        assert_eq!(row(id).spelled, spelled, "{id}");
        assert_eq!(row(id).from, from, "{id}");
    }

    // The cache key format, frozen: changing it re-speaks every project, so the whole string is pinned rather
    // than its shape. §4's Engineering sentence counts this as a parameter in use on this page.
    let entry = Entry {
        s: 1.5,
        e: 34.7,
        text: "We start with …".to_string(),
        emotion: "calm".to_string(),
        ..Default::default()
    };
    let key = narration::tts_key(&entry, None, None);
    assert_eq!(key, "25e0.85|We start with …|calm");
    assert!(key.starts_with("25e0.85|"), "{key}");

    // One filename and one seed, both cut from the same digest: §01 §4's rule, which §4 counts as used here.
    let file = narration::tts_file(&key);
    assert_eq!(file.len(), 16, "{file}");
    assert!(
        file.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "{file} is not lowercase hex"
    );
    let rolled = narration::tts_key(
        &Entry {
            roll: 2,
            ..entry
        },
        None,
        None,
    );
    // §01 §4's key: `[<roll>#]` sits in front of everything but the prefix.
    assert_eq!(rolled, "25e0.85|2#We start with …|calm");
    assert_ne!(narration::tts_file(&key), narration::tts_file(&rolled));
    assert_ne!(narration::tts_seed(&key), narration::tts_seed(&rolled));
}

// --- S6: what §4 names that has no row, and why ---------------------------------------------------------------

/// §07-narrate#4-parameters-used — the ids §4 lists that no rule in this tree reads yet. A number nothing reads
/// has no module to live in, so asserting their absence is what stops one arriving as a decoration.
#[test]
fn sec_07_narrate_4_parameters_used_s6_what_this_section_names_that_has_no_row_and_why() {
    let listed = all_sections();

    // F4.3's fitting arithmetic: packing and speeding live in the render, which this tree has not written yet.
    for id in [
        "P.eng.narrationLeadSeconds",
        "P.eng.narrationGapSeconds",
        "P.eng.narrationMaxTempo",
    ] {
        assert!(
            !listed.iter().any(|seen| seen == id),
            "{id} is F4.3's fitting arithmetic, which has no constant in this tree: a row would claim a rule that does not exist"
        );
    }

    // F4.6/F4.7's automatic voice reference, likewise unimplemented — hand-picked takes are the only ones taken.
    for id in [
        "P.eng.refMinTakeSeconds",
        "P.eng.refPadSeconds",
        "P.machine.refWantSeconds",
        "P.machine.refTakeMax",
        "P.eng.refMinWordsPerSecond",
    ] {
        assert!(
            !listed.iter().any(|seen| seen == id),
            "{id} belongs to F4.6/F4.7's automatic voice reference, which this tree does not run yet"
        );
    }

    // §10:194 spells speechCharsPerSecond as "15 (8..28)". The 15 is read; nothing corrects the estimate, so the
    // bounds are absent while the default has a row — under the id §07-narrate#1-screen chose.
    assert!(listed.iter().any(|id| id == "narrate.speechCharsPerSecond"));
    for bound in ["speechRateFloor", "speechRateCeiling", "speechCharsPerSecondFloor"] {
        assert!(
            !listed.iter().any(|id| id.contains(bound)),
            "{bound} would be a bound nothing corrects"
        );
    }

    // The five ids §10 keeps outside §5's engineering table still have exactly one home each, in this section.
    for id in [
        "P.machine.narrationContextSeconds",
        "P.policy.narrationMinWords",
        "P.policy.narrationMaxWords",
        "P.policy.narrationWordsPerSecond",
        "P.policy.ttsLanguage",
        // §4's alpha, which §10 puts at §10:195 among the engineering constants.
        "P.eng.emotionAlpha",
    ] {
        let homes = listed.iter().filter(|seen| *seen == id).count();
        assert_eq!(homes, 1, "{id} would have {homes} homes; one bound lives in one row");
    }
}

// --- S7: the list's own shape ----------------------------------------------------------------------------------

/// §07-narrate#4-parameters-used — every row names where its value lives, no id repeats, and every id is either
/// this page's own (`P.eng.`/`narrate.`) or one of the six §10 puts in another chapter's table.
#[test]
fn sec_07_narrate_4_parameters_used_s7_every_row_names_where_its_value_lives() {
    let rows = narrate_rows();
    assert!(rows.len() >= 19, "{:?} rows", rows.len());

    for param in &rows {
        assert!(
            param.from.contains("::"),
            "{} names {} rather than a module::CONSTANT",
            param.id,
            param.from
        );
        assert!(!param.spelled.is_empty(), "{} spells nothing", param.id);
    }

    let ids: Vec<&str> = rows.iter().map(|param| param.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "one row repeated itself: {ids:?}");

    let foreign: Vec<&str> = ids
        .iter()
        .filter(|id| !id.starts_with("P.eng.") && !id.starts_with("narrate."))
        .copied()
        .collect();
    assert_eq!(
        foreign,
        [
            "P.machine.narrationContextSeconds",
            "P.policy.narrationMinWords",
            "P.policy.narrationMaxWords",
            "P.policy.narrationWordsPerSecond",
            "P.policy.ttsLanguage"
        ],
        "{foreign:?} — anything else this section carries belongs in another chapter's table"
    );
}
