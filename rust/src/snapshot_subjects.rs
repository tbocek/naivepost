//! Which snapshot screens hand their asked filename to a SUBJECT of their own, and where that subject was staged.
//!
//! Most screens are pictures OF THE PAGE: `just snapshot 05-cut` writes the window at `shots/05-cut.png`, which is
//! what §05 draws. Three screens in §06-effects#2 are not — its three images are the effects lane, the framed
//! preview and the Effect popover — so for those the subject takes `<name>.png` (that file IS the picture the spec
//! drew) and the page it was staged on goes beside it as `<name>-page.png` -- `-page`, not `-window`, so the sidecar
//! sorts BEFORE the asked filename and a viewer comparing the screen with its spec picture lands on the subject.
//! Without this, a checker sampling
//! `shots/06-lane.png` sees a Cut-page window with no lane bar in it and concludes the lane is never painted.
//!
//! The doors themselves stay with the module that owns each subject ([`crate::snapshot_effect_menu::is_menu_subject`],
//! [`crate::snapshot_lane::is_lane_subject`], [`crate::snapshot_preview_frame::is_frame_subject`]); this file only
//! joins them up, so `snapshot::run` asks one place two questions instead of growing a third copy of the screen list.

/// Does this screen's subject take `<name>.png`, pushing the window shot to `<name>-page.png`?
pub fn subject_takes_the_name(screen: &str) -> bool {
    menu(screen) || lane(screen) || frame(screen) || picker(screen)
}

/// The sidecar name the staging step painted this screen's subject into, which the frame-taking step renames over
/// the asked filename. `None` for the popover (its subject is painted at the frame-taking step itself, from the
/// dropdown's box, because a popover has no render node inside the window) and for every ordinary page screen.
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
