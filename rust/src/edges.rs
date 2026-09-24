//! Where a mark's two edges actually land (spec/04-prepare.md F1.11).
//!
//! The aligner returns words back to back — each ends where the next begins — so where the word after
//! a kept word was dropped, the fence has no width: the edge lands on the aligner's time and the sound
//! is never asked. With word times the words fence the cut and the envelope only chooses where inside
//! the fence it falls; without them the mono envelope alone places the edge, which is the rule that had
//! to cope with an ASR stamp running half a second late.
//!
//! As in [`crate::asr`], [`crate::align`] and [`crate::frames`], nothing is spawned and no socket
//! opened: the envelope arrives as the [`Wave`] the timeline already caches, so a rule this load-bearing
//! can be tested against a hand-written one.

use crate::textfmt::Retake;
use crate::tools::mm_ss;
use crate::wave::Wave;

/// What is left of a sound at a cut when there is no envelope to ask: a hair, so the last consonant is
/// not shaved and the first is not clipped. §10 files this bound as `P.eng.wordPadSeconds` (0.08, "room a
/// cut leaves a word"; prototype `wordPad`, gui/retake.go), and `params::prepare()` rows it from this
/// constant.
pub const WORD_PAD: f64 = 0.08;

/// How wide a stretch of the recording is looked at to find its room: this many seconds each side of
/// the second being placed at. §10 files it as `P.eng.envelopeWindowSeconds` (4, "envelope searched
/// each side of a stamp (±400 buckets)"; prototype 400 buckets at the 100 Hz envelope of
/// gui/retake_edge.go:163), and `params::prepare()` rows it from this constant. The 20th percentile
/// of this window is the room; a window too narrow to hold the room raises the floor and hides sound,
/// one too wide lets a far-loud stretch pull it down.
pub const ENVELOPE_WINDOW: f64 = 4.0;

/// How far an edge may be moved off its stamp. A stamp half a second late is what was measured; a sound
/// a whole second away is a different sound. This is the reach of the ENVELOPE-ONLY placement — with
/// aligned word times the words fence the edge and this bound is never consulted. §10 files it as
/// `P.eng.edgeReachSeconds` (0.8, "envelope-only edge placement: search reach"; prototype
/// `retake_edge`, gui/retake_edge.go), and `params::prepare()` rows it from this constant.
pub const EDGE_REACH: f64 = 0.8;

/// What is left of a sound at a cut once the envelope has placed it, in seconds.
pub const EDGE_PAD: f64 = 0.05;

/// How far behind its sound a stamp may run and still be that sound's — half a second was measured on a
/// take's opening word. §10 files it as `P.eng.lateStampSeconds` (0.6, "envelope-only edge placement:
/// how late a stamp may be"; prototype `lateStamp`, gui/retake_edge.go), and `params::prepare()` rows
/// it from this constant.
pub const LATE_STAMP: f64 = 0.6;

/// With aligned words, how far below a word's own level its tail still counts as the word. On the meter
/// scale the envelope is drawn in (−70…0 dBFS over 0…255) a breath sits 20 dB or more under the word it
/// follows and a trailing consonant a few dB under; twelve tells them apart. §10 files it as
/// `P.eng.edgeTailDB` (12, "word-fenced edge placement: tail threshold"; prototype `edgeTailDB`,
/// gui/retake_edge.go), and `params::prepare()` rows it from this constant.
pub const EDGE_TAIL_DB: f64 = 12.0;

/// How far past the aligner's edge a word's own sound is followed. The aligner is within a fiftieth on
/// an onset and can be a touch early on a trailing `s` or `k`; a quarter second is that, and not the
/// breath after. §10 files it as `P.eng.edgeTailMaxSeconds` (0.25, "word-fenced edge placement:
/// longest tail"; prototype `edgeTailMax`, gui/retake_edge.go), and `params::prepare()` rows it from
/// this constant. The level bound beside it is `P.eng.edgeTailDB`: twelve decibels says which sound
/// is still the word, this says how far to keep chasing it.
pub const EDGE_TAIL_MAX: f64 = 0.25;

/// How far into the gap the quietest moment is looked for. Past the word's own tail the gap is breath
/// and room; the cut goes where that is lowest, and a trough further off than this is a different gap.
pub const TROUGH_REACH: f64 = 0.4;

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
}

impl Edges {
    pub fn new(wave: Wave, off: f64) -> Self {
        Self { wave, off }
    }

    fn peaks(&self) -> &[u8] {
        self.wave.chans.first().map(Vec::as_slice).unwrap_or_default()
    }

    /// The bucket a session second falls in, or −1 outside the file.
    fn at(&self, t: f64) -> i64 {
        let i = ((t - self.off) * self.wave.hz) as i64;
        if i < 0 || i >= self.peaks().len() as i64 {
            return -1;
        }
        i
    }

    /// What counts as sound around a point: the quiet of that stretch of the recording, with room over
    /// it. Measured from the buckets around `t` rather than fixed, because a room, a microphone and a
    /// gain setting each put the silence somewhere else — and taken from the quieter end of the window,
    /// so a window that is mostly talking still finds its floor.
    pub fn floor(&self, t: f64) -> u8 {
        let i = self.at(t);
        if i < 0 {
            // Nothing to measure against: nothing can be sound.
            return u8::MAX;
        }
        let peaks = self.peaks();
        let reach = (ENVELOPE_WINDOW * self.wave.hz) as usize;
        let (lo, hi) = (i as usize - (i as usize).min(reach), (i as usize + reach).min(peaks.len()));
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
    ///
    /// The cache holds linear peaks — a byte is peak × 255 of full scale — while this threshold and
    /// [`Edges::floor`] read that byte as a −70…0 dBFS meter, so twelve dB is forty-four bytes. Known
    /// and kept: the placement scored against three hand-cut projects beat every alternative tried,
    /// including a real dB envelope, so the rewrite does not "fix" the scale.
    fn tail_level(&self, i0: i64, i1: i64) -> u8 {
        let peaks = self.peaks();
        let (start, end) = (i0.max(0) as usize, (i1.max(0) as usize).min(peaks.len().saturating_sub(1)));
        let peak = peaks[start..=end].iter().copied().max().unwrap_or(0);
        let below = EDGE_TAIL_DB * 255.0 / 70.0;
        let thr = i16::from(peak) - i16::try_from(below.round() as u32).unwrap_or(i16::MAX);
        let room = self.floor(self.off + f64::from(i32::try_from(i0.max(0)).unwrap_or(i32::MAX)) / self.wave.hz);
        thr.max(i16::from(room)).clamp(0, 255) as u8
    }

    /// Where the cut ends after a word that stays, given how far it may go (the next word's start, which
    /// it never reaches): the word's own sound followed to where it drops ([`EDGE_TAIL_MAX`]), then the
    /// quietest moment after that ([`TROUGH_REACH`]). The words fence it; the sound chooses inside.
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
        let lim = if lim < 0 { self.peaks().len() as i64 - 1 } else { lim };

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

    /// Where the last sound before this session second ends: the cut ends there. The second is the stamp
    /// of the first abandoned word, which may fall inside that word or just before it; either way the
    /// sound run it belongs to is stepped out of first, then the quiet before it, and the sound before
    /// THAT is what the cut keeps. Unchanged when no such edge is within reach — speech with no gap in it
    /// is cut where the stamp says.
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

    /// Where the retake's sound begins: the cut resumes there. A stamp runs LATE, never early, so the
    /// word is the sound run the stamp falls in or the one that ended just before it, and its onset is
    /// where the cut resumes. A stamp with quiet for a while before it is a stamp the sound has not
    /// reached yet and stands as it is: moving forward to the next sound would clip the very word it
    /// names.
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

/// The last word ending at or before this second.
fn last_word(words: &[AlignedWord], t: f64) -> Option<&AlignedWord> {
    words.iter().filter(|w| w.e <= t).max_by(|a, b| a.e.total_cmp(&b.e))
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

/// S1–S7 of the placement: every mark's two edges moved onto the sound.
///
/// `edge_of` answers with the envelope of the recording a session second belongs to — the spec leaves
/// open which recording that is, and the prototype resolves it from the rows spoken nearest the second,
/// so the caller owns that lookup and this only asks. `None` means no envelope: the word pad stands in
/// for the walk, and a stamp with no words at all stays where it was.
///
/// `retimeStrays` is deliberately not here: moving a word the aligner put on nothing belongs to the
/// retake pass that produced the mark (F1.9), not to placing an edge around one it already believes.
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
                // The words fence both edges and the sound chooses inside the fence. The envelope alone
                // cannot tell a breath from a word, and every deep breath heard at a join used to be kept.
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
                }
                if mark.again > mark.e && !words_between(words, mark.e, mark.again) {
                    // Nothing said between: it all goes, up to the retake's own first word.
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
                return mark;
            }

            // No word times: the envelope alone, which is all there is to place an edge by.
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
