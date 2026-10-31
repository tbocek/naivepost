//! F4.2's live seam: the gather-and-drive half of what ▶ does when nothing is scripted.

use crate::cut::{self, Seg};
use crate::layout::Tree;
use crate::narrate_data;
use crate::narrate_run::Written;

/// One live narration run over this session folder: gather, build the pair, drive.
pub fn run(
    tree: &Tree,
    project: &crate::project::Project,
    bounds: &[(f64, f64)],
    progress: &mut dyn FnMut(u32, u32),
) -> Result<Vec<Written>, String> {
    let cut_ = cut::load(tree).unwrap_or_default();
    let segs: Vec<Seg> = bounds
        .iter()
        .filter_map(|(start, end)| {
            cut_.segs
                .iter()
                .find(|seg| (seg.s - start).abs() < 1e-6 && (seg.e - end).abs() < 1e-6)
                .cloned()
        })
        .collect();
    let rows = crate::textfmt::read_session(&tree.session_tsv()).unwrap_or_default();
    let voice = narrate_data::read_voice(tree);
    let narrator = crate::sources::narrator_of(project, 1).unwrap_or("");
    let paths = crate::settings::from_environment().ok_or_else(|| {
        format!(
            "{}/narrate: no settings folder to read the address from",
            crate::narrate_live::STEP
        )
    })?;
    // Both wordings are the shipped prompt plus the profile's section for the step (`prompt_text`). The first
    // rewrite passed an empty fallback here, so the narration went out with no narrate prompt and no house
    // rules at all unless this machine held an edit.
    let read_prompt = |key: &str| {
        let shipped = crate::prompts::shipped_text(key).unwrap_or_default();
        crate::settings::prompt_text(&paths, key, &shipped).unwrap_or(shipped)
    };
    let request = crate::narrate_call::request_from_tree(
        tree,
        project,
        &voice,
        &read_prompt("narrate"),
        &read_prompt("system"),
    );
    let cancelled = crate::cancel_leg::cancel_check_now();
    // §12 #22/#27: the driver is chosen by what the round ACTUALLY was. A round that answered through tools
    // named its clips, so `drive_by_clips` matches those numbers in any order and one-clip-at-a-time;
    // `narrate_call::drive`'s forward-only ±0.5 s bound match is left for a prose answer, which carries no
    // clip numbers at all. Both drivers keep the same attempt loop, thinking rule and `finish` check.
    crate::narrate_call::drive_by_clips(
        &segs,
        |attempt, thinking| {
            crate::narrate_live::ask_round(
                tree, &request, &segs, &rows, narrator, attempt, thinking, &cancelled,
            )
        },
        // §F4.2's streaming progress = written clips / clips, handed straight to the caller so the
        // run bar moves per clip instead of arriving in one jump at the end.
        progress,
        // \u00a712 #23: a refusal has to say WHICH clip's lines will not fit, so they get rewritten shorter here
        // instead of squeezed by the render later. The page's mirror IS that measurement \u{2014} the same
        // ladder the render runs (`narrate_screen::mirror_fit` over `produce_render::fit`), with speech
        // estimated the way a row estimates it, since a take written by this run does not exist yet.
        |written| unfitted_clips(&segs, written),
        crate::ui::window::log_line,
    )
}

pub fn unfitted_clips(segs: &[Seg], written: &[Written]) -> Vec<u32> {
    let mut bad: Vec<u32> = Vec::new();
    for (number, seg) in segs.iter().enumerate() {
        let mine: Vec<&Written> = written
            .iter()
            .filter(|line| (line.start - seg.s).abs() < 1e-6 && (line.end - seg.e).abs() < 1e-6)
            .collect();
        if mine.is_empty() {
            continue;
        }
        let room = crate::narrate_pass::on_screen(seg);
        let lines: Vec<(usize, f64, f64)> = mine
            .iter()
            .enumerate()
            .map(|(index, line)| {
                (
                    index,
                    line.at,
                    crate::narrate_screen::page_speech_seconds(
                        line.text.chars().count(),
                        crate::narrate_screen::SPEECH_CHARS_PER_SECOND,
                    ),
                )
            })
            .collect();
        let fit = crate::narrate_screen::mirror_fit(room, &lines);
        if matches!(fit, crate::narrate_screen::Fit::Overruns { .. }) {
            bad.push(number as u32 + 1);
        }
    }
    bad
}
