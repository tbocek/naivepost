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

/// F0.13: the gear opens the Settings dialog over this window. Nothing is decided here — the rows,
/// their badges and Test All live in [`settings`], and every pass rule lives in [`crate::checks`].
pub(crate) fn wire_settings(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        settings::open_with(&window, crate::settings_probe::live_provider());
    });
}

/// The live policy this window's session holds — the same `Rc` Open/Rescan write, read through the
/// newest slot like [`session_sources`] does. Every reader of the live policy — the pass gates,
/// `get_context`, the tests — goes through this rather than the thread-local.
pub fn session_policy(window: &adw::ApplicationWindow) -> crate::project::Policy {
    let _ = window;
    live_session_policy()
}

/// [`session_policy`] without needing a window handle: a flow that may run with no shell open (a tool call
/// made headless) reads the live policy through this.
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
pub(crate) fn write_policy_field(
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

/// Put a value, source and reason on one policy field of this window's live session.
///
/// Goes through `policy::apply`, so what lands is exactly what the derivation path would have written; the
/// caller may override the origin afterwards. The program's own writers are the derivation and
/// `tool:set_policy`, both of which reach the same door through [`policy::apply`].
pub fn set_session_policy_field(
    window: &adw::ApplicationWindow,
    field: &str,
    value: &str,
    because: &str,
    origin: Origin,
) -> policy::Applied {
    let _ = window;
    write_policy_field(field, value, because, origin)
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
fn live_policy_answer(
    req: &policy::DerivationRequest,
    refusals: &[String],
) -> Vec<policy::Proposal> {
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
    let mut policy = session.borrow().policy.clone();
    let derived = match POLICY_SENDERS.with(|senders| senders.borrow().last().cloned()) {
        Some(sender) => policy::derive(&mut policy, &context, &mut *sender.borrow_mut()),
        None => policy::derive(&mut policy, &context, &mut live_policy_answer),
    };
    finish_derivation(&session, policy, derived, &context)
}

/// Whether a test has put its own answerer in place of the policy model: that one answers at once, so the ▶ door
/// derives on the window thread with it; the live model is asked on the run's worker ([`apply_derivation`]).
pub(crate) fn answerer_installed() -> bool {
    POLICY_SENDERS.with(|senders| !senders.borrow().is_empty())
}

/// The ▶ door's derivation, made on the run's worker and taken back here: the same second half
/// [`derive_policy`] runs, and the same line the door says.
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

/// The derivation's second half, on the GTK thread: the derived policy lands on the session, what it decided
/// is said, the tracker remembers the context, and the marks a changed `markingPass` invalidates go.
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
    // Printed before the run bill so the log reads "what changed" then "what it cost".
    for line in policy::derived_log(&derived, &session.borrow().policy) {
        crate::ui::window::log_line(&line);
    }
    // Record even a defaults-only derivation: an empty context IS an answer, and remembering it is
    // what stops every ▶ from re-asking about a context nobody intends to fill.
    if let Some(tracker) = POLICY_TRACKERS.with(|trackers| trackers.borrow().last().cloned()) {
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
                // A test's installed answer lives on this thread (`Rc`), so it is asked here; the live one
                // dials a model, which takes seconds to minutes, and goes to a worker so the window keeps
                // painting and taking keystrokes (the first rewrite asked on this thread and froze the window,
                // "not responding", for as long as the model took).
                if POLICY_SENDERS.with(|senders| senders.borrow().is_empty()) {
                    derive_in_background(&held);
                } else {
                    let derived = derive_policy(&held);
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
    /// The newest debounced derivation: an older one's answer is dropped when it lands, because the context it
    /// was asked about is no longer the one on the page.
    static DERIVE_GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// F0.7's debounced derivation, its model call on a worker. The session's policy and context are read here;
/// the worker derives and hands back the policy, the outcome and its own log lines; the answer lands only if
/// it is still the newest and the context is still the one asked about.
fn derive_in_background(window: &adw::ApplicationWindow) {
    let context = user_context(window);
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    let policy = session.borrow().policy.clone();
    let live = session_policy(window);
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
                if newest && user_context(&held) == context {
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

/// §F0.7's ⓘ (header badge 6): a button whose whole job is its tooltip — the shown tab's label and help
/// text, kept in sync by `paint_readouts`. It has no window of its own to open: F0.7 ends with "There is no
/// policy form", so the ⓘ is inert on press and the policy is read where it is said — the log.
pub(crate) fn wire_policy(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let _ = (button, window);
}
