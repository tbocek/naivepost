//! §05-cut#1-screen — the Cut page's own order, assembled in one place.
//!
//! `spec/inventory/cut.md` §A: "Page = vertical paned (380): top = horizontal paned (660) of preview
//! §1 lists the same top-to-bottom: **1** preview with **2**-**4** (the
//! reading column) beside it, then the toolbar (**5**-**22**), then the tracks (**23**-**30**), then
//! **31** scrollbar, then **32** the status line. So there are TWO dividers, not one: `cut-page-split`
//! splits the page top/bottom and opens at [`SELF_SPLIT_TOP_PX`] (380 px of upper part), and inside its
//! start half `cut-top` splits left/right and opens at [`SELF_SPLIT_PREVIEW_PX`] (660 px of preview).
//! The bottom pane (`cut-bottom`) holds the toolbar, the tracks and the scrollbar in that order.
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

/// Two halves of the page ask for their own width and must not be stretched across the band: §A's form
/// column is seated by the left/right divider, and §1 puts the selection row's drag surface, ✕ and
/// readout together as one small surface under the bar, not as a page-wide strip.
const NO_FILL: [&str; 2] = ["cut-form-frame", "selection-row"];

/// The toolbar spans the page's width, so `arrange` lets it stretch with the rest of the bottom half.
/// §1 items **9**-**16** are the volume slider and the Copy/Paste/Insert/Lane controls, and
/// `spec/img/05-cut.png` draws them across the whole bar; when that box was `halign(Start)` the bar
/// stopped at the sum of its groups' natural widths and clipped them at its own edge (measured at a
/// 1260 px window: `group-verbs` was given 244 px of a 675 px natural width, so Copy/Paste/Insert/Lane sat
/// at x=769..1194 inside `group-verbs`' own scroller, off the visible bar).
const TOOLBAR_SPANS_PAGE: bool = true;

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

/// §A's opening width for the LEFT/RIGHT divider — "split left and right by a second draggable divider
/// (opened with the preview 660 px wide)", the `paned 660/380` pair of its parameter list. `pub` so
/// `params::cut()` can catalogue it where §10's list is checked against the code.
pub const SELF_SPLIT_PREVIEW_PX: i32 = 660;

/// §A's opening height for the TOP/BOTTOM divider — "split top and bottom by a draggable divider
/// (opened with the upper part 380 px tall)"; the other half of that `paned 660/380` pair.
/// §A's opening height for the TOP/BOTTOM divider — "split top and bottom by a draggable divider
/// (opened with the upper part 380 px tall)"; the other half of that `paned 660/380` pair.
pub const SELF_SPLIT_TOP_PX: i32 = 380;

/// The height the pane below the top/bottom divider asks for, so that divider can actually open at
/// [`SELF_SPLIT_TOP_PX`] in a window sized like `spec/img/05-cut.png`. §A gives no number for it; this is
/// §1's own parts added up -- the toolbar row, one picture row at the open-at thumbnail size with its wave
/// strip (`thumbHt + 4` then 30), and the scrollbar.
/// What the pane below the top/bottom divider would have asked for (§A gives no number; this was §1's parts
/// added up). UNUSED now — see [`UPPER_PANE_MIN_H`] for why neither pane carries a request any more.
#[allow(dead_code)]
pub const LOWER_PANE_MIN_H: i32 = 196;

/// What the pane ABOVE the top/bottom divider asks for, so the divider opens at [`SELF_SPLIT_TOP_PX`]
/// instead of settling where the preview's own minimum happens to land (measured: 304 px -- the picture's
/// 135 px pinned size plus an empty effects lane). §A pins only the preview's floor ("min height 160");
/// everything else here is §1's own arithmetic -- a 16/9 picture at that floor (284) and the ruler (18),
/// selection band (22) and one effect row (26) the lane costs live among -- so the opening is reached by
/// adding up what the page actually draws. Measured: the divider lands exactly on this number, so it is
/// `SELF_SPLIT_TOP_PX` plus the 30 px GTK keeps for itself (handle and frame), not a bigger picture.
/// What the pane ABOVE the top/bottom divider WOULD have asked for. UNUSED and kept as the record of why
/// the openings are positions, not requests: this 410 px plus [`LOWER_PANE_MIN_H`] sums past §1's minimum
/// page band (~700 px), which is how the Cut page ended up scrolling as a whole.
#[allow(dead_code)]
pub const UPPER_PANE_MIN_H: i32 = SELF_SPLIT_TOP_PX + 30;

/// The FLOOR the picture gets in the upper pane: §A's "GtkPicture contain-fit, min height 160". The panel
/// is built at `PREVIEW_PANEL_H` (135 = 240 x 9/16, its thumbnail shape) and stays there until the pane
/// offers more; then it grows at the footage's own shape rather than stretching.
pub const PREVIEW_FLOOR_H: f64 = 160.0;

/// How much of the upper pane the picture does NOT get: the ruler (§A's 18 px) and the selection band
/// (22 px) that the effects lane's rows live among, which sit under the picture in the reference. The
/// picture's share of a [`SELF_SPLIT_TOP_PX`] opening is the rest -- what `spec/img/05-cut.png` draws.
pub const BANDS_ABOVE_PICTURE_PX: i32 = 18 + 22;
/// The scroller's own height band: the picture's own 160 px minimum is the floor, and a form body
/// longer than the cap scrolls inside the frame instead of pushing the tape down the page.
const IDLE_ROWS_MIN_HEIGHT: i32 = 200;
/// The width §1's reading column asks for before the preview starts losing pixels to it.
pub const FORM_COLUMN_MIN_W: i32 = 300;
/// … and the height band the scroller between the pinned heading and footer is sized to. Left uncapped
/// it asked for the whole page's height, which is what pushed the tracks ~750 px below the preview. Measured
/// on `05-insert-form`: at 460 the form body filled the frame and its footer ("Insert it" + "Cancel") fell
/// below the painted window; 300 keeps heading, body and footer all on screen.
const IDLE_ROWS_CAP_H: i32 = 300;

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
            // §1's readings are "label left and value right", so the grid takes the whole pane: pinned to
            // `Start` at its natural width it had no right edge for the values to sit against, and each one
            // printed beside its own label instead of at the column's right end. The scroller inside this
            // frame scrolls horizontally on `Automatic`, so a wide grid costs a scroll, not a wider page.
            grid.set_halign(gtk::Align::Fill);
            grid.set_hexpand(true);
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
                // §1: "above the minimum the layout grows with the window and no part has a fixed pixel
                // size: the video preview scales to the room it is given". The stack therefore FILLS its
                // pane rather than sitting at its built height at the top of it; `grow_to` below keeps the
                // floors (§A's 160 px picture, the panel's own thumbnail shape) as floors.
                box_.set_vexpand(true);
                box_.set_valign(gtk::Align::Fill);
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
        // §1 again: the picture takes the room its pane is given, so it fills vertically (the toolkit keeps
        // the footage's aspect inside it — `cut_picture::build` sets `ContentFit::Contain`). Only the LAYOUT
        // grows here: the camera/drag fractions are still measured against the panel's declared pin through
        // `cut_zoom_hand::preview_picture_px`, whose own comment says why an allocation cannot be the
        // measuring stick (it differs between an isolated and a loaded run).
        preview.set_vexpand(true);
        preview.set_valign(gtk::Align::Fill);
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
    // --- the TOP/BOTTOM divider (§A's "vertical paned (380)") --------------------------------------
    // The two halves are ONE paned, not a stack: §1 says the page is "split top and bottom by a
    // draggable divider (opened with the upper part 380 px tall)", and below it the toolbar, the tracks
    // and the scrollbar. Before this existed the bottom widgets were plain children of the page box, so
    // there was no divider to drag and the top half grew or shrank with the window instead of its pane.
    let vsplit = gtk::Paned::new(gtk::Orientation::Vertical);
    vsplit.set_widget_name("cut-page-split");
    vsplit.set_wide_handle(true);
    vsplit.set_vexpand(true);
    vsplit.set_start_child(Some(&split));
    // The top half keeps its own shape (the picture's height plus the lane under it) and never shrinks
    // below it; the bottom half takes what is left, which is §A's note on the vertical paned.
    vsplit.set_resize_start_child(true);
    // Both panes may shrink: see the note where the openings are settled by POSITION instead of by
    // requests, so the two together still fit §1's minimum-height page band.
    vsplit.set_shrink_start_child(false);
    // `cut-top` asks for less than §A's 380 px on its own (the picture plus the lane came to ~304 px
    // measured), so BOTH panes must be allowed to grow into the leftover, or GTK settles the divider at
    // the start child's natural height and 380 is never reached. With these the position holds.
    split.set_vexpand(true);
    vsplit.set_position(SELF_SPLIT_TOP_PX);
    // ...and the upper pane used to ask for more than its own content (the old `UPPER_PANE_MIN_H`, 410 px) so
    // GTK would settle the divider on §A's opening rather than on the picture's 304 px natural height. That
    // request is GONE: at §1's minimum the page band cannot hold 410 + 196, and a page that overflows its band
    // scrolls as a whole, which §1 forbids. The position above plus vexpand on both panes is what opens it now.
    let bottom = gtk::Box::new(gtk::Orientation::Vertical, 4);
    bottom.set_widget_name("cut-bottom");
    bottom.set_vexpand(true);
    bottom.set_valign(gtk::Align::Fill);
    vsplit.set_end_child(Some(&bottom));
    page_box.insert_child_after(&vsplit, Some(&title_row));
    // The divider where §A puts it. ONE pair of rules for the start half, set here and nowhere else — a
    // second `set_resize_start_child(false)` further down silently cancelled the `true` and the picture
    // collapsed onto its own minimum (measured: 125 px wide).
    split.set_position(SELF_SPLIT_PREVIEW_PX);
    split.set_resize_start_child(true);
    split.set_shrink_start_child(false);

    // --- the bottom half: toolbar, tracks, scrollbar (A's "bottom = toolbar, tracks, scrollbar") --
    // §A's vertical paned gives the BOTTOM half the page's leftover height, so the tape and the lanes
    // it paints sit directly under the toolbar instead of being pushed down the column.
    for name in BELOW_PANED {
        if let Some(widget) = detached.iter().find(|w| w.widget_name() == name).cloned() {
            // §A's band is ONE surface spanning the page, so everything stacked into the bottom half
            // fills the width it is given -- except [`NO_FILL`], which keeps its own shape. `track-strip`
            // comes from `window.rs` sized to `STRIP_WIDTH`
            // (480) and `halign Start`, which is why it sat narrower than the band beside it; the strip
            // paints from `cut_view::view_px(...)` at draw time, so filling costs nothing it relied on.
            widget.set_hexpand(TOOLBAR_SPANS_PAGE || !NO_FILL.contains(&name));
            widget.set_halign(gtk::Align::Fill);
            bottom.append(&widget);
        }
    }
    if let Some(bar) = detached.iter().find(|w| w.widget_name() == "track-scrollbar").cloned() {
        bottom.append(&bar);
    }
    // The lower pane used to carry a hard request ([`LOWER_PANE_MIN_H`]) so GTK would settle the divider on
    // §A's opening instead of on the two panes' natural heights. That request is GONE: §1 fixes the window's
    // MINIMUM at 1240×900 and says no page scrolls as a whole at that size, so the page band (~700 px after
    // header, run bar and log) has to hold BOTH panes, and the old 410 + 196 pair summed past it — measured
    // 101 px of travel in `page-scroll-Cut`. The openings are positions + vexpand now, so the floor stays at
    // §1's number and `tests/suite/window_shrinks.rs` measures zero travel here.
    bottom.set_vexpand(true);
    vsplit.set_shrink_start_child(true);
    vsplit.set_shrink_end_child(true);
    // Now the upper pane has real height (the window minus the divider's opening), hand it to the picture:
    // §A's preview is "contain-fit, min height 160", and the reference draws it filling the whole upper
    // pane, while the panel was built at its thumbnail shape (`PREVIEW_PANEL_H`, 135) and would otherwise
    // sit as a small plate under a wall of empty plate. Ask through the panel's own door so the picture,
    // the camera rect and every overlay are measured against the same box.
    if let Some(panel) = preview.as_ref() {
        // The lane under the picture costs its rows too (an empty one is still `fx_lane::lane_height_px(1)`
        // deep, which is what `sync_lane_rows` asks for), so the picture's share is the pane minus that.
        let lane_h = crate::fx_lane::lane_height_px(1) as i32;
        crate::ui::cut_preview_panel::grow_to(
            panel
                .downcast_ref::<gtk::Widget>()
                .expect("the preview panel is a widget"),
            SELF_SPLIT_PREVIEW_PX as f64,
            (SELF_SPLIT_TOP_PX - BANDS_ABOVE_PICTURE_PX - lane_h) as f64,
        );
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
