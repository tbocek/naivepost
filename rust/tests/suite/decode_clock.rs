//! F1.3 S1 — `voice16k.wav` is decoded on the recording's own clock: a sound track that starts after the picture keeps
//! that lead as silence, so word times, cut edges and envelopes are on the second Produce and ▶ seek to. First rewrite:
//! OBS writes its sound 22 ms after the picture and everything sat 21 ms ahead of what the render cut (09-account).
//! An older decode is moved over once: decoded again, and every time made from it moved later by the lead.

use std::process::Command;

use crate::common;
use naivepost::decode_audio;
use naivepost::requests::{self, AlignedDoc, Chunk, Turn, Word, WordsDoc};
use naivepost::textfmt::{self, Line};

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
}

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(args)
        .output()
        .expect("ffmpeg");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A recording as OBS writes it: picture from 0, its sound from 22 ms.
fn obs_like(dir: &std::path::Path) -> std::path::PathBuf {
    let video = dir.join("v.mkv");
    let tone = dir.join("t.wav");
    let out = dir.join("2026-11-05 11-11-47.mkv");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=gray:s=160x90:r=30:d=4",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        video.to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=300:sample_rate=48000:duration=4",
        tone.to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-i",
        video.to_str().unwrap(),
        "-itsoffset",
        "0.022",
        "-i",
        tone.to_str().unwrap(),
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        out.to_str().unwrap(),
    ]);
    out
}

fn samples(path: &std::path::Path) -> usize {
    naivepost::wave_build::read_wav_samples(path)
        .expect("a wav")
        .len()
}

#[test]
fn f1_3_s1_the_decode_keeps_a_late_sound_tracks_lead() {
    if !have_ffmpeg() {
        eprintln!("no ffmpeg here: not checked");
        return;
    }
    let (root, tree) = common::tree_in("decode-clock-new");
    let source = obs_like(&root);
    let lane = "2026-11-05 11-11-47";
    let out = decode_audio::ensure(&tree, lane, &source, "", &mut |_| {}).expect("decoded");
    // The sound's 22 ms lead is silence in front: its first sound is where the file plays it, 0.022 s in.
    let x = naivepost::wave_build::read_wav_samples(&out).unwrap();
    let first = x
        .iter()
        .position(|s| s.unsigned_abs() > 50)
        .expect("there is sound");
    assert!(
        (320..=400).contains(&first),
        "the first sound at sample {first}, about 0.022 s on the file's clock"
    );
    assert!(
        decode_audio::clock_mark(&tree, lane).exists(),
        "and the decode says which clock it is on"
    );
}

#[test]
fn f1_3_s1_an_older_decode_is_moved_to_the_recordings_clock_once() {
    if !have_ffmpeg() {
        eprintln!("no ffmpeg here: not checked");
        return;
    }
    let (root, tree) = common::tree_in("decode-clock-move");
    let source = obs_like(&root);
    let lane = "2026-11-05 11-11-47";
    let old = tree.voice16k_wav(lane);
    std::fs::create_dir_all(old.parent().unwrap()).unwrap();
    // The decode as it used to be made: no lead.
    ffmpeg(&[
        "-i",
        source.to_str().unwrap(),
        "-vn",
        "-ar",
        "16000",
        "-ac",
        "1",
        "-c:a",
        "pcm_s16le",
        old.to_str().unwrap(),
    ]);
    let before = samples(&old);
    let w = |word: &str, s: u64, e: u64| Word {
        word: word.into(),
        start_sample: s,
        end_sample: e,
    };
    requests::write_words(
        &tree,
        lane,
        &WordsDoc {
            text: "eins zwei".into(),
            words: vec![w("eins", 1600, 8000)],
        },
    )
    .unwrap();
    requests::write_aligned(
        &tree,
        lane,
        &AlignedDoc {
            words: vec![w("eins", 1600, 8000), w("zwei", 8000, 16000)],
        },
    )
    .unwrap();
    requests::write_turns(
        &tree,
        lane,
        &[Turn {
            start_sample: 1600,
            end_sample: 16000,
            speaker_id: 0,
        }],
    )
    .unwrap();
    requests::write_chunks(
        &tree,
        lane,
        &[
            Chunk {
                s: 0.0,
                e: 2.0,
                text: "eins".into(),
            },
            Chunk {
                s: 2.0,
                e: 4.0,
                text: "zwei".into(),
            },
        ],
    )
    .unwrap();
    let fixed = tree.transcript_fixed_tsv(lane);
    std::fs::create_dir_all(fixed.parent().unwrap()).unwrap();
    textfmt::write_lines(
        &[Line {
            start: 0.1,
            end: 1.0,
            speaker: "SPEAKER_00".into(),
            text: "eins zwei".into(),
        }],
        &fixed,
    )
    .unwrap();
    std::fs::create_dir_all(tree.wave(lane).parent().unwrap()).unwrap();
    std::fs::write(tree.wave(lane), b"an envelope of the old decode").unwrap();

    let mut said = Vec::new();
    decode_audio::ensure(&tree, lane, &source, "", &mut |line| {
        said.push(line.to_string())
    })
    .expect("moved over");
    let lead = samples(&old) - before;
    assert!(
        (300..=400).contains(&lead),
        "the new decode is the old one behind the sound's lead: {lead} samples"
    );
    let lead = lead as u64;
    assert!(
        said.iter()
            .any(|l| l.contains("decoded again on the recording's own clock")),
        "{said:?}"
    );
    let aligned = requests::read_aligned(&tree, lane).unwrap().unwrap().words;
    assert_eq!(
        (aligned[0].start_sample, aligned[1].end_sample),
        (1600 + lead, 16000 + lead)
    );
    assert_eq!(
        requests::read_words(&tree, lane).unwrap().unwrap().words[0].start_sample,
        1600 + lead
    );
    assert_eq!(
        requests::read_turns(&tree, lane).unwrap()[0].start_sample,
        1600 + lead
    );
    let chunks = requests::read_chunks(&tree, lane).unwrap();
    let secs = lead as f64 / 16000.0;
    assert_eq!(
        chunks[0].s, 0.0,
        "the first piece still starts at the file's first sample"
    );
    assert!((chunks[1].s - (2.0 + secs)).abs() < 1e-9 && (chunks[1].e - (4.0 + secs)).abs() < 1e-9);
    let lines = textfmt::read_lines(&fixed).unwrap();
    assert!(
        (lines[0].start - (0.1 + secs)).abs() <= 0.005,
        "{}",
        lines[0].start
    );
    assert!(
        !tree.wave(lane).exists(),
        "the old envelope goes, to be measured again"
    );
    assert!(decode_audio::clock_mark(&tree, lane).exists());

    // Once: a second pass changes nothing.
    decode_audio::ensure(&tree, lane, &source, "", &mut |_| {}).expect("already on the clock");
    assert_eq!(
        requests::read_aligned(&tree, lane).unwrap().unwrap().words[0].start_sample,
        1600 + lead
    );
}
