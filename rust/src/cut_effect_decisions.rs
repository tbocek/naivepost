//! Cut and effects — where each of the eleven behind-the-model behaviours of
//! `spec/12-decisions.md` §"Cut and effects" (rows 11–21) lives now.
//!
//! Same shape as [`crate::prepare_decisions`] for Prepare: most of these behaviours are already made
//! by machinery elsewhere in the crate, so this module holds only what nothing else had — the weighted
//! edge contest of row 11, the dead-air bound of row 13, and the withdrawal notice of row 20 — and
//! states every other row as a routing line pointing at the function that really makes the decision,
//! so the table cannot claim a behaviour this build does not have.
//!
//! The rule the section turns on is §1's: **what the model is told**. Row 11 reports the snapped
//! edges because the model may correct them; row 19 counts drops rather than naming them because the
//! cut moved under effects after the answer was already given.

use crate::roles;

/// The default snap tolerance this module's contest is written against —
/// [`crate::tools::cutpass::SNAP_TOLERANCE_SECONDS`], P.policy.snapToleranceSeconds, named here so a
/// reader reaches the live constant rather than a copy of its value.
pub use crate::tools::cutpass::SNAP_TOLERANCE_SECONDS;

/// Re-exported so the routing rows below resolve to real items: the pass's own zoom height
/// ([`crate::cut_effects_pass::ZOOM_HEIGHT`]) and the app defaults the tool reports
/// ([`crate::cut_speed::DEFAULT_RATE`], [`crate::cut_speed::STOP_SECONDS`]).
pub use crate::cut_effects_pass::ZOOM_HEIGHT;
pub use crate::cut_speed::{DEFAULT_RATE, STOP_SECONDS};

/// The "no target" value of P.policy.targetLengthSeconds, re-exported from
/// [`crate::tools::cutpass::NO_TARGET`] so row 21's routing resolves without a second zero.
pub use crate::tools::cutpass::NO_TARGET;

// --- row 11: the weighted edge contest -----------------------------------------------------------------------------------

/// One kind of place an edge may land, with the weight §05 [F2.14](../../spec/05-cut.md#f214-suggest-a-cut) S3
/// gives it. The weights carry no `P.` id (§10 lists none for them), so they take the bare `cut.snap*`
/// prefix of the rule that reads them, exactly as [`crate::prepare_decisions::TAIL_MATCH_MIN`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Candidate {
    /// The midpoint of a silence — quiet, but the least trusted of the four. `cut.snapWeightSilence` 0.8
    SilenceMidpoint,
    /// A word boundary from the aligner. `cut.snapWeightWordEdge` 0.9
    WordEdge,
    /// A transcript line boundary. `cut.snapWeightLineEdge` 0.95
    LineEdge,
    /// A shot change found by ffprobe where nobody talks. `cut.snapWeightVisualCut` 1.0
    VisualCut,
}

impl Candidate {
    /// The weight §05 F2.14 S3 gives this kind of point, before distance and direction are applied.
    pub fn weight(self) -> f64 {
        match self {
            Self::SilenceMidpoint => WEIGHT_SILENCE,
            Self::WordEdge => WEIGHT_WORD_EDGE,
            Self::LineEdge => WEIGHT_LINE_EDGE,
            Self::VisualCut => WEIGHT_VISUAL_CUT,
        }
    }
}

/// Silence midpoints score lowest: quiet is everywhere, so it is the weakest reason to cut there.
/// `cut.snapWeightSilence`
pub const WEIGHT_SILENCE: f64 = 0.8;
/// A word edge is nearly as good as a line edge — the splice lands where a syllable ends either way.
/// `cut.snapWeightWordEdge`
pub const WEIGHT_WORD_EDGE: f64 = 0.9;
/// A line edge keeps the sentence whole, which is why it beats a bare word edge.
/// `cut.snapWeightLineEdge`
pub const WEIGHT_LINE_EDGE: f64 = 0.95;
/// A visual cut with nobody talking is the best of the four: picture and sound both agree.
/// `cut.snapWeightVisualCut` 1.0
pub const WEIGHT_VISUAL_CUT: f64 = 1.0;

/// How far the outward direction is worth. §05 says "outward preferred" without a number, so the bonus
/// is set just under one full weight step: enough to break a tie against a nearer inward point of the
/// same kind, never enough to beat a better *kind* of point further away. `cut.snapOutwardBonus`
pub const OUTWARD_BONUS: f64 = 0.05;

/// The distance penalty per second of travel. Linear in the tolerance-normalised distance, so a point
/// at the very edge of [`cutpass::SNAP_TOLERANCE_SECONDS`] loses its whole weight and only a point
/// close to the stamp can win. `cut.snapDistancePenalty`
pub const DISTANCE_PENALTY_PER_SECOND: f64 = 0.18;

/// Score one candidate landing at `point` for an edge stamped at `stamp`, allowed to move at most
/// `tolerance` seconds. P.policy.snapToleranceSeconds
///
/// `weight − distance × penalty + outward bonus`. Outside the tolerance the score is `None`: a point
/// further off than the tolerance is a different moment, not a snap target. `moves_outward` means the
/// edge moves away from the middle of the kept stretch — earlier for a start edge, later for an end
/// edge — which §05 prefers because it keeps more of what was filmed.
pub fn edge_score(
    kind: Candidate,
    point: f64,
    stamp: f64,
    tolerance: f64,
    moves_outward: bool,
) -> Option<f64> {
    let distance = (point - stamp).abs();
    // Half-open reach: a point exactly `tolerance` away is already outside, so the tolerance means
    // "moved *up to* this many seconds" without rounding letting five whole seconds through.
    if distance >= tolerance {
        return None;
    }
    let bonus = if moves_outward { OUTWARD_BONUS } else { 0.0 };
    Some(kind.weight() - distance * DISTANCE_PENALTY_PER_SECOND + bonus)
}

/// Run the contest §05 F2.14 S3 describes and the model never sees: pick the best candidate within
/// `tolerance` of `stamp`, returning `(point, score, seconds_moved)`. P.policy.snapToleranceSeconds
///
/// Ties go to the nearer point, then to the earlier one, so the result is deterministic — a contest
/// that resolves differently between two runs of the same project is worse than a slightly worse
/// contest. `None` when no candidate is inside the tolerance, in which case the edge stays exactly
/// where it was stamped.
///
/// Note on the live path: [`cutpass::Plan::snap`] currently takes the *nearest* snap point, which is
/// this scorer with equal weights and no direction term. This function is the weighted form the spec
/// asks for, written here so the contest is testable against hand-written candidates without an audio
/// envelope or an ffprobe run.
pub fn choose_edge(
    candidates: &[(f64, Candidate)],
    stamp: f64,
    tolerance: f64,
    moves_outward: bool,
) -> Option<(f64, f64, f64)> {
    let mut best: Option<(f64, f64, f64)> = None;
    for &(point, kind) in candidates {
        let Some(score) = edge_score(kind, point, stamp, tolerance, moves_outward) else {
            continue;
        };
        match best {
            Some((best_point, best_score, _))
                if best_score > score
                    || (best_score == score && best_point.abs() <= point.abs()) => {}
            _ => best = Some((point, score, point - stamp)),
        }
    }
    best
}

// --- row 13: dead air ----------------------------------------------------------------------------------------------------

/// Whether a silence inside a kept clip is long enough to be cut out.
///
/// §12 row 13 calls the bound a policy number rather than a silent rule: `P.policy.deadAirMaxSeconds`
/// (8.0), paired with `P.policy.deadAirKeepSeconds` (0.5) for how much of the beat survives. Neither
/// constant exists in this tree yet — `params::cut()` deliberately omits both rows because "no rule in
/// this tree reads them yet" (`params.rs`, the paragraph above `cut()`), and `params::silence()`
/// answers a different pair (`degraded::SILENCE_THRESHOLD_DB` / `SILENCE_MIN_SECONDS`, the
/// detection floor, not the length bound). So `max` arrives from the caller and this helper only holds
/// the comparison; the round that writes the removal pass adds the constants and this signature stands.
///
/// Strictly greater: a silence of exactly the maximum is a deliberate beat, and the brief tells the
/// model the number so it can keep one by asking.
pub fn dead_air_is_cut(silence_seconds: f64, max: f64) -> bool {
    silence_seconds > max
}

/// What survives of a silenced stretch once the dead air is taken out: [`dead_air_is_cut`] decides
/// whether anything goes, and `keep` is the beat left behind so the cut is not a hard stop.
/// `P.policy.deadAirKeepSeconds`
pub fn dead_air_kept(silence_seconds: f64, max: f64, keep: f64) -> f64 {
    if dead_air_is_cut(silence_seconds, max) {
        keep.max(0.0)
    } else {
        silence_seconds
    }
}

// --- row 20: telling the model what was taken away ------------------------------------------------------------------------

/// The sentence a retry should carry when the app has taken tools or thinking away from the model.
///
/// Both withdrawals stay the app's decisions — [`roles::CutAttempt`] sets `web_tools: attempt == 1`
/// and [`roles::cut_retry_thinking`] turns thinking off after an all-reasoning reply — but §12 row 20
/// says a rewrite SHOULD tell the model rather than leave it looking for `web_search` in a schema that
/// no longer offers it. `None` when nothing was withdrawn, so a first attempt prepends nothing.
///
/// The live path today logs the rejection around the attempt; this is the notice a caller prepends to
/// the next request so the model knows what it is not being given.
pub fn withdrawal_notice(web_withdrawn: bool, thinking_off: bool) -> Option<String> {
    match (web_withdrawn, thinking_off) {
        (false, false) => None,
        (true, true) => Some(
            "note: web_search and web_read are no longer offered to you, and thinking is switched off \
             for this attempt -- answer with content, not reasoning".to_string(),
        ),
        (true, false) => {
            Some("note: web_search and web_read are no longer offered to you.".to_string())
        }
        (false, true) => Some("note: thinking is switched off for this attempt.".to_string()),
    }
}

/// Whether the withdrawal applies to this cut attempt: web tools ride the first attempt only, so they
/// are gone from attempt 2 onward. Read through [`roles::CutAttempt`] so the audit names the live field.
pub fn web_tools_withdrawn(attempt: u32) -> bool {
    !roles::cut_attempt(attempt).web_tools
}

// --- the audit table -----------------------------------------------------------------------------------------------------

/// One row of §12's "Cut and effects" table: the behaviour, whose decision it is, and the code that
/// actually makes it. Kept as data rather than prose so a row naming a function that no longer exists
/// fails a test instead of drifting quietly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ruled {
    /// The behaviour as §12 words it.
    pub what: &'static str,
    /// Whose decision it is, in §1's homes.
    pub home: &'static str,
    /// Where it happens in this tree.
    pub lives_in: &'static str,
}

/// The eleven rows of §12 "Cut and effects" (11–21), in order.
pub fn audit() -> [Ruled; 11] {
    [
        Ruled {
            // Every segment edge is moved up to five seconds by a weighted contest the model never
            // sees. P.policy.snapToleranceSeconds
            what: "every segment edge is moved up to five seconds by a weighted contest",
            home: "add_segment's result: the snapped edges and how far each moved",
            lives_in: "cut_effect_decisions::choose_edge scores the contest; tools::cutpass::Plan::add_segment \
                      reports `span` + `moved`",
        },
        Ruled {
            what: "a segment with no footage under an edge is refused, and the running total stays honest",
            home: "tool argument refused when added; tool result keeps the count",
            lives_in: "tools::cutpass::Plan::add_segment's gap refusal (placed_in_gap); Plan::status and \
                      cut_problems for the running total at finish_cut",
        },
        Ruled {
            // P.policy.deadAirMaxSeconds
            what: "silences over eight seconds are cut out of kept clips",
            home: "after finish — a policy number, not a silent rule",
            lives_in: "cut_effect_decisions::dead_air_is_cut (P.policy.deadAirMaxSeconds, stated in the \
                      cut brief so the model can keep a beat by asking)",
        },
        Ruled {
            what: "a zoom is no longer forced to a centred punch-in: the tool takes a box and the pass \
                  can look before aiming",
            home: "tool argument (box) + tool result (the box used)",
            lives_in: "tools::clips::Clips::add_effect's `box_used` (else DEFAULT_ZOOM_BOX); \
                      get_frames via Clips::clip_frames; cut_effects_pass::ZOOM_HEIGHT is the pass's own \
                      default when no box is sent",
        },
        Ruled {
            what: "a rate over 1 on a captioned clip errors, naming the clip that carries the caption",
            home: "tool argument refused, with the clip number in the reason",
            lives_in: "tools::clips::Clips::set_clip_speed, whose sentence names the clip whose \
                      captions list is non-empty",
        },
        Ruled {
            // P.policy.captionMinSeconds
            what: "kinds outside zoom/stop/volume, volume gains of exactly 1, captions under 0.3 s and \
                  empty texts are each an error, not a silent drop",
            home: "tool result: each is that item's error sentence",
            lives_in: "tools::clips::Clips::add_effect (unknown kind, gain Some(1.0), empty-after-clamp \
                      span); Clips::add_caption (empty text, CAPTION_MIN_SECONDS)",
        },
        Ruled {
            // P.policy.effectDefaultSeconds, P.policy.effectDefaultFades
            what: "a speed with no rate becomes 0.5, a stop with no span 2 s, a caption with no span \
                  3 s; fades and ramps are invented — and the tool result states what was applied",
            home: "app defaults, reported back in the tool result",
            lives_in: "cut_speed::DEFAULT_RATE, cut_speed::STOP_SECONDS, fx_zoom::DEFAULT_SECONDS; \
                      cut_effects_pass::{glide, stop_fade, ramp}; reported by add_effect/set_speed's \
                      `applied`/`fade`/`reason`",
        },
        Ruled {
            what: "the whole effect list is replaced, discarding hand-placed labels and zooms",
            home: "stays: the page's contract with the user (one Undo), not a model decision",
            lives_in: "05-cut F2.14 S3 (effects replaced as a list); cut_trim::pushes_undo for the one Undo",
        },
        Ruled {
            what: "effects are clamped to the cut as applied, dropped under a second of survivor, and \
                  only the count reaches the log",
            home: "after finish (the cut moved under them), but the count is a line the user can act on",
            lives_in: "cut_clamp::clamp_to_cut, cut_clamp::MIN_SURVIVING_SECONDS, cut_clamp::log_line",
        },
        Ruled {
            what: "web tools are withdrawn after the first rejection and thinking switched off after an \
                  all-reasoning reply — and the model is told both",
            home: "the app's, logged, and now announced in the next request",
            lives_in: "roles::CutAttempt.web_tools, roles::cut_retry_thinking; \
                      cut_effect_decisions::withdrawal_notice for what the model is told",
        },
        Ruled {
            // P.policy.targetLengthSeconds
            what: "the target length is a derived policy number shown in the policy form, not a regex \
                  over free text with no fallback",
            home: "policy — F0.7's `set_policy`, seen by the model as wording it cannot mis-set",
            lives_in: "P.policy.targetLengthSeconds, catalogued by params::cut and read by \
                      tools::cutpass::NO_TARGET and footage_window for both count gates and the \
                      footage_window read it for both count gates and the length gate",
        },
    ]
}

/// The ids this round's rows turn on, so a test can assert the catalogue still answers for them.
/// P.policy.snapToleranceSeconds · P.policy.targetLengthSeconds · P.policy.captionMinSeconds ·
/// P.policy.effectDefaultSeconds · P.policy.effectDefaultFades
pub const CITED_PARAMS: [&str; 5] = [
    "P.policy.snapToleranceSeconds",
    "P.policy.targetLengthSeconds",
    "P.policy.captionMinSeconds",
    "P.policy.effectDefaultSeconds",
    "P.policy.effectDefaultFades",
];

/// The tools this round's rows route through.
pub const CITED_TOOLS: [&str; 7] = [
    "add_segment",
    "cut_status",
    "finish_cut",
    "set_clip_speed",
    "add_effect",
    "add_caption",
    "get_frames",
];
