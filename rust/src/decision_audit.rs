//! The five homes of a decision — directive B's audit table, `spec/12-decisions.md` §1.

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
    pub fn model_sees_it(self) -> bool {
        match self {
            Self::ToolArgument | Self::ToolResult | Self::FinishAnswer => true,
            Self::WalkBack | Self::PolicyOrPrompt => false,
        }
    }

    /// Whether the model may react to this in the same conversation.
    pub fn may_act_in_same_conversation(self) -> bool {
        matches!(self, Self::ToolResult | Self::FinishAnswer)
    }

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
pub fn walk_back_admits(
    mechanical: bool,
    explainable_in_one_sentence: bool,
    visible_afterwards: bool,
) -> bool {
    mechanical && explainable_in_one_sentence && visible_afterwards
}

/// Whether a change made at this home reaches the model's ears.
pub fn change_is_announced(home: Home) -> bool {
    matches!(home, Home::ToolResult | Home::FinishAnswer)
}

/// Whether this directive-A home holds a number or a wording rather than a per-item judgement.
pub fn is_number_or_wording(home: decision_homes::Home) -> bool {
    matches!(
        home,
        decision_homes::Home::Policy | decision_homes::Home::Prompt
    )
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
