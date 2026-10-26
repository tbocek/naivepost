//! Real input for widget tests: the pointer and the keyboard driven through the X server with `xdotool`, so an
//! event takes GTK's own route (the window under the pointer, the widget picked there, every gesture and
//! controller on the way, claims and denials) instead of a test emitting one gesture's signal and skipping all
//! of it. Several surfaces passed their signal-emitting tests while a real click did nothing, because another
//! gesture on the same widget ate the press; only this route sees that.
//!
//! The test displays are bare Xvfb servers (no window manager): a window sits where GTK put it, `xdotool`
//! reaches it by a unique title this module gives it, and keyboard focus is set with `windowfocus`. Every helper
//! waits for the window to be mapped and laid out first, sends its events, then turns the main loop until GTK
//! has drained them, so the caller asserts on a page that has handled the input.

use gtk4 as gtk;
use gtk4::prelude::*;
use std::process::Command;
use std::time::{Duration, Instant};

/// Run `xdotool` with these arguments on this process's display, and hand back its stdout.
fn xdotool(args: &[&str]) -> String {
    let out = Command::new("xdotool")
        .args(args)
        .output()
        .unwrap_or_else(|why| panic!("xdotool {args:?} did not run: {why}"));
    assert!(
        out.status.success(),
        "xdotool {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Put the pointer on screen point `(x, y)` and wait until the server has it there. `mousemove --sync` waits
/// for the pointer to MOVE, so it would wait forever for a pointer already standing on the point (or for a
/// point off the screen, where the pointer stops at the edge): neither is sent to it.
fn pointer_to(x: i32, y: i32) {
    let shell = xdotool(&["getmouselocation", "--shell"]);
    let read = |key: &str| -> i32 {
        shell
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(-1)
    };
    if (read("X"), read("Y")) == (x, y) {
        return;
    }
    let screen = xdotool(&["getdisplaygeometry"]);
    let mut size = screen
        .split_whitespace()
        .filter_map(|n| n.parse::<i32>().ok());
    let (w, h) = (
        size.next().unwrap_or(i32::MAX),
        size.next().unwrap_or(i32::MAX),
    );
    assert!(
        (0..w).contains(&x) && (0..h).contains(&y),
        "the point ({x}, {y}) is off the {w}x{h} screen"
    );
    xdotool(&["mousemove", "--sync", &x.to_string(), &y.to_string()]);
}

/// Turn the main loop until nothing has been pending for `quiet`, or `limit` passes.
pub fn drain(quiet: Duration, limit: Duration) {
    let context = glib::MainContext::default();
    let until = Instant::now() + limit;
    let mut idle_since = Instant::now();
    while Instant::now() < until {
        if context.iteration(false) {
            idle_since = Instant::now();
        } else if idle_since.elapsed() >= quiet {
            return;
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

/// The X window of `window`, after it is mapped and laid out. The window gets a unique title (the test owns
/// it), which is how `xdotool` finds it among the other windows a test binary has built.
fn xid_of(window: &impl IsA<gtk::Window>) -> String {
    let window = window.as_ref();
    let title = format!("naivepost-real-input-{}", window.as_ptr() as usize);
    if window.title().as_deref() != Some(title.as_str()) {
        window.set_title(Some(&title));
    }
    window.present();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        drain(Duration::from_millis(30), Duration::from_millis(500));
        if window.is_mapped() && window.width() > 0 {
            let found = Command::new("xdotool")
                .args(["search", "--onlyvisible", "--name", &format!("^{title}$")])
                .output()
                .ok()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .unwrap_or_default();
            if let Some(id) = found.lines().last() {
                return id.to_string();
            }
        }
        assert!(Instant::now() < until, "the window {title} never mapped");
    }
}

/// Where the X window `xid` sits on the screen.
fn origin_of(xid: &str) -> (f64, f64) {
    let shell = xdotool(&["getwindowgeometry", "--shell", xid]);
    let read = |key: &str| -> f64 {
        shell
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0.0)
    };
    (read("X"), read("Y"))
}

/// The screen point of `(x, y)` in `widget`'s own coordinates, and its window's X id.
pub fn screen_point(widget: &impl IsA<gtk::Widget>, x: f64, y: f64) -> (String, i32, i32) {
    let widget = widget.as_ref();
    let root = widget
        .root()
        .expect("the widget is in a window")
        .downcast::<gtk::Window>()
        .expect("its root is a window");
    let xid = xid_of(&root);
    let until = Instant::now() + Duration::from_secs(5);
    while !(widget.is_mapped() && widget.width() > 0) {
        assert!(
            Instant::now() < until,
            "{:?} never got an allocation",
            widget.widget_name()
        );
        drain(Duration::from_millis(20), Duration::from_millis(200));
    }
    let point = widget
        .compute_point(&root, &gtk::graphene::Point::new(x as f32, y as f32))
        .expect("the widget has a point in its window");
    // The window widget sits inside its surface by the client-side decoration's margin.
    let (sx, sy) = root.surface_transform();
    let (ox, oy) = origin_of(&xid);
    (
        xid,
        (ox + sx + f64::from(point.x())).round() as i32,
        (oy + sy + f64::from(point.y())).round() as i32,
    )
}

/// After the X server has the events, give GTK time to read and handle them. The Cut page keeps 50 ms polls
/// running (the thumbnail worker, the background workers) and a playing preview ticks every 20 ms, so the loop
/// is never idle for long: 15 ms without a dispatch is the gap between two of those, by when an input event
/// the server already holds has been read. The cap keeps a page that animates without pause from stalling.
fn settle_input() {
    drain(Duration::from_millis(15), Duration::from_millis(800));
}

/// A real click of `button` (1 left, 2 middle, 3 right) at `(x, y)` in `widget`'s coordinates.
pub fn real_click(widget: &impl IsA<gtk::Widget>, x: f64, y: f64, button: u32) {
    let (_, sx, sy) = screen_point(widget, x, y);
    pointer_to(sx, sy);
    settle_input();
    xdotool(&["click", &button.to_string()]);
    settle_input();
}

/// A real double click of `button` at `(x, y)` in `widget`'s coordinates: two presses inside the
/// display's double-click time, the way a person double-clicks.
pub fn real_double_click(widget: &impl IsA<gtk::Widget>, x: f64, y: f64, button: u32) {
    let (_, sx, sy) = screen_point(widget, x, y);
    pointer_to(sx, sy);
    settle_input();
    xdotool(&[
        "click",
        "--repeat",
        "2",
        "--delay",
        "60",
        &button.to_string(),
    ]);
    settle_input();
}

/// A real click in the middle of `widget`: how a person presses a button.
pub fn real_click_centre(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    let (w, h) = (f64::from(widget.width()), f64::from(widget.height()));
    real_click(widget, w / 2.0, h / 2.0, 1);
}

/// A real drag with `button` from `from` to `to` (both in `widget`'s coordinates), moved in `steps` motions.
pub fn real_drag(widget: &impl IsA<gtk::Widget>, from: (f64, f64), to: (f64, f64), button: u32) {
    let (_, ax, ay) = screen_point(widget, from.0, from.1);
    let (_, bx, by) = screen_point(widget, to.0, to.1);
    pointer_to(ax, ay);
    settle_input();
    xdotool(&["mousedown", &button.to_string()]);
    settle_input();
    let steps = 8;
    for step in 1..=steps {
        let x = ax + (bx - ax) * step / steps;
        let y = ay + (by - ay) * step / steps;
        pointer_to(x, y);
        drain(Duration::from_millis(15), Duration::from_millis(300));
    }
    xdotool(&["mouseup", &button.to_string()]);
    settle_input();
}

/// A press held for `hold` without moving, then released: a long press.
pub fn real_hold(widget: &impl IsA<gtk::Widget>, x: f64, y: f64, button: u32, hold: Duration) {
    let (_, sx, sy) = screen_point(widget, x, y);
    pointer_to(sx, sy);
    settle_input();
    xdotool(&["mousedown", &button.to_string()]);
    let until = Instant::now() + hold;
    while Instant::now() < until {
        drain(Duration::from_millis(10), Duration::from_millis(50));
    }
    xdotool(&["mouseup", &button.to_string()]);
    settle_input();
}

/// Real wheel notches over `(x, y)`: `notches` > 0 scrolls down (away), < 0 up; `mods` held meanwhile
/// (xdotool names: "ctrl", "shift", "alt").
pub fn real_scroll(widget: &impl IsA<gtk::Widget>, x: f64, y: f64, notches: i32, mods: &[&str]) {
    let (_, sx, sy) = screen_point(widget, x, y);
    pointer_to(sx, sy);
    settle_input();
    for key in mods {
        xdotool(&["keydown", key]);
    }
    let button = if notches > 0 { "5" } else { "4" };
    for _ in 0..notches.unsigned_abs() {
        xdotool(&["click", button]);
        drain(Duration::from_millis(15), Duration::from_millis(300));
    }
    for key in mods.iter().rev() {
        xdotool(&["keyup", key]);
    }
    settle_input();
}

/// A real key press on `window` (an xdotool key name, modifiers joined with '+', e.g. "ctrl+z", "Delete",
/// "comma"). The window gets the keyboard focus first, as a person's click would give it.
pub fn real_key(window: &impl IsA<gtk::Window>, keys: &str) {
    let xid = xid_of(window);
    xdotool(&["windowfocus", "--sync", &xid]);
    settle_input();
    xdotool(&["key", "--clearmodifiers", keys]);
    settle_input();
}
