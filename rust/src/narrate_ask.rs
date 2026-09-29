//! F4.2's live seam: the gather-and-drive half of what ▶ does when nothing is scripted.
//!
//! `ui::press_narrate_run`'s no-script branch lands here, and this module holds it so the shell file
//! stays a shell (§Layout). The rules stay where they belong: [`crate::narrate_call::drive`] owns the
//! attempts, the thinking-off rule and the bar's monotonic progress; [`crate::narrate_live`] owns the
//! wire and §3.8's tools. What lives here is only the gathering — the cut's SEGMENTS (whose rate is
//! what "X s of footage at Rx" and the fit answer are made of), the transcript off disk, the voice,
//! the two prompt wordings — plus ⏹ riding along as `cancel_leg::cancel_check_now()`.

use crate::cut::{self, Seg};
use crate::layout::Tree;
use crate::narrate_data;
use crate::narrate_run::Written;

/// One live narration run over this session folder: gather, build the pair, drive.
///
/// `bounds` are the clip seconds the run was planned over; the segments are looked back out of the
/// newest `cut.json` so each keeps its rate. A bound with no matching segment is dropped rather than
/// guessed at — a clip the cut no longer holds cannot be narrated.
pub fn run(
    tree: &Tree,
    project: &crate::project::Project,
    bounds: &[(f64, f64)],
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
        format!("{}/narrate: no settings folder to read the address from", crate::narrate_live::STEP)
    })?;
    // Both wordings come from `prompt_text` with an empty fallback: that is what the Settings box shows
    // when this machine holds no edit, and a missing file is not a reason to refuse the call.
    let read_prompt = |key: &str| crate::settings::prompt_text(&paths, key, "").unwrap_or_default();
    let request = crate::narrate_call::request_from_tree(
        tree,
        project,
        &voice,
        &read_prompt("narrate"),
        &read_prompt("system"),
    );
    let cancelled = crate::cancel_leg::cancel_check_now();
    crate::narrate_call::drive(
        &segs,
        |attempt, thinking| {
            crate::narrate_live::ask_round(
                tree, &request, &segs, &rows, narrator, attempt, thinking, &cancelled,
            )
            .map(|round| round.answer)
        },
        |_written, _total| {},
        // No render runs headless, so nothing measures a line against its clip here: `finish` answers
        // on completeness alone and an empty unfitted list is the honest answer rather than a guess.
        |_written| Vec::new(),
        log,
    )
}

/// Where a rejected attempt is said. The window owns the visible log; this module has no widget, so the
/// line goes through the same sink every other headless flow uses when one is installed, and nowhere
/// otherwise (`log_line` is a no-op with no window).
fn log(line: &str) {
    crate::ui::window::log_line(line);
}
