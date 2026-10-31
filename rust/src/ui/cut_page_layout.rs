//! §05-cut#1-screen: the Cut page's own order. `spec/inventory/cut.md` §A / §1: preview with the reading column beside it
//! (**1**-**4**), then toolbar (**5**-**22**), tracks (**23**-**30**), scrollbar (**31**), status line (**32**). TWO dividers:
//! `cut-page-split` (top/bottom, opens at [`SELF_SPLIT_TOP_PX`]) and `cut-top` (left/right, [`SELF_SPLIT_PREVIEW_PX`]);
//! `cut-bottom` holds toolbar, tracks and scrollbar. This module fixes the ORDER only, re-parenting what `window.rs` built
//! and wired, so names and handlers survive. DECISION (spec silent): the start child is the preview panel, the end child
//! a column of the idle readings plus every form holder (§1's readings sit right of the picture in `img/05-cut.png`).

use adw::prelude::*;
use gtk4 as gtk;

/// The order §A/§1 give the page below the `Cut` title row and the paned; the scrollbar LAST (badge **31** above **32**).
/// The effects lane travels with the picture in [`PREVIEW_STACK`] (§06-effects#2).
const BELOW_PANED: [&str; 5] = [
    "cut-toolbar",
    "track-layers",
    "selection-row",
    "camera-rows",
    "fold-all-holder",
];

const HELD_UNSEEN: [&str; 3] = ["selection-row", "camera-rows", "fold-all-holder"];

const NO_FILL: [&str; 2] = ["cut-form-frame", "selection-row"];

/// The toolbar spans the page's width: with `halign(Start)` it stopped at its groups' natural widths and clipped them
/// (at 1260 px `group-verbs` got 244 px of 675 px, so Copy/Paste/Insert/Lane sat off the visible bar).
const TOOLBAR_SPANS_PAGE: bool = true;

/// The paned's start half, in order: the preview panel and the effects lane (§06-effects#2, `spec/img/06-lane.png`); they
/// stay siblings because readers, and §06's seat test, find the lane as the frame's next sibling.
const PREVIEW_STACK: [&str; 2] = ["preview-panel", "effects-lane"];
/// The form holders beside the readings, one `‹thing›-form-‹tab›` per thing placed or edited (tab-scoped by their builder).
fn form_names(page: &str) -> Vec<String> {
    ["zoom", "speed", "text", "svg", "volume", "label"]
        .into_iter()
        .map(|kind| format!("{kind}-form-{page}"))
        // The insert form's holder is NOT tab-scoped (window.rs names it plain `insert-form`).
        .chain(std::iter::once("insert-form".to_string()))
        .collect()
}

/// §A's opening width for the LEFT/RIGHT divider (`paned 660/380`); `pub` so `params::cut()` can catalogue it.
pub const SELF_SPLIT_PREVIEW_PX: i32 = 660;

/// §A's opening height for the TOP/BOTTOM divider (the other half of `paned 660/380`).
pub const SELF_SPLIT_TOP_PX: i32 = 380;

/// UNUSED: what the pane below the top/bottom divider would have asked for; see [`UPPER_PANE_MIN_H`] on why neither pane carries a request.
#[allow(dead_code)]
pub const LOWER_PANE_MIN_H: i32 = 196;

/// UNUSED, kept as the record of why the openings are positions, not requests: this 410 px plus [`LOWER_PANE_MIN_H`] sums past
/// §1's minimum page band (~700 px) and the Cut page scrolled as a whole.
#[allow(dead_code)]
pub const UPPER_PANE_MIN_H: i32 = SELF_SPLIT_TOP_PX + 30;

/// The FLOOR the picture gets in the upper pane: §A's "GtkPicture contain-fit, min height 160". The panel is built at
/// `PREVIEW_PANEL_H` (135) and grows at the footage's own shape once the pane offers more.
pub const PREVIEW_FLOOR_H: f64 = 160.0;

/// What the upper pane does NOT give the picture: the ruler (18 px) and selection band (22 px) that the effects lane's rows live among.
pub const BANDS_ABOVE_PICTURE_PX: i32 = 18 + 22;

/// Bottom pane's FLOOR in pixels, as `cut-bottom`'s minimum height: a `Paned` will not place its handle past its end child's
/// minimum, so on a short window the picture above the divider gives, not the tape (measured: `cut-bottom` given 274 px of a
/// 634 px pane at 1440×900 while the band asked 126 px and got 132).
pub const CUT_BOTTOM_FLOOR_PX: i32 = 196;

/// The tallest the picture may be ASKED for past §A's opening: an ask above [`SELF_SPLIT_TOP_PX`] becomes the start-pane
/// minimum, the handle clamps, and §1's first idle reading is clipped out. One rule for the page and any later re-ask.
pub fn preview_ask_ceiling(pane_h: i32, lane_h: i32) -> i32 {
    (pane_h - lane_h).min(SELF_SPLIT_TOP_PX - lane_h).max(1)
}

const PANED_HANDLE_GAP_PX: i32 = 1;

/// What the bottom pane costs BESIDES the tape, asked rather than remembered (toolbar minimum, scrollbar, the two gaps),
/// never less than [`CUT_BOTTOM_FLOOR_PX`].
fn cut_bottom_floor_px(band_ask: i32) -> i32 {
    band_ask
        + (cut_toolbar_min_px() + CUT_TRACK_GAP_PX * 2 + scrollbar_ask_px())
            .max(CUT_BOTTOM_FLOOR_PX - band_ask)
}

fn cut_toolbar_min_px() -> i32 {
    TOOLBAR_ASK_PX
}

const TOOLBAR_ASK_PX: i32 = 34;
const CUT_TRACK_GAP_PX: i32 = 4;
/// An unmapped scrollbar answers 0 px; this is what it costs once the tape is wider than the view (§A's badge 31).
const SCROLLBAR_ASK_PX: i32 = 14;

fn scrollbar_ask_px() -> i32 {
    SCROLLBAR_ASK_PX
}

/// The scroller's FLOOR: what the idle readings cost, asked of the real `cut-form` grid. At 200 px the viewport was
/// shorter than the rows (~250 px) and the column opened on its TAIL, the first reading above the visible edge.
pub const IDLE_ROWS_MIN_HEIGHT: i32 = ROW_PITCH_PX * IDLE_ROW_COUNT as i32;
const ROW_PITCH_PX: i32 = 29;
pub const IDLE_ROW_COUNT: usize = 7;
pub const FORM_COLUMN_MIN_W: i32 = 300;
/// The height that column needs to show its idle readings whole (§1). At the minimum window the pane came out ~65 px against
/// ~230 px wanted, showing only the last four rows; what gives is the divider position (§1 allows it).
pub const FORM_COLUMN_MIN_H: i32 = IDLE_ROWS_MIN_HEIGHT + COLUMN_FRAME_PX;
const COLUMN_FRAME_PX: i32 = 40;
/// The height band for the scroller between heading and footer. Uncapped it asked for the whole page and pushed the tracks
/// ~750 px down; at 460 the footer ("Insert it" + "Cancel") fell below the painted window.
const IDLE_ROWS_CAP_H: i32 = 300;

/// Re-order the Cut page's children into §A's shape, once at the end of `window.rs`'s Cut arm. An absent name is skipped, not fatal.
pub fn arrange(page_box: &gtk::Box, window: &adw::ApplicationWindow) {
    arrange_priced(
        page_box,
        crate::ui::cut_tracks::idle_band_height(),
        Some(window),
    );
}

/// `arrange` with the tape's height ask supplied: §1's band costs a row per camera AND per cut lane, so a caller with a cut must
/// charge the bottom pane for the rows THAT scene paints.
pub fn arrange_priced(page_box: &gtk::Box, band_ask: i32, window: Option<&adw::ApplicationWindow>) {
    let pane_floor_px = cut_bottom_floor_px(band_ask);
    let title_row = match page_box.first_child() {
        Some(first) => first,
        None => return,
    };
    // Detach everything placed, so re-inserting cannot leave a widget in two seats (`gtk::Box::remove` needs a direct child).
    let page = crate::shell::Page::Cut.label();
    let forms = form_names(page);
    let mut wanted: Vec<String> = vec!["cut-form-frame".to_string(), "track-scrollbar".to_string()];
    wanted.extend(PREVIEW_STACK.iter().map(|name| name.to_string()));
    wanted.extend(forms.iter().cloned());
    wanted.extend(BELOW_PANED.iter().map(|name| name.to_string()));
    let detached = detach(page_box, &wanted);
    // `fold-all-holder` is appended AFTER this call (window.rs adds it after the Cut arm closes), so pick it up where it sits.
    let fold_all = page_box
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .find(|child| child.widget_name() == "fold-all-holder");
    // §1's bottom half is the band; the placeholder strip stays in the tree for doors that find it but takes no room (seen, it left 96 px of empty page).
    if let Some(strip) = page_box
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .find(|child| child.widget_name() == "track-strip")
    {
        strip.set_visible(false);
    }
    let mut detached = detached;
    if let Some(holder) = &fold_all {
        page_box.remove(holder);
        detached.push(holder.clone());
    }

    let preview = detached
        .iter()
        .find(|w| w.widget_name() == PREVIEW_STACK[0])
        .cloned();
    let lane = detached
        .iter()
        .find(|w| w.widget_name() == PREVIEW_STACK[1])
        .cloned();
    // The form column's scroller vexpands by design, which inside a paned asked for the whole page's height and pushed the tracks ~750 px below the preview.
    let form_frame = detached
        .iter()
        .find(|w| w.widget_name() == "cut-form-frame")
        .cloned();
    if let Some(frame) = &form_frame {
        frame.set_vexpand(false);
        // The Cut page box is as wide as the TAPE (§A badge 31 scrolls), so no per-widget ceiling holds the column narrow; the pane position seats it.
        frame.set_hexpand(false);
        frame.set_size_request(FORM_COLUMN_MIN_W, FORM_COLUMN_MIN_H);
        if let Some(scroller) = frame
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .filter_map(|child| child.downcast::<gtk::ScrolledWindow>().ok())
            .find(|child| child.widget_name() == "cut-form-scroller")
        {
            // The scroller between the pinned heading and footer takes the rest of the height, capped so a long form scrolls instead of pushing the tape down.
            scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
            scroller.set_vexpand(false);
            // Ask the grid what its readings cost, using the larger of that and the row-count floor; the same number drives the frame's request and the scroller's min/max.
            let rows_h = IDLE_ROWS_MIN_HEIGHT.max(idle_rows_height(frame));
            scroller.set_min_content_height(rows_h);
            scroller.set_max_content_height(IDLE_ROWS_CAP_H.max(rows_h));
            // The ASK goes on the scroller alone: a request on the frame is charged to the page box and pushed `page-scroll-Cut` past its band
            // (§1: no whole-page scrolling, `tests/suite/window_shrinks.rs`). The column must also FIT §A's 380 px opening: left to its own asks
            // it measured 417 px and overflowed past the divider. Cap the height here, not after the map: a headless window is allocated once.
            frame.set_valign(gtk::Align::Start);
            frame.set_size_request(
                FORM_COLUMN_MIN_W,
                (rows_h + COLUMN_FRAME_PX).min(SELF_SPLIT_TOP_PX),
            );
            scroller.set_propagate_natural_height(true);
            // A scroller keeps its adjustment's last value, so ask for the TOP of the readings here (and whenever a body is placed in the frame).
            if let Some(window) = window {
                crate::ui::cut_form_frame::scroll_to_top(window);
            }
            // And once more at the first real allocation: the ask above lands at 0 px tall and headless fires no `notify::height` ([`rest_opening_once`]).
            rest_scroll_once(&scroller);
        }
        if let Some(grid) = frame
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .filter_map(|child| child.downcast::<gtk::Grid>().ok())
            .find(|grid| grid.widget_name() == "cut-form")
        {
            // The grid takes the whole pane so the values sit at the column's right end (§1: "label left and value right"); the scroller scrolls horizontally on `Automatic`.
            grid.set_halign(gtk::Align::Fill);
            grid.set_hexpand(true);
        }
    }
    // Hidden holders still cost 4 px box spacing, so they stay in the page box where their owner's door finds them; only the readings come into the paned.
    let mut form_widgets: Vec<gtk::Widget> = Vec::new();
    if let Some(readings) = form_frame {
        // §A's ONE column: "a scroller holding EITHER the idle rows OR a form body". Holders go INSIDE the scroller: under it, an open zoom
        // form made the column 554 px tall in a 380 px pane and the pinned heading and its ✕ were painted above the window.
        let body = find_named_box(&readings, crate::ui::cut_form_frame::BODY_NAME);
        for name in &forms {
            if let Some(holder) = detached.iter().find(|w| &w.widget_name() == name) {
                holder.set_hexpand(false);
                // A holder is found by a tree walk from the window; a `Grid` keeps children privately (`gtk_widget_get_first_child` answers None), so it goes into the frame.
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
    split.set_wide_handle(false);
    split.set_hexpand(true);
    // Otherwise GTK grows the paned to fill the window and pushes the tracks down.
    split.set_vexpand(false);
    if let Some(preview) = &preview {
        // §06-effects#2: the lane goes UNDER the frame it marks as siblings of one vertical box; readers walk from the frame to its next sibling.
        let stack: gtk::Widget = match &lane {
            Some(lane) => {
                let box_ = gtk::Box::new(gtk::Orientation::Vertical, 1);
                box_.set_widget_name("cut-preview-stack");
                // §1: "the video preview scales to the room it is given": the stack FILLS its pane; `grow_to` keeps the floors as floors.
                box_.set_vexpand(true);
                box_.set_valign(gtk::Align::Fill);
                box_.append(preview);
                box_.append(lane);
                box_.upcast()
            }
            None => preview.clone(),
        };
        split.set_start_child(Some(&stack));
        split.set_resize_start_child(true);
        // §A: the preview fills vertically (`cut_picture::build` sets `ContentFit::Contain`). Only the LAYOUT grows: camera/drag
        // fractions are measured against the panel's declared pin (`cut_zoom_hand::preview_picture_px`), as an allocation differs between runs.
        preview.set_vexpand(true);
        preview.set_valign(gtk::Align::Fill);
        // `cut_preview_panel::install` pins the panel to `PREVIEW_PANEL_W` at the left; in the start half it takes every pixel (§1's badge 1 dwarfs the readings).
        preview.set_hexpand(true);
        preview.set_halign(gtk::Align::Fill);
    } else if let Some(lane) = &lane {
        split.set_start_child(Some(lane));
        split.set_resize_start_child(true);
    }
    if !forms.is_empty() {
        // Resize on the END child with shrink off makes GTK honour the column's pinned width.
        split.set_end_child(Some(&forms[0]));
        split.set_resize_end_child(true);
        split.set_shrink_end_child(false);
    }
    // The TOP/BOTTOM divider (§A "vertical paned (380)"): one paned, not a stack, so there is a divider to drag.
    let vsplit = gtk::Paned::new(gtk::Orientation::Vertical);
    vsplit.set_widget_name("cut-page-split");
    vsplit.set_wide_handle(false);
    vsplit.set_vexpand(true);
    vsplit.set_start_child(Some(&split));
    vsplit.set_resize_start_child(true);
    vsplit.set_shrink_start_child(false);
    // `cut-top` asks for less than 380 px (~304 measured), so BOTH panes must be allowed to grow or the divider never reaches 380.
    split.set_vexpand(true);
    vsplit.set_position(SELF_SPLIT_TOP_PX);
    // The page band cannot hold the 380 px opening AND the whole tape at §1's minimum window, and a `gtk::Paned` clamps its position into
    // `[start minimum, own height − end minimum]`. §1 forbids the page scrolling (`window_shrinks.rs`), so the pixels come out of the picture:
    // the bottom pane's floor is the tape plus toolbar and bar. With no ask (1240×900) the bottom half was 32 px short and the wave rows were clipped.
    vsplit.set_size_request(
        -1,
        SELF_SPLIT_TOP_PX
            + pane_floor_px
            + PANED_HANDLE_GAP_PX
            + 2 * crate::ui::margins::DIVIDER_GAP_PX,
    );
    let bottom = gtk::Box::new(gtk::Orientation::Vertical, 4);
    bottom.set_widget_name("cut-bottom");
    // NOT `vexpand`: a Paned hands leftover to expanding children by SHARE, and with both expanding the handle settled on the
    // halves' own asks (29 px upper pane against 380) and every control below painted nothing.
    bottom.set_valign(gtk::Align::Fill);
    // Without a minimum the 380 px opening sits on a bottom half too short for ruler + lane + picture row + wave strip, and the
    // `DrawingArea` clips the wave rows. The floor comes from `cut_tracks` so a row added by an edit raises it.
    bottom.set_size_request(-1, pane_floor_px);

    vsplit.set_end_child(Some(&bottom));
    page_box.insert_child_after(&vsplit, Some(&title_row));
    // ONE pair of rules for the start half: a second `set_resize_start_child(false)` later cancelled the `true` and the picture collapsed (125 px wide).
    split.set_position(SELF_SPLIT_PREVIEW_PX);
    split.set_resize_start_child(true);
    split.set_shrink_start_child(false);
    // §A's 380 px is an OPENING, not a drag, and the ask above was clamped for lack of height: re-ask at the first and every later allocation.
    rest_opening_once(&vsplit);

    for name in BELOW_PANED {
        if let Some(widget) = detached.iter().find(|w| w.widget_name() == name).cloned() {
            // §A's band is ONE surface spanning the page, so the bottom half fills the width, except [`NO_FILL`]. `track-strip` is `halign Start` at `STRIP_WIDTH` (480).
            widget.set_hexpand(TOOLBAR_SPANS_PAGE || !NO_FILL.contains(&name));
            widget.set_halign(gtk::Align::Fill);
            // §1's bottom half is only toolbar, tracks and scrollbar; the old widgets for marks the band draws (`cut_band_marks`) stay in the tree, unseen, for doors that find them.
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
    vsplit.set_shrink_start_child(true);
    vsplit.set_resize_end_child(false);
    vsplit.set_shrink_end_child(false);
    // Now the upper pane has real height: hand it to the picture through the panel's own door so the camera rect and overlays measure against the same box.
    if let Some(panel) = preview.as_ref() {
        crate::ui::cut_preview_panel::grow_to(
            panel
                .downcast_ref::<gtk::Widget>()
                .expect("the preview panel is a widget"),
            crate::ui::window::PREVIEW_PANEL_W,
            PREVIEW_FLOOR_H,
        );
    }
}

/// What the idle readings cost: the `cut-form` grid's own vertical minimum, 0 without a form column. The grid is found by DESCENT
/// (`frame_parts` puts the body inside `cut-form-scroller`), and its Aspect ratio row carries a taller `gtk::DropDown` than [`IDLE_ROWS_MIN_HEIGHT`] assumes.
fn idle_rows_height(frame: &gtk::Widget) -> i32 {
    find_grid(frame)
        .map(|grid| grid.measure(gtk::Orientation::Vertical, -1).0)
        .unwrap_or(0)
}

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
    let mut ordered: Vec<gtk::Widget> = Vec::with_capacity(found.len());
    for name in names {
        if let Some(widget) = found.iter().find(|w| &w.widget_name() == name) {
            ordered.push(widget.clone());
        }
    }
    ordered
}

pub fn rest_scroll_once(scroller: &gtk::ScrolledWindow) {
    crate::ui::settle::once_sized(
        "the form column at the top of its readings",
        scroller,
        |scroller| {
            crate::ui::cut_form_frame::scroll_to_top_of(scroller);
        },
    );
}

/// Asks §A's opening of the Cut page's divider once the paned has a size: `set_position` clamps, and before the first allocation the paned is 1 px tall.
pub fn rest_opening_once(divider: &gtk::Paned) {
    crate::ui::settle::once_sized("the Cut page's split at its opening", divider, |divider| {
        divider.set_position(divider.height().max(SELF_SPLIT_TOP_PX));
    });
}
