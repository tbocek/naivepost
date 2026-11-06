//! Where a mark's two edges actually land (spec/04-prepare.md F1.11).

use crate::textfmt::Retake;
use crate::tools::mm_ss;
use crate::wave::Wave;

pub const WORD_PAD: f64 = 0.08;

pub const ENVELOPE_WINDOW: f64 = 4.0;

pub const EDGE_REACH: f64 = 0.8;

/// What is left of a sound at a cut once the envelope has placed it, in seconds.
pub const EDGE_PAD: f64 = 0.05;

pub const LATE_STAMP: f64 = 0.6;

pub const EDGE_TAIL_DB: f64 = 12.0;

pub const EDGE_TAIL_MAX: f64 = 0.25;

/// How far into the gap the quietest moment is looked for. Past the word's own tail the gap is breath
/// and room; the cut goes where that is lowest, and a trough further off than this is a different gap.
pub const TROUGH_REACH: f64 = 0.4;

/// How long the silence after a recording's first word must be stamped for [`misplaced_first_word`] to look in it.
pub const MISPLACED_GAP_SECONDS: f64 = 0.2;

/// How finely the gap is walked for where the sound starts: the wave cache's own 5 ms bucket.
const ONSET_STEP_SECONDS: f64 = 0.005;
/// How far either side of an aligned boundary between two touching words the place they part is looked for: one step
/// of the aligner's 80 ms grid. `P.eng.partReachSeconds`; `params::prepare()` rows it from here.
pub const PART_REACH: f64 = 0.08;
/// F1.11's hiss: a 5 ms bucket whose sound crosses zero at least this often per sample is an s, z or sch, not a
/// vowel. 0.375 is 30 crossings in a 16 kHz bucket of 80 samples; measured 09-account "Tokens signiert": the hiss
/// 42-67, the vowels around it 1-8.
pub const HISS_CROSSINGS: f64 = 0.375;
/// A hiss is only that loud a share of the loudest sound around it, so a breath or the room's noise is none.
const HISS_SHARE: i32 = 10;
/// Nor quieter than this on the s16 scale, whatever is around it.
const HISS_FLOOR: i32 = 300;
/// The shortest hiss that is a sound of its own; a lone noisy bucket inside a vowel is not one.
const HISS_MIN: f64 = 0.03;
/// How much sound either side of the boundary the hiss is looked for in.
const HISS_LOOK: f64 = 0.3;
/// Two word times this close are one boundary (`retakes.tsv` keeps two decimals).
const TOUCH: f64 = 0.006;

/// A recording's first word stamped too early: the corrected span, or `None` when it stands where it is.
pub fn misplaced_first_word(
    words: &[(f64, f64)],
    mut peak: impl FnMut(f64, f64) -> Option<i32>,
) -> Option<(f64, f64)> {
    let (first, next) = (*words.first()?, *words.get(1)?);
    if next.0 - first.1 < MISPLACED_GAP_SECONDS {
        return None;
    }
    let mut loud: Vec<i32> = words.iter().filter_map(|(s, e)| peak(*s, *e)).collect();
    loud.sort_unstable();
    let median = *loud.get(loud.len() / 2)?;
    let (own, in_gap) = (peak(first.0, first.1)?, peak(first.1, next.0)?);
    if in_gap * 2 < median || in_gap * 2 < own * 3 {
        return None;
    }
    let mut at = first.1;
    while at < next.0 {
        if peak(at, at + ONSET_STEP_SECONDS).is_some_and(|here| here * 2 >= in_gap) {
            return Some((at, next.0));
        }
        at += ONSET_STEP_SECONDS;
    }
    None
}

#[derive(Debug, Clone)]
pub struct Lane {
    pub key: String,
    pub off: f64,
    pub size: i64,
    pub mtime: i64,
}

/// The envelopes this session's `cache/waves` actually holds, in the lane order handed in.
pub fn load(tree: &crate::layout::Tree, lanes: &[Lane]) -> Vec<(Lane, Edges)> {
    lanes
        .iter()
        .filter_map(|lane| {
            let wave = crate::wave::read(tree, &lane.key, lane.size, lane.mtime).ok()??;
            let edges = Edges::new(wave, lane.off);
            let voice = tree.voice16k_wav(&lane.key);
            let edges = if voice.is_file() {
                edges.with_voice(voice)
            } else {
                edges
            };
            Some((lane.clone(), edges))
        })
        .collect()
}

pub fn finder<'a>(held: &'a [(Lane, Edges)]) -> impl Fn(f64) -> Option<&'a Edges> + 'a {
    move |t: f64| {
        held.iter()
            .filter(|(lane, _)| lane.off <= t)
            .max_by(|(a, _), (b, _)| a.off.total_cmp(&b.off))
            .map(|(_, edges)| edges)
    }
}

/// One aligned word on the session clock. Seconds rather than samples so placing an edge needs nothing
/// but the envelope's own rate — the conversion belongs to whoever read `words.aligned.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedWord {
    pub word: String,
    pub s: f64,
    pub e: f64,
}

/// The mono envelope of one recording, with that recording's place in the session so a session second
/// can be looked up in it.
#[derive(Debug, Clone)]
pub struct Edges {
    /// Peaks per second in `wave`'s first channel — the timeline's own cache, 200 Hz, so a bucket is the
    /// 5 ms the spec counts silence in.
    pub wave: Wave,
    /// The session second this file's first sample lands on.
    pub off: f64,
    /// The recording's own sound (`voice16k.wav`, 16 kHz, on the same clock as `wave`), read around an edge for
    /// the hiss rule. None: the rule is skipped and the loudness alone places the edge.
    pub voice: Option<std::path::PathBuf>,
}

impl Edges {
    pub fn new(wave: Wave, off: f64) -> Self {
        Self {
            wave,
            off,
            voice: None,
        }
    }

    /// The same envelope with the recording's 16 kHz sound beside it, for [`Self::hiss`].
    pub fn with_voice(mut self, voice: std::path::PathBuf) -> Self {
        self.voice = Some(voice);
        self
    }

    /// The hiss (an s, z or sch) nearest the session second `t`, as its first and last session second: a run of 5 ms
    /// buckets that cross zero at [`HISS_CROSSINGS`] and carry sound, at least [`HISS_MIN`] long, holding `t` or
    /// ending within [`PART_REACH`] of it. One quiet bucket inside does not break it. None without the sound file.
    ///
    /// Two words that share a hiss have no quieter moment between them: 09-account "Tokens signiert" said the two s
    /// as one 215 ms hiss, and the loudness put the cut after it, keeping the s of the dropped "signiert".
    pub fn hiss(&self, t: f64) -> Option<(f64, f64)> {
        const RATE: f64 = crate::transcribe::SAMPLE_RATE as f64;
        let voice = self.voice.as_ref()?;
        let first = ((t - HISS_LOOK - self.off) * RATE).max(0.0) as usize;
        let last = ((t + HISS_LOOK - self.off) * RATE).max(0.0) as usize;
        let x = crate::wave_build::read_wav_span(voice, first, last.saturating_sub(first)).ok()?;
        let step = crate::wave_build::SAMPLES_PER_BUCKET;
        let buckets: Vec<(i32, f64)> = x
            .chunks_exact(step)
            .map(|b| {
                let peak = b
                    .iter()
                    .map(|v| i32::from(v.unsigned_abs()))
                    .max()
                    .unwrap_or(0);
                let crossings = b.windows(2).filter(|p| (p[0] < 0) != (p[1] < 0)).count();
                (peak, crossings as f64 / (step - 1) as f64)
            })
            .collect();
        let loudest = buckets.iter().map(|b| b.0).max()?;
        let hissing: Vec<bool> = buckets
            .iter()
            .map(|&(peak, rate)| {
                peak >= HISS_FLOOR.max(loudest / HISS_SHARE) && rate >= HISS_CROSSINGS
            })
            .collect();
        let at = |i: usize| self.off + (first + i * step) as f64 / RATE;
        let mut runs = Vec::new();
        let mut i = 0;
        while i < hissing.len() {
            if !hissing[i] {
                i += 1;
                continue;
            }
            let mut j = i;
            while j + 1 < hissing.len() && (hissing[j + 1] || hissing.get(j + 2) == Some(&true)) {
                j += 1;
            }
            runs.push((at(i), at(j + 1)));
            i = j + 1;
        }
        let away = |(a, b): (f64, f64)| {
            if t < a {
                a - t
            } else if t > b {
                t - b
            } else {
                0.0
            }
        };
        runs.into_iter()
            .filter(|&(a, b)| b - a >= HISS_MIN - 1e-9 && away((a, b)) <= PART_REACH)
            .min_by(|&p, &q| away(p).total_cmp(&away(q)))
    }

    fn peaks(&self) -> &[u8] {
        self.wave
            .chans
            .first()
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The bucket a session second falls in, or −1 outside the file.
    fn at(&self, t: f64) -> i64 {
        let i = ((t - self.off) * self.wave.hz) as i64;
        if i < 0 || i >= self.peaks().len() as i64 {
            return -1;
        }
        i
    }

    /// Where two words that touch in the aligner's times part in the sound: the quietest 5 ms nearest the boundary `t`
    /// in the valley between them, the valley `t` sits in or the one a slope down from `t` runs into, within
    /// [`PART_REACH`]. Quiet is under half the loudest bucket in reach. A boundary inside a stretch that stays loud
    /// (two words said in one breath) has no valley to go to and stays: `None`, as for one already at the valley's
    /// quietest. The answer is the start of that bucket, so the cut keeps none of it.
    ///
    /// The aligner's 80 ms grid puts such a boundary up to a step off: 09-account 69.36, the cut after "Ethereum," kept
    /// the first 12 ms of the dropped "wie", a soft onset from 69.3475; the two part at 69.340.
    pub fn parting(&self, t: f64) -> Option<f64> {
        let i = self.at(t);
        if i < 0 {
            return None;
        }
        let peaks = self.peaks();
        let level = |j: i64| i32::from(peaks[j as usize]);
        let reach = (PART_REACH * self.wave.hz).round() as i64;
        let lo = (i - reach).max(0);
        let hi = (i + reach).min(peaks.len() as i64 - 1);
        let loudest = (lo..=hi).map(level).max()?;
        // Nothing in reach to tell a word from the room by.
        if loudest < 8 {
            return None;
        }
        let quiet = |j: i64| level(j) * 2 < loudest;
        // The valley `t` sits in, or the one a slope down from it reaches: each step no louder than the one before
        // (one byte of give), and never more than two steps along a loud stretch that does not fall.
        let seed = if quiet(i) {
            Some(i)
        } else {
            [-1i64, 1]
                .into_iter()
                .filter_map(|dir| {
                    let (mut j, mut prev, mut flat) = (i, level(i), 0);
                    loop {
                        j += dir;
                        if j < lo || j > hi || level(j) > prev + 1 {
                            return None;
                        }
                        if quiet(j) {
                            return Some(j);
                        }
                        if level(j) >= prev {
                            flat += 1;
                            if flat > 2 {
                                return None;
                            }
                        }
                        prev = level(j);
                    }
                })
                .min_by_key(|j| (j - i).abs())
        }?;
        // The valley: every quiet bucket joined to the seed, within reach.
        let (mut from, mut to) = (seed, seed);
        while from > lo && quiet(from - 1) {
            from -= 1;
        }
        while to < hi && quiet(to + 1) {
            to += 1;
        }
        let floor = (from..=to).map(level).min()?;
        let best = (0..=reach)
            .flat_map(|d| [i - d, i + d])
            .find(|&j| j >= from && j <= to && level(j) <= floor + 1)?;
        if best == i {
            return None;
        }
        Some(self.off + best as f64 / self.wave.hz)
    }

    /// The loudest bucket between two session seconds; `None` where the span holds no bucket of this file.
    pub fn peak_between(&self, from: f64, to: f64) -> Option<i32> {
        let peaks = self.peaks();
        let lo = ((from - self.off) * self.wave.hz).floor().max(0.0) as usize;
        let hi = ((to - self.off) * self.wave.hz).ceil().max(0.0) as usize;
        peaks
            .get(lo..hi.min(peaks.len()))?
            .iter()
            .map(|peak| i32::from(*peak))
            .max()
    }

    pub fn floor(&self, t: f64) -> u8 {
        let i = self.at(t);
        if i < 0 {
            return u8::MAX;
        }
        let peaks = self.peaks();
        let reach = (ENVELOPE_WINDOW * self.wave.hz) as usize;
        let (lo, hi) = (
            i as usize - (i as usize).min(reach),
            (i as usize + reach).min(peaks.len()),
        );
        let mut window: Vec<u8> = peaks[lo..hi].to_vec();
        window.sort_unstable();
        if window.is_empty() {
            return u8::MAX;
        }
        let quiet = u16::from(window[window.len() / 5]);
        (quiet * 3).max(quiet + 8).min(u16::from(u8::MAX)) as u8
    }

    /// Whether this bucket is above the floor.
    fn sound(&self, i: i64, thr: u8) -> bool {
        let peaks = self.peaks();
        (0..peaks.len() as i64).contains(&i) && peaks[i as usize] >= thr
    }

    /// What still counts as the word between these buckets: its own peak less [`EDGE_TAIL_DB`], and never
    /// under the room.
    fn tail_level(&self, i0: i64, i1: i64) -> u8 {
        let peaks = self.peaks();
        let (start, end) = (
            i0.max(0) as usize,
            (i1.max(0) as usize).min(peaks.len().saturating_sub(1)),
        );
        let peak = peaks[start..=end].iter().copied().max().unwrap_or(0);
        let below = EDGE_TAIL_DB * 255.0 / 70.0;
        let thr = i16::from(peak) - i16::try_from(below.round() as u32).unwrap_or(i16::MAX);
        let room = self.floor(
            self.off + f64::from(i32::try_from(i0.max(0)).unwrap_or(i32::MAX)) / self.wave.hz,
        );
        thr.max(i16::from(room)).clamp(0, 255) as u8
    }

    pub fn end_after(&self, w: &AlignedWord, limit: f64) -> f64 {
        if limit <= w.e {
            // The fence has no width: the aligner's time is all there is, and the sound is never asked.
            return w.e;
        }
        let (i0, i1) = (self.at(w.s), self.at(w.e));
        if i0 < 0 || i1 < 0 {
            return (w.e + WORD_PAD).min(limit);
        }
        let lim = self.at(limit);
        let lim = if lim < 0 {
            self.peaks().len() as i64 - 1
        } else {
            lim
        };

        let thr = self.tail_level(i0, i1);
        let tail_end = (i1 + (EDGE_TAIL_MAX * self.wave.hz) as i64).min(lim);
        let mut end = i1;
        while end < tail_end && self.sound(end, thr) {
            end += 1;
        }
        let mut best = end;
        let trough_end = (end + (TROUGH_REACH * self.wave.hz) as i64).min(lim);
        for k in end..=trough_end {
            if self.peaks()[k as usize] < self.peaks()[best as usize] {
                best = k;
            }
        }
        limit.min((self.off + best as f64 / self.wave.hz).max(w.e))
    }

    /// The same the other way: where the cut resumes before a word that starts again, given how far back
    /// it may go (the last dropped word's end, which it never reaches).
    pub fn start_before(&self, w: &AlignedWord, limit: f64) -> f64 {
        if limit >= w.s {
            return w.s;
        }
        let (i0, i1) = (self.at(w.s), self.at(w.e));
        if i0 < 0 || i1 < 0 {
            return (w.s - WORD_PAD).max(limit);
        }
        let lim = self.at(limit);
        let lim = if lim < 0 { 0 } else { lim };

        let thr = self.tail_level(i0, i1);
        let tail_start = (i0 - (EDGE_TAIL_MAX * self.wave.hz) as i64).max(lim);
        let mut start = i0;
        while start > tail_start && self.sound(start - 1, thr) {
            start -= 1;
        }
        let mut best = start;
        let trough_start = (start - (TROUGH_REACH * self.wave.hz) as i64).max(lim);
        for k in trough_start..=start {
            if self.peaks()[k as usize] < self.peaks()[best as usize] {
                best = k;
            }
        }
        limit.max((self.off + best as f64 / self.wave.hz).min(w.s))
    }

    pub fn end_before(&self, t: f64) -> f64 {
        let i = self.at(t);
        if i < 0 {
            return t;
        }
        let thr = self.floor(t);
        let limit = (EDGE_REACH * self.wave.hz) as i64;

        let mut j = i;
        while j >= 0 && i - j <= limit && self.sound(j, thr) {
            j -= 1;
        }
        while j >= 0 && i - j <= limit && !self.sound(j, thr) {
            j -= 1;
        }
        if j < 0 || i - j > limit {
            return t;
        }
        self.off + (j + 1) as f64 / self.wave.hz + EDGE_PAD
    }

    pub fn start_at(&self, t: f64) -> f64 {
        let i = self.at(t);
        if i < 0 {
            return t;
        }
        let thr = self.floor(t);
        let limit = (EDGE_REACH * self.wave.hz) as i64;

        let mut j = i;
        if !self.sound(i, thr) {
            // Back over the quiet to the run before it, if the quiet is short enough to be the tail of a
            // late stamp.
            let late = (LATE_STAMP * self.wave.hz) as i64;
            while j > 0 && i - j <= late && !self.sound(j - 1, thr) {
                j -= 1;
            }
            if j == 0 || !self.sound(j - 1, thr) {
                return t;
            }
        }
        while j > 0 && i - j <= limit && self.sound(j - 1, thr) {
            j -= 1;
        }
        if i - j > limit {
            return t;
        }
        (self.off).max(self.off + j as f64 / self.wave.hz - EDGE_PAD)
    }
}

/// Does a word end at `t` and another start there: a boundary with no sound asked of it (§F1.11).
fn touching(words: &[AlignedWord], t: f64) -> bool {
    words.iter().any(|w| (w.e - t).abs() <= TOUCH) && words.iter().any(|w| (w.s - t).abs() <= TOUCH)
}

/// The last word ending at or before this second.
fn last_word(words: &[AlignedWord], t: f64) -> Option<&AlignedWord> {
    words
        .iter()
        .filter(|w| w.e <= t)
        .max_by(|a, b| a.e.total_cmp(&b.e))
}

/// Is any word inside this stretch? "Nothing was said between" is what lets the whole gap go.
fn words_between(words: &[AlignedWord], from: f64, to: f64) -> bool {
    words.iter().any(|w| w.s > from && w.s < to)
}

/// The word a stamp names: the one starting closest to it, within one [`WORD_PAD`].
fn word_at(words: &[AlignedWord], t: f64) -> Option<&AlignedWord> {
    words
        .iter()
        .filter(|w| (w.s - t).abs() <= WORD_PAD)
        .min_by(|a, b| (a.s - t).abs().total_cmp(&(b.s - t).abs()))
}

/// Was anything spoken in this stretch? Other recordings' rows count: a mark cannot resume over the top
/// of a sentence somebody else is saying.
fn spoken_between(spoken: &[(f64, f64)], from: f64, to: f64) -> bool {
    spoken.iter().any(|(s, e)| *e > from && *s < to)
}

/// Does the word begin with a hiss (s, z, sch, x, ß, a soft c)? The aligner's word names are lower case.
fn hiss_first(word: &str) -> bool {
    let mut letters = word.chars();
    match letters.next() {
        Some('s' | 'z' | 'x' | 'ß') => true,
        Some('c') => matches!(letters.next(), Some('e' | 'i' | 'y')),
        _ => false,
    }
}

/// Does the word end in a hiss (s, z, tz, sch, x, ß, ce)?
fn hiss_last(word: &str) -> bool {
    word.ends_with(['s', 'z', 'x', 'ß']) || word.ends_with("sch") || word.ends_with("ce")
}

/// Where two touching words part when a hiss is at their boundary `t`, by which of them the hiss belongs to: both
/// ("Tokens signiert") → its middle, half to each; one → the edge goes round it, taking the kept word's own hiss in
/// whole and leaving the dropped word's out. `keep_after` says the word after `t` is the kept one (the cut resumes
/// there). Already clear of a dropped word's hiss, the edge stays: it never moves to take in more of a dropped word.
fn hiss_parting(edges: &Edges, words: &[AlignedWord], t: f64, keep_after: bool) -> Option<f64> {
    let before = words.iter().rfind(|w| (w.e - t).abs() <= TOUCH)?;
    let after = words.iter().find(|w| (w.s - t).abs() <= TOUCH)?;
    let (ends, starts) = (hiss_last(&before.word), hiss_first(&after.word));
    if !ends && !starts {
        return None;
    }
    let (a, b) = edges.hiss(t)?;
    let at = match (ends, starts, keep_after) {
        (true, true, _) => (a + b) / 2.0,
        // The hiss is the word before's: kept → the clip ends after it; dropped → the clip resumes after it.
        (true, false, _) => b.max(t),
        // The word after's: dropped → the clip ends before it; kept → the clip resumes before it.
        _ => a.min(t),
    };
    ((at - t).abs() > 1e-9).then_some(at)
}

fn hiss_note(was: f64, at: f64) -> String {
    format!(
        ">>> {}: the cut moves {:+.0} ms to where the two words part in the hiss between them",
        mm_ss(at),
        (at - was) * 1000.0
    )
}

fn parting_note(was: f64, at: f64) -> String {
    format!(
        ">>> {}: the cut moves {:+.0} ms to where the two words part, off the aligner's boundary",
        mm_ss(at),
        (at - was) * 1000.0
    )
}

/// S1–S7 of the placement: every mark's two edges moved onto the sound.
pub fn place_edges<'a>(
    marks: Vec<Retake>,
    words: &[AlignedWord],
    mut edge_of: impl FnMut(f64) -> Option<&'a Edges>,
    spoken: &[(f64, f64)],
) -> (Vec<Retake>, Vec<String>) {
    let mut notes = Vec::new();
    let marks = marks
        .into_iter()
        .map(|mut mark| {
            if !words.is_empty() {
                if let Some(w) = last_word(words, mark.s).filter(|w| w.e < mark.s) {
                    let before = mark.s;
                    let mut s = (w.e + WORD_PAD).min(before);
                    if let Some(e) = edge_of(w.e) {
                        s = e.end_after(w, before);
                    }
                    mark.s = s;
                    notes.push(format!(
                        ">>> {}: the cut ends {:.2}s earlier, after the last word that stays",
                        mm_ss(mark.s),
                        before - s
                    ));
                } else if touching(words, mark.s) {
                    // The kept word and the dropped one touch in the word times: the cut goes where they part, in
                    // the hiss when one is at the boundary, else at the quietest moment between them.
                    if let Some(e) = edge_of(mark.s) {
                        if let Some(at) = hiss_parting(e, words, mark.s, false) {
                            notes.push(hiss_note(mark.s, at));
                            mark.s = at;
                        } else if let Some(at) = e.parting(mark.s) {
                            notes.push(parting_note(mark.s, at));
                            mark.s = at;
                        }
                    }
                }
                if mark.again > mark.e && !words_between(words, mark.e, mark.again) {
                    let mut to = (mark.again - WORD_PAD).max(mark.e);
                    if let Some(w) = word_at(words, mark.again) {
                        if let Some(e) = edge_of(mark.again) {
                            to = e.start_before(w, mark.e);
                        }
                    }
                    if to > mark.to {
                        notes.push(format!(
                            ">>> {}: the cut resumes at {}, just before the retake's first word",
                            mm_ss(mark.s),
                            mm_ss(to)
                        ));
                        mark.to = to;
                    }
                }
                // The same where the cut resumes on a kept word that touches the last dropped one.
                if mark.to > mark.s && touching(words, mark.to) {
                    if let Some(e) = edge_of(mark.to) {
                        let hissed =
                            hiss_parting(e, words, mark.to, true).filter(|at| *at > mark.s);
                        if let Some(at) = hissed {
                            notes.push(hiss_note(mark.to, at));
                            mark.to = at;
                        } else if let Some(at) = e.parting(mark.to).filter(|at| *at > mark.s) {
                            notes.push(parting_note(mark.to, at));
                            mark.to = at;
                        }
                    }
                }
                return mark;
            }

            if let Some(e) = edge_of(mark.s) {
                let s = e.end_before(mark.s);
                if s != mark.s {
                    let before = mark.s;
                    notes.push(format!(
                        ">>> {}: the cut ends {:.2}s earlier, where the sound before it stops",
                        mm_ss(s),
                        before - s
                    ));
                    mark.s = s;
                }
            }
            if mark.again > 0.0 && !spoken_between(spoken, mark.e, mark.again) {
                let mut to = mark.again;
                if let Some(e) = edge_of(mark.again) {
                    to = e.start_at(mark.again).clamp(mark.e, mark.again);
                }
                if to > mark.to {
                    notes.push(format!(
                        ">>> {}: the cut resumes at {}, where the retake starts to sound",
                        mm_ss(mark.s),
                        mm_ss(to)
                    ));
                    mark.to = to;
                }
            }
            mark
        })
        .collect();
    (marks, notes)
}
