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
    // page out as a paned whose bottom half carries the toolbar row (~64 px of buttons). The PAGE half keeps
    // its own minimum (`shrink_start_child(false)`): letting it shrink lets GTK crush it to its own floor and
    // hand the leftover to the band — measured, the page got 342 px while the bottom box took 843 of an 844 px
    // divider, so the collapsed log's body swallowed 809 px and the run-bar row was painted over the top of
    // the page. That was the shell shot with no bottom bar.
    //
    // The reason this rule works now and did not before is one line in `window.rs`:
    // `AdwViewStack::set_vhomogeneous(false)`. Measured homogeneously, the stack charged EVERY tab's minimum
    // (the Cut tape's ~760 px) to the start half, so 760 + the band could not fit §1's 900 px and GTK gave the
    // band nothing; per-tab, Prepare charges 342 px and the band gets the other ~500 of the 844 px divider.
    // Only the band may be squeezed, and `bottom-bar` carries its own floor (§1's two rows, asked through
    // [`super::window_shot::rest_px`] in `window.rs`) so a squeeze cannot take it below them.
    let log = log.downcast_ref::<gtk::Expander>().expect("the log is an expander").clone();
    divider.set_shrink_start_child(false);
    // THE BAND MAY BE SQUEEZED, DOWN TO WHAT IT ASKS FOR and no further: §1 wants the page to be wholly on
    // screen at the minimum (which needs the page half NON-shrinkable, so GTK cannot crush it to 342 px and
    // dump the rest on the band) while the band is squeezed to the two rows §1 counts. The handle does that:
    // `rest_split` puts it at `total − band_px(…)` and the paned then allocates the end child exactly that.
    divider.set_shrink_end_child(true);
    // §1's split of the EXTRA height: the page takes it, the band keeps its own. `resize_start_child` is what
    // makes a growing window give its new pixels to the page instead of stretching the log half.
    divider.set_resize_start_child(true);
    // THE PAGE HALF MAY NOT BE FORCED TO CARRY THE BAND. A paned clamps every `set_position` ask into
    // `[start minimum, total − end minimum]`, and headless the two halves' minimums summed to MORE than the
    // window (§1's 900 px): the page asked ~760 (an `AdwViewStack` measures every tab, Cut included) and the
    // bottom box ~274 (a collapsed expander still counts its undrawn 220 px body). A clamped range whose low
    // end is above its high end resolves to "page = everything", which is exactly the shell shot with no bottom
    // bar: run-bar on the last pixel, log header below the edge. Both halves therefore shrink, so the position
    // is the RULE's answer rather than a clamp — each page already scrolls inside itself (`window_page_box`'s
    // per-page scroller), and the band's resting height is what `window_shot::rest_px` says, not what the
    // expander happens to measure.
    divider.set_start_child(Some(page));
    page.set_vexpand(true);
    // THE BAND IS THE HALF THAT DOES NOT EXPAND, and that single fact is what makes §1's sentence come true:
    // a `gtk::Paned` hands its leftover to the halves that expand, so an expanding band ate 501 px of an 844 px
    // divider (measured) while a non-expanding one is allocated exactly what it measures — ~54 px collapsed
    // (run-bar row + log header) and ~274 px open (plus `LOG_OPEN_PX`) — leaving the page, which does expand,
    // "all the rest of the height".
    // …and the band asks for exactly its own height. It does NOT expand: a `gtk::Paned` hands its leftover to
    // the halves that do, and an expanding band ate 501 px of an 844 px divider (measured) instead of the ~274 px
    // it measures with the log open.
    let bottom = bottom_of(run_row, &log.upcast_ref::<gtk::Widget>().clone());
    // THE BAND ASKS FOR THE COLLAPSED TWO ROWS, and that floor is what GTK honours in BOTH log states: a
    // non-shrinkable end child is allocated `max(its minimum, total − position)`, and the collapsed log's body is
    // hidden (`window_log::hide_body_while_collapsed`), so the box measures its 54 px even with the expander
    // open. Asking for §1's OPEN height (274 px = two rows + §10's 220 px body) was what pinned the band at 501 px
    // with the log CLOSED — 10× the height §1 counts. The open log's 220 px comes from the handle instead, which
    // `rest_split` moves to `total − band_px(…)`; the paned clamps that to this minimum, so the ask has to be the
    // small one or the clamp can never come down.
    bottom.set_size_request(-1, super::window_shot::rest_px(run_row, &log));
    let log = log.downcast_ref::<gtk::Expander>().expect("the log is an expander").clone();
    // The closures below outlive this call, so they hold their OWN widget references — clones of the two
    // widgets §1 counts, which live as long as the window that owns the divider.
    let run_row_held = run_row.clone();
    // §1's other half of the same sentence: "while it is collapsed the bottom half is EXACTLY as tall as
    // the run bar row plus the log expander's header, with no empty space under them". A `gtk::Paned`
    // allocates its end child whatever is left below `position`, and a fixed remembered position leaves
    // that leftover as an empty band — which is what the shell shot showed: every control present, all of
    // them drawn in a strip, and a tall blank area under the page. So the handle goes where the band ENDS,
    // measured from the two widgets §1 counts, and the page's own vexpand takes everything above it.
    // The readouts join the run bar's row at its right end. They keep their own name and their own box:
    // what changed is only WHERE that box sits, so every door that finds a readout by name still works.
    // `run-bar`'s first child is the scroller holding the controls (`window_run_scroller`), which is what
    // stops the buttons from pinning the window's width; this item's test walks into it by name.
    run_row
        .downcast_ref::<gtk::Box>()
        .expect("the run bar is a horizontal box")
        .append(readout_row);
    divider.set_resize_start_child(true);
    divider.set_end_child(Some(&bottom));
    into.append(&divider);
    // Every resting ask reads the band AT THAT MOMENT (§1: "Expanding the log moves the divider up so the
    // log gets room of its own; collapsing it returns that height to the page"), so a remembered number
    // captured before the expander could ever change would snap an open log back to its collapsed height.
    // The resting split is asked of the paned's own height when it has one; before the first allocation the
    // collapsed band is the best available guess, and the resize rules below keep it correct after.
    divider.set_position(START_PAGE_PX);
    // THE BAND DECLARES ITS OWN HEIGHT — see the `bottom` above; §1 counts "the run bar row plus the log
    // expander's header" while the log is closed and gives the body "room of its own" once it opens, so the ask
    // moves with the toggle in `connect_band_rest`. Asking `set_position` alone never changed anything: a paned
    // keeps both children at their own asks.
    connect_band_rest(&divider, &run_row_held, &log);
    // …and once more when the window itself gets its real size. The asks below read the paned's OWN height,
    // and an unmapped one answers 1 px, so their first pass lands on a number that means nothing; headless
    // nothing else fires a resize after the map, which left the handle at `START_PAGE_PX` and the band off the
    // bottom of the shot. `connect_realize` alone was not enough: realize precedes the allocation.
    connect_first_allocation(&divider);
    // …and again on every later resize: a window that grows gives the extra to the PAGE (§1: "Expanding the
    // log moves the divider up… collapsing it returns that height to the page", and at rest the band keeps
    // its own height however tall the window gets). Without this the handle stays where the first map put
    // it and the leftover lands under the log as blank space — which is exactly what the shell shot showed.
    divider.connect_notify_local(Some("height"), move |d, _| {
        if d.height() <= 1 {
            return;
        }
        // One rule, one place: [`rest_split`] re-asks the band's height for the log's state and moves the handle.
        rest_split(d);
    });
    // …and at every real ALLOCATION too, so the rule cannot depend on call order. A paned clamps its position to
    // `[start minimum, total − end minimum]`, and headless the two halves' minimums can sum past the window, so an
    // ask made before the size lands is clamped by a number that means nothing. GTK 4 has no allocate signal to
    // hook (`connect_alloc`/`connect_size_allocate` do not exist in gtk4-rs), so this watches `width`/`height`
    // together with one shared cell: it fires whenever either changes, and re-asks the rule only for a height it
    // has not seen, which keeps the re-ask from feeding back into itself.
    let last_alloc_height = std::rc::Rc::new(std::cell::Cell::new(0));
    for name in ["width", "height"] {
        let seen = last_alloc_height.clone();
        divider.connect_notify_local(Some(name), move |d, _| {
            let height = d.height();
            if height > 1 && seen.get() != height {
                seen.set(height);
                rest_split(d);
            }
        });
    }
}

/// Re-ask the resting split the first time the paned is allocated a real height — the one moment the number
/// below stops being a guess. Fired by the paned's own size change and then never again.
fn connect_first_allocation(divider: &gtk::Paned) {
    let asked = std::rc::Rc::new(std::cell::Cell::new(false));
    // `rest_split` finds the two widgets §1 counts inside the band, so this closure needs only the paned.
    // THE ONE DOOR THAT WORKS HEADLESS. `connect_realize` fires before the allocation (measured: the paned
    // answers height 0 there), and a `notify::height` handler never runs at all under xvfb — the paned is
    // allocated once and nobody re-notifies it — so every ask landed on a guess and the band fell off the bottom
    // of the shot. A one-shot idle does see the real size, because by then GTK has sized the window; it is
    // bounded (no sleep, no waiting on a condition a busy machine might not reach in time) and it asks the same
    // rule a person's drag asks.
    let held = divider.clone();
    glib::idle_add_local_once(move || {
        if held.height() <= 1 || asked.get() {
            return;
        }
        asked.set(true);
        rest_split(&held);
    });
}

/// Ask §1's resting split of the shell's divider: the band keeps its own height for the log's current state
/// and the page takes everything above it. `install` wires this to every event that could change it, and a
/// snapshot asks it once more after the window has its real size — headless nothing drags this handle, so the
/// default GTK settled on (measured: 342 px of page against 501 px of bottom box in an 844 px divider) is what
/// a shot would otherwise record. The door a person's drag fires is the same rule.
pub fn rest_split(divider: &gtk::Paned) {
    let Some(bottom) = divider.end_child() else { return };
    let Some(log) = find_in(&bottom, "log-expander").and_then(|w| w.downcast::<gtk::Expander>().ok()) else {
        return;
    };
    let Some(run_row) = bottom.first_child() else { return };
    // THE BAND CARRIES NO `set_size_request`, and that is the rule: a size request is a FLOOR GTK will not
    // squeeze below, so asking for the OPEN band here (274 px with §10's 220 px log body) held the bottom half
    // at 501 px even with the log closed, where §1 wants only the run-bar row + header (~54 px). The handle
    // alone does the work — `rest_position(total, band)` puts it at `total − band` and the paned then allocates
    // the end child exactly that.
    let total = divider.height().max(1);
    let band = super::window_shot::band_px(&run_row, &log).clamp(1, total - 1);
    // NO `set_size_request` on the band here: a size request is a floor GTK will not squeeze below, and the one
    // `install` puts on it is deliberately the COLLAPSED two rows. Asking for the open height (274 px) instead
    // held the bottom half at 501 px even with the log closed, where §1 counts only ~54 px.
    divider.set_position(super::window_shot::rest_position(total, band));
}

/// The first widget named `name` at or below `node` — how [`rest_split`] finds the two widgets §1 counts.
fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    for child in node.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
        if let Some(widget) = child.downcast_ref::<gtk::Widget>().and_then(|w| find_in(w, name)) {
            return Some(widget);
        }
    }
    None
}

/// Re-ask the divider for its resting split whenever the log's state changes: opening it pushes the handle
/// UP (the log gets room of its own out of the page), closing it hands that height straight back.
fn connect_band_rest(divider: &gtk::Paned, run_row: &gtk::Widget, log: &gtk::Expander) {
    divider.connect_realize({
        let (d, run_row, log) = (divider.clone(), run_row.clone(), log.clone());
        move |_d| {
            // Same widget the closure captured: asking the paned for its own height is the only way to know
            // what room the band has once the window is mapped.
            let total = d.height().max(1);
            d.set_position(super::window_shot::rest_position(total, super::window_shot::rest_px(&run_row, &log)));
        }
    });
    log.connect_expanded_notify({
        let (d, run_row) = (divider.clone(), run_row.clone());
        move |l| {
            let total = d.height().max(1);
            // Opening the log is the ONE moment the band is taller than its collapsed pair of rows: §1 says the
            // expander then gets "room of its own", taken out of the page. Closing returns to the resting rule.
            let band = if l.is_expanded() {
                super::window_shot::band_px(&run_row, l)
            } else {
                super::window_shot::rest_px(&run_row, l)
            };
            d.set_position(super::window_shot::rest_position(total, band));
        }
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
