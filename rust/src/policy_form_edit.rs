//! The policy form's VALUE column, as an editor per row (F0.7 S4/S5, §12 §4's sixth bullet).
//!
//! `ui::policy_form` draws the table; this module owns the one control inside each of its value cells.
//! The split is not tidiness: a field's control has to be built, seeded and read back through one door,
//! and the form file is where the four-column layout lives.
//!
//! Why editing belongs here at all: §12 §4 says the pipeline decisions are "decisions the model makes
//! once, visibly, in the policy form", and F0.7 S4 says a hand-set field (`source: user`) is never
//! overwritten. Neither is reachable while the value column is a label — nothing in the program could
//! create a `user` field. So every row gets a control, and its change goes through
//! [`crate::policy::apply`] with `Origin::User`, the same door the model's `set_policy` uses. That is
//! what makes S4's refusal something a person can produce rather than only a rule a test pokes.
//!
//! The spec does not say which control each row gets. Decided here, and it is what
//! `spec/img/03-policy-form.svg` draws (an entry holding `2.0`, a switch thrown on, a field holding
//! `720`, a focused entry `8.0`): an enumerated field gets a dropdown, an on/off field a switch, and a
//! number a text entry. The decision is made from the catalogue row itself, so a new §2 field needs no
//! edit in this file.

use adw::prelude::*;
use gtk4 as gtk;

use crate::policy::{self, CatalogueRow};
use crate::project::Policy;

/// The widget-name prefix, shared with the form so one literal spells both files' names.
const NAME_PREFIX: &str = "policy";

/// Which editor a row gets. Derived from the catalogue, never from a per-field match: the same list that
/// validates a write (`policy::apply`'s `lookup`) decides what can be typed into it.
#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    /// One of these words, chosen from a dropdown.
    Choice(&'static [&'static str]),
    /// A duration in seconds, typed as a number.
    Number,
    /// On or off, thrown with a switch.
    Switch,
}

/// The editor kind for one catalogue row.
///
/// Non-empty `values` means the field is enumerated, which is exactly when a dropdown is honest: every
/// option the validator accepts is on screen and none of them can be mistyped. Among the unenumerated
/// rows, `on`/`off` is the shape a switch shows best (the picture throws those), and everything else is
/// a number the person types.
pub fn control_for(row: &CatalogueRow) -> Control {
    if !row.values.is_empty() {
        return if is_on_off(row.values) {
            Control::Switch
        } else {
            Control::Choice(row.values)
        };
    }
    Control::Number
}

/// Whether a value list is the on/off shape a switch shows best. Read off the list rather than the field
/// name so a future §2 row that ships switch values gets one without anyone remembering to ask. The
/// catalogue spells its three pass fields as `["on", "off", "true", "false"]` (JSON booleans reach a tool
/// call unquoted, so `policy`'s validation must accept both spellings), and a four-item list is still a
/// two-answer question: what decides a switch is that every value means on or off.
fn is_on_off(values: &[&'static str]) -> bool {
    values
        .iter()
        .all(|value| matches!(*value, "on" | "off" | "true" | "false"))
}

/// Build the row's editor, seeded from the live policy, named `policy-edit-<field>` so a test and the
/// snapshot find it by name.
pub fn build(policy: &Policy, row: &CatalogueRow) -> gtk::Widget {
    let control = control_for(row);
    let shown = policy_form_value(policy, row.field);
    let widget = match &control {
        Control::Choice(values) => {
            let picker = gtk::DropDown::from_strings(values);
            // Seed by matching the word the policy holds, not by index: `value_word` is the form's own
            // spelling and a catalogue reordered under it must still land on the right row.
            if let Some(at) = values.iter().position(|word| *word == shown) {
                picker.set_selected(at as u32);
            }
            picker.set_widget_name(&format!("{NAME_PREFIX}-edit-{}", row.field));
            picker.upcast()
        }
        Control::Switch => {
            let switch = gtk::Switch::new();
            switch.set_active(shown == "on");
            // A switch answers "is this pass asked for", so it is halign-start like every other cell:
            // centred in the column it reads as decoration rather than as the row's answer.
            switch.set_halign(gtk::Align::Start);
            switch.set_valign(gtk::Align::Center);
            switch.set_widget_name(&format!("{NAME_PREFIX}-edit-{}", row.field));
            switch.upcast()
        }
        Control::Number => {
            let entry = gtk::Entry::new();
            // The SECONDS, not the form's `30 min` word: what a person types must be unambiguous, and a
            // length spelled in minutes would have to be re-parsed on the way back in. `none` (0 = no
            // target) types as `0`, which is what `policy::apply` accepts for it.
            entry.set_text(&number_text(policy, row.field));
            entry.set_width_chars(10);
            entry.set_max_width_chars(12);
            entry.set_widget_name(&format!("{NAME_PREFIX}-edit-{}", row.field));
            entry.upcast()
        }
    };
    wire(&widget, row.field);
    widget
}

/// The value a row's editor shows, in the words `policy::apply` takes.
///
/// One reader for a test and for the refresh path, so neither downcasts a `gtk::Widget` on its own and
/// neither invents a third spelling of "what does this row say". A numeric row answers in SECONDS, which
/// is what its entry holds; the form's prose column spells the same number `30 min` or "none"
/// (`policy_form::length_word`), so a caller comparing the two compares through [`number_of`].
pub fn value_of(widget: &gtk::Widget) -> String {
    if let Some(picker) = widget.downcast_ref::<gtk::DropDown>() {
        let at = picker.selected();
        if at == gtk::INVALID_LIST_POSITION {
            return String::new();
        }
        return dropdown_string(picker, at);
    }
    if let Some(switch) = widget.downcast_ref::<gtk::Switch>() {
        return (if switch.is_active() { "on" } else { "off" }).to_string();
    }
    if let Some(entry) = widget.downcast_ref::<gtk::Entry>() {
        return entry.text().trim().to_string();
    }
    String::new()
}

/// What a numeric row's editor holds, as seconds. `None` for a control that is not an entry — a dropdown
/// word or a switch is not a duration and pretending otherwise would read "on" as zero seconds.
pub fn number_of(widget: &gtk::Widget) -> Option<f64> {
    widget
        .downcast_ref::<gtk::Entry>()
        .and_then(|entry| entry.text().trim().parse::<f64>().ok())
}

/// Move the control to what the policy now says, after a derivation or a reset changed the state under it.
///
/// In place, like the source and reason columns: rebuilding a row mid-session would drop the focus a
/// person is typing into.
pub fn set_value(widget: &gtk::Widget, policy: &Policy, row: &CatalogueRow) {
    let shown = policy_form_value(policy, row.field);
    if let Some(picker) = widget.downcast_ref::<gtk::DropDown>() {
        if let Some(at) = row.values.iter().position(|word| *word == shown) {
            picker.set_selected(at as u32);
        }
        return;
    }
    if let Some(switch) = widget.downcast_ref::<gtk::Switch>() {
        switch.set_active(shown == "on");
        return;
    }
    if let Some(entry) = widget.downcast_ref::<gtk::Entry>() {
        entry.set_text(&number_text(policy, row.field));
    }
}

/// Attach the signal that matches the control: a dropdown commits on selection, a switch on a throw, and
/// an entry when Enter is pressed — NOT on every keystroke, because a half-typed `18` is not a length.
///
/// Every handler first asks whether the control already says what the policy says. Without that guard a
/// repaint moves a widget and the move writes back: `refresh_policy_form` selecting a dropdown to the
/// stored value would fire `notify::selected`, stamp `source: user` on a field nobody touched, and a
/// following "Reset to defaults" would then be refused by S4 as if a person had set it.
fn wire(widget: &gtk::Widget, field: &str) {
    let name = field.to_string();
    if let Some(picker) = widget.downcast_ref::<gtk::DropDown>() {
        // The closure outlives this call, so it holds its OWN handle on the control rather than the
        // borrow it was built from.
        let held = picker.clone();
        let checked = name.clone();
        picker.connect_selected_notify(move |_| {
            let value = value_of(held.upcast_ref());
            if value.is_empty() || same_as_policy(&checked, held.upcast_ref(), &value) {
                return;
            }
            crate::ui::set_field_from_form(&checked, &value);
        });
        return;
    }
    if let Some(switch) = widget.downcast_ref::<gtk::Switch>() {
        let checked = name.clone();
        switch.connect_state_set(move |switch, active| {
            // `state-set` fires BEFORE the state changes, so the word is said from the argument rather
            // than read back from the widget (which would report the previous throw).
            let word = if active { "on" } else { "off" };
            if !same_as_policy(&checked, switch.upcast_ref(), word) {
                crate::ui::set_field_from_form(&checked, word);
            }
            glib::Propagation::Proceed
        });
        return;
    }
    if let Some(entry) = widget.downcast_ref::<gtk::Entry>() {
        let held = entry.clone();
        let checked = name.clone();
        entry.connect_activate(move |_| {
            let value = value_of(held.upcast_ref());
            if value.is_empty() || same_as_policy(&checked, held.upcast_ref(), &value) {
                return;
            }
            crate::ui::set_field_from_form(&checked, &value);
        });
    }
}

/// Whether the live policy already holds this value, so a repaint's own widget movement is not mistaken
/// for a person's edit. Reads through `session_policy`, the same slot the form paints from.
/// Whether the live policy already holds this value, so a repaint's own widget movement is not mistaken
/// for a person's edit. Reads through `session_policy`, the same slot the form paints from.
///
/// The control comes along because a numeric row's editor holds SECONDS while the prose column spells them
/// (`none`, `30 min`): comparing the two spellings as text would call every typed length a change. Reading
/// the seconds back through [`number_of`] means one reader decides what the entry says — here and in the
/// tests that check the cell against the policy.
fn same_as_policy(field: &str, widget: &gtk::Widget, value: &str) -> bool {
    let policy = crate::ui::session_policy_for_form();
    match field {
        // A number compares as seconds. An entry holding something that is not one answers `None` and falls
        // through to the word comparison, which cannot match it either — so the bad text still reaches
        // `set_field_from_form` and `policy::apply` refuses it out loud, which is the point.
        "targetLengthSeconds" => match number_of(widget) {
            Some(seconds) => (seconds - policy.target_length_seconds.value).abs() < f64::EPSILON,
            None => crate::ui::policy_form::value_word(&policy, field) == value,
        },
        _ => crate::ui::policy_form::value_word(&policy, field) == value,
    }
}

/// The dropdown's selected word. `gtk::DropDown` over strings exposes them as `glib::GObject`s, so the
/// word is read off the model rather than guessed from the index arithmetic a caller would repeat.
fn dropdown_string(picker: &gtk::DropDown, at: u32) -> String {
    picker
        .model()
        .and_then(|model| model.item(at))
        .and_then(|item| {
            item.downcast_ref::<gtk::StringObject>()
                .map(|word| word.string().to_string())
        })
        .unwrap_or_default()
}

/// The policy's word for a field, spelled by the form's own reader so the control and the other three
/// columns cannot drift apart.
fn policy_form_value(policy: &Policy, field: &str) -> String {
    crate::ui::policy_form::value_word(policy, field)
}

/// The number a numeric row seeds its entry with: seconds, or `0` for §10's "none".
fn number_text(policy: &Policy, field: &str) -> String {
    let seconds = policy_target_seconds(policy, field);
    if seconds <= crate::tools::cutpass::NO_TARGET {
        return "0".to_string();
    }
    // Whole seconds print without a decimal point; a fraction keeps it, so what is in the box is what
    // would be written back.
    if (seconds - seconds.round()).abs() < f64::EPSILON {
        format!("{}", seconds.round() as i64)
    } else {
        format!("{seconds}")
    }
}

/// The seconds behind a numeric row. Only `targetLengthSeconds` is numeric today (§10 gives no other
/// derived number), so anything else reads 0 rather than pretending to know.
fn policy_target_seconds(policy: &Policy, field: &str) -> f64 {
    match field {
        "targetLengthSeconds" => policy.target_length_seconds.value,
        _ => 0.0,
    }
}

/// Every editable row, so a caller can walk the open form without knowing the catalogue.
pub fn rows() -> Vec<CatalogueRow> {
    policy::derived_fields().into_iter().collect()
}
