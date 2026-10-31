//! What stays out of the model's hands, on purpose (`spec/12-decisions.md` §4).
//! Mirror of [`crate::decision_audit`] and the per-flow audits ([`crate::prepare_decisions`], [`crate::cut_effect_decisions`]): what the model must never decide. Admission needs three things: mechanical, checkable, visible on screen afterwards. Anything that is a judgement about the video belongs at `finish`.
//! Six rows: five are arithmetic owned elsewhere; the sixth is the pipeline, set once through F0.7's `set_policy`, whose log line is the only place a person sees it.

use crate::decision_audit;
use crate::project::{CutMode, MarkingPass};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandsOff {
    WordTimes,
    EdgeBetweenKnownWords,
    SessionClock,
    WalkBack,
    RenderArithmetic,
    PipelineChoice,
}

impl HandsOff {
    pub fn all() -> [HandsOff; 6] {
        [
            Self::WordTimes,
            Self::EdgeBetweenKnownWords,
            Self::SessionClock,
            Self::WalkBack,
            Self::RenderArithmetic,
            Self::PipelineChoice,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WordTimes => "word times",
            Self::EdgeBetweenKnownWords => "where a cut lands between two known words",
            Self::SessionClock => "the session clock",
            Self::WalkBack => "edge snapping, hole closing, dead-air removal, coalescing, clamping",
            Self::RenderArithmetic => "the render's arithmetic",
            Self::PipelineChoice => {
                "which pass runs, and whether the cut comes from words or a model"
            }
        }
    }

    pub fn why_it_is_hands_off(self) -> &'static str {
        match self {
            Self::WordTimes => {
                "measured off the sound by the aligner; asking a model for a timestamp asks it to \
                 invent what was recorded"
            }
            Self::EdgeBetweenKnownWords => {
                "the model already chose the words; only the exact sample inside that fence is left, \
                 and the envelope reads it"
            }
            Self::SessionClock => {
                "a timestamp read off a file name and a number typed by hand; there is nothing to \
                 propose"
            }
            Self::WalkBack => {
                "runs after finish on every reply, model- or hand-made, so it cannot depend on who \
                 made the reply"
            }
            Self::RenderArithmetic => {
                "seconds divided by rates, gains multiplied, text measured against a box: arithmetic, \
                 and the render does it again anyway"
            }
            Self::PipelineChoice => {
                "not per-item at all: one setting for the project, said in the log with its value \
                 and reason"
            }
        }
    }

    pub fn visible_afterwards(self) -> &'static str {
        match self {
            Self::WordTimes => "the transcript line carries its word timings on the timeline",
            Self::EdgeBetweenKnownWords => {
                "the placed edge is drawn where the sound put it, and named in the mark's report"
            }
            Self::SessionClock => "every lane sits where the clock put it, shifts included",
            Self::WalkBack => {
                "the page shows what the walk-back did, and the log says how much moved"
            }
            Self::RenderArithmetic => {
                "the rendered picture and the fit warnings state the numbers that were used"
            }
            Self::PipelineChoice => {
                "the log says each field this derivation set, with value and reason"
            }
        }
    }
}

/// Whether something may be taken out of the model's hands: the same three questions as [`decision_audit::walk_back_admits`], so the two tables cannot drift.
pub fn admits_to_hands_off(mechanical: bool, checkable: bool, visible_afterwards: bool) -> bool {
    // `checkable` maps onto the audit's "explainable in one sentence".
    decision_audit::walk_back_admits(mechanical, checkable, visible_afterwards)
}

pub fn all_admitted() -> bool {
    HandsOff::all()
        .iter()
        .all(|h| !h.why_it_is_hands_off().is_empty() && !h.visible_afterwards().is_empty())
}

// --- the pipeline fields ------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineField {
    pub field: &'static str,
    pub id: &'static str,
    pub decides: &'static str,
}

/// The three fields F0.7's `set_policy` writes from the User Context. All six writable fields are on [`crate::project::Policy`], with the `P.policy.*` ids [`crate::policy::derived_fields`] catalogues.
pub fn pipeline_fields() -> [PipelineField; 3] {
    [
        PipelineField {
            field: "markingPass",
            id: "P.policy.markingPass",
            decides: "which marking pass runs: joins, retakes, or none",
        },
        PipelineField {
            field: "cutMode",
            id: "P.policy.cutMode",
            decides: "whether the cut comes from the marked words or from a model",
        },
        PipelineField {
            field: "cut-stage passes",
            id: "P.policy.captionsPass / P.policy.speedPass / P.policy.decorationsPass",
            decides: "which cut-stage passes are asked at all",
        },
    ]
}

pub fn cut_modes() -> [CutMode; 2] {
    [CutMode::Words, CutMode::Model]
}

pub fn marking_passes() -> [MarkingPass; 3] {
    [MarkingPass::Joins, MarkingPass::Retakes, MarkingPass::None]
}

// --- the audit table ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ruled {
    pub what: &'static str,
    pub home: &'static str,
    pub lives_in: &'static str,
}

pub fn audit() -> [Ruled; 6] {
    [
        Ruled {
            what: "word times",
            home: "the aligner's, never the model's (rule 2.1)",
            lives_in:
                "align::pieces and the aligned word records; narrate_pass::at_offset takes the \
                      clip-relative offset the writer was shown, so no timestamp is ever asked for",
        },
        Ruled {
            what: "where a cut lands between two known words",
            home: "the audio envelope chooses; the model chose which words (rule 2.2)",
            lives_in: "edges::Edges::end_after and start_before, bounded by edges::EDGE_REACH",
        },
        Ruled {
            what: "the session clock, source placement and hand shift corrections",
            home: "read off file names and typed by hand; nothing to propose",
            lives_in: "clock::clock, clock::name_stamp, Clock::offset; the hand correction is \
                      cut_trim::slide_shift into the Cut's shift map",
        },
        Ruled {
            what: "edge snapping, hole closing, dead-air removal, coalescing, clamping",
            home: "the walk-back (rule 2.6), after finish on every reply",
            lives_in: "tools::cutpass::Plan::snap within P.policy.snapToleranceSeconds; \
                      cut_speed_pass::merge; cut_clamp::clamp_to_cut under \
                      cut_clamp::MIN_SURVIVING_SECONDS (P.eng.effectMinSurvivingSeconds); dead air \
                      by cut_effect_decisions::dead_air_is_cut against P.policy.deadAirMaxSeconds",
        },
        Ruled {
            what: "the render's arithmetic: fitting, ducking, loudness, frame boxes, cue wrapping",
            home: "division and multiplication over numbers the app already holds",
            lives_in: "produce_render::fit up to P.eng.narrationMaxTempo; narrate_preview::duck; \
                      separate::loudness_log; produce_subtitles::wrap at ROW_CHARS",
        },
        Ruled {
            what: "nothing chooses the pipeline but the User Context",
            home: "policy fields the model sets once, visibly in the log (F0.7 set_policy)",
            lives_in: "project::Policy::marking_pass, ::cut_mode and the three pass switches, stored by \
                      policy::apply — which takes whatever the User Context speaks to, since F0.7 has no \
                      form and no hand-set precedence — and said by policy::derived_log with value and \
                      reason; the switches are read back through policy::pass_runs and filtered for the \
                      run by cut_suggest_passes::order",
        },
    ]
}

/// The parameter ids §4's rows turn on. Not here: `P.policy.deadAirMaxSeconds` / `deadAirKeepSeconds` have no row yet (see `params::cut()`; the bound is caller-supplied in [`crate::cut_effect_decisions::dead_air_is_cut`]), and `markingPass`, `cutMode` and the cut-stage switches are project fields with serde names, so [`pipeline_fields`] uses the field name as id.
pub const CITED_PARAMS: [&str; 4] = [
    "P.policy.snapToleranceSeconds",
    "P.eng.effectMinSurvivingSeconds",
    "P.eng.narrationMaxTempo",
    "P.policy.gameVolume",
];

pub const CITED_TOOLS: [&str; 2] = ["set_policy", "get_context"];
