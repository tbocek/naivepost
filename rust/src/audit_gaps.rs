//! Gaps this audit closed — `spec/12-decisions.md` §5.

use crate::narration::{Entry, Silent};

/// Whether an addition is a whole tool or an argument added to one that already existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdditionKind {
    /// A new entry in the catalogue.
    Tool,
    /// A new argument on an existing tool.
    Argument,
}

/// One thing §5 says was added, and the gap it closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Added {
    pub name: &'static str,
    pub kind: AdditionKind,
    /// The gap, in the terms §5 gives for it.
    pub why: &'static str,
}

/// A prototype behaviour §5 marks as a defect rather than a design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Defect {
    /// The narration run wiped the user's hand-curated silent list.
    SilentListWipe,
    /// The cue's placement tag travelled into the translation as a word.
    PlacementTagTravels,
    /// The title was printed onto the picture only once, ever.
    TitlePrintedOnce,
    /// A narration line's `at` on a sped-up clip was read three different ways.
    AtThreeReadings,
}

impl Defect {
    pub fn all() -> [Defect; 4] {
        [
            Self::SilentListWipe,
            Self::PlacementTagTravels,
            Self::TitlePrintedOnce,
            Self::AtThreeReadings,
        ]
    }

    /// Exactly two of the four carry §5's MUST fix; the other two are recorded as decided elsewhere.
    pub fn must_fix(self) -> bool {
        matches!(self, Self::SilentListWipe | Self::PlacementTagTravels)
    }

    /// Where the decision that settles it is written.
    pub fn decided_in(self) -> &'static str {
        match self {
            Self::SilentListWipe => "07-narrate.md F4.1 (the silent list is the user's)",
            Self::PlacementTagTravels => {
                "02-services.md 3.10 (the tag is not the model's to carry)"
            }
            Self::TitlePrintedOnce => "08-produce.md F5.6 (reprint whenever the text changes)",
            Self::AtThreeReadings => "01-project-and-files.md 4 (the recording's own clock)",
        }
    }

    /// The live function that closes it, so a row cannot outlive the code it describes.
    pub fn closed_by(self) -> &'static str {
        match self {
            Self::SilentListWipe => {
                "audit_gaps::silent_survives_entry_rewrite over \
                                    narration::Narration.silent, kept by bounds"
            }
            Self::PlacementTagTravels => {
                "produce_subtitles::placement plus the tag-stripping outbound \
                                        builder (sent_line / numbered)"
            }
            Self::TitlePrintedOnce => {
                "audit_gaps::reprint_title_on_change, mirroring \
                                     youtube_title_redraws_picture"
            }
            Self::AtThreeReadings => "narration::output_seconds",
        }
    }
}

/// The user's silent list survives a rewrite of the lines.
pub fn silent_survives_entry_rewrite(
    silent: &[Silent],
    _old_entries: &[Entry],
    _new_entries: &[Entry],
) -> Vec<Silent> {
    silent.to_vec()
}

/// Whether the picture's title gets reprinted.
pub fn reprint_title_on_change(previous: &str, now: &str, picture_edited_by_hand: bool) -> bool {
    !picture_edited_by_hand && previous != now
}

/// The parameter ids §5's rows turn on.
pub const CITED_PARAMS: [&str; 1] = ["P.policy.subtitleRowChars"];

/// Every tool §5 names as added, plus the policy tool the read/write split turns on.
pub const CITED_TOOLS: [&str; 9] = [
    "get_frames",
    "speech_around",
    "flag_line",
    "get_words",
    "cut_status",
    "list_emotions",
    "describe_insert",
    "add_effect",
    "set_policy",
];
