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
fn cut_upper_pane_px(page_h: i32) -> i32 {
    (page_h.max(1) as f64 * UPPER_PANE_SHARE).round() as i32
}

/// Rest BOTH of the Cut page's dividers the moment this window has a real size — the same door a person's
/// drag fires, and the reason a headless shot can show §A's `paned 660/380` at all.
///
/// A `gtk::Paned` clamps every `set_position` into `[start minimum, total - end minimum]`, and headless the
/// two are read before the window is allocated, so both asks land on their clamp (measured: a 5 px tall
/// `cut-page-split` and a 65 px page scroller — every control below the divider built and unpainted). The
/// shell's own divider solved this by re-asking `rest_split` at its first real allocation
/// (`window_divider::install`); `install` also runs for these two, but a `notify::height` callback never
/// fires for a widget that is allocated exactly once, which is what a headless window does. So the shot asks
/// both rules from a drain that runs AFTER the window took §1's size. No sleep, no guessed wait: the drain
/// only runs the loop that is already pending.
pub fn rest_page_splits(window: &adw::ApplicationWindow) {
    let Some(scroll) = crate::ui::find_source_widget(window, "page-scroll-Cut") else {
        return;
    };
    let page_h = scroll.height().max(1);
    // The shell's page/band split first: it is what decides how much room the page has at all.
    if let Some(divider) = crate::ui::find_source_widget(window, "main-divider")
        .and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        crate::ui::window_divider::rest_split(&divider);
    }
    // …then the Cut page's top/bottom divider, at §A's opening clamped into the room the page really has.
    let Some(vsplit) = crate::ui::find_source_widget(window, "cut-page-split")
        .and_then(|w| w.downcast::<gtk::Paned>().ok())
    else {
        return;
    };
    vsplit
        .set_position(cut_upper_pane_px(page_h).max(crate::ui::cut_page_layout::SELF_SPLIT_TOP_PX));
    // …and the left/right divider, at §A's 660 px of preview, once its own pane has a width to be clamped by.
    if let Some(hsplit) = crate::ui::find_source_widget(window, "cut-top")
        .and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        hsplit.set_position(crate::ui::cut_page_layout::SELF_SPLIT_PREVIEW_PX);
    }
    crate::snapshot_face::pump_context(16);
    report_form_column(window);
}

/// Print what the Cut page's form column was ALLOCATED at the size the shot is taken at, one line: §1's
/// first idle reading is 'Aspect ratio:' (badge **3**), and when a render opens on another reading instead,
/// these are the numbers that say why (a viewport shorter than its body, a row whose y sits above the
/// viewport's own top edge). Same shape as the badge-31 report in `snapshot::run`.
pub fn report_form_column(window: &adw::ApplicationWindow) {
    let Some(scroller) = crate::ui::find_source_widget(window, "cut-form-scroller")
        .and_then(|w| w.downcast::<gtk::ScrolledWindow>().ok())
    else {
        return;
    };
    let target = window.clone().upcast::<gtk::Widget>();
    let body = crate::ui::find_source_widget(window, "cut-form");
    let row = crate::ui::find_source_widget(window, "cut-readout-aspect-ratio-key");
    let adjust = scroller.vadjustment();
    println!(
        "snapshot: 05-cut form column | viewport {} px tall at y={}, body {} px, adjustment page={} value={}, \n         'Aspect ratio:' at y={}",
        scroller.height(),
        scroller
            .compute_bounds(&target)
            .map(|r| r.y() as i32)
            .unwrap_or(-1),
        body.map(|b| b.height()).unwrap_or(-1),
        adjust.page_size(),
        adjust.value(),
        row.and_then(|r| r.compute_bounds(&target))
            .map(|r| r.y() as i32)
            .unwrap_or(-1),
    );
}

/// Ask §A's split of the Cut page's own divider, the same door a person's drag fires, and hand back the line
/// `snapshot::run` prints about it (`None` when there is nothing to say: no page scroller to measure, or a pane
/// already at or under the ratio so GTK's allocation stands).
///
/// The upper pane opened at §A's [`crate::ui::cut_page_layout::SELF_SPLIT_TOP_PX`] and took the whole leftover,
/// so the tape under it — ruler, toolbar, tracks, strip and camera rows, which is what §A's picture is about —
/// fell past the window's bottom edge. `top_h` is the height the layout actually gave that pane.
/// Put the Cut page's transport into the state §1's picture draws it in, through the page's own buttons.
///
/// §1's badges **5**-**9** (▶ recording, ▶✂ cut, ▶✂✂ review, frame steps, preview volume) are greyed until a
/// preview is playing, so the reference shows them inked. A snapshot presses no keys, so this fires the same
/// doors `03-runbar-busy` fires — seed a cut, stand the red line inside the first clip, press the page's own ▶
/// — and then hands the transport back with the run bar's ⏹ door, so the shot is the page at rest with its
/// transport live rather than a running preview whose frame would depend on wall-clock time.
pub fn stage_cut_transport(window: &adw::ApplicationWindow) {
    use crate::{cut, cut_line, ui};
    ui::seed_review_cut(
        window,
        &cut::Cut {
            segs: vec![
                cut::Seg {
                    s: 10.0,
                    e: 30.0,
                    cam: 0,
                    ..Default::default()
                },
                cut::Seg {
                    s: 40.0,
                    e: 60.0,
                    cam: 0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    );
    ui::set_line_position(window, cut_line::LinePos { t: 15.0 });
    ui::set_playhead(window, 15.0);
    if let Some(play) = ui::play_recording_button(window) {
        play.emit_by_name::<()>("clicked", &[]);
    }
    if let Some(stop) = ui::play_button(window) {
        stop.emit_by_name::<()>("clicked", &[]);
    }
}
