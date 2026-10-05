//! Fix's session halves: §F1.8 S1's placement row and S5–S7's merge, marking and text.
//!
//! [`crate::fix_transcripts`] holds every rule of F1.8 and spawns nothing; [`crate::fix_call`] is the
//! request leg that respells ONE source's transcript block by block. What neither did was the SESSION
//! half — `offsets.tsv` before any block goes out, the merged `session.tsv`, the marking pass the policy
//! names, and the `session.txt` the cut model reads. Those are one row at each end of the fix stage, so
//! the order the flowchart draws is the order the walk runs them in.
//!
//! Both rows are DIALS like the vision and frames rows (`program:` markers the walk calls instead of
//! forks), because their work is file arithmetic over the whole session rather than one command per row.

use crate::layout::Tree;
use crate::project::{MarkingPass, Project};
use crate::textfmt::{FrameEvent, Line, Retake};
use crate::{fix_transcripts, joins, retakes, run_stages};

/// The marker for the leading row: place every source on the session clock and write `offsets.tsv`.
pub const PLACEMENT_TASK: &str = "naivepost:fix-placement";
/// The marker for the trailing row: merge, mark, and write `session.tsv` + `session.txt`.
pub const SESSION_TASK: &str = "naivepost:fix-session";

/// The placement row, but only when the session has sources to place — an empty session has no clock to
/// place, and writing an empty `session.tsv` would claim a run happened when nothing could have.
pub fn placement_rows(project: &Project) -> Vec<run_stages::Task> {
    (!project.sources.is_empty()).then(placement_row).into_iter().collect()
}

/// The merge row, under the same gate: no sources, no timeline to merge.
pub fn merge_rows(project: &Project) -> Vec<run_stages::Task> {
    (!project.sources.is_empty()).then(merge_row).into_iter().collect()
}

/// S1's row: place every source and write `offsets.tsv`.
pub fn placement_row() -> run_stages::Task {
    run_stages::Task {
        label: "placing sources".to_string(),
        program: PLACEMENT_TASK.to_string(),
        args: Vec::new(),
        already_done: false,
        session: None,
    }
}

/// S5-S7's row: merge the timeline, run the marking pass, write `session.txt`.
pub fn merge_row() -> run_stages::Task {
    run_stages::Task {
        label: "merging the timeline".to_string(),
        program: SESSION_TASK.to_string(),
        args: Vec::new(),
        already_done: false,
        session: None,
    }
}

/// The walk's door for the placement row: S1 for the whole session.
pub fn run_placement(_task: &crate::run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let tree = session_tree()?;
    let project = crate::project::load(tree.dir()).unwrap_or_default();
    place_and_write(&tree, &project, log)?;
    Ok(())
}

/// The walk's door for the merge row: S5, S6 and S7.
pub fn run_session(_task: &crate::run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let tree = session_tree()?;
    let project = crate::project::load(tree.dir()).unwrap_or_default();
    merge_mark_write(&tree, &project, log)
}

/// Resolve the session folder the way every other dial does (§1's re-read-per-request seams).
fn session_tree() -> Result<Tree, String> {
    let session = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    Tree::new(&session).map_err(|_| format!("{}: not a project folder to fix into", session.display()))
}

/// Every source as this step sees it: its lane, its stamped name, and which kind of recording it is.
///
/// `is_video` follows `footage` and `is_commentary` follows `narrator == 1`, exactly as
/// [`crate::fix_call::stage`] spells both onto its rows, so the two halves of the stage cannot disagree
/// about whether a source owes `subtitles.srt` or `commentary.fixed.tsv`.
pub fn placed_sources(project: &Project) -> Vec<fix_transcripts::Source> {
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.path.rsplit('/').next().unwrap_or(&source.path))
        .collect();
    let starts = fix_transcripts::placement(&names);
    project
        .sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let name = names[index];
            fix_transcripts::Source {
                base: lane_of(source),
                file_name: name.to_string(),
                start: starts.get(index).copied().unwrap_or(0.0),
                duration: 0.0,
                is_video: source.footage,
                is_commentary: source.narrator == 1,
            }
        })
        .collect()
}

/// The lane a project row files its transcripts under: the stem of its file name, the same rule
/// [`crate::audio_task::source_lane`] and [`crate::fix_call`] use.
fn lane_of(source: &crate::project::Source) -> String {
    let name = source.path.rsplit('/').next().unwrap_or(&source.path);
    match name.split_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name.to_string(),
    }
}

/// S1: place every source and write `offsets.tsv`, saying each pairing out loud.
///
/// Returns the placed list so a caller that needs the seconds (the merge below) reads them from here
/// rather than computing the clock a second time.
pub fn place_and_write(
    tree: &Tree,
    project: &Project,
    log: &mut impl FnMut(&str),
) -> Result<Vec<fix_transcripts::Source>, String> {
    let placed = placed_sources(project);
    fix_transcripts::write_offsets(tree, &placed)?;
    for (video, audio, offset) in fix_transcripts::offsets_rows(&placed) {
        log(&fix_transcripts::offsets_log(&video, &audio, offset));
    }
    Ok(placed)
}

/// S5–S7: the merged timeline, the marking pass the policy names, and the text the cut reads.
pub fn merge_mark_write(
    tree: &Tree,
    project: &Project,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    // S1 again, cheaply: the merge places rows on the session clock, and a run that reached this row
    // without the placement row (a resumed run, a hand-made plan) still gets the same seconds.
    let placed = place_and_write(tree, project, &mut |_| {})?;
    let mut lines_by_source: Vec<(String, Vec<Line>)> = Vec::new();
    let mut events_by_source: Vec<(String, Vec<FrameEvent>)> = Vec::new();
    for source in &placed {
        let at = source.start;
        // The FIXED rows if the fix rows wrote them, the ASR rows if they did not: the timeline is owed
        // every word either way, and a source whose fixer refused still belongs on it.
        let mut lines = read_fixed(tree, source);
        for line in lines.iter_mut() {
            line.start += at;
            line.end += at;
        }
        let mut events = fix_transcripts::load_events(tree, &source.base);
        for event in events.iter_mut() {
            event.start += at;
            event.end += at;
        }
        if !lines.is_empty() || !events.is_empty() {
            lines_by_source.push((source.base.clone(), lines));
            events_by_source.push((source.base.clone(), events));
        }
    }

    // S5: one timeline, events labelled EVENT, sorted by start.
    let rows = fix_transcripts::merge_session(&lines_by_source, &events_by_source);
    crate::textfmt::write_session(&rows, &tree.session_tsv())?;
    let sources = lines_by_source
        .iter()
        .map(|(base, _)| base.clone())
        .chain(events_by_source.iter().map(|(base, _)| base.clone()))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    log(&fix_transcripts::timeline_log(rows.len(), sources));

    // S6: the pass P.policy.markingPass names. Its failure is a sentence, not this row's end: the
    // timeline on disk is usable, and losing the fixes to a marking refusal would be the worse error.
    let pass = crate::policy::marking_pass_of(&project.policy);
    let marks = match fix_transcripts::marks_for(pass, || run_marking_pass(tree, project, pass, log)) {
        Ok(marks) => marks,
        Err(err) => {
            log(&fix_transcripts::marking_log(&err, pass));
            Vec::new()
        }
    };

    // S7: the text the cut model reads, which is also the text the person opens.
    let narrator = narrator_of(project);
    let text = fix_transcripts::session_text(&rows, &narrator, &marks);
    fix_transcripts::write_session_text(tree, &text)?;
    log(&format!(
        ">>> session.txt: {} line(s), {} marked stretch(es)",
        text.lines().count(),
        marks.len()
    ));
    Ok(())
}

/// The fixed rows for one source, with the commentary/video choice the fix leg wrote them under.
fn read_fixed(tree: &Tree, source: &fix_transcripts::Source) -> Vec<Line> {
    let path = match source.is_commentary {
        true => tree.commentary_fixed_tsv(&source.base),
        false => tree.transcript_fixed_tsv(&source.base),
    };
    crate::textfmt::read_lines(&path).unwrap_or_else(|_| fallback_transcript(tree, source))
}

/// No fixed file yet (nothing was fixed, or this source's fix row failed): the ASR transcript stands.
fn fallback_transcript(tree: &Tree, source: &fix_transcripts::Source) -> Vec<Line> {
    fix_transcripts::load_transcript(tree, &source.base).unwrap_or_default()
}

/// The narrator's lane: the row ticked as the narrator, since §4 labels that microphone by role rather
/// than by a diariser. Empty when nobody was named — an unlabelled speaker is honest, an invented one is
/// a claim about a person no recording identified.
pub fn narrator_of(project: &Project) -> String {
    project
        .sources
        .iter()
        .find(|source| source.narrator == 1)
        .map(lane_of)
        .unwrap_or_default()
}

/// S6: run whichever marking pass the policy names, through the doors those items own, and read back the
/// stretches they wrote. A pass that produced nothing but ran is NOT an error: an empty mark list is a
/// real answer, and the timeline simply goes unmarked.
fn run_marking_pass(
    tree: &Tree,
    project: &Project,
    pass: MarkingPass,
    log: &mut impl FnMut(&str),
) -> Result<Vec<fix_transcripts::Mark>, String> {
    let lines = match pass {
        MarkingPass::Retakes => retakes::press_marking(tree, project, pass),
        MarkingPass::Joins => joins::press_joins(tree, project, pass),
        MarkingPass::None => Vec::new(),
    };
    for line in &lines {
        log(line);
    }
    // The marks live in `retakes.tsv` for both passes (joins writes its splices there too, through
    // `hand_edit::write_marks`), so one read answers either policy.
    let stored = retakes::read_marks(&tree.retakes_tsv()).unwrap_or_default();
    Ok(marks_to_fix(&stored))
}

/// A stored mark as this step folds it: `to` is where the removal ends (wider than `e` when everything up
/// to the abandoned words goes with them), and `again == 0.0` means "no later take to read instead".
pub fn marks_to_fix(stored: &[Retake]) -> Vec<fix_transcripts::Mark> {
    stored
        .iter()
        .map(|mark| fix_transcripts::Mark {
            start: mark.s,
            end: mark.to.max(mark.e),
            again: (mark.again > 0.0).then_some(mark.again),
        })
        .collect()
}
