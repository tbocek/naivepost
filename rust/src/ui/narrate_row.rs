//! §07-narrate#1-screen: one line row, built once as two stacked things.
//! A header line: time entry (**14**), status with end time (**15**) and fit warning (**21**), then at its right end ▶ (**16**), ↻ (**17**), ＋ (**18**), 🗑 (**19**).
//! Under it, the row's full width: the text box (**20**), `[emotion] words`.
//! Widget names (`line-time-*`, `line-status-*`, ...) are what `wire_row_buttons` and the `narrate_lines` / `narrate_text` seams find.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_screen;
use crate::narration::Entry;

const LINE_HEIGHT_PX: i32 = 24;

/// One row of the lines column for `entry` at sorted position `index`. `fit` is the render's precomputed answer; no text means no warning and a red fit adds `error`.
pub fn build(
    index: usize,
    entry: &Entry,
    session_second: f64,
    fit: narrate_screen::Fit,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("narrate-line-{index}"));
    let stack = gtk::Box::new(gtk::Orientation::Vertical, 2);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.set_widget_name(&format!("line-header-{index}"));
    // §A.5-1: the time is an ENTRY (`press_line_move` reads it on Enter); seven chars holds "mm:ss.s", and a max width stops it taking the status's room.
    let time = gtk::Entry::new();
    time.set_widget_name(&format!("line-time-{index}"));
    time.set_width_chars(7);
    time.set_max_width_chars(7);
    time.set_hexpand(false);
    time.set_text(&narrate_screen::time_field(session_second));
    time.set_tooltip_text(Some(crate::ui::narrate_lines::TIME_FIELD_TIP));
    header.append(&time);

    let status = gtk::Label::new(Some(&narrate_screen::status_line(narrate_screen::status(
        entry, false,
    ))));
    status.set_widget_name(&format!("line-status-{index}"));
    // Fixed parts come first; a status that cannot fit ellipsizes rather than shoving them out.
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header.append(&status);

    // **21**: the fit warning, appended to the status before the spacer.
    let warning = gtk::Label::new(Some(&narrate_screen::fit_warning(fit).unwrap_or_default()));
    warning.set_widget_name(&format!("line-warning-{index}"));
    warning.set_xalign(0.0);
    // Ellipsized like the status; the full sentence stays in the tooltip.
    warning.set_ellipsize(gtk::pango::EllipsizeMode::End);
    if narrate_screen::row_is_red(fit) {
        warning.add_css_class("error");
        // A.5 marks the row red, not the warning alone.
        row.add_css_class("error");
    }
    header.append(&warning);

    // The gap that keeps the four buttons at the right end; named so a check can tell it from a filler.
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_widget_name(&format!("line-header-gap-{index}"));
    spacer.set_hexpand(true);
    header.append(&spacer);

    for (suffix, label, tip) in [
        ("speak", "\u{25b6}", "speak this line"),
        ("reroll", "\u{27f3}", "a new take, same words"),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_widget_name(&format!("line-{suffix}-{index}"));
        button.add_css_class("media-button");
        button.set_tooltip_text(Some(tip));
        header.append(&button);
    }
    let below = gtk::Button::from_icon_name(narrate_screen::ADD_LINE_ICON);
    below.set_widget_name(&format!("line-add-below-{index}"));
    below.set_tooltip_text(Some("Add a line below this one"));
    header.append(&below);
    let remove = gtk::Button::with_label("\u{1f5d1}");
    remove.set_widget_name(&format!("line-remove-{index}"));
    remove.set_tooltip_text(Some("remove this line"));
    header.append(&remove);

    stack.append(&header);

    // **20**: the text box under the header, wrapping; a `TextView` because an `Entry` cannot wrap.
    let text = gtk::TextView::new();
    crate::ui::margins::text_box(&text);
    text.set_widget_name(&format!("line-text-{index}"));
    text.set_wrap_mode(gtk::WrapMode::Word);
    text.add_css_class("monospace");
    text.add_css_class("line-text");
    text.set_size_request(-1, 2 * LINE_HEIGHT_PX);
    // Needs an explicit focusable, unlike an `Entry`.
    text.set_cursor_visible(true);
    text.set_can_focus(true);
    text.set_editable(true);
    set_box_text(&text, &narrate_screen::write_box(entry));
    text.set_tooltip_text(Some(&narrate_screen::row_tooltip()));
    text.set_hexpand(true);
    text.set_halign(gtk::Align::Fill);
    stack.append(&text);

    row.set_child(Some(&stack));
    row
}

pub fn box_text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, true).trim_end().to_string()
}

/// Put `text` in a row's text box; every writer goes through here.
pub fn set_box_text(view: &gtk::TextView, text: &str) {
    view.buffer().set_text(text);
    let start = view.buffer().start_iter();
    view.buffer().place_cursor(&start);
}
