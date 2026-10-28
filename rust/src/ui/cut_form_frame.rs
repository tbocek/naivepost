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

/// The box inside `cut-form-scroller` that holds the idle rows and every form holder.
pub const BODY_NAME: &str = "cut-form-body";

/// The §A column: pinned heading, then a scroller holding `body`, then a pinned footer. Appended in that
/// order so GTK keeps the heading and the footer at their edges while the middle scrolls — which is the
/// whole point of pinning them, and why the body goes inside a `gtk::Scroller` rather than straight into
/// the box. Heading and footer start hidden: an idle column has no title to show and no buttons to press.
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
    // The scroller's one child is a box: the idle rows first, then (seated by `cut_page_layout::arrange`) the
    // form holders, so an open form scrolls inside the column like the rows do instead of stacking under the
    // scroller and pushing the pinned heading off the pane.
    let scrolled = gtk::Box::new(gtk::Orientation::Vertical, 4);
    scrolled.set_widget_name(BODY_NAME);
    scrolled.append(body);
    scroller.set_child(Some(&scrolled));
    // §1's idle column STARTS with the readings, and a `gtk::ScrolledWindow` keeps whatever value its
    // adjustment last held: once anything scrolled this viewport (a form taller than the pane, or a window
    // squeezed down to §1's minimum), the column opened on its TAIL and the render showed only the last four
    // readings with three clipped pills above them. So every time a body goes into the frame -- the page
    // painting its idle rows, a form opening, a form closing back to the readings -- the ask is to start at
    // the top. `set_child` fires `child-notify::child`, which is what makes this the program's own door.
    scroller.connect_child_notify({
        let scroller = scroller.clone();
        move |_pspec| {
            scroll_to_top_of(&scroller);
        }
    });
    // §1's "the idle column STARTS with the readings", held as a rule on the adjustment itself rather
    // than as one ask made at build time. Anything can park a `gtk::ScrolledWindow`'s viewport lower — a
    // focus landing on the Aspect ratio dropdown, a re-render at another size, a later resize — and a
    // column parked past its first line loses its first reading off the painted pixels while
    // every tree walk still finds the row (measured: the shot's viewport reported `value=0` a moment
    // before its PNG opened on 'Aspect ratio:'). While a form is open the column may scroll — §A pins
    // only its heading and footer — so the guard reads the pinned heading: visible heading means a form
    // is open and its body keeps its scroll.
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
    // §1's idle column STARTS with the readings: a `gtk::ScrolledWindow` keeps whatever value its
    // adjustment last held, so a body that once grew past this viewport (a form opened and closed, or a
    // window resized down) would leave the column parked at its TAIL -- the render showed only the last
    // four readings with three clipped pills above them. The page asks for the top whenever it paints the
    // idle rows (`cut_page_layout::arrange`) and whenever a form closes; this is where the ask lands.

    // The INSERT's own pinned footer, handed out beside the column so the page can place it directly under the
    // insert form (§S4 draws that form below the frame, in place of the readings). It is a SECOND box rather
    // than `cut-form-footer` because that one belongs to the frame and would sit ABOVE this form; the name is
    // unscoped like the form holder's, so `set_footer` only ever writes the page whose form is drawing.
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    footer.set_widget_name("insert-form-footer");
    footer.set_visible(false);

    column.append(&heading);
    column.append(&scroller);
    column.append(&idle_footer);
    (column, footer)
}

/// Put one scroller's viewport at the top of its body. `frame_parts` wires this to every frame's own
/// scroller, so it runs whenever a body goes into the column; [`scroll_to_top`] is the same ask for a
/// caller that only has the window. The page's layout re-asks it on every new height of the viewport
/// (`cut_page_layout::arrange_priced`), because GTK can park the adjustment at its upper limit before the
/// viewport has grown to fit the readings.
pub fn scroll_to_top_of(scroller: &gtk::ScrolledWindow) {
    let adjust = scroller.vadjustment();
    adjust.set_value(adjust.lower());
}

/// Ask this window's form column to show the TOP of its body -- §1's "the idle column starts with the
/// readings". The page calls it after placing the idle rows and after a form closes; a column parked at its
/// tail would hide the first readings (see [`frame_parts`]). Returns whether the window had a column.
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
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Some(found) = find(&widget, name) {
            return Some(found);
        }
    }
    None
}

/// §A's pinned FOOTER: the buttons that finish the form live here, not at the end of the scrolling body.
/// A footer inside the scroller fell below the painted frame (measured on `05-insert-form`: "Insert it" and
/// "Cancel" were never on screen), so the step that places the insert could not be seen or clicked. `Some`
/// puts `buttons` in the footer and shows it; `None` empties it and hides it again.
pub fn set_footer(window: &adw::ApplicationWindow, buttons: Option<&gtk::Box>) -> bool {
    // The footer belongs to the PAGE (`page_box` runs once per tab), not to the form, and it is named apart
    // from §A's `cut-form-footer` (see [`frame_parts`]) so the one in the open page's tree is also the insert's
    // own. It is emptied before anything new goes in: a box that already has a parent cannot be appended again.
    let Some(footer) = live_footer(window) else {
        return false;
    };
    let mut old: Vec<gtk::Widget> = Vec::new();
    for n in 0..footer.observe_children().n_items() {
        if let Some(child) = footer.observe_children().item(n) {
            if let Ok(widget) = child.downcast::<gtk::Widget>() {
                old.push(widget);
            }
        }
    }
    for widget in old {
        footer.remove(&widget);
    }
    match buttons {
        Some(widget) => {
            footer.append(widget);
            footer.set_visible(true);
        }
        None => footer.set_visible(false),
    }
    true
}

/// §S4: a form sits in the column "in place of the idle readings", so opening one hides the eight-row grid
/// (`cut-form`) and closing it brings them back. Hidden rather than removed: the readings are rebuilt by
/// their own owner and must stay where that owner can find them. Found from the WINDOW ROOT, not from the
/// open form's holder: `show_insert_form` calls this at the end of drawing the form, and a walk that started
/// at the holder could only ever find the holder's own children.
pub fn set_readings(window: &adw::ApplicationWindow, show: bool) -> bool {
    let Some(grid) = named(window, "cut-form") else {
        return false;
    };
    grid.set_visible(show);
    true
}

/// The page's own insert-form holder while a form is DRAWING in it (visible and filled), so a footer is only
/// written on the page that has a form. Same walk `visible_insert_form` does, called rather than copied.
fn visible_insert_form_holder(window: &adw::ApplicationWindow) -> Option<gtk::Widget> {
    fn walk(root: &gtk::Widget) -> Option<gtk::Widget> {
        if root.widget_name() == "insert-form"
            && root.is_visible()
            && root.observe_children().n_items() > 0
        {
            return Some(root.clone());
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else {
                continue;
            };
            if let Some(found) = walk(&widget) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref())
}

/// The footer of the page whose form is open, as a `gtk::Box`. Named from the WINDOW ROOT: `show_insert_form`
/// asks for it while drawing the form, so a walk starting at the form's own holder could not reach it — the
/// footer is the form's SIBLING, one level up. Four tabs each carry one; the window root is still the only
/// place that finds any of them, which is how every other per-window reader here works.
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

/// Show the heading's title and the footer, or drop back to the idle column. One door so the pinned parts
/// cannot disagree about whether a form is open: the caller says what is open (or `None`), and this decides
/// what the frame looks like. Called from the paths that already open and close a form.
pub fn set_pinned(window: &adw::ApplicationWindow, heading: Option<&str>) -> bool {
    // The column's own pinned parts. An INSERT is not one of them: its form and its footer are their own boxes
    // below the frame (§S4 draws the form in place of the readings), so `set_footer` owns that pair and this
    // door must not hide it while a form is drawing.
    let insert_open = visible_insert_form_holder(window).is_some();
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
                    let mut old: Vec<gtk::Widget> = Vec::new();
                    for n in 0..footer.observe_children().n_items() {
                        if let Some(child) = footer.observe_children().item(n) {
                            if let Ok(widget) = child.downcast::<gtk::Widget>() {
                                old.push(widget);
                            }
                        }
                    }
                    for widget in old {
                        footer.remove(&widget);
                    }
                    footer.set_visible(false);
                }
            }
            set_readings(window, true);
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
