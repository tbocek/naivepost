//! F1.11 — two touching words that meet in a hiss part inside it. 09-account "Tokens signiert": the two s were said as
//! one 215 ms hiss with no quieter moment in it, and the loudness rule cut after it, at the "i", so the kept clip
//! ended on the s of the dropped "signiert". The sound here is made to that shape: "-ken" a tone, the hiss noise
//! from 0.295 to 0.510, "i" a tone, a closure, more voice; the file starts 10 s into the session.

use naivepost::edges::{place_edges, AlignedWord, Edges};
use naivepost::textfmt::Retake;

use crate::common;
use common::word;

const RATE: usize = 16_000;
const OFF: f64 = 10.0;

fn tone(x: &mut Vec<i16>, secs: f64, hz: f64, amp: f64) {
    let start = x.len();
    for n in 0..(secs * RATE as f64).round() as usize {
        let t = (start + n) as f64 / RATE as f64;
        x.push((amp * (2.0 * std::f64::consts::PI * hz * t).sin()) as i16);
    }
}

fn hiss(x: &mut Vec<i16>, secs: f64, amp: i32) {
    let mut seed: u32 = 0x1234_5678;
    for _ in 0..(secs * RATE as f64).round() as usize {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        x.push((((seed >> 16) as i32 % (2 * amp + 1)) - amp) as i16);
    }
}

fn write_wav(path: &std::path::Path, x: &[i16]) {
    let data: Vec<u8> = x.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(RATE as u32).to_le_bytes());
    out.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out).unwrap();
}

/// The envelope the band and the loudness rule read, and the same recording's sound beside it (or not).
fn tokens_signiert(with_voice: bool) -> Edges {
    let mut x = Vec::new();
    tone(&mut x, 0.290, 200.0, 4000.0);
    tone(&mut x, 0.005, 200.0, 500.0);
    hiss(&mut x, 0.215, 2500);
    tone(&mut x, 0.045, 250.0, 4000.0);
    tone(&mut x, 0.100, 150.0, 100.0);
    tone(&mut x, 0.540, 180.0, 2500.0);
    let wave = naivepost::wave::Wave {
        hz: 200.0,
        chans: vec![naivepost::wave_build::peaks_from_s16(&x)],
    };
    let edges = Edges::new(wave, OFF);
    if !with_voice {
        return edges;
    }
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = common::temp_dir_created(&format!("edge-hiss-{n}"));
    let path = dir.join("voice16k.wav");
    write_wav(&path, &x);
    edges.with_voice(path)
}

fn mark(s: f64, to: f64) -> Retake {
    Retake { s, e: to, again: 0.0, to, text: String::new(), whole: String::new() }
}

fn placed(edges: &Edges, words: &[AlignedWord], m: Retake) -> (Retake, Vec<String>) {
    let (mut marks, notes) = place_edges(vec![m], words, |_| Some(edges), &[]);
    (marks.remove(0), notes)
}

#[test]
fn f1_11_the_hiss_is_found_where_the_sound_crosses_zero_fast() {
    let (a, b) = tokens_signiert(true).hiss(OFF + 0.49).expect("a hiss at the boundary");
    assert!((a - (OFF + 0.295)).abs() <= 0.006 && (b - (OFF + 0.510)).abs() <= 0.006, "{a:.3}..{b:.3}");
    assert_eq!(tokens_signiert(true).hiss(OFF + 0.15), None, "inside a vowel there is none");
    assert_eq!(tokens_signiert(false).hiss(OFF + 0.49), None, "and without the sound file nothing is guessed");
}

#[test]
fn f1_11_a_shared_hiss_is_cut_in_its_middle() {
    let words = vec![word("tokens", OFF, OFF + 0.49), word("signiert", OFF + 0.49, OFF + 1.13)];
    let (cut, notes) = placed(&tokens_signiert(true), &words, mark(OFF + 0.49, OFF + 1.13));
    assert!((cut.s - (OFF + 0.4025)).abs() <= 0.006, "half the hiss stays with \"Tokens\": {:.4}", cut.s);
    assert!(notes.iter().any(|n| n.contains("in the hiss between them")), "{notes:?}");
    // Without the sound file the loudness alone places it, as before.
    let (_, loud) = placed(&tokens_signiert(false), &words, mark(OFF + 0.49, OFF + 1.13));
    assert!(!loud.iter().any(|n| n.contains("hiss")), "{loud:?}");
}

#[test]
fn f1_11_a_dropped_words_own_hiss_is_left_out_whole() {
    let words = vec![word("der", OFF, OFF + 0.49), word("signiert", OFF + 0.49, OFF + 1.13)];
    let (cut, _) = placed(&tokens_signiert(true), &words, mark(OFF + 0.49, OFF + 1.13));
    assert!((cut.s - (OFF + 0.295)).abs() <= 0.006, "the clip ends before the s: {:.4}", cut.s);
}

#[test]
fn f1_11_a_kept_words_own_hiss_comes_in_whole() {
    // "einige" is dropped, "signiert" kept, and the aligner put the boundary inside the s.
    let words = vec![
        word("vor", OFF - 1.0, OFF - 0.5),
        word("einige", OFF - 0.5, OFF + 0.40),
        word("signiert", OFF + 0.40, OFF + 1.13),
    ];
    let (cut, _) = placed(&tokens_signiert(true), &words, mark(OFF - 0.5, OFF + 0.40));
    assert!((cut.to - (OFF + 0.295)).abs() <= 0.006, "the clip resumes at the start of the s: {:.4}", cut.to);
}

#[test]
fn f1_11_an_edge_already_clear_of_a_dropped_hiss_stays_clear() {
    // The boundary before the hiss starts: moving to the hiss would take in what lies between.
    let words = vec![word("ab", OFF, OFF + 0.25), word("spaetestens", OFF + 0.25, OFF + 1.13)];
    let (cut, _) = placed(&tokens_signiert(true), &words, mark(OFF + 0.25, OFF + 1.13));
    assert!(cut.s <= OFF + 0.25 + 1e-9, "never later than the aligner's boundary: {:.4}", cut.s);
}
