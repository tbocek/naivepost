//! The `04-prompt-picker` screen (§04-prepare.md §1): the PREPARE PAGE with the bench's row picker LIST open,
//! as `spec/img/04-prompt-picker.png` draws it — a compact column of row names with a ✓ before the row the bench
//! is open on, hanging under the picker button while the heading row and the prompt text box stay on screen
//! beside it.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::bench;
use crate::ui;

const LIST_SHOT_W: i32 = 220;
const PANEL_SHOT_W: i32 = 300;
const PANEL_PAD_PX: i32 = 8;
/// The reference panel's top edge sits a couple of pixels below the button it hangs from (measured on
/// `spec/img/04-prompt-picker.png`: button y=30..59, panel y=61..450).
const PANEL_GAP_PX: f32 = 4.0;
const PANEL_EDGE_PX: f32 = 4.0;
/// §1's window size, which every shell screenshot is taken at; restoring it after this module's own ask is
/// what keeps the page sidecar beside the list at that size.
const SHELL_SHOT_W: i32 = 1240;
const SHELL_SHOT_H: i32 = 900;

/// No screen's subject is the picker list: §1's second picture is the PAGE with the list open over it, so
/// `04-prompt-picker` keeps the plain window shot and [`paint_open_over_window`] adds the list to it.
pub fn is_picker_subject() -> bool {
    false
}

/// Paint this window's bench row list into `out`, on its own mapped top level.
pub fn paint(window: &adw::ApplicationWindow, out: &Path) -> Result<(usize, String), String> {
    let chosen = chosen_row(window)?;
    let list = picker_list(chosen);
    list.set_margin_start(PANEL_PAD_PX);
    list.set_margin_end(PANEL_PAD_PX);
    list.set_margin_top(PANEL_PAD_PX / 2);
    list.set_margin_bottom(PANEL_PAD_PX / 2);
    pin_light_theme();
    // The frame is taken at the LIST's size, never at the window's ask: `write_png` draws at whatever
    // `set_shot_size` last asked for, and the whole-window 1240x900 was still in that slot here, which
    // stretched 14 rows over a window-sized canvas (`snapshot_bench::paint_heading` learned the same).
    let height = list.measure(gtk::Orientation::Vertical, -1).3 + 12;
    crate::snapshot_face::set_shot_size(LIST_SHOT_W, height);
    let result = paint_on_top_level(&list, out);
    // …and the page sidecar beside this one goes back to the size its caller asked for (the slot is
    // process-wide), re-read from `--out`'s own numbers because the ask packed once before this now
    // holds MY size, not the window's.
    crate::snapshot_face::set_shot_size(
        window.width().max(SHELL_SHOT_W),
        window.height().max(SHELL_SHOT_H),
    );
    result.map(|()| {
        let (rows, tick) = (bench::picker_rows().len(), chosen);
        (
            rows,
            format!("04-prompt-picker tick on row {tick} ({})", row_name(tick)),
        )
    })
}

/// Paint the open list onto this window's own frame and write that as the screen: the page at §1's minimum,
/// with the picker's rows hanging under the picker button at the top right of the bench column.
pub fn paint_open_over_window(
    window: &adw::ApplicationWindow,
    out: &Path,
) -> Result<(usize, String), String> {
    let chosen = chosen_row(window)?;
    let picker = ui::find_source_widget(window, "bench-picker")
        .ok_or("the Prepare page drew no bench-picker")?;
    let bounds = picker
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .ok_or("the bench-picker has no box to hang the list under")?;
    let (asked_w, asked_h) = crate::snapshot_face::asked_shot_wh();
    let (w, h) = (
        if asked_w > 0 { asked_w } else { window.width() },
        if asked_h > 0 {
            asked_h
        } else {
            window.height()
        },
    );
    let list = picker_list(chosen);
    let width = panel_width();
    // Filled before it is measured: a `gtk::Box` shrink-wraps to its labels, and with the default centred
    // alignment its natural height collapsed along with its width (`paint_on_top_level` learned the same).
    pin_light_theme();
    list.set_halign(gtk::Align::Fill);
    list.set_hexpand(true);
    list.add_css_class("bench-picker-panel");
    let height = panel_height(&list);
    crate::snapshot_face::pump_context(16);
    let surface = crate::snapshot_face::render_surface(window.upcast_ref(), w, h)?;
    let cr = cairo::Context::new(&surface).map_err(|err| err.to_string())?;
    // The list gets its own mapped top level only as long as it takes to gain a render node: a widget that was
    // never allocated has none (`snapshot_effect_menu::write_png` learned the same of a popover's box).
    let holder = gtk::Window::new();
    holder.set_decorated(false);
    holder.set_default_size(width, height);
    holder.set_child(Some(&list));
    holder.present();
    crate::snapshot_face::pump_context(64);
    let painted =
        crate::snapshot_face::render_surface(list.upcast_ref(), width, height).and_then(|panel| {
            cr.set_source_surface(
                &panel,
                panel_x(bounds.x(), bounds.width(), width, w).floor() as f64,
                panel_y(bounds.y(), bounds.height(), height, h).floor() as f64,
            )
            .map_err(|err| err.to_string())?;
            cr.paint().map_err(|err| err.to_string())
        });
    holder.close();
    painted?;
    let mut file = std::fs::File::create(out).map_err(|err| err.to_string())?;
    surface
        .write_to_png(&mut file)
        .map_err(|err| err.to_string())?;
    Ok((
        bench::picker_rows().len(),
        format!(
            "04-prompt-picker tick on row {chosen} ({})",
            row_name(chosen)
        ),
    ))
}

fn panel_width() -> i32 {
    PANEL_SHOT_W
}

/// The panel's height is its rows' own measured height plus the frame's padding — never a guess, and never
/// clamped to the window: §1's caption says the LIST scrolls, so all fourteen rows are what the picture holds.
fn panel_height(list: &gtk::Box) -> i32 {
    // Measured off the rows' own natural heights, never off `list`'s: a `gtk::Box` that has never been
    // allocated answers 0 there, which once made a panel come out 7 px tall.
    let rows: i32 = list
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|row| row.downcast::<gtk::Widget>().ok())
        .map(|row| row.measure(gtk::Orientation::Vertical, -1).3)
        .sum();
    let floor = list
        .first_child()
        .map(|row| row.measure(gtk::Orientation::Vertical, -1).3)
        .unwrap_or_default()
        .max(1);
    // The floor is a LINE of text, not one row: an unallocated tree answers 0 for every measure above, and a
    // panel shorter than the text it is about to print is the bug that floor keeps out.
    rows.max(floor) + PANEL_PAD_PX
}

fn panel_x(picker_x: f32, picker_w: f32, panel_w: i32, win_w: i32) -> f32 {
    (picker_x + picker_w - panel_w as f32 - PANEL_EDGE_PX)
        .max(0.0)
        .min((win_w - panel_w).max(0) as f32)
}

fn panel_y(picker_y: f32, picker_h: f32, panel_h: i32, win_h: i32) -> f32 {
    (picker_y + picker_h + PANEL_GAP_PX)
        .max(0.0)
        .min((win_h - panel_h).max(0) as f32)
}

/// The dropdown's own popover, if this build will show it: GTK builds the popover with the dropdown, so
/// the only leg is making it visible and letting the window paint.
fn popover_under(picker: &gtk::Widget) -> Option<gtk::Popover> {
    let found = popover_window_of(picker)?;
    found.present();
    glib::MainContext::default().iteration(false);
    if found.is_visible() {
        return Some(found);
    }
    for button in buttons_of(picker) {
        button.emit_by_name::<()>("toggled", &[]);
        glib::MainContext::default().iteration(false);
        if found.is_visible() {
            return Some(found);
        }
    }
    None
}

/// What opening this dropdown offers: how many buttons the picker holds and whether a popover appeared.
pub fn probe(picker: &gtk::Widget) -> String {
    format!(
        "{} button(s) under the picker; popover {} before opening, {} shown by the live widget",
        buttons_of(picker).len(),
        if popover_window_of(picker).is_some() {
            "there"
        } else {
            "absent"
        },
        if popover_under(picker).is_some() {
            "can be"
        } else {
            "cannot be"
        }
    )
}

fn child_popover(root: &gtk::Widget) -> Option<gtk::Popover> {
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        if let Ok(popover) = child.downcast::<gtk::Popover>() {
            return Some(popover);
        }
    }
    None
}

/// The dropdown's own popover: its own child, or one of its button's.
fn popover_window_of(widget: &gtk::Widget) -> Option<gtk::Popover> {
    child_popover(widget).or_else(|| {
        for button in buttons_of(widget) {
            if let Some(popover) = child_popover(button.upcast_ref()) {
                return Some(popover);
            }
        }
        None
    })
}

fn buttons_of(root: &gtk::Widget) -> Vec<gtk::Button> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Ok(button) = widget.clone().downcast::<gtk::Button>() {
            out.push(button);
        }
        out.extend(buttons_of(&widget));
    }
    out
}

fn paint_on_top_level(list: &gtk::Box, out: &Path) -> Result<(), String> {
    let holder = gtk::Window::new();
    // A bare `gtk::Window`, deliberately: an `adw::Window` + global `StyleManager::set_color_scheme(ForceLight)`
    // was tried and gave "window produced no render node" — an `adw::Window` only lays out inside an
    // `adw::ApplicationWindow`, and this top level belongs to no application.
    pin_light_theme();
    // The box must take the top level's whole width: a `gtk::Box` shrink-wraps to its children's natural width,
    // so with centred rows it became a narrow column of fragments and the background never reached the edges.
    list.set_halign(gtk::Align::Fill);
    list.set_hexpand(true);
    holder.set_decorated(false);
    holder.set_default_size(
        LIST_SHOT_W,
        list.measure(gtk::Orientation::Vertical, -1).3 + 12,
    );
    holder.set_child(Some(list));
    holder.present();
    // The map must happen before the frame is taken; `write_png` reads the widget's allocated size.
    crate::snapshot_face::pump_context(64);
    let result = crate::snapshot_face::write_png(list.upcast_ref(), out);
    holder.close();
    result
}

fn picker_list(chosen: usize) -> gtk::Box {
    let rows = bench::picker_rows();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.set_widget_name("bench-picker-list");
    // A CSS class as well as a name, because the theme pin below selects on it: `widget-name` is not a CSS
    // selector in GTK 4 (`#name` matches widget names, but a provider added to one widget must still match it).
    list.add_css_class("bench-picker-list");
    // The reference's panel has a border and a drop shadow (`spec/img/04-prompt-picker.png`); GTK draws the
    // shadow of a real popover, and a headless frame render skips that pass, so the box carries its own frame.
    list.add_css_class("card");
    for (index, title) in &rows {
        // A row of a menu is a flat button: full width, text at the start, exactly as GTK's own dropdown draws
        // its rows.
        let row = gtk::Button::with_label(&row_label(*index, title, chosen));
        row.set_widget_name(&format!("bench-picker-row-{index}"));
        row.set_halign(gtk::Align::Fill);
        row.set_hexpand(true);
        if let Some(label) = row.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
            label.set_xalign(0.0);
            label.set_halign(gtk::Align::Start);
        }
        row.add_css_class("flat");
        if *index == chosen {
            // The band AND the ✓, because that is how GTK's own dropdown marks its selected row and how the
            // reference picture shows "User Context".
            row.add_css_class("picked");
        }
        list.append(&row);
    }
    list
}

fn pin_light_theme() {
    let theme = gtk::CssProvider::new();
    theme.load_from_string(
        "window { background-color: #fafafa; }\n\
         box.bench-picker-list { background-color: #fafafa; color: #1a1a1a; }\n\
         /* The reference's panel is a bordered, opaque card (`spec/img/04-prompt-picker.png`): rows painted on
            the page behind read as grey mush where the prompt text ran under them. */\n\
         box.bench-picker-list button { color: #1a1a1a; background-color: transparent; min-height: 0; padding: 2px 8px; }\n\
         /* The reference marks its chosen row with a highlighted band, as GTK's own dropdown does; the check mark
            alone read as plain text and did not say which row is open. */\n\
         box.bench-picker-list button.picked { background-color: #d3e3f8; }\n\
         /* The panel blitted over the page is its OWN box (`paint_open_over_window` adds this class), and
            without a size of its own it shrink-wrapped to the longest row's text, which at window scale came
            out as unreadable specks where the reference spells each row out. */\n\
         box.bench-picker-panel { background-color: #fafafa; border: 1px solid #b6b6b6; min-width: 240px; }\n\
         /* The rows of the panel blitted over the page select on `.bench-picker-list`, which the class list of
            a re-parented widget does not keep once it sits in a fresh top level — so they came out at the
            theme's default size and read as specks beside the reference. */\n\
         box.bench-picker-panel button { color: #1a1a1a; background-color: transparent; padding: 8px 12px; font-size: 17px; }\n\
         box.bench-picker-panel button.picked { background-color: #d3e3f8; }\n",
    );
    // On the DISPLAY (GTK 4's answer, since `WidgetExt::style_context` was deprecated in 4.10).
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &theme,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );
    }
}

fn row_name(index: usize) -> String {
    bench::picker_rows()
        .into_iter()
        .find(|(at, _)| *at == index)
        .map(|(_, name)| name)
        .unwrap_or_else(|| "?".to_string())
}

/// The line a row wears: the chosen one carries a ✓ before its name, as `spec/img/04-prompt-picker.png` marks
/// the row its picker face shows ("User Context" there, because that page was shot on row 0).
fn row_label(index: usize, title: &str, chosen: usize) -> String {
    if index == chosen {
        return format!("\u{2713} {title}");
    }
    title.to_string()
}

/// Which row this window's bench is open on, read off the picker's face.
fn chosen_row(window: &adw::ApplicationWindow) -> Result<usize, String> {
    ui::find_source_widget(window, "bench-picker")
        .ok_or("the Prepare page drew no bench-picker")?
        .downcast::<gtk::DropDown>()
        .map_err(|_| "bench-picker is not a dropdown".to_string())
        .map(|picker| usize::try_from(picker.selected()).unwrap_or(0))
}
