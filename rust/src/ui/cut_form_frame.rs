//! §05-cut#1-screen — the form column's PINNED parts. `spec/inventory/cut.md` §A spells the column as
//! three things, not one: "pinned heading (title + ✕ \"close this form — nothing is lost that was not
//! already saved\"), a scroller holding either the idle rows or a form body, pinned footer for buttons".

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui;
use crate::ui::window::find_widget_by_name;

/// §A's tooltip for the heading's ✕, spelled once here because it is this widget's sentence.
pub const CLOSE_TIP: &str = "close this form \u{2014} nothing is lost that was not already saved";

/// The cross's label. §A names it ✕ and §1's badge list reuses the same glyph for the other closes.
pub const CLOSE_LABEL: &str = "\u{2715}";

pub const NOTHING_OPEN: &str = "nothing is open \u{2014} the timeline shows the idle readings";

/// The box inside `cut-form-scroller` that holds the idle rows and every form holder.
pub const BODY_NAME: &str = "cut-form-body";

pub fn frame(body: &gtk::Widget) -> gtk::Box {
    let (column, _footer) = frame_parts(body);
    column
}

/// The §A column and its footer as two handles. `frame` hands out only the column because that is all a page
/// needs to place; the insert flow also needs the footer itself, so it gets this instead.
pub fn frame_parts(body: &gtk::Widget) -> (gtk::Box, gtk::Box) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_widget_name("cut-form-frame");
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    heading.set_widget_name("cut-form-heading");
    // §A's own pinned footer, inside the column so GTK keeps it at the column's bottom edge under the
    // scrolling body. It is the footer every non-insert form ends with; the insert takes the separate box
    // `frame_parts` hands out instead, because its form is drawn BELOW this frame (§S4).
    let idle_footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    idle_footer.set_widget_name("cut-form-footer");
    idle_footer.set_visible(false);
    let title = gtk::Label::new(None);
    title.set_widget_name("cut-form-title");
    title.set_xalign(0.0);
    title.add_css_class("heading");
    // Ellipsised rather than wrapped: the heading is one line by §A, and a two-line heading would move the
    // scroller's top edge between forms.
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_hexpand(true);
    let close = gtk::Button::with_label(CLOSE_LABEL);
    close.set_widget_name("cut-form-close");
    close.set_tooltip_text(Some(CLOSE_TIP));
    close.set_valign(gtk::Align::Center);
    heading.append(&title);
    heading.append(&close);
    heading.set_visible(false);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_widget_name("cut-form-scroller");
    // Vertical only, and `External` horizontally so a wide form cannot widen the column (§A's scroller holds
    // either the idle rows or a form body, and the column's width is the page's, not the form's).
    scroller.set_policy(gtk::PolicyType::External, gtk::PolicyType::Automatic);
    scroller.set_vexpand(true);
    let scrolled = gtk::Box::new(gtk::Orientation::Vertical, 4);
    scrolled.set_widget_name(BODY_NAME);
    scrolled.append(body);
    scroller.set_child(Some(&scrolled));
    scroller.connect_child_notify({
        let scroller = scroller.clone();
        move |_pspec| {
            scroll_to_top_of(&scroller);
        }
    });
    let heading_for_adjust = heading.clone();
    let correcting = std::rc::Rc::new(std::cell::Cell::new(false));
    let hold = correcting.clone();
    scroller.vadjustment().connect_value_changed(move |adjust| {
        if heading_for_adjust.is_visible() || hold.get() || adjust.value() <= adjust.lower() {
            return;
        }
        hold.set(true);
        adjust.set_value(adjust.lower());
        hold.set(false);
    });

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    footer.set_widget_name("insert-form-footer");
    footer.set_visible(false);

    column.append(&heading);
    column.append(&scroller);
    column.append(&idle_footer);
    (column, footer)
}

pub fn scroll_to_top_of(scroller: &gtk::ScrolledWindow) {
    let adjust = scroller.vadjustment();
    adjust.set_value(adjust.lower());
}

pub fn scroll_to_top(window: &adw::ApplicationWindow) -> bool {
    let Some(scroller) = named(window, "cut-form-scroller") else {
        return false;
    };
    let Some(scroller) = scroller.downcast::<gtk::ScrolledWindow>().ok() else {
        return false;
    };
    scroll_to_top_of(&scroller);
    true
}

/// The frame's widgets, newest window winning like every other per-window reader here.
fn named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    let content = window.content()?;
    find_widget_by_name(content.upcast_ref(), name)
}

/// Empty a holder: what it owns comes out, never by a tree-wide name search (a closed window's widgets stay parented
/// somewhere).
pub(crate) fn clear_children(holder: &gtk::Box) {
    let stale: Vec<gtk::Widget> = holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .collect();
    for old in stale {
        holder.remove(&old);
    }
}

/// §A's pinned FOOTER: the buttons that finish the form live here, not at the end of the scrolling body.
pub fn set_footer(window: &adw::ApplicationWindow, buttons: Option<&gtk::Box>) -> bool {
    // The footer belongs to the PAGE (`page_box` runs once per tab), not to the form, and it is named apart
    // from §A's `cut-form-footer` (see [`frame_parts`]) so the one in the open page's tree is also the insert's
    // own. It is emptied before anything new goes in: a box that already has a parent cannot be appended again.
    let Some(footer) = live_footer(window) else {
        return false;
    };
    clear_children(&footer);
    match buttons {
        Some(widget) => {
            footer.append(widget);
            footer.set_visible(true);
        }
        None => footer.set_visible(false),
    }
    true
}

pub fn set_readings(window: &adw::ApplicationWindow, show: bool) -> bool {
    let Some(grid) = named(window, "cut-form") else {
        return false;
    };
    grid.set_visible(show);
    true
}

fn live_footer(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    named(window, "insert-form-footer")?
        .downcast::<gtk::Box>()
        .ok()
}

/// The column's OWN pinned footer (§A's), inside the frame under its scroller. `frame` keeps it; only the
/// insert flow takes the separate box `frame_parts` hands out.
fn column_footer(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    named(window, "cut-form-footer")?
        .downcast::<gtk::Box>()
        .ok()
}

pub fn set_pinned(window: &adw::ApplicationWindow, heading: Option<&str>) -> bool {
    // The column's own pinned parts. An INSERT is not one of them: its form and its footer are their own boxes
    // below the frame (§S4 draws the form in place of the readings), so `set_footer` owns that pair and this
    // door must not hide it while a form is drawing.
    let insert_open = ui::visible_insert_form(window).is_some();
    let (Some(title), Some(head)) = (
        named(window, "cut-form-title").and_then(|w| w.downcast::<gtk::Label>().ok()),
        named(window, "cut-form-heading"),
    ) else {
        return false;
    };
    if named(window, "cut-form-footer").is_none() && named(window, "insert-form-footer").is_none() {
        return false;
    }
    match heading {
        Some(words) => {
            title.set_text(words);
            head.set_visible(true);
            // §A's scroller holds EITHER the idle rows OR a form body. The form holders sit in this column
            // under the readings, so a form opened with the eight rows still showing stacked both, the
            // column outgrew its pane and the heading with its ✕ was painted above the window's top edge.
            set_readings(window, false);
            if !insert_open {
                if let Some(footer) = column_footer(window) {
                    // Only show a footer that actually holds something: a form that puts no buttons here (zoom,
                    // label, the ✕ path) would otherwise paint a bare 4 px box.
                    footer
                        .set_visible(footer.observe_children().iter::<glib::Object>().count() > 0);
                }
            }
        }
        None => {
            title.set_text("");
            head.set_visible(false);
            if !insert_open {
                // A closed form leaves no buttons behind: an empty footer would still paint its 4 px box.
                if let Some(footer) = column_footer(window) {
                    clear_children(&footer);
                    footer.set_visible(false);
                }
            }
            set_readings(window, true);
        }
    }
    true
}

pub fn open_form_title() -> Option<String> {
    if let Some((path, _, _)) = ui::insert_open() {
        let base = std::path::Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or(path);
        return Some(format!("insert {base}"));
    }
    if ui::zoom_form_open().is_some() {
        return Some("place the zoom".to_string());
    }
    if ui::cut_hand::held_effect().is_some() {
        return Some("edit the effect".to_string());
    }
    if ui::cut_hand::held_clip_or_edge().is_some() || ui::cut_hand::held_edge().is_some() {
        return Some("picked up clip".to_string());
    }
    None
}

pub fn press_close(window: &adw::ApplicationWindow) -> String {
    if ui::insert_open().is_some() {
        let said = ui::close_insert_form(window);
        set_pinned(window, None);
        return said;
    }
    if ui::zoom_form_open().is_some() {
        let said = ui::press_zoom_cancel(window);
        set_pinned(window, None);
        return said;
    }
    if let Some(said) = ui::press_band_esc(window) {
        set_pinned(window, None);
        // Back to the readings, from their first line (§1).
        scroll_to_top(window);
        return said;
    }
    set_pinned(window, None);
    scroll_to_top(window);
    NOTHING_OPEN.to_string()
}

/// Attach the cross's click. Called from `build_window` after `set_content`, like every other control, so
/// the button is inside the realized tree and a test's `emit_by_name("clicked")` reaches it.
pub fn wire_close(window: &adw::ApplicationWindow) {
    let Some(button) =
        named(window, "cut-form-close").and_then(|w| w.downcast::<gtk::Button>().ok())
    else {
        return;
    };
    let win = window.clone();
    button.connect_clicked(move |_| {
        let said = press_close(&win);
        if let Some(status) = ui::find_status(win.upcast_ref()) {
            status.set_text(&said);
        }
    });
}
