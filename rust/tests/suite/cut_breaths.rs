//! Suggestions — at most one breath at a join ([`naivepost::cut_breaths`]). The sounds are made here: "speech" is a
//! 150 Hz voice with its harmonics, a "breath" is noise centred near 3 kHz, 25 dB under the speech, and an "s" is
//! noise centred near 5.5 kHz. 08-dex 2648.63 is the case heard: "…getradet," inhale | inhale "kann…".

use naivepost::cut_breaths::{one_breath, Recording, SR};

/// A tiny deterministic noise source.
struct Noise(u64);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}

enum Part {
    Speech(f64),
    Breath(f64),
    Ess(f64),
    Quiet(f64),
}

/// A 16 kHz recording made of the parts in order, as `voice16k.wav` holds it.
fn record(parts: &[Part]) -> Vec<i16> {
    let mut noise = Noise(7);
    let mut out: Vec<f32> = Vec::new();
    let mut last = 0.0f32;
    for part in parts {
        let (seconds, kind) = match part {
            Part::Speech(s) => (*s, 0),
            Part::Breath(s) => (*s, 1),
            Part::Ess(s) => (*s, 2),
            Part::Quiet(s) => (*s, 3),
        };
        for n in 0..(seconds * SR as f64) as usize {
            let t = n as f32 / SR as f32;
            let w = noise.next();
            let v = match kind {
                0 => (1..=12).map(|h| (2.0 * std::f32::consts::PI * 150.0 * h as f32 * t).sin() / h as f32).sum::<f32>() * 0.12,
                // tilted down: centre about 3 kHz
                1 => (w + 0.5 * last) * 0.012,
                // tilted up: centre about 5.5 kHz
                2 => (w - 0.9 * last) * 0.012,
                _ => w * 0.0002,
            };
            last = w;
            out.push(v);
        }
    }
    out.iter().map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16).collect()
}

fn speech_then(end: Part) -> Vec<i16> {
    record(&[Part::Quiet(0.3), Part::Speech(1.5), Part::Quiet(0.1), end])
}

fn breath_then_speech(start: Part) -> Vec<i16> {
    record(&[start, Part::Quiet(0.05), Part::Speech(1.5), Part::Quiet(0.3)])
}

/// The clip before runs on recording A (at session 0) up to its end; the clip after starts at session 100 on B.
fn scene(a: &[i16], b: &[i16]) -> (Vec<(f64, f64)>, Vec<Recording>) {
    let a_end = a.len() as f64 / SR as f64 - 0.06;
    let spans = vec![(0.2, a_end), (100.06, 100.0 + b.len() as f64 / SR as f64 - 0.1)];
    (spans, vec![Recording::new(0.0, a), Recording::new(100.0, b)])
}

#[test]
fn two_breaths_at_a_join_become_one() {
    let a = speech_then(Part::Breath(0.35));
    let b = breath_then_speech(Part::Breath(0.35));
    let (spans, recordings) = scene(&a, &b);
    let (out, logs) = one_breath(&spans, &recordings);
    assert_eq!(logs.len(), 1, "{logs:?}");
    assert!(logs[0].contains("two breaths in a row"), "{logs:?}");
    // The breath on A starts at 0.3 + 1.5 + 0.1 = 1.9 s: the clip before ends there, give or take a 20 ms frame.
    assert!((out[0].1 - 1.9).abs() <= 0.04, "{out:?}");
    assert_eq!(out[1], spans[1], "the breath before the next sentence stays");
}

#[test]
fn one_breath_at_a_join_is_left_alone() {
    let a = speech_then(Part::Quiet(0.35));
    let b = breath_then_speech(Part::Breath(0.35));
    let (spans, recordings) = scene(&a, &b);
    let (out, logs) = one_breath(&spans, &recordings);
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!(out, spans);
}

#[test]
fn a_closing_s_is_not_a_breath() {
    // "…Preis" | inhale "Und…": the word's own "s" sits far higher than an inhale and stays.
    let a = speech_then(Part::Ess(0.35));
    let b = breath_then_speech(Part::Breath(0.35));
    let (spans, recordings) = scene(&a, &b);
    let (out, logs) = one_breath(&spans, &recordings);
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!(out, spans);
}

#[test]
fn a_clip_is_never_cut_down_to_nothing() {
    // A clip that is almost all breath keeps it: the rule never leaves a clip under 0.3 s.
    let a = record(&[Part::Quiet(0.1), Part::Speech(0.2), Part::Breath(0.4)]);
    let b = breath_then_speech(Part::Breath(0.35));
    let spans = vec![(0.25, 0.68), (100.06, 101.8)];
    let recordings = vec![Recording::new(0.0, &a), Recording::new(100.0, &b)];
    let (out, _) = one_breath(&spans, &recordings);
    assert_eq!(out[0], spans[0], "{out:?}");
}
