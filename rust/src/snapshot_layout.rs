//! Where a Cut-page snapshot puts its top/bottom divider, so `snapshot::run` holds no arithmetic and §05-cut#1 can be compared with `spec/img/05-cut.png`.
//! Headless nothing drags the handle and GTK's allocation gives the preview the whole band, so this restores §A's `paned 660/380` for a shot only.

use adw::prelude::*;
use gtk4 as gtk;

/// Rest BOTH Cut-page dividers once the window has a real size, as a drag would.
/// A `gtk::Paned` clamps `set_position` into its minimums and headless those are read before allocation; `notify::height` never fires for a widget allocated once, so the shot asks from a drain that runs AFTER the window took §1's size.
pub fn rest_page_splits(window: &adw::ApplicationWindow) {
    if let Some(divider) = crate::ui::find_source_widget(window, "main-divider")
        .and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        crate::ui::window_divider::rest_split(&divider);
    }
    let Some(vsplit) = crate::ui::find_source_widget(window, "cut-page-split")
        .and_then(|w| w.downcast::<gtk::Paned>().ok())
    else {
        return;
    };
    vsplit.set_position(
        vsplit
            .height()
            .max(crate::ui::cut_page_layout::SELF_SPLIT_TOP_PX),
    );
    if let Some(hsplit) = crate::ui::find_source_widget(window, "cut-top")
        .and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        hsplit.set_position(crate::ui::cut_page_layout::SELF_SPLIT_PREVIEW_PX);
    }
    crate::snapshot_face::pump_context(16);
    report_form_column(window);
}

/// Print what the Cut page form column was ALLOCATED at the shot size, to explain why a render opens on another reading than §1's 'Aspect ratio:' (badge **3**).
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

/// Put the Cut page transport in the state §1 draws it (badges **5**-**9** are greyed until a preview plays): seed a cut, stand the red line in the first clip, press ▶, then hand back with the run bar's ⏹ so the frame does not depend on wall-clock time.
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
