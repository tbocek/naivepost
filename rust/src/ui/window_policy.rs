//! F0.7's policy doors and F0.13's gear: the ⚙/ⓘ form's open slot, the User Context tracker,
//! the debounced derivation, the session policy's two read/write doors, and the wires into them.
//! Split out of `window.rs`, which is past its size budget; its doors are re-exported through
//! `window`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout;
use crate::policy;
use crate::project::Origin;
use crate::startup;
use crate::ui::policy_form;
use crate::ui::settings;
use crate::ui::window::SESSION;
use crate::ui::window_doors::find_widget_by_name;

/// F0.13: the gear opens the Settings dialog over this window. Nothing is decided here — the rows,
/// their badges and Test All live in [`settings`], and every pass rule lives in [`crate::checks`].
pub(crate) fn wire_settings(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        settings::open_with(&window, crate::settings_probe::live_provider());
    });
}

/// F0.7 S5: where a press of the policy control goes — build the form off this window's live policy and
/// show it. The button decides nothing: [`policy_form::build`] reads every row from
/// [`policy::derived_fields`] plus the [`Project`]'s `Policy`, so what shows is the current state and
/// not a snapshot someone remembered to refresh.
///
/// A form already open is raised rather than duplicated: pressing ⚙ twice should not put two copies of
/// one screen on the desk.
pub fn press_policy(window: &adw::ApplicationWindow) {
    // F0.7's door into the form also closes the staleness gap: if this window has never derived from
    // its User Context, derive now so the first thing the form paints is what the model said rather
    // than a default nobody chose. The tracker makes this free on every later open.
    let context = user_context(window);
    if POLICY_TRACKERS
        .with(|trackers| trackers.borrow().last().cloned())
        .is_some_and(|tracker| tracker.borrow().needs_derive(&context))
    {
        derive_policy(window);
    }
    if let Some(existing) = POLICY_FORMS.with(|forms| forms.borrow().last().cloned()) {
        // Rebuild the rows in place rather than only raising the old window: the policy may have changed
        // since this form was opened, and a form that shows last-opened values is exactly the staleness
        // S5 exists to avoid.
        refresh_policy_form(&existing);
        existing.present();
        return;
    }
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    // Read at open time, through the same handle Open/Rescan write, so the form cannot drift from the
    // project it describes.
    let policy = session.borrow().policy.clone();
    let form = policy_form::build(Some(window.upcast_ref()), &policy);
    POLICY_FORMS.with(|forms| forms.borrow_mut().push(form.clone()));
    form.present();
}

/// Whether this window has a policy form showing.
pub fn policy_form_open(_window: &adw::ApplicationWindow) -> bool {
    POLICY_FORMS.with(|forms| !forms.borrow().is_empty())
}

/// The open policy form, newest first — the same handle `press_policy` raised. Published so a test can
/// read the labels inside it without walking GTK's application window list (which needs a running
/// `GApplication` registration this harness does not have).
pub fn open_policy_form() -> Option<adw::Window> {
    POLICY_FORMS.with(|forms| forms.borrow().last().cloned())
}

/// Re-read the live policy into an already-open form's labels, in place.
///
/// The row widgets are found by their stable names rather than kept in a struct, so a rebuild needs no
/// bookkeeping and cannot hold a stale handle — the same reason the buttons are looked up per press.
/// the form is showing. Public so `policy_form`'s Re-derive button repaints through exactly the same
/// path a policy change already uses.
pub fn refresh_policy_form(form: &adw::Window) {
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    let policy = session.borrow().policy.clone();
    for entry in policy::derived_fields() {
        if let Some(label) =
            find_widget_by_name(form.upcast_ref(), &format!("policy-value-{}", entry.field))
        {
            if let Ok(label) = label.downcast::<gtk::Label>() {
                label.set_text(&policy_form::value_word(&policy, entry.field));
            }
        }
        if let Some(label) =
            find_widget_by_name(form.upcast_ref(), &format!("policy-source-{}", entry.field))
        {
            if let Ok(label) = label.downcast::<gtk::Label>() {
                label.set_text(policy_form::origin_word(policy_form::field_origin(&policy, entry.field)));
            }
        }
        if let Some(label) =
            find_widget_by_name(form.upcast_ref(), &format!("policy-reason-{}", entry.field))
        {
            if let Ok(label) = label.downcast::<gtk::Label>() {
                label.set_text(&policy_form::reason_word(&policy, entry.field));
            }
        }
    }
}

/// The live policy this window's session holds — the same `Rc` Open/Rescan write, read through the
/// newest slot like [`session_sources`] does. Published so a test can set a field's origin to `User`
/// and watch what the form then prints, rather than poking the thread-local from outside.
pub fn session_policy(window: &adw::ApplicationWindow) -> crate::project::Policy {
    let _ = window;
    SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|project| project.borrow().policy.clone())
        .unwrap_or_default()
}

/// Put a value, source and reason on one policy field of this window's live session.
///
/// A test seam for S4/S5: it goes through `policy::apply`, so what lands is exactly what the derivation
/// path would have written, and the caller only overrides the origin afterwards to stand in for "a
/// person set this by hand".
pub fn set_session_policy_field(
    window: &adw::ApplicationWindow,
    field: &str,
    value: &str,
    because: &str,
    origin: Origin,
) -> policy::Applied {
    let _ = window;
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
    // untouched field would show the form something that never happened.
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
        _ => {}
    }
}

/// Reset the live session's policy to the §2 defaults and repaint every open form.
///
/// The form asks for this; it does not decide anything. `policy::reset_to_defaults` owns the rule,
/// this only reaches the newest session (the one Save writes) and refreshes what is showing, so a
/// reset cannot leave a stale clone behind that overwrites the reset on the next save.
pub fn reset_policy_to_defaults() {
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    policy::reset_to_defaults(&mut session.borrow_mut().policy);
    POLICY_FORMS.with(|forms| {
        for form in forms.borrow().iter() {
            refresh_policy_form(form);
        }
    });
}

/// Drop one form from the open-forms slot, so `policy_form_open` stops reporting a window that is on
/// its way out. Called from the form's own close path: the thread-local would otherwise keep a closed
/// window alive and make `press_policy` raise something invisible.
pub fn forget_policy_form(form: &adw::Window) {
    // The index is found with the list only briefly borrowed, and removal goes through a queued idle
    // rather than happening inside the close signal: `close()` runs the delete path synchronously and
    // anything that reads this slot from there (a repaint, a re-entrant press) would meet a borrow held
    // across the signal and panic "RefCell already borrowed".
    let index = POLICY_FORMS.with(|forms| {
        forms
            .borrow()
            .iter()
            .position(|open| std::ptr::eq(open.as_ptr(), form.as_ptr()))
    });
    let Some(index) = index else { return };
    glib::idle_add_local(move || {
        POLICY_FORMS.with(|forms| {
            let mut forms = forms.borrow_mut();
            if index < forms.len() {
                forms.remove(index);
            }
        });
        glib::ControlFlow::Break
    });
}

/// Who answers one derivation request, injected so a check can stub the HTTP layer (`POLICY_SENDERS`).
type PolicySenders = std::cell::RefCell<
    Vec<Rc<std::cell::RefCell<dyn FnMut(&policy::DerivationRequest, &[String]) -> Vec<policy::Proposal>>>>,
>;

thread_local! {
    /// Who answers a derivation's request, newest last — the same newest-slot rule as
    /// [`WINDOW_SHELLS`]. Injected rather than hard-wired to the HTTP layer so the whole of F0.7 S2-S4
    /// is testable with no server: a check installs a stub here and drives the real ⚙.
    static POLICY_SENDERS: PolicySenders = const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// Per-window record of which User Context the policy was derived from (F0.7). Newest last.
    pub(crate) static POLICY_TRACKERS: std::cell::RefCell<Vec<Rc<RefCell<policy::Tracker>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The pending debounced derivation, so a burst of keystrokes replaces one timer instead of
    /// queueing one per letter. `policy::DEBOUNCE_MS` is the wait; without this slot every keystroke
    /// would fire its own request and the last few would race.
    static POLICY_DEBOUNCE: std::cell::RefCell<Vec<glib::SourceId>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The newest window this process built. The run-bar handler is wired before the handle is in scope at
/// the call site, so it reads the window back from [`POLICY_WATCH_WINDOW`] — the same newest-slot rule
/// every other per-window piece here uses. Panics for a caller that cannot legitimately run outside a
/// window; a door a headless flow can reach takes [`built_window`] instead.
pub fn main_window() -> adw::ApplicationWindow {
    built_window().expect("a policy request happens inside a built window")
}

/// [`main_window`] without the promise: `None` when this thread has never built a window. A door a
/// headless flow can reach (a chat call made with no shell open) reads through this instead, because a
/// log line is never a reason to fail the thing that wrote it.
pub fn built_window() -> Option<adw::ApplicationWindow> {
    POLICY_WATCH_WINDOW.with(|watch| watch.borrow().clone())
}

thread_local! {
    /// The window the bench's User Context editor belongs to, newest last. The bench is built before
    /// the window handle reaches `wire_*`, so its keystroke hook reads the window back from here —
    /// the same newest-slot rule as [`POLICY_TRACKERS`].
    pub(crate) static POLICY_WATCH_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

/// Install the answer used by later derivations in this process (a test seam). Newest wins, so a
/// check can put its own model in front of whatever `build_window` installed.
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
pub(crate) fn user_context(window: &adw::ApplicationWindow) -> String {
    let _ = window;
    SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|session| session.borrow().context.clone())
        .unwrap_or_default()
}

/// The window's User Context text, as the bench and `derive_policy` read it.
pub fn set_session_context(window: &adw::ApplicationWindow, text: &str) {
    let _ = window;
    if let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) {
        session.borrow_mut().context = text.to_string();
    }
}

/// Whether this window's policy still matches its User Context (F0.7's "changed" question, published
/// so a widget test asserts the same fact the ▶ path acted on).
pub fn policy_is_current(window: &adw::ApplicationWindow, context: &str) -> bool {
    let _ = window;
    POLICY_TRACKERS
        .with(|trackers| trackers.borrow().last().cloned())
        .is_some_and(|tracker| !tracker.borrow().needs_derive(context))
}

/// The live answer a derivation dials through: the real HTTP leg, with the session's policy handed to it
/// so `get_context` answers with the fields as they stand. `set_policy_sender_for_tests` still wins when
/// a check installs one (newest-slot), which is how the widget tests keep injecting.
fn live_policy_answer(req: &policy::DerivationRequest, refusals: &[String]) -> Vec<policy::Proposal> {
    // `session_policy` is the same slot the ⚙ form reads, so the model's view of the fields and the
    // form's cannot drift.
    crate::policy_call::set_live_policy(&session_policy(&main_window()));
    crate::policy_call::answer(req, refusals)
}

/// Derive this window's editing policy from its User Context now (F0.7 S1-S4), and drop the marks a
/// changed `markingPass` invalidates.
///
/// Thin on purpose: every rule is `policy::derive`'s. This reads the two things the rule needs (the
/// context, the live policy), sends them through the installed answer, records the result on the
/// tracker, and lets the caller print `Derived::status()` where it belongs.
pub fn derive_policy(window: &adw::ApplicationWindow) -> policy::Derived {
    let context = user_context(window);
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return policy::Derived {
            used_defaults: true,
            ..policy::Derived::default()
        };
    };
    let before = session.borrow().policy.clone();
    let mut derived = {
        let mut policy = session.borrow().policy.clone();
        let out = match POLICY_SENDERS.with(|senders| senders.borrow().last().cloned()) {
            Some(sender) => policy::derive(&mut policy, &context, &mut *sender.borrow_mut()),
            None => policy::derive(&mut policy, &context, &mut live_policy_answer),
        };
        session.borrow_mut().policy = policy;
        out
    };
    // Record even a defaults-only derivation: an empty context IS an answer, and remembering it is
    // what stops every ▶ from re-asking about a context nobody intends to fill.
    if let Some(tracker) = POLICY_TRACKERS.with(|trackers| trackers.borrow().last().cloned()) {
        tracker.borrow_mut().mark_derived(&context);
    }
    // §03: a changed markingPass invalidates the marks, so Prepare re-runs its marking pass instead
    // of resuming off output made under a different pass.
    let dropped = policy::invalidated_marks(&before, &session.borrow().policy);
    if !dropped.is_empty() {
        let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
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

/// Queue one debounced derivation (F0.7's "debounce"). Called from the User Context editor on every
/// keystroke; a pending one is cancelled first, so typing a paragraph costs one request.
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
                let derived = derive_policy(&held);
                if let Some(status) = find_widget_by_name(held.upcast_ref(), "status-line")
                    .and_then(|w| w.downcast::<gtk::Label>().ok())
                {
                    status.set_text(&derived.status());
                }
                POLICY_FORMS.with(|forms| {
                    for form in forms.borrow().iter() {
                        refresh_policy_form(form);
                    }
                });
                glib::ControlFlow::Break
            },
        );
        pending.borrow_mut().push(id);
    });
}

/// test between rounds: the thread-local outlives a window, and a leftover form would make
/// `press_policy`'s raise-instead-of-rebuild branch fire when the check wants a fresh build.
pub fn close_policy_forms() {
    // Drained BEFORE any `close()` runs: closing now fires the form's own `close-request`, which calls
    // `forget_policy_form`, and mutating this list while it is mutably borrowed would panic.
    let open: Vec<adw::Window> = POLICY_FORMS.with(|forms| forms.borrow_mut().drain(..).collect());
    for form in open {
        form.close();
    }
}

/// F0.7: the run bar's policy gear, wired after the widgets exist. Its sibling in the header is each
/// tab's ⓘ; both call the same [`press_policy`], so the form is reachable from the ⚙ AND from ⓘ with
/// one implementation behind them.
pub(crate) fn wire_policy(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| press_policy(&window));
}

thread_local! {
    /// The policy form this window has open (F0.7 S5), newest last. Held so a second press on ⚙ or ⓘ
/// raises the form that is already there instead of stacking a third copy of the same screen, and so
/// `policy_form_open` can answer whether one is showing without searching the widget tree.
static POLICY_FORMS: std::cell::RefCell<Vec<adw::Window>> = const { std::cell::RefCell::new(Vec::new()) };
}
