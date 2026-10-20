//! F3.7 🏷 Label by hand — the page half of the flow: the press, the form drawer, the Apply.
//!
//! Split out of `window.rs` unchanged: the rules live in [`crate::fx_label`], and what is here is only
//! the state a press leaves behind and the widgets that show it — the name entry, the length stepper and
//! the two buttons. No decision of its own. A label has no box and nothing to arm; unlike Volume it also
//! has a second refusal, the one for a missing NAME, which is why its Apply can fail twice over.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::fx_label;
use crate::shell::Page;
use crate::ui::window::{
    find_status, find_widget_by_name, INSERT_PLACE_KNOWN, line_step_button,
    live_form_answer_is_undo, live_form_refused, log_line, paste_line, record_edit,
    refresh_effects_lane, review_cut_of, save_insert_cut, selection, trim_seconds, zoom_field_row,
};

// --- F3.7 Label by hand: the press, the form, the Apply --------------------------------------------------
//
// The rule half lives in `src/fx_label.rs` (`press`/`Pressed`, `NO_SECONDS`, `NO_NAME`, `initial`/
// `initial_at_line`, `LINE_SECONDS`/`MIN_SECONDS`/`NAME_MAX_CHARS`, `form_title`, `FORM_FIELDS`, `NAME_HELP`/
// `LENGTH_HELP`, `Form`, `apply`, `label`/`placed_status`/`NOTHING_CHANGES`, `never_rendered`,
// `drawn_as_a_tag`). What is here is only the state a press leaves behind and the widgets that show it — no
// decision of its own. A label has no box and nothing to arm; unlike Volume it also has a second refusal, the
// one for a missing NAME, which is why its Apply can fail twice over.

/// This window's label-form holder name. Only the Cut tab's instance is ever drawn into or read from —
/// `page_box` runs once per tab, so an unscoped name exists four times per window and every lookup lands on
/// whichever box the walk reaches first (the `fold-badges` / `camera-rows` / `zoom-form` / `volume-form` rule).
fn cut_label_form_name() -> String {
    format!("label-form-{}", Page::Cut.label())
}

/// Resolve THIS window's label-form holder through its own content, so a closed window's surviving tree cannot
/// answer for a live one.
fn label_form_box_raw(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, &cut_label_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

/// F3.7 S1: what pressing 🏷 Label did — as the sentence for the status line, with the form opened when there
/// is a moment to name. NOTHING is recorded on this press: the record happens in [`press_label_apply`], because
/// until someone gives the name there is nothing to mark ("nothing is placed until then").
///
/// The refusal is answered BEFORE any form is drawn, so a cancelled dialog can never cost the user a refusal
/// (the F2.12 Insert lesson). A band under [`cut_speed::MIN_MARKED_SECONDS`] is not a band at all and falls
/// through to the line exactly as the flowchart draws it; the line is read through the same seam Paste, Insert,
/// Speed and Volume use (`paste_line`) and gated by whether one was ever placed, so the page has one notion of
/// "a line exists".
pub fn press_label_item(window: &adw::ApplicationWindow) -> String {
    let band = selection(window)
        .filter(|band| band.length() > 0.0)
        .map(|band| (band.start, band.end));
    let known = INSERT_PLACE_KNOWN.with(|cell| *cell.borrow()) == Some(true);
    let line = known.then(|| paste_line(window));
    match fx_label::press(band, line) {
        fx_label::Pressed::Refused => {
            close_label_form(window);
            fx_label::NO_SECONDS.to_string()
        }
        pressed => {
            // `initial` is the mark the press opens the form with: two seconds wide, unnamed. Its `t`/`dur` are
            // the form's Length; its empty `text` is the Name field waiting to be filled. For the line branch
            // `initial` cannot know the playhead (it leaves 0.0), so `initial_at_line` supplies the real second
            // rather than letting the title read `Label at 00:00`.
            let fx = match (pressed, line) {
                (fx_label::Pressed::MarkAtLine, Some(at)) => fx_label::initial_at_line(at),
                (pressed, _) => fx_label::initial(pressed).unwrap_or_default(),
            };
            let title = fx_label::form_title(fx.t);
            show_label_form(window, &fx);
            format!("{title} \u{2014} name it and \u{25b8} Apply marks it")
        }
    }
}

/// Hide and empty the label form. Used by the refusal path too, so a form left open from an earlier press cannot
/// sit on screen while the page says there is no moment to work on.
pub(crate) fn close_label_form(window: &adw::ApplicationWindow) {
    set_label_form(None);
    if let Some(holder) = label_form_box_raw(window) {
        holder.set_visible(false);
    }
}

/// F3.7 S3: draw the form "Label at m:ss" — the two fields in [`fx_label::FORM_FIELDS`]'s order on one row as
/// `spec/img/06-label.png` lays them out, plus Apply / Cancel. Built into locals and appended LAST in one pass:
/// a widget that already has a parent cannot be appended again and GTK asserts it, so nothing here may travel
/// between calls.
pub(crate) fn show_label_form(window: &adw::ApplicationWindow, fx: &cut::Fx) {
    // Hidden FIRST, before anything is touched: while invisible its children cannot read as "already parented
    // here" to a later pass, and the clear below then removes exactly what this holder owns.
    let Some(holder) = label_form_box_raw(window) else {
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
    LAST_LABEL_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));

    let heading = gtk::Label::new(Some(&fx_label::form_title(fx.t)));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");
    heading.set_widget_name("label-heading");

    // One row: Name then Length (s), §A.7's order. The name comes first because it is the whole answer — the
    // length only bounds how wide the marked moment is drawn. It starts EMPTY on purpose: the name is the
    // question, and a prefilled placeholder would place a word nobody typed (§F3.7 refuses an empty name for
    // exactly that reason). `max_length` makes GTK enforce the tag's 10-char budget at the keyboard as well as
    // `fx_label::apply` enforcing it on the way in.
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let name_key = gtk::Label::new(Some(fx_label::FORM_FIELDS[0]));
    name_key.set_xalign(0.0);
    let name = gtk::Entry::new();
    name.set_widget_name("label-field-name");
    name.set_text(&fx.text);
    name.set_tooltip_text(Some(fx_label::NAME_HELP));
    name.set_width_chars(12);
    name.set_max_length(fx_label::NAME_MAX_CHARS as i32);
    row.append(&name_key);
    row.append(&name);
    row.append(&zoom_field_row(
        fx_label::FORM_FIELDS[1],
        fx_label::LENGTH_HELP,
        "label-field-length",
        &trim_seconds(fx.dur),
    ));

    // The spec's picture carries only a ✕ in the corner; every other effect form pairs an explicit Apply with a
    // Cancel, and §F3.7's TAG node needs a deliberate "mark it" — so both buttons are here, named like the
    // volume form's.
    let buttons = super::cut_form_buttons::form_buttons_row("label");

    let footer = super::cut_form_buttons::form_note("label");

    holder.append(&heading);
    holder.append(&row);
    holder.append(&buttons);
    holder.append(&footer);
    holder.set_visible(true);
    // Stored only now that the widgets exist and belong to THIS window: `label_form_open()` then always means
    // "there is a form on screen behind it", never "a value was parked somewhere".
    set_label_form(Some(label_form_of(fx)));
    wire_label_buttons(window);
}

/// The form a press starts from, built out of the record so no default is invented in the widget layer. The name
/// is whatever the mark holds — empty until the human types one.
fn label_form_of(fx: &cut::Fx) -> fx_label::Form {
    fx_label::Form { t: fx.t, name: fx.text.clone(), dur: fx.dur }
}

/// F3.7 S2/S3: the form waiting on the page, before Apply. What a widget test reads to assert the same state
/// the logic test asserts ([`fx_label::Form`]) rather than a painted field.
pub fn label_form_open() -> Option<fx_label::Form> {
    LABEL_FORM.with(|cell| cell.borrow().clone())
}

fn set_label_form(form: Option<fx_label::Form>) {
    LABEL_FORM.with(|cell| *cell.borrow_mut() = form);
}

thread_local! {
    /// The form a Label press left waiting on the page. `None` is no form open, which is also what hides the
    /// holder — one slot answers both questions so they cannot disagree.
    static LABEL_FORM: std::cell::RefCell<Option<fx_label::Form>> =
        const { std::cell::RefCell::new(None) };

    /// The window whose label form was last shown, so the field readers have THIS window's tree to look in and
    /// a stale window cannot answer for a live one.
    static LAST_LABEL_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn label_form_owner(window: &adw::ApplicationWindow) -> bool {
    LAST_LABEL_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|w: &adw::ApplicationWindow| w.as_ptr() == window.as_ptr())
            .unwrap_or(false)
    })
}

/// One of this window's label-field entries, by name, looked up through the window that owns the form.
fn label_entry(name: &str) -> Option<gtk::Entry> {
    LAST_LABEL_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
    })
}

/// F3.7 S2/S4: Apply. Reads the two fields back, hands them to [`fx_label::apply`] (which refuses an empty
/// name first and the 0.4 s floor second, then clips the name to the tag's budget), and puts the result on the
/// cut as ONE edit.
pub fn press_label_apply(window: &adw::ApplicationWindow) -> String {
    if !label_form_owner(window) {
        return "no label form on this page \u{2014} mark a stretch or put the line down first".to_string();
    }
    let Some(stored) = label_form_open() else {
        return "no label to apply \u{2014} press \u{1f3f7} Label first".to_string();
    };
    // A field that does not parse keeps what the form held rather than becoming 0: a half-typed "1." must not
    // collapse the marked moment. Same rule as the volume and speed forms' readers.
    let read = |name: &str, keep: f64| -> f64 {
        label_entry(name)
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(keep)
    };
    let name = label_entry("label-field-name")
        .map(|entry| entry.text().to_string())
        .unwrap_or_else(|| stored.name.clone());
    let form = fx_label::Form {
        t: stored.t,
        name,
        dur: read("label-field-length", stored.dur),
    };
    let effect = match fx_label::apply(&form) {
        Ok(effect) => effect,
        Err(reason) => {
            // The reason prints verbatim and the form STAYS OPEN: an empty-name refusal that closed the form
            log_line(&reason);
            live_form_refused();
            return reason;
        }
    };
    let mut cut_ = review_cut_of(window);
    cut_.fx.push(effect.clone());
    save_insert_cut(&cut_);
    // `record_edit`, NOT `seed_review_cut`: the addition must sit on the history so F2.13's ↶ takes it back.
    live_form_answer_is_undo(window);
    record_edit(window, &cut_);
    refresh_effects_lane(window);
    close_label_form(window);
    let said = fx_label::placed_status(&effect);
    log_line(&said);
    said
}

/// F3.7 S3: Cancel drops the form and changes nothing on the cut — no record pushed, no history written, so
/// ↶ still points where it did before the press.
pub fn press_label_cancel(window: &adw::ApplicationWindow) -> String {
    close_label_form(window);
    "left as it was \u{2014} nothing marked".to_string()
}

/// F3.7: Esc drops an open label form, and claims no other key. Returns `None` when nothing was open, or when
/// this window is not the one holding it, so the key travels on.
pub fn press_label_esc(window: &adw::ApplicationWindow) -> Option<String> {
    if label_form_open().is_none() || !label_form_owner(window) {
        return None;
    }
    Some(press_label_cancel(window))
}

/// Wire the form's two buttons BY NAME. Each forwards one press and prints what comes back; neither holds a rule.
fn wire_label_buttons(window: &adw::ApplicationWindow) {
    for (name, door) in [("label-apply-button", true), ("label-cancel-button", false)] {
        if let Some(button) = line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let said = if door {
                    press_label_apply(&win)
                } else {
                    press_label_cancel(&win)
                };
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
            });
        }
    }
}

/// Esc releases an open label form, wired after `set_content` like every other control. Claims Escape only when
/// this window owns a form; otherwise the key travels on to whatever else is listening.
pub(crate) fn wire_label_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match press_label_esc(&win) {
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

