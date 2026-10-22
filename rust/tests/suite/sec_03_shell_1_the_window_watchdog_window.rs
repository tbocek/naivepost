//! §03-shell.md §1's last bullet, the GUI hang watchdog, on a live window: a window whose GTK thread
//! keeps turning beats, and gets no dump. In a file, and so a process, of its own: the watcher starts once
//! per process, and the check in `sec_03_shell_1_the_window_watchdog.rs` that hangs on purpose would
//! otherwise share it, so this window's closing silence became that check's second dump.
//!
//! GTK comes up once, inside ONE `adw::Application::run_with_args`, as in `sec_03_shell_1_the_window.rs`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use adw::prelude::*;
use naivepost::ui;
use naivepost::watchdog;

use crate::common;
use common::wait_until;

/// One GTK application for this binary's widget round.
static GTK: Once = Once::new();
static RAN: AtomicBool = AtomicBool::new(false);

const SHORT_LIMIT: std::time::Duration = common::WATCHDOG_TEST_LIMIT;

/// Every scenario that needs GTK, inside one application's one `activate`.
#[test]
fn sec_03_shell_1_the_window_s10_a_live_window_keeps_the_beat_and_stays_quiet() {
    common::pin_private_cwd();
    let paths = common::watchdog_paths("live");
    GTK.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(move |app| {
            // The launch's own door, with §1's limit shortened so the check finishes. A window that is
            // really turning its main context beats, so nothing may be dumped while it lives.
            watchdog::start_watch(Some(paths.clone()), SHORT_LIMIT);
            let model = naivepost::project::Project {
                sources: vec![common::footage("project:sources/a.mkv")],
                ..Default::default()
            };
            let window = ui::build_window(app, &model, "Prepare");
            window.present();
            common::hold_last_window(window.clone());
            // `build_window` armed the heartbeat; wait for the timer to have actually fired rather than
            // guessing a number of turns.
            wait_until("the window's heartbeat timer fired", || watchdog::beats() > 0);
            let beats_at_window = watchdog::beats();
            // Now pump the main context for longer than the silence limit: if the beat were not real,
            // this is where the dump would land. The condition waited for is the shared clock passing the
            // limit — no guessed number of turns, and `wait_until` pumps the context while it waits.
            wait_until(
                "the main context had turned past the silence limit",
                || watchdog::clock() > SHORT_LIMIT * 3,
            );
            assert!(
                watchdog::beats() > beats_at_window,
                "a live window keeps beating: {} -> {}",
                beats_at_window,
                watchdog::beats()
            );
            assert!(
                common::hang_dumps(&paths).is_empty(),
                "§1: a window whose GUI thread is answering gets no dump: {:?}",
                common::hang_dumps(&paths)
            );
            // And the beat it rings is the GTK timer's own, not a test's: the marker moved on its own
            // between the two reads above.
            assert!(watchdog::last_beat().is_some(), "the thread has said it was there");
            window.close();
            common::release_last_window();
            RAN.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(RAN.load(Ordering::SeqCst), "the live watchdog round never ran");
}
