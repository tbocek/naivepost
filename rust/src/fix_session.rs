//! Fix's session halves: §F1.8 S1's placement row and S5–S7's merge, marking and text.
//!
//! [`crate::fix_transcripts`] holds the F1.8 rules and spawns nothing; [`crate::fix_call`] respells ONE source's transcript. This is the SESSION half: `offsets.tsv` before any block goes out, the merged `session.tsv`, the policy's marking pass and `session.txt`.
//! Both rows are DIALS (`program:` markers the walk calls instead of forks): their work is file arithmetic over the whole session.

use crate::layout::Tree;
use crate::project::{MarkingPass, Project};
use crate::textfmt::{FrameEvent, Line, Retake};
use crate::{fix_transcripts, joins, retakes, run_stages};

/// The marker for the leading row: place every source on the session clock and write `offsets.tsv`.
pub const PLACEMENT_TASK: &str = "naivepost:fix-placement";
/// The marker for the trailing row: merge, mark, and write `session.tsv` + `session.txt`.
pub const SESSION_TASK: &str = "naivepost:fix-session";

/// The placement row, only when the session has sources: an empty `session.tsv` would claim a run happened.
pub fn placement_rows(project: &Project) -> Vec<run_stages::Task> {
    (!project.sources.is_empty())
        .then(placement_row)
        .into_iter()
        .collect()
}

/// The merge row, under the same gate: no sources, no timeline to merge.
pub fn merge_rows(project: &Project) -> Vec<run_stages::Task> {
    (!project.sources.is_empty())
        .then(merge_row)
        .into_iter()
        .collect()
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
pub fn run_placement(
    _task: &crate::run_stages::Task,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let tree = session_tree()?;
    let project = crate::project::load(tree.dir()).unwrap_or_default();
    place_and_write(&tree, &project, log)?;
    Ok(())
}

/// The walk's door for the merge row: S5, S6 and S7.
///
/// The session fix step (§F1.8 S1-S4 over every recording, [`session_fix`] via [`crate::fix_transcripts::run`]) runs FIRST, and only if some source has no answers on disk: blocks read back from `cache/llm/fix` cost no ask (§S3's resume rule).
/// A session with no settings folder skips the step and merges whatever transcripts are on disk.
pub fn run_session(
    _task: &crate::run_stages::Task,
    log: &mut impl FnMut(&str),
    reached: &mut dyn FnMut(f64, &str),
) -> Result<(), String> {
    let tree = session_tree()?;
    let project = crate::project::load(tree.dir()).unwrap_or_default();
    // §S3's resume rule: a source whose FIXED file is on disk was walked by this run's per-source rows. Any other (resumed run, failed or skipped fix row) still owes blocks, and gets the whole-session walk over EVERY recording.
    let placed = placed_sources(&project);
    if placed
        .iter()
        .any(|source| fix_transcripts::owes_the_fixer(&tree, source))
    {
        session_fix(&tree, &project, log, reached)?;
    }
    merge_mark_write(&tree, &project, log, reached)
}

/// §F1.8 S1–S4 for the whole session in one walk: place, block by block over EVERY recording, write the fixed files. `ask` is [`crate::fix_call::ask_block`], the clean-transcript job's door (§3.1 reads stay answerable, §09 §5's bound is applied by `llm_request::body`).
pub fn session_fix(
    tree: &Tree,
    project: &Project,
    log: &mut impl FnMut(&str),
    reached: &mut dyn FnMut(f64, &str),
) -> Result<(), String> {
    let Some(paths) = crate::settings::from_environment() else {
        // No settings folder means nothing to dial; the merge still owes a timeline, so a silent skip.
        return Ok(());
    };
    let conf = crate::settings::read(&paths).unwrap_or_default();
    let pass = crate::policy::marking_pass_of(&project.policy);
    let outcome = fix_transcripts::run(
        tree,
        &placed_sources(project),
        &narrator_of(project),
        pass,
        &crate::fix_call::cache_prompt(&paths, &project.context),
        |message| crate::fix_call::ask_block(tree, &paths, &conf, message),
        || run_marking_pass(tree, project, pass, log, reached),
    )?;
    for line in &outcome.logs {
        log(line);
    }
    Ok(())
}

/// Resolve the session folder the way every other dial does (§1's re-read-per-request seams).
fn session_tree() -> Result<Tree, String> {
    let session = crate::startup::open_dir();
    Tree::new(&session)
        .map_err(|_| format!("{}: not a project folder to fix into", session.display()))
}

/// The project's rows in session-clock order: by the start each file name's timestamp places it at ([`placed_sources`]); stable, so unstamped rows keep their added order.
pub fn in_clock_order(project: &Project) -> Vec<&crate::project::Source> {
    let placed = placed_sources(project);
    let mut order: Vec<usize> = (0..project.sources.len()).collect();
    order.sort_by(|a, b| {
        let at = |index: usize| placed.get(index).map_or(0.0, |source| source.start);
        at(*a)
            .partial_cmp(&at(*b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    order
        .into_iter()
        .map(|index| &project.sources[index])
        .collect()
}

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
                is_commentary: crate::sources::is_narrator_mic(source) && source.narrator == 1,
            }
        })
        .collect()
}

/// The lane a project row files its transcripts under: the stem of its file name (as [`crate::audio_task::source_lane`] and [`crate::fix_call`]).
fn lane_of(source: &crate::project::Source) -> String {
    let name = source.path.rsplit('/').next().unwrap_or(&source.path);
    match name.split_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name.to_string(),
    }
}

/// S1: place every source and write `offsets.tsv`, saying each pairing out loud.
///
/// Returns the placed list so the merge reads the seconds from here, not a second clock computation.
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
    reached: &mut dyn FnMut(f64, &str),
) -> Result<(), String> {
    // S1 again, cheaply: a run that reached this row without the placement row still gets the same seconds.
    let placed = place_and_write(tree, project, &mut |_| {})?;
    let mut lines_by_source: Vec<(String, Vec<Line>)> = Vec::new();
    let mut events_by_source: Vec<(String, Vec<FrameEvent>)> = Vec::new();
    for source in &placed {
        let at = source.start;
        // The FIXED rows if the fix rows wrote them, else the ASR rows: a source whose fixer refused still belongs on the timeline.
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

    // S6: the pass P.policy.markingPass names. Its failure is a sentence, not this row's end: losing the fixes to a marking refusal would be worse.
    let pass = crate::policy::marking_pass_of(&project.policy);
    let marks = match fix_transcripts::marks_for(pass, || {
        run_marking_pass(tree, project, pass, log, reached)
    }) {
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

/// The narrator's lane: the row ticked as the narrator (§4 labels that microphone by role, not by diariser). Empty when nobody was named: no invented speaker.
pub fn narrator_of(project: &Project) -> String {
    project
        .sources
        .iter()
        .find(|source| crate::sources::is_narrator_mic(source) && source.narrator == 1)
        .map(lane_of)
        .unwrap_or_default()
}

/// S6: run whichever marking pass the policy names and read back the stretches they wrote. A pass that ran and produced nothing is not an error.
pub fn run_marking_pass(
    tree: &Tree,
    project: &Project,
    pass: MarkingPass,
    log: &mut impl FnMut(&str),
    reached: &mut dyn FnMut(f64, &str),
) -> Result<Vec<fix_transcripts::Mark>, String> {
    let lines = match pass {
        MarkingPass::Retakes => {
            reached(0.0, "marking retakes");
            retakes::press_marking(tree, project, pass)
        }
        MarkingPass::Joins => joins::press_joins(tree, project, pass, reached),
        MarkingPass::None => Vec::new(),
    };
    for line in &lines {
        log(line);
    }
    let refused = lines.iter().any(|line| line.starts_with("!!!"));
    let stored = if refused {
        Vec::new()
    } else {
        retakes::read_marks(&tree.retakes_tsv()).unwrap_or_default()
    };
    Ok(marks_to_fix(&stored))
}

/// A stored mark as this step folds it: `to` is where the removal ends (wider than `e` when the abandoned words go with everything up to them), `again == 0.0` means no later take.
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
