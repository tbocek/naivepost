//! §07-narrate#5-rules: the preview's transport — what ▶/⏸/⏹ do, and what the run bar reads.
//!
//! The page publishes `playing` and `started`; this module owns the two doors that change them and the
//! one call that hands them to the run bar, so a press and the bar cannot disagree about whether a
//! preview exists. Living beside the page rather than inside it keeps the page at its size budget; like
//! `narrate_page_seek`, nothing here draws a widget.

use crate::narrate_preview::{self};
use crate::narrate_preview_leg;
use crate::narrate_screen;
use crate::ui::narrate_page::{self, NarrateState};

/// The session spans the preview may cue against. An unpublished state (empty) is read as full coverage of
/// the cut: the refusal exists for a recording that does not reach the cue, not for a flow that forgot to
/// publish one.
fn preview_covered(s: &NarrateState) -> Vec<(f64, f64)> {
    if s.covered_spans.is_empty() {
        return s.segs.iter().map(|seg| (seg.s, seg.e)).collect();
    }
    s.covered_spans.clone()
}

/// F4.5 S1: one click on the picture, or the transport's ▶ — the same door, so the two cannot drift.
/// Asks `narrate_preview::press_picture` with what this page knows: the kept clips, the line under the
/// playhead (if any) at its own start second, and the covered spans. Stores the answer in `playing` and says
/// the refusal verbatim, or names the second playback starts from.
pub fn press_preview_picture(window: &adw::ApplicationWindow) -> String {
    let s = narrate_page::read_state();
    let line_start = narrate_preview::line_at(s.session, &s.entries)
        .map(|index| s.entries[index].s + s.entries[index].at);
    let pressed =
        narrate_preview::press_picture(s.playing, &s.segs, line_start, &preview_covered(&s));
    let next_playing = pressed.playing();
    narrate_page::mutate(|state| state.playing = next_playing);
    // §5: the bar follows whatever this press decided, before any of the arms below can return early.
    publish_transport();
    match pressed {
        narrate_preview::Pressed::Refused(why) => narrate_page::say(window, &why),
        narrate_preview::Pressed::Playing { from } => {
            // F4.5 S6: a press over a HELD preview thaws the same players rather than rebuilding them — a
            // re-cue is a black frame where a still belongs, so the freeze is what ▶ walks on.
            if narrate_preview_leg::hold_paused(false) == Some(true) {
                note_started();
                return narrate_page::say(
                    window,
                    &format!(
                        "playing on from {} \u{2014} \u{23f9} stops both players",
                        narrate_screen::time_field(from)
                    ),
                );
            }
            // F4.5 S1: the decision above is only half the press — the players have to actually run.
            // A picture that will not spawn is said, not swallowed: claiming a preview with no process
            // behind it is worse than naming the program that failed.
            if let Some(tree) = crate::ui::window::narrate_session_tree() {
                let cut = crate::cut::load(&tree).unwrap_or_default();
                let driver = narrate_page::PageDriver::for_window(window);
                if let Err(why) = narrate_preview_leg::start_running(
                    &tree,
                    &cut,
                    &s.entries,
                    from,
                    narrate_preview_leg::PreviewPage::game_volume(&driver),
                    narrate_preview_leg::PreviewPage::volume(&driver),
                    &mut narrate_preview_leg::spawn_player,
                ) {
                    return narrate_page::say(window, &why);
                }
                note_started();
            }
            narrate_page::say(
                window,
                &format!(
                    "playing the cut from {} \u{2014} \u{23f9} stops both players",
                    narrate_screen::time_field(from)
                ),
            )
        }
        narrate_preview::Pressed::Paused => {
            // S6's pause is a HOLD, not a kill: `SIGSTOP` leaves each player built and sitting on its own
            // second, which is exactly what the next ▶ thaws. With nothing in the slot there is no player to
            // freeze, and whatever was running is still owed to ⏹'s door.
            if narrate_preview_leg::hold_paused(true).is_none() {
                let _ = narrate_preview_leg::stop_running();
            }
            narrate_page::say(window, "paused \u{2014} \u{25b6} plays on from here")
        }
    }
}

/// The preview's play/pause. Routes through the picture's door: inventory §A.3/§A.4 are one toggle wearing
/// two faces, and two implementations would disagree about what "play" means after an edit.
pub fn press_narrate_play(window: &adw::ApplicationWindow) -> String {
    press_preview_picture(window)
}

/// Whether the preview is running right now. Exported so the run bar can ask before reading a ⏹ press as the
/// end of a run: while a preview runs, ⏹ belongs to the preview (F4.5 S6).
pub fn narrate_preview_playing() -> bool {
    narrate_page::read_state().playing
}

/// §5: publish this page's transport as the run bar's preview. `started` is set by the doors that cue
/// players (`note_started`) and cleared only by ⏹, so a paused-but-started preview keeps ⏸/⏹ up while a
/// page nobody has played yet leaves ▶ as the step's button.
pub(crate) fn publish_transport() {
    let s = narrate_page::read_state();
    let window = crate::ui::window::main_window();
    crate::ui::window::publish_preview_transport(
        &window,
        crate::run::Transport {
            playing: s.playing,
            started: s.started,
        },
    );
    // The page's own ▶ shows the same answer as the bar: ⏸ while the preview plays (§1's "▶/⏸").
    narrate_page::paint_play_face(&window, s.playing);
    // And the bar itself: ⏹ is live while a preview stands (F4.5 S6). The first rewrite published the transport
    // without painting the bar, so ⏹ stayed grey under a playing preview.
    crate::ui::window_bench::repaint_run_bar(&window);
}

/// A press cued real players: from now on the bar belongs to this preview (§5).
fn note_started() {
    narrate_page::mutate(|state| state.started = true);
    publish_transport();
}

/// F4.5 S6: ⏹ over the preview. Stops the picture and the voice riding along with it and hands ▶ back to
/// the step; the sentence comes from `narrate_preview::hand_play_back`, so the bar cannot claim ▶ back while
/// a player is still going.
pub fn press_preview_stop(window: &adw::ApplicationWindow) -> String {
    // Read the pids BEFORE the stop: after `stop_running` the slot is empty and F0.3's drain would
    // have nothing to name. The log line is what makes the ownership visible — these are the ids ⏹
    // is responsible for killing (§F4.5 S6, §F0.3).
    let pids = narrate_preview_leg::running_children();
    let (stopped, killed) = narrate_preview_leg::stop_running();
    narrate_page::mutate(|state| {
        state.playing = false;
        // ⏹ ENDS the preview rather than pausing it: with `started` left up, `bar_serves_the_preview`
        // would keep the bar answering for a preview that no longer exists and ⏹ would stay sensitive.
        state.started = false;
        // A row's take still out on a worker is given up: it is filed when it lands, and not played.
        state.busy = false;
    });
    publish_transport();
    let said = narrate_preview::hand_play_back(stopped)
        .unwrap_or("the preview did not stop \u{2014} see log")
        .to_string();
    crate::ui::window::log_line(&format!(
        "stopped the preview players {} -> {} killed",
        pids.iter()
            .map(|pid| pid.to_string())
            .collect::<Vec<_>>()
            .join(" "),
        killed.len()
    ));
    narrate_page::say(window, &said)
}
