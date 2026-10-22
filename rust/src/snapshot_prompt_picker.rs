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

/// Is this screen's SUBJECT the picker list? §1's second picture (`spec/img/04-prompt-picker.png`) is the open
/// list, not the page behind it, so the list takes `<name>.png` and the Prepare page goes beside it as
/// `<name>-page.png` — the reading `snapshot_subjects` applies to a dialog or a popover the spec pictures.
pub fn is_picker_subject(screen: &str) -> bool {
    screen == "04-prompt-picker"
}

/// Paint this window's bench row list into `out`, on its own mapped top level. Returns how many rows were
/// drawn and one line naming the row that carries the tick, so a clipped list or a tick on the wrong row
/// shows up in the log and not only in the PNG.
///
/// The chosen row comes off the live widget: the DropDown's own face says which row the bench opened on, so the
/// tick cannot disagree with the heading beside it in the page shot.
pub fn paint(window: &adw::ApplicationWindow, out: &Path) -> Result<(usize, String), String> {
    let chosen = chosen_row(window)?;
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
    let holder = gtk::Window::new();
    // A bare `gtk::Window`, deliberately: an `adw::Window` + global `StyleManager::set_color_scheme(ForceLight)`
    // was tried and gave "window produced no render node" — an `adw::Window` only lays out inside an
    // `adw::ApplicationWindow`, and this top level belongs to no application. Forcing the scheme globally would
    // also repaint the PAGE sidecar, which must keep the theme the app really draws in.
    // The reference (`spec/img/04-prompt-picker.png`) is a LIGHT-theme picture; this container's headless GTK
    // defaults to dark, so the rows came out as pale text on black and could not be read beside their reference.
    // So the colours are pinned HERE, on this widget only, at USER priority (the level user CSS sits at, above the
    // theme): the box gets the light background and dark ink of the reference, nothing else changes.
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
    // The box must take the top level's whole width: a `gtk::Box` shrink-wraps to its children's natural width,
    // so with centred rows it became a narrow column of fragments and the background never reached the edges.
    list.set_halign(gtk::Align::Fill);
    list.set_hexpand(true);
    holder.set_decorated(false);
    // Height measured off the rows (+ padding), as `snapshot_effect_menu::write_png` learned: a guessed height
    // clips the last row, and the clip is invisible in the exit code.
    let height = list.measure(gtk::Orientation::Vertical, -1).3;
    holder.set_default_size(LIST_SHOT_W, height + 12);
    holder.set_child(Some(&list));
    holder.present();
    // The map must happen before the frame is taken; `write_png` reads the widget's allocated size.
    crate::snapshot_face::pump_context(64);
    let result = crate::snapshot_face::write_png(list.upcast_ref(), out);
    holder.close();
    result.map(|()| {
        let (rows, tick) = (rows.len(), chosen);
        (
            rows,
            format!("04-prompt-picker tick on row {tick} ({})", row_name(tick)),
        )
    })
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
