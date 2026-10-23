//! §4's bill, asked and shown by the program — `spec/11-flow-index.md` §4.
//!
//! [`crate::model_calls`] already computes a step's bill; nothing in the app ever asked. This module is
//! that asking: it reads the live session, gathers the four numbers §4 divides by (frames filmed,
//! transcript lines, joins, clips kept — plus what the policy switches and the Produce settings say),
//! hands them to `model_calls`, and returns one line for the log, so `> Log` says what a press is about
//! to cost before the first request goes out.
//!
//! Everything that only needs numbers lives in the pure helpers at the bottom, so the counting can be
//! tested without a window; the doors above do nothing but read. A helper below that no door calls is
//! named `*_for_test`.

use crate::{
    cut, cut_captions, joins, layout, model_calls, narration, policy, prepare, project::Project,
    roles, run, word_list,
};

/// The bill for the step a ▶ press is starting, as one log line: `Prepare: describe ×2 · fix ×1 …`.
///
/// `None` when the step asks no model at all — §4 counts requests, and a line reading `Prepare: ` with
/// nothing after it would say "this step ran and asked nobody", which is a claim about the session, not
/// the empty answer §4 gives a step with nothing to ask (§4 is silent on the zero case; the decision
/// here is to print nothing rather than print a step that spent nothing).
pub fn line_for(step: run::Step) -> Option<String> {
    let summary = summary_for(step)?;
    if summary.asked() == 0 {
        return None;
    }
    Some(format!("{}: {}", summary.stage.label(), summary.one_line()))
}

/// The Setup row: `policy ×1`. §4 puts it beside the four pages even though it is not a page — it is the
/// call F0.7 makes when the User Context changes — so `derive_policy` prints this after a derivation
/// rather than the pages' bills.
pub fn setup_line() -> String {
    let summary = model_calls::setup();
    format!("{}: {}", summary.stage.label(), summary.one_line())
}

/// The bill for a step, read off the live session. Split from [`line_for`] so a test can ask for a
/// step's [`model_calls::Summary`] and read its counts rather than a joined string.
pub fn summary_for(step: run::Step) -> Option<model_calls::Summary> {
    let project = crate::ui::window::live_project();
    let tree = session_tree();
    Some(bill_for(step, &project, tree.as_ref()))
}

/// The bill for one step from a project and a session folder handed in, rather than read off the live
/// window. The doors above are one-line calls here, which is what lets a test check a bill for a state
/// it can build on disk (a cut file, a narration, a word list) without a window at all.
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

/// This session's project folder, resolved the way every other flow resolves it. `None` when no
/// session folder exists yet: nothing counted is knowable, and "0 frames" is a claim, not an absence.
fn session_tree() -> Option<layout::Tree> {
    let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    layout::Tree::new(&dir).ok()
}

/// Prepare: `describe ×(frames/4) · fix ×(lines/25) · retake ×3 or textedit ×(joins)`.
///
/// The two divisors are not repeated here: `prepare::vision_calls`/`prepare::fixer_calls` are the same
/// functions the Inputs row counts with, so a retuned `P.machine.describeFramesPerReq` or
/// `P.machine.fixBlockLines` moves the bill and the readout together. The marking pass is the table's
/// "or", read from `P.policy.markingPass`; the joins count is the seams the pass would actually repair
/// — source changes in the session word list ([`joins::seams`]), the same list F1.10 works from.
pub fn prepare_bill(project: &Project, tree: Option<&layout::Tree>) -> model_calls::Summary {
    let counts = tree.map_or_else(prepare::Counts::default, |tree| {
        prepare::count(tree, project)
    });
    let marking = policy::marking_pass_of(&project.policy);
    let joins = tree.map_or(0, seam_count);
    model_calls::prepare(counts.frames, counts.lines, marking, joins)
}

/// Cut: `cut ×1 (+web) · captions ×(clips/5) · speed ×1 · effects ×1`, each pass only when the policy
/// switches it on. The switches are the same three `policy::pass_runs` answers the run-bar buttons
/// grey from, so a bill can never promise a pass the bar refuses to run.
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

/// Narrate: `narrate ×1 (+web) · TTS ×(lines)` — the line count is what `narration.json` holds, because
/// those are the lines the voice has to speak.
pub fn narrate_bill(tree: Option<&layout::Tree>) -> model_calls::Summary {
    let lines = tree
        .and_then(|tree| narration::load(tree).ok())
        .map_or(0, |narration| narration.entries.len());
    model_calls::narrate(lines)
}

/// Produce: `youtube ×1 (+web) · sd.cpp ×1 when drawn · translate ×(languages × batches)`.
///
/// A decision §4 leaves open: the cue sheet is cut by the render from the clips it plans
/// (`produce_exec::plan`), so the cue count is not knowable when the press starts. The bill counts what
/// IS knowable at that moment — the languages ticked and how many batches the held cut's clips make —
/// and a step with no ticked language therefore shows no translate row, which is true either way.
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

/// How many seams the joins pass would repair: the word list's source changes, which is exactly what
/// `textedit ×(joins)` counts. No word list yet (nothing aligned) is zero joins, not a guess.
fn seam_count(tree: &layout::Tree) -> usize {
    word_list::load(tree)
        .ok()
        .flatten()
        .map_or(0, |words| joins::seams(&words).len())
}

/// The divisors §4 names, spelled where the bill is built so a reader can see the bill divides by the
/// OTHER modules' constants rather than by a literal written here. Not on the program's bill path —
/// `model_calls::prepare`/`cut` call their owners directly — so it exists for the test that pins the
/// three numbers against them.
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

/// Which models a bill asks, as the bare names in the table's order. §4 counts only requests that carry
/// text, so the audio.cpp speech row is not one of them (a voice call is counted by
/// [`crate::model_calls::narrate`] separately). For the tests that check a bill's model list; the
/// program prints the whole line through [`line_for`] instead.
pub fn models_of_for_test(summary: model_calls::Summary) -> Option<Vec<&'static str>> {
    let names: Vec<&'static str> = summary
        .calls
        .iter()
        .filter(|call| call.calls > 0 && call.model != "tts")
        .map(|call| call.model)
        .collect();
    (!names.is_empty()).then_some(names)
}
