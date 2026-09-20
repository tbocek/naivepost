//! Retake marking — spec/02-services.md §3.4.
//!
//! The model says which stretches of the transcript were abandoned; everything else about a mark is
//! arithmetic on lines and audio that only the app can do. That asymmetry is what the tool's result
//! has to carry: an ok naming *the stretch that will actually be removed* — after trimming to the
//! words the later take repeats and placing the edges on the audio — because the seconds the model
//! sent are its guess at where the attempt ends, not what gets cut.
//!
//! The prototype ran this in three pooled runs and told the model nothing about any of it: each mark
//! was trimmed by a fuzzy matcher, overlapping marks were merged, and over the ceiling every mark
//! from all three runs was thrown away. Here a refusal names itself, a merge is reported, and the
//! ceiling comes back as a number while there is still time to take something back.

use serde_json::json;

/// How far from the end of an attempt its replacement may be and still be that attempt's retake.
/// P.machine.retakeReachSeconds
pub const RETAKE_REACH_SECONDS: f64 = 180.0;

/// A removal shorter than this is a breath, not an abandoned take. P.machine.retakeMinSeconds
pub const RETAKE_MIN_SECONDS: f64 = 0.3;

/// A pause worth drawing in the retake brief — and the threshold the brief left out, since gaps
/// under it are exactly where a speaker restarts. P.machine.retakePauseSeconds
pub const RETAKE_PAUSE_SECONDS: f64 = 1.5;

/// Refused when more than this share of the session's speech is called abandoned.
/// P.machine.retakeCeil
pub const RETAKE_CEIL: f64 = 0.4;

/// Longest line still a broken-off fragment rather than somebody starting a new thought — the retake
/// brief keeps a tail this long with what came before it. P.machine.retakeFragmentSeconds
pub const RETAKE_FRAGMENT_SECONDS: f64 = 6.0;

/// How much of the later take is read for the repeat, so a mark's start can be pulled back to where the
/// attempt stops being said again. P.machine.againReachSeconds
pub const AGAIN_REACH_SECONDS: f64 = 25.0;

/// One transcript line, as a mark sees it: its number and where it sits in time.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The number the model is shown and sends back.
    pub n: u32,
    pub start: f64,
    pub end: f64,
}

/// One marked stretch of the session, in line numbers and in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// The first abandoned line, as the model named it.
    pub from: u32,
    /// The last one.
    pub to: u32,
    /// What will actually be removed, after trimming to the words the later take repeats and placing
    /// the edges on the audio. Not `from`..`to`: a mark whose tail is repeated by the retake leaves
    /// that tail standing, and cutting where the model pointed would cut words that were meant.
    pub removed: (f64, f64),
    /// Whether the trim moved either edge off the lines named — which is what tells the model its
    /// numbers were understood rather than obeyed.
    pub trimmed: bool,
    /// The line the attempt is said again at; `None` for "never picked up" (`again` 0).
    pub again: Option<u32>,
}

/// A session's marks, and the speech total they are measured against.
#[derive(Debug, Clone, PartialEq)]
pub struct Marks {
    /// Every line of the session, in order — the range a mark has to fall inside.
    lines: Vec<Line>,
    /// Seconds spoken in the whole session: what the running share is a share *of*.
    speech_seconds: f64,
    marks: Vec<Mark>,
}

impl Marks {
    /// A session of transcript lines. `speech_seconds` is what the ceiling is measured against — the
    /// seconds actually spoken, not the session's wall-clock length, because silence cannot be
    /// abandoned and counting it would let a long recording absorb any number of marks.
    pub fn new(lines: Vec<Line>, speech_seconds: f64) -> Marks {
        Marks {
            lines,
            speech_seconds,
            marks: Vec::new(),
        }
    }

    /// The session's lines.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// The marks so far, in the order they were made.
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// `mark_abandoned(from, to, again)` — ok with what will actually be removed and the running
    /// share of speech now marked, or a refusal naming the reason.
    ///
    /// `again` is `None` for "never picked up" (the model sends 0): always legal, because a take that
    /// was simply dropped has no replacement to be near.
    pub fn mark_abandoned(
        &mut self,
        from: u32,
        to: u32,
        again: Option<u32>,
        trim: Trim,
    ) -> String {
        // The range has to name lines that exist. Checked first because every other message quotes
        // these numbers, and a refusal about line 900 of 40 should not also explain a reach rule.
        let (Some(first), Some(last)) = (self.lines.first(), self.lines.last()) else {
            return crate::tools::error("this session has no lines to mark");
        };
        if from < first.n || to > last.n || from > to {
            return crate::tools::error(&format!(
                "lines {from}-{to} are out of range -- this session runs {}..={}",
                first.n, last.n
            ));
        }
        let (replacement, replacement_gap) = match again {
            // `again` 0 means never picked up: no line to point at and no distance to check.
            None => (None, 0.0),
            Some(n) => {
                let Some(line) = self.lines.iter().find(|line| line.n == n) else {
                    return crate::tools::error(&format!(
                        "line {n} is not in this session -- send 0 when the attempt was never \
                         picked up"
                    ));
                };
                // How far the replacement starts after the end of what is being abandoned. A
                // negative gap means it starts before, which the inside check below refuses.
                let gap = line.start - self.seconds(to).1;
                (Some(line), gap)
            }
        };

        // A line cannot be picked up inside the stretch that abandons it: the mark would remove the
        // very lines it says are said again. §3.4 gives the wording; only the numbers change.
        if let Some(line) = replacement {
            if line.n >= from && line.n <= to {
                return crate::tools::error(&format!(
                    "lines {from}-{to} say they are said again at line {}, which is inside them \
                     -- not a mark",
                    line.n
                ));
            }
            // The replacement has to be the attempt's retake, not some later passage that happens to
            // repeat a phrase. Exactly the reach is allowed: it is the limit, not a tolerance to
            // round away.
            if replacement_gap > RETAKE_REACH_SECONDS {
                return crate::tools::error(&format!(
                    "line {} starts {:.1} s after line {to} ends -- a replacement is at most \
                     {RETAKE_REACH_SECONDS:.0} s away, so this is not the same attempt",
                    line.n, replacement_gap
                ));
            }
        }

        let edges = self.seconds(from).0..self.seconds(to).1;
        let (removed_start, removed_end) = trim.apply(edges.start, edges.end);
        // A removal shorter than a breath is not an abandoned take: it is the model pointing at a
        // stumble, and cutting it costs more than leaving it in.
        if removed_end - removed_start < RETAKE_MIN_SECONDS {
            return crate::tools::error(&format!(
                "lines {from}-{to} leave {:.2} s to remove -- under {RETAKE_MIN_SECONDS} s that is \
                 a breath, not an attempt",
                removed_end - removed_start
            ));
        }

        // Overlapping marks merge. The prototype merged in silence; here the ok says it happened,
        // because a model that sees two stretches become one knows its second mark was redundant.
        let mut trimmed = trim.lead > 0.0 || trim.tail > 0.0;
        let mut merged = false;
        let mut into: Option<usize> = None;
        for (index, mark) in self.marks.iter().enumerate() {
            if removed_start <= mark.removed.1 && removed_end >= mark.removed.0 {
                into = Some(index);
                break;
            }
        }
        if let Some(index) = into {
            let existing = &self.marks[index];
            let start = removed_start.min(existing.removed.0);
            let end = removed_end.max(existing.removed.1);
            trimmed |= (start - edges.start).abs() > 0.0 || (end - edges.end).abs() > 0.0;
            self.marks[index].removed = (start, end);
            self.marks[index].trimmed = trimmed;
            merged = true;
        } else {
            self.marks.push(Mark {
                from,
                to,
                removed: (removed_start, removed_end),
                trimmed,
                again,
            });
        }

        let share = self.share();
        crate::tools::ok(&json!({
            "lines": [from, to],
            "removed": [removed_start, removed_end],
            "seconds": (removed_end - removed_start) as f64,
            "trimmed": trimmed,
            "merged": merged,
            "again": again,
            "speech_marked": share,
            "ceiling": RETAKE_CEIL,
        }))
    }

    /// `unmark(from, to)` — ok. A mark overlapping the range loses that stretch; taking back part of
    /// a mark is how a model comes back under the ceiling without losing the rest of its work.
    pub fn unmark(&mut self, from: u32, to: u32) -> String {
        let (Some(first), Some(last)) = (self.lines.first(), self.lines.last()) else {
            return crate::tools::error("this session has no lines to unmark");
        };
        if from < first.n || to > last.n || from > to {
            return crate::tools::error(&format!(
                "lines {from}-{to} are out of range -- this session runs {}..={}",
                first.n, last.n
            ));
        }
        let start = self.seconds(from).0;
        let end = self.seconds(to).1;

        // Cut the asked stretch out of every mark it touches, keeping whatever is left on either side.
        let mut taken = 0.0;
        let mut kept: Vec<Mark> = Vec::new();
        for mark in std::mem::take(&mut self.marks) {
            let (ms, me) = mark.removed;
            taken += overlap(ms, me, start, end);
            for piece in subtract((ms, me), (start, end)) {
                kept.push(Mark { removed: piece, ..mark.clone() });
            }
        }
        self.marks = kept;
        let share = self.share();
        crate::tools::ok(&json!({
            "unmarked": [from, to],
            "seconds_taken_back": taken,
            "speech_marked": share,
            "ceiling": RETAKE_CEIL,
        }))
    }

    /// `get_lines(from, to)` — the lines again, each with the pause before it in seconds.
    ///
    /// Including the short ones: §3.4 calls out that the brief omits pauses under
    /// [`RETAKE_PAUSE_SECONDS`], and those are exactly where a speaker stops, breathes and starts the
    /// sentence over again — the shape of an abandoned take.
    pub fn lines_with_pause<'a>(&'a self, from: u32, to: u32) -> Vec<(&'a Line, f64)> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(index, line)| {
                let _ = index;
                line.n >= from && line.n <= to
            })
            .map(|(_, line)| {
                let pause = match self.lines.iter().find(|other| other.n + 1 == line.n) {
                    Some(previous) => (line.start - previous.end).max(0.0),
                    None => 0.0,
                };
                (line, pause)
            })
            .collect()
    }

    /// `finish` — the total marked against the ceiling, so a model over it can take some back.
    pub fn finish(&self) -> Total {
        let share = self.share();
        Total {
            share,
            ceiling: RETAKE_CEIL,
            over: share > RETAKE_CEIL,
            marks: self.marks.len(),
        }
    }

    /// The running share of the session's speech now marked.
    fn share(&self) -> f64 {
        if self.speech_seconds <= 0.0 {
            return 0.0;
        }
        let marked: f64 = self
            .marks
            .iter()
            // Marks can touch after a merge attempt, so the total counts each second once rather
            // than adding two overlapping stretches twice and reporting a share over 1.
            .map(|mark| (mark.removed.1 - mark.removed.0).max(0.0))
            .sum::<f64>()
            / self.speech_seconds;
        marked.min(1.0)
    }

    /// The session seconds a line number spans.
    fn seconds(&self, n: u32) -> (f64, f64) {
        match self.lines.iter().find(|line| line.n == n) {
            Some(line) => (line.start, line.end),
            None => (0.0, 0.0),
        }
    }
}

/// What a model's mark leaves once the repeated tail is trimmed and the edges are placed on the
/// audio. Supplied by the caller because both halves are work this module must not invent: the trim
/// comes from matching the later take's words, the placement from the waveform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trim {
    /// Seconds cut off the front — an edge slid to the nearest silence.
    pub lead: f64,
    /// Seconds cut off the back — the words the later take repeats.
    pub tail: f64,
}

impl Trim {
    /// Nothing trimmed: the mark removes exactly the lines named.
    pub fn none() -> Trim {
        Trim { lead: 0.0, tail: 0.0 }
    }

    /// The stretch that will actually be removed. Each side is clamped so a trim cannot swallow the
    /// whole mark and leave a negative span — which would otherwise read as an empty removal rather
    /// than as the refusal the length rule is there to give.
    pub fn apply(&self, start: f64, end: f64) -> (f64, f64) {
        let mid = (start + end) / 2.0;
        let out_start = (start + self.lead).min(mid);
        let out_end = (end - self.tail).max(mid);
        (out_start, out_end.max(out_start))
    }
}

/// The answer `finish` gives: the share of speech marked against the ceiling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Total {
    pub share: f64,
    pub ceiling: f64,
    /// Over the ceiling — and still holding every mark. The prototype threw all three runs away at
    /// this point; here the model is told and can take some back with `unmark`.
    pub over: bool,
    pub marks: usize,
}

/// S1: what one marking pass that came back unusable says, and what the pass then does about it. The
/// three runs are pooled because they cost one request each and answer one question; losing one is not
/// a reason to lose the marks the other two agreed on — hence "going on with N" rather than a failure.
/// `count` is the runs still standing after this one was set aside.
pub fn run_set_aside_log(run: usize, reason: &str, count: usize) -> Vec<String> {
    vec![
        format!("!!! retakes: run {}: {reason} -- its answer is set aside", run + 1),
        format!(">>> retakes: going on with {count}"),
    ]
}

/// S1: how many of this session's pooled runs were read back from `cache/llm` instead of asked again.
pub fn cached_runs_log(cached: usize, total: usize) -> String {
    format!(">>> retakes: {cached} of {total} run(s) answered from the cache")
}

/// S1: a session with nothing said twice. Said out loud because silence looks like a pass that never
/// ran, and "no retakes" is the answer the model gave — not an absence of one.
pub fn no_retakes_log() -> &'static str {
    ">>> retakes: none"
}

/// A line with the pause before it, as `get_lines` returns them.
#[allow(dead_code, reason = "the method below is what a caller uses; this is the same rule once")]
pub fn pause_before(lines: &[Line], n: u32) -> f64 {
    let Some(line) = lines.iter().find(|line| line.n == n) else {
        return 0.0;
    };
    match lines.iter().find(|other| other.n + 1 == line.n) {
        Some(previous) => (line.start - previous.end).max(0.0),
        None => 0.0,
    }
}

/// The overlap of two spans in seconds.
fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// `span` with `cut` taken out of it: nothing, one piece, or the two pieces either side.
fn subtract(span: (f64, f64), cut: (f64, f64)) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    if cut.0 > span.0 {
        out.push((span.0, span.0.max(cut.0)));
    }
    if cut.1 < span.1 {
        out.push((span.1.min(cut.1), span.1));
    }
    out.into_iter().filter(|piece| piece.1 > piece.0).collect()
}
