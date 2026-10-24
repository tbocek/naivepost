//! What a window still owes before it shows its settled state: setup steps that can only run once
//! their widget has a size (the form column scrolled to its top, a divider at its opening).
//!
//! A step registers with [`owe`] and is done when the returned [`Owed`] is dropped. GTK has no
//! "everything is laid out" event, so this is the app's own: the snapshot takes its picture at the
//! first painted frame where no step of a shown widget is still owed. A step whose widget is not
//! mapped (a page that is not the visible tab) does not hold anything back: it runs when that page
//! is shown.

use gtk4 as gtk;
use gtk4::prelude::*;
use std::cell::RefCell;

struct Step {
    id: u64,
    what: &'static str,
    widget: glib::WeakRef<gtk::Widget>,
}

thread_local! {
    static OWED: RefCell<Vec<Step>> = const { RefCell::new(Vec::new()) };
    static DONE: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A setup step still owed; dropping it marks the step done.
pub struct Owed(u64);

impl Drop for Owed {
    fn drop(&mut self) {
        let what = OWED.with(|owed| {
            let mut owed = owed.borrow_mut();
            let at = owed.iter().position(|step| step.id == self.0)?;
            Some(owed.remove(at).what)
        });
        if let Some(what) = what {
            DONE.with(|done| done.borrow_mut().push(what));
        }
    }
}

/// Registers `what`, a step that `widget` still owes, until the returned value is dropped.
pub fn owe(what: &'static str, widget: &impl IsA<gtk::Widget>) -> Owed {
    let id = NEXT.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    OWED.with(|owed| {
        owed.borrow_mut().push(Step {
            id,
            what,
            widget: widget.upcast_ref::<gtk::Widget>().downgrade(),
        })
    });
    Owed(id)
}

/// The steps still owed by widgets that are shown.
pub fn owed_shown() -> Vec<&'static str> {
    OWED.with(|owed| {
        owed.borrow()
            .iter()
            .filter(|step| {
                step.widget
                    .upgrade()
                    .is_some_and(|widget| widget.is_mapped())
            })
            .map(|step| step.what)
            .collect()
    })
}

/// The steps done so far, in order.
pub fn done() -> Vec<&'static str> {
    DONE.with(|done| done.borrow().clone())
}

/// Runs `step` once, at the first frame where `ready` holds for `widget`, and owes it until then.
pub fn when<W: IsA<gtk::Widget>>(
    what: &'static str,
    widget: &W,
    ready: impl Fn(&W) -> bool + 'static,
    step: impl FnOnce(&W) + 'static,
) {
    let owed = RefCell::new(Some(owe(what, widget)));
    let step = RefCell::new(Some(step));
    widget.add_tick_callback(move |widget, _| {
        if !ready(widget) {
            return glib::ControlFlow::Continue;
        }
        if let Some(step) = step.borrow_mut().take() {
            step(widget);
        }
        owed.borrow_mut().take();
        glib::ControlFlow::Break
    });
}

/// Runs `step` once, at the first frame where `widget` has a height. The event-driven form of "once its
/// size is known": a one-shot idle that found the widget unsized gave up for good, and the form column
/// stayed wherever it had been scrolled, in the app and in its shots.
pub fn once_sized<W: IsA<gtk::Widget>>(
    what: &'static str,
    widget: &W,
    step: impl FnOnce(&W) + 'static,
) {
    when(what, widget, |widget| widget.height() > 1, step);
}
