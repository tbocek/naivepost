//! F2.1's page half: what ▶ does after [`crate::preview`] has decided what the press means.
//!
//! The rule lives in `preview` (which of switch / pause / play-from-the-red-line) and in `play_back` (the
//! process that plays). This file only reads the page, hands numbers across, and paints the button — no rule
//! is decided here, per spec/00-principles.md §5.

use adw::prelude::*;

use crate::preview::{self, Press};
use crate::ui::cut_folds;
use crate::ui::window::log_line;
use crate::{layout, play_back, timeline, ui};

/// The label the toolbar ships (§1's transport table): ▶ while nothing of ours runs.
pub const PLAY_LABEL: &str = crate::cut_screen::PLAY_GLYPH_RECORDING;
/// Inventory §D's three-▶ rule: "each wears ⏸ only while its thing runs". The run bar's ▶ gets its icon from
/// `run::controls`; this button belongs to the preview, so its face changes here and nowhere else.
pub const PAUSE_LABEL: &str = "\u{23f8}";

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
        let Some(base) = base_at(&cut_folds::page_recordings(), *start) else {
            continue;
        };
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

/// The file the session's sources name for recording `base` (its stem), resolved and on disk.
fn source_file(window: &adw::ApplicationWindow, base: &str) -> Option<String> {
    let _ = window;
    recording_file(base).map(|path| path.to_string_lossy().into_owned())
}

/// The file on disk a recording (by its base, the file's stem) is played and pictured from: the open project's
/// source of that name, resolved by §2's one path rule. `None` when no source has that name or its file is gone.
/// Shared by the player and the band's film strip (`cut_film_cache`), which has no window in its draw.
pub(crate) fn recording_file(base: &str) -> Option<std::path::PathBuf> {
    let root = std::env::current_dir().unwrap_or_default();
    let written = crate::ui::window::SESSION.with(|slots| {
        slots.borrow().last().and_then(|project| {
            project
                .borrow()
                .sources
                .iter()
                .map(|source| source.path.clone())
                .find(|path| file_stem(path.rsplit('/').next().unwrap_or(path)) == base)
        })
    })?;
    let path = layout::resolve(&root, &startup_session_dir(), &written);
    path.is_file().then_some(path)
}

/// How near the line must be to the player's own second to be the player's move rather than a person's: two ticks.
const SAME_SECOND_S: f64 = 0.25;

thread_local! {
    /// The recording the built-in player holds: its session span, so the player's position reads as a session
    /// second and its end as the recording's end.
    static IN_VIDEO: std::cell::Cell<Option<(f64, f64)>> = const { std::cell::Cell::new(None) };
}

/// F2.1 through the built-in player: the recording the picture shows at session second `at`, from that second of
/// its file, playing or held on the frame, on the preview's own picture. False (and said in the log) when there is
/// nothing on disk to play there or the pipeline will not start.
pub fn play_in_preview(window: &adw::ApplicationWindow, at: f64, play: bool) -> bool {
    let Some((recording, local)) = crate::ui::cut_picture::picture_source(window, at) else {
        log_line(&preview::playback_failure(
            "Cut",
            &format!("nothing filmed at {at:.1} s"),
        ));
        return false;
    };
    let Some(file) = source_file(window, &recording.base) else {
        log_line(&preview::playback_failure(
            "Cut",
            &format!("{} is not on disk", recording.base),
        ));
        return false;
    };
    match crate::play_built_in::cue(&file, local, play, &|line| log_line(line)) {
        Ok(()) => {
            IN_VIDEO.with(|cell| cell.set(Some((at - local, recording.end))));
            crate::ui::cut_picture::show_video(window);
            true
        }
        Err(why) => {
            log_line(&preview::playback_failure("Cut", &why));
            false
        }
    }
}

/// The line moved to `at` while the picture is the built-in player's: seek it there (the same recording) or hand
/// it the recording that holds `at`, playing or held as it was. False when it cannot follow, and the caller gives
/// the picture back to the extracted frames.
pub fn follow_in_video(window: &adw::ApplicationWindow, at: f64) -> bool {
    let Some((recording, _)) = crate::ui::cut_picture::picture_source(window, at) else {
        return false;
    };
    let Some(file) = source_file(window, &recording.base) else {
        return false;
    };
    let same = crate::play_built_in::file().as_deref() == Some(file.as_str());
    // The tick moved the line to where the player already is: nothing to seek. A person's move lands elsewhere,
    // and the player goes there, playing or held as it was.
    let there = video_second().is_some_and(|stands| (stands - at).abs() < SAME_SECOND_S);
    if same && there {
        return true;
    }
    play_in_preview(window, at, crate::play_built_in::playing())
}

/// Where the built-in player stands, as a session second: what ▶'s line follows (F2.1 S4). At the end of its file it
/// answers the recording's end, so the line walks on from there.
pub fn video_second() -> Option<f64> {
    let (start, end) = IN_VIDEO.with(|cell| cell.get())?;
    if crate::play_built_in::ended() {
        return Some(end);
    }
    crate::play_built_in::position().map(|local| start + local)
}

/// [`video_second`] while the built-in player is actually playing in the preview, `None` otherwise: what every
/// preview tick starts its line from, so the line cannot drift from the picture.
pub fn playing_second() -> Option<f64> {
    if crate::ui::cut_picture::video_shown() && crate::play_built_in::playing() {
        video_second()
    } else {
        None
    }
}

/// Where the press landed, so the leg knows what to start or stop. `SwitchedToRecording` touches nothing:
/// S1 says "already playing: carry on", and the pipeline that is running is the one being carried on.
pub fn after_press(window: &adw::ApplicationWindow, pressed: &Press, runs: &[(f64, f64)]) {
    match pressed {
        Press::Playing { from } if crate::play_built_in::in_use() => {
            // ▶ takes the preview from ▶✂: the cut's own player and spare go first, or two pipelines would
            // make sound at once (F2.2 S2 hands the pipeline over, it does not share it).
            crate::cut_play_back::halt();
            play_in_preview(window, *from, true);
        }
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
    paint_transport(window);
}

/// All three faces (▶, ▶✂, ▶✂✂) and which one is lit, from the preview's state. A press on one changes the others'
/// modes too (▶✂ ends a review, ▶ takes the preview from the cut), so every press, ⏹, frame step and tick that
/// changes the preview paints all three: the first rewrite repainted only the pressed button, and ▶✂✂ kept its
/// ⏸ after ▶✂ had ended the review.
pub fn paint_transport(window: &adw::ApplicationWindow) {
    paint_button(window);
    crate::ui::cut_cut_play::paint(window);
    crate::ui::cut_review_play::paint(window);
    crate::ui::window_bench::repaint_run_bar(window);
}

/// The button's face follows the preview's state, never its own memory: ⏸ only while OUR preview runs.
///
/// Which mode is ours comes from [`crate::tab_states::preview_lits`] — §2's "one lit" decided once for all
/// three buttons — and the ⏸ itself additionally needs the transport to be actually playing: a parked
/// recording is still the recording's turn but owes a ▶, not a ⏸.
pub fn paint_button(window: &adw::ApplicationWindow) {
    let Some(button) = ui::play_recording_button(window) else {
        return;
    };
    let player = ui::preview_player(window);
    let [ours_lit, _cut_lit, _review_lit] =
        crate::tab_states::preview_lits(&crate::cut_screen::Preview {
            cut_only: player.cut_only,
            reviewing: player.reviewing,
        });
    set_steady_label(
        &button,
        if ours_lit && player.transport.playing {
            PAUSE_LABEL
        } else {
            PLAY_LABEL
        },
        &[PLAY_LABEL, PAUSE_LABEL],
    );
    light_lit(window);
}

/// Show `text` on a transport button whose face swaps (▶ and ⏸): its label is sized for the largest of `all`, so
/// the swap never resizes the button. ⏸ comes from another font than ▶ and stands a pixel taller, so every press
/// grew or shrank the button by a pixel. `set_label` keeps the same label widget, which carries the size.
pub fn set_steady_label(button: &gtk4::Button, text: &str, all: &[&str]) {
    button.set_label(text);
    let Some(label) = button
        .child()
        .and_then(|child| child.downcast::<gtk4::Label>().ok())
    else {
        return;
    };
    let (wide, high) = all.iter().fold((0, 0), |(wide, high), face| {
        let (w, h) = label.create_pango_layout(Some(face)).pixel_size();
        (wide.max(w), high.max(h))
    });
    label.set_size_request(wide, high);
}

/// Inventory §D's "exactly one lit": the preview mode in use wears the accent (`suggested-action`, the
/// prototype's `lamp`), the other two do not. All three are restated from [`crate::tab_states::preview_lits`]
/// whichever button was painted, because a press on one changes which of the others is lit.
pub fn light_lit(window: &adw::ApplicationWindow) {
    let player = ui::preview_player(window);
    let lits = crate::tab_states::preview_lits(&crate::cut_screen::Preview {
        cut_only: player.cut_only,
        reviewing: player.reviewing,
    });
    let buttons = [
        ui::play_recording_button(window),
        ui::play_cut_button(window),
        ui::review_cuts_button(window),
    ];
    for (button, lit) in buttons.into_iter().zip(lits) {
        if let Some(button) = button {
            if lit {
                button.add_css_class("suggested-action");
            } else {
                button.remove_css_class("suggested-action");
            }
        }
    }
}

fn startup_session_dir() -> std::path::PathBuf {
    crate::startup::open_dir()
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
