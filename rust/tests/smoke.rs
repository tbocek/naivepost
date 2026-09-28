//! Smoke tests: the project file round-trips, and GTK actually links and draws.

use std::path::PathBuf;

use adw::prelude::*;
use glib;
use naivepost::project::{self, Project, Source};
use naivepost::shell::{self, Page};
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir};

/// A directory under the temp dir that no other test run shares.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-smoke-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
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
        ..Default::default()
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

/// One running GTK application per test binary, taken in turn by the window tests.
///
/// `g_application_run` claims the process's default main context and refuses a second claimant ("it
/// is already acquired"), and an application that has run cannot be run again — so a test binary gets
/// exactly one. All three window checks therefore happen inside its single `activate`, in order, and
/// each records that it ran; the `#[test]`s below assert on those flags, so a check that never
/// happened is a failure rather than a quiet pass. The application is run by the first test to ask,
/// and `window_round`'s `Once` makes every other test wait for it, which is what keeps GTK's own
/// single-main-loop rule out of the tests' way.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(naivepost::ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            // Each check records itself only if it got that far, so the first one to fail does not
            // leave the other two reported as never having run.
            check_four_pages(app);
            RAN_FOUR_PAGES.store(true, std::sync::atomic::Ordering::SeqCst);
            check_locked_tab(app);
            RAN_LOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
            check_unlocked_tab(app);
            RAN_UNLOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_FOUR_PAGES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_LOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_UNLOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn ran(flag: &std::sync::atomic::AtomicBool, what: &str) {
    window_round();
    assert!(flag.load(std::sync::atomic::Ordering::SeqCst), "{what} never ran");
}

#[test]
fn window_shows_the_four_pages() {
    ran(&RAN_FOUR_PAGES, "the four-page check");
}

/// F0.1 through the widgets: clicking a tab goes through the shell, so a locked one bounces and the
/// status line says why.
#[test]
fn clicking_a_locked_tab_bounces_and_says_why() {
    ran(&RAN_LOCKED, "the locked-tab check");
}

/// And an unlocked tab does move, with the status line left saying nothing.
#[test]
fn clicking_an_unlocked_tab_shows_its_page() {
    ran(&RAN_UNLOCKED, "the unlocked-tab check");
}

/// GTK links and the window builds on a real display (the justfile runs tests under `xvfb-run`).
/// The four pages are there and show the project's state.
fn check_four_pages(app: &adw::Application) {
    let model = project::load(&fixture_dir()).expect("fixture loads");
    let window = naivepost::ui::build_window(app, &model, "Prepare");
    window.present();

    let names = page_names(&window);
    let text = context_label(&window);
    assert_eq!(names, naivepost::PAGES, "page labels on screen");
    assert!(
        text.contains("blockchain lecture"),
        "user context missing from the window: {text}"
    );
}

/// F0.1 through the widgets: clicking a tab goes through the shell, so a locked one bounces and the
/// status line says why. The click is real — `emit_clicked` is what a press on the tab does — and
/// nothing about the decision is repeated here (spec/00-principles.md §5).
fn check_locked_tab(app: &adw::Application) {
    // No source is marked footage, which is the state that locks Cut (§1).
    let model = Project {
        sources: vec![Source { path: "voice.wav".to_string(), ..Default::default() }],
        ..Default::default()
    };
    let window = naivepost::ui::build_window(app, &model, "Prepare");
    window.present();

    // Greyed and holding the reason, but still clickable — that is how a click gets to bounce at all.
    assert!(naivepost::ui::tab_dimmed(&window, Page::Cut), "the Cut tab is greyed");
    assert!(!naivepost::ui::tab_dimmed(&window, Page::Prepare));
    assert_eq!(
        naivepost::ui::tab_tooltip(&window, Page::Cut).as_deref(),
        Some(shell::CUT_LOCK),
        "tooltip = the reason"
    );

    let cut = naivepost::ui::tab_button(&window, Page::Cut).expect("a Cut tab");
    assert!(cut.is_sensitive(), "greyed, not disabled");
    cut.emit_clicked();

    let state = naivepost::ui::state(&window);
    assert_eq!(state.page, Page::Prepare, "the page did not move");
    assert_eq!(state.status, shell::CUT_LOCK, "status = the lock reason");
}

/// And an unlocked tab does move, with the status line left saying nothing.
fn check_unlocked_tab(app: &adw::Application) {
    let model = project::load(&fixture_dir()).expect("fixture loads");
    let window = naivepost::ui::build_window(app, &model, "Prepare");
    window.present();

    assert!(!naivepost::ui::tab_dimmed(&window, Page::Cut), "the fixture has footage");
    assert_eq!(
        naivepost::ui::tab_tooltip(&window, Page::Cut).as_deref(),
        Some(Page::Cut.tip()),
        "an unlocked tab describes its page instead"
    );

    naivepost::ui::tab_button(&window, Page::Cut).expect("a Cut tab").emit_clicked();
    let state = naivepost::ui::state(&window);
    assert_eq!(state.page, Page::Cut);
    assert_eq!(state.status, "", "a switch that worked has nothing to report");
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
