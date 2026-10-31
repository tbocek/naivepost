//! §05-cut#2 S1/S2: the Cut page's ▶ as the window seam. Rules live in [`crate::ui::cut_suggest_land`] and
//! [`crate::ui::cut_suggest_model_run`]; landing is theirs.

/// The ▶ on the Cut page is Suggest: refused or answered here, before the run bar opens. Returns whether the caller must stop.
/// `preview_started` is §F2.14's entry node: with the preview cued or playing, ▶ is the transport toggle and falls through to `press`.
pub fn on_play(
    window: &adw::ApplicationWindow,
    busy: bool,
    preview_started: bool,
    mut say: impl FnMut(&str),
    repaint: impl FnOnce(),
) -> bool {
    if preview_started {
        return false;
    }
    let answer = crate::ui::cut_suggest_land::suggest_press(
        window,
        &crate::ui::window::session_policy(window),
        busy,
    );
    if answer.refused {
        crate::ui::window::log_line(&format!(">>> suggest: {}", answer.said));
    }
    if !answer.said.is_empty() {
        say(&answer.said);
    }
    repaint();
    answer.stops
}
