//! Cut and effects — where each of the eleven behind-the-model behaviours of
//! `spec/12-decisions.md` §"Cut and effects" (rows 11–21) lives now.

use crate::roles;

pub use crate::tools::cutpass::SNAP_TOLERANCE_SECONDS;

pub use crate::cut_effects_pass::ZOOM_HEIGHT;
pub use crate::cut_speed::{DEFAULT_RATE, STOP_SECONDS};

/// The "no target" value of P.policy.targetLengthSeconds, re-exported from
/// [`crate::tools::cutpass::NO_TARGET`] so row 21's routing resolves without a second zero.
pub use crate::tools::cutpass::NO_TARGET;

// --- row 11: the weighted edge contest -----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Candidate {
    SilenceMidpoint,
    WordEdge,
    LineEdge,
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
pub const WEIGHT_SILENCE: f64 = 0.8;
/// A word edge is nearly as good as a line edge — the splice lands where a syllable ends either way.
pub const WEIGHT_WORD_EDGE: f64 = 0.9;
/// A line edge keeps the sentence whole, which is why it beats a bare word edge.
pub const WEIGHT_LINE_EDGE: f64 = 0.95;
/// A visual cut with nobody talking is the best of the four: picture and sound both agree.
pub const WEIGHT_VISUAL_CUT: f64 = 1.0;

pub const OUTWARD_BONUS: f64 = 0.05;

pub const DISTANCE_PENALTY_PER_SECOND: f64 = 0.18;

pub fn edge_score(
    kind: Candidate,
    point: f64,
    stamp: f64,
    tolerance: f64,
    moves_outward: bool,
) -> Option<f64> {
    let distance = (point - stamp).abs();
    if distance >= tolerance {
        return None;
    }
    let bonus = if moves_outward { OUTWARD_BONUS } else { 0.0 };
    Some(kind.weight() - distance * DISTANCE_PENALTY_PER_SECOND + bonus)
}

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
pub fn dead_air_is_cut(silence_seconds: f64, max: f64) -> bool {
    silence_seconds > max
}

/// What survives of a silenced stretch once the dead air is taken out: [`dead_air_is_cut`] decides
/// whether anything goes, and `keep` is the beat left behind so the cut is not a hard stop.
pub fn dead_air_kept(silence_seconds: f64, max: f64, keep: f64) -> f64 {
    if dead_air_is_cut(silence_seconds, max) {
        keep.max(0.0)
    } else {
        silence_seconds
    }
}

// --- row 20: telling the model what was taken away ------------------------------------------------------------------------

/// The sentence a retry should carry when the app has taken tools or thinking away from the model.
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
            what: "kinds outside zoom/stop/volume, volume gains of exactly 1, captions under 0.3 s and \
                  empty texts are each an error, not a silent drop",
            home: "tool result: each is that item's error sentence",
            lives_in: "tools::clips::Clips::add_effect (unknown kind, gain Some(1.0), empty-after-clamp \
                      span); Clips::add_caption (empty text, CAPTION_MIN_SECONDS)",
        },
        Ruled {
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
            what: "the target length is a derived policy number said in the log, not a regex \
                  over free text with no fallback",
            home: "policy — F0.7's `set_policy`, seen by the model as wording it cannot mis-set",
            lives_in: "P.policy.targetLengthSeconds, catalogued by params::cut and read by \
                      tools::cutpass::NO_TARGET and footage_window for both count gates and the \
                      footage_window read it for both count gates and the length gate",
        },
    ]
}

/// The ids this round's rows turn on, so a test can assert the catalogue still answers for them.
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
