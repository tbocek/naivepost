//! §07-narrate#1-screen (**Preview**): the wheel that steps a frame while the picture is PAUSED. The
//! rule is [`crate::narrate_frame`]'s; this file only reads the page's state, asks for the next second,
//! and writes it back — the shape every other Narrate wire uses, so the step can be tested without a
//! window and the widget cannot disagree with the rule.
//!
//! One `EventControllerScroll` on `narrate-picture`, named `wheel-narrate-picture`. It must not sit on
//! ground that already carries one: the Cut page attaches its own preview wheel through
//! [`crate::ui::cut_view::attach_wheel`] to `preview-panel`, which is a different widget in a different
//! page, so the two never meet on one surface.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_frame::{self, DEFAULT_FPS};

/// Attach the paused-frame wheel. Called once from the page's `wire`, after `set_content`.
pub fn install(window: &adw::ApplicationWindow) {
    let Some(picture) = picture_of(window) else {
        return;
    };
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_name(Some("wheel-narrate-picture"));
    let win = window.clone();
    wheel.connect_scroll(move |_ctrl, _dx, dy| {
        match step_paused(&win, dy) {
            Some(_stepped) => glib::Propagation::Stop,
            // Playing (the players own the clock) or no cut (no clock at all): the notch belongs to
            // whatever is behind the picture, so it is left unclaimed.
            None => glib::Propagation::Proceed,
        }
    });
    picture.add_controller(wheel);
}

/// One notch while paused: the second the picture should show, or `None` when the notch is not ours.
/// Split out so the wheel handler is three lines and the branch is testable through the same door a
/// real scroll signal reaches.
pub fn step_paused(window: &adw::ApplicationWindow, dy: f64) -> Option<f64> {
    let state = crate::ui::narrate_page::read_state();
    if state.playing || dy == 0.0 {
        return None;
    }
    let stepped = narrate_frame::step(
        state.session,
        state.length,
        narrate_frame::frame_seconds(DEFAULT_FPS),
        // A notch away from the viewer (positive dy) walks forward along the cut, the way the Cut page's
        // transport reads the same gesture.
        dy > 0.0,
    )?;
    let state = crate::ui::narrate_page::read_state();
    crate::ui::narrate_page::mutate(|s| s.session = stepped);
    // The still has to be the new frame, not just the number beside it: same seek seam the transport uses,
    // which cues a frozen picture at the snapped second when nothing is playing.
    crate::ui::narrate_page::seek_preview(window, stepped, Some(&state.segs), dy > 0.0);
    crate::ui::narrate_picture::repaint(window);
    Some(stepped)
}

/// The page's preview plate. Same lookup the painter's `install_draw` uses, so the wheel and the ink are
/// certainly on one widget.
fn picture_of(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(&content, "narrate-picture")?
        .downcast()
        .ok()
}
