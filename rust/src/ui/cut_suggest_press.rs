//! §05-cut#2 S1/S2 — the Cut page's ▶, as the window seam. Split out of `window.rs`, which is past
//! its size budget: this file owns what that press says and shows, and nothing else. The rules live in
//! [`crate::ui::cut_suggest_land`] (refusal order, both modes) and
//! [`crate::ui::cut_suggest_model_run`] (§S2b's call); landing is theirs too, never this module's.

/// The ▶ on the Cut page is Suggest: refused or answered here, before the run bar opens, and the bar
/// never opens either way — a suggestion is an answer, not a run. Returns whether the caller must stop
/// (the flow's own `stops`), after having said its sentence and repainted the bar once.
///
/// `preview_started` answers §F2.14's entry node, which reads "▶ · preview not started": with the
/// preview cued or playing, this same button is §F0.2 S2's transport toggle, and asking for a cut there
/// would answer a question the person did not ask while parking nothing. So a started preview is not
/// this press's to take, and the caller falls through to `press`.
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
