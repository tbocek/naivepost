//! Model calls at a glance — `spec/11-flow-index.md` §4.

use crate::{
    cut_captions, llm_cache, narrate_pass, prepare, produce_subtitles,
    project::{CutMode, MarkingPass},
    roles::{self, Job},
};

/// The five rows of §4's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Prepare,
    Cut,
    Narrate,
    Produce,
    Setup,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Prepare => "Prepare",
            Self::Cut => "Cut",
            Self::Narrate => "Narrate",
            Self::Produce => "Produce",
            Self::Setup => "Setup",
        }
    }
}

/// One model's share of a step: how many requests it takes and what rode with them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub model: &'static str,
    pub calls: usize,
    /// The web tools went with the request (§2's `(+ web tools)` rows).
    pub web: bool,
    /// An unchanged repeat of this call is answered from disk instead of asked again.
    pub cached: bool,
}

impl Call {
    /// The table's own notation: `describe ×10 (+web) · cached`.
    pub fn line(&self) -> String {
        if self.calls == 0 {
            return String::new();
        }
        let mut out = format!("{} \u{d7}{}", self.model, self.calls);
        if self.web {
            out.push_str(" (+web)");
        }
        if self.cached {
            out.push_str(" \u{b7} cached");
        }
        out
    }
}

/// One step's whole bill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub stage: Stage,
    pub calls: Vec<Call>,
}

impl Summary {
    fn new(stage: Stage) -> Self {
        Self {
            stage,
            calls: Vec::new(),
        }
    }

    /// Total requests this step makes.
    pub fn asked(&self) -> usize {
        self.calls.iter().map(|call| call.calls).sum()
    }

    /// The step as one line: its calls joined by ` · `, empty when the step asks nobody.
    pub fn one_line(&self) -> String {
        self.calls
            .iter()
            .map(Call::line)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} ")
    }

    /// The call for one model, if this step makes any.
    pub fn of(&self, model: &str) -> Option<&Call> {
        self.calls.iter().find(|call| call.model == model)
    }
}

/// A call whose flags come from §2's two tables rather than being typed here.
fn from_job(model: &'static str, job: Job, calls: usize) -> Call {
    Call {
        model,
        calls,
        web: roles::web_tools(job),
        cached: llm_cache::uses_cache(job),
    }
}

/// Prepare: `describe ×(frames/4) · fix ×(lines/25) · retake ×3 or textedit ×(joins)`, cached.
pub fn prepare(frames: usize, lines: usize, marking: MarkingPass, joins: usize) -> Summary {
    let mut s = Summary::new(Stage::Prepare);
    s.calls.push(from_job(
        "describe",
        Job::Describe,
        prepare::vision_calls(frames),
    ));
    s.calls.push(from_job(
        "fix",
        Job::CleanTranscript,
        prepare::fixer_calls(lines),
    ));
    match marking {
        MarkingPass::Retakes => s.calls.push(from_job(
            "retake",
            Job::Retakes,
            roles::RETAKE_RUNS_POOLED as usize,
        )),
        MarkingPass::Joins => s.calls.push(from_job("textedit", Job::Joins, joins)),
        MarkingPass::None => {}
    }
    s
}

/// Cut: `cut ×1 (+web) · captions ×(clips/5) · speed ×1 · effects ×1`.
pub fn cut(
    clips: usize,
    cut_mode: CutMode,
    captions_on: bool,
    speed_on: bool,
    decorations_on: bool,
) -> Summary {
    let mut s = Summary::new(Stage::Cut);
    if cut_mode == CutMode::Model {
        s.calls.push(from_job("cut", Job::ModelCut, 1));
    }
    if captions_on {
        s.calls.push(from_job(
            "captions",
            Job::ClipRules,
            cut_captions::batches(clips).len(),
        ));
    }
    if speed_on {
        s.calls.push(from_job("speed", Job::ClipRules, 1));
    }
    if decorations_on {
        s.calls.push(from_job("effects", Job::ClipRules, 1));
    }
    s
}

/// Narrate: `narrate ×1 (+web) · TTS ×(lines)`.
pub fn narrate(lines: usize) -> Summary {
    let mut s = Summary::new(Stage::Narrate);
    s.calls.push(Call {
        model: "narrate",
        calls: 1,
        web: roles::web_tools(Job::Narrate),
        cached: narrate_pass::served_from_cache(),
    });
    s.calls.push(Call {
        model: "tts",
        calls: lines,
        web: false,
        cached: true,
    });
    s
}

/// Produce: `youtube ×1 (+web) · sd.cpp ×1 when drawn · translate ×(languages × batches)`.
pub fn produce(instruction: &str, frame_named: bool, cues: usize, languages: usize) -> Summary {
    let mut s = Summary::new(Stage::Produce);
    s.calls.push(from_job("youtube", Job::UploadText, 1));
    if roles::thumbnail_wanted(instruction, frame_named) {
        s.calls.push(Call {
            model: "sd.cpp",
            calls: 1,
            web: false,
            cached: false,
        });
    }
    s.calls.push(from_job(
        "translate",
        Job::Subtitles,
        produce_subtitles::batches(cues).len() * languages,
    ));
    s
}

/// Setup: `policy ×1` — new, run when the User Context changes.
pub fn setup() -> Summary {
    let mut s = Summary::new(Stage::Setup);
    s.calls.push(Call {
        model: "policy",
        calls: 1,
        web: false,
        cached: false,
    });
    s
}
