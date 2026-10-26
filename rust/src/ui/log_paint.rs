//! §7's exchange lines as they reach the shell's log, and the log's own text view.
//!
//! Split out of `window.rs`, which is past its size budget. Both doors are small but neither belongs in
//! `log_link` (which owns only which line is clickable) nor in `exchanges` (which owns the page format and
//! must stay free of widgets): this is the moment a recorded line becomes something on the screen.

use adw::prelude::*;
use gtk4 as gtk;

use super::window;

/// The log's text view, for a reader that has the window but not the expander: an expander keeps its
/// body behind an internal widget, so every caller reaches it this way and none should re-derive it.
///
/// The expander's child is the log's OWN scroller (the text view cannot be the direct child: uncapped,
/// it reported the whole document's height as the window's minimum and the shell could not be shrunk —
/// see `tests/suite/window_shrinks.rs`), so the walk goes expander → scroller → text view.
pub fn log_view(window: &adw::ApplicationWindow) -> Option<gtk::TextView> {
    let child = window::log_expander(window)?.child()?;
    if let Some(view) = child.downcast_ref::<gtk::TextView>() {
        return Some(view.clone());
    }
    child
        .downcast_ref::<gtk::ScrolledWindow>()?
        .child()?
        .downcast()
        .ok()
}

/// A §7 exchange line written into the shell's log: the door §7's recorder writes its two per-call lines
/// and its once-per-run link through. A line that reaches the log is not left invisible -- the expander
/// opens and the body is repainted through [`window::paint_progress`], the only door that does both, so
/// the link sits where §1 says it must be clickable while the run that wrote it is still going.
///
/// With no window on this thread (a headless walk dialling a chat call) the line still goes to the sink
/// and nothing is painted: recording never fails the thing that recorded.
pub fn note_exchange_line(line: &str) {
    window::log_line(line);
    let Some(window) = window::built_window() else {
        return;
    };
    let (Some(progress), Some(expander), Some(view), Some(queue), Some(status)) = (
        window::progress_bar(&window),
        window::log_expander(&window),
        log_view(&window),
        window::run_queue(&window),
        window::find_status(window.upcast_ref()),
    ) else {
        return;
    };
    window::paint_progress(
        &progress,
        &expander,
        &view,
        &queue.borrow(),
        crate::ui::shell_beats::StatusLine {
            status: &status,
            owned: false,
        },
        true,
    );
}

/// How close to the end counts as "at the end": a line's worth, so a person who scrolled to the bottom by
/// hand is following even if the wheel stopped a few pixels short.
const AT_END_SLACK_PX: f64 = 24.0;

/// TEMPORARY diagnosis (2026-10-26): the open log sometimes stops following its newest line on the user's
/// machine and never on the test display. With `NAIVEPOST_TRACE_LOG=1` every move of the log's scroll position
/// and every decision of the follow rule goes to stderr as one `[log-follow]` line, so the moment it lets go
/// is on record. Remove once the cause is found and fixed.
fn trace(what: &str, adj: &gtk::Adjustment) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("NAIVEPOST_TRACE_LOG").is_some()) {
        eprintln!(
            "[log-follow] {:?} {what}: value={:.0} page={:.0} upper={:.0} gap={:.0}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() % 100_000)
                .unwrap_or_default(),
            adj.value(),
            adj.page_size(),
            adj.upper(),
            adj.upper() - adj.page_size() - adj.value()
        );
    }
}

thread_local! {
    /// Each log view built on this thread and whether it is pinned to its end, newest last.
    static PINS: std::cell::RefCell<Vec<(glib::WeakRef<gtk::TextView>, std::rc::Rc<std::cell::Cell<bool>>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// §1: the log follows its newest line while the person is at its end. Installed once on the log's scroller:
/// while the view is pinned, every growth of the text (GTK measures wrapped lines a while after they arrive)
/// puts the view back at the end; a wheel turn upwards unpins it at once, and each repaint pins it again only
/// if the person is at the end then. The first rewrite scrolled to the end once, right after replacing the
/// text and before anything was measured, so the view stayed where it was and the newest lines ran off.
pub(crate) fn follow_end(scroller: &gtk::ScrolledWindow, view: &gtk::TextView) {
    let pinned = std::rc::Rc::new(std::cell::Cell::new(true));
    {
        let pinned = pinned.clone();
        scroller.vadjustment().connect_changed(move |adj| {
            trace(&format!("changed pinned={}", pinned.get()), adj);
            if pinned.get() {
                adj.set_value((adj.upper() - adj.page_size()).max(0.0));
            }
        });
    }
    {
        let pinned = pinned.clone();
        let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
        let adj = scroller.vadjustment();
        wheel.connect_scroll(move |_, _, dy| {
            trace(&format!("wheel dy={dy:.2} unpins={}", dy < 0.0), &adj);
            if dy < 0.0 {
                pinned.set(false);
            }
            glib::Propagation::Proceed
        });
        scroller.add_controller(wheel);
        scroller
            .vadjustment()
            .connect_value_changed(|adj| trace("value moved", adj));
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let adj = scroller.vadjustment();
        click.connect_pressed(move |_, n, x, y| {
            trace(&format!("click n={n} at {x:.0},{y:.0}"), &adj)
        });
        scroller.add_controller(click);
    }
    PINS.with(|slots| slots.borrow_mut().push((view.downgrade(), pinned)));
}

fn pin_of(view: &gtk::TextView) -> Option<std::rc::Rc<std::cell::Cell<bool>>> {
    PINS.with(|slots| {
        slots
            .borrow()
            .iter()
            .find(|(held, _)| held.upgrade().as_ref() == Some(view))
            .map(|(_, pin)| pin.clone())
    })
}

/// §1: opening the log shows its newest line. A collapsed log is not repainted while a run writes to it, so its
/// buffer is whatever it held when it was folded; opening it repaints it from `lines` and pins it to the end,
/// because a person opening the log asks for what just happened (first rewrite: it opened on the old text,
/// or on nothing, and the newest lines had to be scrolled to).
pub(crate) fn open_at_end(view: &gtk::TextView, lines: &[String]) {
    if let Some(pin) = pin_of(view) {
        pin.set(true);
    }
    repaint_lines(view, lines);
}

/// Whether this view follows its newest line now (what a person scrolling the log would find).
pub fn follows_end_for_test(view: &gtk::TextView) -> bool {
    pin_of(view).is_none_or(|pin| pin.get())
}

fn at_end(view: &gtk::TextView) -> bool {
    view.vadjustment()
        .is_none_or(|adj| adj.value() + adj.page_size() >= adj.upper() - AT_END_SLACK_PX)
}

thread_local! {
    /// What each log view shows: how many lines, and the last of them. A repaint whose lines continue exactly
    /// that only appends.
    static PAINTED: std::cell::RefCell<Vec<(glib::WeakRef<gtk::TextView>, usize, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Where `lines` continues what `view` already shows: the index of the first new line, or `None` when the
/// view shows something else (a cleared log, a new run) and has to be painted whole.
fn painted_before(view: &gtk::TextView, lines: &[String]) -> Option<usize> {
    PAINTED.with(|slots| {
        slots.borrow().iter().find_map(|(held, count, last)| {
            (held.upgrade().as_ref() == Some(view)
                && *count > 0
                && lines.len() >= *count
                && lines[*count - 1] == *last)
                .then_some(*count)
        })
    })
}

fn remember_painted(view: &gtk::TextView, lines: &[String]) {
    PAINTED.with(|slots| {
        let mut slots = slots.borrow_mut();
        slots.retain(|(held, _, _)| held.upgrade().is_some_and(|held| held != *view));
        slots.push((
            view.downgrade(),
            lines.len(),
            lines.last().cloned().unwrap_or_default(),
        ));
    });
}

/// Replace the log's text with `lines`, keeping the person's place: pinned to the end when they were at it,
/// where they were otherwise.
pub(crate) fn repaint_lines(view: &gtk::TextView, lines: &[String]) {
    let pin = pin_of(view);
    let following = pin.as_ref().is_some_and(|pin| pin.get()) || at_end(view);
    let kept = view.vadjustment().map(|adj| adj.value());
    if let Some(adj) = view.vadjustment() {
        trace(
            &format!(
                "repaint {} lines: pinned={} at_end={} following={following}",
                lines.len(),
                pin.as_ref().is_some_and(|pin| pin.get()),
                at_end(view)
            ),
            &adj,
        );
    }
    if let Some(pin) = &pin {
        pin.set(following);
    }
    let buffer = view.buffer();
    // Only the lines that are new are added when the log is the one already on screen plus more. Emptying and
    // refilling the whole buffer at every repaint made the text view move itself back to near the top a moment
    // after the follow had put it at the end, with nothing left to bring it back once the run's last repaint was
    // done (traced on the user's machine 2026-10-26: 57 jumps, the log never unpinned).
    match painted_before(view, lines) {
        Some(from) => super::log_link::paint_lines(&buffer, &lines[from..]),
        None => {
            buffer.set_text("");
            super::log_link::paint_lines(&buffer, lines);
        }
    }
    remember_painted(view, lines);
    if following {
        if let Some(adj) = view.vadjustment() {
            adj.set_value((adj.upper() - adj.page_size()).max(0.0));
        }
    } else if let (Some(adj), Some(at)) = (view.vadjustment(), kept) {
        // Put back once the new text is measured; an emptied-then-refilled view starts at the top.
        glib::idle_add_local_once(move || {
            adj.set_value(at.min((adj.upper() - adj.page_size()).max(0.0)));
        });
    }
}
