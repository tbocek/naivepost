//! F3.8 S5: every effect form ends with the same row — "Apply" and "Cancel", left to right — and under it the
//! note "Kept as you type — ↶ Undo takes the whole edit back."
//!
//! The six forms built that row six times, in three different orders, and one of them (Label's and Volume's
//! picture) carried only a corner ✕ until the row was added. The row is built once here so the NAMES and the
//! LEFT-TO-RIGHT ORDER are one fact: `<kind>-apply-button`, `<kind>-cancel-button`, inside one box named
//! `form-buttons-<kind>` whose children are [Apply, Cancel]. Each form keeps its own click handlers: the row's
//! shape is shared, the doors behind the two buttons are not.

use adw::prelude::*;
use gtk4 as gtk;

/// The two labels §F3.8 S5 spells, in the order it puts them on the row.
pub const FORM_BUTTONS: [&str; 2] = ["Apply", "Cancel"];

use crate::fx_band;
use crate::ui::window::find_widget_by_name;

thread_local! {
    /// Per-form-kind debounce state: when the last keystroke landed, and whether a timer is already waiting for the
    /// pause to run out. `false` means none is, which is what makes a burst cost ONE commit: the first keystroke
    /// arms the timer, the rest only move the stamp.
    static FORM_STAMPS: std::cell::RefCell<std::collections::HashMap<String, (std::time::Instant, bool)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// F3.8 S5 (`every keystroke lands after a debounce`): wire a live form's number entries so typing lands on a
/// pause rather than per character. Every `<kind>-field-*` entry in the window gets a `changed` handler that stamps
/// the keystroke and, when no timer is pending, arms one `glib::timeout_add_local` of [`fx_band::DEBOUNCE_MS`]. On
/// fire the gate is [`fx_band::keystroke_lands`]: if the pause ran out the commit runs ONCE for the whole burst, if
/// something arrived during the wait the timer simply waits again. A second keystroke before the fire restarts the
/// wait — that is what a debounce is, and why one edit of "12" is one commit rather than two.
///
/// THE DECISION, since the spec does not spell it out: the debounced commit writes the FORM'S OWN stored state and
/// prints the kept reading; it does NOT write the cut. The cut record still changes only at Apply, through
/// `record_edit`. That is what makes the footer's promise true as written — "↶ Undo takes the whole edit back", not
/// a step per keystroke — and it keeps one edit from becoming two history entries. The footer this module draws and
/// this timer serve the same rule.
pub fn wire_live_fields(
    window: &adw::ApplicationWindow,
    kind: &str,
    commit: impl Fn(&adw::ApplicationWindow) + Clone + 'static,
) {
    let Some(content) = window.content() else {
        return;
    };
    let prefix = format!("{kind}-field-");
    let win = window.clone();
    let key = kind.to_string();
    for name in entry_names_under(&content, &prefix) {
        let Some(widget) = find_widget_by_name(&content, &name) else {
            continue;
        };
        let Some(entry) = widget.downcast::<gtk::Entry>().ok() else {
            continue;
        };
        let stamp_key = key.clone();
        let fire = win.clone();
        let on_fire = commit.clone();
        let on_outer = commit.clone();
        let _ = &on_outer;
        entry.connect_changed(move |_| {
            let pending = FORM_STAMPS.with(|cell| {
                let mut map = cell.borrow_mut();
                let slot = map
                    .entry(stamp_key.clone())
                    .or_insert((std::time::Instant::now(), false));
                // Restamp first: the timer, when it fires, measures the PAUSE since the most recent keystroke.
                slot.0 = std::time::Instant::now();
                std::mem::replace(&mut slot.1, true)
            });
            if pending {
                // A timer is already on its way; the restamp above is all the restart it needs — that is the whole
                // difference between a debounce and a per-character write.
                return;
            }
            let fired_key = stamp_key.clone();
            let fired_win = fire.clone();
            let on_fired = on_fire.clone();
            glib::timeout_add_local(
                std::time::Duration::from_millis(fx_band::DEBOUNCE_MS as u64),
                move || {
                    let elapsed_ms = FORM_STAMPS.with(|cell| {
                        cell.borrow()
                            .get(&fired_key)
                            .map(|(since, _)| since.elapsed().as_secs_f64() * 1000.0)
                            .unwrap_or(fx_band::DEBOUNCE_MS)
                    });
                    // The gate itself, not a guess: a keystroke that arrived during the wait pushes the stamp forward,
                    // so `keystroke_lands` says no and the timer waits one more pause.
                    if !fx_band::keystroke_lands(elapsed_ms, fx_band::DEBOUNCE_MS) {
                        return glib::ControlFlow::Continue;
                    }
                    FORM_STAMPS.with(|cell| {
                        if let Some(slot) = cell.borrow_mut().get_mut(&fired_key) {
                            slot.1 = false;
                        }
                    });
                    on_fired(&fired_win);
                    glib::ControlFlow::Break
                },
            );
        });
    }
}

/// How a debounced commit puts its entry's text on the status row, so the person sees the number the form kept
/// without the cut being touched. Every form's wire shares this rather than writing its own.
pub(crate) fn say_kept(window: &adw::ApplicationWindow, text: &str) {
    if let Some(status) = crate::ui::find_status(window.upcast_ref()) {
        status.set_text(text);
    }
    crate::ui::window_logs::log_line(text);
}

/// The names of every `gtk::Entry` under `root` whose widget name starts with `prefix` — the form's own number
/// fields, found by the naming rule the forms already keep rather than a hand-written list per kind.
pub(crate) fn entry_names_under(root: &gtk::Widget, prefix: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        let name = widget.widget_name().to_string();
        // An entry that carries a `<kind>-field-` name is one of the form's number fields; anything else with that
        // prefix (a label, a dropdown) is not this timer's business.
        if name.starts_with(prefix) && widget.downcast_ref::<gtk::Entry>().is_some() {
            found.push(name);
        }
        if let Some(child) = widget.first_child() {
            let mut cursor = child;
            loop {
                stack.push(cursor.clone());
                match cursor.next_sibling() {
                    Some(next) => cursor = next,
                    None => break,
                }
            }
        }
    }
    found
}

/// The row's box name for one form kind (`form-buttons-zoom`). A check finds the buttons through this box
/// rather than by a per-kind name, which is what proves they are ONE row and not two loose buttons.
pub fn form_buttons_name(kind: &str) -> String {
    format!("form-buttons-{kind}")
}

/// Build the shared row. The two buttons keep the per-kind names every existing wire and test already uses, so
/// nothing that presses them by name has to change.
pub fn form_buttons_row(kind: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.set_widget_name(&form_buttons_name(kind));
    let apply = gtk::Button::with_label(FORM_BUTTONS[0]);
    apply.set_widget_name(&format!("{kind}-apply-button"));
    let cancel = gtk::Button::with_label(FORM_BUTTONS[1]);
    cancel.set_widget_name(&format!("{kind}-cancel-button"));
    // S5 spells the order: Apply first, then Cancel.
    row.append(&apply);
    row.append(&cancel);
    row
}

/// The row's two buttons, in S5's order — `[Apply, Cancel]`. A wire attaches its own doors to them by name;
/// this hands the pair back to a form that wires them from the locals rather than by a tree walk.
pub fn row_buttons(row: &gtk::Box) -> (gtk::Button, gtk::Button) {
    let children = row.observe_children();
    let mut seen = children
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Button>().ok());
    let apply = seen.next().expect("the row is built with Apply first");
    let cancel = seen.next().expect("the row is built with Cancel second");
    (apply, cancel)
}

/// The note S5 puts UNDER the row, as one label with the form's own name.
pub fn form_note(kind: &str) -> gtk::Label {
    let note = gtk::Label::new(Some(crate::fx_band::KEPT_AS_YOU_TYPE));
    note.set_xalign(0.0);
    note.add_css_class("dim-label");
    note.set_widget_name(&format!("{kind}-form-footer"));
    note
}
