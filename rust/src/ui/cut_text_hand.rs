//! F3.4 ✐ Text (caption) by hand — the page half of the flow: the arm, the form drawer, the Apply —
//! plus F3.2's Aspect-ratio dropdown door, which reads and repaints the same readout row.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_screen;
use crate::fx_aspect;
use crate::fx_band;
use crate::fx_record;
use crate::fx_text;
use crate::fx_zoom;
use crate::shell::Page;
use crate::ui::cut_label_hand::label_entry_of as text_entry_of;
use crate::ui::window::{
    find_status, find_widget_by_name, line_step_button, live_form_refused, log_line,
    preview_picture_px, record_edit, refresh_effects_lane, refresh_selection_readout, trim_seconds,
    zoom_field_row, PREVIEW_PANEL_H, PREVIEW_PANEL_W,
};
use crate::ui::{cut_form_buttons, cut_form_frame};

pub fn press_text_item(window: &adw::ApplicationWindow) -> String {
    // A marked stretch outranks the line for the tail's wording; a band under every effect's floor for "marked"
    // (cut_speed::MIN_MARKED_SECONDS) is not a band at all, exactly as F3.3 reads it.
    let marked = cut_form_buttons::marked_band();
    match fx_text::arm(text_armed()) {
        fx_text::Press::Armed => {
            set_text_armed(true);
            fx_text::arm_words(marked)
        }
        fx_text::Press::Disarmed => {
            set_text_armed(false);
            close_text_form(window);
            // `fx_text` spells the arm's words but no disarm sentence, so this reuses the one shape
            // `press_zoom_item` gives for taking an arm off -- one wording, never two near-duplicates.
            "text disarmed \u{2014} the picture stops waiting for a box".to_string()
        }
    }
}

fn cut_text_form_name() -> String {
    format!("text-form-{}", Page::Cut.label())
}

fn text_form_box_raw(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, &cut_text_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

pub(crate) fn close_text_form(window: &adw::ApplicationWindow) {
    set_text_form(None);
    if let Some(holder) = text_form_box_raw(window) {
        holder.set_visible(false);
    }
}

pub(crate) fn show_text_form(window: &adw::ApplicationWindow, fx: &cut::Fx) {
    // Hidden FIRST, before anything is touched: while invisible its children cannot read as "already parented
    // here" to a later pass, and the clear below then removes exactly what this holder owns.
    let Some(holder) = text_form_box_raw(window) else {
        return;
    };
    holder.set_visible(false);
    cut_form_frame::clear_children(&holder);
    LAST_TEXT_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));

    let heading = gtk::Label::new(Some(&fx_text::form_title(fx.t)));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");
    heading.set_widget_name("text-heading");

    // Words: a TextView, NOT an Entry, ON PURPOSE. `fx_text::WORDS_ROWS` is three lines tall and Enter must
    // start a new line inside those three; a one-line `gtk::Entry` ends the field on Enter, which would leave
    // no way to break a line at all. A later refactor that "simplifies" this to an Entry silently breaks the
    // multi-line rule, so the type itself is the documentation.
    let words_row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let words_key = gtk::Label::new(Some(fx_text::FORM_FIELDS[0]));
    words_key.set_xalign(0.0);
    let words_view = gtk::TextView::new();
    crate::ui::margins::text_box(&words_view);
    words_view.set_widget_name("text-field-words");
    words_view.set_wrap_mode(gtk::WrapMode::WordChar);
    words_view.set_tooltip_text(Some(fx_text::WORDS_HELP));
    // Three rows of roughly 18 px each; the exact pixel height is a layout matter, the ROW COUNT is the rule.
    // Width is deliberately not asked for: the box takes the column's own (§S3).
    words_view.set_size_request(-1, 18 * fx_text::WORDS_ROWS as i32);
    words_view.set_hexpand(true);
    words_view.set_halign(gtk::Align::Fill);
    if !fx.text.is_empty() {
        words_view.buffer().insert_at_cursor(&fx.text);
    }
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&words_view));
    // Horizontal scroll stays OFF the window's side (the TextView wraps), vertical only, so three rows of
    // typed text never widen the form column.
    scroller.set_policy(gtk::PolicyType::External, gtk::PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_halign(gtk::Align::Fill);
    words_row.append(&words_key);
    words_row.append(&scroller);
    words_row.set_hexpand(true);
    words_row.set_halign(gtk::Align::Fill);

    let length = zoom_field_row(
        fx_text::FORM_FIELDS[1],
        "how long the caption stays on screen",
        "text-field-length",
        &trim_seconds(fx.dur),
    );
    let fades = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    fades.append(&zoom_field_row(
        fx_text::FORM_FIELDS[2],
        fx_text::FADES_HELP,
        "text-field-fade-in",
        &trim_seconds(fx.trans),
    ));
    fades.append(&zoom_field_row(
        fx_text::FORM_FIELDS[3],
        fx_text::FADES_HELP,
        "text-field-fade-out",
        &trim_seconds(fx.tout),
    ));
    // Curve: one choice today, shown as a label rather than a dropdown with nothing in it — the same shape and
    // the same words the zoom and speed forms use, because `fx_text::apply` feeds this through
    // `fx_zoom::curve_stored` and all three forms must offer what that function understands.
    let curve_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let curve_key = gtk::Label::new(Some(fx_text::FORM_FIELDS[4]));
    curve_key.set_xalign(0.0);
    let curve_value = gtk::Label::new(Some(fx_zoom::CURVE_CHOICES[0]));
    curve_value.set_widget_name("text-field-curve");
    curve_value.set_tooltip_text(Some(fx_zoom::CURVE_HELP));
    curve_row.append(&curve_key);
    curve_row.append(&curve_value);

    let buttons = super::cut_form_buttons::form_buttons_row("text");

    let footer = super::cut_form_buttons::form_note("text");

    holder.append(&heading);
    holder.append(&words_row);
    holder.append(&length);
    holder.append(&fades);
    holder.append(&curve_row);
    holder.append(&buttons);
    holder.append(&footer);
    holder.set_visible(true);
    // Stored only now that the widgets exist and belong to THIS window: `text_form_open()` then always means
    // "there is a form on screen behind it", never "a value was parked somewhere".
    set_text_form(Some(caption_form_of(fx)));
    // Buttons wired after the widgets are in the tree, so the lookup by name finds them.
    wire_text_buttons(window);
}

fn caption_form_of(fx: &cut::Fx) -> fx_text::Form {
    fx_text::Form {
        t: fx.t,
        dur: fx.dur,
        trans: fx.trans,
        tout: fx.tout,
        curve: String::new(),
        words: fx.text.clone(),
        on: fx_text::Box_ {
            cx: fx.cx.unwrap_or(fx_text::LOWER_THIRD.cx),
            cy: fx.cy.unwrap_or(fx_text::LOWER_THIRD.cy),
            wf: fx.wf.unwrap_or(fx_text::LOWER_THIRD.wf),
            hf: fx.hf.unwrap_or(fx_text::LOWER_THIRD.hf),
        },
    }
}

pub fn text_armed() -> bool {
    TEXT_ARMED.with(|cell| cell.get())
}

/// Take the caption arm down WITHOUT touching an open form — the mirror of `cut_svg::release_svg_arm_only`.
pub fn release_text_arm_only() {
    set_text_armed(false);
}

fn set_text_armed(armed: bool) {
    TEXT_ARMED.with(|cell| cell.set(armed));
}

pub fn text_form_open() -> Option<fx_text::Form> {
    TEXT_FORM.with(|cell| cell.borrow().clone())
}

fn set_text_form(form: Option<fx_text::Form>) {
    TEXT_FORM.with(|cell| *cell.borrow_mut() = form);
}

thread_local! {
    static TEXT_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    static TEXT_FORM: std::cell::RefCell<Option<fx_text::Form>> =
        const { std::cell::RefCell::new(None) };

    static LAST_TEXT_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };

    static TEXT_FORM_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Read one of the caption form's entries back from THIS window's tree.
fn text_entry(name: &str) -> Option<gtk::Entry> {
    text_entry_of(&current_text_window().unwrap_or_default(), name)
}

/// The window that last drew a caption form, if any — the owner `text_form_owner` compares against.
fn current_text_window() -> Option<adw::ApplicationWindow> {
    LAST_TEXT_WINDOW.with(|cell| cell.borrow().clone())
}

fn text_form_owner(window: &adw::ApplicationWindow) -> bool {
    LAST_TEXT_WINDOW.with(|cell| cut_form_buttons::owns_form(&cell.borrow(), window))
}

pub fn press_text_apply(window: &adw::ApplicationWindow) -> String {
    if !text_form_owner(window) {
        return "no caption form on this page \u{2014} press \u{275d} Text first".to_string();
    }
    let Some(stored) = text_form_open() else {
        return "no caption to apply \u{2014} press \u{275d} Text first".to_string();
    };
    // A non-parsing entry keeps what the form already held rather than becoming zero, so a half-typed number
    // cannot silently place a 0-second caption -- the same rule `press_speed_apply` holds to.
    let read = |name: &str, keep: f64| -> f64 {
        text_entry(name)
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(keep)
    };
    let form = fx_text::Form {
        t: stored.t,
        dur: read("text-field-length", stored.dur),
        trans: read("text-field-fade-in", stored.trans),
        tout: read("text-field-fade-out", stored.tout),
        curve: stored.curve.clone(),
        words: text_words().unwrap_or(stored.words.clone()),
        on: stored.on,
    };
    let fx = match fx_text::apply(&form) {
        Ok(fx) => fx,
        Err(reason) => {
            // Verbatim, form still open. No record, no history write: nothing happened yet.
            log_line(&reason);
            live_form_refused();
            return reason;
        }
    };
    cut_form_buttons::land_new_effect(window, fx);
    close_text_form(window);
    set_text_armed(false);
    // The sentence is `fx_record`'s own ("Text recorded — ↶ Undo takes it back"), reused rather than composed
    // here: a hand-placed caption and a dropdown-recorded one are the same news about the same kind.
    let said = fx_record::recorded_status(cut::EffectKind::Text);
    log_line(&said);
    said
}

pub fn text_type_words(window: &adw::ApplicationWindow, words: &str) -> String {
    find_widget_by_name(window.upcast_ref(), "text-field-words")
        .and_then(|w| w.downcast::<gtk::TextView>().ok())
        .map(|view| {
            view.buffer().set_text(words);
            // The page already watches this buffer for typing (`note_typing`), so the repaint that follows is the
            // one a keystroke gets; nothing is poked around the buffer behind its back.
            format!("typed {words}")
        })
        .unwrap_or_else(|| {
            "no caption form on this page \u{2014} press \u{275d} Text first".to_string()
        })
}

fn text_words() -> Option<String> {
    LAST_TEXT_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), "text-field-words"))
            .and_then(|w| w.downcast::<gtk::TextView>().ok())
            .map(|view| {
                let buffer = view.buffer();
                let (start, end) = buffer.bounds();
                buffer.text(&start, &end, true).to_string()
            })
    })
}

pub fn press_text_cancel(window: &adw::ApplicationWindow) -> String {
    close_text_form(window);
    set_text_armed(false);
    "left as it was \u{2014} no caption placed".to_string()
}

pub fn press_text_esc(window: &adw::ApplicationWindow) -> Option<String> {
    if !text_form_owner(window) || (text_form_open().is_none() && !text_armed()) {
        return None;
    }
    Some(press_text_cancel(window))
}

fn wire_text_buttons(window: &adw::ApplicationWindow) {
    for (name, door) in [("text-apply-button", true), ("text-cancel-button", false)] {
        if let Some(button) = line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let said = if door {
                    press_text_apply(&win)
                } else {
                    press_text_cancel(&win)
                };
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
            });
        }
    }
    // F3.8 S5: the entries land on a pause, not per character. The commit re-reads THIS form's three number fields
    // into the stored form (so Apply later reads back what was typed even if Apply is pressed between two bursts,
    // and so a field that does not parse keeps what the form held, exactly as Apply's own `read` does) and prints the
    // kept length. It deliberately does not touch the cut — see `wire_live_fields`.
    crate::ui::cut_form_buttons::wire_live_fields(window, "text", |win| {
        let Some(stored) = text_form_open() else {
            return;
        };
        let read = |name: &str, keep: f64| -> f64 {
            text_entry_of(win, name)
                .map(|entry| entry.text().trim().to_string())
                .and_then(|text| text.parse::<f64>().ok())
                .unwrap_or(keep)
        };
        let dur = read("text-field-length", stored.dur);
        let form = fx_text::Form {
            dur,
            trans: read("text-field-fade-in", stored.trans),
            tout: read("text-field-fade-out", stored.tout),
            ..stored
        };
        set_text_form(Some(form));
        crate::ui::cut_form_buttons::say_kept(
            win,
            &format!("{} \u{2014} kept as you type", trim_seconds(dur)),
        );
    });
}

pub(crate) fn wire_text_esc(window: &adw::ApplicationWindow) {
    cut_form_buttons::wire_esc(window, press_text_esc);
}

pub fn text_drag_ended_with_source(
    window: &adw::ApplicationWindow,
    from: (f64, f64),
    to: (f64, f64),
    source: (f64, f64),
) -> String {
    let choice = fx_text::drag_choice(from, to);
    // The line is known only if this session ever placed one, read through the same seam Paste/Insert/Speed use.
    let line = cut_form_buttons::known_line();
    let selection = cut_form_buttons::marked_band();
    match fx_text::place(choice, line, selection, source) {
        Some(fx) => {
            show_text_form(window, &fx);
            format!(
                "{} \u{2014} type the words and \u{25b8} Apply puts them on the picture",
                fx_text::form_title(fx.t)
            )
        }
        // No seconds to place it at: no line and no selection. NO form is drawn -- a chooser with no moment to
        // happen in cannot be answered. The wording is `fx_zoom::NO_LINE`, shared with ⊕ Zoom because §F3.4
        // spells no refusal of its own for this branch (`fx_text::NO_WORDS` is about empty WORDS, a different
        // missing thing), so one sentence stays written down once.
        None => fx_zoom::NO_LINE.to_string(),
    }
}

/// The app-side door: same seam, source size read off the pinned preview frame.
pub fn text_drag_ended(
    window: &adw::ApplicationWindow,
    from: (f64, f64),
    to: (f64, f64),
) -> String {
    let source = preview_picture_px();
    text_drag_ended_with_source(window, from, to, source)
}

pub fn nudge_text_box(
    window: &adw::ApplicationWindow,
    target: Option<usize>,
    delta: (f64, f64),
) -> String {
    let frame = (PREVIEW_PANEL_W, PREVIEW_PANEL_H);
    if let Some(index) = target {
        return match placed_text_box(index) {
            Some(mut record) => {
                let moved = fx_text::move_box(record_box(&record), delta, frame);
                record.cx = Some(moved.cx);
                record.cy = Some(moved.cy);
                write_text_record(window, index, &record);
                let said = fx_band::moved(fx_record::label_of(cut::EffectKind::Text));
                log_line(&said);
                said
            }
            None => no_box_to_move(),
        };
    }
    let Some(mut form) = text_form_open() else {
        return no_box_to_move();
    };
    let moved = fx_text::move_box(form.on, delta, frame);
    form.on = moved;
    set_text_form(Some(form.clone()));
    let said =
        format!(
        "caption box at {:.3}, {:.3} \u{2014} snapped to the finished frame's lines within {} px",
        moved.cx, moved.cy, fx_text::SNAP_PX
    );
    log_line(&said);
    said
}

/// §S5's refusal when there is nothing in hand and nothing placed to grab.
fn no_box_to_move() -> String {
    "no caption box to move \u{2014} press \u{275d} Text and land one first".to_string()
}

fn no_box_to_resize() -> String {
    "no caption box to resize \u{2014} press \u{275d} Text and land one first".to_string()
}

/// The record at `index`, if it is a text one — the only kind §F3.4 gives a preview box to.
fn placed_text_box(index: usize) -> Option<cut::Fx> {
    let record = crate::ui::window_preview::newest_review_cut()
        .fx
        .into_iter()
        .nth(index)?;
    (record.kind == "text").then_some(record)
}

fn record_box(record: &cut::Fx) -> fx_text::Box_ {
    let (cx, cy) = record.centre();
    fx_text::Box_ {
        cx,
        cy,
        wf: record.wf.unwrap_or(fx_text::LOWER_THIRD.wf),
        hf: record.hf.unwrap_or(fx_text::LOWER_THIRD.hf),
    }
}

fn write_text_record(window: &adw::ApplicationWindow, index: usize, record: &cut::Fx) {
    let mut edited = crate::ui::window_preview::newest_review_cut();
    if let Some(slot) = edited.fx.get_mut(index) {
        *slot = record.clone();
    }
    record_edit(window, &edited);
    refresh_effects_lane(window);
}

pub fn drag_text_edge(
    window: &adw::ApplicationWindow,
    target: Option<usize>,
    edge: fx_text::Edge,
    to_px: f64,
) -> String {
    let frame = (PREVIEW_PANEL_W, PREVIEW_PANEL_H);
    if let Some(index) = target {
        return match placed_text_box(index) {
            Some(mut record) => {
                let resized = fx_text::resize_box(record_box(&record), edge, to_px, frame);
                record.cx = Some(resized.cx);
                record.cy = Some(resized.cy);
                record.wf = Some(resized.wf);
                record.hf = Some(resized.hf);
                write_text_record(window, index, &record);
                let said = fx_band::resized(fx_record::label_of(cut::EffectKind::Text));
                log_line(&said);
                said
            }
            None => no_box_to_resize(),
        };
    }
    let Some(mut form) = text_form_open() else {
        return no_box_to_resize();
    };
    let resized = fx_text::resize_box(form.on, edge, to_px, (PREVIEW_PANEL_W, PREVIEW_PANEL_H));
    form.on = resized;
    set_text_form(Some(form.clone()));
    let said = format!(
        "caption box {:.3} wide by {:.3} tall \u{2014} the other axis untouched, floor {} px",
        resized.wf,
        resized.hf,
        fx_text::MIN_BOX_PX
    );
    log_line(&said);
    said
}

pub fn press_aspect(window: &adw::ApplicationWindow, choice: &str) -> String {
    let mut cut_ = crate::ui::window_preview::newest_review_cut();
    let status = fx_aspect::apply(&mut cut_, choice);
    record_edit(window, &cut_);
    // The shape changes what the preview frames, so the lane and the preview redraw in the same turn as the
    // zoom that may have just arrived — a bar left off screen for a zoom that exists is a lie about the cut.
    refresh_effects_lane(window);
    // And every readout rides the shared refresh path (the one F2.6/F2.9/F2.13 already use), with the aspect
    // row repainted inside it from the LIVE cut. A row still showing 16:9 after ↶ took a stored 9:16 away
    // would be a stale look rather than today's answer, which is the rule this page holds to.
    refresh_selection_readout(window);
    log_line(&status);
    status
}

pub(crate) fn refresh_aspect_readout(window: &adw::ApplicationWindow) {
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let name = cut_screen::readout_widget("Aspect ratio");
    if let Some(label) = find_widget_by_name(window.upcast_ref(), &name) {
        if let Ok(label) = label.downcast::<gtk::Label>() {
            label.set_text(&cut_screen::aspect_shown(&cut_.aspect));
        }
    }
}
