//! §05-cut#8 (`Suggest`, bullet 3) — `spec/05-cut.md` §8.

use crate::cut::EffectKind;
use crate::roles::{self, Job};
use crate::timeline;
use crate::tools;

pub const THINKING_RETRY_LOG: &str =
    ">>> suggest: the model spent the whole call thinking and wrote nothing \
                                      \u{2014} asking again with thinking off";

pub const WEB_TOOLS_WITHDRAWN_LOG: &str = ">>> suggest: asking again without the web tools";

pub fn thinking_after_empty_reply(reply_was_all_reasoning: bool, attempt: u32) -> bool {
    // The first attempt has nothing behind it yet — §2's table gives the cut model thinking, and
    // roles::thinking is that row.
    if attempt <= 1 {
        return roles::thinking(Job::ModelCut);
    }
    roles::cut_retry_thinking(reply_was_all_reasoning, reply_was_all_reasoning)
}

pub fn offers_web(rejections: u32) -> bool {
    roles::cut_attempt(rejections + 1).web_tools
}

pub const THINKING_STATUS: &str = "thinking over the whole session";

pub const PROGRESS_FLOOR_SECONDS: f64 = 0.02;

pub fn progress(moments: usize, at: f64, total: f64) -> String {
    format!(
        "{moments} moments, at {} of {}",
        tools::mm_ss(at),
        tools::mm_ss(total)
    )
}

pub fn place_progress(last_closed_end: f64, shown: f64) -> f64 {
    last_closed_end.max(PROGRESS_FLOOR_SECONDS).max(shown)
}

/// `P.eng.suggestFallbackSegments`: the denominator for a reply whose segments have no readable end.
pub const SUGGEST_FALLBACK_SEGMENTS: usize = 20;

/// The count-only progress: how far through a plausible answer N moments look.
pub fn fallback_fraction(moments: usize) -> f64 {
    (moments as f64 / SUGGEST_FALLBACK_SEGMENTS as f64).clamp(PROGRESS_FLOOR_SECONDS, 1.0)
}

/// What fraction of the way through the suggest call the bar should read.
pub fn progress_fraction(moments: usize, through: f64, span: f64) -> f64 {
    if span > 0.0 && through > 0.0 {
        return (through / span).clamp(PROGRESS_FLOOR_SECONDS, 1.0);
    }
    fallback_fraction(moments)
}

pub fn pulses(shown: f64, segments_closed: usize) -> bool {
    segments_closed == 0 && shown <= PROGRESS_FLOOR_SECONDS
}

pub fn drop_unfilmed(segs: Vec<(f64, f64)>, runs: &[(f64, f64)]) -> (Vec<(f64, f64)>, usize) {
    let filmed = |from: f64, to: f64| {
        runs.iter()
            .any(|run| timeline::spans_overlap((from, to), *run))
    };
    let total = segs.len();
    let kept: Vec<(f64, f64)> = segs
        .into_iter()
        .filter(|(start, end)| filmed(*start, start + 0.001) && filmed(end - 0.001, *end))
        .collect();
    let dropped = total - kept.len();
    (kept, dropped)
}

pub fn dropped_log(attempt: u32, dropped: usize) -> String {
    format!(">>> suggest attempt {attempt}: {dropped} segment(s) dropped for having no footage")
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fault {
    /// A number outside the session — the conversion of it goes in [`past_end_fault`].
    PastEnd(u32),
    /// Fewer segments than the cut may be made of.
    TooFew(usize),
    /// More segments than the cut may be made of.
    TooMany(usize),
    /// A segment under the shortest stretch this cut keeps.
    Short(f64),
    Quoted(String),
}

impl Fault {
    /// Worse faults first: out of range, then counts, then lengths.
    fn rank(&self) -> u8 {
        match self {
            Fault::PastEnd(_) => 0,
            Fault::TooFew(_) | Fault::TooMany(_) => 1,
            Fault::Short(_) => 2,
            // A quoted fault carries its own precedence already: `cut_problems` returns worst-first, and the
            // stable sort keeps that order among the equals it is joined with.
            Fault::Quoted(_) => 1,
        }
    }

    fn said(&self) -> String {
        use crate::tools::cutpass::{self, Status};
        let mut status = Status {
            footage: 600.0,
            window: (0.0, f64::INFINITY),
            segments: 0,
            min_segments: 0,
            max_segments: usize::MAX,
            marks_pending: 0.0,
            dead_air_pending: 0.0,
            short_scenes: Vec::new(),
        };
        match self {
            Fault::Quoted(said) => said.clone(),
            Fault::PastEnd(claimed) => past_end_fault(*claimed),
            Fault::TooFew(got) => {
                // The low end is the only threshold this variant breaks: `max_segments` and every other window
                // stay satisfied, so one complaint comes back — with real numbers rather than a sentinel.
                status.window = cutpass::footage_window(600.0);
                status.min_segments = got + 1;
                status.max_segments = usize::MAX;
                status.segments = *got;
                cutpass::cut_problems(&status).join("; ")
            }
            Fault::TooMany(got) => {
                // Both count thresholds are compared with `segments`, so the low end is satisfied by raising it
                // with the high one — otherwise this variant would answer with two complaints about one fault.
                status.window = cutpass::footage_window(600.0);
                status.segments = *got;
                status.min_segments = *got;
                status.max_segments = got - 1;
                cutpass::cut_problems(&status).join("; ")
            }
            Fault::Short(at) => {
                // (start, seconds on screen): a stretch under the floor, which is what makes it a fault at all.
                status.short_scenes = vec![(*at, 0.5)];
                cutpass::cut_problems(&status).join("; ")
            }
        }
    }
}

pub fn answer(faults: &[Fault]) -> String {
    let ordered = worst_first(faults.to_vec());
    let said: Vec<String> = ordered.iter().map(Fault::said).collect();
    said.join("; ")
}

pub fn worst_first(faults: Vec<Fault>) -> Vec<Fault> {
    let mut sorted = faults;
    // A stable sort keeps the model's own order among equally bad faults, so a reply does not shuffle between
    // two runs of the same answer.
    sorted.sort_by_key(|fault| fault.rank());
    sorted
}

pub fn past_end_fault(claimed: u32) -> String {
    let (minutes, seconds) = (claimed / 100, claimed % 100);
    format!(
        "{claimed} is not a second: a stamp [{minutes}:{seconds:02}] is mm*60+ss, {}",
        minutes * 60 + seconds
    )
}

pub fn is_speed_effect(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Speed)
}

pub fn streamed(text: &str, key: &str) -> Option<(usize, Option<f64>)> {
    let marker = format!("\"{key}\": [");
    let open = text.rfind(&marker)? + marker.len();
    let body = &text[open..];

    let mut closed = 0;
    let mut last_end: Option<f64> = None;
    // The object currently being read, from its `{` — remembered on the way in and spent on the way out, so a
    // `}` reads its own object rather than whatever brace appeared last somewhere in the text.
    let mut object_start: Option<usize> = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in body.char_indices() {
        if in_string {
            // Inside a string value every brace is text: only the escape and the closing quote matter.
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => {
                depth += 1;
                if depth == 1 {
                    object_start = Some(index);
                }
            }
            '[' => depth += 1,
            '}' | ']' if depth > 0 => depth -= 1,
            // depth == 0 and an unlabelled `]` is the array's own end: everything it held has been counted,
            // and what follows belongs to another key. An EMPTY array says so here, and an answer that named no
            // segment at all has no progress — which is what keeps the page pulsing rather than placing 0.
            ']' if closed == 0 => return None,
            ']' => break,
            _ => {}
        }
        // A depth that fell back to nought closed one object of the array: its `end` is the progress. An
        // object that named none leaves the last known position standing rather than resetting it, and the
        // start comes from the way in — searching backwards for a `{` would find a brace inside a string.
        if ch == '}' && depth == 0 {
            closed += 1;
            let start = object_start.take().unwrap_or(0);
            last_end = end_of(&body[start..=index]).or(last_end);
        }
    }
    Some((closed, last_end))
}

fn end_of(object: &str) -> Option<f64> {
    let at = object.find("\"end\":")? + "\"end\":".len();
    let digits: String = object[at..]
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    digits.parse().ok()
}
