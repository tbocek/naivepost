//! §05-cut#1-screen — the form column's PINNED parts. `spec/inventory/cut.md` §A spells the column as
//! three things, not one: "pinned heading (title + ✕ \"close this form — nothing is lost that was not
//! already saved\"), a scroller holding either the idle rows or a form body, pinned footer for buttons".
//! The eight idle readings and each form's own fields already existed; what they lacked was the frame that
//! holds them, so a long form scrolled its heading out of reach and there was no single ✕ that closed
//! whatever was open. This module owns only that frame and the press behind its cross — every rule about
//! WHAT a form contains stays with the module that draws it (`cut_insert`, `fx_zoom`, `cut_text_hand`, …).

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui;

/// §A's tooltip for the heading's ✕, spelled once here because it is this widget's sentence.
pub const CLOSE_TIP: &str = "close this form \u{2014} nothing is lost that was not already saved";

/// The cross's label. §A names it ✕ and §1's badge list reuses the same glyph for the other closes.
pub const CLOSE_LABEL: &str = "\u{2715}";

/// What the status line says when the cross is pressed with nothing open. The spec gives no wording for a
/// press that had nothing to do, so this states the fact plainly rather than staying silent: a person who
/// clicked it wants to know the page is already clean.
pub const NOTHING_OPEN: &str = "nothing is open \u{2014} the timeline shows the idle readings";

/// The §A column: pinned heading, then a scroller holding `body`, then a pinned footer. Appended in that
/// order so GTK keeps the heading and the footer at their edges while the middle scrolls — which is the
/// whole point of pinning them, and why the body goes inside a `gtk::Scroller` rather than straight into
/// the box. Heading and footer start hidden: an idle column has no title to show and no buttons to press.
pub fn frame(body: &gtk::Widget) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_widget_name("cut-form-frame");
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    heading.set_widget_name("cut-form-heading");
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
    scroller.set_child(Some(body));

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    footer.set_widget_name("cut-form-footer");
    footer.set_visible(false);

    column.append(&heading);
    column.append(&scroller);
    column.append(&footer);
    column
}

/// The frame's widgets, newest window winning like every other per-window reader here.
fn named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    let content = window.content()?;
    find(content.upcast_ref(), name)
}

/// Depth-first search by widget name, the same walk `window.rs` uses. Kept local because that one is
/// `pub(crate)` to its own module and this file must not reach past the module boundary for a helper.
fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find(&widget, name) {
            return Some(found);
        }
    }
    None
}

/// Show the heading's title and the footer, or drop back to the idle column. One door so the pinned parts
/// cannot disagree about whether a form is open: the caller says what is open (or `None`), and this decides
/// what the frame looks like. Called from the paths that already open and close a form.
pub fn set_pinned(window: &adw::ApplicationWindow, heading: Option<&str>) -> bool {
    let (Some(title), Some(head), Some(footer)) = (
        named(window, "cut-form-title").and_then(|w| w.downcast::<gtk::Label>().ok()),
        named(window, "cut-form-heading"),
        named(window, "cut-form-footer"),
    ) else {
        return false;
    };
    match heading {
        Some(words) => {
            title.set_text(words);
            head.set_visible(true);
            footer.set_visible(true);
        }
        None => {
            title.set_text("");
            head.set_visible(false);
            footer.set_visible(false);
        }
    }
    true
}

/// Is any form open on this window? Asked of the doors that hold the state, never of a copy kept here: the
/// insert form, the zoom form, a held effect being edited, and a clip picked up by ⧉ Copy. `pub` because the
/// paths that OPEN a form ask it what to pin, so the heading can never name a form that is not open.
pub fn open_form_title(window: &adw::ApplicationWindow) -> Option<String> {
    if let Some((path, _, _)) = ui::insert_open(window) {
        let base = std::path::Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or(path);
        return Some(format!("insert {base}"));
    }
    if ui::zoom_form_open().is_some() {
        return Some("place the zoom".to_string());
    }
    if ui::held_effect().is_some() {
        return Some("edit the effect".to_string());
    }
    if ui::held_segment().is_some() || ui::held_edge().is_some() {
        return Some("picked up clip".to_string());
    }
    None
}

/// The heading's ✕ (§1's "pinned heading (✕ closes)"). ONE press closes whichever form the page holds,
/// forwarding to the door that owns that form rather than duplicating its cleanup: the insert form through
/// [`ui::close_insert_form`], a pending zoom through [`ui::press_zoom_cancel`] (which also drops the stored
/// form, so the cross cannot leave a form behind with no panel), a held effect or band through
/// [`ui::press_band_esc`]. Nothing open answers [`NOTHING_OPEN`] rather than pretending to have closed.
pub fn press_close(window: &adw::ApplicationWindow) -> String {
    if ui::insert_open(window).is_some() {
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
        return said;
    }
    set_pinned(window, None);
    NOTHING_OPEN.to_string()
}

/// Attach the cross's click. Called from `build_window` after `set_content`, like every other control, so
/// the button is inside the realized tree and a test's `emit_by_name("clicked")` reaches it.
pub fn wire_close(window: &adw::ApplicationWindow) {
    let Some(button) = named(window, "cut-form-close").and_then(|w| w.downcast::<gtk::Button>().ok()) else {
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
