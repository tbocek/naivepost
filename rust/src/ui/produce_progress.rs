//! A Produce run on the bar: what its walk reports, painted on the window thread for as long as the run is open.

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::glib;

/// How often the bar looks at the walk.
const LOOK: std::time::Duration = std::time::Duration::from_millis(200);

/// What the status line says once ⏸ has actually landed.
pub const PARKED_STATUS: &str = "paused \u{2014} \u{25b6} carries on";

/// Follow the run until `running` says it is over.
pub fn follow(window: &adw::ApplicationWindow, running: impl Fn() -> bool + 'static) {
    let _ = crate::run_stages::take_progress();
    let window = window.clone();
    let mut last = String::new();
    glib::timeout_add_local(LOOK, move || {
        if !running() {
            return glib::ControlFlow::Break;
        }
        let bar = crate::ui::window::find_widget_by_name(window.upcast_ref(), "run-progress")
            .and_then(|widget| widget.downcast::<gtk::ProgressBar>().ok());
        if let Some((status, fraction)) = crate::run_stages::take_progress() {
            last = status;
            if let (Some(bar), Some(fraction)) = (&bar, fraction) {
                // A count takes the needle over from the pulse.
                crate::ui::shell_beats::stop_pulse();
                bar.set_fraction(fraction);
            }
        }
        let line = if crate::run_stages::parked() {
            PARKED_STATUS
        } else {
            last.as_str()
        };
        if !line.is_empty() {
            if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
                status.set_text(line);
            }
        }
        glib::ControlFlow::Continue
    });
}
