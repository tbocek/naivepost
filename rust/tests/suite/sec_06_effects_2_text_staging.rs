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
use naivepost::fx_text;
use naivepost::ui;

use crate::common;
use common::settle;

static RAN_TEXT_STAGE: AtomicBool = AtomicBool::new(false);

/// The second the `06-text` shot places its line at, mirrored from the staging branch's own value so a change
/// there without a change here fails loudly. (The branch spells 81.0 so the heading reads `Text at 1:21`, as
/// `spec/img/06-text.png` shows it.)
const LINE_SECOND: f64 = 81.0;

// The shared builder, under the name these checks already call: an empty Cut opened on the Cut page.
use common::cut_page_empty as cut_page;

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
    let owned = naivepost::snapshot_effects::paint(
        &window,
        std::path::Path::new("/tmp/f3_4_text_staging"),
        // The PNG this staging would write its sidecar beside; `06-text` writes none, so the path is only
        // carried through the door (the lane screen is the one that puts a second image next to it).
        std::path::Path::new("/tmp/f3_4_text_staging.png"),
        "06-text",
    );
    assert!(
        owned,
        "`snapshot_effects::paint` no longer owns the `06-text` screen"
    );
    let form = ui::text_form_open().expect("the staged `06-text` shot drew no caption form");
    assert_eq!(
        form.t, LINE_SECOND,
        "the staged caption does not start at the line it placed"
    );
    assert_eq!(
        form.on,
        fx_text::LOWER_THIRD,
        "a click puts the caption across the lower third, as `spec/img/06-text.png` shows"
    );
    // The heading the shot shows, spelled by the module that owns it -- and the app's own `tools::mm_ss`, not a
    // hand-rolled copy of it, so the two can never drift.
    assert_eq!(
        fx_text::form_title(form.t),
        format!("Text at {}", naivepost::tools::mm_ss(form.t))
    );
    for field in [
        "text-heading",
        "text-field-words",
        "text-field-length",
        "text-field-fade-in",
        "text-field-fade-out",
        "text-field-curve",
    ] {
        let widget = find_widget(window.upcast_ref(), field)
            .unwrap_or_else(|| panic!("the form draws no {field}"));
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
        app.run_with_args::<&str>(&[]);
    });

    assert!(
        RAN_TEXT_STAGE.load(Ordering::SeqCst),
        "the `06-text` staging check never ran"
    );
}

/// §F3.4 S4 (`PREVIEW = RENDER`, and the shot has to show the words): a placed caption is only painted where
/// `fx_lane::visibility_at` says it is visible. The staging used to leave the red line AT the record's own first
/// second, and `visibility_at` ramps the fade-in from there (`(t - from) / trans`), so the frame painted the
/// caption at alpha 0.0 -- an invisible caption on a screen whose whole point is the caption. This pins both
/// edges of that ramp: zero AT the record's second, full once the fade has run out.
#[test]
fn f3_4_s4_the_placed_caption_is_visible_at_the_second_the_shot_paints_it() {
    use naivepost::cut::Fx;
    // The record the `06-text` shot applies: 81.0 s (the line the shot places), 3 s long, 0.3 s fades.
    let caption = Fx {
        kind: "text".into(),
        t: 81.0,
        dur: 3.0,
        trans: 0.3,
        tout: 0.3,
        text: fx_text::SNAPSHOT_CAPTION.to_string(),
        ..Default::default()
    };
    // An overlay the paused preview draws at all: a zoom's rect and a rate's mask are guides, a caption is ink.
    assert!(
        naivepost::fx_lane::drawn_paused(naivepost::cut::EffectKind::Text),
        "a caption must be one of the kinds the paused frame paints"
    );
    // The bug: standing exactly on the record's first second, the fade-in has not started -- alpha 0.0.
    assert_eq!(naivepost::fx_lane::visibility_at(&caption, 81.0), 0.0);
    // What the staging now does instead: one second inside, past the whole fade-in, fully visible.
    assert_eq!(naivepost::fx_lane::visibility_at(&caption, 82.0), 1.0);
    // And the far edge keeps working: inside the fade-out the caption is partly there, past the span it is gone.
    assert!(
        (naivepost::fx_lane::visibility_at(&caption, 83.9) - (84.0 - 83.9) / 0.3).abs() < 1e-9,
        "the fade-out half of the ramp is what the preview paints too"
    );
    assert_eq!(naivepost::fx_lane::visibility_at(&caption, 84.0), 0.0);
}
