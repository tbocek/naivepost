//! F0.7's policy doors and F0.13's gear: the User Context tracker, the debounced derivation, the session
//! policy's read/write doors, and the wires into them. Split out of `window.rs`, which is past its size
//! budget; its doors are re-exported through `window`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout;
use crate::policy;
use crate::project::Origin;
use crate::ui::settings;
use crate::ui::window::SESSION;
use crate::ui::window_doors::find_widget_by_name;

pub(crate) fn wire_settings(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        settings::open_with(&window, crate::settings_probe::live_provider());
    });
}

pub fn live_session_policy() -> crate::project::Policy {
    // A run's worker holds no window session: it reads the policy the run saved before its first stage
    // (F1.1 S2), which is the one it runs under.
    SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|project| project.borrow().policy.clone())
        .or_else(|| {
            crate::project::load(&crate::startup::open_dir())
                .ok()
                .map(|project| project.policy)
        })
        .unwrap_or_default()
}

/// The shared half of every writer: validate through `policy::apply`, then stamp the origin on a Set.
pub fn write_policy_field(
    field: &str,
    value: &str,
    because: &str,
    origin: Origin,
) -> policy::Applied {
    let Some(project) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return policy::Applied::Refused("this window holds no session".to_string());
    };
    let proposal = policy::Proposal {
        field: field.to_string(),
        value: value.to_string(),
        because: because.to_string(),
    };
    let outcome = policy::apply(&mut project.borrow_mut().policy, &proposal);
    // Only after a Set does the origin override make sense: refusing first and stamping `User` onto an
    // untouched field would report something that never happened.
    if outcome == policy::Applied::Set {
        stamp_origin(&mut project.borrow_mut().policy, field, origin);
    }
    outcome
}

/// Move one field's `source` to `origin` without touching its value or reason.
fn stamp_origin(policy: &mut crate::project::Policy, field: &str, origin: Origin) {
    match field {
        "markingPass" => policy.marking_pass.origin = origin,
        "cutMode" => policy.cut_mode.origin = origin,
        "captionsPass" => policy.captions_pass.origin = origin,
        "speedPass" => policy.speed_pass.origin = origin,
        "decorationsPass" => policy.decorations_pass.origin = origin,
        "targetLengthSeconds" => policy.target_length_seconds.origin = origin,
        _ => {}
    }
}

/// Who answers one derivation request, injected so a check can stub the HTTP layer (`POLICY_SENDERS`).
type PolicySenders = std::cell::RefCell<
    Vec<
        Rc<
            std::cell::RefCell<
                dyn FnMut(&policy::DerivationRequest, &[String]) -> Vec<policy::Proposal>,
            >,
        >,
    >,
>;

thread_local! {
    static POLICY_SENDERS: PolicySenders = const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// Per-window record of which User Context the policy was derived from (F0.7). Newest last.
    pub(crate) static POLICY_TRACKERS: std::cell::RefCell<Vec<Rc<RefCell<policy::Tracker>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    static POLICY_DEBOUNCE: std::cell::RefCell<Vec<glib::SourceId>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub fn main_window() -> adw::ApplicationWindow {
    built_window().expect("a policy request happens inside a built window")
}

pub fn built_window() -> Option<adw::ApplicationWindow> {
    POLICY_WATCH_WINDOW.with(|watch| watch.borrow().clone())
}

thread_local! {
    pub(crate) static POLICY_WATCH_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

pub fn set_policy_sender_for_tests(
    sender: impl FnMut(&policy::DerivationRequest, &[String]) -> Vec<policy::Proposal> + 'static,
) {
    POLICY_SENDERS.with(|senders| {
        senders
            .borrow_mut()
            .push(Rc::new(std::cell::RefCell::new(sender)))
    });
}

/// The window's User Context text, read through the session the bench writes.
pub(crate) fn user_context() -> String {
    SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|session| session.borrow().context.clone())
        .unwrap_or_default()
}

/// The window's User Context text, as the bench and `derive_policy` read it.
pub fn set_session_context(text: &str) {
    if let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) {
        session.borrow_mut().context = text.to_string();
    }
}

pub fn policy_is_current(context: &str) -> bool {
    POLICY_TRACKERS
        .with(|trackers| trackers.borrow().last().cloned())
        .is_some_and(|tracker| !tracker.borrow().needs_derive(context))
}

fn live_policy_answer(
    req: &policy::DerivationRequest,
    refusals: &[String],
) -> Vec<policy::Proposal> {
    // `window_policy::live_session_policy` is the same slot the ⚙ form reads, so the model's view of the fields and the
    // form's cannot drift.
    crate::policy_call::set_live_policy(&live_session_policy());
    crate::policy_call::answer(req, refusals)
}

pub fn derive_policy() -> policy::Derived {
    let context = user_context();
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return policy::Derived {
            used_defaults: true,
            ..policy::Derived::default()
        };
    };
    let mut policy = session.borrow().policy.clone();
    let derived = match POLICY_SENDERS.with(|senders| senders.borrow().last().cloned()) {
        Some(sender) => policy::derive(&mut policy, &context, &mut *sender.borrow_mut()),
        None => policy::derive(&mut policy, &context, &mut live_policy_answer),
    };
    finish_derivation(&session, policy, derived, &context)
}

pub(crate) fn answerer_installed() -> bool {
    POLICY_SENDERS.with(|senders| !senders.borrow().is_empty())
}

pub(crate) fn apply_derivation(
    policy: crate::project::Policy,
    derived: policy::Derived,
    context: &str,
) {
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    let derived = finish_derivation(&session, policy, derived, context);
    if !derived.used_defaults {
        crate::ui::window::log_line(&format!("policy: {}", derived.status()));
    }
}

fn finish_derivation(
    session: &Rc<RefCell<crate::project::Project>>,
    policy: crate::project::Policy,
    mut derived: policy::Derived,
    context: &str,
) -> policy::Derived {
    let before = session.borrow().policy.clone();
    session.borrow_mut().policy = policy;
    // §12 entry 21: the target-length read happened inside `policy::derive` (the one door every caller uses),
    // and its outcome is said here. A length that was read is said by `derived_log` below with its value and
    // reason; what this line adds is the gap — a length mentioned that could not be read, which must not pass
    // for a silent "no target".
    if let Some(line) = policy::target_length_log(&derived) {
        crate::ui::window::log_line(&line);
    }
    // S4: every value this derivation stored is said with its field, value and `because`, so what the model
    // decided is visible without a form to open — §12's policy decisions are made "visibly in the log".
    for line in policy::derived_log(&derived, &session.borrow().policy) {
        crate::ui::window::log_line(&line);
    }
    for line in policy::refused_log(&derived) {
        crate::ui::window::log_line(&line);
    }
    // Record even a defaults-only derivation: an empty context IS an answer, and remembering it is
    // what stops every ▶ from re-asking about a context nobody intends to fill.
    if let Some(tracker) = POLICY_TRACKERS
        .with(|trackers| trackers.borrow().last().cloned())
        .filter(|_| !derived.model_fault)
    {
        tracker.borrow_mut().mark_derived(context);
    }
    // §03: a changed markingPass invalidates the marks, so Prepare re-runs its marking pass instead
    // of resuming off output made under a different pass.
    let dropped = policy::invalidated_marks(&before, &session.borrow().policy);
    if !dropped.is_empty() {
        let dir = crate::startup::open_dir();
        if let Ok(tree) = layout::Tree::new(&dir) {
            for name in dropped {
                let path = if name == "final.txt" {
                    tree.final_txt()
                } else {
                    tree.retakes_tsv()
                };
                let _ = std::fs::remove_file(path);
            }
        }
    }
    // Nothing refused and nothing landed but the model was reached: say so, rather than reporting a
    // derivation of zero fields as if it were news.
    let _ = &mut derived;
    // §4's Setup row: the derivation IS a model call (`policy ×1`), and this is the one door that makes
    // it, so the bill is printed here rather than on a page. Printed after the derivation, so the log
    // says what was asked rather than what is about to be.
    crate::ui::window::log_line(&crate::ui::run_bill::setup_line());
    derived
}

pub(crate) fn schedule_policy_derive(window: &adw::ApplicationWindow) {
    let held = window.clone();
    POLICY_DEBOUNCE.with(|pending| {
        if let Some(id) = pending.borrow_mut().pop() {
            id.remove();
        }
        let id = glib::timeout_add_local(
            std::time::Duration::from_millis(policy::DEBOUNCE_MS),
            move || {
                POLICY_DEBOUNCE.with(|p| p.borrow_mut().clear());
                if POLICY_SENDERS.with(|senders| senders.borrow().is_empty()) {
                    derive_in_background(&held);
                } else {
                    let derived = derive_policy();
                    say_status(&held, &derived);
                }
                glib::ControlFlow::Break
            },
        );
        pending.borrow_mut().push(id);
    });
}

fn say_status(window: &adw::ApplicationWindow, derived: &policy::Derived) {
    if let Some(status) = find_widget_by_name(window.upcast_ref(), "status-line")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
    {
        status.set_text(&derived.status());
    }
}

thread_local! {
    static DERIVE_GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn derive_in_background(window: &adw::ApplicationWindow) {
    let context = user_context();
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    let policy = session.borrow().policy.clone();
    let live = live_session_policy();
    let generation = DERIVE_GENERATION.with(|newest| {
        newest.set(newest.get() + 1);
        newest.get()
    });
    let (tell, told) = std::sync::mpsc::channel();
    let asked = context.clone();
    std::thread::spawn(move || {
        crate::policy_call::set_live_policy(&live);
        let mut policy = policy;
        let derived = policy::derive(&mut policy, &asked, &mut crate::policy_call::answer);
        // What the leg said went to THIS thread's log sink; it travels back with the answer.
        let said = super::window_logs::window_logs();
        let _ = tell.send((policy, derived, said));
    });
    let held = window.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        match told.try_recv() {
            Ok((policy, derived, said)) => {
                for line in said {
                    crate::ui::window::log_line(&line);
                }
                let newest = DERIVE_GENERATION.with(|newest| newest.get()) == generation;
                if newest && user_context() == context {
                    let derived = finish_derivation(&session, policy, derived, &context);
                    say_status(&held, &derived);
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break,
        }
    });
}
