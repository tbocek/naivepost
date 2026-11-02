//! §07-narrate#6 — "Details confirmed against the code (verification pass)".
//!
//! One test per §6 bullet of `spec/07-narrate.md` (lines 250-257), each named
//! `sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_sN…`: what ▶ keeps and
//! what it wipes, what an unusable narration reply says, what the Inputs row and its tooltip name, and every
//! re-roll / sample refusal and status. All eight bullets are here: the last four (reference build,
//! "Add file…", the take band, the preview's bounds) were appended into this same file.
//!
//! Nothing here is new app code: §6 confirms wording and behaviour that already exist, so every
//! assert calls a helper that is already shipped and copies the spec's string exactly (em dash
//! `\u{2014}`, mid dot `\u{b7}`, the glyphs ＋ `U+FF0B` and － `U+FF0D`).
//!
//! `tests/suite/narrate_details.rs` pins the same bullets' tooltip, take-band and reference wording from
//! the §10 side; the tests here add the F4.1/F4.2 seams (the file copy, the silent wipe, the two
//! jobs) that file does not reach, rather than restating what it already proves.

use std::path::PathBuf;

use naivepost::layout::Tree;
use naivepost::narrate_data::keep_previous;
use naivepost::narrate_details as det;
use naivepost::narrate_pass as pass;
use naivepost::narrate_reply as reply;
use naivepost::narrate_rules as rules;
use naivepost::narrate_run::{
    captions_only_done, fold_written, plan_run, run_with, Outcome, JOBS, JOB_NARRATION,
    JOB_SPEAKING,
};
use naivepost::narrate_screen::{self as screen, CAPTIONS};
use naivepost::narration::{Entry, Narration, Silent};
use naivepost::voice_ref;

/// The spec item every test in this file belongs to; each one asserts it, so a renamed item fails.
const ITEM: &str = "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass";

fn tmp_tree(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("np-s6pass-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.with_extension("naivepost");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a folder ending in .naivepost is a project");
    (dir, tree)
}

fn entry(s: f64, e: f64, at: f64, text: &str) -> Entry {
    Entry {
        s,
        e,
        at,
        text: text.to_string(),
        emotion: String::new(),
        pos: String::new(),
        roll: 0,
    }
}

/// The clips §F4.1's own tests use, so a plan built here matches theirs.
const CLIPS: [(f64, f64); 3] = [(0.0, 10.0), (10.0, 24.0), (24.0, 31.0)];

// ---- s1: the previous file, the silent list, and captions-only closing the bar ------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s1_previous_kept_silent_wiped_captions_closes_the_bar(
) {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );
    // §6, first sentence: the log names where the overwritten narration went. FOUR leading spaces,
    // because it hangs under the step's own line rather than starting one of its own.
    assert_eq!(
        det::previous_kept_log("demo.naivepost/narrate/narration.prev.json"),
        "    the narration it replaced is kept at demo.naivepost/narrate/narration.prev.json"
    );

    // "previous file copied aside ONLY ONCE A VALID NARRATION CAME BACK". On a tree with nothing to
    // copy the answer is a plain no, and no prev file appears.
    let (_root, tree) = tmp_tree("prev");
    assert_eq!(
        keep_previous(&tree),
        Ok(false),
        "no narration.json means nothing was replaced, so nothing is kept"
    );
    assert!(!tree.narration_prev_json().exists());

    // Copy an aside, then check BOTH files: the previous generation reads back as the record it was
    // copied from, and the current one is untouched by the copy.
    let before = Narration {
        entries: vec![entry(0.0, 10.0, 0.0, "the old words")],
        silent: vec![Silent { s: 24.0, e: 31.0 }],
    };
    naivepost::narration::save(&before, &tree).expect("narration.json written");
    let old_bytes = std::fs::read(tree.narration_json()).unwrap();
    assert_eq!(keep_previous(&tree), Ok(true));
    assert_eq!(
        std::fs::read(tree.narration_prev_json()).unwrap(),
        old_bytes
    );
    assert_eq!(
        naivepost::narrate_data::previous(&tree).expect("the previous generation parses"),
        before,
        "the kept file is the record the run replaced, not the one it wrote"
    );

    // "silent list cleared with the replacement": the marker a user left goes with the narration it
    // was written against as soon as a line lands on those same bounds.
    let replaced = fold_written(
        &before,
        &[naivepost::narrate_run::Written {
            start: 24.0,
            end: 31.0,
            at: 0.0,
            text: "the line that replaced the silence".to_string(),
            emotion: String::new(),
        }],
    );
    assert!(
        !replaced.is_silent(24.0, 31.0),
        "the silent marker goes with the replacement"
    );
    assert!(replaced.has_line(24.0, 31.0));
    // A marker on bounds the rewrite did not touch survives — §F4.1 S4's rule, unchanged here.
    let elsewhere = fold_written(
        &before,
        &[naivepost::narrate_run::Written {
            start: 0.0,
            end: 10.0,
            at: 0.0,
            text: "a rewrite of a narrated clip".to_string(),
            emotion: String::new(),
        }],
    );
    assert!(
        elsewhere.is_silent(24.0, 31.0),
        "a rewrite of one clip leaves the others' silence"
    );

    // "a captions-only voice still OPENS AND CLOSES the speaking 2/2 job so the bar finishes": the
    // job exists (§F4.1 queues it) even though nothing is spoken, and the close is the second job's
    // own name at 2/2. The voice answers the question "is there anything to speak" with a no.
    assert_eq!(JOBS, 2);
    assert_eq!(JOB_NARRATION, "narration");
    assert_eq!(JOB_SPEAKING, "speaking");
    assert!(rules::voice_is_captions(CAPTIONS));
    assert_eq!(pass::progress(2, 2), "writing 2/2 clips");

    // The run itself stops at the write, so the speaking half never synthesizes — but the outcome it
    // reports is the captions one, which is what lets the bar close rather than hang.
    let narration = Narration {
        entries: vec![entry(0.0, 10.0, 0.0, "already written")],
        silent: vec![],
    };
    let plan = plan_run(true, &CLIPS, &narration, CAPTIONS);
    assert!(plan.captions_only);
    let mut spoken = 0usize;
    let outcome = run_with(
        &plan,
        |_| Ok(2),
        |_, _, _| {
            spoken += 1;
            Ok(true)
        },
    );
    assert_eq!(outcome, Ok(Outcome::CaptionsOnly { written: 2 }));
    assert_eq!(spoken, 0, "captions only means NOTHING is spoken");
    assert_eq!(
        captions_only_done(3),
        "narrate: captions only \u{2014} 3 line(s) written, none spoken"
    );
}

// ---- s2: the validation refusals, and several entries on one clip ------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s2_reply_validation_wording(
) {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );
    // The halves a second of slack cannot bridge: the bounds are echoed from the request, so a reply
    // that does not echo one matches nothing.
    assert_eq!(reply::ENTRY_MATCH_TOLERANCE_SECONDS, 0.5);

    // "a skipped clip (\"clip N (a-b) got no entry\")". N is the 1-based clip number and the bounds
    // print at one decimal, which is what both seams do.
    let segs = [
        naivepost::cut::Seg {
            s: 0.0,
            e: 10.0,
            ..Default::default()
        },
        naivepost::cut::Seg {
            s: 10.0,
            e: 20.0,
            ..Default::default()
        },
        naivepost::cut::Seg {
            s: 20.0,
            e: 30.0,
            ..Default::default()
        },
    ];
    assert_eq!(
        pass::missing_clip_fault(2, 10.0, 24.0),
        "clip 2 (10.0-24.0) got no entry"
    );
    assert_eq!(
        reply::clip_without_entry(2, &segs),
        "clip 3 (20.0-30.0) got no entry"
    );
    assert_eq!(
        reply::clip_without_entry(2, &segs),
        pass::missing_clip_fault(3, 20.0, 30.0),
        "the call's own refusal and the prompt's fault are the same sentence"
    );

    // "no clip with a line (\"every clip came back with no line at all\")".
    assert_eq!(
        pass::no_line_fault(),
        "every clip came back with no line at all"
    );

    // "entries matched forward only (\"an entry says X-Y, which matches no clip (or is out of
    // order)\")": the bounds print bare, the fault carries the order clause.
    assert_eq!(
        reply::unmatched_entry_fault(30.0, 40.0),
        "an entry says 30-40, which matches no clip (or is out of order)"
    );
    assert_eq!(
        reply::unmatched_entry_fault(12.4, 18.9),
        "an entry says 12.4-18.9, which matches no clip (or is out of order)"
    );

    // "several entries on one clip sorted by offset" — the record sorts on every read and write.
    let record = Narration {
        entries: vec![
            entry(10.0, 20.0, 1.0, "second clip"),
            entry(0.0, 10.0, 4.0, "said later in the first clip"),
            entry(0.0, 10.0, 1.0, "said first"),
        ],
        silent: vec![],
    };
    let mut record = record;
    record.sort();
    let on_first: Vec<f64> = record
        .entries
        .iter()
        .filter(|line| line.s == 0.0)
        .map(|line| line.at)
        .collect();
    assert_eq!(
        on_first,
        vec![1.0, 4.0],
        "one clip's lines come out in offset order"
    );
}

// ---- s3: the Inputs tooltip names its inputs ----------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s3_inputs_tooltip_names_every_input(
) {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );
    // The tooltip is five rows in §6's order, each one an input the write reads.
    let tip = det::inputs_tooltip(
        2,
        120.0,
        "the cut changed",
        688,
        "a demo of the app",
        Some(("Mira", -1.5)),
    );
    let rows: Vec<&str> = tip.split("\n\n").collect();
    assert_eq!(rows.len(), 5, "{tip}");

    assert_eq!(rows[0], det::cut_detail(2, 120.0));
    assert!(rows[0].contains("cut/cut.json"), "{rows:?}");
    assert_eq!(
        det::cut_detail(2, 120.0),
        "cut/cut.json \u{2014} 2 clips, 02:00 of video to write for"
    );

    // The staleness sentence plus what fixes it, glued to the ⚠.
    assert_eq!(
        rows[1],
        "\u{26a0} the cut changed \u{2014} \u{25b6} writes the narration again"
    );
    // The two counts the ⚠ stands for, spelled the tooltip's way ("N clips", "N lines").
    assert_eq!(
        det::stale_mark(3, 2),
        "3 clips unwritten, 2 lines off the cut"
    );
    assert_eq!(det::stale_mark(0, 0), "");

    // The transcript row: the file, its line count, and the ± rule quoted with its §10 id.
    assert_eq!(det::CONTEXT_WINDOW_ID, "P.machine.narrationContextSeconds");
    assert_eq!(pass::CONTEXT_SECONDS, 4.0);
    assert!(
        rows[2].contains("prepare/transcript/session.tsv"),
        "{rows:?}"
    );
    assert!(rows[2].contains("688 lines"), "{rows:?}");
    assert!(rows[2].contains("\u{b1}4 s"), "{rows:?}");
    assert_eq!(det::timeline_line(0), None, "no transcript is no row");

    // The session context verbatim, then the voice line, which names the reference file.
    assert_eq!(
        rows[3],
        "Session context (Describe), sent with the narration:\na demo of the app"
    );
    assert_eq!(rows[4], det::voice_line("Mira", -1.5));
    assert_eq!(
        det::voice_line("Mira", -1.5),
        "Spoken by Mira at -1.5 semitones (narrate/voice_ref.wav)"
    );
    // A pitchless voice still gets the sign, because the line is read as "±X.X semitones".
    assert_eq!(
        det::voice_line("own", 0.0),
        "Spoken by own at +0.0 semitones (narrate/voice_ref.wav)"
    );

    // No cut means no tooltip: no clip row, no voice, nothing to name.
    assert_eq!(det::inputs_tooltip(0, 0.0, "", 0, "", None), "");
}

// ---- s4: re-roll and sample refusals ------------------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s4_reroll_and_sample_refusals(
) {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );
    // "clip N has no line to re-roll" — the page's refusal carries the clip number, which is why it
    // is `narrate_screen`'s (the number is only known where a row is) and not `narrate_details`'.
    assert_eq!(
        screen::re_roll_refusal(3, false),
        Some("clip 3 has no line to re-roll".to_string())
    );
    assert_eq!(
        screen::re_roll_refusal(3, true),
        None,
        "a clip with a line re-rolls"
    );
    // "no audio is chosen — a caption has no take to re-roll".
    assert_eq!(
        det::re_roll_captions_refusal(),
        "no audio is chosen \u{2014} a caption has no take to re-roll"
    );
    // "line N: new take, speaking it" — the row's number is 1-based in the sentence.
    assert_eq!(det::new_take_status(0), "line 1: new take, speaking it");

    // The sample's four refusals, each asking for a different click, so each its own sentence.
    assert_eq!(det::pick_a_voice_refusal(), "pick a voice first");
    assert_eq!(
        det::no_voice_to_sample_refusal(),
        "no audio is chosen \u{2014} there is no voice to sample"
    );
    assert_eq!(
        det::empty_sample_refusal(),
        "type a sample sentence to hear the voice"
    );
    assert_eq!(
        det::busy_sample_status(),
        "still synthesizing the last sample\u{2026}"
    );

    // Its three transport states.
    assert_eq!(det::sample_playing_status(), "sample playing");
    assert_eq!(
        det::sample_paused_status(),
        "sample paused \u{2014} \u{25b6} resumes, \u{23f9} starts over"
    );
    assert_eq!(det::sample_stopped_status(), "sample stopped");

    // "Each sample logs \">>> sample[ take N]: <voice> at ±X semitones — \"text\"" then the file line.
    assert_eq!(
        det::sample_log("Mira", -1.5, None, "one sentence"),
        ">>> sample: Mira at -1.5 semitones \u{2014} \"one sentence\""
    );
    assert_eq!(
        det::sample_log("Mira", 0.0, Some(1), "one sentence"),
        ">>> sample take 2: Mira at +0.0 semitones \u{2014} \"one sentence\""
    );
    // "    sample: <file> (N kB, spoken in T | spoken earlier)" — kB, not bytes, and a take that was
    // already synthesized says so rather than claiming a timing it did not measure.
    assert_eq!(
        det::sample_file_log("voice_ref-1.wav", 61_440, Some("0.4s")),
        "    sample: voice_ref-1.wav (60 kB, spoken in 0.4s)"
    );
    assert_eq!(
        det::sample_file_log("voice_ref-1.wav", 61_440, None),
        "    sample: voice_ref-1.wav (60 kB, spoken earlier)"
    );
    // "!!! sample: <file> is N bytes — no audio came back": the header-only answer is the failure a
    // player cannot show, so only the log carries it.
    assert_eq!(
        det::sample_too_small_log("voice_ref-1.wav", 44),
        "!!! sample: voice_ref-1.wav is 44 bytes \u{2014} no audio came back"
    );
}

// ---- s5: the reference build's log, and the automatic ranking ---------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s5_reference_build_and_ranking(
) {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );
    // §6 writes the line as "X s, N words | N hand-picked take(s) from <base>": the two kinds of
    // reference fail differently, so exactly one half ever appears — an automatic one that sounds
    // wrong is a ranking to overrule by hand, a hand-picked one is seconds to re-pick.
    assert_eq!(
        det::reference_build_log(13.4, 41, 0, "rec-2025-08-14"),
        ">>> voice reference built: 13.4 s, 41 words from rec-2025-08-14"
    );
    assert_eq!(
        det::reference_build_log(13.4, 41, 3, "rec-2025-08-14"),
        ">>> voice reference built: 13.4 s, 3 hand-picked take(s) from rec-2025-08-14"
    );

    // "speaker with the most turn time": the ranking sums each speaker's turns and takes the
    // largest, so a speaker with many short turns loses to one with fewer longer ones.
    let turns = [
        (0.0, 3.0, 1_u32),
        (4.0, 6.0, 1),
        (10.0, 20.0, 2),
        (30.0, 33.0, 1),
    ];
    assert_eq!(voice_ref::turn_seconds(&turns, 1), 8.0);
    assert_eq!(voice_ref::turn_seconds(&turns, 2), 10.0);
    assert_eq!(voice_ref::ranked_speaker(&turns), Some(2));
    // A lone speaker is picked rather than refused, and ties break to the lower id (§6: the
    // diarizer numbers speakers as it meets them, so the earlier id spoke first).
    assert_eq!(voice_ref::ranked_speaker(&[(0.0, 5.0, 3)]), Some(3));
    assert_eq!(
        voice_ref::ranked_speaker(&[(0.0, 5.0, 2), (6.0, 11.0, 1)]),
        Some(1),
        "a tie goes to the speaker who appeared first"
    );
    // No diarization at all is a refusal, never a guess.
    assert_eq!(voice_ref::ranked_speaker(&[]), None);
    // The word source these cases use: every window is filled edge to edge with words, so the narrowing
    // cannot change what is being tested here (§6's narrowing has its own tests below).
    let full = |from: f64, until: f64| Some((from, until));
    assert_eq!(
        voice_ref::plan_reference(&[], &[], &|_, _| 0, &full, 1),
        voice_ref::ReferencePlan::Unavailable(voice_ref::NO_DIARIZATION.to_string()),
        "no diarization stays a refusal rather than becoming a guess"
    );

    // The two numbers §6 names for the narrowing and the drop are ONE value each, held by the
    // module whose rule reads them (`refPadSeconds` = how far another speaker must stay clear,
    // `refMinWordsPerSecond` = how dense a turn must be to be worth cloning).
    assert_eq!(voice_ref::REF_PAD_SECONDS, 2.0);
    assert_eq!(voice_ref::REF_MIN_WORDS_PER_SECOND, 1.5);
    assert_eq!(voice_ref::REF_MIN_TAKE_SECONDS, 5.0);
    assert_eq!(voice_ref::REF_TAKE_MAX, 3);
    assert_eq!(det::REFERENCE_WANTED_SECONDS, 14.0);

    // Hand-picked takes outrank diarization outright — no cap, no diarization needed.
    assert_eq!(
        voice_ref::plan_reference(&[(2.0, 9.0)], &turns, &|_, _| 0, &full, 2),
        voice_ref::ReferencePlan::HandPicked(vec![voice_ref::Piece {
            start: 2.0,
            end: 9.0
        }])
    );
    // A solo stretch crowded by another speaker inside the pad is not clean, so nothing survives
    // and the plan says so with §F4.6 S2's own sentence.
    let crowded = [(0.0, 10.0, 1_u32), (11.5, 21.0, 2)];
    assert_eq!(
        voice_ref::plan_reference(&[], &crowded, &|_, _| 30, &full, 1),
        voice_ref::ReferencePlan::Unavailable(voice_ref::NO_CLEAN_STRETCH.to_string())
    );
    // The same stretch with the other speaker further away than the pad IS clean; density decides
    // which candidate is kept, which is why the word counter is passed in rather than guessed from
    // the length. A stretch under the density floor (`refMinWordsPerSecond` × its seconds) is dropped
    // when anything denser competes — and kept when it is all the diarization offers, because §6 drops a
    // thin take "unless that leaves none" and audio beats a refusal.
    let clean = [(0.0, 10.0, 1_u32), (20.0, 30.0, 2)];
    assert_eq!(
        voice_ref::plan_reference(&[], &clean, &|_, _| 30, &full, 1),
        voice_ref::ReferencePlan::Automatic(vec![voice_ref::Piece {
            start: 0.0,
            end: 10.0
        }])
    );
    assert_eq!(
        voice_ref::plan_reference(&[], &clean, &|_, _| 4, &full, 1),
        voice_ref::ReferencePlan::Automatic(vec![voice_ref::Piece {
            start: 0.0,
            end: 10.0
        }]),
        "the only candidate is thin, so §6's exception keeps it rather than refusing"
    );
}

/// §6 (`Automatic ranking`): "solo turns … NARROWED TO THEIR WORDS". A diarized turn carries the room tone
/// before the first word and the tail after the last; what is cloned is the words, so the piece handed to
/// ffmpeg is the transcript's own span, and the length the density floor divides by is that span too.
#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s5_a_solo_turn_is_narrowed_to_its_words(
) {
    // The diarizer claims 0–20 s; the words actually run 4–14 s.
    let turns = [(0.0, 20.0, 1_u32), (40.0, 50.0, 2)];
    let narrowed = voice_ref::plan_reference(&[], &turns, &|_, _| 40, &|_, _| Some((4.0, 14.0)), 1);
    assert_eq!(
        narrowed,
        voice_ref::ReferencePlan::Automatic(vec![voice_ref::Piece {
            start: 4.0,
            end: 14.0
        }]),
        "the cut follows the words, not the diarizer's padded turn"
    );
    // The floor is measured on the narrowed span too: the same turn passes at 6 spoken seconds and fails
    // when its words only fill 4 of them, even though the diarized turn was 20 s long.
    let short_words =
        voice_ref::plan_reference(&[], &turns, &|_, _| 40, &|_, _| Some((4.0, 8.0)), 1);
    assert_eq!(
        short_words,
        voice_ref::ReferencePlan::Unavailable(voice_ref::NO_CLEAN_STRETCH.to_string()),
        "a turn whose WORDS are under P.eng.refMinTakeSeconds is not rescued by the diarizer's padding"
    );
    // And a turn with no words in it at all is the diarizer guessing over noise: no span, no candidate.
    let silent = voice_ref::plan_reference(&[], &turns, &|_, _| 0, &|_, _| None, 1);
    assert_eq!(
        silent,
        voice_ref::ReferencePlan::Unavailable(voice_ref::NO_CLEAN_STRETCH.to_string()),
        "nothing said means nothing to clone"
    );
    // Density is checked against the narrowed span as well: 10 spoken seconds need 15 words.
    let thin = voice_ref::plan_reference(&[], &turns, &|_, _| 12, &|_, _| Some((4.0, 14.0)), 1);
    assert_eq!(
        thin,
        voice_ref::ReferencePlan::Automatic(vec![voice_ref::Piece {
            start: 4.0,
            end: 14.0
        }]),
        "§6's exception keeps the only candidate even at 1.2 words per second"
    );
}

/// §6 (`Automatic ranking`): the drop of a thin take is conditional — "dropped UNLESS THAT LEAVES NONE".
/// A session whose only solo stretches are sparse still gets a reference; one with no solo stretch at all
/// still hears §F4.6 S2's refusal.
#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s5_a_thin_take_is_kept_when_nothing_else_remains(
) {
    let full = |from: f64, until: f64| Some((from, until));
    // Two solo stretches: one dense, one thin. The thin one loses and is NOT offered.
    let turns = [(0.0, 10.0, 1_u32), (20.0, 30.0, 1)];
    let mixed = voice_ref::plan_reference(
        &[],
        &turns,
        &|from, _| if from < 1.0 { 30 } else { 5 },
        &full,
        1,
    );
    let voice_ref::ReferencePlan::Automatic(pieces) = mixed else {
        panic!("a dense stretch must produce a plan, got {mixed:?}");
    };
    assert_eq!(
        pieces,
        vec![voice_ref::Piece {
            start: 0.0,
            end: 10.0
        }],
        "the dense stretch is taken and the thin one dropped: {pieces:?}"
    );
    // Both thin: the drop would leave none, so both stand and the app gets audio instead of a refusal.
    let both_thin = voice_ref::plan_reference(&[], &turns, &|_, _| 5, &full, 1);
    let voice_ref::ReferencePlan::Automatic(pieces) = both_thin else {
        panic!("§6's exception must build a reference from the thin takes, got {both_thin:?}");
    };
    assert_eq!(
        pieces,
        vec![
            voice_ref::Piece {
                start: 0.0,
                end: 10.0
            },
            voice_ref::Piece {
                start: 20.0,
                end: 30.0
            }
        ],
        "still ranked and still capped, just not dropped: {pieces:?}"
    );
    // No solo stretch anywhere: the refusal stands, thin or not.
    let crowded = [(0.0, 10.0, 1_u32), (11.0, 21.0, 2)];
    assert_eq!(
        voice_ref::plan_reference(&[], &crowded, &|_, _| 5, &full, 1),
        voice_ref::ReferencePlan::Unavailable(voice_ref::NO_CLEAN_STRETCH.to_string()),
        "a neighbour inside the pad is not made usable by the exception"
    );
}

// ---- s6: "Add file…" — the name is the voice id, and it never overwrites -----------

// ---- s7: the take band's messages, and what its takes are keyed by ----------------

// ---- s8: the preview's hold bound, the pause past the end, a line alone -------------
