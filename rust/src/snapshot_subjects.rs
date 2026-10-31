//! Which snapshot screens hand their asked filename to a SUBJECT of their own, and where that subject was staged.

/// Does this screen's subject take `<name>.png`, pushing the window shot to `<name>-page.png`?
pub fn subject_takes_the_name(screen: &str) -> bool {
    menu(screen) || lane(screen) || frame(screen) || picker(screen) || sources(screen)
}

/// §4's picture is the sources LIST plus its frame controls, not the page beside them.
pub fn is_sources_subject(screen: &str) -> bool {
    screen == "03-sources"
}

fn sources(screen: &str) -> bool {
    is_sources_subject(screen)
}

pub fn subject_sidecar(screen: &str) -> Option<&'static str> {
    if lane(screen) {
        return Some("lane-shot");
    }
    if frame(screen) {
        return Some("frame");
    }
    None
}

fn menu(screen: &str) -> bool {
    crate::snapshot_effect_menu::is_menu_subject(screen)
}

fn lane(screen: &str) -> bool {
    crate::snapshot_lane::is_lane_subject(screen)
}

fn frame(screen: &str) -> bool {
    crate::snapshot_preview_frame::is_frame_subject(screen)
}

fn picker(screen: &str) -> bool {
    crate::snapshot_prompt_picker::is_picker_subject(screen)
}
