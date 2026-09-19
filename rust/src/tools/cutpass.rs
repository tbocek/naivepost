//! §02-services.md §3 tool catalogue, rewrite directive B: the cut pass's own tools.
//!
//! The model is shown a batch of candidate segments and asked for the keep list; these five tools are how
//! it answers. They mirror what `cut.json` holds (see [`crate::cut`]), so the numbers that come back from
//! a tool are the ones written to disk — but they do not touch the file, which is the caller's, and none
//! of them reach a server or a process.
//!
//! A segment is addressed by its session `start`, never by a position in a list: §3.6 sends `start` for
//! both `remove_segment` and `set_speed`, and an index would change under the model's feet the moment one
//! of its own calls added or removed something.

use serde_json::json;

/// The fastest a rate may go, and therefore how much footage a target can swallow: footage under the
/// window's floor cannot fill the target at any rate, and footage over `floor × MAX_RATE` could not be
/// squeezed into it even at this rate. 4 and not 8 — eight is for a minute of loading screen, and a
/// ceiling built on it would accept cuts nothing could make watchable. P.policy.maxSpeedRate
pub const MAX_RATE: f64 = 4.0;

/// A stretch shorter than this on screen is not worth a rate of its own: the render cannot cut finer, so
/// a rate that lands under it is lowered until it does. P.eng.minClipSeconds — "also the speed clamp
/// floor", which is why it is this pass's bound and not only the renderer's.
pub const MIN_CLIP_SECONDS: f64 = 0.5;

/// The shortest stretch worth keeping. A segment under it is accepted and reported rather than refused —
/// §3.6 asks for "whether it survives P.policy.minSceneSeconds" — because a model that has just chosen a
/// moment learns more from being told it is short than from a rejection. It is `finish_cut` that refuses.
/// P.policy.minSceneSeconds
pub const MIN_SCENE_SECONDS: f64 = 1.0;

/// How far an edge may move to a word edge, silence or visual cut. The caller may override it per plan.
/// P.policy.snapToleranceSeconds
pub const SNAP_TOLERANCE_SECONDS: f64 = 5.0;

/// A target up to this is a format and its footage window narrows; above it the target is a wish and the
/// window widens. P.policy.shortTargetSeconds
pub const SHORT_TARGET_SECONDS: f64 = 60.0;

/// Seconds as the model will read them — [`crate::tools::mm_ss`] under this module's name, so a cut
/// message and one of these error sentences cannot drift into two readings of the same number. Past an
/// hour the minutes keep counting: `clock(3725.0)` is `62:05`.
pub fn clock(seconds: f64) -> String {
    crate::tools::mm_ss(seconds)
}

/// A segment the model has asked for, as it was placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    /// The rate applied to this segment, 1.0 for none. `set_speed` is what changes it.
    pub speed: f64,
    /// The model's own reason for keeping it, kept so a caller can show the choice beside the cut.
    pub why: String,
}

impl Segment {
    fn length(&self) -> f64 {
        self.end - self.start
    }

    /// Seconds this segment spends on screen: its own length divided by the rate it plays at.
    fn on_screen(&self) -> f64 {
        self.length() / self.speed.max(f64::EPSILON)
    }
}

/// What `cut_status` reports — and what `finish_cut` judges, at any time. One reading, computed once.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    /// Seconds kept on screen by everything added so far.
    pub footage: f64,
    /// The accepted window for the target, as [`footage_window`] gives it.
    pub window: (f64, f64),
    pub segments: usize,
    pub min_segments: usize,
    pub max_segments: usize,
    /// Seconds of retake mark still to come out of what is kept.
    pub marks_pending: f64,
    /// Seconds of dead air the speed pass still owes.
    pub dead_air_pending: f64,
    /// Segments under [`MIN_SCENE_SECONDS`] on screen, as (start, seconds on screen).
    pub short_scenes: Vec<(f64, f64)>,
}

/// A cut being built: the session's footage, what has been added, and what is still to come out.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Seconds of footage in the session — the end a segment may not be placed past.
    footage_end: f64,
    /// The length the cut aims at (`P.policy.targetLengthSeconds`; 0 means none).
    target: f64,
    /// How far an edge may move to a word edge, silence or visual cut.
    snap_tolerance: f64,
    segments: Vec<Segment>,
    /// Seconds of retake mark still not reflected in any segment.
    marks_pending: f64,
    /// Seconds of dead air the speed pass has not yet taken out.
    dead_air_pending: f64,
    /// Spans with no footage under them — an edge may not be dropped inside one.
    gaps: Vec<(f64, f64)>,
    /// Where an edge is allowed to land: word edges, silences and visual cuts.
    snap_points: Vec<f64>,
    /// Retake marks over the session, so `add_segment` can say what one will take out of a segment.
    marks: Vec<(f64, f64)>,
}

impl Plan {
    /// A plan for a session of `footage_end` seconds aiming at `target`.
    pub fn new(footage_end: f64, target: f64) -> Plan {
        Plan {
            footage_end,
            target,
            snap_tolerance: SNAP_TOLERANCE_SECONDS,
            segments: Vec::new(),
            marks_pending: 0.0,
            dead_air_pending: 0.0,
            gaps: Vec::new(),
            snap_points: Vec::new(),
            marks: Vec::new(),
        }
    }

    /// `add_segment(start, end, why)` — ok with what the segment became, or an error naming the problem in
    /// seconds the model itself used.
    pub fn add_segment(&mut self, start: f64, end: f64, why: &str) -> String {
        // Ends before anything else: every other complaint would be about a range that does not exist.
        if end < start {
            return crate::tools::error(&format!(
                "your segment ends at {} and starts at {}, so it has no length",
                clock(end),
                clock(start)
            ));
        }
        if start < 0.0 || end > self.footage_end {
            // The model's own number leads, and it is the one that fell outside: `start` when the segment
            // begins before the footage does, its end otherwise. Quoting the other timestamp instead would
            // send the model off to correct a number that was never wrong.
            let outside = if start < 0.0 { start } else { end };
            return crate::tools::error(&format!(
                "{} is past the end of the footage, which runs to {} -- keep your segment inside \
                 0 to {}",
                clock(outside),
                clock(self.footage_end),
                clock(self.footage_end)
            ));
        }
        // No footage under an edge: the caller says where the gaps are, this module only refuses to place
        // a segment that would spend its length on seconds nobody filmed.
        if let Some(gap) = self
            .gaps
            .iter()
            .find(|(from, to)| placed_in_gap(*from, *to, start, end))
        {
            return crate::tools::error(&format!(
                "{} to {} has no footage under it -- there is a gap from {} to {}",
                clock(start),
                clock(end),
                clock(gap.0),
                clock(gap.1)
            ));
        }
        let placed = self.snap((start, end));
        // Checked after the snap, so an edge moved by tolerance cannot smuggle an overlap past it.
        // Touching is not overlapping: this pass joins takes, and a cut lands on a frame.
        if let Some(other) = self
            .segments
            .iter()
            .find(|other| placed.0 < other.end && other.start < placed.1)
        {
            return crate::tools::error(&format!(
                "{} to {} overlaps the segment already added at {} to {}",
                clock(placed.0),
                clock(placed.1),
                clock(other.start),
                clock(other.end)
            ));
        }
        let mark = self.mark_seconds(placed.0, placed.1);
        let survives = placed.1 - placed.0 >= MIN_SCENE_SECONDS;
        self.segments.push(Segment {
            start: placed.0,
            end: placed.1,
            speed: 1.0,
            why: why.to_string(),
        });
        self.segments.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let status = self.status();
        crate::tools::ok(&json!({
            // The edges as placed, and how far each moved to get there.
            "span": [placed.0, placed.1],
            "moved": [placed.0 - start, placed.1 - end],
            "seconds": placed.1 - placed.0,
            // What a retake mark over this segment will take out of it.
            "mark_seconds": mark,
            "survives_min_scene": survives,
            // The running total against the window: the model is choosing the whole cut, one segment at
            // a time, and cannot add up what it has asked for so far.
            "footage": status.footage,
            "window": [status.window.0, status.window.1],
        }))
    }

    /// `remove_segment(start)` — ok. A segment is named by where it starts, because a position in a list
    /// would move under the model between one call and the next.
    pub fn remove_segment(&mut self, start: f64) -> String {
        let Some(at) = self.segments.iter().position(|segment| {
            (segment.start - start).abs() <= self.snap_tolerance.max(f64::EPSILON)
        }) else {
            return crate::tools::error(&format!(
                "there is no segment starting at {} -- the cut holds {} segment(s), at {}",
                clock(start),
                self.segments.len(),
                self.starts()
            ));
        };
        let gone = self.segments.remove(at);
        crate::tools::ok(&json!({
            "start": gone.start,
            "end": gone.end,
            "seconds": gone.length(),
            "segments_left": self.segments.len(),
        }))
    }

    /// `set_speed(start, rate)` — ok with the rate as applied and the resulting on-screen length.
    pub fn set_speed(&mut self, start: f64, rate: f64) -> String {
        let Some(at) = self.segments.iter().position(|segment| {
            (segment.start - start).abs() <= self.snap_tolerance.max(f64::EPSILON)
        }) else {
            return crate::tools::error(&format!(
                "there is no segment starting at {} -- the cut holds {} segment(s), at {}",
                clock(start),
                self.segments.len(),
                self.starts()
            ));
        };
        let seconds = self.segments[at].length();
        let applied = apply_rate(rate, seconds);
        self.segments[at].speed = applied;
        crate::tools::ok(&json!({
            "start": self.segments[at].start,
            "asked": rate,
            "applied": applied,
            "seconds": seconds,
            "on_screen": seconds / applied.max(f64::EPSILON),
            // Why the number is not the one asked for, when it is not: §3 says a change the model did not
            // ask for MUST be said, never silently dropped.
            "reason": rate_reason(rate, applied, seconds),
        }))
    }

    /// The segments in timeline order.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// `cut_status` — the reading [`Plan::finish_cut`] would give, at any time.
    pub fn status(&self) -> Status {
        Status {
            footage: self.footage(),
            window: footage_window(self.target),
            segments: self.segments.len(),
            min_segments: min_segments(self.target),
            max_segments: max_segments(self.target),
            marks_pending: self.marks_pending,
            dead_air_pending: self.dead_air_pending,
            short_scenes: self
                .segments
                .iter()
                .filter(|segment| segment.on_screen() < MIN_SCENE_SECONDS)
                .map(|segment| (segment.start, segment.on_screen()))
                .collect(),
        }
    }

    /// `cut_status` as the tool answers it — [`Plan::status`] serialised, so the two cannot disagree.
    pub fn status_json(&self) -> String {
        let status = self.status();
        crate::tools::ok(&json!({
            "footage": status.footage,
            "window": [status.window.0, status.window.1],
            "segments": status.segments,
            "min_segments": status.min_segments,
            "max_segments": status.max_segments,
            "marks_pending": status.marks_pending,
            "dead_air_pending": status.dead_air_pending,
        }))
    }

    /// `finish_cut` — the whole-cut checks as a problem list, worst first, or ok.
    pub fn finish_cut(&self) -> String {
        let status = self.status();
        let problems = cut_problems(&status);
        if !problems.is_empty() {
            return crate::tools::error(&problems.join(". "));
        }
        crate::tools::ok(&json!({
            "footage": status.footage,
            "window": [status.window.0, status.window.1],
            "segments": status.segments,
        }))
    }

    /// Seconds kept on screen by everything added so far.
    fn footage(&self) -> f64 {
        self.segments.iter().map(Segment::on_screen).sum()
    }

    /// The segments' starts as a sentence, for an error that has to say what the cut does hold.
    fn starts(&self) -> String {
        if self.segments.is_empty() {
            return "nothing yet".to_string();
        }
        self.segments
            .iter()
            .map(|segment| clock(segment.start))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Seconds of retake mark lying inside a segment: what the cut will take out of it later.
    fn mark_seconds(&self, start: f64, end: f64) -> f64 {
        self.marks
            .iter()
            .map(|(from, to)| {
                (*to).min(end) - (*from).max(start)
            })
            .filter(|seconds| *seconds > 0.0)
            .sum()
    }

    /// Move an edge to the nearest allowed point within tolerance. With nothing to snap to — no transcript,
    /// no ffprobe run — the edges stand exactly where they were put.
    fn snap(&self, placed: (f64, f64)) -> (f64, f64) {
        if self.snap_points.is_empty() {
            return placed;
        }
        let near = |edge: f64| -> f64 {
            *self
                .snap_points
                .iter()
                .filter(|point| (**point - edge).abs() <= self.snap_tolerance)
                .min_by(|a, b| {
                    (**a - edge)
                        .abs()
                        .partial_cmp(&(**b - edge).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(&edge)
        };
        let start = near(placed.0);
        let end = near(placed.1);
        // A snap may move an edge, not reverse the segment or shrink it out of existence.
        if end <= start {
            return placed;
        }
        (start, end)
    }

    /// Seconds of footage in the session.
    pub fn footage_end(&self) -> f64 {
        self.footage_end
    }

    /// The target this plan aims at.
    pub fn target(&self) -> f64 {
        self.target
    }

    /// Marks and dead air still to come out, as the speed and cut passes would report them.
    pub fn set_pending(&mut self, marks: f64, dead_air: f64) {
        self.marks_pending = marks;
        self.dead_air_pending = dead_air;
    }

    /// The seconds an edge may move to reach one of these. P.policy.snapToleranceSeconds
    pub fn set_snap_tolerance(&mut self, seconds: f64) {
        self.snap_tolerance = seconds;
    }

    /// Where an edge is allowed to land — word edges, silences, visual cuts. Given by the caller because
    /// finding them is audio and ffprobe work this module must not invent.
    pub fn with_snap_points(mut self, points: Vec<f64>) -> Self {
        self.snap_points = points;
        self
    }

    /// Spans with no footage under them, and the retake marks over the session. A caller with a transcript
    /// passes both; a plan without gaps has footage everywhere it has seconds.
    pub fn with_gaps(mut self, gaps: Vec<(f64, f64)>, marks: Vec<(f64, f64)>) -> Self {
        self.gaps = gaps;
        self.marks = marks;
        self
    }
}

/// Does the requested range land on the gap? An edge inside one, or a range that spans it, is a segment
/// that would spend part of its length on seconds nobody filmed.
fn placed_in_gap(from: f64, to: f64, start: f64, end: f64) -> bool {
    (start > from && start < to) || (end > from && end < to) || (start <= from && end >= to)
}

/// The rate a requested speed becomes: at most [`MAX_RATE`], then lowered where the stretch would render
/// under [`MIN_CLIP_SECONDS`] on screen. The floor is 1.0 because this pass only ever shortens — slowing
/// footage down is a speed *effect* placed by hand, clamped by P.eng.minRate/maxRate instead.
pub fn apply_rate(rate: f64, seconds: f64) -> f64 {
    // Under 1 there is nothing to clamp: the rate stands as sent and the stretch simply lengthens.
    if rate <= 1.0 {
        return rate.max(0.0);
    }
    let clamped = rate.min(MAX_RATE);
    if seconds / clamped < MIN_CLIP_SECONDS {
        return (seconds / MIN_CLIP_SECONDS).max(1.0);
    }
    clamped
}

/// Why a rate is not the one asked for, in terms of what it would have done. `None` when it could be sent
/// as it stands — and only then, because §3 says a change the model did not ask for MUST be said. A rate over
/// the ceiling always earns a sentence, even when the clip floor is what finally stopped it: the model asked
/// for something impossible on two counts and should hear about the one it hit first.
fn why_rate(rate: f64, seconds: f64) -> Option<String> {
    if rate > MAX_RATE {
        return Some(format!(
            "the fastest this cut plays footage is {MAX_RATE}, which leaves {:.2} s on screen",
            seconds / MAX_RATE
        ));
    }
    if rate > 1.0 && seconds / rate < MIN_CLIP_SECONDS {
        return Some(format!(
            "{rate} would leave {:.2} s on screen, under the shortest clip the render makes \
             ({MIN_CLIP_SECONDS} s)",
            seconds / rate
        ));
    }
    None
}

/// The sentence explaining a rate that is not the one asked for; `None` when it was applied as sent.
fn rate_reason(rate: f64, applied: f64, seconds: f64) -> Option<String> {
    if (applied - rate).abs() < 1e-9 {
        return None;
    }
    // Over the ceiling on a stretch too short for even that: both bounds are true, and the clip floor is
    // what decided the number the model got, so it leads.
    if rate > MAX_RATE && seconds / MAX_RATE < MIN_CLIP_SECONDS {
        return Some(format!(
            "{rate} is over the fastest this cut plays footage ({MAX_RATE}), and even that would leave \
             {:.2} s on screen under the shortest clip the render makes ({MIN_CLIP_SECONDS} s)",
            seconds / MAX_RATE
        ));
    }
    why_rate(rate, seconds)
}

/// How much session footage a target accepts. The floor is the finished video's own — footage under it
/// cannot fill the target at any rate — and the ceiling is what the speed pass could squeeze into the
/// target at [`MAX_RATE`], so a cut inside it is one the arithmetic can still land. Wide on purpose: which
/// of those seconds are worth keeping is the cut's judgement, and how many of them run fast is a later
/// call's. P.machine.footageWindow, from `suggestWindow` × [`MAX_RATE`].
pub fn footage_window(target: f64) -> (f64, f64) {
    if target <= 0.0 {
        return (0.0, 0.0);
    }
    // A long cut is a wish, so half under is allowed; under a minute it is a promise, and the ceiling is
    // a fifth over — a 25 s target must not ship as 37. The floor is shared. P.policy.shortTargetSeconds
    let high = if target <= SHORT_TARGET_SECONDS {
        target * 1.2
    } else {
        target * 1.5
    };
    (target * 0.6, high * MAX_RATE)
}

/// How few segments a cut may come back with before it is arithmetic nonsense rather than a choice: about
/// one per half minute, at least four. P.machine.suggestMinSegments
pub fn min_segments(target: f64) -> usize {
    (1 + (target / 30.0) as usize).min(4)
}

/// The other end of the same question. It exists because of one failure: an answer of 548 segments. A cut
/// asks for about one segment per 20 s of target, so this is four times that with a floor — it catches the
/// runaway, which misses by an order of magnitude and not by a few. P.machine.suggestMaxSegments
pub fn max_segments(target: f64) -> usize {
    ((target / 5.0) as usize).max(40)
}

/// What stands between this cut and finishing, worst first: too little footage kept (the target cannot be
/// filled), then the segment count outside its bounds, then a stretch below the minimum scene. The length
/// complaint leads because it is the one that changes what the model should add or drop next, and the count
/// is only worth saying once there is footage to divide up.
pub fn cut_problems(status: &Status) -> Vec<String> {
    let mut problems = Vec::new();
    let (low, high) = status.window;
    if low > 0.0 && status.footage < low {
        problems.push(format!(
            "{} kept is under the {} this cut accepts -- add footage or speed it up",
            clock(status.footage),
            clock(low)
        ));
    } else if high > 0.0 && status.footage > high {
        problems.push(format!(
            "{} kept is over the {} this cut accepts -- drop some footage",
            clock(status.footage),
            clock(high)
        ));
    }
    if status.segments < status.min_segments {
        problems.push(format!(
            "{} segments is under the {} this cut needs -- add more",
            status.segments, status.min_segments
        ));
    }
    if status.segments > status.max_segments {
        problems.push(format!(
            "{} segments is over the {} this cut allows -- merge or drop some",
            status.segments, status.max_segments
        ));
    }
    if let Some(short) = status.short_scenes.first() {
        problems.push(format!(
            "the segment at {} runs {:.1} s on screen, under the shortest stretch this cut keeps \
             ({MIN_SCENE_SECONDS} s)",
            clock(short.0),
            short.1
        ));
    }
    problems
}
