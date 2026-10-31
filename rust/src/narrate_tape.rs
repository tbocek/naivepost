//! §07-narrate#1-screen **10**, the take band's tape: everything one narrator says in the cut, end to end. The band shows the stretches of every tagged recording that the cut KEEPS, in the cut's order.
//! A take stays stored in the seconds of its own recording (`narrate/takes.json`, §07-narrate#3-data); the tape only maps band seconds to (recording, second). Pure, so a test needs no window.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::cut::Cut;
use crate::narrate_data::Take;
use crate::timeline::Recording;

#[derive(Debug, Clone, PartialEq)]
pub struct Stretch {
    pub file: PathBuf,
    pub base: String,
    pub from: f64,
    pub to: f64,
    pub at: f64,
    pub session: Option<f64>,
}

impl Stretch {
    fn len(&self) -> f64 {
        (self.to - self.from).max(0.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tape(pub Vec<Stretch>);

/// The narrator's kept stretches. A kept footage clip on camera row N takes the part of every narrated recording on row N it overlaps ([`crate::timeline::rows_for`]), at that recording's own second ([`crate::timeline::local_at`], hand correction included).
pub fn kept(cut: &Cut, recordings: &[Recording], narrated: &[(String, PathBuf)]) -> Tape {
    let rows = crate::timeline::rows_for(recordings, cut);
    let mut stretches: Vec<Stretch> = Vec::new();
    let mut at = 0.0;
    for seg in cut
        .segs
        .iter()
        .filter(|seg| !seg.is_insert() && seg.e > seg.s)
    {
        for (recording, row) in recordings.iter().zip(&rows) {
            if seg.cam < 0 || *row != seg.cam as usize {
                continue;
            }
            let Some((_, file)) = narrated.iter().find(|(base, _)| *base == recording.base) else {
                continue;
            };
            let (s, e) = (seg.s.max(recording.start), seg.e.min(recording.end));
            if e <= s {
                continue;
            }
            let shift = crate::timeline::shift_of(cut, &recording.base);
            let from = crate::timeline::local_at(s, recording.start, 0.0, shift).max(0.0);
            let stretch = Stretch {
                file: file.clone(),
                base: recording.base.clone(),
                from,
                to: from + (e - s),
                at,
                session: Some(s),
            };
            at += stretch.len();
            stretches.push(stretch);
        }
    }
    Tape(stretches)
}

/// A recording shown whole: a voice that is one file with no kept clips (separate microphone, voices-folder file).
pub fn whole(file: PathBuf, seconds: f64) -> Tape {
    let base = crate::narrate_data::take_base(&file);
    Tape(vec![Stretch {
        file,
        base,
        from: 0.0,
        to: seconds.max(0.0),
        at: 0.0,
        session: None,
    }])
}

impl Tape {
    pub fn length(&self) -> f64 {
        self.0.last().map_or(0.0, |last| last.at + last.len())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn bases(&self) -> Vec<String> {
        let mut bases: Vec<String> = Vec::new();
        for stretch in &self.0 {
            if !bases.contains(&stretch.base) {
                bases.push(stretch.base.clone());
            }
        }
        bases
    }

    /// The session second under a band second; `None` past the tape's end or on a stretch no clip places.
    pub fn session_at(&self, band: f64) -> Option<f64> {
        let stretch = self
            .0
            .iter()
            .find(|stretch| band >= stretch.at && band < stretch.at + stretch.len())?;
        stretch.session.map(|start| start + band - stretch.at)
    }

    /// The band second under a session second. In a removed stretch the next kept one's start; past the last the tape's end; `None` when no clip places the tape.
    pub fn band_at(&self, session: f64) -> Option<f64> {
        let placed = || {
            self.0
                .iter()
                .filter_map(|stretch| stretch.session.map(|start| (stretch, start)))
        };
        if let Some((stretch, start)) =
            placed().find(|(stretch, start)| session >= *start && session < start + stretch.len())
        {
            return Some(stretch.at + session - start);
        }
        placed()
            .filter(|(_, start)| *start > session)
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(stretch, _)| stretch.at)
            .or_else(|| placed().next().map(|_| self.length()))
    }

    /// A span of band seconds as `(stretch, file from, file to, band from)` pieces; a span over a seam is two.
    pub fn pieces(&self, span: (f64, f64)) -> Vec<(&Stretch, f64, f64, f64)> {
        self.0
            .iter()
            .filter_map(|stretch| {
                let s = span.0.max(stretch.at);
                let e = span.1.min(stretch.at + stretch.len());
                (e > s).then_some({
                    (
                        stretch,
                        stretch.from + s - stretch.at,
                        stretch.from + e - stretch.at,
                        s,
                    )
                })
            })
            .collect()
    }

    /// The stored takes as the band shows them: parts inside kept stretches in band seconds, joined across seams.
    pub fn takes_on(&self, stored: &BTreeMap<String, Vec<Take>>) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = Vec::new();
        for stretch in &self.0 {
            for take in stored.get(&stretch.base).into_iter().flatten() {
                let (s, e) = (take.s.max(stretch.from), take.e.min(stretch.to));
                if e <= s {
                    continue;
                }
                let span = (stretch.at + s - stretch.from, stretch.at + e - stretch.from);
                match out.last_mut() {
                    Some(last) if (span.0 - last.1).abs() < 1e-6 => last.1 = span.1,
                    _ => out.push(span),
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    /// The takes to store after the band became `band`: per recording, what it keeps outside the tape's stretches (not the band's to drop) plus the band's takes inside them. Recordings not on the tape are not in the answer.
    pub fn store(
        &self,
        band: &[(f64, f64)],
        stored: &BTreeMap<String, Vec<Take>>,
    ) -> BTreeMap<String, Vec<Take>> {
        let mut out: BTreeMap<String, Vec<Take>> = BTreeMap::new();
        for base in self.bases() {
            let shown: Vec<(f64, f64)> = self
                .0
                .iter()
                .filter(|stretch| stretch.base == base)
                .map(|stretch| (stretch.from, stretch.to))
                .collect();
            let mut kept: Vec<Take> = Vec::new();
            for take in stored.get(&base).into_iter().flatten() {
                let mut rest = vec![(take.s, take.e)];
                for (from, to) in &shown {
                    rest = rest
                        .into_iter()
                        .flat_map(|(s, e)| {
                            let mut left = Vec::new();
                            if s < *from {
                                left.push((s, e.min(*from)));
                            }
                            if e > *to {
                                left.push((s.max(*to), e));
                            }
                            left
                        })
                        .collect();
                }
                kept.extend(rest.into_iter().map(|(s, e)| Take { s, e }));
            }
            out.insert(base, kept);
        }
        for &span in band {
            for (stretch, from, to, _) in self.pieces(span) {
                out.entry(stretch.base.clone())
                    .or_default()
                    .push(Take { s: from, e: to });
            }
        }
        out
    }
}
