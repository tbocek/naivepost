//! §05-cut#1-screen — the Cut page's own order, assembled in one place.
//!
//! `spec/inventory/cut.md` §A: "Page = vertical paned (380): top = horizontal paned (660) of preview
//! (GtkPicture contain-fit, min height 160, click toggles play) and the form column; bottom =
//! toolbar, tracks, scrollbar." §1 lists the same top-to-bottom: **1** preview with **2**-**4** (the
//! reading column) beside it, then the toolbar (**5**-**22**), then the tracks (**23**-**30**), then
//! **31** scrollbar, then **32** the status line.
//!
//! The widgets were built in the order the flows arrived, so the page box held them in that order and
//! the shot showed the tape at the top, one small plate of preview under it, ~750 px of empty box,
//! and the separate recordings' lanes stranded at the bottom. This module fixes the ORDER only: it
//! re-parents the widgets `window.rs` already built and wired — nothing is rebuilt, so every name a
//! test finds its widget by, and every handler attached to it, keeps working. `window.rs` is past its
//! size budget, which is why the arrangement lives here and is called once.
//!
//! DECISION (spec silent on the split): the horizontal paned's start child is the preview panel and
//! its end child is a column holding the idle readings plus every form holder, because §A describes
//! exactly those two halves and §1's readings sit to the right of the picture in `img/05-cut.png`.
//! The scroll position, zoom and every wire are untouched: a re-parent moves a widget, it does not
//! reset it.

use gtk4 as gtk;
use gtk::prelude::*;

/// The order §A/§1 give the page, below the `Cut` title row and the paned: toolbar, band, strip,
/// camera rows, fold-all badge, then the scrollbar — the scrollbar LAST so it is the
/// thing above the run bar and the status line, as badge **31** sits above badge **32**.
/// The effects lane is NOT here: §06-effects#2 puts it directly beneath the preview frame, so it
/// travels with the picture in [`PREVIEW_STACK`] instead.
const BELOW_PANED: [&str; 6] = [
    "cut-toolbar",
    "track-layers",
    "track-strip",
    "selection-row",
    "camera-rows",
    "fold-all-holder",
];

/// The two widgets that make up the paned's start half, in order: the preview panel and, immediately
/// under it, the effects lane (§06-effects#2, `spec/img/06-lane.png`). They stay siblings because a
/// whole set of readers -- and §06's own seat test -- find the lane as the frame's next sibling.
const PREVIEW_STACK: [&str; 2] = ["preview-panel", "effects-lane"];
/// The form holders beside the readings: one `‹thing›-form-‹tab›` per thing being placed or edited.
/// Tab-scoped by their builder, so the Cut page's own suffix is appended here (PAGES's Cut label).
fn form_names(page: &str) -> Vec<String> {
    ["zoom", "speed", "text", "svg", "volume", "label"]
        .into_iter()
        .map(|kind| format!("{kind}-form-{page}"))
        // The insert form's holder is the one that is NOT tab-scoped (window.rs names it plain
        // `insert-form`), so it is spelled without the suffix the others carry.
        .chain(std::iter::once("insert-form".to_string()))
        .collect()
}

/// The width §A's horizontal paned gives the preview at rest. `spec/img/05-cut.png` shows the picture
/// at roughly half the page; the preview panel's own minimum is `PREVIEW_PANEL_W` (240) and the
/// The preview's share of the top row: §A's "paned (660)" puts the divider near half a 1440 px window,
/// which is also where `spec/img/05-cut.png` draws it.
const SELF_SPLIT_PREVIEW_PX: i32 = 700;
/// The scroller's own height band: the picture's own 160 px minimum is the floor, and a form body
/// longer than the cap scrolls inside the frame instead of pushing the tape down the page.
const IDLE_ROWS_MIN_HEIGHT: i32 = 200;
/// The width §1's reading column asks for before the preview starts losing pixels to it.
pub const FORM_COLUMN_MIN_W: i32 = 300;
/// … and the height band the scroller between the pinned heading and footer is sized to. Left uncapped
/// it asked for the whole page's height, which is what pushed the tracks ~750 px below the preview.
const IDLE_ROWS_CAP_H: i32 = 460;

/// Re-order the Cut page's children into §A's shape. Called once, at the end of `window.rs`'s Cut
/// arm, after every part has been built. A name that is absent (a page built without that part) is
/// skipped rather than fatal: the arrangement is a layout, not a contract on which flows have landed.
pub fn arrange(page_box: &gtk::Box) {
    let title_row = match page_box.first_child() {
        Some(first) => first,
        None => return,
    };
    // Detach everything this arrangement places, so re-inserting cannot leave a widget in two seats.
    // `gtk::Box::remove` only works on a direct child, and everything named here IS one.
    let page = crate::shell::Page::Cut.label();
    let forms = form_names(page);
    let mut wanted: Vec<String> = vec![
        "cut-form-frame".to_string(),
        "track-scrollbar".to_string(),
    ];
    wanted.extend(PREVIEW_STACK.iter().map(|name| name.to_string()));
    wanted.extend(forms.iter().cloned());
    wanted.extend(BELOW_PANED.iter().map(|name| name.to_string()));
    let detached = detach(page_box, &wanted);
    // `fold-all-holder` is appended AFTER this call runs (window.rs adds it to the page box after the
    // Cut arm closes), so it cannot come out of `detach` with the rest. Pick it up where it sits.
    let fold_all = page_box
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .find(|child| child.widget_name() == "fold-all-holder");
    let mut detached = detached;
    if let Some(holder) = &fold_all {
        page_box.remove(holder);
        detached.push(holder.clone());
    }

    // --- the top half: preview | form column, in one horizontal paned (A's "paned (660)") -------
    let preview = detached.iter().find(|w| w.widget_name() == PREVIEW_STACK[0]).cloned();
    let lane = detached.iter().find(|w| w.widget_name() == PREVIEW_STACK[1]).cloned();
    // §A's form column is a pinned heading, a scroller and a pinned footer; its scroller vexpands by
    // design, and inside a paned that asks for the whole page's height, which is what pushed the
    // tracks ~750 px below the preview. Both halves of the top row keep their own shape instead.
    let form_frame = detached.iter().find(|w| w.widget_name() == "cut-form-frame").cloned();
    if let Some(frame) = &form_frame {
        // §A's form column is a PINNED heading, a scroller and a PINNED footer (§05#1's note on the
        // pin), and only the scroller may stretch: left as built, the frame vexpanded and asked for the
        // whole page's height, which is what pushed the tracks ~750 px below the preview.
        frame.set_vexpand(false);
        // The column's own width is bounded only by where the divider sits: the Cut page box is as wide as
        // the TAPE stacked under the paned (§A badge 31 scrolls that width rather than clipping it), so no
        // per-widget ceiling here can hold the column narrow -- the pane position is what seats it.
        frame.set_hexpand(false);
        // The frame states only the floor it must never go under; the height band lives on the scroller.
        frame.set_size_request(FORM_COLUMN_MIN_W, -1);
        if let Some(scroller) = frame
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .filter_map(|child| child.downcast::<gtk::ScrolledWindow>().ok())
            .find(|child| child.widget_name() == "cut-form-scroller")
        {
            // §A pins the heading and the footer, so the scroller between them takes the rest of the
            // column's height: capped here so a long form scrolls instead of pushing the tape down.
            // `Automatic` horizontally lets it scroll a body wider than the column, which is what §A's
            // `External` policy was standing in for once the column sat beside the picture.
            scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
            scroller.set_vexpand(false);
            scroller.set_min_content_height(IDLE_ROWS_MIN_HEIGHT);
            scroller.set_max_content_height(IDLE_ROWS_CAP_H);
        }
        if let Some(grid) = frame
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .filter_map(|child| child.downcast::<gtk::Grid>().ok())
            .find(|grid| grid.widget_name() == "cut-form")
        {
            // The readings grid fills whatever box it is given; §1 draws them left-aligned in the column.
            grid.set_halign(gtk::Align::Start);
            grid.set_hexpand(false);
        }
    }
    // A form holder hides until its form opens; packed into a column with the idle readings they
    // stacked as two blank bands beside the preview (a hidden widget keeps its 4 px box spacing).
    // §A's seat is ONE column whose idle rows and form body share the scroller, so the holders stay
    // in the page box where their owner's door finds them and only the readings come into the paned.
    let mut form_widgets: Vec<gtk::Widget> = Vec::new();
    if let Some(readings) = form_frame {
        // §A's ONE column: "a scroller holding EITHER the idle rows OR a form body". The holders
        // (`insert-form`, `zoom-form-Cut`, …) were left as direct children of the page box, where their
        // owner's door found them but their width went to the PAGE instead of the column. They move under
        // the readings now; hidden ones paint nothing, so an idle page shows exactly its eight rows.
        for name in &forms {
            if let Some(holder) = detached.iter().find(|w| &w.widget_name() == name) {
                holder.set_hexpand(false);
                // A holder is found by its owner's door through a tree walk from the window, so it must
                // stay reachable from the page. Attaching it to the readings GRID does not: `Grid` keeps
                // children in its own private layout and `gtk_widget_get_first_child` then answers None,
                // which is why `insert-form` vanished from every lookup (`insert_form_box` -> None). It
                // goes into the frame -- the column §A means -- where a walk still finds it.
                if let Some(column) = readings.downcast_ref::<gtk::Box>() {
                    column.append(holder);
                }
            }
        }
        form_widgets.push(readings);
    }
    let forms = form_widgets;
    if preview.is_none() && forms.is_empty() {
        return;
    }
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    split.set_widget_name("cut-top");
    split.set_wide_handle(true);
    split.set_hexpand(true);
    // The top half takes the height its own content asks for and no more: GTK otherwise grows the
    // paned to fill the window and the tracks are pushed down the column behind a wall of plate.
    split.set_vexpand(false);
    if let Some(preview) = &preview {
        // §06-effects#2: the lane belongs UNDER the frame it marks, so the two go into the paned's start
        // half together as siblings of one vertical box -- the same pair `cut_preview_panel::install`
        // packed side by side in the page box. Splitting them (frame into the paned, lane left below)
        // broke that rule and every reader that walks from the frame to its next sibling.
        let stack: gtk::Widget = match &lane {
            Some(lane) => {
                let box_ = gtk::Box::new(gtk::Orientation::Vertical, 1);
                box_.set_widget_name("cut-preview-stack");
                box_.set_valign(gtk::Align::Start);
                box_.append(preview);
                box_.append(lane);
                box_.upcast()
            }
            // A page built without the lane keeps the plain fallback: the frame alone.
            None => preview.clone(),
        };
        split.set_start_child(Some(&stack));
        // §1's picture gives the preview the wider share of the top half.
        split.set_resize_start_child(true);
        // §A: the preview is a GtkPicture with min height 160; the panel itself is pinned to the
        // picture's own shape, so it does not stretch when the column beside it is taller.
        preview.set_valign(gtk::Align::Start);
        // `cut_preview_panel::install` pins the panel to `PREVIEW_PANEL_W` and starts it at the left;
        // in the paned's start half it must instead take every pixel that half gets, which is what makes
        // it the wide side (§1's badge 1 sits to the LEFT of the readings and dwarfs them).
        preview.set_hexpand(true);
        preview.set_halign(gtk::Align::Fill);
    } else if let Some(lane) = &lane {
        // Only the lane exists on this page: it still gets the start half rather than vanishing.
        split.set_start_child(Some(lane));
        split.set_resize_start_child(true);
    }
    if !forms.is_empty() {
        // The divider sits at the picture's width (§A's "paned (660)"). Resize on the END child with
        // shrink off makes GTK honour the column's pinned width instead of letting it flow into the row.
        split.set_end_child(Some(&forms[0]));
        split.set_resize_end_child(true);
        split.set_shrink_end_child(false);
    }
    page_box.insert_child_after(&split, Some(&title_row));
    // The divider where §A puts it. ONE pair of rules for the start half, set here and nowhere else — a
    // second `set_resize_start_child(false)` further down silently cancelled the `true` and the picture
    // collapsed onto its own minimum (measured: 125 px wide).
    split.set_position(SELF_SPLIT_PREVIEW_PX);
    split.set_resize_start_child(true);
    split.set_shrink_start_child(false);

    // --- the bottom half: toolbar, tracks, scrollbar (A's "bottom = toolbar, tracks, scrollbar") --
    // §A's vertical paned gives the BOTTOM half the page's leftover height, so the tape and the lanes
    // it paints sit directly under the toolbar instead of being pushed down the column.
    let mut previous: gtk::Widget = split.upcast();
    for name in BELOW_PANED {
        if let Some(widget) = detached.iter().find(|w| w.widget_name() == name).cloned() {
            // §A's band is ONE surface spanning the page, so everything stacked into the bottom half
            // fills the width it is given. `track-strip` comes from `window.rs` sized to `STRIP_WIDTH`
            // (480) and `halign Start`, which is why it sat narrower than the band beside it; the strip
            // paints from `cut_view::view_px(...)` at draw time, so filling costs nothing it relied on.
            widget.set_hexpand(true);
            widget.set_halign(gtk::Align::Fill);
            page_box.insert_child_after(&widget, Some(&previous));
            previous = widget;
        }
    }
    if let Some(bar) = detached.iter().find(|w| w.widget_name() == "track-scrollbar").cloned() {
        page_box.insert_child_after(&bar, Some(&previous));
    }
}

/// Take every named widget out of `page_box` and hand it back, in the order asked for. Detaching (as
/// opposed to moving one at a time) is what keeps the walk simple: after this, `page_box` holds only
/// what this arrangement does not place, and the order is rebuilt from one list.
fn detach(page_box: &gtk::Box, names: &[String]) -> Vec<gtk::Widget> {
    let mut found: Vec<gtk::Widget> = Vec::new();
    let mut node = page_box.first_child();
    while let Some(child) = node {
        node = child.next_sibling();
        if names.iter().any(|name| &child.widget_name() == name)
            && child.parent().as_ref() == Some(&page_box.clone().upcast::<gtk::Widget>())
        {
            page_box.remove(&child);
            found.push(child);
        }
    }
    // Hand them back in the asked order so the caller can look a name up without caring where it was.
    let mut ordered: Vec<gtk::Widget> = Vec::with_capacity(found.len());
    for name in names {
        if let Some(widget) = found.iter().find(|w| &w.widget_name() == name) {
            ordered.push(widget.clone());
        }
    }
    ordered
}
