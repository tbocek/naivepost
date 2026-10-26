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

use adw::prelude::*;
use gtk4 as gtk;

/// The order §A/§1 give the page, below the `Cut` title row and the paned: toolbar, band, strip,
/// camera rows, fold-all badge, then the scrollbar — the scrollbar LAST so it is the
/// thing above the run bar and the status line, as badge **31** sits above badge **32**.
/// The effects lane is NOT here: §06-effects#2 puts it directly beneath the preview frame, so it
/// travels with the picture in [`PREVIEW_STACK`] instead.
const BELOW_PANED: [&str; 5] = [
    "cut-toolbar",
    "track-layers",
    "selection-row",
    "camera-rows",
    "fold-all-holder",
];

/// The rows below the tape whose controls the band now draws and answers itself: kept in the tree, not shown.
const HELD_UNSEEN: [&str; 3] = ["selection-row", "camera-rows", "fold-all-holder"];

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

/// Bottom pane's FLOOR, in pixels: what the tape costs at its idle height plus the toolbar and the scrollbar
/// above and below it. Set as `cut-bottom`'s own minimum height so §A's 380 px opening can never be paid for
/// by starving the tape: a `Paned` will not place its handle past its end child's minimum, so with this floor
/// GTK moves the handle to `pane − floor` when the window is too short for both, and what gives is the picture
/// above the divider, which has room to give. Before this, the floor was whatever the widgets happened to
/// measure, so the divider sat wherever the two panes' natural sizes put it and the wave rows fell below the
/// allocation the band was clipped to (measured: `cut-bottom` given 274 px of a 634 px pane at a 1440×900
/// window, while the band itself asked 126 px and got 132 px).
pub const CUT_BOTTOM_FLOOR_PX: i32 = 196;

/// The tallest the picture may be ASKED for once a pane has grown past §A's opening: an ask above
/// [`SELF_SPLIT_TOP_PX`] becomes the paned's start-pane minimum, the handle clamps to it, and the pane
/// overflows the divider — everything above the new handle position is clipped out of the render, §1's
/// first idle reading with it. One rule for the page and for anything that re-asks the panel later (the
/// snapshot does, at its final size), so a re-ask can never undo the opening.
pub fn preview_ask_ceiling(pane_h: i32, lane_h: i32) -> i32 {
    (pane_h - lane_h).min(SELF_SPLIT_TOP_PX - lane_h).max(1)
}

/// What a wide `gtk::Paned` keeps for its own handle and frame, measured on the built page: a 634 px paned
/// holding a 380 px start pane and a 250 px end pane leaves 4 px over. Named here because it is this page's
/// divider that has to be paid for, not a GTK-wide constant.
const PANED_HANDLE_GAP_PX: i32 = 4;

/// What the bottom pane costs BESIDES the tape, asked rather than remembered: the toolbar's own minimum
/// height, the scrollbar's (an unseen one is `SCROLLBAR_ASK_PX` thick), and the two gaps `cut-bottom` keeps
/// between the three (§A: "bottom = toolbar, tracks, scrollbar") — but never less than
/// [`CUT_BOTTOM_FLOOR_PX`] in total, so a page whose tape is short still leaves the divider where §1's minimum
/// window can hold all three.
fn cut_bottom_floor_px(band_ask: i32) -> i32 {
    band_ask
        + (cut_toolbar_min_px() + CUT_TRACK_GAP_PX * 2 + scrollbar_ask_px())
            .max(CUT_BOTTOM_FLOOR_PX - band_ask)
}

/// The toolbar's height as the page built it — asked of the real widget when `arrange` has it in hand, and
/// of its built minimum otherwise. Measured: 34 px.
fn cut_toolbar_min_px() -> i32 {
    TOOLBAR_ASK_PX
}

const TOOLBAR_ASK_PX: i32 = 34;
/// The gap `cut-bottom` keeps between the three things it stacks.
const CUT_TRACK_GAP_PX: i32 = 4;
/// How thick the scrollbar is when it has no allocation to be asked about (an unmapped bar answers 0 px,
/// which is not what it will cost once the tape is wider than the view and §A's badge 31 appears).
const SCROLLBAR_ASK_PX: i32 = 14;

fn scrollbar_ask_px() -> i32 {
    SCROLLBAR_ASK_PX
}

/// The scroller's FLOOR: what the eight idle readings cost, asked of the real `cut-form` grid rather
/// than remembered. A form body longer than this still scrolls (the cap below), but §1's readings never
/// do: at 200 px the viewport was shorter than the eight rows (~250 px), so the column opened on its TAIL
/// and "Thumbnails:" — badge **2**, the first reading — was painted above the visible edge even though
/// every door that asks for the top had run.
pub const IDLE_ROWS_MIN_HEIGHT: i32 = ROW_PITCH_PX * IDLE_ROW_COUNT as i32;
/// One reading row's pitch: a label line plus the 2 px the grid keeps between rows (`cut_form_column`).
const ROW_PITCH_PX: i32 = 29;
/// How many readings §1 lists (**4** playhead, selection, cut, cut at 1×, source, segments, plus the
/// thumbnail size and the aspect) — spelled once in `cut_screen::IDLE_FORM_ROWS`, which is also checked
/// against this by `tests/suite/sec_05_cut_1_screen_layout.rs`.
pub const IDLE_ROW_COUNT: usize = 8;
/// The width §1's reading column asks for before the preview starts losing pixels to it.
pub const FORM_COLUMN_MIN_W: i32 = 300;
/// The height that column needs to show its eight idle readings whole (§1: "idle readings, one per line").
/// Measured at the minimum window the pane came out ~65 px against the ~230 px the grid wants, so the
/// scroller scrolled and the render showed only the last four rows (three clipped pills above them). The
/// floor makes the pane ask for its readings instead of clipping them; what gives is the divider position,
/// which §1 already allows ("above the minimum the layout grows with the window").
pub const FORM_COLUMN_MIN_H: i32 = IDLE_ROWS_MIN_HEIGHT + COLUMN_FRAME_PX;
/// What the column costs around its readings: the pinned heading and footer (§A pins both, hidden while
/// idle but still box-spaced) and the scroller's own frame.
const COLUMN_FRAME_PX: i32 = 40;
/// … and the height band the scroller between the pinned heading and footer is sized to. Left uncapped
/// it asked for the whole page's height, which is what pushed the tracks ~750 px below the preview. Measured
/// on `05-insert-form`: at 460 the form body filled the frame and its footer ("Insert it" + "Cancel") fell
/// below the painted window; 300 keeps heading, body and footer all on screen.
const IDLE_ROWS_CAP_H: i32 = 300;

/// Re-order the Cut page's children into §A's shape. Called once, at the end of `window.rs`'s Cut
/// arm, after every part has been built. A name that is absent (a page built without that part) is
/// skipped rather than fatal: the arrangement is a layout, not a contract on which flows have landed.
pub fn arrange(page_box: &gtk::Box, window: &adw::ApplicationWindow) {
    arrange_priced(
        page_box,
        crate::ui::cut_tracks::idle_band_height(),
        Some(window),
    );
}

/// `arrange`, with the tape's height ask supplied. Split out because §1's band costs a row per camera AND a
/// row per cut lane, so a caller that has already put a cut on the page (or a test that seeds one) must charge
/// the bottom pane for the rows THAT scene paints, not for the idle one `arrange` can see.
pub fn arrange_priced(page_box: &gtk::Box, band_ask: i32, window: Option<&adw::ApplicationWindow>) {
    // The toolbar and the scrollbar around the tape, plus the box's own row spacing: the band's ask is only
    // one of the three things the bottom pane stacks (§A: "bottom = toolbar, tracks, scrollbar").
    let pane_floor_px = cut_bottom_floor_px(band_ask);
    let title_row = match page_box.first_child() {
        Some(first) => first,
        None => return,
    };
    // Detach everything this arrangement places, so re-inserting cannot leave a widget in two seats.
    // `gtk::Box::remove` only works on a direct child, and everything named here IS one.
    let page = crate::shell::Page::Cut.label();
    let forms = form_names(page);
    let mut wanted: Vec<String> = vec!["cut-form-frame".to_string(), "track-scrollbar".to_string()];
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
    let preview = detached
        .iter()
        .find(|w| w.widget_name() == PREVIEW_STACK[0])
        .cloned();
    let lane = detached
        .iter()
        .find(|w| w.widget_name() == PREVIEW_STACK[1])
        .cloned();
    // §A's form column is a pinned heading, a scroller and a pinned footer; its scroller vexpands by
    // design, and inside a paned that asks for the whole page's height, which is what pushed the
    // tracks ~750 px below the preview. Both halves of the top row keep their own shape instead.
    let form_frame = detached
        .iter()
        .find(|w| w.widget_name() == "cut-form-frame")
        .cloned();
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
        frame.set_size_request(FORM_COLUMN_MIN_W, FORM_COLUMN_MIN_H);
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
            // Ask the grid itself what its readings cost and use the larger of that and the row-count
            // floor: a taller ladder or a wrapped label raises the ask instead of scrolling rows away.
            // The floor and the ask are ONE number from here on: the frame's request below and the
            // scroller's min/max both come off `rows_h`, so the viewport is never shorter than its body.
            let rows_h = IDLE_ROWS_MIN_HEIGHT.max(idle_rows_height(frame));
            scroller.set_min_content_height(rows_h);
            scroller.set_max_content_height(IDLE_ROWS_CAP_H.max(rows_h));
            // The ASK goes on the scroller alone, not on the frame: §1 forbids the page from scrolling as a
            // whole at its minimum (`tests/suite/window_shrinks.rs` measures zero travel on every tab), and
            // a request on the frame is charged to the page box, which pushed `page-scroll-Cut` past its
            // band (measured: 826 px of ask against a 790 px band). The frame keeps its own floor; the paned
            // gives this pane what it can.
            // …and the column must FIT the pane it is seated in. §A opens the upper part 380 px tall; left to
            // its own asks the frame measured 417 px, so GTK overflowed it past the divider and the first
            // reading — badge **2**, "Thumbnails:" with its −/+ ladder — landed under the header bar's edge,
            // present in the tree and painted where nobody can see it. Capping the height here (not after the
            // map: a headless window is allocated exactly once, and every ask made later never reaches the
            // render node `write_png` draws) keeps the whole column inside §A's opening.
            frame.set_valign(gtk::Align::Start);
            frame.set_size_request(
                FORM_COLUMN_MIN_W,
                (rows_h + COLUMN_FRAME_PX).min(SELF_SPLIT_TOP_PX),
            );
            // The eight rows are shown WHOLE rather than scrolled into that cap: the scroller takes its
            // body's natural height up to the cap instead of answering its minimum and clipping the grid.
            scroller.set_propagate_natural_height(true);
            // A scroller keeps whatever value its adjustment last held, so the page asks for the TOP of the
            // readings here, at the seat §1 describes — the page painting its idle rows. `frame_parts` also
            // asks this whenever a body is placed in the frame.
            if let Some(window) = window {
                crate::ui::cut_form_frame::scroll_to_top(window);
            }
            // …and once more at the first real allocation. The ask above lands while the viewport is still
            // 0 px tall, and headless a widget allocated once fires no `notify::height` (see
            // [`rest_opening_once`]), so a scroll position that anything else parked elsewhere — a focus
            // moving to the dropdown, a later resize — is never corrected. One bounded idle, no sleep.
            rest_scroll_once(&scroller);
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
        // They go INSIDE the scroller: appended to the column under it, an open zoom form made the column
        // 554 px tall in a 380 px pane, and the paned kept the column's bottom edge, so the pinned heading
        // and its ✕ were painted above the window.
        let body = find_named_box(&readings, crate::ui::cut_form_frame::BODY_NAME);
        for name in &forms {
            if let Some(holder) = detached.iter().find(|w| &w.widget_name() == name) {
                holder.set_hexpand(false);
                // A holder is found by its owner's door through a tree walk from the window, so it must
                // stay reachable from the page. Attaching it to the readings GRID does not: `Grid` keeps
                // children in its own private layout and `gtk_widget_get_first_child` then answers None,
                // which is why `insert-form` vanished from every lookup (`insert_form_box` -> None). It
                // goes into the frame -- the column §A means -- where a walk still finds it.
                if let Some(body) = &body {
                    body.append(holder);
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
    // The page's band cannot hold §A's 380 px opening AND the whole tape at §1's minimum window, and a
    // `gtk::Paned` clamps its position into `[start minimum, own height − end minimum]` — so the opening
    // needs an ask to be paid for out of, stated here rather than left to the two panes' natural sizes.
    // §1 forbids the PAGE from scrolling at its minimum (`window_shrinks.rs` measures zero travel), so the
    // money comes out of the bottom pane's share: its floor is the tape's own ask plus the toolbar and the
    // bar around it, and when the window is shorter than the sum, GTK moves the handle down and the
    // PICTURE gives the pixels, which is what §A's "above the minimum the layout grows with the window"
    // already says may give. Measured at 1240×900 with no ask: handle at 380, pane 416 px, the bottom half
    // 32 px too short, and GTK clipped the wave rows off the bottom of the band's `DrawingArea`.
    vsplit.set_size_request(-1, SELF_SPLIT_TOP_PX + pane_floor_px + PANED_HANDLE_GAP_PX);
    // ...and the upper pane used to ask for more than its own content (the old `UPPER_PANE_MIN_H`, 410 px) so
    // GTK would settle the divider on §A's opening rather than on the picture's 304 px natural height. That
    // request is GONE: at §1's minimum the page band cannot hold 410 + 196, and a page that overflows its band
    // scrolls as a whole, which §1 forbids. The position above plus vexpand on both panes is what opens it now.
    let bottom = gtk::Box::new(gtk::Orientation::Vertical, 4);
    bottom.set_widget_name("cut-bottom");
    // NOT `vexpand`: §1 opens this pane at the page's leftover (the divider sits at 380 px), and a Paned
    // hands its leftover to an expanding child by SHARE — with both panes expanding GTK settled the handle on
    // the two halves' own asks instead of on the position above (measured: 29 px of upper pane against the
    // 380 px §A opens it at, so every control below the divider was built but painted nothing). The band
    // still fills what its pane gives it, which is what `valign(Fill)` below does on its own.
    bottom.set_valign(gtk::Align::Fill);
    // §A's tape is the reason this pane has a floor at all: without a minimum the paned happily places §A's
    // 380 px opening on top of a bottom half too short for ruler + lane + picture row + wave strip, and GTK
    // clips a `DrawingArea` to its allocation, so the missing pixels come off the WAVE ROWS at the bottom of
    // the tape (measured: band 132 px against a 126 px ask, the wave strip drawn but outside the surface).
    // The floor is the band's own arithmetic plus the toolbar and the bar around it, asked from `cut_tracks`
    // so a row added by an edit raises it instead of overflowing.
    bottom.set_size_request(-1, pane_floor_px);

    vsplit.set_end_child(Some(&bottom));
    page_box.insert_child_after(&vsplit, Some(&title_row));
    // The divider where §A puts it. ONE pair of rules for the start half, set here and nowhere else — a
    // second `set_resize_start_child(false)` further down silently cancelled the `true` and the picture
    // collapsed onto its own minimum (measured: 125 px wide).
    split.set_position(SELF_SPLIT_PREVIEW_PX);
    split.set_resize_start_child(true);
    split.set_shrink_start_child(false);
    // §A's 380 px opening is an OPENING, not a drag a person has made, and headless nothing drags it: the ask
    // above was clamped because the paned had no height yet, so the same rule is re-asked at the first real
    // allocation and at every later one. In a window on a display this is a no-op after the first pass.
    rest_opening_once(&vsplit);
    // …and once more AFTER the band has been grown to its scene. `cut_tracks::fit_band_height` raises the
    // tape's own request when the cut needs rows (§1: a row per camera AND a row per lane), and GTK honours a
    // child's new minimum by pushing the handle toward the START pane — measured at 1240×900 with only
    // [`rest_opening_once`] wired: handle at 380, bottom pane 287 px of which the band took 132 px, leaving
    // the selection row (109 px) plus the scrollbar 46 px short, and GTK clipped the bottom of the tape.
    // Re-asking takes the position back to §A's number at whatever size the window actually reached.
    rest_on_resize(&vsplit);

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
            // §1's bottom half is "the toolbar, the tracks, the scrollbar" and nothing else: the selection's ✕, the
            // camera rows' plates, crosses, speakers and lenses, and the fold-all badge are marks the band draws and
            // answers (`cut_band_marks`). Their old widgets stay in the tree, unseen, for the doors that find them.
            widget.set_visible(!HELD_UNSEEN.contains(&name));
            bottom.append(&widget);
        }
    }
    if let Some(bar) = detached
        .iter()
        .find(|w| w.widget_name() == "track-scrollbar")
        .cloned()
    {
        bottom.append(&bar);
    }
    // The lower pane used to carry a hard request ([`LOWER_PANE_MIN_H`]) so GTK would settle the divider on
    // §A's opening instead of on the two panes' natural heights. That request is GONE: §1 fixes the window's
    // MINIMUM at 1240×900 and says no page scrolls as a whole at that size, so the page band (~700 px after
    // header, run bar and log) has to hold BOTH panes, and the old 410 + 196 pair summed past it — measured
    // 101 px of travel in `page-scroll-Cut`. The openings are positions + vexpand now, so the floor stays at
    // §1's number and `tests/suite/window_shrinks.rs` measures zero travel here.
    // Only the UPPER pane expands (§A's "above the minimum the layout grows with the window": the extra height
    // belongs to the picture). With both expanding the paned split its leftover between them and ignored the
    // position entirely — see [`SELF_SPLIT_TOP_PX`]'s opening note and the `cut-bottom` valign above.
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

/// What the eight idle readings actually cost: the `cut-form` grid's own vertical minimum, asked of the
/// widget rather than remembered. 0 when this page has no readings (a page built without the form column).
///
/// The grid is found by DESCENT, not among the frame's direct children: `cut_form_frame::frame_parts`
/// puts the body INSIDE `cut-form-scroller`, so a one-level scan answered `None` here, this returned 0,
/// and every ask below fell back to [`IDLE_ROWS_MIN_HEIGHT`] (8 × 29 = 232 px) — less than the grid
/// really costs, because its Aspect ratio row carries a `gtk::DropDown` taller than a label line. The
/// viewport was then shorter than its body and §1's first reading, "Thumbnails:" (badge **2**), was
/// painted above the visible edge.
fn idle_rows_height(frame: &gtk::Widget) -> i32 {
    find_grid(frame)
        .map(|grid| grid.measure(gtk::Orientation::Vertical, -1).0)
        .unwrap_or(0)
}

/// Depth-first search for a named box under `node`.
fn find_named_box(node: &gtk::Widget, name: &str) -> Option<gtk::Box> {
    if node.widget_name() == name {
        return node.downcast_ref::<gtk::Box>().cloned();
    }
    node.observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .find_map(|child| find_named_box(&child, name))
}

/// Depth-first search for the readings grid, the same walk `cut_form_frame` uses to find its own parts.
fn find_grid(node: &gtk::Widget) -> Option<gtk::Grid> {
    if node.widget_name() == "cut-form" {
        return node.downcast_ref::<gtk::Grid>().cloned();
    }
    for child in node.observe_children().iter::<glib::Object>().flatten() {
        if let Some(grid) = find_grid(child.downcast_ref::<gtk::Widget>()?) {
            return Some(grid);
        }
    }
    None
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

/// Puts the form column at the TOP of its readings once the scroller has a size: §1's "the idle column
/// starts with the readings" has to hold at the size the page is actually painted at.
pub fn rest_scroll_once(scroller: &gtk::ScrolledWindow) {
    crate::ui::settle::once_sized(
        "the form column at the top of its readings",
        scroller,
        |scroller| {
            crate::ui::cut_form_frame::scroll_to_top_of(scroller);
        },
    );
}

/// Asks §A's opening of the Cut page's own divider once the paned has a size. A `gtk::Paned` clamps every
/// `set_position` into `[start minimum, total − end minimum]`, and before its first real allocation it
/// answers 1 px tall, so an ask made earlier lands on the clamp.
pub fn rest_opening_once(divider: &gtk::Paned) {
    crate::ui::settle::once_sized("the Cut page's split at its opening", divider, |divider| {
        divider.set_position(SELF_SPLIT_TOP_PX);
    });
}

/// Re-ask §A's opening whenever this page's divider is given a new height — the Cut page's own copy of the
/// rule `window_divider::install` runs for the shell's divider, which is why the shell's shot rests right and
/// this page's did not. A `set_position` made while the paned is unallocated lands on its clamp (measured: 29 px
/// of upper pane instead of §A's 380), and GTK 4 fires no allocate signal, so the two `notify` watches plus one
/// shared cell are the door: it re-asks only for a height it has not seen, which keeps the ask from feeding back
/// into itself, and a person's drag is untouched because their drag also changes the height only via the paned.
pub fn rest_on_resize(divider: &gtk::Paned) {
    let seen_height = std::rc::Rc::new(std::cell::Cell::new(0));
    for name in ["width", "height"] {
        let seen = seen_height.clone();
        divider.connect_notify_local(Some(name), move |paned, _| {
            let height = paned.height();
            if height > 1 && seen.get() != height {
                seen.set(height);
                paned.set_position(SELF_SPLIT_TOP_PX);
            }
        });
    }
}
