//! F3.4 ✐ Text (caption) by hand — the page half of the flow: the arm, the form drawer, the Apply —
//! plus F3.2's Aspect-ratio dropdown door, which reads and repaints the same readout row.
//!
//! Split out of `window.rs` unchanged: the rules live in [`crate::fx_text`] and [`crate::fx_aspect`], and
//! what is here is only the state a press leaves behind and the widgets that show it — the words entry, the
//! box-choice dropdown, the fade toggles and the two buttons. No decision of its own. A caption has a box,
//! so unlike Volume and Label there IS something to arm: the drawing layer stays up, and the drag legs land
//! through [`text_drag_ended_with_source`] rather than a rule of their own.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_screen;
use crate::cut_speed;
use crate::fx_aspect;
use crate::fx_band;
use crate::fx_record;
use crate::fx_text;
use crate::fx_zoom;
use crate::shell::Page;
use crate::ui::window::{
    find_status, find_widget_by_name, preview_picture_px, refresh_selection_readout,
    INSERT_PLACE_KNOWN, PREVIEW_PANEL_H, PREVIEW_PANEL_W, line_step_button,
    live_form_answer_is_undo, live_form_refused, log_line, paste_line, record_edit,
    refresh_effects_lane, review_cut_of, save_insert_cut, selection, trim_seconds, zoom_field_row,
};

// --- F3.4 Text (caption) by hand: the arm, the form drawer ------------------------------------------------------
//
// The rule half lives in `src/fx_text.rs` (`arm`/`arm_words`, `camera_layer_stays_up`, `BoxChoice`, `place`,
// `form_title`, `FORM_FIELDS`, `WORDS_HELP`/`FADES_HELP`/`WORDS_ROWS`/`MIN_SECONDS`/`NO_WORDS`, `Form`,
// `apply`, and the S4 fitting + S5 snap/move/resize rules). What is here is only the state a press leaves
// behind and the widgets that show it — no decision of its own.

/// F3.4 S1: what pressing ❝ Text did — arm or disarm. NOTHING is recorded on this press: until the words are
/// typed there is no caption to place, so the record happens in `press_text_apply`.
///
/// This branch deliberately does NOT call `set_camera_layer_down(...)`. §S1 keeps the camera layer UP while a
/// caption is drawn because the box is measured against the OUTPUT frame ([`fx_text::camera_layer_stays_up`])
/// — the mirror-image reason ⊕ Zoom puts it DOWN: a zoom frames on the source and must see the source, while a
/// caption sits on what the finished video shows, so a camera stepping aside would move the ground under it.
pub fn press_text_item(window: &adw::ApplicationWindow) -> String {
    // A marked stretch outranks the line for the tail's wording; a band under every effect's floor for "marked"
    // (cut_speed::MIN_MARKED_SECONDS) is not a band at all, exactly as F3.3 reads it.
    let marked = selection(window)
        .filter(|band| band.length() >= cut_speed::MIN_MARKED_SECONDS)
        .map(|band| (band.start, band.end));
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

/// This window's text-form holder name. Only the Cut tab's instance is ever drawn into or read from —
/// `page_box` runs once per tab, so an unscoped name exists four times per window (the `fold-badges` /
/// `camera-rows` / `zoom-form` / `speed-form` rule).
fn cut_text_form_name() -> String {
    format!("text-form-{}", Page::Cut.label())
}

/// Resolve THIS window's text-form holder through its own content, so a closed window's surviving tree cannot
/// answer for a live one.
fn text_form_box_raw(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, &cut_text_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

/// Hide and empty the caption form. Used by the disarm path too, so a form left open from an earlier press
/// cannot sit on screen after the entry that opened it has been released.
pub(crate) fn close_text_form(window: &adw::ApplicationWindow) {
    set_text_form(None);
    if let Some(holder) = text_form_box_raw(window) {
        holder.set_visible(false);
    }
}

/// F3.4 S3: draw the form "Text at m:ss" — the five fields in [`fx_text::FORM_FIELDS`]'s order, with Apply
/// and Cancel. Built into locals and appended LAST in one pass: a widget that already has a parent cannot be
/// appended again and GTK asserts it, so nothing here may travel between calls.
pub(crate) fn show_text_form(window: &adw::ApplicationWindow, fx: &cut::Fx) {
    // Hidden FIRST, before anything is touched: while invisible its children cannot read as "already parented
    // here" to a later pass, and the clear below then removes exactly what this holder owns.
    let Some(holder) = text_form_box_raw(window) else {
        return;
    };
    holder.set_visible(false);
    let stale: Vec<gtk::Widget> = holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .collect();
    for old in stale {
        holder.remove(&old);
    }
    LAST_TEXT_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));

    let heading = gtk::Label::new(Some(&fx_text::form_title(fx.t)));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");
    heading.set_widget_name("text-heading");

    // Words: a TextView, NOT an Entry, ON PURPOSE. `fx_text::WORDS_ROWS` is three lines tall and Enter must
    // start a new line inside those three; a one-line `gtk::Entry` ends the field on Enter, which would leave
    // no way to break a line at all. A later refactor that "simplifies" this to an Entry silently breaks the
    // multi-line rule, so the type itself is the documentation.
    // §S3 asks for "the column's full width", so the key sits ABOVE the box instead of beside it: in a row with
    // a label the box only ever got what the label left over (and a fixed 220 px at that).
    let words_row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let words_key = gtk::Label::new(Some(fx_text::FORM_FIELDS[0]));
    words_key.set_xalign(0.0);
    let words_view = gtk::TextView::new();
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
    // The two fades share ONE row, as `spec/img/06-text.png` draws them: `Fade in (s) [0.3]  Fade out (s) [0.3]`
    // side by side rather than stacked. §S3's ORDER is untouched — Length, then Fade in, then Fade out, then Curve,
    // each field still its own named entry under one holder row (`caption_text_widgets` reads that order off the
    // holder's children) — only their layout differs, and it is text-local: `zoom_field_row` stays as the zoom and
    // speed forms want it.
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

/// The `Fx` the gesture produced, as the caption form's own state — the fields a person edits are these
/// numbers and words, so the form holds them and Apply reads them back rather than reaching into the cut.
/// Named `caption_form_of`, not `form_of`: F3.3's speed block already owns that name for its own Form.
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

/// F3.4: whether ❝ Text is armed right now — waiting for a click or a drag on the preview.
pub fn text_armed() -> bool {
    TEXT_ARMED.with(|cell| cell.get())
}

/// Take the caption arm down WITHOUT touching an open form — the mirror of `cut_svg::release_svg_arm_only`.
/// Public because the band's Esc (`ui::press_band_esc`) owns the key whenever an effect is in hand and so
/// claims it before this kind's own controller runs: with the arm raised on a window that does not own the
/// caption form, `press_text_esc` answers `None` (it only releases what its own window drew) and the arm
/// would stay up after a press that said "cancelled". This clears the arm alone, leaving any form standing.
pub fn release_text_arm_only() {
    set_text_armed(false);
}

fn set_text_armed(armed: bool) {
    TEXT_ARMED.with(|cell| cell.set(armed));
}

/// F3.4 S3: the caption form waiting on the page, before Apply. What a widget test reads to assert the same
/// state the logic test asserts ([`fx_text::Form`]) rather than a painted field.
pub fn text_form_open() -> Option<fx_text::Form> {
    TEXT_FORM.with(|cell| cell.borrow().clone())
}

fn set_text_form(form: Option<fx_text::Form>) {
    TEXT_FORM.with(|cell| *cell.borrow_mut() = form);
}

thread_local! {
    /// Whether ❝ Text is armed, i.e. the next press on the preview draws a caption box. Shaped exactly like
    /// F3.1's `ZOOM_ARMED`: the preview's ONE drag gesture dispatches on this to decide whose flow a press is.
    static TEXT_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// The form a landed box left waiting on the page. `None` is no form open, which is also what hides the
    /// holder — one slot answers both questions so they cannot disagree.
    static TEXT_FORM: std::cell::RefCell<Option<fx_text::Form>> =
        const { std::cell::RefCell::new(None) };

    /// The window whose caption form was last shown, so the field readers have THIS window's tree to look in
    /// rather than whichever tree a name walk reaches first.
    static LAST_TEXT_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };

    /// Guard against a refresh-driven property write re-entering a handler through the sensitivity writes
    /// below — the `CAMERA_REFRESH` / `ZOOM_FORM_REFRESH` shape. Nothing toggles a control on this form
    /// today, so the guard is here for the day something does, not because a path needs it now.
    static TEXT_FORM_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Read one of the caption form's entries back from THIS window's tree.
fn text_entry(name: &str) -> Option<gtk::Entry> {
    text_entry_of(&current_text_window().unwrap_or_default(), name)
}

/// The same lookup with the window passed in, which is what the debounced commit needs: by the time the pause runs
/// out, the closure only has the window it was given.
fn text_entry_of(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Entry> {
    find_widget_by_name(window.upcast_ref(), name)
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
}

/// The window that last drew a caption form, if any — the owner `text_form_owner` compares against.
fn current_text_window() -> Option<adw::ApplicationWindow> {
    LAST_TEXT_WINDOW.with(|cell| cell.borrow().clone())
}

/// Does THIS window own the live caption form? Two windows both holding one would put Apply's write into
/// whichever tree the reader reached first.
fn text_form_owner(window: &adw::ApplicationWindow) -> bool {
    LAST_TEXT_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|w: &adw::ApplicationWindow| w.as_ptr() == window.as_ptr())
            .unwrap_or(false)
    })
}

/// F3.4 S3: Apply. Reads the five fields back into an [`fx_text::Form`] and hands them to [`fx_text::apply`],
/// which decides whether there are words at all and shares the fades. A refusal keeps the form OPEN.
///
/// The empty-words case is why: `fx_text::NO_WORDS` says "type the words and they go on the picture — the form
/// applies as you type it". Closing the form on that refusal would throw away the very words the sentence is
/// asking for, so the door prints the reason and leaves everything on screen.
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
    let mut cut_ = review_cut_of(window);
    cut_.fx.push(fx.clone());
    save_insert_cut(&cut_);
    // `record_edit`, NOT `seed_review_cut`: the addition must sit on the history so F2.13's ↶ takes it back.
    live_form_answer_is_undo(window);
    record_edit(window, &cut_);
    refresh_effects_lane(window);
    close_text_form(window);
    set_text_armed(false);
    // The sentence is `fx_record`'s own ("Text recorded — ↶ Undo takes it back"), reused rather than composed
    // here: a hand-placed caption and a dropdown-recorded one are the same news about the same kind.
    let said = fx_record::recorded_status(cut::EffectKind::Text);
    log_line(&said);
    said
}

/// Type words into the caption form's OWN TextView and let the page repaint from it, which is the path typing
/// takes: the field is the app's widget, and [`press_text_apply`] reads it back from the same place. A snapshot
/// needs the caption visible in the shot, and there was no door for putting words into the field from outside a
/// keyboard until this one.
pub fn text_type_words(window: &adw::ApplicationWindow, words: &str) -> String {
    find_widget_by_name(window.upcast_ref(), "text-field-words")
        .and_then(|w| w.downcast::<gtk::TextView>().ok())
        .map(|view| {
            view.buffer().set_text(words);
            // The page already watches this buffer for typing (`note_typing`), so the repaint that follows is the
            // one a keystroke gets; nothing is poked around the buffer behind its back.
            format!("typed {words}")
        })
        .unwrap_or_else(|| "no caption form on this page \u{2014} press \u{275d} Text first".to_string())
}

/// Read the caption's words back out of the TextView, typed newlines intact. `buffer.bounds()` gives the whole
/// buffer's range and `delete_range(.., true)` keeps the visible characters rather than their display
/// equivalents, which is what a file has to store.
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

/// F3.4 S3: Cancel drops the form and the arm and changes nothing on the cut — no record pushed, no history
/// written, so ↶ still points where it did before ❝ Text was pressed.
pub fn press_text_cancel(window: &adw::ApplicationWindow) -> String {
    close_text_form(window);
    set_text_armed(false);
    "left as it was \u{2014} no caption placed".to_string()
}

/// F3.4: Esc releases the arm or drops an open caption form, and claims no other key. Returns `None` when this
/// window holds neither, so the key travels on to whatever else is listening.
pub fn press_text_esc(window: &adw::ApplicationWindow) -> Option<String> {
    if !text_form_owner(window) || (text_form_open().is_none() && !text_armed()) {
        return None;
    }
    Some(press_text_cancel(window))
}

/// Wire the caption form's two buttons BY NAME. Each forwards one press and prints what comes back; neither
/// holds a rule — same shape as `wire_speed_buttons`.
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
        let Some(stored) = text_form_open() else { return };
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

/// Esc releases the caption arm / form, wired after `set_content` like every other control. Claims Escape only
/// when this window owns something to release; otherwise the key travels on.
pub(crate) fn wire_text_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match press_text_esc(&win) {
            Some(said) => {
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    });
    window.add_controller(controller);
}

/// F3.4 S2: the box came off the preview, now what? The seam BOTH the gesture AND the widget test drive,
/// shaped exactly like `zoom_drag_ended_with_source`: the source's pixel size is handed in because no ffprobe
/// runs headless, and nothing here composes a rule — every answer comes from `fx_text`.
pub fn text_drag_ended_with_source(
    window: &adw::ApplicationWindow,
    from: (f64, f64),
    to: (f64, f64),
    source: (f64, f64),
) -> String {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    // A press that did not travel is a CLICK wherever it landed, and a click takes the lower third (§S2).
    let choice = if dx.abs() < fx_zoom::DRAG_MIN_PX || dy.abs() < fx_zoom::DRAG_MIN_PX {
        fx_text::BoxChoice::Click
    } else {
        fx_text::BoxChoice::Dragged {
            x: from.0.min(to.0),
            y: from.1.min(to.1),
            w: dx.abs(),
            h: dy.abs(),
        }
    };
    // The line is known only if this session ever placed one, read through the same seam Paste/Insert/Speed use.
    let known = INSERT_PLACE_KNOWN.with(|cell| *cell.borrow()) == Some(true);
    let line = known.then(|| paste_line(window));
    let selection = selection(window)
        .filter(|band| band.length() >= cut_speed::MIN_MARKED_SECONDS)
        .map(|band| (band.start, band.end));
    match fx_text::place(choice, line, selection, source) {
        Some(fx) => {
            show_text_form(window, &fx);
            wire_text_buttons(window);
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
    let source = preview_picture_px(window);
    text_drag_ended_with_source(window, from, to, source)
}

/// F3.4 S5: move the caption box by `delta` px on the preview. [`fx_text::move_box`] against the pinned panel
/// frame is what supplies the rules — the `SNAP_PX` (10 px) snap to the finished frame's left/centre/right
/// and top/middle/bottom lines, and the fact that a MOVED box offers all three of its own lines to snap with.
///
/// `target` decides WHICH box moves, and that is the whole difference between an edit and a draft:
///  - `None` — the form's own box, the one a just-landed caption carries before Apply. Nothing is on the cut yet,
///    so nothing is recorded; the sentence names the new fractions because the person is still choosing them.
///  - `Some(index)` — a text record already on the cut. The change is written through `record_edit`, the same door
///    Apply uses, so one ↶ takes this move back on its own (§F3.8 S3), and the sentence is `fx_band::moved`'s.
pub fn nudge_text_box(
    window: &adw::ApplicationWindow,
    target: Option<usize>,
    delta: (f64, f64),
) -> String {
    let frame = (PREVIEW_PANEL_W, PREVIEW_PANEL_H);
    if let Some(index) = target {
        return match placed_text_box(window, index) {
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
    let said = format!(
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

/// The resize twin of [`no_box_to_move`] — a different missing thing, so a different sentence, exactly as the two
/// doors are separate gestures.
fn no_box_to_resize() -> String {
    "no caption box to resize \u{2014} press \u{275d} Text and land one first".to_string()
}

/// The record at `index`, if it is a text one — the only kind §F3.4 gives a preview box to.
fn placed_text_box(window: &adw::ApplicationWindow, index: usize) -> Option<cut::Fx> {
    let record = review_cut_of(window).fx.into_iter().nth(index)?;
    (record.kind == "text").then_some(record)
}

/// A text record's box. `Fx::centre` already answers the lower third for a record that stores no box (§06#1's
/// legacy line), which is the same default a freshly landed caption gets, so a placed caption without a stored box
/// still grabs where it draws.
fn record_box(record: &cut::Fx) -> fx_text::Box_ {
    let (cx, cy) = record.centre();
    fx_text::Box_ { cx, cy, wf: record.wf.unwrap_or(fx_text::LOWER_THIRD.wf), hf: record.hf.unwrap_or(fx_text::LOWER_THIRD.hf) }
}

/// Put `record` back at `index` on the page's cut and take the history step. Built from `review_cut_of` with ONE
/// entry replaced, then handed to `record_edit` — the door Apply uses, so the undo lands the move as its own step
/// rather than folding it into the caption's placement.
fn write_text_record(window: &adw::ApplicationWindow, index: usize, record: &cut::Fx) {
    let mut edited = review_cut_of(window);
    if let Some(slot) = edited.fx.get_mut(index) {
        *slot = record.clone();
    }
    record_edit(window, &edited);
    refresh_effects_lane(window);
}

/// F3.4 S5: drag one edge of the caption box to `to_px`. [`fx_text::resize_box`] gives the rules — the dragged
/// edge offers ONLY itself to snap with (pulling a side to the centre must not be second-guessed into moving the
/// middle there), the axes stay independent, and the axis never goes under `MIN_BOX_PX` (16 px) so a caption
/// cannot be shrunk into nothing and lost.
///
/// `target` is [`nudge_text_box`]'s question asked of a resize: `None` sizes the draft on the form, `Some(index)`
/// sizes a record already on the cut and writes it through `record_edit`, so ↶ takes the resize back alone.
pub fn drag_text_edge(
    window: &adw::ApplicationWindow,
    target: Option<usize>,
    edge: fx_text::Edge,
    to_px: f64,
) -> String {
    let frame = (PREVIEW_PANEL_W, PREVIEW_PANEL_H);
    if let Some(index) = target {
        return match placed_text_box(window, index) {
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
        resized.wf, resized.hf, fx_text::MIN_BOX_PX
    );
    log_line(&said);
    said
}


// --- F3.2 Aspect ratio: the dropdown ------------------------------------------------------------------------
//
// The rule half lives in `src/fx_aspect.rs` (`ASPECTS`, `stored`, `choose`, `apply` and the three status
// sentences). What is here is only the door the dropdown calls and the row it repaints, so the callback
// cannot hold a decision of its own.

/// F3.2: pick one of the five shapes and this answers the whole press — store the shape, add the staying zoom
/// when the flowchart says to, say what happened. Every string comes from [`fx_aspect::apply`]; nothing is
/// composed here.
///
/// It writes through [`record_edit`] and never `seed_review_cut`: the aspect AND the ⊕ zoom it brings are ONE
/// Undo step (§F3.2's "same Undo step"), which is exactly why they go through one call rather than two
/// publishes. A refused or reported pick (source, or a lane that already decides) still goes through the same
/// door — `fx_aspect::apply` adds no zoom there, so the history sees the aspect change alone and ↶ takes just
/// that back.
pub fn press_aspect(window: &adw::ApplicationWindow, choice: &str) -> String {
    let mut cut_ = review_cut_of(window);
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

/// Repaint the idle form's Aspect ratio row from the cut this window holds NOW. `cut_screen::aspect_shown`
/// holds the one spelling of 'empty means the default', shared with `idle_readouts`, so the row and the
/// dropdown beside it cannot drift to two different answers. Called from [`refresh_selection_readout`] —
/// the same path that keeps every other control honest — so Undo, Revert and Clear move this row too instead
/// of leaving the last picked shape on screen.
pub(crate) fn refresh_aspect_readout(window: &adw::ApplicationWindow) {
    let cut_ = review_cut_of(window);
    let name = cut_screen::readout_widget("Aspect ratio");
    if let Some(label) = find_widget_by_name(window.upcast_ref(), &name) {
        if let Ok(label) = label.downcast::<gtk::Label>() {
            label.set_text(&cut_screen::aspect_shown(&cut_.aspect));
        }
    }
}
