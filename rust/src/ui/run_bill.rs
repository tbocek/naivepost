//! §4's bill (`spec/11-flow-index.md` §4), asked and shown: reads the live session, gathers the numbers §4 divides by, hands them to [`crate::model_calls`] and returns one log line saying what a press will cost.
//! Pure counting helpers at the bottom are testable without a window; helpers no door calls are named `*_for_test`.

use crate::narrate_preview_media::session_tree;
use crate::{
    cut, cut_captions, joins, layout, model_calls, narration, policy, prepare, project::Project,
    roles, run, word_list,
};

/// The bill for the step a ▶ press is starting, as one log line: `Prepare: describe ×2 · fix ×1 …`.
/// `None` when the step asks no model (§4 is silent on zero; we print nothing).
pub fn line_for(step: run::Step) -> Option<String> {
    let summary = summary_for(step)?;
    if summary.asked() == 0 {
        return None;
    }
    Some(format!("{}: {}", summary.stage.label(), summary.one_line()))
}

/// The Setup row: `policy ×1`, the call F0.7 makes on the next ▶ after the User Context changed; `derive_policy` prints it after a derivation.
pub fn setup_line() -> String {
    let summary = model_calls::setup();
    format!("{}: {}", summary.stage.label(), summary.one_line())
}

/// The bill for a step, read off the live session; split from [`line_for`] so a test can read [`model_calls::Summary`] counts.
pub fn summary_for(step: run::Step) -> Option<model_calls::Summary> {
    let project = crate::ui::window::live_project();
    let tree = session_tree();
    Some(bill_for(step, &project, tree.as_ref()))
}

/// The bill from a project and session folder handed in, so a test can build the state on disk.
pub fn bill_for(
    step: run::Step,
    project: &Project,
    tree: Option<&layout::Tree>,
) -> model_calls::Summary {
    match step {
        run::Step::Prepare => prepare_bill(project, tree),
        run::Step::Suggest => cut_bill(project, tree),
        run::Step::Narrate => narrate_bill(tree),
        run::Step::Produce => produce_bill(project, tree),
    }
}

/// Prepare: `describe ×(frames/4) · fix ×(lines/25) · retake ×3 or textedit ×(joins)`.
/// Divisors come from `roles::vision_batches`/`prepare::fixer_calls`, so retuning `P.machine.*` moves bill and readout together. The marking pass is `P.policy.markingPass`; joins are source changes in the session word list ([`joins::seams`]).
pub fn prepare_bill(project: &Project, tree: Option<&layout::Tree>) -> model_calls::Summary {
    let counts = tree.map_or_else(prepare::Counts::default, |tree| {
        prepare::count(tree, project)
    });
    let marking = policy::marking_pass_of(&project.policy);
    let joins = tree.map_or(0, seam_count);
    model_calls::prepare(counts.frames, counts.lines, marking, joins)
}

/// Cut: `cut ×1 (+web) · captions ×(clips/5) · speed ×1 · effects ×1`, each pass only when `policy::pass_runs` switches it on, so a bill never promises a pass the bar refuses.
pub fn cut_bill(project: &Project, tree: Option<&layout::Tree>) -> model_calls::Summary {
    let held = tree
        .and_then(|tree| cut::load(tree).ok())
        .unwrap_or_default();
    model_calls::cut(
        held.segs.len(),
        policy::cut_mode_of(&project.policy),
        policy::pass_runs(&project.policy, policy::Pass::Captions),
        policy::pass_runs(&project.policy, policy::Pass::Speeds),
        policy::pass_runs(&project.policy, policy::Pass::Decorations),
    )
}

/// Narrate: `narrate ×1 (+web) · TTS ×(lines)`; lines are what `narration.json` holds.
pub fn narrate_bill(tree: Option<&layout::Tree>) -> model_calls::Summary {
    let lines = tree
        .and_then(|tree| narration::load(tree).ok())
        .map_or(0, |narration| narration.entries.len());
    model_calls::narrate(lines)
}

/// Produce: `youtube ×1 (+web) · sd.cpp ×1 when drawn · translate ×(languages × batches)`.
/// The cue count is not knowable at press time (the render cuts cues from planned clips, `produce_exec::plan`), so the bill counts the ticked languages and the batches the held cut's clips make.
pub fn produce_bill(project: &Project, tree: Option<&layout::Tree>) -> model_calls::Summary {
    let languages = project.produce.translate.len();
    let held = tree
        .and_then(|tree| cut::load(tree).ok())
        .unwrap_or_default();
    let publish = project.publish.clone().unwrap_or_default();
    model_calls::produce(
        &publish.prompt,
        publish.own || !publish.frames.is_empty(),
        held.segs.len(),
        languages,
    )
}

/// How many seams the joins pass would repair (the word list's source changes); no word list is zero, not a guess.
fn seam_count(tree: &layout::Tree) -> usize {
    word_list::load(tree)
        .ok()
        .flatten()
        .map_or(0, |words| joins::seams(&words).len())
}

/// The divisors §4 names, spelled so the bill visibly divides by other modules' constants. Test-only: pins the three numbers.
pub fn divisors_for_test() -> [(&'static str, usize); 3] {
    [
        (
            "describe frames per request",
            roles::VISION_FRAMES_PER_CALL as usize,
        ),
        ("fix lines per block", prepare::FIX_BLOCK_LINES),
        ("caption clips per batch", cut_captions::BATCH),
    ]
}

/// Which models a bill asks, as bare names in the table's order. Speech is not one (§4 counts text requests; see [`crate::model_calls::narrate`]). For tests; the program prints through [`line_for`].
pub fn models_of_for_test(summary: model_calls::Summary) -> Option<Vec<&'static str>> {
    let names: Vec<&'static str> = summary
        .calls
        .iter()
        .filter(|call| call.calls > 0 && call.model != "tts")
        .map(|call| call.model)
        .collect();
    (!names.is_empty()).then_some(names)
}
