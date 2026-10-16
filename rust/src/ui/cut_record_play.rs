//! F2.1's page half: what ▶ does after [`crate::preview`] has decided what the press means.
//!
//! The rule lives in `preview` (which of switch / pause / play-from-the-red-line) and in `play_back` (the
//! process that plays). This file only reads the page, hands numbers across, and paints the button — no rule
//! is decided here, per spec/00-principles.md §5.

use adw::prelude::*;

use crate::preview::{self, Press};
use crate::ui::window::log_line;
use crate::{layout, play_back, timeline, ui};
use crate::ui::cut_folds;

/// The label the toolbar ships (§1's transport table): ▶ while nothing of ours runs.
pub const PLAY_LABEL: &str = "\u{25b6} Play the recording";
/// Inventory §D's three-▶ rule: "each wears ⏸ only while its thing runs". The run bar's ▶ gets its icon from
/// `run::controls`; this button belongs to the preview, so its face changes here and nowhere else.
pub const PAUSE_LABEL: &str = "\u{23f8} Pause the recording";

/// The filmed stretches of the session this page shows — merged spans over the recordings the page holds.
/// Before this door existed the press was handed an empty list, so it could only ever refuse.
pub fn runs_now() -> Vec<(f64, f64)> {
    timeline::filmed_runs(&cut_folds::page_recordings())
}

/// The files to hand the player, in run order: each run's start second says which recording is on screen
/// there, and the session's source list names the file. A run whose file is not on disk is dropped and said
/// by name (§0: failure is specific), because playing nine of ten recordings is worth more than refusing all
/// ten — the gap is visible in the log rather than hidden behind a refusal.
///
/// DECISION (F2.1 does not say which file plays): the recording preview plays the session's own footage end
/// to end — that is what "every second plays, cuts and all" means — so the sources are filtered to the ones
/// with footage, the same rows Prepare filmed.
pub fn files_for(window: &adw::ApplicationWindow, runs: &[(f64, f64)]) -> Vec<String> {
    let root = std::env::current_dir().unwrap_or_default();
    // The session folder is the project folder, resolved the one way every flow resolves it.
    let project_dir = startup_session_dir();
    let _ = window;
    let mut files: Vec<String> = Vec::new();
    for (start, _) in runs {
        let Some(base) = base_at(&cut_folds::page_recordings(), *start) else { continue };
        let written = ui::session_sources(window)
            .into_iter()
            .find(|path| file_stem(path.rsplit('/').next().unwrap_or(path)) == base);
        let Some(written) = written else {
            log_line(&preview::playback_failure(
                "Cut",
                &format!("nothing filmed at {start:.1} s is in this session's sources"),
            ));
            continue;
        };
        let path = layout::resolve(&root, &project_dir, &written);
        if !path.is_file() {
            log_line(&preview::playback_failure(
                "Cut",
                &format!("{} is not on disk", path.display()),
            ));
            continue;
        }
        let text = path.to_string_lossy().into_owned();
        if !files.contains(&text) {
            files.push(text);
        }
    }
    files
}

/// Where the press landed, so the leg knows what to start or stop. `SwitchedToRecording` touches nothing:
/// S1 says "already playing: carry on", and the pipeline that is running is the one being carried on.
pub fn after_press(window: &adw::ApplicationWindow, pressed: &Press, runs: &[(f64, f64)]) {
    match pressed {
        Press::Playing { from } => {
            let files = files_for(window, runs);
            // ▶ takes the preview from ▶✂: the cut's own player and spare go first, or two pipelines would
            // make sound at once (F2.2 S2 hands the pipeline over, it does not share it).
            crate::cut_play_back::halt();
            // A player that will not start is logged and the transport is left alone: the line still moves
            // and every other preview rule still holds, which is the "carry on elsewhere" half of §0.
            if let Err(why) = play_back::cue(&files, *from, &|line| log_line(line)) {
                log_line(&preview::playback_failure("Cut", &why));
            }
        }
        Press::Paused => {
            play_back::halt();
            // A paused preview holds no sound at all: the lane pipelines are the recording's other voices,
            // and the tick cannot stop them itself — it returns as soon as the transport is off.
            crate::lane_hear::halt_all();
        }
        Press::SwitchedToRecording { .. } | Press::NoFootage => {}
    }
    paint_button(window);
}

/// The button's face follows the preview's state, never its own memory: ⏸ only while OUR preview runs.
///
/// Which mode is ours comes from [`crate::tab_states::preview_lits`] — §2's "one lit" decided once for all
/// three buttons — and the ⏸ itself additionally needs the transport to be actually playing: a parked
/// recording is still the recording's turn but owes a ▶, not a ⏸.
pub fn paint_button(window: &adw::ApplicationWindow) {
    let Some(button) = ui::play_recording_button(window) else { return };
    let player = ui::preview_player(window);
    let [ours_lit, _cut_lit, _review_lit] = crate::tab_states::preview_lits(&crate::cut_screen::Preview {
        cut_only: player.cut_only,
        reviewing: player.reviewing,
    });
    button.set_label(if ours_lit && player.transport.playing { PAUSE_LABEL } else { PLAY_LABEL });
}

fn startup_session_dir() -> std::path::PathBuf {
    crate::startup::session_dir(&std::env::current_dir().unwrap_or_default())
}

/// A stored path's name without its extension — the lane name Prepare files everything under, which is what
/// a `Recording`'s `base` holds (`prepare::lane`'s rule, applied here to the source spellings).
fn file_stem(name: &str) -> String {
    match name.rfind('.') {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name.to_string(),
    }
}

/// Which recording covers this second — the one the player must cue, not merely the next one.
fn base_at(recordings: &[timeline::Recording], at: f64) -> Option<String> {
    recordings
        .iter()
        .find(|rec| at >= rec.start && at < rec.end)
        .or_else(|| recordings.iter().find(|rec| rec.start > at))
        .map(|rec| rec.base.clone())
}
