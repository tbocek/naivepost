//! F4.5 S4: the page's transport seams — every door that moves the playhead, in one place so none
//! of them can be a label-only move. `narrate_page` keeps the drawing and the wires.

use crate::narrate_preview;
use crate::narrate_preview_leg;
use crate::narrate_screen;
use crate::ui::narrate_page_state::{NarratePage, PAGE as NARRATE_PAGE};

/// ‹‹ back three seconds; the clock is what the button reports, §1 giving it no sentence of its
/// own.
pub fn step_clock(back: bool) {
    crate::ui::narrate_page::mutate(|s| {
        s.cut_at = if back {
            (s.cut_at - narrate_screen::BACK_SECONDS).max(0.0)
        } else {
            s.cut_at + narrate_screen::BACK_SECONDS
        };
    });
}

pub fn press_back(window: &adw::ApplicationWindow) -> String {
    let s = crate::ui::narrate_page::read_state();
    let stepped = (s.session - narrate_screen::BACK_SECONDS).max(0.0);
    step_clock(true);
    seek_preview(window, stepped, None, false)
}

pub fn press_forward(window: &adw::ApplicationWindow) -> String {
    let s = crate::ui::narrate_page::read_state();
    let stepped = s.session + narrate_screen::BACK_SECONDS;
    step_clock(false);
    seek_preview(window, stepped, None, true)
}

fn snap_reference(tree: &crate::layout::Tree, segs: Option<&[crate::cut::Seg]>) -> crate::cut::Cut {
    let mut cut = crate::cut::load(tree).unwrap_or_default();
    if let Some(segs) = segs {
        if !segs.is_empty() {
            cut.segs = segs.to_vec();
        }
    }
    cut
}

/// The scrub bar's drag, debounced (§1's slider, inventory §A.4's `debounced 120 ms`).
pub fn note_scrub_drag(
    window: &adw::ApplicationWindow,
    target: f64,
    segs: Option<&[crate::cut::Seg]>,
    forward: bool,
) {
    let segs: Vec<crate::cut::Seg> = segs.map(|s| s.to_vec()).unwrap_or_default();
    NARRATE_PAGE.with(
        |NarratePage {
             pending_scrub: pending,
             ..
         }| {
            if let Some((_, id)) = pending.borrow_mut().take() {
                id.remove();
            }
            let id = glib::timeout_add_local(
                std::time::Duration::from_millis(narrate_screen::SEEK_DEBOUNCE_MS),
                {
                    let window = window.clone();
                    move || {
                        // Clear first: the seek repaints, and that repaint must not find a drag still
                        // owed.
                        NARRATE_PAGE.with(
                            |NarratePage {
                                 pending_scrub: pending,
                                 ..
                             }| pending.borrow_mut().take(),
                        );
                        seek_preview(&window, target, Some(&segs), forward);
                        glib::ControlFlow::Break
                    }
                },
            );
            *pending.borrow_mut() = Some((target, id));
        },
    );
}

/// What the scrub bar is waiting to seek to, if a drag is mid-flight. Test-only readback: it proves
/// a drag was held by the debounce rather than acted on per pixel, which is otherwise invisible
/// from outside.
pub fn pending_scrub_for_test() -> Option<f64> {
    NARRATE_PAGE.with(
        |NarratePage {
             pending_scrub: pending,
             ..
         }| pending.borrow().as_ref().map(|(target, _)| *target),
    )
}

/// S4: a transport seek in one place — the picture is cued at the second asked for, and the page's
/// clock takes the boundary the rules snapped it to (`snap_seek`, through the leg).
pub fn seek_preview(
    window: &adw::ApplicationWindow,
    target: f64,
    segs: Option<&[crate::cut::Seg]>,
    forward: bool,
) -> String {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        // No project folder: there is nothing to cue and no cut to snap against, so the clock moves
        // and the reason is said rather than a seek claimed.
        crate::ui::narrate_page::mutate(|s| s.session = target);
        crate::ui::narrate_page::refresh(window);
        let s = crate::ui::narrate_page::read_state();
        return crate::ui::narrate_page::say(
            window,
            &format!(
                "{} \u{2014} no session to preview",
                crate::ui::narrate_page::clock_text(&s)
            ),
        );
    };
    let segs = snap_reference(&tree, segs);
    let clock = narrate_preview::snap_clock(target, &segs.segs, forward);
    crate::ui::narrate_page::mutate(|s| s.session = clock);
    // The CUE is snapped as well, to the clip's boundary rather than the edge inside it: a player
    // asked for a second outside the footage answers no frame.
    let cue = narrate_preview::snap_seek(target, &segs.segs, forward);
    if narrate_preview_leg::seek(
        &tree,
        &segs,
        cue,
        forward,
        &mut narrate_preview_leg::spawn_player,
    )
    .is_none()
    {
        // Nothing to cue (no recording reaches the cut). The clock already moved.
        crate::ui::narrate_page::refresh(window);
        return crate::ui::narrate_page::say(window, narrate_preview::NO_RECORDING_AT_START);
    }
    let s = crate::ui::narrate_page::read_state();
    crate::ui::narrate_page::refresh(window);
    crate::ui::narrate_page::say(window, &crate::ui::narrate_page::clock_text(&s))
}
