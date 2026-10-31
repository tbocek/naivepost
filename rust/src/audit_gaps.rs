//! Gaps this audit closed — `spec/12-decisions.md` §5.

use crate::narration::{self, Entry, Silent};
use crate::roles::Job;
use crate::tools::{self, Tool};

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

/// The eight additions, in §5's order.
pub fn additions() -> [Added; 8] {
    [
        Added {
            name: "get_frames",
            kind: AdditionKind::Tool,
            why: "promised in the principles chapter and never defined until this audit asked who shows \
                 the model a picture",
        },
        Added {
            name: "speech_around",
            kind: AdditionKind::Tool,
            why: "the brief extracts two lines per side; a describer asking about a stretch needs every \
                 line spoken in it, from every recording",
        },
        Added {
            name: "flag_line",
            kind: AdditionKind::Tool,
            why: "a wrong speaker or mis-timed row is 'not yours to fix' with nowhere to say so; this \
                 is the channel where the note is logged and the line stands",
        },
        Added {
            name: "get_words",
            kind: AdditionKind::Tool,
            why: "the seam brief shows a fixed reach; without more words the model cannot tell whether \
                 it is being short-changed at the join",
        },
        Added {
            name: "cut_status",
            kind: AdditionKind::Tool,
            why: "the same reading finish_cut gives, available at any time, so a cut is not discovered \
                 short only at the end",
        },
        Added {
            name: "list_emotions",
            kind: AdditionKind::Tool,
            why: "the eight bases, their kin words and the named blends were a table the prototype kept \
                 to itself, so an unknown tag silently meant something else",
        },
        Added {
            name: "describe_insert",
            kind: AdditionKind::Tool,
            why: "the prototype passed only the file name, so a full-screen graphic was known to the \
                  writer as tier.svg?S=Dust II",
        },
        Added {
            name: "box",
            kind: AdditionKind::Argument,
            why: "add_effect without a box could only punch into the middle; the prompt asks for zooms \
                  onto the score, the face, the mistake",
        },
    ]
}

/// The read set — tools whose answer tells the model something rather than changing anything.
const READS: [Tool; 9] = [
    Tool::GetContext,
    Tool::GetLines,
    Tool::GetEvents,
    Tool::GetFrames,
    Tool::SpeechAround,
    Tool::GetWords,
    Tool::ListEmotions,
    Tool::CutStatus,
    Tool::DescribeInsert,
];

/// Is this tool a read?
pub fn is_read(tool: Tool) -> bool {
    READS.contains(&tool)
}

/// The jobs §5's every-job-names-its-reads claim covers.
pub fn covered_jobs() -> [Job; 7] {
    [
        Job::Describe,
        Job::CleanTranscript,
        Job::Retakes,
        Job::Joins,
        Job::ModelCut,
        Job::ClipRules,
        Job::Narrate,
    ]
}

/// Plus the two output-text jobs, which also get reads and are part of the same claim.
pub fn covered_output_jobs() -> [Job; 3] {
    [Job::UploadText, Job::Thumbnail, Job::Subtitles]
}

/// Every job the claim covers, in the order the pipeline runs them.
pub fn all_covered_jobs() -> Vec<Job> {
    let mut jobs: Vec<Job> = covered_jobs().to_vec();
    jobs.extend(covered_output_jobs());
    jobs
}

/// Does this job get at least one read tool alongside whatever it writes?
pub fn job_names_reads(job: Job) -> bool {
    tools::offered(job).iter().any(|t| is_read(*t))
}

/// How many reads the job was offered — the number a failure message should show, because §5's contrast
/// ("before, only the policy job had one") is about counts, not presence.
pub fn read_count(job: Job) -> usize {
    tools::offered(job).iter().filter(|t| is_read(**t)).count()
}

/// The pair F0.7's policy job gets, named here because §5 points at it as the only read the old jobs
/// had: `get_context` beside `set_policy`.
pub fn policy_pair() -> Vec<Tool> {
    tools::policy_tools()
}

/// Whether a tool reply says more than bare success.
pub fn result_carries_more_than_ok(reply: &str) -> bool {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(reply) else {
        return false;
    };
    if body.get("error").is_some() {
        return false;
    }
    match body.as_object() {
        Some(map) => !map.is_empty(),
        None => false,
    }
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
            Self::AtThreeReadings => {
                "narration::output_seconds, reached through \
                                    audit_gaps::lands_at"
            }
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

/// Where a line lands in the clip's output seconds.
pub fn lands_at(at: f64, rate: f64) -> f64 {
    narration::output_seconds(at, rate)
}

/// Whether a placement comes from the app rather than from translated text.
pub fn tag_is_app_owned(pos: &str) -> bool {
    matches!(
        pos.trim().to_ascii_lowercase().as_str(),
        "top" | "center" | "centre" | "middle" | "bottom" | ""
    )
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
