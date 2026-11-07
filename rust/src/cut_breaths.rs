//! At most one breath at a join (spec/05-cut.md, suggestions): where the clip before a join ends on an inhale and the
//! clip after it starts with one, the listener hears two gasps in a row. The first clip then ends where its breath
//! begins; the breath before the next sentence stays.
//!
//! Heard on 08-dex at 2648.63 ("…getradet," inhale | inhale "kann…"); 3 of 269 joins in the five lecture projects with
//! a session word list. A breath is found in the sound, not in the word times: the aligner stretches the next word
//! back over the inhale ("kann" timed from the clip's first sample for 0.8 s).
//!
//! What counts as a breath, per 20 ms frame (40 ms Hann window): unvoiced (autocorrelation peak under 0.35 at 50-400
//! Hz), noise-like (spectral flatness over 0.4), centred between 2.0 and 4.2 kHz, and 15 to 45 dB under the
//! recording's speech level; at least 160 ms of such frames in a row. The centre band is what keeps a word's own
//! closing "s", "ß" or "ts" out (4.4-5.4 kHz on these recordings, "Preis", "groß", "Shorts"); an inhale sits at
//! 2.4-3.4 kHz. The stretch searched is between the cut and the nearest voiced speech on that side.

/// Samples a second: the rate `voice16k.wav` is written at.
pub const SR: usize = 16_000;
/// 20 ms frames.
const HOP: usize = 320;
/// 40 ms analysis window.
const WIN: usize = 640;
/// Shortest breath, seconds.
pub const MIN_BREATH: f64 = 0.16;
/// How far from the cut the nearest voiced speech is looked for, seconds.
pub const REACH: f64 = 1.2;
/// A clip is never shortened under this by the rule, seconds.
const MIN_CLIP: f64 = 0.3;

/// One recording's voice track on the session clock.
pub struct Recording {
    /// The session second its first sample lands on.
    pub off: f64,
    pub samples: Vec<f32>,
    /// The recording's speech level in dB (90th percentile of the louder half of its 10 ms frames), plus 3 dB.
    pub speech_db: f64,
}

impl Recording {
    pub fn new(off: f64, samples: &[i16]) -> Self {
        let samples: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();
        let speech_db = speech_level(&samples) + 3.0;
        Recording {
            off,
            samples,
            speech_db,
        }
    }

    pub fn seconds(&self) -> f64 {
        self.samples.len() as f64 / SR as f64
    }

    fn covers(&self, t: f64) -> bool {
        t >= self.off + 0.05 && t <= self.off + self.seconds() - 0.05
    }
}

/// 90th percentile of the louder half of the 10 ms frames' power in dB.
fn speech_level(x: &[f32]) -> f64 {
    let hop = 160;
    let frames: Vec<f64> = x
        .chunks_exact(hop)
        .map(|frame| {
            let power = frame.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / hop as f64;
            10.0 * (power + 1e-10).log10()
        })
        .collect();
    if frames.is_empty() {
        return -100.0;
    }
    let median = percentile(&frames, 50.0);
    let loud: Vec<f64> = frames.into_iter().filter(|&d| d > median).collect();
    if loud.is_empty() {
        return median;
    }
    percentile(&loud, 90.0)
}

/// Linear interpolation between closest ranks, as numpy's default.
fn percentile(values: &[f64], q: f64) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = q / 100.0 * (v.len() - 1) as f64;
    let (lo, hi) = (rank.floor() as usize, rank.ceil() as usize);
    v[lo] + (v[hi] - v[lo]) * (rank - lo as f64)
}

struct Analyser {
    window: Vec<f64>,
    cos: Vec<f64>,
    sin: Vec<f64>,
}

impl Analyser {
    fn new() -> Self {
        let window = (0..WIN)
            .map(|n| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / (WIN - 1) as f64).cos())
            .collect();
        let cos = (0..WIN)
            .map(|k| (2.0 * std::f64::consts::PI * k as f64 / WIN as f64).cos())
            .collect();
        let sin = (0..WIN)
            .map(|k| (2.0 * std::f64::consts::PI * k as f64 / WIN as f64).sin())
            .collect();
        Analyser { window, cos, sin }
    }

    /// How many 20 ms frames `x` holds.
    fn count(x: &[f32]) -> usize {
        x.len() / HOP
    }

    /// Frame `i` of `x`: 640 samples from 160 before its hop, zero-padded at the slice's end.
    fn segment(x: &[f32], i: usize) -> Vec<f64> {
        let from = (i * HOP).saturating_sub(160);
        let to = (i * HOP + 480).min(x.len());
        let mut seg = vec![0.0f64; WIN];
        for (j, &v) in x[from..to].iter().enumerate().take(WIN) {
            seg[j] = v as f64;
        }
        seg
    }

    fn level(seg: &[f64]) -> f64 {
        10.0 * (seg.iter().map(|s| s * s).sum::<f64>() / WIN as f64 + 1e-10).log10()
    }

    fn centred(seg: &[f64]) -> Vec<f64> {
        let mean = seg.iter().sum::<f64>() / WIN as f64;
        seg.iter().map(|v| v - mean).collect()
    }

    /// The autocorrelation peak at 50-400 Hz.
    fn voicing(seg: &[f64]) -> f64 {
        let s = Self::centred(seg);
        let zero: f64 = s.iter().map(|v| v * v).sum();
        (40..320)
            .map(|lag| {
                s[..WIN - lag]
                    .iter()
                    .zip(&s[lag..])
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
                    / (zero + 1e-12)
            })
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Spectral flatness and centre frequency of the Hann-windowed frame.
    fn spectrum(&self, seg: &[f64]) -> (f64, f64) {
        let w: Vec<f64> = Self::centred(seg)
            .iter()
            .zip(&self.window)
            .map(|(v, h)| v * h)
            .collect();
        let bins = WIN / 2 + 1;
        let (mut log_sum, mut sum, mut weighted) = (0.0, 0.0, 0.0);
        for k in 0..bins {
            let (mut re, mut im) = (0.0, 0.0);
            for (n, v) in w.iter().enumerate() {
                let idx = (k * n) % WIN;
                re += v * self.cos[idx];
                im -= v * self.sin[idx];
            }
            let mag = (re * re + im * im).sqrt() + 1e-9;
            log_sum += mag.ln();
            sum += mag;
            weighted += mag * (k as f64 * SR as f64 / WIN as f64);
        }
        (
            (log_sum / bins as f64).exp() / (sum / bins as f64),
            weighted / sum,
        )
    }

    /// Whether frame `seg` is breath, measuring only as far as the answer needs.
    fn is_breath(&self, seg: &[f64], speech_db: f64) -> bool {
        let level = Self::level(seg) - speech_db;
        if !(level > -45.0 && level < -15.0) || Self::voicing(seg) >= 0.35 {
            return false;
        }
        let (flatness, centroid) = self.spectrum(seg);
        flatness > 0.4 && centroid > 2000.0 && centroid < 4200.0
    }

    /// Whether frame `seg` is voiced speech: pitched and within 20 dB of the speech level.
    fn is_voiced(seg: &[f64], speech_db: f64) -> bool {
        Self::level(seg) - speech_db > -20.0 && Self::voicing(seg) > 0.5
    }
}

/// The breaths inside `[t0, t1)` (seconds on the recording's own clock).
fn breath_spans(an: &Analyser, rec: &Recording, t0: f64, t1: f64) -> Vec<(f64, f64)> {
    if t1 - t0 < MIN_BREATH {
        return Vec::new();
    }
    let a = (t0.max(0.0) * SR as f64) as usize;
    let b = ((t1 * SR as f64) as usize).min(rec.samples.len());
    if b <= a {
        return Vec::new();
    }
    let x = &rec.samples[a..b];
    let mut breath: Vec<bool> = (0..Analyser::count(x))
        .map(|i| an.is_breath(&Analyser::segment(x, i), rec.speech_db))
        .collect();
    // A single frame between two breath frames is breath too.
    for i in 1..breath.len().saturating_sub(1) {
        if !breath[i] && breath[i - 1] && breath[i + 1] {
            breath[i] = true;
        }
    }
    let mut spans = Vec::new();
    let mut i = 0;
    while i < breath.len() {
        if !breath[i] {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < breath.len() && breath[j] {
            j += 1;
        }
        let (s, e) = (
            t0 + (i * HOP) as f64 / SR as f64,
            t0 + (j * HOP) as f64 / SR as f64,
        );
        if e - s >= MIN_BREATH - 1e-9 {
            spans.push((s, e));
        }
        i = j;
    }
    spans
}

/// The stretch between the cut at `t` and the nearest voiced speech on one side (before it for a clip's end).
fn toward_speech(an: &Analyser, rec: &Recording, t: f64, before: bool) -> (f64, f64) {
    let from = if before { t - REACH } else { t };
    let to = if before { t } else { t + REACH };
    let a = (from.max(0.0) * SR as f64) as usize;
    let b = ((to * SR as f64) as usize).min(rec.samples.len());
    let x = if b > a {
        &rec.samples[a..b]
    } else {
        &rec.samples[0..0]
    };
    let n = Analyser::count(x);
    let voiced = |i: usize| Analyser::is_voiced(&Analyser::segment(x, i), rec.speech_db);
    let start = a as f64 / SR as f64;
    let _ = an;
    if before {
        // The nearest voiced frame before the cut: scanned from the cut backwards.
        let k = (0..n)
            .rev()
            .find(|&i| voiced(i))
            .map(|k| k + 1)
            .unwrap_or(0);
        (start + (k * HOP) as f64 / SR as f64, t)
    } else {
        let k = (0..n).find(|&i| voiced(i)).unwrap_or(n);
        (t, start + (k * HOP) as f64 / SR as f64)
    }
}

/// One join's breaths, in session seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinBreaths {
    /// The session second the clip before the join ends at.
    pub at: f64,
    /// Breaths between the clip before's last voiced sound and its end.
    pub before: Vec<(f64, f64)>,
    /// Breaths between the clip after's start and its first voiced sound.
    pub after: Vec<(f64, f64)>,
}

/// The breaths at every join between consecutive spans (session seconds, sorted). A join whose either side no
/// recording covers is left out.
pub fn joins(spans: &[(f64, f64)], recordings: &[Recording]) -> Vec<JoinBreaths> {
    let an = Analyser::new();
    let at = |t: f64| recordings.iter().find(|r| r.covers(t));
    spans
        .windows(2)
        .filter_map(|pair| {
            let ((_, end), (start, _)) = (pair[0], pair[1]);
            let (ra, rb) = (at(end)?, at(start)?);
            let (a0, a1) = toward_speech(&an, ra, end - ra.off, true);
            let (b0, b1) = toward_speech(&an, rb, start - rb.off, false);
            let shift = |r: &Recording, spans: Vec<(f64, f64)>| -> Vec<(f64, f64)> {
                spans
                    .into_iter()
                    .map(|(s, e)| (s + r.off, e + r.off))
                    .collect()
            };
            Some(JoinBreaths {
                at: end,
                before: shift(ra, breath_spans(&an, ra, a0, a1)),
                after: shift(rb, breath_spans(&an, rb, b0, b1)),
            })
        })
        .collect()
}

/// The spans with at most one breath at each join, and a log line per join changed.
pub fn one_breath(
    spans: &[(f64, f64)],
    recordings: &[Recording],
) -> (Vec<(f64, f64)>, Vec<String>) {
    let mut sorted = spans.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut logs = Vec::new();
    for join in joins(&sorted, recordings) {
        let (Some(&(first, _)), false) = (join.before.first(), join.after.is_empty()) else {
            continue;
        };
        let Some(span) = sorted.iter_mut().find(|(_, e)| (*e - join.at).abs() < 1e-9) else {
            continue;
        };
        if first - span.0 < MIN_CLIP || first >= span.1 {
            continue;
        }
        logs.push(log(join.at, span.1 - first));
        span.1 = first;
    }
    (sorted, logs)
}

pub fn log(at: f64, seconds: f64) -> String {
    format!(
        ">>> cut: two breaths in a row at {} -- the clip before ends {:.2} s earlier, where its breath begins",
        crate::tools::mm_ss(at),
        seconds
    )
}

/// The project's voice tracks on the session clock, read from `prepare/inputs/<base>/voice16k.wav`, for the
/// recordings that hold a second some span ends or starts at.
pub fn recordings_for(
    tree: &crate::layout::Tree,
    project: &crate::project::Project,
    spans: &[(f64, f64)],
) -> Vec<Recording> {
    let bases: Vec<String> = project
        .sources
        .iter()
        .map(|source| crate::audio_task::source_lane(&source.path))
        .collect();
    let names: Vec<&str> = bases.iter().map(String::as_str).collect();
    let offsets = crate::fix_transcripts::placement(&names);
    let edges: Vec<f64> = spans.iter().flat_map(|(s, e)| [*s, *e]).collect();
    bases
        .iter()
        .zip(offsets)
        .filter_map(|(base, off)| {
            let path = tree.voice16k_wav(base);
            let seconds = (std::fs::metadata(&path).ok()?.len() as f64 - 44.0) / (2 * SR) as f64;
            edges
                .iter()
                .any(|&t| t >= off && t <= off + seconds)
                .then(|| crate::wave_build::read_wav_samples(&path).ok())
                .flatten()
                .map(|samples| Recording::new(off, &samples))
        })
        .collect()
}

/// [`one_breath`] over a suggestion's footage before it lands. A join with an insert in it (a card spliced or laid
/// between the two clips) is left as it is: the two breaths do not meet there.
pub fn apply(
    tree: &crate::layout::Tree,
    project: &crate::project::Project,
    spans: &[(f64, f64)],
    inserts: &[crate::cut::Seg],
) -> (Vec<(f64, f64)>, Vec<String>) {
    let recordings = recordings_for(tree, project, spans);
    let (mut out, _) = one_breath(spans, &recordings);
    let mut was = spans.to_vec();
    was.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut logs = Vec::new();
    for i in 0..out.len() {
        if (was[i].1 - out[i].1).abs() < 1e-9 {
            continue;
        }
        let next = was.get(i + 1).map(|n| n.0).unwrap_or(f64::INFINITY);
        let between = inserts
            .iter()
            .any(|seg| !seg.ins.is_empty() && seg.s >= was[i].1 - 0.01 && seg.s <= next + 0.01);
        if between {
            out[i] = was[i];
        } else {
            logs.push(log(was[i].1, was[i].1 - out[i].1));
        }
    }
    (out, logs)
}
