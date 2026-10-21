//! F0.7 S5 — the wire: a real click on ⚙ (and on a tab's ⓘ) opens the policy form, and the form shows
//! what `policy` actually holds rather than a copy someone remembered to refresh.
//!
//! The logic tests (`tests/suite/policy_derive.rs`) prove the derivation, the validation and the S4 rule that
//! a hand-set field is never overwritten. This proves the other half: the buttons reach that code, and
//! the labels a person reads are the state's own value / source / reason — including after the state
//! changes underneath an already-open form.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this window's live session policy and the open-form slot), so they run once in sequence and
//! report through flags the test asserts on — the shape `tests/suite/cut_verb_widgets.rs` uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::policy;
use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use crate::common;
use common::{fixture_dir, settle};

static RAN_GEAR: AtomicBool = AtomicBool::new(false);
static RAN_ROWS: AtomicBool = AtomicBool::new(false);
static RAN_USER_STICKS: AtomicBool = AtomicBool::new(false);
static RAN_INFO: AtomicBool = AtomicBool::new(false);
static RAN_COLUMNS: AtomicBool = AtomicBool::new(false);
static RAN_DERIVE: AtomicBool = AtomicBool::new(false);
static RAN_TARGET: AtomicBool = AtomicBool::new(false);

/// A window with the fixture loaded, on Prepare. Every check starts from no open form: the thread-local
/// outlives a window, so a leftover would make the next press raise instead of build.
fn fresh_window(app: &adw::Application) -> adw::ApplicationWindow {
    ui::close_policy_forms();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();
    window
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the shell carries {name}"))
}

/// A button inside the open form. `button` searches the main window's tree; the form is its own
/// top-level, so its buttons come from here.
fn form_button(name: &str) -> gtk::Button {
    form_widget(name)
        .and_then(|widget| widget.downcast::<gtk::Button>().ok())
        .unwrap_or_else(|| panic!("the policy form carries {name}"))
}

/// Find a named widget anywhere under the open policy form — which is its own top-level, so it comes
/// from `ui::open_policy_form` rather than the main window's tree.
fn form_widget(name: &str) -> Option<gtk::Widget> {
    ui::open_policy_form()
        .and_then(|form| find_by_name_recursive(form.upcast_ref(), name))
}

/// Walk every descendant of `root` looking for the widget name. Written against `gtk::Widget`'s own
/// sibling/child accessors rather than per-container-type downcasts, so it reaches a named box inside an
/// `adw::Window`'s content bin without knowing which widget types Adwaita put in between.
fn find_by_name_recursive(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_by_name_recursive(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

/// A named widget inside the open form, whatever type it is: a value cell is an editor now (a dropdown,
/// a switch or an entry), so the walk returns the widget and each reader downcasts what it expects.
fn control(name: &str) -> gtk::Widget {
    form_widget(name).unwrap_or_else(|| panic!("the policy form carries a control named {name}"))
}

/// The value a row's editor shows, read through the same door the form reads it by.
fn edit_value(name: &str) -> String {
    naivepost::policy_form_edit::value_of(&control(name))
}

fn label_text(name: &str) -> String {
    form_widget(name)
        .and_then(|widget| widget.downcast::<gtk::Label>().ok())
        .map(|label| label.text().to_string())
        .unwrap_or_else(|| panic!("the form has a label named {name}"))
}

/// S5: a real click on the run bar's ⚙ opens the form, and it is showing.
fn check_the_gear_opens_the_form(app: &adw::Application) {
    let window = fresh_window(app);
    assert!(
        !ui::policy_form_open(&window),
        "no form is open before the click — otherwise this proves nothing about the click"
    );

    let gear = button(&window, "policy-button");
    assert_eq!(
        gear.tooltip_text().as_deref(),
        Some(naivepost::ui::policy_form::OPEN_TIP),
        "the gear says where it leads"
    );
    gear.emit_clicked();
    settle();

    assert!(
        ui::policy_form_open(&window),
        "F0.7 S5: clicking ⚙ opened the policy form"
    );
    assert!(
        form_widget("policy-form").is_some(),
        "the form itself is reachable by name"
    );
    window.close();
}

/// S5: one row per `derived_fields()` entry, each printing the policy's own value / source / reason.
fn check_every_field_has_a_row_with_value_source_and_reason(app: &adw::Application) {
    let window = fresh_window(app);
    // Put a derived value with a reason on two fields first, so the form has something specific to show
    // rather than five defaults that all read "the default".
    let marked = ui::set_session_policy_field(
        &window,
        "markingPass",
        "joins",
        "shot in one take per slide",
        Origin::Model,
    );
    assert_eq!(marked, policy::Applied::Set, "// P.policy.markingPass");
    let passed = ui::set_session_policy_field(
        &window,
        "speedPass",
        "off",
        "no time-lapse wanted",
        Origin::Model,
    );
    assert_eq!(passed, policy::Applied::Set, "// P.policy.speedPass");

    button(&window, "policy-button").emit_clicked();
    settle();

    let rows = policy::derived_fields();
    assert_eq!(rows.len(), 6, "six settable fields (five switches + the target length)");
    for entry in rows {
        assert!(
            form_widget(&format!("policy-row-{}", entry.field)).is_some(),
            "a row exists for {}",
            entry.field
        );
        let value = edit_value(&format!("policy-edit-{}", entry.field));
        let source = label_text(&format!("policy-source-{}", entry.field));
        let reason = label_text(&format!("policy-reason-{}", entry.field));
        assert!(!value.is_empty(), "{} shows no value", entry.field);
        assert!(
            ["default", "model", "user"].contains(&source.as_str()),
            "{} shows source {source:?}, which is not one of the three",
            entry.field
        );
        assert!(!reason.is_empty(), "{} shows no reason line at all", entry.field);
    }

    // And the two fields touched above print exactly what the state holds, not a generic string.
    assert_eq!(
        edit_value("policy-edit-markingPass"),
        "joins",
        "// P.policy.markingPass"
    );
    assert_eq!(label_text("policy-source-markingPass"), "model");
    assert_eq!(label_text("policy-reason-markingPass"), "shot in one take per slide");
    assert_eq!(edit_value("policy-edit-speedPass"), "off"); // P.policy.speedPass
    assert_eq!(label_text("policy-reason-speedPass"), "no time-lapse wanted");
    // An untouched field still reads as a default, with no invented reason.
    assert_eq!(label_text("policy-source-cutMode"), "default"); // P.policy.cutMode
    assert_eq!(label_text("policy-reason-cutMode"), "the default");
    window.close();
}

/// S4 shown on screen: a field set by hand keeps its value AND its `user` source when the form is
/// re-opened after a later derivation tries to change it.
fn check_a_hand_set_field_still_reads_user_after_a_later_derivation(app: &adw::Application) {
    let window = fresh_window(app);
    // A person sets cutMode by hand.
    ui::set_session_policy_field(&window, "cutMode", "words", "", Origin::User); // P.policy.cutMode
    assert_eq!(
        ui::session_policy(&window).cut_mode.value,
        naivepost::project::CutMode::Words
    );

    button(&window, "policy-button").emit_clicked();
    settle();
    assert_eq!(edit_value("policy-edit-cutMode"), "words");
    assert_eq!(
        label_text("policy-source-cutMode"),
        "user",
        "the form says who set it"
    );

    // Now the derivation runs against that field — the same `apply` a real `set_policy` call goes
    // through — and must be turned away.
    let outcome = ui::set_session_policy_field(
        &window,
        "cutMode",
        "model",
        "the context sounds like a model cut",
        Origin::Model,
    );
    assert_eq!(outcome, policy::Applied::KeptUser, "F0.7 S4 refused the overwrite");
    assert_eq!(
        ui::session_policy(&window).cut_mode.value,
        naivepost::project::CutMode::Words,
        "the old value survived in the state"
    );

    // Re-opening the form (which takes the raise-and-refresh path) must show the SURVIVING value, not
    // what the rejected proposal asked for.
    button(&window, "policy-button").emit_clicked();
    settle();
    assert_eq!(
        edit_value("policy-edit-cutMode"),
        "words",
        "the raised form refreshed to the live state, not the refused proposal"
    );
    assert_eq!(
        label_text("policy-source-cutMode"),
        "user",
        "and still credits the person"
    );
    assert_eq!(
        label_text("policy-reason-cutMode"),
        "set by you",
        "with their own reason line, not the model's"
    );
    window.close();
}

/// The second way in: the header's ⓘ (badge **6**) reaches the same form as the gear, so the page is not
/// the only door. §1 puts ⓘ once in the header bar and F0.1 S4 syncs its tooltip to the tab showing — so
/// this presses THE header button, after moving to Cut, and asks for the same form.
fn check_the_tab_info_button_opens_the_same_form(app: &adw::Application) {
    let window = fresh_window(app);
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    settle();

    let info = button(&window, "tab-info-button");
    assert_eq!(
        info.tooltip_text().as_deref(),
        Some(naivepost::shell::info_tip(Page::Cut).as_str()),
        "F0.1 S4: the header ⓘ describes the tab now showing — its label plus that tab's help text"
    );
    info.emit_clicked();
    settle();
    assert!(
        ui::policy_form_open(&window),
        "F0.7 S5: the header ⓘ opened the policy form too"
    );
    assert!(form_widget("policy-form").is_some());
    window.close();
}

fn window_round() {
    common::one_app_round(|app| {
        check_the_gear_opens_the_form(app);
        RAN_GEAR.store(true, Ordering::SeqCst);
        check_every_field_has_a_row_with_value_source_and_reason(app);
        RAN_ROWS.store(true, Ordering::SeqCst);
        check_a_hand_set_field_still_reads_user_after_a_later_derivation(app);
        RAN_USER_STICKS.store(true, Ordering::SeqCst);
        check_the_tab_info_button_opens_the_same_form(app);
        RAN_INFO.store(true, Ordering::SeqCst);
        check_the_four_columns_and_the_two_buttons(app);
        RAN_COLUMNS.store(true, Ordering::SeqCst);
        check_the_gear_derives_before_it_paints(app);
        RAN_DERIVE.store(true, Ordering::SeqCst);
        check_the_target_length_row_shows_the_derived_number(app);
        RAN_TARGET.store(true, Ordering::SeqCst);
    });
}

/// S5: the form draws four columns, one per answer the spec's picture labels, and closes from its own
/// button. The overlap this guards was real: the reason used to sit at `(1, row + 1)` while the caller
/// advanced `row` by one, so each field's NAME landed on the PREVIOUS field's reason — `cutMode`
/// printed over `default`. That is why the assertion below compares every reason against every other
/// field's name rather than only checking the reason is non-empty.
fn check_the_four_columns_and_the_two_buttons(app: &adw::Application) {
    let window = fresh_window(app);
    // A model-sourced field to reset away from, so "back to defaults" has something to undo.
    ui::set_session_policy_field(
        &window,
        "cutMode",
        "words",
        "cut on the words, not the model",
        Origin::Model,
    );
    button(&window, "policy-button").emit_clicked();
    settle();

    for caption in ["field", "value", "source", "because"] {
        assert_eq!(
            label_text(&format!("policy-caption-{caption}")),
            caption,
            "img/03-policy-form.svg spells the four captions lowercase"
        );
    }

    let fields: Vec<&str> = policy::derived_fields().iter().map(|e| e.field).collect();
    for entry in policy::derived_fields() {
        let value = edit_value(&format!("policy-edit-{}", entry.field));
        let source = label_text(&format!("policy-source-{}", entry.field));
        let reason = label_text(&format!("policy-reason-{}", entry.field));
        assert!(!value.is_empty(), "{} shows no value", entry.field);
        assert!(!reason.is_empty(), "{} shows no reason", entry.field);
        // The regression guard: a reason that reads as another field's name means two rows share a
        // grid line again.
        for other in &fields {
            if *other != entry.field {
                assert_ne!(
                    reason, *other,
                    "{}'s reason reads {:?}, which is another field's name — the rows overlap",
                    entry.field, reason
                );
            }
        }
        assert!(
            ["default", "model", "user"].contains(&source.as_str()),
            "{} shows source {source:?}",
            entry.field
        );
    }

    assert_eq!(
        label_text("policy-footer"),
        "... every field of 10-parameters.md \u{a7}2",
        "the footer says the page is the whole section 2 catalogue"
    );

    let reset = form_button("policy-reset-button");
    let close = form_button("policy-close-button");
    assert_eq!(reset.label().as_deref(), Some("Reset to defaults"));
    assert_eq!(close.label().as_deref(), Some("Close"));

    // Reset puts the derived fields back at their §2 defaults — with origin `default`, NOT `user`,
    // so a later derivation can still speak to them (see policy::reset_to_defaults).
    assert_eq!(edit_value("policy-edit-cutMode"), "words"); // P.policy.cutMode
    reset.emit_clicked();
    settle();
    assert_eq!(edit_value("policy-edit-cutMode"), "model", "// P.policy.cutMode");
    assert_eq!(label_text("policy-source-cutMode"), "default");
    assert_eq!(label_text("policy-reason-cutMode"), "the default");
    assert_eq!(
        ui::session_policy(&window).cut_mode.origin,
        Origin::Default,
        "a reset leaves the field open to a later derivation"
    );

    close.emit_clicked();
    settle();
    assert!(
        !ui::policy_form_open(&window),
        "Close must dismiss the form"
    );
}

#[test]
fn f0_7_s5_policy_form_reopens_from_the_menu_through_a_real_click() {
    common::pin_private_cwd();
    window_round();
    assert!(RAN_GEAR.load(Ordering::SeqCst), "the ⚙ check never ran");
    assert!(RAN_ROWS.load(Ordering::SeqCst), "the row-by-row check never ran");
    assert!(
        RAN_USER_STICKS.load(Ordering::SeqCst),
        "the hand-set-field check never ran"
    );
    assert!(RAN_INFO.load(Ordering::SeqCst), "the ⓘ check never ran");
    assert!(
        RAN_COLUMNS.load(Ordering::SeqCst),
        "the four-column / buttons check never ran"
    );
    assert!(
        RAN_DERIVE.load(Ordering::SeqCst),
        "the derive-before-paint check never ran"
    );
    assert!(
        RAN_TARGET.load(Ordering::SeqCst),
        "the target-length row check never ran"
    );
}

/// F0.7 S2-S4 through the wire: pressing ⚙ on a session whose User Context has never been derived
/// runs the derivation BEFORE painting, so the labels a person reads are what the model just decided.
///
/// The stub sender stands in for the LLM (no server in this harness); everything else is the real
/// path — the real button, `press_policy`, `derive_policy`, `policy::derive` and `policy::apply`.
/// The assertion is deliberately made through the same widgets the logic test's state drives: if the
/// click reached nothing, `cutMode` would still read its default.
fn check_the_gear_derives_before_it_paints(app: &adw::Application) {
    use naivepost::policy::{self, Proposal};
    ui::close_policy_forms();
    let window = fresh_window(app);
    // A context set but never derived from: exactly the state a person leaves after typing.
    ui::set_session_context(&window, "cut on the words, not the model");
    assert!(
        !ui::policy_is_current(&window, "cut on the words, not the model"),
        "a fresh window has derived nothing"
    );

    // The model answers one field, with a reason we can look for on screen.
    ui::set_policy_sender_for_tests(move |_req: &policy::DerivationRequest, refusals: &[String]| {
        if !refusals.is_empty() {
            return Vec::new();
        }
        vec![Proposal {
            field: "cutMode".into(),
            value: "words".into(),
            because: "cut on the words, not the model".into(),
        }]
    });

    button(&window, "policy-button").emit_clicked();
    settle();

    // P.policy.cutMode — derived, sourced to the model, carrying its reason.
    assert_eq!(edit_value("policy-edit-cutMode"), "words", "// P.policy.cutMode");
    assert_eq!(label_text("policy-source-cutMode"), "model");
    assert_eq!(
        label_text("policy-reason-cutMode"),
        "cut on the words, not the model"
    );
    assert_eq!(
        ui::session_policy(&window).cut_mode.origin,
        Origin::Model,
        "the stored policy says who set it"
    );
    assert!(
        ui::policy_is_current(&window, "cut on the words, not the model"),
        "the tracker now agrees the policy answers this context"
    );
}

/// §12 entry 21 on screen: a target length written in plain words is DERIVED by F0.7 and SHOWN in the form,
/// instead of the prototype's regex that silently left `0 = none`. Driven through the same door as the check
/// above — a real ⚙ press, which runs `derive_policy` before painting — with no model stub needed, because a
/// spelled length is arithmetic, not interpretation.
fn check_the_target_length_row_shows_the_derived_number(app: &adw::Application) {
    ui::close_policy_forms();
    let window = fresh_window(app);
    // The §12 example itself, in a person's words. No sender stub: nothing here asks a model.
    ui::set_session_context(&window, "keep it to half an hour");
    button(&window, "policy-button").emit_clicked();
    settle();

    // P.policy.targetLengthSeconds — the derived 1800 s sits in the entry as seconds, credited to the
    // derivation. The form's own prose still spells it `30 min` (`policy_form::length_word`); the EDITOR
    // holds the number, because that is what a person can correct without re-parsing a unit.
    assert_eq!(
        edit_value("policy-edit-targetLengthSeconds"),
        "1800",
        "// P.policy.targetLengthSeconds — the derived length is on the form"
    );
    assert_eq!(label_text("policy-source-targetLengthSeconds"), "model");
    assert!(
        !label_text("policy-reason-targetLengthSeconds").is_empty(),
        "a derived row says why"
    );
    window.close();

    // And the other half of §10's row: with no context there is no target, and the editor holds 0 (the
    // value "no target" carries) rather than an empty box that cannot be told from a mistake.
    ui::close_policy_forms();
    let bare = fresh_window(app);
    button(&bare, "policy-button").emit_clicked();
    settle();
    assert_eq!(
        edit_value("policy-edit-targetLengthSeconds"),
        "0",
        "no target is the number 0 in the editor, not an empty box"
    );
    assert_eq!(label_text("policy-source-targetLengthSeconds"), "default");
}
