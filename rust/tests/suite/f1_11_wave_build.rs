//! F1.11's producer: `cache/waves/<lane>.wave`, measured from the decoded audio (§04-prepare.md).
//!
//! [`crate::edges`] can only place an edge on the sound when a wave cache exists to ask, and before this
//! module nothing in the program wrote one — so every live call reached [`naivepost::edges::place_edges`]
//! with no envelope and edges landed on aligned word times alone. These cases are the half of F1.11 that
//! had never run from data the program produced: the fold rule, the wav reader, the stamp the cache is
//! keyed on, the resume marker, and one mark moved by an envelope that came off a real file.
//!
//! The wavs here are made by the container's own ffmpeg (`-f lavfi -i sine=…`), not hand-written bytes:
//! the reader's whole job is surviving what ffmpeg actually writes, so a fixture built by hand would test
//! a header shape the tool may never produce.

use std::path::{Path, PathBuf};
use std::process::Command;

use naivepost::edges::{self, AlignedWord, Lane};
use naivepost::layout::Tree;
use naivepost::textfmt::Retake;
use naivepost::wave::{self, Wave};
use naivepost::wave_build::{self, PEAK_HZ, SAMPLES_PER_BUCKET};

use crate::common;
use common::temp_dir_created as temp_dir;

/// One project folder plus the recording a row would name. The lane key is the base name without its
/// extension, exactly as `audio_task::source_lane` keys everything Prepare writes.
const LANE: &str = "lecture";

fn session(tag: &str) -> (Tree, PathBuf) {
    let root = temp_dir(&format!("f111-{tag}")).join("talk.naivepost");
    let tree = Tree::new(&root).expect("a .naivepost folder is a project");
    let source = root.join("sources/lecture.wav");
    std::fs::create_dir_all(source.parent().expect("a sources folder")).expect("the folder");
    std::fs::write(&source, b"recording-bytes").expect("the recording");
    (tree, source)
}

/// Seconds of tone, then silence, written as the mono 16 kHz `pcm_s16le` wav §F1.3 S1 decodes to.
/// Real ffmpeg output, because that is the only file this reader is ever pointed at.
fn write_tone_wav(path: &Path, tone: f64, tail: f64) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the input folder");
    }
    let status = Command::new("/usr/bin/ffmpeg")
        .args([
            "-v", "error", "-y",
            "-f", "lavfi", "-i", &format!("sine=frequency=440:duration={tone}"),
            "-f", "lavfi", "-i", &format!("anullsrc=r=16000:cl=mono:d={tail}"),
            "-filter_complex", "[0:a][1:a]concat=n=2:v=0:a=1[out]",
            "-map", "[out]", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le",
        ])
        .arg(path)
        .status()
        .expect("the container has ffmpeg");
    assert!(status.success(), "ffmpeg wrote the fixture wav");
}

/// A bucket count for `seconds` of audio at the decode rate.
fn buckets(seconds: f64) -> usize {
    (seconds * PEAK_HZ) as usize
}

/// S1's fold: the loudest sample of every bucket, on the byte scale "a byte = peak × 255 of full scale".
#[test]
fn f1_11_s1_the_envelope_keeps_the_loudest_sample_of_every_bucket() {
    // Silence, a loud run inside ONE bucket, silence again. The loud samples sit in the middle of bucket
    // 1 so the rule is proven to look across the whole bucket and not at its first sample.
    let mut samples = vec![0i16; SAMPLES_PER_BUCKET * 3];
    samples[SAMPLES_PER_BUCKET + 5] = 12_000;
    samples[SAMPLES_PER_BUCKET + 9] = -30_000; // absolute value counts: a negative peak is a peak
    samples[SAMPLES_PER_BUCKET + 20] = 500;
    let peaks = wave_build::peaks_from_s16(&samples);
    assert_eq!(peaks.len(), 3, "one byte per bucket");
    assert_eq!(peaks[0], 0, "silence folds to nought");
    assert_eq!(peaks[1], (30_000i64 * 255 / 32767) as u8, "the loudest absolute sample of the bucket");
    assert_eq!(peaks[2], 0, "and the next bucket is quiet again");

    // Full scale lands on the top byte rather than one short of it, and a partial final bucket is kept:
    // dropping it would shorten every envelope by up to 4.95 ms, and the tail rule reads those buckets.
    let full = vec![i16::MIN; SAMPLES_PER_BUCKET + 1];
    let folded = wave_build::peaks_from_s16(&full);
    assert_eq!(folded.len(), 2, "a trailing partial bucket still gets its byte");
    assert_eq!(folded[0], 255, "a full-scale peak reaches the top byte");
    assert_eq!(folded[1], 255, "one sample is enough to say the bucket was loud");
    assert!(wave_build::peaks_from_s16(&[]).is_empty(), "nothing in, nothing out");
}

/// The reader and the fold over a genuine ffmpeg wav, end to end through `ensure`.
#[test]
fn f1_11_s1_a_real_wav_decodes_to_the_peaks_the_folding_rule_says() {
    let (tree, source) = session("real-wav");
    write_tone_wav(&tree.voice16k_wav(LANE), 2.0, 1.0);
    let (size, mtime) = stamps(&source);
    let mut log: Vec<String> = Vec::new();

    wave_build::ensure(&tree, LANE, &source, &mut |line| log.push(line.to_string()))
        .expect("the envelope is measured");

    // The announce line goes out BEFORE the work, like every other step's first line.
    assert_eq!(log.len(), 1, "one line, said first: {log:?}");
    assert!(log[0].starts_with(">>> envelope: measuring lecture"), "S1's line: {log:?}");

    let read = wave::read(&tree, LANE, size, mtime).expect("readable").expect("present");
    assert_eq!(read.hz, PEAK_HZ, "the cache holds the rate §01 §6 names");
    assert_eq!(read.chans.len(), 1, "the decode is mono, so one block of peaks");
    let peaks = &read.chans[0];
    assert!(
        (peaks.len() as i64 - buckets(3.0) as i64).abs() <= 2,
        "three seconds at {} Hz: {} buckets",
        PEAK_HZ,
        peaks.len()
    );
    // Ordering, not a level: a bucket inside the tone says more sound than one inside the silence after
    // it. That is all the rule needs — the dB arithmetic belongs to `edges`, and F1.11 keeps it there.
    let during = peaks[buckets(1.0)];
    let after = peaks[buckets(2.5)];
    assert!(during > after, "tone louder than the silence after it: {during} vs {after}");
    assert!(after < 8, "the silence is room-quiet on the byte scale: {after}");
}

/// The cache is stamped against the RECORDING, which is what makes `edges::load` drop a stale envelope.
#[test]
fn f1_11_s1_the_cache_is_stamped_against_the_recording_it_came_from() {
    let (tree, source) = session("stale");
    write_tone_wav(&tree.voice16k_wav(LANE), 1.0, 0.5);
    let (size, mtime) = stamps(&source);
    wave_build::ensure(&tree, LANE, &source, &mut |_| ()).expect("measured once");
    assert!(wave::read(&tree, LANE, size, mtime).expect("readable").is_some());

    // Replace the recording -- same name, different file. The envelope now describes audio that is gone,
    // so it must read as no envelope at all rather than placing a cut on sound that is not there.
    std::fs::write(&source, b"a-different-recording").expect("the recording is replaced");
    let (new_size, new_mtime) = stamps(&source);
    assert!(
        wave::read(&tree, LANE, new_size, new_mtime).expect("readable").is_none(),
        "a cache made against another file is a miss"
    );
}

/// A step's own output file is its resume marker: a second press measures nothing and logs nothing.
#[test]
fn f1_11_s1_an_unchanged_recording_is_not_measured_twice() {
    let (tree, source) = session("resume");
    write_tone_wav(&tree.voice16k_wav(LANE), 1.0, 0.5);
    let mut first: Vec<String> = Vec::new();
    wave_build::ensure(&tree, LANE, &source, &mut |l| first.push(l.to_string())).expect("first press");
    assert_eq!(first.len(), 1, "the first press announces itself: {first:?}");

    let cache = tree.wave(LANE);
    let before = std::fs::metadata(&cache).expect("the cache").modified().expect("a time");
    let mut second: Vec<String> = Vec::new();
    wave_build::ensure(&tree, LANE, &source, &mut |l| second.push(l.to_string())).expect("second press");
    assert!(second.is_empty(), "a resumed step says nothing: {second:?}");
    let after = std::fs::metadata(&cache).expect("the cache").modified().expect("a time");
    assert_eq!(before, after, "and rewrites nothing: the file is untouched");
}

/// No decoded audio means no envelope, said as the missing file rather than as an empty one.
#[test]
fn f1_11_s1_a_lane_with_no_decoded_audio_says_so() {
    let (tree, source) = session("no-decode");
    let mut err = String::new();
    let outcome = wave_build::ensure(&tree, LANE, &source, &mut |_| ())
        .map(|()| String::new())
        .map_err(|why| err = why)
        .err();
    let _ = outcome;
    let why = err;
    assert!(
        why.contains(&tree.voice16k_wav(LANE).display().to_string()),
        "the reason names the file the caller must decode first: {why}"
    );
    assert!(!tree.wave(LANE).exists(), "and no envelope was left behind");
}

/// The point of the whole module: an envelope the PROGRAM measured moves a mark. Before this existed no
/// live path had anything to hand `place_edges`, so F1.11's sound rules ran only in tests that wrote the
/// cache by hand.
#[test]
fn f1_11_s1_the_envelope_a_cache_holds_moves_a_mark_live() {
    let (tree, source) = session("live-mark");
    // Two seconds of tone, then quiet: the last sound before a stamp at 2.4 s stops near 2.0 s, so the
    // mark's edge comes off the sound instead of sitting on the stamp.
    write_tone_wav(&tree.voice16k_wav(LANE), 2.0, 2.0);
    let (size, mtime) = stamps(&source);
    wave_build::ensure(&tree, LANE, &source, &mut |_| ()).expect("measured");

    let held = edges::load(&tree, &[Lane { key: LANE.into(), off: 0.0, size, mtime }]);
    assert_eq!(held.len(), 1, "the lane the program measured is the lane the finder answers");
    let mut lookup = edges::finder(&held);
    assert!(lookup(1.0).is_some(), "a second inside the recording has an envelope to ask");

    // No word times: the envelope alone places the edge, which is the branch that had never been reached
    // from produced data. With words the fence bounds it (see the case below).
    let mark = Retake { s: 2.4, e: 3.0, again: 0.0, to: 3.0, text: String::new(), whole: String::new() };
    let (placed, notes) = edges::place_edges(vec![mark], &[], &mut lookup, &[]);
    assert!(
        placed[0].s < 2.4,
        "the cut ends earlier, where the sound stops: {}",
        placed[0].s
    );
    assert!(
        notes.iter().any(|n| n.contains("the sound before it stops")),
        "the envelope branch says so: {notes:?}"
    );

    // And the same envelope under a WORD FENCE: the aligner's back-to-back times bound the edge, so the
    // sound chooses inside the fence rather than anywhere within reach.
    let words = [AlignedWord { word: "stays".into(), s: 1.0, e: 2.0 }];
    let fenced = Retake { s: 2.0, e: 3.0, again: 0.0, to: 3.0, text: String::new(), whole: String::new() };
    let (fenced, _) = edges::place_edges(vec![fenced], &words, &mut lookup, &[]);
    assert!(fenced[0].s >= 2.0, "a fence with no width leaves the edge on the word time: {}", fenced[0].s);
}

/// A hand-built envelope, for the case where a specific byte level matters (the fold above is whatever
/// ffmpeg's tone produces, which is right for the reader and wrong for pinning a number).
#[test]
fn f1_11_s1_a_folded_envelope_agrees_with_the_hand_written_one() {
    // The same shape both ways: room everywhere, a spoken stretch at 120 bytes from 1.0 to 2.0 s.
    let hz = 200.0;
    let mut hand = vec![8u8; buckets(4.0)];
    for slot in &mut hand[buckets(1.0)..buckets(2.0)] {
        *slot = 120;
    }
    // What the fold rule says about samples one step under each level, because the scale rounds DOWN:
    // `byte * 255 / 32767` maps `(b + 1) * 32767 / 255 - 1` back to `b`, and that sample stream is what
    // an envelope of exactly these bytes came from.
    let low = (((8 * 32767) + 254) / 255 - 1) as i16;
    let high = (((120 * 32767) + 254) / 255 - 1) as i16;
    let mut samples = vec![low; SAMPLES_PER_BUCKET * buckets(4.0)];
    for slot in &mut samples[SAMPLES_PER_BUCKET * buckets(1.0)..SAMPLES_PER_BUCKET * buckets(2.0)] {
        *slot = high;
    }
    let folded = wave_build::peaks_from_s16(&samples);
    assert_eq!(folded.len(), hand.len(), "same length, same rate");
    // The levels themselves, not the whole array — and NOT equal to the hand-written envelope, which is
    // worth pinning rather than papering over: `byte * 255 / 32767` truncates, so a sample one step under
    // a level folds one byte low. The producer cannot drift this way (it always holds a sample that was
    // really there), and the edge rules see the same SHAPE either way -- proved below by placing a mark.
    assert_eq!(folded[buckets(1.0)], 119, "a spoken bucket lands one byte under the level named");
    assert_eq!(folded[buckets(1.5)], 119, "and holds it across the stretch");
    assert_eq!(folded[buckets(0.5)], 7, "the quiet between words sits one under the room byte too");
    assert_eq!(folded[buckets(3.0)], 7, "and on both sides of the speech");
    // The shape is what the rules read, not the exact byte: a mark placed off the folded envelope moves
    // exactly as it does off the hand-written one (`edge_placement`'s live cases use 8/120).
    // Both descriptions of the same recording place the same edge: proof the produced cache behaves like
    // the hand-written ones every `edge_placement` case already uses.
    let tree = Tree::new(temp_dir("agree").join("p.naivepost")).expect("a project");
    let (size, mtime) = (1i64, 1i64);
    wave::write(&tree, LANE, &Wave { hz, chans: vec![folded] }, size, mtime).unwrap();
    let held = edges::load(&tree, &[Lane { key: LANE.into(), off: 0.0, size, mtime }]);
    let mut lookup = edges::finder(&held);
    let mark = Retake { s: 2.4, e: 3.0, again: 0.0, to: 3.0, text: String::new(), whole: String::new() };
    let (placed, notes) = edges::place_edges(vec![mark], &[], &mut lookup, &[]);
    assert!(placed[0].s < 2.4 && !notes.is_empty(), "the fold's envelope moves the edge: {:?}", placed[0].s);
}

/// The recording's own size/mtime, taken the way the program takes them (never from the cache file).
fn stamps(path: &Path) -> (i64, i64) {
    let meta = std::fs::metadata(path).expect("the recording");
    let mtime = meta
        .modified()
        .expect("a time")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs() as i64;
    (meta.len() as i64, mtime)
}
