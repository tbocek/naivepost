//! §06-effects#2-the-lane-and-the-preview — the Cut page's preview frame and the lane under it, assembled in
//! one call, so `window.rs` only says "install them here". No rule lives here: alphas, rows, plate sentences and rectangles come from [`crate::fx_lane`]'s view-model, pushed in by `window::refresh_effects_lane`.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::fx_lane;
use crate::ui::{cut_picture, cut_preview_frame};

/// Build the effects-lane holder and the preview panel and pack them into `page_box` directly after `previous`, in the order `spec/img/06-lane.png` gives: frame first, then the lane. Returns the panel.
/// Both come back EMPTY: `window::refresh_effects_lane` fills them once there is a cut, and nothing here looks a widget up by name (the window is not realized yet).
pub fn install(page_box: &gtk::Box, previous: &gtk::Widget) -> gtk::Widget {
    // THE CUT PAGE ONLY, like `fold-badges`: `page_box` runs once per tab and a second box sharing the name makes every lookup ambiguous.
    let lane_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    lane_box.set_widget_name("effects-lane");
    lane_box.set_tooltip_text(Some(
        "one row per overlapping group \u{00b7} each kind its own colour",
    ));

    // The preview as ONE frame: an `Overlay` whose child is the picture, with every mark painted ON TOP. The rect / overlays / plate boxes keep their names because tests read them
    // (`preview-camera-rect`'s opacity answers F3.1 S1, `preview-overlay-<i>` the held overlay). No drawing happens in these boxes; numbers come from `fx_lane`'s view-model via `refresh_effects_lane`.
    //
    // The height is pinned (`135` = 240 x 9/16) and every overlay is measured (`set_measure_overlay`): a filling overlay child stretches the panel far below the picture,
    // and a `DrawingArea` clips its draw func to its allocation, so a filling surface paints nothing.
    let preview = gtk::Overlay::new();
    preview.set_widget_name("preview-panel");
    preview.set_size_request(
        crate::ui::window::PREVIEW_PANEL_W as i32,
        crate::ui::window::PREVIEW_PANEL_H as i32,
    );
    preview.set_halign(gtk::Align::Start);
    preview.set_tooltip_text(Some(
        "paused: everything outside the camera rect dimmed \u{00b7} playing: only the mask and the titles",
    ));

    // §A's GtkPicture is the overlay's CHILD, under every mark above it; it fills the panel, which every overlay is measured against (§2: the dim is everything OUTSIDE the camera rect).
    let rect = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rect.set_widget_name("preview-camera-rect");
    // The panel's size is a FLOOR now that it sits in a resizable pane (§A's "paned (660)"): this box, its only child, fills whatever the panel is given, so picture, dim and overlays share one box at any width (§2).
    rect.set_hexpand(true);
    rect.set_vexpand(true);
    rect.set_size_request(
        crate::ui::window::PREVIEW_PANEL_W as i32,
        crate::ui::window::PREVIEW_PANEL_H as i32,
    );
    rect.append(&cut_picture::build());
    preview.set_child(Some(&rect));

    // The dim is PAINTED, not plated: `cut_preview_frame::paint_paused` draws the four bands outside the camera rect, since a CSS box over the panel would cover the crop and the lane too
    // (and clipped to the panel it grew 600 px down the column). So no widget is drawn for the dim.

    // The overlay stack holds one child per effect that draws while paused, named by its index in the cut's `fx` list (`preview-overlay-<i>`'s opacity and `.held-effect` class are what §2's held-one-full / released rules check).
    // VISIBLE with no size of its own: the words painted on the surface above are the same records', so it must not draw a second copy.
    let overlays = gtk::Box::new(gtk::Orientation::Vertical, 0);
    overlays.set_widget_name("preview-overlays");
    preview.add_overlay(&overlays);

    // The debt sentence, centred on the picture (§2's "rect stroked white with a plate").
    let plate = gtk::Label::new(None);
    plate.set_widget_name("preview-plate");
    plate.set_valign(gtk::Align::Center);
    plate.set_halign(gtk::Align::Center);
    preview.add_overlay(&plate);
    preview.set_measure_overlay(&plate, true);
    preview.set_clip_overlay(&plate, true);

    // The painted surface: one `DrawingArea` over everything above (rules in `cut_preview_frame`).
    let frame = cut_preview_frame::build();
    frame.set_size_request(
        crate::ui::window::PREVIEW_PANEL_W as i32,
        crate::ui::window::PREVIEW_PANEL_H as i32,
    );
    preview.add_overlay(&frame);
    preview.set_measure_overlay(&frame, true);
    preview.set_clip_overlay(&frame, true);

    // Under the frame, in the spec picture's order: preview, then the lane (camera rows and the time ruler sit above today: the one layout gap §2 still has).
    page_box.insert_child_after(&preview, Some(previous));
    page_box.insert_child_after(&lane_box, Some(&preview));
    preview.upcast()
}

/// Raise the panel's floor so the picture fills the upper pane instead of a 240x135 plate (§A: "GtkPicture contain-fit, min height 160"); `arrange` is the one place that knows the pane size.
/// Only the FLOOR moves (`preview-camera-rect` already hexpands/vexpands and overlays are measured against it), so a smaller pane still gets the old minimum.
pub fn grow_to(preview: &gtk::Widget, width: f64, height: f64) {
    // The FLOOR is the declared shape, not the ask: a pane that got LESS than PREVIEW_PANEL_W/H (the 660 px §A gives the preview) must not be forced to 1280, or GTK clips the picture at its right edge.
    let wide = width.max(crate::ui::window::PREVIEW_PANEL_W);
    let tall = height
        .max(crate::ui::cut_page_layout::PREVIEW_FLOOR_H)
        .max(crate::ui::window::PREVIEW_PANEL_H);
    preview.set_size_request(wide as i32, tall as i32);
    // The child carries the same floor: it is what the picture and every overlay are measured against.
    if let Some(rect) = preview
        .downcast_ref::<gtk::Overlay>()
        .and_then(|panel| panel.child())
    {
        rect.set_size_request(wide as i32, tall as i32);
    }
    // The painted surface is a MEASURED overlay, so its own 135 px request would drag the panel's minimum back down; it states the same box.
    if let Some(frame) = preview
        .downcast_ref::<gtk::Overlay>()
        .map(|panel| panel.upcast_ref::<gtk::Widget>().clone())
        .as_ref()
        .and_then(|panel| overlay_child_named(panel, crate::ui::cut_preview_frame::FRAME_NAME))
    {
        frame.set_size_request(wide as i32, tall as i32);
    }
}

/// One of an overlay's children by name, searched among ITS children only (a tree-wide walk could answer for another window's copy).
fn overlay_child_named(panel: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    let mut node = panel.first_child();
    while let Some(child) = node {
        if child.widget_name() == name {
            return Some(child);
        }
        node = child.next_sibling();
    }
    None
}

/// Bring the overlay stack to exactly the overlays this frame lists, then push their alphas and classes.
pub(crate) fn sync_overlay_stack(
    stack: &gtk::Box,
    scene: &fx_lane::PausedScene,
    held_index: Option<usize>,
) {
    let wanted: Vec<String> = scene
        .overlays
        .iter()
        .map(|(index, _, _)| format!("preview-overlay-{index}"))
        .collect();
    let mut node = stack.first_child();
    while let Some(child) = node {
        let next = child.next_sibling();
        if !wanted.contains(&child.widget_name().to_string()) {
            stack.remove(&child);
        }
        node = next;
    }
    for (index, _kind, alpha) in &scene.overlays {
        let name = format!("preview-overlay-{index}");
        let overlay = ensure_named_child(stack, &name, |_| {
            let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
            box_.set_vexpand(true);
            box_
        });
        // The one in hand draws full (§2: "the held one full"); every other keeps its glide's alpha (`held_full` says which).
        let shown = if Some(*index) == held_index && scene.held_full {
            1.0
        } else {
            *alpha
        };
        overlay.set_opacity(shown);
        // The dashed violet outline is ADDED and REMOVED by state: one left on after the effect was put down would claim it is still in hand.
        set_class(
            &overlay,
            "held-effect",
            Some(*index) == held_index && scene.held_outline_dashed,
        );
    }
}

/// Find a direct child of `holder` by name, or build it once and append it; direct only, since a tree-wide search can land on another window's copy (a closed GTK window is not destroyed).
fn ensure_named_child<F: FnOnce(i32) -> gtk::Box>(
    holder: &gtk::Box,
    name: &str,
    make: F,
) -> gtk::Box {
    if let Some(found) = find_in_holder(holder.as_ref(), name) {
        return found;
    }
    let made = make(holder.height().max(1));
    made.set_widget_name(name);
    holder.append(&made);
    made
}

/// This module's own copy of the page's direct-child lookup, so the two do not share a growing file.
fn find_in_holder(holder: &gtk::Widget, name: &str) -> Option<gtk::Box> {
    let mut child = holder.first_child();
    while let Some(node) = child {
        if node.widget_name() == name {
            return node.downcast::<gtk::Box>().ok();
        }
        child = node.next_sibling();
    }
    None
}

fn set_class<W: AsRef<gtk::Widget>>(widget: &W, class: &str, on: bool) {
    let widget = widget.as_ref();
    if on {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}
