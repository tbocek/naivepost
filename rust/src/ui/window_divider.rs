//! §03-shell.md §1: "Page and bottom bar are two halves of a draggable divider, not a fixed strip;
//! collapsing the log expander returns its height to the page."
//!
//! The window's own geometry, split out of `window.rs` (which is past its size budget): this builds the
//! vertical [`gtk::Paned`] whose start child is the page stack and whose end child is the box holding the
//! run bar, the Inputs/Outputs readouts and the log expander. Dragging the handle moves height between
//! the two halves, so the log is a half of the window rather than a strip bolted under it.
//!
//! Both halves shrink (`shrink_start_child` / `shrink_end_child`), which is what keeps the window's floor
//! the pages' own minimum instead of the sum of the two — `tests/suite/window_shrinks.rs` measures that
//! at exactly §1's 1240×900 minimum on every tab. The page's own scroller is what stops the top half becoming
//! unreadable when the handle is dragged down.

use adw::prelude::*;
use gtk4 as gtk;

/// The starting split, in pixels of page height. At rest the log is collapsed, so the handle sits where
/// the page keeps its height and the thin bottom row keeps its own; this number only ever decides the
/// STARTING position, never a floor — the user's drag replaces it and nothing reads it back.
const START_PAGE_PX: i32 = 480;

/// Build §1's divider into `into`: the page stack above, the bottom bar below. §1's order for that bar is
/// "one row holding the run bar and, to its right in the same row, the visible tab's Inputs readout,
/// then the Outputs folder button, then the Outputs readout; UNDER that row the log expander" — so the
/// readouts are part of the run bar's row rather than a strip of their own, and only two children go on
/// `bottom`.
pub fn install(into: &gtk::Box, page: &gtk::Widget, run_row: &gtk::Widget, readout_row: &gtk::Widget, log: &gtk::Widget) {
    let divider = gtk::Paned::new(gtk::Orientation::Vertical);
    divider.set_widget_name("main-divider");
    divider.set_wide_handle(false);
    // §1's window MINIMUM (1240x900) says the whole page is on screen at that size, and §05#1 lays the Cut
    // page out as a paned whose bottom half carries the toolbar row (~64 px of buttons). With BOTH halves
    // shrinkable GTK was free to allocate the bottom half its own natural height first, and headless — where
    // nothing maps the window before a test reads `is_mapped()` — it did exactly that: the page got a sliver
    // and every Cut-page control (`preview-volume`, `line-step-*`, `camera-rows`) reported unmapped. The page
    // half must therefore keep its own minimum; only the band may be squeezed.
    divider.set_shrink_start_child(false);
    divider.set_shrink_end_child(true);
    divider.set_start_child(Some(page));
    page.set_vexpand(true);
    // §1's other half of the same sentence: "while it is collapsed the bottom half is EXACTLY as tall as
    // the run bar row plus the log expander's header, with no empty space under them". A `gtk::Paned`
    // allocates its end child whatever is left below `position`, and a fixed remembered position leaves
    // that leftover as an empty band — which is what the shell shot showed: every control present, all of
    // them drawn in a strip, and a tall blank area under the page. So the handle goes where the band ENDS,
    // measured from the two widgets §1 counts, and the page's own vexpand takes everything above it.
    let band = super::window_shot::height_px(run_row, log.downcast_ref::<gtk::Expander>().expect("the log is an expander"));
    // The readouts join the run bar's row at its right end. They keep their own name and their own box:
    // what changed is only WHERE that box sits, so every door that finds a readout by name still works.
    // `run-bar`'s first child is the scroller holding the controls (`window_run_scroller`), which is what
    // stops the buttons from pinning the window's width; this item's test walks into it by name.
    run_row
        .downcast_ref::<gtk::Box>()
        .expect("the run bar is a horizontal box")
        .append(readout_row);
    divider.set_resize_start_child(true);
    divider.set_end_child(Some(&bottom_of(run_row, log)));
    into.append(&divider);
    // The resting split is asked of the paned's own height when it has one; before the first allocation the
    // remembered start is the best available guess, and the resize rule above keeps the band correct after.
    divider.set_position(START_PAGE_PX);
    divider.connect_realize({
        let divider = divider.clone();
        move |d| {
            // Same widget the closure captured: `connect_realize` hands it back, and asking the paned for
            // its own height is the only way to know what room the band has once the window is mapped.
            let _ = &divider;
            let total = d.height().max(1);
            d.set_position(super::window_shot::rest_position(total, band));
        }
    });
    // …and again on every later resize: a window that grows gives the extra to the PAGE (§1: "Expanding the
    // log moves the divider up… collapsing it returns that height to the page", and at rest the band keeps
    // its own height however tall the window gets). Without this the handle stays where the first map put
    // it and the leftover lands under the log as blank space — which is exactly what the shell shot showed.
    divider.connect_notify_local(Some("height"), move |d, _| {
        let total = d.height().max(1);
        if total <= 1 {
            return;
        }
        d.set_position(super::window_shot::rest_position(total, band));
    });
}

/// The bottom half: the run bar's row (which already carries the readouts) and under it the log expander.
fn bottom_of(run_row: &gtk::Widget, log: &gtk::Widget) -> gtk::Box {
    let bottom = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom.set_widget_name("bottom-bar");
    bottom.append(run_row);
    bottom.append(log);
    bottom
}
