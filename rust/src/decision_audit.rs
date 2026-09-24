//! The five homes of a decision — directive B's audit table, `spec/12-decisions.md` §1.
//!
//! Every model job finishes by calling tools, and around each call sit five places a decision can
//! live. What separates them is not where the code runs but **what the model is told**:
//!
//! | home | the model … | scope |
//! |---|---|---|
//! | tool argument | states it | one item |
//! | tool result | is told what the app made of its answer; may act on it | one item |
//! | `finish` answer | is told what is still wrong with the whole; may fix it | the whole answer |
//! | the walk-back (after `finish`) | never sees it | after the answer |
//! | policy / prompt | neither: a number or wording | no judgement at all |
//!
//! Three of the five are heard, one is deliberately unseen, and one is not a decision about this
//! video at all. The point of sorting them is the announcement discipline in [`change_is_announced`]
//! — the rule of thumb that if the app changes what the model said, the model hears about it.
//!
//! This is a different question from [`crate::decision_homes`] (directive A), which sorts the app's
//! *numbers and wording* by who owns them. Here the unit is one decision inside one conversation
//! with a model. The two meet at the last row: a policy number or a prompt wording is §1's fifth
//! home and directive A's first two at once, which [`is_number_or_wording`] states rather than
//! leaving to be noticed.

use crate::decision_homes;

/// One of §1's five homes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Home {
    /// "states it" — the model's own words arriving as a tool argument.
    ToolArgument,
    /// "is told what the app made of its answer; may act on it in the same conversation".
    ToolResult,
    /// "is told what is still wrong with the whole; may fix it".
    FinishAnswer,
    /// "never sees it — so it must be mechanical, explainable in one sentence, visible on screen
    /// afterwards".
    WalkBack,
    /// "neither: a number or wording, not a per-item judgement".
    PolicyOrPrompt,
}

impl Home {
    pub fn all() -> [Home; 5] {
        [
            Self::ToolArgument,
            Self::ToolResult,
            Self::FinishAnswer,
            Self::WalkBack,
            Self::PolicyOrPrompt,
        ]
    }

    /// The home as §1 names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::ToolArgument => "tool argument",
            Self::ToolResult => "tool result",
            Self::FinishAnswer => "`finish` answer",
            Self::WalkBack => "the walk-back",
            Self::PolicyOrPrompt => "policy / prompt",
        }
    }

    /// Whether the model has this decision in front of it at all.
    ///
    /// The walk-back's invisibility is not an oversight: it is the reason
    /// [`walk_back_admits`] demands three things before a decision may go there. An unseen change
    /// that is not mechanical, not explainable and not visible afterwards is a change nobody can
    /// account for.
    pub fn model_sees_it(self) -> bool {
        match self {
            Self::ToolArgument | Self::ToolResult | Self::FinishAnswer => true,
            Self::WalkBack | Self::PolicyOrPrompt => false,
        }
    }

    /// Whether the model may react to this in the same conversation.
    ///
    /// Only the two cells §1 spells with "may act on it" / "may fix it". A tool argument is the
    /// model's own statement, so there is nothing for it to react to; the last two homes are not
    /// seen, so they cannot be acted on either.
    pub fn may_act_in_same_conversation(self) -> bool {
        matches!(self, Self::ToolResult | Self::FinishAnswer)
    }

    /// What the decision is about — the split the rule of thumb turns on. Per-item changes travel in
    /// the tool's answer; changes about the whole travel at `finish`.
    pub fn scope(self) -> &'static str {
        match self {
            Self::ToolArgument | Self::ToolResult => "one item",
            Self::FinishAnswer => "the whole answer",
            Self::WalkBack => "after the answer",
            Self::PolicyOrPrompt => "no judgement at all",
        }
    }
}

/// Whether a decision may live in the walk-back.
///
/// §1 gives the walk-back three requirements in one dash clause, and all three are load-bearing.
/// Invisibility is only safe when a person can later see the result on screen, read why it moved
/// in a single sentence, and know that no judgement about the video went into it. Miss any one of
/// the three and the decision belongs at `finish` instead, where the model hears about it and can
/// fix it — because an unseeable, unexplainable change is indistinguishable from a silent
/// overrule, which is exactly what this audit exists to move out of the app.
pub fn walk_back_admits(
    mechanical: bool,
    explainable_in_one_sentence: bool,
    visible_afterwards: bool,
) -> bool {
    mechanical && explainable_in_one_sentence && visible_afterwards
}

/// Whether a change made at this home reaches the model's ears.
///
/// This is §1's rule of thumb as code: *"if the app changes what the model said, the model hears
/// about it"* — per item in the tool's answer ([`Home::ToolResult`], which reports the snapped
/// edges and how far each moved), for the whole at `finish` ([`Home::FinishAnswer`], which says
/// what is still wrong with the cut as a set).
///
/// [`Home::WalkBack`] returning false is not an exception to the rule. The walk-back does not
/// change what the model said; it places what the model proposed, which is rule 2.1 — *"models
/// propose, the machine places."* Nothing of the model's answer was altered, so there is nothing
/// to announce.
pub fn change_is_announced(home: Home) -> bool {
    matches!(home, Home::ToolResult | Home::FinishAnswer)
}

/// Whether this directive-A home holds a number or a wording rather than a per-item judgement.
///
/// §1's fifth home spans two of directive A's four: a policy value and a prompt wording are the
/// same kind of non-decision — fixed before the conversation, applied to every item alike, never
/// argued per clip. Machine settings and engineering constants are also not judgements, but they
/// are not §1's fifth home either: they decide how the servers run and how the app behaves, not
/// what this video becomes.
pub fn is_number_or_wording(home: decision_homes::Home) -> bool {
    matches!(home, decision_homes::Home::Policy | decision_homes::Home::Prompt)
}

/// One of §1's examples, tied to the code path that actually makes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Example {
    pub home: Home,
    /// The example as §1 words it.
    pub what: &'static str,
    /// Where it happens, so the table cannot describe a decision this build does not make.
    pub lives_in: &'static str,
}

/// The table's examples, in the order §1 lists them.
pub fn examples() -> [Example; 7] {
    [
        Example {
            home: Home::ToolArgument,
            what: "which lines are an abandoned attempt",
            lives_in: "tools::retakes::Marks::mark_abandoned \u{2014} line numbers, never seconds",
        },
        Example {
            home: Home::ToolResult,
            what: "a segment's snapped edges and how far they moved",
            lives_in: "tools::cutpass::Plan::add_segment's `span` + `moved`, snapped within \
                      P.policy.snapToleranceSeconds",
        },
        Example {
            home: Home::FinishAnswer,
            what: "fewer segments than the target allows",
            lives_in: "tools::cutpass::cut_problems, joined by finish_cut",
        },
        Example {
            home: Home::FinishAnswer,
            what: "a clip with no line",
            lives_in: "narrate_pass::no_line_fault",
        },
        Example {
            home: Home::WalkBack,
            what: "placing a cut edge on the audio envelope",
            lives_in: "edges::place_edges",
        },
        Example {
            home: Home::PolicyOrPrompt,
            what: "the 40 % ceiling on marks",
            lives_in: "tools::retakes::RETAKE_CEIL = P.machine.retakeCeil",
        },
        Example {
            home: Home::PolicyOrPrompt,
            what: "clean it like a subtitler",
            lives_in: "spec/prompts/captions.md",
        },
    ]
}
