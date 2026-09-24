//! §05-cut#8 (`Suggest`, bullet 3) — `spec/05-cut.md` §8.
//!
//! ▶ on the Cut page is F2.13's flow and its round will write the pass itself. What §8 confirms, and what
//! lives here until then, are the rules around that call: when thinking stays on, when the web tools come back
//! off, how the streamed answer is turned into progress, which segments are thrown away before their length is
//! worth arguing about, how the checks answer (every fault at once, worst first), and what a `speed`/`rate` for
//! a whole segment counts as.
//!
//! Nothing here re-decides what another module already owns: [`crate::roles`] answers thinking and web tools,
//! [`crate::timeline`] answers what was filmed and whether two ranges overlap, [`crate::tools::cutpass`] holds
//! the tool-side fault for a segment placed past the footage. This module turns those into the sentences and
//! the one progress number the page shows.

use crate::cut::EffectKind;
use crate::roles::{self, Job};
use crate::timeline;
use crate::tools;

// --- the retries ------------------------------------------------------------------------------------------

/// §05-cut#8 (`thinking on for the first attempt, off after an all-reasoning reply`): the log line for the
/// retry that stops asking for reasoning. A model that filled the thinking block and wrote no answer is not
/// stuck for a reason — it is stuck producing one, so the next call asks for the answer alone.
pub const THINKING_RETRY_LOG: &str = ">>> suggest: the model spent the whole call thinking and wrote nothing \
                                      \u{2014} asking again with thinking off";

/// §05-cut#8 (`web tools withdrawn on the first rejection`): the log line for taking them away. They cost a
/// round each, and the second call is asked to copy segments off the timeline it was already given.
pub const WEB_TOOLS_WITHDRAWN_LOG: &str = ">>> suggest: asking again without the web tools";

/// §05-cut#8: does the NEXT attempt think? On until a reply that was all reasoning and no answer, then off —
/// which is [`roles::cut_retry_thinking`]'s rule read from the other side (that fn takes the two halves of the
/// observation, this one the fact §8 states as one). Thinking stays available for every attempt: what changes
/// is whether it is asked for, never whether the model may use it.
pub fn thinking_after_empty_reply(reply_was_all_reasoning: bool, attempt: u32) -> bool {
    // The first attempt has nothing behind it yet — §2's table gives the cut model thinking, and
    // roles::thinking is that row.
    if attempt <= 1 {
        return roles::thinking(Job::ModelCut);
    }
    roles::cut_retry_thinking(reply_was_all_reasoning, reply_was_all_reasoning)
}

/// §05-cut#8: does this call get the web tools? Only until something was rejected once — the same rule as
/// [`roles::cut_attempt`]'s `web_tools`, which is why this counts rejections rather than deciding again.
pub fn offers_web(rejections: u32) -> bool {
    roles::cut_attempt(rejections + 1).web_tools
}

// --- the progress the streamed answer allows --------------------------------------------------------------

/// §05-cut#8 (`pulsing under "thinking over the whole session"`): what the status says before the model has
/// closed a single segment — there is no position to report yet, only that it is reading.
pub const THINKING_STATUS: &str = "thinking over the whole session";

/// §05-cut#8 (`never below 0.02`): the smallest progress the bar may show while a call is open. A bar that
/// reads exactly 0% is indistinguishable from one that never started, which is how a stuck run gets waited on.
/// `// suggest.progressFloorSeconds` — §10 gives this no `P.` id, so it carries a bare prefix.
pub const PROGRESS_FLOOR_SECONDS: f64 = 0.02;

/// §05-cut#8 (`progress "N moments, at m:ss of m:ss"`): the status line, both stamps in session time.
pub fn progress(moments: usize, at: f64, total: f64) -> String {
    format!("{moments} moments, at {} of {}", tools::mm_ss(at), tools::mm_ss(total))
}

/// §05-cut#8 (`placed by the last closed segment's end, never below 0.02 nor backwards`): where the line sits
/// for the progress just reported. `shown` is what the page displayed at the previous tick — a streamed answer
/// arrives out of order only in the sense that nothing has arrived yet, so the line may stand still or move on,
/// never back.
pub fn place_progress(last_closed_end: f64, shown: f64) -> f64 {
    last_closed_end.max(PROGRESS_FLOOR_SECONDS).max(shown)
}

/// `P.eng.suggestFallbackSegments`: the denominator for a reply whose segments have no readable end.
/// Progress only — it judges nothing and rejects nothing (prototype `suggestMaxSegs`, gui/cut.go:108).
///
/// Twenty is roughly the shape of a normal cut list, so N moments counted against it reads as "N of the
/// way through a plausible answer" when there is no position to place the bar on.
///
/// Deliberately not a [`crate::params`] row and not a field on [`crate::project::Policy`]: §05's own §6
/// does not name a fallback denominator among the parameters the Cut page is tuned by, and
/// `rust/tests/cut_parameters_used.rs` pins that section's row list. The value lives with the rule that
/// reads it, as every `P.*` value must; [`crate::params::family`] still answers `Family::Eng` from the
/// prefix — the same choice made for `P.machine.seamRetries` and `P.policy.narratorSlots`.
pub const SUGGEST_FALLBACK_SEGMENTS: usize = 20;

/// The count-only progress: how far through a plausible answer N moments look.
///
/// Clamped into the same band the placed progress uses — the floor is [`PROGRESS_FLOOR_SECONDS`]
/// (`suggest.progressFloorSeconds`) because a bar reading exactly 0% is indistinguishable from one that
/// never started, which is how a stuck run gets waited on.
pub fn fallback_fraction(moments: usize) -> f64 {
    (moments as f64 / SUGGEST_FALLBACK_SEGMENTS as f64).clamp(PROGRESS_FLOOR_SECONDS, 1.0)
}

/// What fraction of the way through the suggest call the bar should read.
///
/// A real position always wins: with a session span and somewhere reached, `through / span` says more than
/// any count could. The count over [`SUGGEST_FALLBACK_SEGMENTS`] is only the stand-in while the streamed
/// reply has no readable `end` to place against — no timeline yet, so counting is all there is. Both are
/// progress: neither one accepts or rejects the answer.
pub fn progress_fraction(moments: usize, through: f64, span: f64) -> f64 {
    if span > 0.0 && through > 0.0 {
        return (through / span).clamp(PROGRESS_FLOOR_SECONDS, 1.0);
    }
    fallback_fraction(moments)
}

/// §05-cut#8 (`pulsing under "thinking over the whole session" until the first segment closes`): the pulse is
/// the "something is happening" signal for a call with no position to report, so it ends exactly when there is
/// one.
pub fn pulses(shown: f64, segments_closed: usize) -> bool {
    segments_closed == 0 && shown <= PROGRESS_FLOOR_SECONDS
}

// --- what the answer has to be about -----------------------------------------------------------------------

/// §05-cut#8 (`segments with no recording at either end dropped before the length is judged`): the kept
/// segments and how many went missing. A segment with no footage at either end names a stretch of session that
/// was never filmed, so it has no length to judge — measuring one would reject an answer that is already void,
/// and the model would be told to fix a number instead of to look at the pictures.
///
/// `runs` is [`timeline::filmed_runs`]' answer for the session, so "was there a recording here" is the same
/// question the band lays itself out from. Touching a run's end does not count: the seam second belongs to the
/// later take, and an end that lands on it is inside a recording, which [`timeline::spans_overlap`] already
/// reads that way.
pub fn drop_unfilmed(segs: Vec<(f64, f64)>, runs: &[(f64, f64)]) -> (Vec<(f64, f64)>, usize) {
    // A second is filmed when a recording covers it; the point test stands in for "either end of the segment".
    let filmed = |t: f64| runs.iter().any(|run| timeline::spans_overlap((t, t + 0.001), *run));
    let total = segs.len();
    let kept: Vec<(f64, f64)> = segs
        .into_iter()
        .filter(|(start, end)| filmed(*start) && filmed(*end))
        .collect();
    let dropped = total - kept.len();
    (kept, dropped)
}

/// §05-cut#8 (`">>> suggest attempt N: M segment(s) dropped for having no footage"`): the log line naming how
/// many were thrown away on which attempt — the count is what tells a person to look at the session's clock.
pub fn dropped_log(attempt: u32, dropped: usize) -> String {
    format!(">>> suggest attempt {attempt}: {dropped} segment(s) dropped for having no footage")
}

// --- the checks answer --------------------------------------------------------------------------------------

/// §05-cut#8 (`the checks answer with every fault at once, worst first, joined by "; "`): one of the faults a
/// checked answer can carry. The order is the declaration's own: an out-of-range number makes every other
/// complaint about that segment meaningless, a count is cheaper to fix than a length, and a short scene is the
/// least wrong thing a model can hand back.
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
}

impl Fault {
    /// Worse faults first: out of range, then counts, then lengths.
    fn rank(&self) -> u8 {
        match self {
            Fault::PastEnd(_) => 0,
            Fault::TooFew(_) | Fault::TooMany(_) => 1,
            Fault::Short(_) => 2,
        }
    }

    /// What the fault says. The count and length sentences are [`crate::tools::cutpass`]'s own — the tool that
    /// rejected the segment is what phrases the complaint — so these delegate to it with a [`Status`] sitting
    /// inside every other window, which leaves exactly one complaint to read back.
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
                // The count stays where the base put it — inside both thresholds — so only the short stretch is
                // left to complain about.
                status.short_scenes = vec![(*at, 0.5)];
                cutpass::cut_problems(&status).join("; ")
            }
        }
    }
}

/// §05-cut#8 (`the checks answer with every fault at once … joined by "; "`): the whole reply to the model —
/// nothing is held back for a second round trip, because a model that learns one fault per call fixes them one
/// call at a time. Ordered worst first and worded by [`Fault`], which is what makes it one sentence per fault
/// and no more.
pub fn answer(faults: &[Fault]) -> String {
    let ordered = worst_first(faults.to_vec());
    let said: Vec<String> = ordered.iter().map(Fault::said).collect();
    said.join("; ")
}

/// §05-cut#8 (`worst first`): the faults in the order they are answered with.
pub fn worst_first(faults: Vec<Fault>) -> Vec<Fault> {
    let mut sorted = faults;
    // A stable sort keeps the model's own order among equally bad faults, so a reply does not shuffle between
    // two runs of the same answer.
    sorted.sort_by_key(|fault| fault.rank());
    sorted
}

/// §05-cut#8 (`the past-the-end fault shows the conversion on the model's own number`) — e.g. `2804 is not a
/// second: a stamp [28:04] is mm*60+ss, 1684`. The model wrote a stamp with its colons taken out; showing what
/// those digits mean as a stamp is the only way it can tell "too big" from "misread".
///
/// [`crate::tools::cutpass`]'s fault for the same segment says something else and stays where it is: that one
/// tells the tool's caller the range fell outside the footage, this one teaches the model its own number.
pub fn past_end_fault(claimed: u32) -> String {
    let (minutes, seconds) = (claimed / 100, claimed % 100);
    format!(
        "{claimed} is not a second: a stamp [{minutes}:{seconds:02}] is mm*60+ss, {}",
        minutes * 60 + seconds
    )
}

// --- what a rate for a whole segment counts as --------------------------------------------------------------

/// §05-cut#8 (`a speed/rate on a segment is a speed effect over it`): the kind those two names mean. A stop is
/// the same kind at rate 0, which is [`EffectKind`]'s reading rather than a second one taken here.
pub fn is_speed_effect(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Speed)
}

/// §05-cut#8 (`… exempt from any decorations cap`; spec/06-effects.md: "A cut-model rate for a whole segment is
/// a speed effect spanning it, not counted against any decorations ceiling"): a rate the cut asked for is part
/// of the cut's shape — it decides how long the segment runs — while the cap counts what the decoration pass
/// adds on top. Counting one against the other would let the number of captions decide whether the video runs
/// at half speed.
pub fn exempt_from_decorations_cap(kind: EffectKind) -> bool {
    is_speed_effect(kind)
}

// --- streamed progress everywhere ---------------------------------------------------------------------------

/// §05-cut#8: `Streamed progress everywhere = objects closed inside the text's last "<key>": [` (braces inside
/// strings ignored) `and the last closed object's end`.
///
/// The answer arrives as text, and a model streams `"segments": [` before it has anything to say about the
/// first one. Counting `}` characters anywhere in the reply would read a brace inside a caption's text as a
/// segment closing, so string contents are skipped: a `"` toggles in or out and a `\` inside a string escapes
/// the next character, which is what keeps `"say \"}"` from ending the count early. Only the LAST `"<key>": [`
/// counts, because a re-issued answer replaces what came before it rather than adding to it.
///
/// Returns how many objects closed in that array and the last closed object's `end`, when it carried one.
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

/// The `end` of one closed object — the number after `"end":`, ignoring whitespace, or `None` when the object
/// never named one. A hand-rolled read rather than a `serde_json` parse: the text is a truncated document, and
/// parsing it is exactly what cannot be done to something still arriving.
fn end_of(object: &str) -> Option<f64> {
    let at = object.find("\"end\":")? + "\"end\":".len();
    let digits: String = object[at..]
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    digits.parse().ok()
}
