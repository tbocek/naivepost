//! §02-services.md §3 tool catalogue, rewrite directive B: the cut pass's own tools.
//!
//! The model is shown candidate segments and asked for the keep list; these five tools are how it answers.
//! They mirror `cut.json` ([`crate::cut`]) but never touch the file or a server. A segment is addressed by
//! its session `start`, never a list position (§3.6), which would change under the model's feet.

use serde_json::json;

/// The fastest a rate may go. 4 and not 8: eight is for a minute of loading screen and would accept cuts
/// nothing could make watchable. P.policy.maxSpeedRate
pub const MAX_RATE: f64 = 4.0;

/// A stretch shorter than this on screen is not worth a rate of its own; a rate that lands under it is
/// lowered. P.eng.minClipSeconds, "also the speed clamp floor", hence this pass's bound too.
pub const MIN_CLIP_SECONDS: f64 = 0.5;

/// The shortest stretch worth keeping. A shorter segment is accepted and reported, not refused (§3.6: "whether
/// it survives P.policy.minSceneSeconds"); `finish_cut` is what refuses. P.policy.minSceneSeconds
pub const MIN_SCENE_SECONDS: f64 = 1.0;

/// How far an edge may move to a word edge, silence or visual cut; the caller may override it per plan.
pub const SNAP_TOLERANCE_SECONDS: f64 = 5.0;

/// A target up to this is a format and its footage window narrows; above it the target is a wish and the window widens.
/// P.policy.shortTargetSeconds
pub const SHORT_TARGET_SECONDS: f64 = 60.0;

/// No target at all: §10's `P.policy.targetLengthSeconds` default, `0 (none)`. Zero is not a very short
/// video, so [`footage_window`] guards on this rather than a bare `0.0`.
pub const NO_TARGET: f64 = 0.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    /// The rate applied to this segment, 1.0 for none.
    pub speed: f64,
    /// The model's own reason for keeping it, so a caller can show the choice beside the cut.
    pub why: String,
}

impl Segment {
    fn length(&self) -> f64 {
        self.end - self.start
    }

    fn on_screen(&self) -> f64 {
        self.length() / self.speed.max(f64::EPSILON)
    }
}

/// What `cut_status` reports and what `finish_cut` judges, at any time. One reading, computed once.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub footage: f64,
    pub window: (f64, f64),
    pub segments: usize,
    pub min_segments: usize,
    pub max_segments: usize,
    pub marks_pending: f64,
    pub dead_air_pending: f64,
    pub short_scenes: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Seconds of footage in the session: the end a segment may not be placed past.
    footage_end: f64,
    target: f64,
    snap_tolerance: f64,
    segments: Vec<Segment>,
    marks_pending: f64,
    dead_air_pending: f64,
    gaps: Vec<(f64, f64)>,
    snap_points: Vec<f64>,
    marks: Vec<(f64, f64)>,
    /// The User Context, which `get_context` hands back: it was answered empty, so a model that asked for it was
    /// told there was none (09-admin, 2026-11-07).
    context: String,
}

impl Plan {
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
            context: String::new(),
        }
    }

    /// `add_segment(start, end, why)`: ok with what the segment became, or an error naming the problem in seconds
    /// the model itself used.
    pub fn add_segment(&mut self, start: f64, end: f64, why: &str) -> String {
        // Ends before anything else: every other complaint would be about a range that does not exist.
        if end < start {
            return crate::tools::error(&format!(
                "your segment ends at {} and starts at {}, so it has no length",
                crate::tools::mm_ss(end),
                crate::tools::mm_ss(start)
            ));
        }
        if start < 0.0 || end > self.footage_end {
            // The model's own number leads: `start` when the segment begins before the footage, its end otherwise.
            // Quoting the other timestamp would send the model off to correct a number that was never wrong.
            let outside = if start < 0.0 { start } else { end };
            return crate::tools::error(&format!(
                "{} is past the end of the footage, which runs to {} -- keep your segment inside \
                 0 to {}",
                crate::tools::mm_ss(outside),
                crate::tools::mm_ss(self.footage_end),
                crate::tools::mm_ss(self.footage_end)
            ));
        }
        // No footage under an edge: this module only refuses; the caller says where the gaps are.
        if let Some(gap) = self
            .gaps
            .iter()
            .find(|(from, to)| placed_in_gap(*from, *to, start, end))
        {
            return crate::tools::error(&format!(
                "{} to {} has no footage under it -- there is a gap from {} to {}",
                crate::tools::mm_ss(start),
                crate::tools::mm_ss(end),
                crate::tools::mm_ss(gap.0),
                crate::tools::mm_ss(gap.1)
            ));
        }
        let placed = self.snap((start, end));
        // Checked after the snap, so an edge moved by tolerance cannot smuggle an overlap past it.
        // Touching is not overlapping: this pass joins takes, and a cut lands on a frame.
        if let Some(other) = self
            .segments
            .iter()
            .find(|other| crate::timeline::spans_overlap(placed, (other.start, other.end)))
        {
            return crate::tools::error(&format!(
                "{} to {} overlaps the segment already added at {} to {}",
                crate::tools::mm_ss(placed.0),
                crate::tools::mm_ss(placed.1),
                crate::tools::mm_ss(other.start),
                crate::tools::mm_ss(other.end)
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
            "span": [placed.0, placed.1],
            "moved": [placed.0 - start, placed.1 - end],
            "seconds": placed.1 - placed.0,
            "mark_seconds": mark,
            "survives_min_scene": survives,
            // The running total against the window: the model chooses the whole cut one segment at a time.
            "footage": status.footage,
            "window": [status.window.0, status.window.1],
        }))
    }

    /// `remove_segment(start)`: ok. Named by where it starts, since a list position would move between calls.
    pub fn remove_segment(&mut self, start: f64) -> String {
        let Some(at) = self.segments.iter().position(|segment| {
            (segment.start - start).abs() <= self.snap_tolerance.max(f64::EPSILON)
        }) else {
            return crate::tools::error(&format!(
                "there is no segment starting at {} -- the cut holds {} segment(s), at {}",
                crate::tools::mm_ss(start),
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

    /// `set_speed(start, rate)`: ok with the rate as applied and the resulting on-screen length.
    pub fn set_speed(&mut self, start: f64, rate: f64) -> String {
        let Some(at) = self.segments.iter().position(|segment| {
            (segment.start - start).abs() <= self.snap_tolerance.max(f64::EPSILON)
        }) else {
            return crate::tools::error(&format!(
                "there is no segment starting at {} -- the cut holds {} segment(s), at {}",
                crate::tools::mm_ss(start),
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
            // §3: a change the model did not ask for MUST be said, never silently dropped.
            "reason": rate_reason(rate, applied, seconds),
        }))
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// `cut_status`: the reading [`Plan::finish_cut`] would give, at any time.
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

    /// `cut_status` as the tool answers it: [`Plan::status`] serialised, so the two cannot disagree.
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

    /// `finish_cut`: the whole-cut checks as a problem list, worst first, or ok.
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

    fn footage(&self) -> f64 {
        self.segments.iter().map(Segment::on_screen).sum()
    }

    fn starts(&self) -> String {
        if self.segments.is_empty() {
            return "nothing yet".to_string();
        }
        self.segments
            .iter()
            .map(|segment| crate::tools::mm_ss(segment.start))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn mark_seconds(&self, start: f64, end: f64) -> f64 {
        self.marks
            .iter()
            .map(|(from, to)| (*to).min(end) - (*from).max(start))
            .filter(|seconds| *seconds > 0.0)
            .sum()
    }

    /// Move an edge to the nearest allowed point within tolerance; with nothing to snap to, edges stand as put.
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
        // A snap may not reverse the segment or shrink it out of existence.
        if end <= start {
            return placed;
        }
        (start, end)
    }

    pub fn footage_end(&self) -> f64 {
        self.footage_end
    }

    pub fn target(&self) -> f64 {
        self.target
    }

    pub fn set_pending(&mut self, marks: f64, dead_air: f64) {
        self.marks_pending = marks;
        self.dead_air_pending = dead_air;
    }

    pub fn set_snap_tolerance(&mut self, seconds: f64) {
        self.snap_tolerance = seconds;
    }

    /// Where an edge is allowed to land: word edges, silences, visual cuts. Given by the caller (audio and ffprobe work).
    pub fn with_context(mut self, context: &str) -> Self {
        self.context = context.to_string();
        self
    }

    pub fn context(&self) -> &str {
        &self.context
    }

    pub fn with_snap_points(mut self, points: Vec<f64>) -> Self {
        self.snap_points = points;
        self
    }

    /// Spans with no footage under them, and the retake marks over the session. A plan without gaps has footage everywhere.
    pub fn with_gaps(mut self, gaps: Vec<(f64, f64)>, marks: Vec<(f64, f64)>) -> Self {
        self.gaps = gaps;
        self.marks = marks;
        self
    }
}

/// Does the requested range land on a gap? An edge inside one, or a range spanning it, spends length on unfilmed seconds.
fn placed_in_gap(from: f64, to: f64, start: f64, end: f64) -> bool {
    (start > from && start < to) || (end > from && end < to) || (start <= from && end >= to)
}

/// The rate a requested speed becomes: at most [`MAX_RATE`], then lowered where the stretch would render
/// under [`MIN_CLIP_SECONDS`] on screen. The floor is 1.0: this pass only shortens; slowing is a speed
/// *effect* placed by hand, clamped by P.eng.minRate/maxRate.
pub fn apply_rate(rate: f64, seconds: f64) -> f64 {
    // Under 1 there is nothing to clamp: the rate stands as sent.
    if rate <= 1.0 {
        return rate.max(0.0);
    }
    let clamped = rate.min(MAX_RATE);
    if seconds / clamped < MIN_CLIP_SECONDS {
        return (seconds / MIN_CLIP_SECONDS).max(1.0);
    }
    clamped
}

/// Why a rate is not the one asked for, `None` when it could be sent as it stands (§3: a change the model
/// did not ask for MUST be said). A rate over the ceiling always earns a sentence, even when the clip floor
/// stopped it; the model hears about the first bound it hit.
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

/// The sentence explaining a rate that is not the one asked for; `None` when applied as sent.
fn rate_reason(rate: f64, applied: f64, seconds: f64) -> Option<String> {
    if (applied - rate).abs() < 1e-9 {
        return None;
    }
    // Over the ceiling on a stretch too short for even that: the clip floor decided the number, so it leads.
    if rate > MAX_RATE && seconds / MAX_RATE < MIN_CLIP_SECONDS {
        return Some(format!(
            "{rate} is over the fastest this cut plays footage ({MAX_RATE}), and even that would leave \
             {:.2} s on screen under the shortest clip the render makes ({MIN_CLIP_SECONDS} s)",
            seconds / MAX_RATE
        ));
    }
    why_rate(rate, seconds)
}

/// How much session footage a target accepts. The floor is the finished video's own; the ceiling is what the
/// speed pass could squeeze into the target at [`MAX_RATE`]. Wide on purpose: which seconds are worth
/// keeping is the cut's judgement. P.machine.footageWindow, from `suggestWindow` × [`MAX_RATE`].
pub fn footage_window(target: f64) -> (f64, f64) {
    if target <= NO_TARGET {
        return (0.0, 0.0);
    }
    // A long cut is a wish, so half under is allowed; under a minute it is a promise and the ceiling is a
    // fifth over (a 25 s target must not ship as 37). The floor is shared. P.policy.shortTargetSeconds
    let high = if target <= SHORT_TARGET_SECONDS {
        target * 1.2
    } else {
        target * 1.5
    };
    (target * 0.6, high * MAX_RATE)
}

/// How few segments a cut may come back with before it is arithmetic nonsense: about one per half minute,
/// at least four. P.machine.suggestMinSegments
pub fn min_segments(target: f64) -> usize {
    (1 + (target / 30.0) as usize).min(4)
}

/// The other end: four times about one segment per 20 s of target, with a floor, to catch a runaway answer
/// (548 segments) that misses by an order of magnitude. P.machine.suggestMaxSegments
pub fn max_segments(target: f64) -> usize {
    ((target / 5.0) as usize).max(40)
}

/// What stands between this cut and finishing, worst first: too little footage kept, then the segment count
/// outside its bounds, then a stretch below the minimum scene. The length complaint leads because it changes
/// what the model should add or drop next.
pub fn cut_problems(status: &Status) -> Vec<String> {
    let mut problems = Vec::new();
    let (low, high) = status.window;
    if low > 0.0 && status.footage < low {
        problems.push(format!(
            "{} kept is under the {} this cut accepts -- add footage or speed it up",
            crate::tools::mm_ss(status.footage),
            crate::tools::mm_ss(low)
        ));
    } else if high > 0.0 && status.footage > high {
        problems.push(format!(
            "{} kept is over the {} this cut accepts -- drop some footage",
            crate::tools::mm_ss(status.footage),
            crate::tools::mm_ss(high)
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
            crate::tools::mm_ss(short.0),
            short.1
        ));
    }
    problems
}
