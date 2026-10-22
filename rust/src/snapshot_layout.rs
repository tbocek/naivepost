//! One rule for where a Cut-page snapshot puts its top/bottom divider, so `snapshot::run` asks it instead of
//! holding the arithmetic, and §05-cut#1's picture can be compared with `spec/img/05-cut.png` at all.
//!
//! §03-shell.md §1 says "Above the minimum the layout grows with the window … splits keep their proportions",
//! and §A's opening for the Cut page's vertical paned is `paned 660/380` — in the spec's own 772 px-tall
//! window the upper pane is roughly half the page band. Headless nothing drags that handle, so a snapshot gets
//! whatever GTK's allocation gives: after §1's growth change the preview pane took the whole band and the ruler,
//! toolbar and tracks fell below the window's bottom edge, which is a shot of the wrong page. This puts the
//! split back at the ratio the picture draws, and only for a shot: a live window is the user's to drag.

use adw::prelude::*;
use gtk4 as gtk;

/// Share of the page band §A's picture gives the pane above the Cut page's divider: the upper pane sits a
/// little under half (`paned 660/380`, and `spec/img/05-cut.png` draws the preview pane just short of half of
/// the page under the header bar). Rounded to whole pixels and floored so a tiny band still shows a picture.
const UPPER_PANE_SHARE: f64 = 0.45;

/// Where the Cut page's top/bottom divider goes in a snapshot `page_h` px tall.
pub fn cut_upper_pane_px(page_h: i32) -> i32 {
    (page_h.max(1) as f64 * UPPER_PANE_SHARE).round() as i32
}

/// Ask §A's split of the Cut page's own divider, the same door a person's drag fires, and hand back the line
/// `snapshot::run` prints about it (`None` when there is nothing to say: no page scroller to measure, or a pane
/// already at or under the ratio so GTK's allocation stands).
///
/// The upper pane opened at §A's [`crate::ui::cut_page_layout::SELF_SPLIT_TOP_PX`] and took the whole leftover,
/// so the tape under it — ruler, toolbar, tracks, strip and camera rows, which is what §A's picture is about —
/// fell past the window's bottom edge. `top_h` is the height the layout actually gave that pane.
pub fn rest_cut_split(
    window: &adw::ApplicationWindow,
    split: &gtk::Paned,
    top_h: i32,
) -> Option<String> {
    let scroll = crate::ui::find_source_widget(window, "page-scroll-Cut")?;
    let page_h = scroll.height().max(1);
    let keep = cut_upper_pane_px(page_h);
    // The ask goes to the paned whether or not the pane needs moving: its own position may already sit lower
    // than the ratio asks, and printing the number it lands on is how a wrong shot can be traced.
    split.set_position(keep.min(top_h));
    // The relayout has to land before the pixels are taken; headless nothing else pumps the context here.
    crate::snapshot_face::pump_context(16);
    Some(format!(
        "05-cut split | page {page_h} px, upper pane asked {top_h}, set to {}",
        split.position()
    ))
}
