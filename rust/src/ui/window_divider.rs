//! §03-shell.md §1: "Page and bottom bar are two halves of a draggable divider, not a fixed strip;
//! collapsing the log expander returns its height to the page."
//!
//! Builds the vertical [`gtk::Paned`]: page stack above, below it the box holding the run bar, the
//! Inputs/Outputs readouts and the log expander. Both halves shrink, which keeps the window's floor the pages'
//! own minimum (`tests/suite/window_shrinks.rs` measures it at §1's 1240×900).

use adw::prelude::*;
use gtk4 as gtk;

/// The starting split in pixels of page height; never a floor, the user's drag replaces it.
const START_PAGE_PX: i32 = 480;

/// Build §1's divider into `into`: the page stack above, the bottom bar below. The readouts are part of the
/// run bar's row (§1), so only two children go on `bottom`.
pub fn install(
    into: &gtk::Box,
    page: &gtk::Widget,
    run_row: &gtk::Widget,
    readout_row: &gtk::Widget,
    log: &gtk::Widget,
) {
    let divider = gtk::Paned::new(gtk::Orientation::Vertical);
    divider.set_widget_name("main-divider");
    divider.set_wide_handle(false);
    // The PAGE half keeps its own minimum (`shrink_start_child(false)`): letting it shrink lets GTK crush it
    // to its floor and hand the leftover to the band (measured: page 342 px, bottom box 843 of 844).
    // That works only with `AdwViewStack::set_vhomogeneous(false)` in `window.rs`: homogeneous, every tab's
    // minimum (the Cut tape's ~760 px) was charged to the start half. Only the band may be squeezed, and
    // `bottom-bar` carries its own floor ([`window_shot::height_ask`]).
    let log = log
        .downcast_ref::<gtk::Expander>()
        .expect("the log is an expander")
        .clone();
    divider.set_shrink_start_child(false);
    divider.set_shrink_end_child(false);
    // The page takes the EXTRA height, the band keeps its own (§1).
    divider.set_resize_start_child(true);
    // THE PAGE HALF MAY NOT BE FORCED TO CARRY THE BAND. A paned clamps `set_position` into
    // `[start minimum, total − end minimum]`, and headless the minimums summed to MORE than the window (page
    // ~760, bottom ~274 because a collapsed expander still counts its 220 px body); an inverted range resolves
    // to "page = everything" and the run bar falls off the shot. Both halves therefore shrink, so the position
    // is the RULE's answer (`window_shot::height_ask`), not a clamp.
    divider.set_start_child(Some(page));
    page.set_vexpand(true);
    // THE BAND DOES NOT EXPAND: a `gtk::Paned` hands its leftover to the halves that do (an expanding band ate
    // 501 px of 844), while a non-expanding one gets exactly what it measures (~54 px collapsed, ~274 open).
    let bottom = bottom_of(run_row, &log.upcast_ref::<gtk::Widget>().clone());
    // THE BAND ASKS FOR THE COLLAPSED TWO ROWS: a non-shrinkable end child is allocated `max(its minimum,
    // total − position)`, and the collapsed log's body is hidden (`window_log::hide_body_while_collapsed`).
    // Asking for the OPEN height pinned the band at 501 px with the log closed. The open log's room comes
    // from the handle (`rest_split` moves it to `total − band_px(…)`). Margins (8 px above, 12 below) come on top.
    bottom.set_size_request(
        -1,
        super::window_shot::height_ask(run_row, &log) - crate::ui::margins::BAR_MARGINS_PX,
    );
    let log = log
        .downcast_ref::<gtk::Expander>()
        .expect("the log is an expander")
        .clone();
    // The closures outlive this call, so they hold their OWN clones of the two widgets §1 counts.
    let run_row_held = run_row.clone();
    // §1: "while it is collapsed the bottom half is EXACTLY as tall as the run bar row plus the log expander's
    // header". A `gtk::Paned` allocates its end child whatever is left below `position`, so the handle goes
    // where the band ENDS. The readouts keep their name and box, so every door finding one by name still works.
    // `run-bar`'s first child is the scroller (`window_run_scroller`) that stops the buttons pinning the window's width.
    run_row
        .downcast_ref::<gtk::Box>()
        .expect("the run bar is a horizontal box")
        .append(readout_row);
    divider.set_resize_start_child(true);
    divider.set_end_child(Some(&bottom));
    into.append(&divider);
    // Every resting ask reads the band AT THAT MOMENT (§1: expanding the log moves the divider up), so a
    // remembered number would snap an open log back to its collapsed height. Before the first allocation the
    // collapsed band is the best guess.
    divider.set_position(START_PAGE_PX);
    // The band declares its own height (see `bottom`); the ask moves with the toggle in `connect_band_rest`,
    // since `set_position` alone never changes a paned's children's asks.
    connect_band_rest(&divider, &run_row_held, &log);
    // Once more when the window gets its real size: an unmapped paned answers 1 px, and headless nothing else
    // fires a resize after the map. `connect_realize` alone was not enough: realize precedes the allocation.
    connect_first_allocation(&divider);
    // And on every later resize: a growing window gives the extra to the PAGE (§1); otherwise the leftover
    // lands under the log as blank space.
    connect_every_layout(&divider);
}

/// Re-ask the resting split the first time the paned is allocated a real height, then never again.
fn connect_first_allocation(divider: &gtk::Paned) {
    let asked = std::rc::Rc::new(std::cell::Cell::new(false));
    // THE ONE DOOR THAT WORKS HEADLESS: `connect_realize` fires before the allocation (height 0) and a
    // `notify::height` handler never runs under xvfb. A one-shot idle sees the real size, is bounded, and asks
    // the same rule a person's drag asks.
    let held = divider.clone();
    glib::idle_add_local_once(move || {
        if held.height() <= 1 || asked.get() {
            return;
        }
        asked.set(true);
        rest_split(&held);
    });
}

/// Re-ask the resting split whenever the window is laid out at a height the divider has not had yet.
///
/// The hook is the window surface's `layout` signal (GTK allocates first, so the height is the new one). A
/// widget has no `height` property to watch: `notify::height` handlers never fired, on Xvfb or anywhere.
fn connect_every_layout(divider: &gtk::Paned) {
    let seen = std::rc::Rc::new(std::cell::Cell::new(0));
    let share = std::rc::Rc::new(std::cell::Cell::new(0.0_f64));
    divider.connect_position_notify({
        let (seen, share) = (seen.clone(), share.clone());
        move |d| {
            let height = d.height();
            if height > 1 && height == seen.get() && open_log(d).is_some() {
                share.set(f64::from(height - d.position()) / f64::from(height));
            }
        }
    });
    divider.connect_realize(move |d| {
        let Some(surface) = d.native().and_then(|native| native.surface()) else {
            return;
        };
        let held = d.downgrade();
        let (seen, share) = (seen.clone(), share.clone());
        surface.connect_layout(move |_, _, _| {
            let Some(d) = held.upgrade() else { return };
            let height = d.height();
            // Only a height not seen yet: the re-ask moves the handle, which lays the window out again.
            if height <= 1 || seen.get() == height {
                return;
            }
            seen.set(height);
            collapse_without_room(&d);
            match open_log(&d) {
                Some((run_row, log)) if share.get() > 0.0 => {
                    let kept = (share.get() * f64::from(height)).round() as i32;
                    let band = kept
                        .max(super::window_shot::band_px(&run_row, &log))
                        .clamp(1, height - 1);
                    d.set_position(super::window_shot::rest_position(height, band));
                    crate::ui::window_run_scroller::hand_the_free_width_to(&run_row);
                }
                _ => rest_split(&d),
            }
        });
    });
}

/// Too little room for an open log: it collapses, the "Log" header and the run bar stay. On the next idle,
/// because this runs mid-layout. Asked of the page's own minimum, which the band cannot take from.
fn collapse_without_room(divider: &gtk::Paned) {
    let held = divider.clone();
    glib::idle_add_local_once(move || {
        let Some((run_row, log)) = open_log(&held) else {
            return;
        };
        let Some(page) = held.start_child() else {
            return;
        };
        let page_floor = page.measure(gtk::Orientation::Vertical, -1).0;
        let rows = super::window_shot::height_px(&run_row, &log);
        if held.height() > 1
            && held.height() - page_floor - rows < super::window_bench::LOG_MIN_BODY_PX
        {
            log.set_expanded(false);
        }
    });
}

/// The band's run-bar row and log expander, when the log is open: what an open band is measured by.
fn open_log(divider: &gtk::Paned) -> Option<(gtk::Widget, gtk::Expander)> {
    let bottom = divider.end_child()?;
    let log = find_in(&bottom, "log-expander")?
        .downcast::<gtk::Expander>()
        .ok()?;
    if !log.is_expanded() {
        return None;
    }
    Some((bottom.first_child()?, log))
}

/// Ask §1's resting split of the shell's divider: the band keeps its own height for the log's current state
/// and the page takes the rest. `install` wires this to every event that could change it; a snapshot asks it
/// once more after the window has its real size, since headless nothing drags the handle.
pub fn rest_split(divider: &gtk::Paned) {
    let Some(bottom) = divider.end_child() else {
        return;
    };
    let Some(log) =
        find_in(&bottom, "log-expander").and_then(|w| w.downcast::<gtk::Expander>().ok())
    else {
        return;
    };
    let Some(run_row) = bottom.first_child() else {
        return;
    };
    // THE BAND CARRIES NO `set_size_request`: a size request is a FLOOR GTK will not squeeze below, so asking
    // for the OPEN band held the bottom half at 501 px with the log closed. The handle alone does the work
    // (`rest_position(total, band)` puts it at `total − band`).
    let total = divider.height();
    if total <= 1 {
        return;
    }
    // THE ASK CANNOT EXCEED THE PANED'S OWN ROOM: `set_position` PANICS when its clamp range is inverted
    // (`min > max`), and headless the idle and `notify::width` handlers fire before allocation, when the
    // divider answers height 0.
    let band = super::window_shot::band_px(&run_row, &log).clamp(1, total - 1);
    divider.set_position(super::window_shot::rest_position(total, band));
    // The run-bar row now has a width, so the progress bar's ask is re-made from it (§1 badge 13).
    crate::ui::window_run_scroller::hand_the_free_width_to(&run_row);
}

/// The first widget named `name` at or below `node`, how [`rest_split`] finds the two widgets §1 counts.
pub(crate) fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    for child in node
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
    {
        if let Some(widget) = child
            .downcast_ref::<gtk::Widget>()
            .and_then(|w| find_in(w, name))
        {
            return Some(widget);
        }
    }
    None
}

/// Re-ask the resting split whenever the log's state changes: opening pushes the handle UP, closing hands
/// the height back.
fn connect_band_rest(divider: &gtk::Paned, run_row: &gtk::Widget, log: &gtk::Expander) {
    divider.connect_realize({
        let (d, run_row, log) = (divider.clone(), run_row.clone(), log.clone());
        move |_d| {
            // Asking the paned for its own height is the only way to know what room the band has once mapped.
            let total = d.height().max(1);
            d.set_position(super::window_shot::rest_position(
                total,
                crate::ui::window_shot::height_ask(&run_row, &log),
            ));
        }
    });
    log.connect_expanded_notify({
        let (d, run_row) = (divider.clone(), run_row.clone());
        move |l| {
            let total = d.height().max(1);
            // Opening the log is the ONE moment the band is taller than its collapsed rows: §1 gives the expander "room of its own".
            let band = if l.is_expanded() {
                collapse_without_room(&d);
                super::window_shot::band_px(&run_row, l)
            } else {
                crate::ui::window_shot::height_ask(&run_row, l)
            };
            d.set_position(super::window_shot::rest_position(total, band));
        }
    });
}

/// The bottom half: the run bar's row (which carries the readouts) and under it the log expander.
fn bottom_of(run_row: &gtk::Widget, log: &gtk::Widget) -> gtk::Box {
    // 03-shell §1: 8 px between the run row and the log header, 8 px under the divider, 12 px from the window's other edges.
    let bottom = gtk::Box::new(gtk::Orientation::Vertical, crate::ui::margins::BAR_GAP_PX);
    bottom.set_widget_name("bottom-bar");
    bottom.set_margin_top(crate::ui::margins::BAR_TOP_PX);
    bottom.set_margin_bottom(crate::ui::margins::BAR_EDGE_PX);
    bottom.set_margin_start(crate::ui::margins::BAR_EDGE_PX);
    bottom.set_margin_end(crate::ui::margins::BAR_EDGE_PX);
    bottom.append(run_row);
    bottom.append(log);
    bottom
}
