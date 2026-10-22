//! §12-decisions#4's sixth bullet, on the WIRE: the policy form's editor is the door a person's pipeline
//! decision enters the program through, and a decision made there survives the next derivation.
//!
//! `tests/suite/policy_derive.rs` proves F0.7's rules on handed-in proposals and `tests/suite/cut_policy_form_widgets.rs`
//! proves the ⚙ opens a form that reads the state. What neither could prove while the value column was a
//! label is the two things §12 §4 and F0.7 S4 actually claim: that a decision is made **in the form**
//! ("decisions the model makes once, visibly, in the policy form"), and that a field so set is then
//! **never overwritten** by the policy job. Both are pinned here against the widget: `set_selected` on the
//! dropdown is what a click is to GTK, and the assertion lands on the same `session_policy` state the logic
//! test checks.
//!
//! One application, one `connect_activate`, one `#[test]` per window case — the shape
//! `tests/suite/cut_policy_form_widgets.rs` uses, because every window shares this process's live session.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::policy;
use naivepost::project::{CutMode, Origin};
use naivepost::ui;
use naivepost::ui::REFUSED_LENGTH;

use crate::common;
use common::{fixture_dir, settle, status_text, wait_until};

static RAN_HAND_OVER: AtomicBool = AtomicBool::new(false);
static RAN_SURVIVES: AtomicBool = AtomicBool::new(false);

/// A window with the fixture loaded, on the Cut page (the page whose pipeline fields are at stake), and no
/// policy form left open from a previous check: the slot outlives a window, so a leftover would make the
/// next press raise instead of build.
fn fresh_window(app: &adw::Application) -> adw::ApplicationWindow {
    ui::close_policy_forms();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Cut");
    window.present();
    settle();
    window
}

/// Open the form the way a person does — the run bar's ⚙ — and hand back its handle. `open_policy_form` is
/// the same slot `press_policy` pushed it into, which is how a test reaches a separate top level.
fn open_form(window: &adw::ApplicationWindow) -> adw::Window {
    ui::line_step_button(window, "policy-button")
        .expect("the run bar carries the policy gear")
        .emit_clicked();
    settle();
    ui::open_policy_form().expect("the gear opened the policy form")
}

/// A control inside the open form, found by the name its builder gave it.
fn control_in(form: &adw::Window, name: &str) -> gtk::Widget {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        let mut child = node.first_child();
        while let Some(current) = child {
            if let Some(found) = walk(&current, name) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }
    walk(form.upcast_ref(), name).unwrap_or_else(|| panic!("the policy form carries {name}"))
}

/// The source badge for a field, as the form shows it.
fn source_word(form: &adw::Window, field: &str) -> String {
    control_in(form, &format!("policy-source-{field}"))
        .downcast::<gtk::Label>()
        .expect("a source cell is a label")
        .label()
        .to_string()
}

/// The reason line for a field.
fn reason_line(form: &adw::Window, field: &str) -> String {
    control_in(form, &format!("policy-reason-{field}"))
        .downcast::<gtk::Label>()
        .expect("a reason cell is a label")
        .label()
        .to_string()
}

/// Entry 4 / S7: a dropdown pick in the form **is** the policy decision — it lands on the live session with
/// `source: user`, the form repaints itself to say who set it, and a value the catalogue refuses is refused
/// out loud instead of half-written.
fn s7_the_form_hands_a_field_over_to_the_policy(app: &adw::Application) {
    let window = fresh_window(app);
    let form = open_form(&window);

    // `policy-edit-cutMode` is the dropdown P.policy.cutMode's catalogue row earns (`["words", "model"]`).
    let before = ui::session_policy(&window).target_length_seconds.value;
    let cut_mode = control_in(&form, "policy-edit-cutMode");
    let picker = cut_mode
        .clone()
        .downcast::<gtk::DropDown>()
        .expect("an enumerated policy field gets a dropdown");
    // "words" is index 0 of the catalogue's own list, not an index this test chose for itself.
    let words_at = naivepost::policy::derived_fields()
        .into_iter()
        .find(|row| row.field == "cutMode")
        .expect("cutMode is a derived field")
        .values
        .iter()
        .position(|word| *word == "words")
        .expect("cutMode takes `words`");
    assert_ne!(
        ui::session_policy(&window).cut_mode.value,
        CutMode::Words,
        "the fixture does not already say `words` — otherwise this pick would prove nothing"
    );

    // THE CLICK: moving a DropDown's selection is what a press on it is to GTK, and it is the signal the
    // form wired itself to.
    picker.set_selected(words_at as u32);
    wait_until("the form's cutMode pick reached the session policy", || {
        ui::session_policy(&window).cut_mode.value == CutMode::Words
    });

    // The state the logic test checks, and who the program says set it.
    let stored = ui::session_policy(&window).cut_mode.clone();
    assert_eq!(stored.value, CutMode::Words, "// P.policy.cutMode");
    assert_eq!(
        stored.origin,
        Origin::User,
        "a form pick is a person's own answer, so S4 must protect it from here on"
    );
    // The form shows the same fact it just wrote, without being rebuilt: this is "visibly".
    assert_eq!(source_word(&form, "cutMode"), "user", "the source badge moved");
    assert!(
        !reason_line(&form, "cutMode").is_empty(),
        "and the reason column says what happened: {:?}",
        reason_line(&form, "cutMode")
    );

    // A write the catalogue refuses leaves the field exactly where it was and is said out loud where the
    // person can read it. The entry is the row that can earn this: its value is free-form seconds, so a
    // person can type something that is not one, and a box that keeps bad text and says nothing is a
    // silent no. `policy::apply`'s refusal names the field and the bad value, which is what the log keeps.
    let entry = control_in(&form, "policy-edit-targetLengthSeconds")
        .downcast::<gtk::Entry>()
        .expect("a seconds field gets an entry");
    let logs_before = ui::window_logs().len();
    entry.set_text("not a number");
    // Enter is the commit signal this editor uses — a half-typed number is not a length — and it is the
    // signal the row is wired to, so the call below is the keypress, not a shortcut around it.
    entry.emit_by_name::<()>("activate", &[]);
    // Wait on the refusal rather than on a number of loop turns: on a loaded machine the handler has not
    // run yet when `settle` runs out, and a check that then reads an empty slice fails for the wrong reason.
    wait_until("the refused length was refused out loud", || {
        ui::window_logs()[logs_before..]
            .iter()
            .any(|line| line.starts_with("!!!") && line.contains("targetLengthSeconds"))
    });
    assert_eq!(
        status_text(&window),
        REFUSED_LENGTH,
        "the status row beside the box says it too, in the short form"
    );
    assert_eq!(
        ui::session_policy(&window).target_length_seconds.value,
        before,
        "a refused length changed nothing"
    );

    form.close();
    window.close();
    RAN_HAND_OVER.store(true, Ordering::SeqCst);
}

/// Entry 4 / S8 (F0.7 S4 on the wire): a field set **in the form** is what the derivation is then refused.
/// Without the editor there is no way for a person to produce a `user` field at all, so this rule — the
/// spec's reason the decisions belong in the form — could only ever be poked from a test.
fn s8_a_hand_set_field_survives_a_re_derivation(app: &adw::Application) {
    let window = fresh_window(app);
    let form = open_form(&window);

    let picker = control_in(&form, "policy-edit-cutMode")
        .downcast::<gtk::DropDown>()
        .expect("cutMode gets a dropdown");
    picker.set_selected(0); // "words"
    wait_until("the form pick reached the policy", || {
        ui::session_policy(&window).cut_mode.value == CutMode::Words
    });

    // The policy job's own door, with a model that asks for the opposite. Newest sender wins, which is how
    // a check stands where the live HTTP leg would otherwise dial.
    ui::set_policy_sender_for_tests(move |_request, _refusals| {
        vec![policy::Proposal {
            field: "cutMode".to_string(),
            value: "model".to_string(),
            because: "the context sounds like a model cut".to_string(),
        }]
    });
    let derived = ui::derive_policy(&window);
    settle();

    let stored = ui::session_policy(&window).cut_mode.clone();
    assert_eq!(
        stored.value,
        CutMode::Words,
        "F0.7 S4: a person's field is never overwritten — the derivation could not move it: {derived:?}"
    );
    assert_eq!(stored.origin, Origin::User, "and it is still credited to them");
    assert_eq!(source_word(&form, "cutMode"), "user", "and still reads that way on the form");

    form.close();
    window.close();
    RAN_SURVIVES.store(true, Ordering::SeqCst);
}

fn round(app: &adw::Application) {
    s7_the_form_hands_a_field_over_to_the_policy(app);
    s8_a_hand_set_field_survives_a_re_derivation(app);
}

fn run_round() {
    common::one_app_round(round);
}

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s7_the_form_hands_a_field_over_to_the_policy() {
    // A working directory of its own: the app keeps `session.naivepost` there, and test files run side by side.
    common::pin_private_cwd();
    run_round();
    assert!(
        RAN_HAND_OVER.load(Ordering::SeqCst),
        "the form-pick check never ran inside activate"
    );
}

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s8_a_hand_set_field_survives_a_re_derivation() {
    common::pin_private_cwd();
    run_round();
    assert!(
        RAN_SURVIVES.load(Ordering::SeqCst),
        "the survives-a-re-derivation check never ran inside activate"
    );
}
