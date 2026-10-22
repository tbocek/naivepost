//! The `04-prompt-picker` screen (§04-prepare.md §1): the bench's row picker LIST, as
//! `spec/img/04-prompt-picker.png` draws it — one column of row names with a ✓ before the row the bench is
//! open on. Split out of `snapshot.rs`, which is past the size another staging block fits in.
//!
//! WHY this paints a copy instead of popping the live widget: a `gtk::DropDown` shows its rows in a `GtkPopover`
//! on its OWN surface, so the window's render node never contains them, and the popover's list is a private
//! child (`GtkDropDownPopover` exposes no handle to its box). Popping it for the shot therefore changes nothing
//! the picture can show — an earlier attempt left `shots/04-prompt-picker.png` byte-identical to
//! `04-prepare.png`. So this builds the list from [`bench::picker_rows`], the SAME function the live DropDown is
//! built from (one source of truth: reorder or drop a prompt and this shot changes with it), and paints it on its
//! own mapped top level — the `snapshot_effect_menu::write_png` / `snapshot_face::write_face_png` precedent, for
//! the same reason: a widget whose real home is a separate surface gives no legible pixels inside a window shot.
//!
//! Nothing here decides a rule: the row names and their order come from `bench`, and the tick marks the row the
//! bench is actually open on, read back off that window's own picker face rather than restated.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::bench;
use crate::ui;

/// Width of the painted list. Wide enough for the longest row ("Editing policy", then "User Context") plus the
/// ✓ and a button's padding; measured heights are used, widths cannot be (a menu's text width ignores its icon).
const LIST_SHOT_W: i32 = 400;
/// §1's window size, which every shell screenshot is taken at; restoring it after this module's own ask is
/// what keeps the page sidecar beside the list at that size.
const SHELL_SHOT_W: i32 = 1240;
const SHELL_SHOT_H: i32 = 900;

/// Is this screen's SUBJECT the picker list? §1's second picture (`spec/img/04-prompt-picker.png`) is the open
/// list, not the page behind it, so the list takes `<name>.png` and the Prepare page goes beside it as
/// `<name>-page.png` — the reading `snapshot_subjects` applies to a dialog or a popover the spec pictures.
pub fn is_picker_subject(screen: &str) -> bool {
    screen == "04-prompt-picker"
}

/// Paint this window's bench row list into `out`, on its own mapped top level. The fallback for a build
/// where the page-with-panel shot cannot be made; returns how many rows were drawn and one line naming the
/// row that carries the tick.
///
/// The chosen row comes off the live widget: the DropDown's own face says which row the bench opened on, so the
/// tick cannot disagree with the heading beside it in the page shot.
pub fn paint(window: &adw::ApplicationWindow, out: &Path) -> Result<(usize, String), String> {
    let chosen = chosen_row(window)?;
    let list = picker_list(chosen);
    pin_light_theme();
    // The frame is taken at the LIST's size, never at the window's ask: `write_png` draws at whatever
    // `set_shot_size` last asked for, and the whole-window 1240x900 was still in that slot here, which
    // stretched 14 rows over a window-sized canvas (`snapshot_bench::paint_heading` learned the same).
    // Height off the rows (+ padding), as `snapshot_effect_menu::write_png`: a guessed height clips the
    // last row, and the clip is invisible in the exit code.
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

/// Draw the list as the compact panel §1's picture shows it: hung under the picker button, over the
/// Prepare page, and the whole window written.
///
/// A `gtk::DropDown` keeps its rows in a `GtkPopover`, and GTK will not hold that popover open for a
/// headless frame — measured here: it stays `visible=false` through `present()` and through the button's
/// own `toggled`, and gtk4-rs 0.11 exposes neither `DropDown::popover` nor a `native` toggle to force it
/// inside the window's surface. So the SAME list the dropdown is built from (`bench::picker_rows`) is
/// painted at its measured size and blitted where the popover would sit: right-aligned under the picker's
/// own box. Nothing about the rows is restated — names, order and tick come from the live state.
pub fn paint_over_page(window: &adw::ApplicationWindow, out: &Path) -> Result<(usize, String), String> {
    let chosen = chosen_row(window)?;
    let picker = ui::find_source_widget(window, "bench-picker")
        .ok_or("the Prepare page drew no bench-picker")?;
    // Preferred where GTK allows it: open the live dropdown and shoot the window, so the panel IS the
    // widget's own popover.
    if let Some(popover) = popover_under(&picker) {
        let result = crate::snapshot_face::write_png(window.upcast_ref(), out);
        popover.popdown();
        return result.map(|()| {
            (
                bench::picker_rows().len(),
                format!("04-prompt-picker tick on row {chosen} ({})", row_name(chosen)),
            )
        });
    }
    // The reference is the PAGE with a compact panel hanging under the picker button. GTK will not keep
    // this dropdown's popover open for a headless frame, so the panel is drawn where the popover would
    // sit — the live list, at its measured size, right-aligned under the picker's own box — over a
    // snapshot of the real window.
    let bounds = picker.compute_bounds(window.upcast_ref::<gtk::Widget>()).unwrap();
    let panel_bounds = (bounds.x(), bounds.y(), bounds.width(), bounds.height());
    let list = picker_list(chosen);
    pin_light_theme();
    list.set_halign(gtk::Align::Fill);
    list.set_hexpand(true);
    let width = LIST_SHOT_W.min(bounds.width() as i32 * 2).max(220);
    // All fourteen rows, as §1's caption says ("The list scrolls: Translate and Upload text sit below
    // Narration") — the panel is as tall as its own content, only clamped inside the window.
    let height = list.measure(gtk::Orientation::Vertical, -1).3 + 12;    let holder = gtk::Window::new();
    holder.set_decorated(false);
    holder.set_default_size(width, height);
    holder.set_child(Some(&list));
    holder.present();
    crate::snapshot_face::pump_context(64);
    // The page behind must be laid out before it can be shot: this holder has just taken the seatbelts off
    // the main window's mapping, and a window that was never allocated renders as an empty plate.
    crate::snapshot_face::pump_context(64);
    let painted = composite_panel(window, &list, width, height, panel_bounds, out);
    holder.close();
    painted.map(|()| {
        (
            bench::picker_rows().len(),
            format!("04-prompt-picker tick on row {chosen} ({})", row_name(chosen)),
        )
    })
}

/// Snapshot the window, then blit the list onto it under the picker button and write the result.
fn composite_panel(
    window: &adw::ApplicationWindow,
    list: &gtk::Box,
    width: i32,
    height: i32,
    picker_bounds: (f32, f32, f32, f32),
    out: &Path,
) -> Result<(), String> {
    let (asked_w, asked_h) = crate::snapshot_face::asked_shot_wh();
    let (w, h) = if asked_w > 0 && asked_h > 0 {
        (asked_w, asked_h)
    } else {
        (window.width(), window.height())
    };
    // Ask for exactly this canvas while the frame is taken: `shot_size` answers with whatever ask is in
    // the slot, and leaving the window's own ask there would stretch or crop the page underneath.
    crate::snapshot_face::set_shot_size(w, h);
    let surface = crate::snapshot_face::render_surface(window.upcast_ref(), w, h)?;
    let cr = cairo::Context::new(&surface).map_err(|err| err.to_string())?;
    let panel = crate::snapshot_face::render_surface(list.upcast_ref(), width, height)?;
    // Right edge under the picker's right edge, top just below the button, clamped inside the window.
    let (px, py, pw, _) = picker_bounds;
    let x = (px + pw - width as f32).max(0.0);
    let y = (py + picker_bounds.3)
        .min(h as f32 - height as f32)
        .max(0.0);
    cr.set_source_surface(&panel, x.floor() as f64, y.floor() as f64)
        .map_err(|err| err.to_string())?;
    cr.paint().map_err(|err| err.to_string())?;
    let mut file = std::fs::File::create(out).map_err(|err| err.to_string())?;
    surface.write_to_png(&mut file).map_err(|err| err.to_string())
}

/// The dropdown's own popover, if this build will show it: GTK builds the popover with the dropdown, so
/// the only leg is making it visible and letting the window paint. Measured on gtk4-rs 0.11 / GTK 4.14:
/// there is no `DropDown::popover`, no `GtkDropDown:popover` property and no `GtkPopover:native` (the
/// native toggle was removed when popovers moved in-tree), and a bare `present()` leaves the popover
/// unmapped — so today [`paint_over_page`] composites the same rows instead.
fn popover_under(picker: &gtk::Widget) -> Option<gtk::Popover> {
    let found = popover_window_of(picker)?;
    found.present();
    glib::MainContext::default().iteration(false);
    if found.is_visible() {
        return Some(found);
    }
    for button in buttons_of(picker) {
        // A DropDown holds one button; emitting its `toggled` is what opens its list — the same leg a
        // press takes, without needing a pointer.
        button.emit_by_name::<()>("toggled", &[]);
        glib::MainContext::default().iteration(false);
        if found.is_visible() {
            return Some(found);
        }
    }
    None
}

/// What opening this dropdown offers: how many buttons the picker holds and whether a popover appeared.
/// Printed by the snapshot so a fallback to the list alone says WHICH leg failed.
pub fn probe(picker: &gtk::Widget) -> String {
    format!(
        "{} button(s) under the picker; popover {} before opening, {} shown by the live widget",
        buttons_of(picker).len(),
        if popover_window_of(picker).is_some() { "there" } else { "absent" },
        if popover_under(picker).is_some() { "can be" } else { "cannot be" }
    )
}

/// A `gtk::Popover` among `root`'s own children — the dropdown keeps its own popover as a direct child.
fn child_popover(root: &gtk::Widget) -> Option<gtk::Popover> {
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        if let Ok(popover) = child.downcast::<gtk::Popover>() {
            return Some(popover);
        }
    }
    None
}

/// The dropdown's own popover: its own child, or one of its button's. (Walking the whole window instead
/// lands on an unrelated control's popover — the Freq dropdown's — which is why it never showed.)
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

/// Every button under `root`, the dropdown's toggle among them.
fn buttons_of(root: &gtk::Widget) -> Vec<gtk::Button> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Ok(button) = widget.clone().downcast::<gtk::Button>() {
            out.push(button);
        }
        out.extend(buttons_of(&widget));
    }
    out
}

/// Paint `list` on a throwaway mapped top level of its own.
fn paint_on_top_level(list: &gtk::Box, out: &Path) -> Result<(), String> {
    let holder = gtk::Window::new();
    // A bare `gtk::Window`, deliberately: an `adw::Window` + global `StyleManager::set_color_scheme(ForceLight)`
    // was tried and gave "window produced no render node" — an `adw::Window` only lays out inside an
    // `adw::ApplicationWindow`, and this top level belongs to no application. Forcing the scheme globally would
    // also repaint the PAGE sidecar, which must keep the theme the app really draws in.
    // The reference (`spec/img/04-prompt-picker.png`) is a LIGHT-theme picture; this container's headless GTK
    // defaults to dark, so the rows came out as pale text on black and could not be read beside their reference.
    // So the colours are pinned HERE, on this widget only, at USER priority (the level user CSS sits at, above
    // the theme): the box gets the light background and dark ink of the reference, nothing else changes.
    pin_light_theme();
    // The box must take the top level's whole width: a `gtk::Box` shrink-wraps to its children's natural width,
    // so with centred rows it became a narrow column of fragments and the background never reached the edges.
    list.set_halign(gtk::Align::Fill);
    list.set_hexpand(true);
    holder.set_decorated(false);
    holder.set_default_size(LIST_SHOT_W, list.measure(gtk::Orientation::Vertical, -1).3 + 12);
    holder.set_child(Some(list));
    holder.present();
    // The map must happen before the frame is taken; `write_png` reads the widget's allocated size.
    crate::snapshot_face::pump_context(64);
    let result = crate::snapshot_face::write_png(list.upcast_ref(), out);
    holder.close();
    result
}

/// Build the row list: one flat button per bench row, the tick on the row the bench is open on.
fn picker_list(chosen: usize) -> gtk::Box {
    let rows = bench::picker_rows();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.set_widget_name("bench-picker-list");
    // A CSS class as well as a name, because the theme pin below selects on it: `widget-name` is not a CSS
    // selector in GTK 4 (`#name` matches widget names, but a provider added to one widget must still match it).
    list.add_css_class("bench-picker-list");
    for (index, title) in &rows {
        // A row of a menu is a flat button: full width, text at the start, exactly as GTK's own dropdown draws
        // its rows. A plain label would centre and shrink-wrap into a ragged column.
        let row = gtk::Button::with_label(&row_label(*index, title, chosen));
        row.set_widget_name(&format!("bench-picker-row-{index}"));
        // Fill + start: with the default centred alignment every row collapsed to its own text and printed as a
        // ragged column of fragments with empty space to their right (the same lesson `snapshot_effect_menu` took).
        row.set_halign(gtk::Align::Fill);
        row.set_hexpand(true);
        // The label is what needs the left alignment — a button does not pass one on to its own label.
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

/// The light colours of the reference picture, pinned on this widget only, at USER priority.
fn pin_light_theme() {
    let theme = gtk::CssProvider::new();
    theme.load_from_string(
        "window { background-color: #fafafa; }\n\
         box.bench-picker-list { background-color: #fafafa; color: #1a1a1a; }\n\
         box.bench-picker-list button { color: #1a1a1a; background-color: transparent; }\n\
         /* The reference marks its chosen row with a highlighted band, as GTK's own dropdown does; the check mark
            alone read as plain text and did not say which row is open. */\n\
         box.bench-picker-list button.picked { background-color: #d3e3f8; }\n",
    );
    // On the DISPLAY (GTK 4's answer, since `WidgetExt::style_context` was deprecated in 4.10). The rules select
    // on `window` and on `.bench-picker-list`, so only this throwaway top level and its rows take these colours —
    // the page sidecar, painted from the real window, keeps the theme the app really draws in.
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &theme,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );
    }
}

/// The name of the picker's row `index`, from the same list the menu is built from.
fn row_name(index: usize) -> String {
    bench::picker_rows()
        .into_iter()
        .find(|(at, _)| *at == index)
        .map(|(_, name)| name)
        .unwrap_or_else(|| "?".to_string())
}

/// The line a row wears: the chosen one carries a ✓ before its name, as `spec/img/04-prompt-picker.png` marks
/// the row its picker face shows ("User Context" there, because that page was shot on row 0). That is GTK's own
/// convention for a dropdown's selected row, so the mark needs no new string — and it follows whichever row THIS
/// shot opened on, which §1's other picture (`04-prepare.png`, the Describe prompt open) puts on row 2.
fn row_label(index: usize, title: &str, chosen: usize) -> String {
    if index == chosen {
        return format!("\u{2713} {title}");
    }
    title.to_string()
}

/// Which row this window's bench is open on, read off the picker's face. `bench_box` sets the face from the same
/// state it paints the heading from, so the two cannot disagree, and no new state slot was invented for the shot.
fn chosen_row(window: &adw::ApplicationWindow) -> Result<usize, String> {
    ui::find_source_widget(window, "bench-picker")
        .ok_or("the Prepare page drew no bench-picker")?
        .downcast::<gtk::DropDown>()
        .map_err(|_| "bench-picker is not a dropdown".to_string())
        .map(|picker| usize::try_from(picker.selected()).unwrap_or(0))
}
