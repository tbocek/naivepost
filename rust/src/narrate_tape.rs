//! §07-narrate#1-screen **10**, the take band's tape: everything one narrator says in the cut, end to end. A
//! narrator is tagged on as many recordings as they speak in, and the band shows the stretches of those recordings
//! the cut KEEPS (the green bars of the Cut page), in the cut's order, one after another; the first rewrite showed
//! the first tagged recording alone, so a session filmed in nineteen files offered half a minute of it to pick
//! takes from.
//!
//! A take is still stored in the seconds of the recording it was cut from, under that recording's name
//! (`narrate/takes.json`, §07-narrate#3-data): the tape only maps band seconds to (recording, second) and back.
//! Pure: the recordings, their rows and the cut come in, so a test pins the mapping without a window.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::cut::Cut;
use crate::narrate_data::Take;
use crate::timeline::Recording;

/// One kept stretch of one recording on the tape: the file's own seconds `from..to`, laid at band second `at`.
#[derive(Debug, Clone, PartialEq)]
pub struct Stretch {
    pub file: PathBuf,
    /// The recording's name, the key its takes are stored under.
    pub base: String,
    pub from: f64,
    pub to: f64,
    pub at: f64,
}

impl Stretch {
    fn len(&self) -> f64 {
        (self.to - self.from).max(0.0)
    }
}

/// The stretches, in band order, each starting where the one before ended.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tape(pub Vec<Stretch>);

/// The narrator's kept stretches. `recordings` are the session's recordings (their spans on the session clock)
/// and `narrated` the ones tagged with this narrator, by name, with their files. A kept footage clip on camera
/// row N takes the part of every narrated recording on row N that it overlaps (the recording the cut shows there,
/// the same rows [`crate::timeline::rows_for`] colours the Cut page with), at that recording's own second
/// ([`crate::timeline::local_at`], hand correction included).
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
            };
            at += stretch.len();
            stretches.push(stretch);
        }
    }
    Tape(stretches)
}

/// A recording shown whole: the tape of a voice that is one file and no kept clips (a separate microphone, a
/// voices-folder file), `seconds` long.
pub fn whole(file: PathBuf, seconds: f64) -> Tape {
    let base = crate::narrate_data::take_base(&file);
    Tape(vec![Stretch {
        file,
        base,
        from: 0.0,
        to: seconds.max(0.0),
        at: 0.0,
    }])
}

impl Tape {
    /// The band's length: every stretch, end to end.
    pub fn length(&self) -> f64 {
        self.0.last().map_or(0.0, |last| last.at + last.len())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The recordings on the tape, each once, in band order.
    pub fn bases(&self) -> Vec<String> {
        let mut bases: Vec<String> = Vec::new();
        for stretch in &self.0 {
            if !bases.contains(&stretch.base) {
                bases.push(stretch.base.clone());
            }
        }
        bases
    }

    /// A span of band seconds as the pieces of recordings it covers: `(stretch, file from, file to, band from)`,
    /// in band order. A span over a seam between two stretches is two pieces.
    pub fn pieces(&self, span: (f64, f64)) -> Vec<(&Stretch, f64, f64, f64)> {
        self.0
            .iter()
            .filter_map(|stretch| {
                let s = span.0.max(stretch.at);
                let e = span.1.min(stretch.at + stretch.len());
                (e > s).then(|| {
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

    /// The stored takes as the band shows them: each take's part inside a kept stretch, in band seconds, the parts
    /// that meet across a seam joined into one.
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

    /// The takes to store after the band's takes became `band`: for every recording on the tape, what it keeps
    /// outside the tape's stretches (picked before the cut changed, and not the band's to drop) and the band's
    /// takes inside them, in that recording's seconds. Recordings the tape does not show are not in the answer.
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
                // What lies outside every shown stretch survives, cut at their edges.
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
