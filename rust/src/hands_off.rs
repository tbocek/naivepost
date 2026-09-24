//! What stays out of the model's hands, on purpose — `spec/12-decisions.md` §4.
//!
//! §4 is the mirror of [`crate::decision_audit`] (the five homes) and of
//! [`crate::prepare_decisions`] / [`crate::cut_effect_decisions`] (per-flow audits). Those ask
//! *where a decision lives*; this asks what the model must **never** be asked to decide, and the test
//! for admission is the same three-part one: mechanical, checkable, and visible on screen afterwards.
//! Nothing on this list is a judgement about the video — if it were, it would belong in a tool argument
//! or at `finish`, where the model hears about it and can argue back.
//!
//! Six bullets, six rows. Five are arithmetic the app already owns elsewhere; the sixth is the pipeline
//! itself, which the model sets **once**, visibly, through F0.7's `set_policy`, rather than per call.

use crate::decision_audit;
use crate::decision_homes;
use crate::project::{CutMode, MarkingPass, Origin};

/// One thing the model never decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandsOff {
    /// Word times are the aligner's (rule 2.1): a model is never asked to compute a timestamp.
    WordTimes,
    /// Where between two known words a splice falls (rule 2.2): the envelope chooses, the model chose
    /// which words.
    EdgeBetweenKnownWords,
    /// The session clock: source placement by file-name stamp, plus hand shift corrections.
    SessionClock,
    /// Snapping, hole closing, dead-air removal, coalescing, clamping — the walk-back (rule 2.6).
    WalkBack,
    /// Fitting, ducking, loudness, frame boxes, cue wrapping: the render's numbers.
    RenderArithmetic,
    /// Which passes run at all. Set once from the User Context in the policy form, not per request.
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

    /// The bullet as §4 heads it.
    pub fn label(self) -> &'static str {
        match self {
            Self::WordTimes => "word times",
            Self::EdgeBetweenKnownWords => "where a cut lands between two known words",
            Self::SessionClock => "the session clock",
            Self::WalkBack => "edge snapping, hole closing, dead-air removal, coalescing, clamping",
            Self::RenderArithmetic => "the render's arithmetic",
            Self::PipelineChoice => "which pass runs, and whether the cut comes from words or a model",
        }
    }

    /// Why this is mechanical rather than a judgement about the video.
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
                "not per-item at all: one setting for the project, shown in the policy form with its \
                 origin beside it"
            }
        }
    }

    /// How the user sees it afterwards — the third part of the admission test. A hands-off change the
    /// page never shows is indistinguishable from a silent overrule.
    pub fn visible_afterwards(self) -> &'static str {
        match self {
            Self::WordTimes => "the transcript line carries its word timings on the timeline",
            Self::EdgeBetweenKnownWords => {
                "the placed edge is drawn where the sound put it, and named in the mark's report"
            }
            Self::SessionClock => "every lane sits where the clock put it, shifts included",
            Self::WalkBack => "the page shows what the walk-back did, and the log says how much moved",
            Self::RenderArithmetic => {
                "the rendered picture and the fit warnings state the numbers that were used"
            }
            Self::PipelineChoice => "the policy form lists each field with its source beside it",
        }
    }
}

/// Whether something may be taken out of the model's hands.
///
/// This is [`decision_audit::walk_back_admits`] seen from the other side: the walk-back is one of the
/// six categories here, and both ask the same three questions, because the reason a change is safe to
/// make without the model is exactly the reason it is safe to make without asking anyone. Requiring all
/// three keeps the two tables from drifting apart — if a candidate fails here it also fails there, and
/// belongs at `finish` where the model hears about it.
pub fn admits_to_hands_off(
    mechanical: bool,
    checkable: bool,
    visible_afterwards: bool,
) -> bool {
    // `checkable` maps onto the audit's "explainable in one sentence": a rule nobody can state cannot be
    // checked by a person looking at the result. Both predicates are asked because §4 and §1's
    // walk-back row must not drift into two different bars for the same admission.
    decision_audit::walk_back_admits(mechanical, checkable, visible_afterwards)
}

/// Every category passes the admission test — that is §4's opening claim, stated as code so a new
/// category added to the enum has to answer the three questions before it can join the list.
pub fn all_admitted() -> bool {
    HandsOff::all()
        .iter()
        .all(|h| !h.why_it_is_hands_off().is_empty() && !h.visible_afterwards().is_empty())
}

// --- the pipeline fields ------------------------------------------------------------------------

/// One field that decides the pipeline instead of a style dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineField {
    /// The field's name as the policy form shows it.
    pub field: &'static str,
    /// The live id it answers to (a `P.policy.*` row or the project field's serde name).
    pub id: &'static str,
    /// What choosing it changes.
    pub decides: &'static str,
}

/// The three fields F0.7's `set_policy` writes from the User Context.
///
/// The first two are real fields on [`crate::project::Policy`]. The third is deliberately described as
/// a set of switches rather than one field: `P.policy.captionsPass`, `P.policy.speedPass` and
/// `P.policy.decorationsPass` are §10 rows that [`crate::model_calls`] reads, but they have no field
/// on `Policy` yet — recorded here as it is rather than pretending the struct carries them.
pub fn pipeline_fields() -> [PipelineField; 3] {
    [
        PipelineField {
            field: "markingPass",
            id: "project::Policy::marking_pass (no P.policy row; the form's own field name)",
            decides: "which marking pass runs: joins, retakes, or none",
        },
        PipelineField {
            field: "cutMode",
            id: "project::Policy::cut_mode (no P.policy row; the form's own field name)",
            decides: "whether the cut comes from the marked words or from a model",
        },
        PipelineField {
            field: "cut-stage passes",
            id: "P.policy.captionsPass / P.policy.speedPass / P.policy.decorationsPass",
            decides: "which cut-stage passes are asked at all",
        },
    ]
}

/// Whether the derivation may write this pipeline field.
///
/// Delegated whole to [`decision_homes::overridable_by_derivation`]: F0.7 S4 says a hand-set field
/// (`source: user`) is never overwritten, which is what makes the policy home derived *and* editable.
/// The model sets the pipeline once, in the form, where a person can see and overrule it; after that
/// the file decides, not the next request.
pub fn model_may_set_pipeline(origin: Origin) -> bool {
    decision_homes::overridable_by_derivation(origin)
}

/// The two ways the cut can come about, exposed so a caller can show the choice without reaching into
/// [`crate::project`].
pub fn cut_modes() -> [CutMode; 2] {
    [CutMode::Words, CutMode::Model]
}

/// The three marking passes, likewise.
pub fn marking_passes() -> [MarkingPass; 3] {
    [MarkingPass::Joins, MarkingPass::Retakes, MarkingPass::None]
}

// --- the audit table ---------------------------------------------------------------------------

/// One row of §4: the thing kept away from the model, why, and the code that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ruled {
    /// The thing, as §4 names it.
    pub what: &'static str,
    /// Why it is mechanical rather than a judgement.
    pub home: &'static str,
    /// Where the decision is actually made in this tree.
    pub lives_in: &'static str,
}

/// §4's six bullets, in order.
pub fn audit() -> [Ruled; 6] {
    [
        Ruled {
            what: "word times",
            home: "the aligner's, never the model's (rule 2.1)",
            lives_in: "align::pieces and the aligned word records; narrate_pass::at_offset takes the \
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
            home: "policy fields the model sets once, visibly, in the policy form (F0.7 set_policy)",
            lives_in: "project::Policy::marking_pass and ::cut_mode via tool:set_policy; the \
                      cut-stage switches read by model_calls; hand-set fields stick through \
                      decision_homes::overridable_by_derivation",
        },
    ]
}

/// The parameter ids §4's rows turn on.
///
/// Four are live rows in the catalogue. Note what is *not* here, because the truth matters more than a
/// tidy list: `P.policy.deadAirMaxSeconds` and `P.policy.deadAirKeepSeconds` have **no** row yet —
/// `params::cut()`'s header lists them among the ids "deliberately absent, because no rule in this
/// tree reads them yet", and the dead-air comparison lives as a caller-supplied bound in
/// [`crate::cut_effect_decisions::dead_air_is_cut`] until the removal pass lands. Likewise
/// `P.policy.markingPass`, `P.policy.cutMode` and the three cut-stage switches are project fields with
/// serde names (`markingPass`, `cutMode`) rather than tuned numbers, so [`pipeline_fields`] gives the
/// field name as their id instead of inventing catalogue rows.
pub const CITED_PARAMS: [&str; 4] = [
    "P.policy.snapToleranceSeconds",
    "P.eng.effectMinSurvivingSeconds",
    "P.eng.narrationMaxTempo",
    "P.policy.gameVolume",
];

/// The tools this item routes through.
pub const CITED_TOOLS: [&str; 2] = ["set_policy", "get_context"];
