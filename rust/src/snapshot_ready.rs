//! When a snapshot takes its picture: at an event, not after a guessed wait.
//!
//! The event is `--shot-on`: `ready` (the default) is the first painted frame where no shown widget
//! still owes a setup step (`ui::settle`); `open:<name>` also wants the widget named `<name>` shown.
//! The picture is taken one painted frame later, and only if no widget moved in between: a setup step
//! that never registered would otherwise be caught half done, the way a 300 ms timer caught the Cut
//! page's form column still scrolled in one run and at its top in the next. A screen that does not
//! settle fails loudly, naming what it still owes or what moved, instead of writing a wrong picture.

use gtk4 as gtk;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The frame count at which a screen that never becomes ready is given up on: a bound on a hang, not
/// a wait for readiness, which comes as an event.
const FRAME_LIMIT: u32 = 600;

/// What `--shot-on` names.
#[derive(Clone, Debug, PartialEq)]
pub enum ShotOn {
    Ready,
    Open(String),
}

impl ShotOn {
    pub fn parse(arg: &str) -> Result<ShotOn, String> {
        match arg {
            "ready" => Ok(ShotOn::Ready),
            _ => match arg.strip_prefix("open:") {
                Some(name) if !name.is_empty() => Ok(ShotOn::Open(name.to_string())),
                _ => Err(format!("--shot-on takes `ready` or `open:<widget name>`, not {arg:?}")),
            },
        }
    }

    fn label(&self) -> String {
        match self {
            ShotOn::Ready => "ready".to_string(),
            ShotOn::Open(name) => format!("open:{name}"),
        }
    }
}

/// Every shown widget's name and box in `root`'s coordinates, in tree order.
fn layout(root: &gtk::Widget) -> Vec<(String, [i32; 4])> {
    fn walk(root: &gtk::Widget, widget: &gtk::Widget, out: &mut Vec<(String, [i32; 4])>) {
        if !widget.is_mapped() {
            return;
        }
        if let Some(r) = widget.compute_bounds(root) {
            out.push((widget.widget_name().to_string(), [r.x(), r.y(), r.width(), r.height()].map(|v| v.round() as i32)));
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            walk(root, &c, out);
            child = c.next_sibling();
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

fn shown_by_name(root: &gtk::Widget, name: &str) -> bool {
    if root.widget_name() == name {
        return root.is_mapped();
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        if shown_by_name(&c, name) {
            return true;
        }
        child = c.next_sibling();
    }
    false
}

/// Calls `shoot` when `painted`, and the other top levels in `also` (a menu staged in a window of its
/// own), are settled as `on` says, then `finish` with what triggered the picture, or with why there is none.
pub fn shoot_when_ready(
    painted: &gtk::Widget,
    also: Vec<gtk::Widget>,
    on: ShotOn,
    shoot: impl FnOnce() + 'static,
    finish: impl FnOnce(Result<String, String>) + 'static,
) {
    let Some(clock) = painted.frame_clock() else {
        finish(Err("the widget to paint has no frame clock: it was never shown".to_string()));
        return;
    };
    struct State {
        frame: u32,
        seen: Option<Vec<(String, [i32; 4])>>,
        shooting: bool,
        shoot: Option<Box<dyn FnOnce()>>,
        finish: Option<Box<dyn FnOnce(Result<String, String>)>>,
    }
    enum Next {
        Wait,
        Shoot(Box<dyn FnOnce()>),
        Finish(Result<String, String>),
    }
    let state = Rc::new(RefCell::new(State {
        frame: 0,
        seen: None,
        shooting: false,
        shoot: Some(Box::new(shoot)),
        finish: Some(Box::new(finish)),
    }));
    let handler: Rc<RefCell<Option<glib::SignalHandlerId>>> = Rc::new(RefCell::new(None));
    let (clock_h, handler_h) = (clock.clone(), handler.clone());
    let roots: Vec<gtk::Widget> = std::iter::once(painted.clone()).chain(also).collect();
    let layout_all = {
        let roots = roots.clone();
        move || roots.iter().flat_map(layout).collect::<Vec<_>>()
    };
    let id = clock.connect_after_paint(move |_| {
        // Decided under the borrow, acted on after it: taking the picture drives the main loop, which
        // can paint a frame and land here again.
        let next = {
            let mut st = state.borrow_mut();
            if st.finish.is_none() || st.shooting {
                return;
            }
            st.frame += 1;
            let frame = st.frame;
            if let Some(seen) = st.seen.take() {
                // The confirming frame: the picture is taken only if nothing moved since the ready one.
                let now = layout_all();
                match seen.iter().zip(now.iter()).find(|(a, b)| a != b) {
                    Some((before, after)) => Next::Finish(Err(format!(
                        "still changing after {} at frame {frame}: `{}` moved from {:?} to {:?}",
                        on.label(),
                        before.0,
                        before.1,
                        after.1
                    ))),
                    None if seen.len() != now.len() => Next::Finish(Err(format!(
                        "still changing after {} at frame {frame}: {} widgets shown, then {}",
                        on.label(),
                        seen.len(),
                        now.len()
                    ))),
                    None => match st.shoot.take() {
                        Some(shoot) => {
                            st.shooting = true;
                            Next::Shoot(shoot)
                        }
                        None => Next::Wait,
                    },
                }
            } else {
                let owed = crate::ui::settle::owed_shown();
                let open_missing = match &on {
                    ShotOn::Open(name) if !roots.iter().any(|root| shown_by_name(root, name)) => Some(name.clone()),
                    _ => None,
                };
                if owed.is_empty() && open_missing.is_none() {
                    st.seen = Some(layout_all());
                    Next::Wait
                } else if frame >= FRAME_LIMIT {
                    let mut why = Vec::new();
                    if !owed.is_empty() {
                        why.push(format!("still owed: {}", owed.join(", ")));
                    }
                    if let Some(name) = open_missing {
                        why.push(format!("`{name}` never shown"));
                    }
                    Next::Finish(Err(format!("not {} after {frame} frames: {}", on.label(), why.join("; "))))
                } else {
                    Next::Wait
                }
            }
        };
        let result = match next {
            // A frame is only painted when something asks for one: ask, so the next look comes.
            Next::Wait => {
                for root in &roots {
                    root.queue_draw();
                }
                return;
            }
            Next::Shoot(shoot) => {
                shoot();
                let frame = state.borrow().frame;
                Ok(format!(
                    "taken on {} at frame {frame}; setup steps done: {}",
                    on.label(),
                    crate::ui::settle::done().join(", ")
                ))
            }
            Next::Finish(result) => result,
        };
        let finish = state.borrow_mut().finish.take();
        if let Some(id) = handler_h.borrow_mut().take() {
            clock_h.disconnect(id);
        }
        if let Some(finish) = finish {
            finish(result);
        }
    });
    *handler.borrow_mut() = Some(id);
    painted.queue_draw();
}
