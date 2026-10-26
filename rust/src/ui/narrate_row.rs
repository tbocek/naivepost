//! §07-narrate#1-screen — one line row's shape, built once.
//!
//! §1 spells a row as TWO stacked things, and the badge numbers say where each sits:
//!  - a HEADER LINE, left to right: time entry (**14**), status with the end time (**15**) and the fit
//!    warning appended to it (**21**), then AT THE HEADER LINE'S RIGHT END ▶ speak this line (**16**),
//!    ↻ re-roll (**17**), ＋ add a line below (**18**), 🗑 remove (**19**);
//!  - UNDER the header line, THE ROW'S FULL WIDTH: the text box (**20**), `[emotion] words`.
//!
//! §A.5 says it the same way: "Warnings appended and the row marked red" — appended to the status, on the
//! header line, not standing under the words as a third row of its own. Before this module all five widgets
//! went into ONE horizontal box, so the text box was squeezed onto the header line and every row ran off the
//! page's right edge instead of wrapping under it. The row is now a vertical box of exactly two children:
//! header on top, wrapped text box under it.
//!
//! It lives in its own file because `narrate_page.rs` is past its size budget; the page keeps the call site
//! and this file keeps the shape. Every widget name is unchanged (`line-time-*`, `line-status-*`,
//! `line-speak-*`, `line-reroll-*`, `line-add-below-*`, `line-remove-*`, `line-text-*`, `line-warning-*`),
//! so `wire_row_buttons` and the seams in `narrate_lines` / `narrate_text` find what they always found.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_screen;
use crate::narration::Entry;

/// A.5's "1–3 lines tall": two lines as the resting height, the picture's rows wrap across two.
const LINE_HEIGHT_PX: i32 = 24;

/// One row of the lines column, for `entry` at sorted position `index`.
///
/// `fit` is the row's already-computed answer from the render's own ladder (the page asks
/// `mirror_fit` over every line of the clip together, which a row cannot do for itself), so this builder
/// only renders it: no text means no warning, and a red fit adds the `error` class.
pub fn build(
    index: usize,
    entry: &Entry,
    session_second: f64,
    fit: narrate_screen::Fit,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("narrate-line-{index}"));
    let stack = gtk::Box::new(gtk::Orientation::Vertical, 2);

    // --- the header line: 14, 15, then the four buttons at its right end: 16, 17, 18, 19 --------------
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.set_widget_name(&format!("line-header-{index}"));
    // §A.5-1: the row's time is an ENTRY, not a label — typing a second in it is how a line is moved, and
    // `press_line_move` reads this widget on Enter. Seven chars holds "mm:ss.s" and no more: an entry with
    // no max width asks for a natural width past what it can ever hold and takes the row from the status.
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
    // The header's fixed parts (the time field and the four buttons) come first; a status that cannot fit
    // ellipsizes rather than shoving them out of the row. §A.5 puts the end time on this line, not wrapped under it.
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header.append(&status);

    // --- **21**: the fit warning, APPENDED to the status on the same header line ----------------------
    // §A.5: "Warnings appended and the row marked red" — appended after the end time, before the spacer that
    // holds the buttons at the header's right end, so a long warning shoves nothing out of the row.
    let warning = gtk::Label::new(Some(&narrate_screen::fit_warning(fit).unwrap_or_default()));
    warning.set_widget_name(&format!("line-warning-{index}"));
    warning.set_xalign(0.0);
    // Same rule as the status: the red fit sentence is long ("⚠ ~11 s of speech, 8 s before the clip ends", or
    // the longer "run N s past it…" one), and at its full width it pushed ▶ ↻ ＋ 🗑 past the column's right edge.
    // Ellipsizing keeps §1's order — the buttons stay at the header line's RIGHT END — and the whole sentence is
    // still readable in the row's tooltip.
    warning.set_ellipsize(gtk::pango::EllipsizeMode::End);
    if narrate_screen::row_is_red(fit) {
        warning.add_css_class("error");
        // A.5 marks THE ROW red, not the warning alone.
        row.add_css_class("error");
    }
    header.append(&warning);

    // The gap that pushes the four buttons to the header's RIGHT END (§1's words), rather than leaving them
    // packed behind the status where a wide warning would shove them out of the row. Named so a check can
    // tell it apart from an unnamed filler and still read §1's six widgets in their own order.
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
        button.set_tooltip_text(Some(tip));
        header.append(&button);
    }
    // §1's note on the prototype: the row's ＋ was the full-width sign U+FF0B, which only CJK fonts carry and
    // which draws as a missing-glyph box anywhere else. The rewrite uses the list-add icon.
    let below = gtk::Button::from_icon_name(narrate_screen::ADD_LINE_ICON);
    below.set_widget_name(&format!("line-add-below-{index}"));
    below.set_tooltip_text(Some("Add a line below this one"));
    header.append(&below);
    let remove = gtk::Button::with_label("\u{1f5d1}");
    remove.set_widget_name(&format!("line-remove-{index}"));
    remove.set_tooltip_text(Some("remove this line"));
    header.append(&remove);

    stack.append(&header);

    // --- **20**: the text box, UNDER the header line, across the row's whole width, WRAPPING ----------
    // A.5 calls the box "monospace, word wrap, 1–3 lines tall", which an `Entry` cannot do (one line, no
    // wrap), so it is a `TextView`: monospace by CSS class like its `Entry` predecessor, wrapped at word
    // boundaries, and two lines tall at rest so a long line wraps in place instead of running off the page.
    let text = gtk::TextView::new();
    crate::ui::margins::text_box(&text);
    text.set_widget_name(&format!("line-text-{index}"));
    text.set_wrap_mode(gtk::WrapMode::Word);
    text.add_css_class("monospace");
    text.add_css_class("line-text");
    text.set_size_request(-1, 2 * LINE_HEIGHT_PX);
    // A.5's box is where the words are typed, so it must take a caret and the keyboard focus — an `Entry`
    // did that by itself, a `TextView` needs saying.
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

/// The words currently in a row's text box, without the newline a `TextView`'s buffer keeps at its end.
pub fn box_text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, true).trim_end().to_string()
}

/// Put `text` in a row's text box — the door every writer of the box goes through, this file's own build
/// included, so no caller has to know that a `TextView`'s words live on its buffer.
pub fn set_box_text(view: &gtk::TextView, text: &str) {
    // A rebuild must not leave the caret sitting past the end of shorter new words.
    view.buffer().set_text(text);
    let start = view.buffer().start_iter();
    view.buffer().place_cursor(&start);
}
