//! §07-narrate#6-details-confirmed (`Inputs tooltip`): the Narrate row's per-file breakdown.
//!
//! The shell counts what every page's Inputs row says; the Narrate row's TOOLTIP has to say more than the
//! row — which cut file the counts came from, why the narration is stale and what fixes it, how many
//! transcript lines the call will read and with what context window, the session context verbatim, and who
//! speaks. [`crate::narrate_details::inputs_tooltip`] holds that wording; this module supplies its numbers,
//! reading them off the same files the run itself reads (`cut/cut.json`, `prepare/transcript/session.tsv`,
//! `naivepost.json`) so a tooltip can never disagree with the call it describes.

use crate::narrate_details;
use crate::narrate_rewrite;
use crate::narrate_run;
use crate::ui::narrate_page::NarrateState;

/// The tooltip for the Narrate Inputs row. `tree` is the open project folder, if any: with none there are
/// no files to break down, and §6 wants the row left unexplained rather than invented — so an empty string,
/// which both callers read as "no tooltip".
pub fn tooltip(tree: Option<&crate::layout::Tree>, s: &NarrateState) -> String {
    let Some(tree) = tree else {
        return String::new();
    };
    // The cut off disk, not the state's mirror: the tooltip names `cut/cut.json`, so it must count what
    // that file holds right now (the page's `segs` only mirror what a flow last published).
    let cut = crate::cut::load(tree).unwrap_or_default();
    let clips: Vec<(f64, f64)> = cut.segs.iter().map(|seg| (seg.s, seg.e)).collect();
    let seconds = crate::cut_screen::cut_seconds(&cut);
    let narration = crate::narration::Narration {
        entries: s.entries.clone(),
        silent: silent_of(tree),
    };
    // §6's two counts, computed here rather than carried in state: a clip is unwritten when no entry sits
    // on it AND it was not deliberately silenced (a silent clip needs a caption but no line of its own is
    // not staleness the person caused), and a line is off the cut when it stands on video the cut no longer
    // has. Both come from the rules' own functions so one change moves the row, the tooltip and ▶ together.
    let unwritten = narrate_rewrite::indexes_without_line(&clips, &narration)
        .iter()
        .filter(|index| !narration.is_silent(clips[**index].0, clips[**index].1))
        .count();
    let off_cut = narrate_run::lines_off_cut(&clips, &narration);
    let stale = narrate_details::stale_mark(unwritten, off_cut);
    let transcript_lines = crate::textfmt::read_session(&tree.session_tsv())
        .unwrap_or_default()
        .len();
    // The session context verbatim, i.e. the same string the narration call sends (Describe's copy).
    let context = crate::project::load(tree.dir())
        .map(|opened| opened.context)
        .unwrap_or_default();
    // The voice row appears only for a voice that actually speaks: captions-only has no reference to clone
    // and an unset picker has nothing to name (§6: "Spoken by <voice> at ±X.X semitones").
    let voice = if s.voice.is_empty() || narrate_run::voice_is_captions_only(&s.voice) {
        None
    } else {
        Some((s.voice.as_str(), s.pitch))
    };
    narrate_details::inputs_tooltip(
        clips.len(),
        seconds,
        &stale,
        transcript_lines,
        &context,
        voice,
    )
}

/// The record's silence markers, read back from the saved narration: `§6` counts silence as the person's
/// own decision, so the tooltip asks the file that stores it rather than trusting a published count.
fn silent_of(tree: &crate::layout::Tree) -> Vec<crate::narration::Silent> {
    crate::narration::load(tree)
        .map(|saved| saved.silent)
        .unwrap_or_default()
}
