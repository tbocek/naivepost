//! Smoke tests: the project file round-trips, and GTK actually links and draws.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use glib::{self};
use naivepost::project::{self, Project, Source};

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-smoke-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

#[test]
fn project_round_trip() {
    let dir = temp_dir("roundtrip");
    let want = Project {
        sources: vec![Source {
            path: "project:sources/cam.mkv".to_string(),
            footage: true,
            narrator: 2,
            sepvoice: true,
            tracks: vec![0, 1],
        }],
        interval: 0.25,
        language: "de".to_string(),
        no_narration: true,
        reference_sources: true,
        context: "a lecture with two cameras".to_string(),
    };

    project::save(&want, &dir).expect("save");
    let got = project::load(&dir).expect("load");
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(want, got);
}

#[test]
fn empty_project_file_holds_the_spec_defaults() {
    let dir = temp_dir("defaults");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(project::PROJECT_FILE), "{}").unwrap();

    let loaded = project::load(&dir).expect("load");
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(loaded.interval, 1.0);
    assert_eq!(loaded.language, "en");
    assert!(!loaded.no_narration);
    assert!(!loaded.reference_sources);
    assert_eq!(loaded.context, "");
    assert!(loaded.sources.is_empty());
}

#[test]
fn fixture_project_loads() {
    let loaded = project::load(&fixture_dir()).expect("fixture loads");
    assert_eq!(loaded.sources.len(), 1);
    assert!(!loaded.context.is_empty());
}

/// GTK links and the window builds on a real display (the justfile runs tests
/// under `xvfb-run`). The four pages are there and show the project's state.
#[test]
fn window_shows_the_four_pages() {
    let app = adw::Application::builder()
        .application_id(naivepost::ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let model = project::load(&fixture_dir()).expect("fixture loads");

    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    {
        let app = app.clone();
        let done = done.clone();
        app.connect_activate(move |app| {
            let window = naivepost::ui::build_window(app, &model, "Prepare");
            window.present();

            let names = page_names(&window);
            let text = context_label(&window);

            assert_eq!(names, naivepost::PAGES, "page labels on screen");
            assert!(
                text.contains("blockchain lecture"),
                "user context missing from the window: {text}"
            );
            done.set(true);
            app.quit();
        });
    }

    app.run_with_args::<String>(&[]);
    assert!(done.get(), "the activate handler never ran");
}

/// Depth-first walk of the window's widget tree.
///
/// Children are reached through `observe_children`'s element type: iterating it as
/// `gtk::Widget` asserts on the first non-widget child (a GtkConstraint, …).
fn widgets(root: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut out = vec![root.clone()];
    for child in root.observe_children().iter::<glib::Object>() {
        if let Ok(child) = child {
            if let Ok(widget) = child.downcast::<gtk4::Widget>() {
                out.extend(widgets(&widget));
            }
        }
    }
    out
}

fn labels(window: &adw::ApplicationWindow) -> Vec<String> {
    let all = widgets(&window.clone().upcast());
    all.iter()
        .filter_map(|w| w.downcast_ref::<gtk4::Label>().map(|l| l.text().to_string()))
        .collect()
}

fn page_names(window: &adw::ApplicationWindow) -> Vec<&'static str> {
    let shown = labels(window);
    naivepost::PAGES
        .into_iter()
        .filter(|page| shown.iter().any(|t| t == page))
        .collect()
}

fn context_label(window: &adw::ApplicationWindow) -> String {
    labels(window)
        .into_iter()
        .find(|t| t.contains("recorded in one sitting"))
        .unwrap_or_default()
}
