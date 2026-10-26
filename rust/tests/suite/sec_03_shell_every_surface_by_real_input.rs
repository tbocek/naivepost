//! §03-shell through the pointer: the tabs, the log expander, Settings and Profiles pressed with REAL clicks
//! (`common::real_input`: xdotool on the test's X display, GTK's own event route), each effect asserted on what
//! the window then shows. A test that calls `emit_clicked` reaches the handler even when the button is covered,
//! insensitive to the pointer or never laid out; these only pass when a person's click would.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use crate::common::real_input::{real_click, real_click_centre};

static RAN: AtomicBool = AtomicBool::new(false);

/// One stamped recording, so the Cut tab is not locked (shell::lock asks for a footage row).
const A: &str = "2026-10-18 19-21-52";

fn project(root: &Path) -> PathBuf {
    let dir = root.join("shell.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc=size=160x90:rate=25:duration=6")
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=6",
            "-shortest",
        ])
        .arg(dir.join("sources").join(format!("{A}.mkv")))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip was written");
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [{{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}}]}}"#
        ),
    )
    .unwrap();
    dir
}

#[test]
fn sec_03_shell_every_surface_answers_real_clicks() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    let root = common::temp_dir("shell-real-input");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Prepare", &dir);
        window.set_default_size(1240, 900);
        window.present();
        common::settle();
        common::settle_workers();

        // §2 the tabs: a click on each shows its page.
        for page in [Page::Cut, Page::Narrate, Page::Produce, Page::Prepare] {
            let tab = ui::tab_button(&window, page).expect("the shell has the tab");
            real_click_centre(&tab);
            common::settle_workers();
            assert_eq!(
                ui::state(&window).page,
                page,
                "a real click on the {page:?} tab (status \"{}\")",
                ui::state(&window).status
            );
        }

        // §1 the log: a click on its header opens it and a second closes it.
        let expanded = ui::state(&window).log_expanded;
        let header = common::widget_in(&window, "log-header").expect("the log's header");
        let on_title =
            |header: &gtk4::Widget| real_click(header, 12.0, f64::from(header.height()) / 2.0, 1);
        on_title(&header);
        assert_ne!(
            ui::state(&window).log_expanded,
            expanded,
            "a click on the log's header toggles it"
        );
        on_title(&header);
        assert_eq!(ui::state(&window).log_expanded, expanded, "and back");

        // §5 Settings and Profiles open their windows on a real click.
        let settings = common::widget_in(&window, "settings-button").expect("Settings");
        real_click_centre(&settings);
        common::settle();
        assert!(
            ui::settings::dialog_open(&window),
            "a real click on Settings opened the dialog"
        );
        for toplevel in gtk4::Window::list_toplevels() {
            if let Ok(other) = toplevel.downcast::<gtk4::Window>() {
                if other != *window.upcast_ref::<gtk4::Window>() {
                    other.close();
                }
            }
        }
        common::settle();
        let profiles = common::widget_in(&window, "profiles-button").expect("Profiles");
        real_click_centre(&profiles);
        common::settle();
        let opened = ui::profiles_dialog::open_window();
        assert!(
            opened.is_some(),
            "a real click on Profiles opened its window"
        );
        if let Some(opened) = opened {
            opened.close();
        }
        common::settle();

        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
