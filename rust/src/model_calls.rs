//! Model calls at a glance — `spec/11-flow-index.md` §4.
//!
//! §4 is one table: for each of the five steps, which models it asks and how many times. Every
//! count in it already has an owner elsewhere in this tree; nothing here re-declares a number. What
//! this module adds is the aggregation — a step's calls as one list a status line or a log can
//! print, with the two flags that decide whether the call costs anything again (`web`, `cached`)
//! read from the tables that already answer them ([`roles::web_tools`] and
//! [`crate::llm_cache::uses_cache`]) rather than restated per row.
//!
//! Three calls cannot be read from those tables and state their flags directly, each for a
//! different reason:
//! - `policy` — F0.7's derivation has no row in §2's job table (it asks the LLM but is not one
//!   of the fourteen jobs), so it has neither web nor cache to inherit.
//! - `tts` — not an LLM call at all: audio.cpp speaks it, and its cache is the frozen take key
//!   ([`crate::narration::tts_file`]), a different store from `cache/llm`.
//! - `sd.cpp` — likewise an image server, asked once and never cached.

use crate::{
    cut_captions, llm_cache, narrate_pass, prepare, produce_subtitles,
    project::{CutMode, MarkingPass},
    roles::{self, Job},
};

/// The five rows of §4's table. Four are pages; `Setup` is not — it is the step F0.7 runs when the
/// User Context changes, which is why it appears here and nowhere in [`crate::shell::Page`].
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
    /// The table's own notation: `describe ×10 (+web) · cached`. A call that asks nothing renders
    /// as nothing — §4 counts requests, and showing `translate ×0` on a project that translates
    /// nothing would read like a step that ran and found nothing rather than one nobody asked.
    /// (§4 is silent on the zero case.)
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
        Self { stage, calls: Vec::new() }
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
    Call { model, calls, web: roles::web_tools(job), cached: llm_cache::uses_cache(job) }
}

/// Prepare: `describe ×(frames/4) · fix ×(lines/25) · retake ×3 or textedit ×(joins)`, cached.
///
/// Both divisors are other modules' constants — four frames a vision call
/// (`P.machine.describeFramesPerReq`) and 25 lines a fixer block (`P.machine.fixBlockLines`) —
/// read through the functions that build those requests, so this cannot disagree with a retuned
/// batch size. The marking pass is the table's "or": `P.policy.markingPass` picks exactly one of
/// the three pooled retake runs or one textedit call per join, and `None` asks nobody
/// ([`crate::fix_transcripts::marks_for`] treats that as a policy, not an omission).
pub fn prepare(frames: usize, lines: usize, marking: MarkingPass, joins: usize) -> Summary {
    let mut s = Summary::new(Stage::Prepare);
    s.calls.push(from_job(
        "describe",
        Job::Describe,
        prepare::vision_calls(frames),
    ));
    s.calls.push(from_job("fix", Job::CleanTranscript, prepare::fixer_calls(lines)));
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
///
/// "cutMode model only": under `CutMode::Words` the timeline comes from the marked text and the
/// cut model is not asked at all ([`roles::job_applies`] says the same thing from the job side).
/// The three passes ride only when their switches are on — `P.policy.captionsPass`,
/// `P.policy.speedPass` and `P.policy.decorationsPass`, taken from the User Context by F0.7's
/// form. Those three are §10 rows with no field on [`crate::project::Policy`] yet (it carries
/// `marking_pass` and `cut_mode` alone), so they arrive as arguments here rather than being read
/// off the project; when the policy item gives them fields, the caller passes them straight in.
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
///
/// The writer is *not* cached — [`narrate_pass::served_from_cache`] answers false because the
/// same brief must be rewritten when the prompt is edited or a cut moves, and the expensive half
/// is the voice rather than the words. The voices are cached, under the frozen TTS take key
/// ([`crate::narration::tts_file`]): a line nobody changed costs nothing to re-speak, and
/// changing that key re-speaks every project. Hence one row not cached and one cached, which is
/// the pair §4's blank note column leaves for this step to explain.
pub fn narrate(lines: usize) -> Summary {
    let mut s = Summary::new(Stage::Narrate);
    s.calls.push(Call {
        model: "narrate",
        calls: 1,
        web: roles::web_tools(Job::Narrate),
        cached: narrate_pass::served_from_cache(),
    });
    // Not an LLM call, so neither flag is inherited: audio.cpp speaks, and its own cache holds it.
    s.calls.push(Call { model: "tts", calls: lines, web: false, cached: true });
    s
}

/// Produce: `youtube ×1 (+web) · sd.cpp ×1 when drawn · translate ×(languages × batches)`.
///
/// The thumbnail is asked only under [`roles::thumbnail_wanted`] — an instruction exists and no
/// frame was named, because a named frame is cut out of the source and needs no server. The
/// translation count is the product §4 spells out: batches of `P.machine.translateBatch` lines
/// per language, so a track of 320 cues in two languages is six requests.
pub fn produce(instruction: &str, frame_named: bool, cues: usize, languages: usize) -> Summary {
    let mut s = Summary::new(Stage::Produce);
    s.calls.push(from_job("youtube", Job::UploadText, 1));
    if roles::thumbnail_wanted(instruction, frame_named) {
        // sd.cpp draws once and caches nothing: there is no repeat of an image to replay.
        s.calls.push(Call { model: "sd.cpp", calls: 1, web: false, cached: false });
    }
    s.calls.push(from_job(
        "translate",
        Job::Subtitles,
        produce_subtitles::batches(cues).len() * languages,
    ));
    s
}

/// Setup: `policy ×1` — new, run when the User Context changes.
///
/// F0.7 fires on a debounce after the context is edited, on the next ▶, or on demand from the
/// policy form, so one edit costs one call rather than one per keystroke. It is not one of
/// `cache/llm`'s five jobs and gets no web tools, so both flags are stated: an unchanged context
/// still costs the call it is asked with, and the debounce is what keeps that from costing many.
pub fn setup() -> Summary {
    let mut s = Summary::new(Stage::Setup);
    s.calls.push(Call { model: "policy", calls: 1, web: false, cached: false });
    s
}
