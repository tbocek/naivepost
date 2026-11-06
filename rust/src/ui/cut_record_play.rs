//! F2.1's page half: what ▶ does after [`crate::preview`] has decided what the press means.

use adw::prelude::*;

use crate::narrate_preview_media::file_stem;
use crate::preview::{self, Press};
use crate::ui::cut_folds;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::ui::window::log_line;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};
use crate::{layout, play_back, timeline, ui};

/// The label the toolbar ships (§1's transport table): ▶ while nothing of ours runs.
pub const PLAY_LABEL: &str = crate::cut_screen::PLAY_GLYPH_RECORDING;
/// Inventory §D's three-▶ rule: "each wears ⏸ only while its thing runs".
pub const PAUSE_LABEL: &str = "\u{23f8}";

pub fn runs_now() -> Vec<(f64, f64)> {
    timeline::filmed_runs(&cut_folds::page_recordings())
}

/// The files to hand the player, in run order: each run's start second says which recording is on
/// screen there, and the session's source list names the file.
pub fn files_for(runs: &[(f64, f64)]) -> Vec<String> {
    let root = std::env::current_dir().unwrap_or_default();
    let project_dir = crate::startup::open_dir();
    let mut files: Vec<String> = Vec::new();
    for (start, _) in runs {
        let Some(base) = base_at(&cut_folds::page_recordings(), *start) else {
            continue;
        };
        let written = ui::session_sources()
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

fn source_file(base: &str) -> Option<String> {
    recording_file(base).map(|path| path.to_string_lossy().into_owned())
}

/// The file on disk a recording (by its base, the file's stem) is played and pictured from: the
/// open project's source of that name, resolved by §2's one path rule.
pub(crate) fn recording_file(base: &str) -> Option<std::path::PathBuf> {
    let root = std::env::current_dir().unwrap_or_default();
    let written = WINDOW_PAGE.with(|WindowPage { session: slots, .. }| {
        slots.borrow().last().and_then(|project| {
            project
                .borrow()
                .sources
                .iter()
                .map(|source| source.path.clone())
                .find(|path| file_stem(path.rsplit('/').next().unwrap_or(path)) == base)
        })
    })?;
    let path = layout::resolve(&root, &crate::startup::open_dir(), &written);
    path.is_file().then_some(path)
}

/// How near the line must be to the player's own second to be the player's move rather than a
/// person's: two ticks.
const SAME_SECOND_S: f64 = 0.25;

/// F2.1 through the built-in player: the recording the picture shows at session second `at`, from
/// that second of its file, playing or held on the frame, on the preview's own picture.
pub fn play_in_preview(window: &adw::ApplicationWindow, at: f64, play: bool) -> bool {
    let Some((recording, local)) = crate::ui::cut_picture::picture_source(at) else {
        log_line(&preview::playback_failure(
            "Cut",
            &format!("nothing filmed at {at:.1} s"),
        ));
        return false;
    };
    let Some(file) = source_file(&recording.base) else {
        log_line(&preview::playback_failure(
            "Cut",
            &format!("{} is not on disk", recording.base),
        ));
        return false;
    };
    // The file starts at `at - local` on the session clock; the wanted silence goes on its clock before it decodes.
    // Only ▶✂ silences past a stretch's end; ▶ plays everything, cuts and all.
    let cutting = crate::ui::preview_player().cut_only;
    let silence = WANTED_SILENCE
        .with(|cell| cell.get())
        .filter(|_| cutting)
        .map(|t| t - (at - local));
    // F2.2 S5: the jump the spare was prerolled for — the pipelines swap, nothing reloads, and the picture never shows
    // the next file's first frame on the way to the seeked one.
    if play && crate::play_built_in::swap_in(&file, local, silence) {
        CUT_PAGE.with(|CutPage { in_video: cell, .. }| cell.set(Some((at - local, recording.end))));
        crate::ui::cut_picture::show_video(window);
        SWAPPED.with(|cell| cell.set(true));
        return true;
    }
    match crate::play_built_in::cue_silenced(&file, local, play, silence) {
        Ok(()) => {
            CUT_PAGE
                .with(|CutPage { in_video: cell, .. }| cell.set(Some((at - local, recording.end))));
            crate::ui::cut_picture::show_video(window);
            true
        }
        Err(why) => {
            log_line(&preview::playback_failure("Cut", &why));
            false
        }
    }
}

thread_local! {
    /// The session second ▶✂ wants the sound silenced from: what the next cue installs before it decodes.
    static WANTED_SILENCE: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
    /// A jump was crossed by swapping the spare in since the tick last asked ([`take_swapped`]).
    static SWAPPED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// F2.2 S5 through the built-in player: preroll the spare at session second `at` — the recording the picture will
/// show there, at that second of its file, silenced from the end of the kept stretch it starts. Answers whether a
/// pipeline is (or already was) prerolling there.
pub fn preroll_in_preview(at: f64) -> bool {
    let Some((recording, local)) = crate::ui::cut_picture::picture_source(at) else {
        log_line(&format!(
            "    built-in player: no spare for the join at {}: nothing filmed there",
            crate::tools::mm_ss(at)
        ));
        return false;
    };
    let Some(file) = source_file(&recording.base) else {
        log_line(&format!(
            "    built-in player: no spare for the join at {}: {} is not on disk",
            crate::tools::mm_ss(at),
            recording.base
        ));
        return false;
    };
    let cut = crate::ui::window::newest_review_cut();
    let silence = crate::cut_play_leg::stretch_end(&cut, at).map(|end| end - (at - local));
    match crate::play_built_in::preroll(&file, local, silence) {
        Ok(()) => true,
        Err(why) => {
            log_line(&format!(
                "    built-in player: no spare for the join at {}: {why}",
                crate::tools::mm_ss(at)
            ));
            false
        }
    }
}

/// Whether a swap crossed a join since this was last asked; the tick reads it once per step.
pub fn take_swapped() -> bool {
    SWAPPED.with(|cell| cell.replace(false))
}

/// Silence the built-in player's sound from this session second on (F2.2 S5). It is what every later cue installs
/// before the pipeline decodes ([`play_in_preview`]); `now` also applies it to the file playing, on its own clock — not
/// at a jump, where the stretch being left must stay silenced until the seek has flushed it.
pub fn silence_after(session: Option<f64>, now: bool) {
    WANTED_SILENCE.with(|cell| cell.set(session));
    if now {
        let start = CUT_PAGE
            .with(|CutPage { in_video: cell, .. }| cell.get())
            .map(|(start, _)| start);
        crate::play_built_in::silence_from(session.zip(start).map(|(t, start)| t - start));
    }
}

pub fn follow_in_video(window: &adw::ApplicationWindow, at: f64) -> bool {
    let Some((recording, _)) = crate::ui::cut_picture::picture_source(at) else {
        return false;
    };
    let Some(file) = source_file(&recording.base) else {
        return false;
    };
    let same = crate::play_built_in::file().as_deref() == Some(file.as_str());
    let there = video_second().is_some_and(|stands| (stands - at).abs() < SAME_SECOND_S);
    if same && there {
        return true;
    }
    play_in_preview(window, at, crate::play_built_in::playing())
}

/// Where the built-in player stands, as a session second: what ▶'s line follows (F2.1 S4).
pub fn video_second() -> Option<f64> {
    let (start, end) = CUT_PAGE.with(|CutPage { in_video: cell, .. }| cell.get())?;
    if crate::play_built_in::ended() {
        return Some(end);
    }
    crate::play_built_in::position().map(|local| start + local)
}

/// [`video_second`] while the built-in player is actually playing in the preview, `None` otherwise:
/// what every preview tick starts its line from, so the line cannot drift from the picture.
pub fn playing_second() -> Option<f64> {
    if crate::ui::cut_picture::video_shown() && crate::play_built_in::playing() {
        video_second()
    } else {
        None
    }
}

pub fn after_press(window: &adw::ApplicationWindow, pressed: &Press, runs: &[(f64, f64)]) {
    let pressed = &match pressed {
        Press::SwitchedToRecording {
            started: Some(from),
            ..
        } => Press::Playing { from: *from },
        other => *other,
    };
    match pressed {
        Press::Playing { from } if crate::play_built_in::in_use() => {
            // ▶ takes the preview from ▶✂: the cut's own player and spare go first, or two
            // pipelines would make sound at once (F2.2 S2 hands the pipeline over, it does not
            // share it).
            crate::cut_play_back::halt();
            play_in_preview(window, *from, true);
        }
        Press::Playing { from } => {
            let files = files_for(runs);
            // ▶ takes the preview from ▶✂: the cut's own player and spare go first, or two
            // pipelines would make sound at once (F2.2 S2 hands the pipeline over, it does not
            // share it).
            crate::cut_play_back::halt();
            // A player that will not start is logged and the transport is left alone: the line
            // still moves and every other preview rule still holds, which is the "carry on
            // elsewhere" half of §0.
            if let Err(why) = play_back::cue(&files, *from, &|line| log_line(line)) {
                log_line(&preview::playback_failure("Cut", &why));
            }
        }
        Press::Paused => {
            play_back::halt();
            crate::lane_hear::halt_all();
        }
        Press::SwitchedToRecording { .. } | Press::NoFootage => {}
    }
    paint_transport(window);
}

pub fn paint_transport(window: &adw::ApplicationWindow) {
    paint_button(window);
    crate::ui::cut_cut_play::paint(window);
    crate::ui::cut_review_play::paint(window);
    crate::ui::window_bench::repaint_run_bar(window);
}

/// The button's face follows the preview's state, never its own memory: ⏸ only while OUR preview
/// runs.
pub fn paint_button(window: &adw::ApplicationWindow) {
    let Some(button) = ui::play_recording_button(window) else {
        return;
    };
    let player = ui::preview_player();
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

/// Show `text` on a transport button whose face swaps (▶ and ⏸): its label is sized for the largest
/// of `all`, so the swap never resizes the button. ⏸ comes from another font than ▶ and stands a
/// pixel taller, so every press grew or shrank the button by a pixel.
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

/// All three are restated from [`crate::tab_states::preview_lits`] whichever button was painted,
/// because a press on one changes which of the others is lit.
pub fn light_lit(window: &adw::ApplicationWindow) {
    let player = ui::preview_player();
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

/// Which recording covers this second — the one the player must cue, not merely the next one.
fn base_at(recordings: &[timeline::Recording], at: f64) -> Option<String> {
    recordings
        .iter()
        .find(|rec| at >= rec.start && at < rec.end)
        .or_else(|| recordings.iter().find(|rec| rec.start > at))
        .map(|rec| rec.base.clone())
}
