//! Retake marking, spec/02-services.md §3.4.
//! The model names abandoned stretches; the app does the arithmetic. The tool result names the stretch that will actually be removed (after trimming to the words the later take repeats and placing edges on the audio), since the model's seconds are a guess. Refusals name themselves, merges are reported, and the ceiling comes back as a number.

use serde_json::json;

/// How far from the end of an attempt its replacement may be and still be that attempt's retake. P.machine.retakeReachSeconds
pub const RETAKE_REACH_SECONDS: f64 = 180.0;

/// A removal shorter than this is a breath, not an abandoned take. P.machine.retakeMinSeconds
pub const RETAKE_MIN_SECONDS: f64 = 0.3;

/// A pause worth drawing in the retake brief. P.machine.retakePauseSeconds
pub const RETAKE_PAUSE_SECONDS: f64 = 1.5;

/// Refused when more than this share of the session's speech is called abandoned.
/// P.machine.retakeCeil
pub const RETAKE_CEIL: f64 = 0.4;

/// Longest line still a broken-off fragment; the brief keeps a tail this long with what came before. P.machine.retakeFragmentSeconds
pub const RETAKE_FRAGMENT_SECONDS: f64 = 6.0;

/// How much of the later take is read for the repeat. P.machine.againReachSeconds
pub const AGAIN_REACH_SECONDS: f64 = 25.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub n: u32,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub from: u32,
    pub to: u32,
    /// What will actually be removed after trimming and edge placement; not `from`..`to`.
    pub removed: (f64, f64),
    /// Whether the trim moved either edge off the lines named.
    pub trimmed: bool,
    pub again: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Marks {
    lines: Vec<Line>,
    speech_seconds: f64,
    marks: Vec<Mark>,
}

impl Marks {
    /// A session of transcript lines. `speech_seconds` is the seconds actually spoken, not wall-clock length, since silence cannot be abandoned.
    pub fn new(lines: Vec<Line>, speech_seconds: f64) -> Marks {
        Marks {
            lines,
            speech_seconds,
            marks: Vec::new(),
        }
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// `mark_abandoned(from, to, again)`: ok with what will actually be removed and the running share, or a refusal. `again` `None` (model sends 0) is always legal.
    pub fn mark_abandoned(&mut self, from: u32, to: u32, again: Option<u32>, trim: Trim) -> String {
        self.mark_abandoned_at(from, to, again, trim, 0.0)
    }

    /// `mark_abandoned` with the attempt's onset handed in (§12 §2): seconds up to the retake's first word go with the abandoned take. `0.0` leaves the trim's own end as the end of the cut.
    pub fn mark_abandoned_at(
        &mut self,
        from: u32,
        to: u32,
        again: Option<u32>,
        trim: Trim,
        onset: f64,
    ) -> String {
        // The range must name lines that exist; checked first since every other message quotes these numbers.
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
            None => (None, 0.0),
            Some(n) => {
                let Some(line) = self.lines.iter().find(|line| line.n == n) else {
                    return crate::tools::error(&format!(
                        "line {n} is not in this session -- send 0 when the attempt was never \
                         picked up"
                    ));
                };
                // Gap after the end of what is abandoned; negative starts before, refused below.
                let gap = line.start - self.seconds(to).1;
                (Some(line), gap)
            }
        };

        // A line cannot be picked up inside the stretch that abandons it. §3.4 gives the wording.
        if let Some(line) = replacement {
            if line.n >= from && line.n <= to {
                return crate::tools::error(&format!(
                    "lines {from}-{to} say they are said again at line {}, which is inside them \
                     -- not a mark",
                    line.n
                ));
            }
            // The replacement must be the attempt's retake; exactly the reach is allowed.
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
        // Shorter than a breath is a stumble; cutting it costs more.
        if removed_end - removed_start < RETAKE_MIN_SECONDS {
            return crate::tools::error(&format!(
                "lines {from}-{to} leave {:.2} s to remove -- under {RETAKE_MIN_SECONDS} s that is \
                 a breath, not an attempt",
                removed_end - removed_start
            ));
        }

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
            "seconds": removed_end - removed_start,
            "trimmed": trimmed,
            "merged": merged,
            "again": again,
            "speech_marked": share,
            "ceiling": RETAKE_CEIL,
            // §12 §2: the PLAYED cut loses everything up to the picked-up attempt's onset (`Retake::to`), so the model gets the wider pair in the same answer. The caller passes `onset`; without it the trim's end is the cut's end.
            "cut_to_seconds": onset.max(removed_end),
            // §12 §3: each verdict carries a one-sentence reason built from what this call did.
            "why": mark_reason(again, trimmed, merged),
        }))
    }

    /// `unmark(from, to)`: a mark overlapping the range loses that stretch.
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

        let mut taken = 0.0;
        let mut kept: Vec<Mark> = Vec::new();
        for mark in std::mem::take(&mut self.marks) {
            let (ms, me) = mark.removed;
            taken += overlap(ms, me, start, end);
            for piece in subtract((ms, me), (start, end)) {
                kept.push(Mark {
                    removed: piece,
                    ..mark.clone()
                });
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

    /// `get_lines(from, to)`: the lines again with the pause before each, including those under [`RETAKE_PAUSE_SECONDS`] (§3.4), where a speaker restarts.
    pub fn lines_with_pause(&self, from: u32, to: u32) -> Vec<(&Line, f64)> {
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

    pub fn finish(&self) -> Total {
        let share = self.share();
        Total {
            share,
            ceiling: RETAKE_CEIL,
            over: share > RETAKE_CEIL,
            marks: self.marks.len(),
        }
    }

    fn share(&self) -> f64 {
        if self.speech_seconds <= 0.0 {
            return 0.0;
        }
        let marked: f64 = self
            .marks
            .iter()
            // Marks can touch after a merge, so each second is counted once.
            .map(|mark| (mark.removed.1 - mark.removed.0).max(0.0))
            .sum::<f64>()
            / self.speech_seconds;
        marked.min(1.0)
    }

    /// Seconds of speech the marks take out, each second once.
    pub fn marked_seconds(&self) -> f64 {
        self.marks
            .iter()
            .map(|mark| (mark.removed.1 - mark.removed.0).max(0.0))
            .sum::<f64>()
    }

    pub fn speech_seconds(&self) -> f64 {
        self.speech_seconds
    }

    fn seconds(&self, n: u32) -> (f64, f64) {
        match self.lines.iter().find(|line| line.n == n) {
            Some(line) => (line.start, line.end),
            None => (0.0, 0.0),
        }
    }
}

/// What a mark leaves once the repeated tail is trimmed and edges placed; supplied by the caller (word matching, waveform).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trim {
    pub lead: f64,
    pub tail: f64,
}

impl Trim {
    pub fn none() -> Trim {
        Trim {
            lead: 0.0,
            tail: 0.0,
        }
    }

    /// The stretch actually removed; each side is clamped so a trim cannot swallow the mark.
    pub fn apply(&self, start: f64, end: f64) -> (f64, f64) {
        let mid = (start + end) / 2.0;
        let out_start = (start + self.lead).min(mid);
        let out_end = (end - self.tail).max(mid);
        (out_start, out_end.max(out_start))
    }
}

/// The one-sentence reason §12 §3 wants with every accepted mark ("whole take, never picked up", "merged into the mark already there").
fn mark_reason(again: Option<u32>, trimmed: bool, merged: bool) -> &'static str {
    if merged {
        return "this stretch was already marked; the two became one";
    }
    if trimmed {
        return "only the words the later take repeats were taken out; the rest of the lines stand";
    }
    match again {
        Some(_) => {
            "every second from the first word of the abandoned take to the retake's onset is cut"
        }
        None => "the whole take is treated as never picked up, so its own words are all that goes",
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Total {
    pub share: f64,
    pub ceiling: f64,
    /// Over the ceiling and still holding every mark; the model can `unmark`.
    pub over: bool,
    pub marks: usize,
}

/// S1: what one unusable marking pass says and what the pass does about it. `count` is the runs still standing.
pub fn run_set_aside_log(run: usize, reason: &str, count: usize) -> Vec<String> {
    vec![
        format!(
            "!!! retakes: run {}: {reason} -- its answer is set aside",
            run + 1
        ),
        format!(">>> retakes: going on with {count}"),
    ]
}

pub fn cached_runs_log(cached: usize, total: usize) -> String {
    format!(">>> retakes: {cached} of {total} run(s) answered from the cache")
}

/// S1: a session with nothing said twice; said aloud since silence looks like a pass that never ran.
pub fn no_retakes_log() -> &'static str {
    ">>> retakes: none"
}

#[allow(
    dead_code,
    reason = "the method below is what a caller uses; this is the same rule once"
)]
pub fn pause_before(lines: &[Line], n: u32) -> f64 {
    let Some(line) = lines.iter().find(|line| line.n == n) else {
        return 0.0;
    };
    match lines.iter().find(|other| other.n + 1 == line.n) {
        Some(previous) => (line.start - previous.end).max(0.0),
        None => 0.0,
    }
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

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
