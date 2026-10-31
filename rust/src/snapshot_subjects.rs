//! Which snapshot screens hand their asked filename to a SUBJECT of their own, and where that subject was staged.

/// Does this screen's subject take `<name>.png`, pushing the window shot to `<name>-page.png`?
pub fn subject_takes_the_name(screen: &str) -> bool {
    crate::snapshot_effect_menu::is_menu_subject(screen)
        || crate::snapshot_lane::is_lane_subject(screen)
        || crate::snapshot_preview_frame::is_frame_subject(screen)
        || crate::snapshot_prompt_picker::is_picker_subject()
        || is_sources_subject(screen)
}

/// §4's picture is the sources LIST plus its frame controls, not the page beside them.
pub fn is_sources_subject(screen: &str) -> bool {
    screen == "03-sources"
}

pub fn subject_sidecar(screen: &str) -> Option<&'static str> {
    if crate::snapshot_lane::is_lane_subject(screen) {
        return Some("lane-shot");
    }
    if crate::snapshot_preview_frame::is_frame_subject(screen) {
        return Some("frame");
    }
    None
}
