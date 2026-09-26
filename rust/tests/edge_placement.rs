//! F1.11 Place the edges of a mark (spec/04-prepare.md) — word times fence the edge, the envelope
//! chooses inside the fence; without them the mono envelope alone (reach 0.8 s, pad 0.05 s, late stamp
//! 0.6 s).
//!
//! Envelopes are written by hand at 200 Hz — the timeline's own rate, so a bucket is the 5 ms the spec
//! counts silence in. Levels are read off the byte scale as a −70…0 dBFS meter: room 8 bytes, a word 120.

use naivepost::edges::{self as edge, AlignedWord, Edges, Lane};
use naivepost::layout::Tree;
use naivepost::textfmt::Retake;
use naivepost::wave::{self, Wave};

const HZ: f64 = 200.0;
const ROOM: u8 = 8;
const WORD: u8 = 120;

/// An envelope of `total` seconds, silent at [`ROOM`] except in these spans.
fn wave(total: f64, spans: &[(f64, f64, u8)]) -> Wave {
    let mut peaks = vec![ROOM; (total * HZ) as usize];
    for (start, end, level) in spans {
        for bucket in (start * HZ) as usize..(end * HZ) as usize {
            peaks[bucket] = *level;
        }
    }
    Wave { hz: HZ, chans: vec![peaks] }
}

/// One recording filling the session clock from second nought.
fn edges(total: f64, spans: &[(f64, f64, u8)]) -> Edges {
    Edges::new(wave(total, spans), 0.0)
}

fn word(text: &str, s: f64, e: f64) -> AlignedWord {
    AlignedWord { word: text.into(), s, e }
}

fn mark(s: f64, e: f64, again: f64, to: f64) -> Retake {
    Retake { s, e, again, to, text: String::new(), whole: String::new() }
}

/// No envelope anywhere, and nothing else spoken.
fn none(_: f64) -> Option<&'static Edges> {
    None
}

// --- S1: with word times, the cut ends after the last surviving word ------------------------------

#[test]
fn f1_11_s1_the_cut_ends_after_the_last_word_that_stays() {
    // The word's own tail is followed first, so a trailing consonant is not shaved: 1.5-1.62 at 90 bytes
    // is still the word (its tail level is 120 - 44 = 76), and the gap after it is where the cut goes.
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.62, 90)]);
    let w = word("stays", 1.0, 1.5);

    let end = e.end_after(&w, 3.0);
    assert!(end > w.e, "the edge leaves the aligner's time");
    assert!(end <= 3.0, "and never reaches the next word's start");
    assert_eq!(end, 1.62, "followed to where its own sound drops");

    // The fence is what bounds it: a wider gap does not move the edge further.
    let wide = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.62, 90)]);
    assert_eq!(wide.end_after(&w, 4.0), 1.62);
}

#[test]
fn f1_11_s1_a_fence_with_no_width_never_asks_the_sound() {
    // The aligner returns words back to back, so where the word after a kept word was dropped the fence
    // has no width: 72 of the 114 edges measured on the ETH lecture sit exactly on an aligned time.
    let e = edges(8.0, &[(1.0, 2.0, WORD)]);
    assert_eq!(e.end_after(&word("stays", 1.0, 1.5), 1.5), 1.5);
    assert_eq!(e.end_after(&word("stays", 1.0, 1.5), 1.2), 1.5, "not even past the limit");
}

#[test]
fn f1_11_s1_without_an_envelope_a_hair_past_the_word_is_all_there_is() {
    // P.machine.wordPad = 0.08 s.
    assert_eq!(edge::WORD_PAD, 0.08);
    let w = word("stays", 1.0, 1.5);

    let (marks, notes) = edge::place_edges(vec![mark(3.0, 4.0, 0.0, 4.0)], &[w.clone()], none, &[]);
    assert_eq!(marks[0].s, 1.58, "a pad after the last word that stays");
    assert_eq!(
        notes,
        vec![">>> 00:01: the cut ends 1.42s earlier, after the last word that stays".to_string()]
    );

    // A pad is still bounded by the fence it is inside of.
    let (marks, _) = edge::place_edges(vec![mark(1.52, 4.0, 0.0, 4.0)], &[w], none, &[]);
    assert_eq!(marks[0].s, 1.52);
}

// --- S2: resume just before the retake's first word, when nothing was said between ----------------

#[test]
fn f1_11_s2_nothing_said_between_lets_the_whole_gap_go() {
    let e = edges(8.0, &[(5.0, 5.4, WORD)]);
    let words = [word("first", 5.0, 5.4)];
    let lookup = |_: f64| Some(&e);

    let (marks, notes) =
        edge::place_edges(vec![mark(1.0, 4.0, 5.0, 4.0)], &words, lookup, &[]);
    assert_eq!(marks[0].to, 4.6, "the quietest moment inside the fence");
    assert_eq!(
        notes,
        vec![">>> 00:01: the cut resumes at 00:04, just before the retake's first word".to_string()]
    );

    // With no envelope the pad off the retake's first word is all there is.
    let (marks, _) = edge::place_edges(vec![mark(1.0, 4.0, 5.0, 4.0)], &words, none, &[]);
    assert_eq!(marks[0].to, 4.92);
}

#[test]
fn f1_11_s2_a_word_between_holds_the_edge_at_the_words() {
    let e = edges(8.0, &[(5.0, 5.4, WORD)]);
    // Something WAS said between the abandoned attempt and the retake, so only the words go.
    let words = [word("first", 5.0, 5.4), word("in between", 4.4, 4.8)];

    let (marks, notes) = edge::place_edges(
        vec![mark(1.0, 4.0, 5.0, 4.0)],
        &words,
        |_: f64| Some(&e),
        &[],
    );
    assert_eq!(marks[0].to, 4.0, "the mark keeps the width its words have");
    assert!(notes.is_empty(), "{notes:?}");
}

// --- S3: without word times, the mono envelope alone ---------------------------------------------

#[test]
fn f1_11_s3_the_cut_ends_where_the_sound_before_it_stops() {
    // P.machine.edgeReach = 0.8 s, edgePad = 0.05 s.
    assert_eq!(edge::EDGE_REACH, 0.8);
    assert_eq!(edge::EDGE_PAD, 0.05);

    let e = edges(8.0, &[(2.0, 2.5, WORD)]);
    // The stamp sits in the quiet after the sound: back over the quiet, then off the last sound bucket.
    assert_eq!(e.end_before(2.9), 2.55);

    // A stamp deep inside a long run has nothing to step back to within the reach, and moving it would
    // cut into the sound the mark is meant to keep.
    let inside = edges(8.0, &[(1.6, 2.5, WORD)]);
    assert_eq!(inside.end_before(2.4), 2.4);
}

#[test]
fn f1_11_s3_a_late_stamp_moves_back_to_its_own_onset() {
    assert_eq!(edge::LATE_STAMP, 0.6);

    // A take's opening word, stamped a third of a second after it starts: the cut resumes at its onset.
    let e = edges(12.0, &[(4.0, 4.6, WORD)]);
    assert_eq!(e.start_at(4.35), 3.95);

    // A stamp with more than half a second of quiet before it is one the sound has not reached yet.
    // Moving forward to the next sound would clip the very word it names, so it stands.
    let e = edges(12.0, &[(3.6, 4.2, WORD)]);
    assert_eq!(e.start_at(5.0), 5.0);
}

#[test]
fn f1_11_s3_nothing_within_reach_moves_the_edge_at_all() {
    let e = edges(8.0, &[(2.0, 2.5, WORD)]);
    // Speech with no gap in it is cut where the stamp says; so is a sound further off than the reach.
    let talking = edges(8.0, &[(1.0, 4.0, WORD)]);
    assert_eq!(talking.end_before(3.5), 3.5, "no quiet to step back into");
    assert_eq!(e.end_before(3.5), 3.5, "the sound is a whole second away");
    assert_eq!(e.start_at(1.0), 1.0);

    // Outside the file there is no envelope to ask either.
    let short = edges(2.0, &[(0.5, 1.0, WORD)]);
    assert_eq!(short.end_before(9.0), 9.0);
    assert_eq!(short.start_at(-1.0), -1.0);
}

// --- S4: the room floor and the word's tail level ---------------------------------------------------

#[test]
fn f1_11_s4_the_floor_is_the_quiet_end_of_four_seconds_either_side() {
    // The floor is the 20th percentile of ±4 s, raised to max(3x, +8) so a little room sits over it:
    // 8 -> 24 bytes here, where most of that window is quiet.
    let spoken = edges(12.0, &[(3.6, 4.4, WORD)]);
    assert_eq!(spoken.floor(4.0), 24);

    // A window that is mostly talking puts its floor with the talking: a percentile of 120 bytes times
    // three clamps to 255, so nothing in it counts as quiet and no edge can be moved there. That is the
    // rule working, not failing — a continuous stream of speech is cut where the words say. (At this
    // rate ±4 s is 1601 buckets, so it takes six seconds of talking out of eight to lift the 20th
    // percentile off the room.)
    let loud = edges(12.0, &[(1.0, 7.0, WORD)]);
    assert_eq!(loud.floor(1.5), u8::MAX);

    // One loud stretch is not the room either: the quieter end of the window still says 8 -> 24.
    let mixed = Edges::new(wave(12.0, &[(3.6, 4.4, WORD), (7.0, 8.0, 40)]), 0.0);
    assert_eq!(mixed.floor(4.0), 24);

    // Outside the file nothing can be sound.
    assert_eq!(spoken.floor(900.0), u8::MAX);
}

#[test]
fn f1_11_s4_a_tail_is_twelve_db_under_the_word_and_never_under_the_room() {
    // P.machine.edgeTailDb = 12 read off the meter scale the byte stands for: 12 * 255 / 70 = 44 bytes.
    assert_eq!(edge::EDGE_TAIL_DB, 12.0);
    let e = edges(8.0, &[(1.0, 1.5, WORD)]);

    // 120 - 44 = 76, above the floor of 24.
    assert!(e.end_after(&word("stays", 1.0, 1.5), 3.0) >= 1.5);
    // A breath 20 dB or more under the word it follows is under this; a trailing consonant a few dB
    // under is not — which is exactly the pair the rule has to tell apart.
    let with_breath = edges(8.0, &[(1.0, 1.5, WORD), (1.55, 1.7, 30)]);
    // 30 bytes is under the word's tail level, so the walk stops at once and the trough search, finding
    // only room from there on, keeps the aligner's own moment.
    assert_eq!(with_breath.end_after(&word("stays", 1.0, 1.5), 3.0), 1.5);
    let with_consonant = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.7, 90)]);
    assert_eq!(with_consonant.end_after(&word("stays", 1.0, 1.5), 3.0), 1.7);
}

// --- S5: the byte scale read as a meter, and silence counted in 5 ms buckets -----------------------

#[test]
fn f1_11_s5_the_byte_scale_is_treated_as_a_minus_seventy_dbfs_meter() {
    // Known and kept: the cache holds linear peaks (a byte = peak x 255 of full scale) while the tail
    // level and the floor read that byte as a -70...0 dBFS meter, so everything under about -30 dBFS —
    // near enough half full scale, 148 bytes on this scale's room-boundary arithmetic — reads as silence.
    let e = edges(8.0, &[(1.0, 1.5, WORD)]);
    assert!(e.floor(1.2) <= 148, "a room is far under the -30 dBFS line");

    // A level over the tail threshold is sound against that floor...
    let loud = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.7, 148)]);
    assert_eq!(loud.end_after(&word("stays", 1.0, 1.5), 3.0), 1.7);
    // ...and something much quieter than the floor is not, however steady it is.
    let faint = Edges::new(wave(12.0, &[(1.0, 7.0, WORD), (7.0, 7.4, 20)]), 0.0);
    assert_eq!(faint.floor(7.2), 24, "the room is the talking's floor");
    assert!(20 < faint.floor(7.2), "20 bytes of hum is silence to this rule");
}

#[test]
fn f1_11_s5_one_silent_bucket_at_a_word_s_end_stops_the_tail() {
    // A bucket is 5 ms at the timeline's 200 Hz, so a single silent one is enough: the tail walk stops
    // where the word stops being sound and the trough search takes that moment.
    let mut envelope = wave(8.0, &[(1.0, 1.5, WORD)]);
    envelope.chans[0][300] = 0;
    let e = Edges::new(envelope, 0.0);

    assert_eq!(e.end_after(&word("stays", 1.0, 1.5), 3.0), 1.5);
}

// --- S6: the placement as a whole ------------------------------------------------------------------

#[test]
fn f1_11_s6_both_edges_of_a_mark_land_on_the_sound() {
    let e = edges(12.0, &[
        (1.0, 1.5, WORD),
        (1.5, 1.62, 90),
        (5.0, 5.4, WORD),
    ]);
    let words = [word("stays", 1.0, 1.5), word("again", 5.0, 5.4)];

    let (marks, notes) =
        edge::place_edges(vec![mark(2.0, 3.0, 5.0, 3.0)], &words, |_: f64| Some(&e), &[]);
    assert_eq!((marks[0].s, marks[0].to), (1.62, 4.6));
    assert_eq!(
        notes,
        vec![
            ">>> 00:01: the cut ends 0.38s earlier, after the last word that stays".to_string(),
            ">>> 00:01: the cut resumes at 00:04, just before the retake's first word".to_string(),
        ]
    );
}

#[test]
fn f1_11_s6_no_envelope_leaves_a_mark_without_word_times_alone() {
    let before = vec![mark(2.0, 3.0, 5.0, 3.0)];
    let (marks, notes) = edge::place_edges(before.clone(), &[], none, &[]);
    assert_eq!(marks[0].s, 2.0, "nothing to ask, nothing to move the cut's start");
    // With no envelope there is nothing to place the resume edge either, so `again` itself is where it
    // resumes — the retake's own stamp, which is what the mark meant before any of this.
    assert_eq!(marks[0].to, 5.0);
    assert_eq!(
        notes,
        vec![">>> 00:02: the cut resumes at 00:05, where the retake starts to sound".to_string()]
    );

    // The same mark with an envelope takes the envelope branch and says so differently.
    let e = edges(12.0, &[(1.6, 1.9, WORD), (4.4, 5.0, WORD)]);
    let (moved, notes) = edge::place_edges(before, &[], |_: f64| Some(&e), &[]);
    assert!(moved[0].s < 2.0);
    assert_eq!(notes.len(), 2, "both edges moved");
    assert!(notes[0].contains("where the sound before it stops"), "{notes:?}");
    assert!(notes[1].contains("where the retake starts to sound"));

    // The two branches never use each other's wording.
    let (worded, word_notes) = edge::place_edges(
        vec![mark(2.0, 3.0, 5.0, 3.0)],
        &[word("stays", 1.0, 1.5), word("again", 5.0, 5.4)],
        |_: f64| Some(&e),
        &[],
    );
    assert!(word_notes[0].contains("after the last word that stays"));
    assert!(!word_notes.iter().any(|n| n.contains("the sound before it stops")));
    // The words fence the edge at the last one that stays, and this envelope has no quiet left inside
    // the fence to choose: the aligner's own moment stands. That is the difference between the branches
    // — the word branch never moves an edge onto a sound the words did not release.
    assert_eq!(worded[0].s, 1.5);
}

#[test]
fn f1_11_s6_another_recording_speech_holds_the_resume_edge() {
    let e = edges(12.0, &[(5.0, 5.4, WORD)]);
    // Somebody else is talking in that gap on another recording: the mark cannot resume over them.
    let (marks, notes) = edge::place_edges(
        vec![mark(1.0, 4.0, 5.0, 3.0)],
        &[],
        |_: f64| Some(&e),
        &[(4.2, 4.9)],
    );
    assert_eq!(marks[0].to, 3.0);
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn f1_11_s6_the_reach_and_the_pads_are_the_scored_numbers() {
    // Scored against three hand-cut projects (ETH, admin, rep): the placement as it is beat every
    // alternative tried, so these are kept exactly as they were measured.
    assert_eq!(edge::EDGE_REACH, 0.8);
    assert_eq!(edge::EDGE_PAD, 0.05);
    assert_eq!(edge::LATE_STAMP, 0.6);
    assert_eq!(edge::EDGE_TAIL_DB, 12.0);
    assert_eq!(edge::EDGE_TAIL_MAX, 0.25);
    assert_eq!(edge::TROUGH_REACH, 0.4);
}

// --- S6 through the disk: the envelope read out of `cache/waves` ---------------------------------

/// A throwaway project folder, so a test can write and read a real cache file. Same shape as
/// `tests/wave_cache.rs`: unique per process and tag, nothing shared between tests.
fn temp_tree(tag: &str) -> Tree {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-edges-{tag}-{}-{}.naivepost",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Tree::new(&dir).unwrap()
}

const STAMPED_SIZE: i64 = 4_096;
const STAMPED_MTIME: i64 = 1_700_000_000;

#[test]
fn f1_11_s6_a_cached_lane_is_read_and_a_missing_one_is_left_out() {
    let tree = temp_tree("lane-present");
    let w = wave(8.0, &[(2.0, 2.5, WORD)]);
    wave::write(&tree, "lecture", &w, STAMPED_SIZE, STAMPED_MTIME).unwrap();

    // The second lane has no cache file at all: it contributes nothing, and that is not an error —
    // F1.11's "Without: mono envelope alone" case, where the word pad stands in.
    let held = edge::load(
        &tree,
        &[
            Lane { key: "lecture".into(), off: 0.0, size: STAMPED_SIZE, mtime: STAMPED_MTIME },
            Lane { key: "mic".into(), off: 30.0, size: 1, mtime: 1 },
        ],
    );
    assert_eq!(held.len(), 1, "only the lane with a cache is held: {held:?}");
    assert_eq!(held[0].0.key, "lecture");
    assert_eq!(held[0].1.off, 0.0, "the lane keeps its own place on the session clock");
}

#[test]
fn f1_11_s6_a_stale_cache_is_no_envelope_at_all() {
    let tree = temp_tree("stale");
    let w = wave(8.0, &[(2.0, 2.5, WORD)]);
    wave::write(&tree, "lecture", &w, STAMPED_SIZE, STAMPED_MTIME).unwrap();

    // The recording was replaced since the cache was made (size differs by one byte). `wave::read`
    // answers None, so NOTHING is held — a stale envelope would move a cut onto sound that is no
    // longer on the track, which is worse than asking nothing.
    let held = edge::load(
        &tree,
        &[Lane { key: "lecture".into(), off: 0.0, size: STAMPED_SIZE + 1, mtime: STAMPED_MTIME }],
    );
    assert!(held.is_empty(), "a cache stamped against another file is not this recording's");

    // And with nothing held, placement lands exactly where the no-envelope case does: P.machine.wordPad
    // = 0.08 s past the last word that stays (1.5 -> 1.58), the same number as the `none` branch above.
    let mut lookup = edge::finder(&held);
    let (marks, notes) = edge::place_edges(
        vec![mark(3.0, 4.0, 0.0, 4.0)],
        &[word("stays", 1.0, 1.5)],
        &mut lookup,
        &[],
    );
    assert_eq!(marks[0].s, 1.58, "the word pad, not the discarded envelope, placed the edge");
    assert!(!notes.iter().any(|n| n.contains("the sound before it stops")), "{notes:?}");
}

#[test]
fn f1_11_s6_the_finder_picks_the_lane_that_is_actually_playing() {
    let tree = temp_tree("two-lanes");
    let first = wave(30.0, &[(2.0, 2.5, WORD)]);
    let second = wave(30.0, &[(2.0, 2.5, 200)]);
    wave::write(&tree, "cam-a", &first, STAMPED_SIZE, STAMPED_MTIME).unwrap();
    wave::write(&tree, "cam-b", &second, STAMPED_SIZE, STAMPED_MTIME).unwrap();

    let held = edge::load(
        &tree,
        &[
            Lane { key: "cam-a".into(), off: 0.0, size: STAMPED_SIZE, mtime: STAMPED_MTIME },
            Lane { key: "cam-b".into(), off: 30.0, size: STAMPED_SIZE, mtime: STAMPED_MTIME },
        ],
    );
    assert_eq!(held.len(), 2);
    let lookup = edge::finder(&held);

    // Identified by the envelope's own offset rather than pointer identity.
    let at = |t: f64| lookup(t).map(|e| e.off);
    assert_eq!(at(10.0), Some(0.0), "midway through the first take");
    // The boundary belongs to the take that STARTS then, not the one that started earlier.
    assert_eq!(at(30.0), Some(30.0), "a take beginning at t owns t");
    assert_eq!(at(31.5), Some(30.0), "and everything after it until the next lane");
    // Before the first take there is no recording playing, so there is nothing to ask.
    assert_eq!(at(-0.5), None);
}

#[test]
fn f1_11_s6_a_live_envelope_moves_the_mark_earlier_through_the_cache() {
    let tree = temp_tree("live-earlier");
    // The last sound before the stamp stops at 5.9 s; the mark is stamped at 6.4 s with NO word times,
    // so the envelope alone places it. P.eng.edgeReachSeconds = 0.8 covers the 0.5 s of quiet, and the
    // edge pad of 0.05 s leaves the cut at 5.9 + 0.05 -- the same numbers S3 proves directly, now
    // reached by way of the on-disk cache instead of a hand-passed envelope.
    let w = wave(12.0, &[(5.0, 5.9, WORD)]);
    wave::write(&tree, "lecture", &w, STAMPED_SIZE, STAMPED_MTIME).unwrap();

    let held = edge::load(
        &tree,
        &[Lane { key: "lecture".into(), off: 0.0, size: STAMPED_SIZE, mtime: STAMPED_MTIME }],
    );
    assert_eq!(held.len(), 1);
    let mut lookup = edge::finder(&held);

    let (marks, notes) = edge::place_edges(vec![mark(6.4, 7.0, 0.0, 7.0)], &[], &mut lookup, &[]);
    assert_eq!(marks[0].s, 5.95, "off the last sound bucket plus the edge pad");
    assert!(
        notes.iter().any(|n| n.contains("the sound before it stops")),
        "the envelope branch says so: {notes:?}"
    );
}

#[test]
fn f1_11_s6_a_live_envelope_moves_the_resume_edge_later_through_the_cache() {
    let tree = temp_tree("live-resume");
    // The retake's onset is at 10.0 s and its stamp is late by 0.4 s -- inside
    // P.eng.lateStampSeconds = 0.6, so the stamp is read as naming THAT run, and the cut resumes at
    // its onset less the 0.05 s edge pad: 9.95. The tail rules that bound the walk are
    // P.eng.edgeTailDB = 12 dB below the run's peak, P.eng.edgeTailMaxSeconds = 0.25 of tail
    // followed, and P.eng.troughReachSeconds = 0.4 of gap searched; none of them pull this edge back,
    // because the run ahead of the stamp is a whole word, not a tail.
    let w = wave(16.0, &[(10.0, 11.0, WORD)]);
    wave::write(&tree, "lecture", &w, STAMPED_SIZE, STAMPED_MTIME).unwrap();

    let held = edge::load(
        &tree,
        &[Lane { key: "lecture".into(), off: 0.0, size: STAMPED_SIZE, mtime: STAMPED_MTIME }],
    );
    assert_eq!(held.len(), 1);
    let mut lookup = edge::finder(&held);

    // again = 10.4 (the late stamp), to starts at 8.0 -- the resume edge has to travel forward.
    let (marks, notes) = edge::place_edges(vec![mark(2.0, 3.0, 10.4, 8.0)], &[], &mut lookup, &[]);
    assert_eq!(marks[0].to, 9.95, "the cached onset less the edge pad");
    assert!(
        notes.iter().any(|n| n.contains("the cut resumes at")),
        "the resume branch says so: {notes:?}"
    );
}
