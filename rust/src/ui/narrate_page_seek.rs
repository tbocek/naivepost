//! F4.5 S4: the page's transport seams — every door that moves the playhead, in one place so none of them can
//! be a label-only move. `narrate_page` keeps the drawing and the wires; this holds what a seek does.
//!
//! A seek is two snapped seconds, not one: the CLOCK takes `narrate_preview::snap_clock` (inside the clip, by
//! §C.4's `cutEdge`) and the PLAYER is cued at `narrate_preview::snap_seek` (the boundary itself). One number
//! cannot serve both: a cue outside the footage answers no frame, and a reading on the boundary names a second
//! whose frame does not exist.

use crate::narrate_preview;
use crate::narrate_preview_leg;
use crate::narrate_screen;

/// ‹‹ back three seconds; the clock is what the button reports, §1 giving it no sentence of its own.
/// S4: the clock's second half — the position inside the finished video — walks by `BACK_SECONDS`, clamped
/// at zero, while `session` is left to [`seek_preview`]: the playhead lands on the cut, this half just counts.
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

/// The page's clips, for a snap against what this page is showing. The page's own list is the reference for
/// every transport seek — the slider, the wheel and the two buttons all step over the clips on screen; the
/// session's `cut.json` (which another flow may have written with other seconds) is the fallback for a seek
/// that arrives with no page state, and `snap_seek` leaves a second it cannot bracket alone.
fn snap_reference(tree: &crate::layout::Tree, segs: Option<&[crate::cut::Seg]>) -> crate::cut::Cut {
    let mut cut = crate::cut::load(tree).unwrap_or_default();
    if let Some(segs) = segs {
        if !segs.is_empty() {
            cut.segs = segs.to_vec();
        }
    }
    cut
}

thread_local! {
    /// The second the scrub bar was last dragged to, waiting out [`narrate_screen::SEEK_DEBOUNCE_MS`], with the
    /// armed timer that will issue it. One pending drag at a time: each new move replaces the target and re-arms
    /// the timer, so a drag across fifty pixels produces the seek the finger ended on, not fifty of them.
    static PENDING_SCRUB: std::cell::RefCell<Option<(f64, glib::SourceId)>> =
        const { std::cell::RefCell::new(None) };
}

/// The scrub bar's drag, debounced (§1's slider, inventory §A.4's `debounced 120 ms`). Every `value-changed` a
/// drag produces comes through here; only the LAST one is issued, after the bar has been quiet for
/// [`narrate_screen::SEEK_DEBOUNCE_MS`]. The seek itself is the unchanged [`seek_preview`], so a dragged-to
/// second lands on the cut exactly as a button's does. Only the DRAG waits: the transport buttons and the
/// paused wheel call [`seek_preview`] directly, because each of those is one deliberate press, not a burst.
pub fn note_scrub_drag(
    window: &adw::ApplicationWindow,
    target: f64,
    segs: Option<&[crate::cut::Seg]>,
    forward: bool,
) {
    let segs: Vec<crate::cut::Seg> = segs.map(|s| s.to_vec()).unwrap_or_default();
    PENDING_SCRUB.with(|pending| {
        if let Some((_, id)) = pending.borrow_mut().take() {
            id.remove();
        }
        let id = glib::timeout_add_local(
            std::time::Duration::from_millis(narrate_screen::SEEK_DEBOUNCE_MS),
            {
                let window = window.clone();
                move || {
                    // Clear first: the seek repaints, and that repaint must not find a drag still owed.
                    PENDING_SCRUB.with(|pending| pending.borrow_mut().take());
                    seek_preview(&window, target, Some(&segs), forward);
                    glib::ControlFlow::Break
                }
            },
        );
        *pending.borrow_mut() = Some((target, id));
    });
}

/// What the scrub bar is waiting to seek to, if a drag is mid-flight. Test-only readback: it proves a drag was
/// held by the debounce rather than acted on per pixel, which is otherwise invisible from outside.
pub fn pending_scrub_for_test() -> Option<f64> {
    PENDING_SCRUB.with(|pending| pending.borrow().as_ref().map(|(target, _)| *target))
}

/// S4: a transport seek in one place — the picture is cued at the second asked for, and the page's clock
/// takes the boundary the rules snapped it to (`snap_seek`, through the leg). Every door that moves the
/// playhead (back, forward, the slider, the paused wheel) comes through here so none of them can be a
/// label-only move. With no cut to snap against the clock simply moves.
///
/// The page hands its own clips in as the snap's reference (the same list the transport, the slider and the
/// wheel step over); `None` means "whatever this session's `cut.json` says", which is what a flow that has no
/// page state of its own gets.
pub fn seek_preview(
    window: &adw::ApplicationWindow,
    target: f64,
    segs: Option<&[crate::cut::Seg]>,
    forward: bool,
) -> String {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        // No project folder: there is nothing to cue and no cut to snap against, so the clock moves and the
        // reason is said rather than a seek claimed.
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
    // The snap is the rules' and it comes FIRST — that is the spec's own order (S4: a gap target answers the
    // previous clip's end / the next clip's start), and it is what the clock takes. Only then is the picture
    // asked for a cue, and when there is no footage to cue the clock has already landed on the boundary.
    let clock = narrate_preview::snap_clock(target, &segs.segs, forward);
    crate::ui::narrate_page::mutate(|s| s.session = clock);
    // The CUE is snapped as well, to the clip's boundary rather than the edge inside it: a player asked for a
    // second outside the footage answers no frame.
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
        // Nothing to cue (no recording reaches the cut). The clock already moved; the reason is said, not
        // swallowed — a seek that landed in a gap would otherwise read as a broken button.
        crate::ui::narrate_page::refresh(window);
        return crate::ui::narrate_page::say(window, narrate_preview::NO_RECORDING_AT_START);
    }
    let s = crate::ui::narrate_page::read_state();
    crate::ui::narrate_page::refresh(window);
    crate::ui::narrate_page::say(window, &crate::ui::narrate_page::clock_text(&s))
}
