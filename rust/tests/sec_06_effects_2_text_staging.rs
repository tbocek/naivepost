// §06-effects#2-the-lane-and-the-preview S3 — the `06-text` staging branch, in its own binary.
//
// The caption form and its arm live in process-wide slots (`LAST_TEXT_WINDOW`, `TEXT_ARMED`), so this scenario
// cannot share a test binary with any other window-building check -- the harness rule every live fixture here
// follows, and the reason `06-text` sits alone here rather than beside the menu check in
// `sec_06_effects_2_staging.rs`.
//
// It calls the snapshot's own staging entry point and asserts what the shot depends on: the form open at the
// line's second, its heading spelled by `fx_text`, and its words field holding the caption the shot types in.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Cut;
use naivepost::fx_text;
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_TEXT_STAGE: AtomicBool = AtomicBool::new(false);

/// The second the `06-text` shot places its line at, mirrored from the staging branch's own value so a change
/// there without a change here fails loudly. (The branch spells 81.0 so the heading reads `Text at 1:21`, as
/// `spec/img/06-text.png` shows it.)
const LINE_SECOND: f64 = 81.0;

fn cut_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, &Cut::default());
    settle();
    window
}

/// First widget in this window's tree carrying this name (`ui::find_widget_by_name` is crate-private).
fn find_widget(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut cursor = node.first_child();
    while let Some(current) = cursor {
        if let Some(found) = find_widget(&current, name) {
            return Some(found);
        }
        cursor = current.next_sibling();
    }
    None
}

fn staged_text_shot_has_its_form_open(app: &adw::Application) {
    let window = cut_page(app);
    // The staging entry point itself, not a re-run of its steps: if the branch is ever dropped, `paint` returns
    // false and this fires rather than the shot quietly becoming the bare page again.
    let owned = naivepost::snapshot_effects::paint(&window, "06-text");
    assert!(owned, "`snapshot_effects::paint` no longer owns the `06-text` screen");
    let form = ui::text_form_open().expect("the staged `06-text` shot drew no caption form");
    assert_eq!(form.t, LINE_SECOND, "the staged caption does not start at the line it placed");
    assert_eq!(
        form.on,
        fx_text::LOWER_THIRD,
        "a click puts the caption across the lower third, as `spec/img/06-text.png` shows"
    );
    // The heading the shot shows, spelled by the module that owns it -- and the app's own `tools::mm_ss`, not a
    // hand-rolled copy of it, so the two can never drift.
    assert_eq!(fx_text::form_title(form.t), format!("Text at {}", naivepost::tools::mm_ss(form.t)));
    for field in [
        "text-heading",
        "text-field-words",
        "text-field-length",
        "text-field-fade-in",
        "text-field-fade-out",
        "text-field-curve",
    ] {
        let widget = find_widget(window.upcast_ref(), field).unwrap_or_else(|| panic!("the form draws no {field}"));
        assert!(
            widget.is_visible() && widget.is_mapped(),
            "`{field}` is not on screen in the staged shot"
        );
    }
    // The words field the shot types into is the form's own TextView, and it holds what staging put in it.
    let words = find_widget(window.upcast_ref(), "text-field-words")
        .and_then(|w| w.downcast::<gtk::TextView>().ok())
        .expect("the caption form draws its words field");
    let (start, end) = words.buffer().bounds();
    assert_eq!(
        words.buffer().text(&start, &end, true).as_str(),
        fx_text::SNAPSHOT_CAPTION,
        "the staged shot left the words field empty -- the picture would show a form with no caption"
    );

    // Tidy: take the arm and the form back down (`press_text_cancel` writes no history).
    let said = ui::press_text_cancel(&window);
    assert!(said.contains("no caption placed"), "cancel said {said}");
    RAN_TEXT_STAGE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s3_the_staged_text_shot_has_its_form_open() {
    let root = std::env::temp_dir().join(format!("np-s06e2text-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(staged_text_shot_has_its_form_open);
        app.run();
    });

    assert!(RAN_TEXT_STAGE.load(Ordering::SeqCst), "the `06-text` staging check never ran");
}
