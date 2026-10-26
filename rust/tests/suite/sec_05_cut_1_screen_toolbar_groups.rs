//! §05-cut#1-screen "Toolbar groups", read against inventory §A: each group is `linked(...)`, the groups stand
//! `cut_screen::TOOLBAR_GAP_PX` apart (the prototype's bar spacing), a separator follows the preview volume, the
//! verbs, history and zoom wear the prototype's icons, and of ▶, ▶✂, ▶✂✂ exactly the one in use is lit (§D's
//! "exactly one lit"). The first port packed every button flush against the next with no group visible.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut_screen;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, settle};

static RAN: AtomicBool = AtomicBool::new(false);

fn named(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    common::find_named(window, name).unwrap_or_else(|| panic!("no `{name}` on the page"))
}

fn check_the_groups_are_linked_and_spaced(window: &adw::ApplicationWindow) {
    let bar = named(window, "cut-toolbar")
        .downcast::<gtk::Box>()
        .expect("the toolbar is a box");
    assert_eq!(
        bar.spacing(),
        cut_screen::TOOLBAR_GAP_PX,
        "the groups stand the prototype's gap apart"
    );
    for group in ["transport", "verbs", "history", "zoom"] {
        let row = named(window, &format!("group-{group}-row"));
        assert!(
            row.has_css_class("linked"),
            "`group-{group}` is one linked control (inventory \u{a7}A)"
        );
    }
    // The transport is what a hand goes back to all the time: each of its buttons keeps a thumb-wide target.
    for name in [
        "play-recording-button",
        "play-cut-button",
        "review-cuts-button",
        "line-step-back-five",
        "line-step-back",
        "line-step-forward",
        "line-step-forward-five",
    ] {
        let width = named(window, name).width();
        assert!(
            width >= cut_screen::TRANSPORT_MIN_WIDTH_PX as i32,
            "`{name}` is {width} px wide, under the transport's {} px",
            cut_screen::TRANSPORT_MIN_WIDTH_PX
        );
    }
    // Inventory §A item 3: the separator between the volume and the verbs.
    let order: Vec<String> = bar
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .collect();
    let at = |name: &str| {
        order
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| panic!("`{name}` is on the bar: {order:?}"))
    };
    assert_eq!(
        at("toolbar-rule"),
        at("group-volume") + 1,
        "the separator follows the volume: {order:?}"
    );
    assert_eq!(
        at("group-verbs"),
        at("toolbar-rule") + 1,
        "and the verbs follow it: {order:?}"
    );
    assert!(
        named(window, "toolbar-rule").is::<gtk::Separator>(),
        "a real separator"
    );
    // The prototype's icons on the verbs, the history and the zoom.
    for name in [
        "add-button",
        "split-button",
        "remove-button",
        "copy-button",
        "paste-button",
        "lane-button",
        "undo-button",
        "redo-button",
        "revert-button",
        "clear-cut-button",
        "zoom-out-button",
        "zoom-in-button",
    ] {
        let button = named(window, name)
            .downcast::<gtk::Button>()
            .expect("a button");
        assert_eq!(
            button.icon_name().as_deref(),
            cut_screen::tool_icon(name),
            "`{name}` wears its icon"
        );
        assert!(cut_screen::tool_icon(name).is_some(), "`{name}` has one");
        // Linked buttons stand flush, so each icon keeps its own width or the icons touch.
        assert!(
            button.width() >= cut_screen::BUTTON_MIN_WIDTH_PX as i32,
            "`{name}` is {} px wide, under the bar's {} px",
            button.width(),
            cut_screen::BUTTON_MIN_WIDTH_PX
        );
    }
}

fn check_exactly_one_preview_is_lit(window: &adw::ApplicationWindow) {
    let lit = |name: &str| named(window, name).has_css_class("suggested-action");
    let all = || {
        [
            lit("play-recording-button"),
            lit("play-cut-button"),
            lit("review-cuts-button"),
        ]
    };
    assert_eq!(
        all(),
        [true, false, false],
        "the page opens on the recording, so \u{25b6} is lit"
    );
    ui::set_preview_state(window, true, false, Default::default());
    ui::cut_record_play::light_lit(window);
    assert_eq!(
        all(),
        [false, true, false],
        "under \u{2702} the cut's \u{25b6} is lit"
    );
    ui::set_preview_state(window, true, true, Default::default());
    ui::cut_record_play::light_lit(window);
    assert_eq!(
        all(),
        [false, false, true],
        "a review lights \u{25b6}\u{2702}\u{2702}"
    );
    ui::set_preview_state(window, false, false, Default::default());
    ui::cut_record_play::light_lit(window);
    assert_eq!(all(), [true, false, false], "and back");
}

/// ▶ and ⏸ are drawn from different fonts and ⏸ stands taller: each button is sized for both faces, so a press
/// that swaps them never moves the bar (the first rewrite's button grew a pixel on every ▶).
fn check_play_and_pause_faces_are_one_size(window: &adw::ApplicationWindow) {
    let size = |name: &str| {
        let button = named(window, name);
        let (_, wide, _, _) = button.measure(gtk::Orientation::Horizontal, -1);
        let (_, high, _, _) = button.measure(gtk::Orientation::Vertical, -1);
        (wide, high)
    };
    let face = |name: &str| {
        named(window, name)
            .downcast::<gtk::Button>()
            .expect("a button")
            .label()
            .unwrap_or_default()
            .to_string()
    };
    let paint = || {
        ui::cut_record_play::paint_button(window);
        ui::cut_cut_play::paint(window);
        ui::cut_review_play::paint(window);
        settle();
    };
    for (cut_only, reviewing, name) in [
        (false, false, "play-recording-button"),
        (true, false, "play-cut-button"),
        (true, true, "review-cuts-button"),
    ] {
        let transport = |playing| naivepost::run::Transport {
            playing,
            started: true,
        };
        ui::set_preview_state(window, cut_only, reviewing, transport(false));
        paint();
        let resting = size(name);
        ui::set_preview_state(window, cut_only, reviewing, transport(true));
        paint();
        assert!(
            face(name).starts_with('\u{23f8}'),
            "`{name}` wears \u{23f8} while its preview plays: {}",
            face(name)
        );
        assert_eq!(
            size(name),
            resting,
            "`{name}` keeps its size when \u{25b6} turns into \u{23f8}"
        );
        // This box's fonts may draw both faces the same height, so the size check alone cannot see the pixel a
        // desktop's fallback font adds: the label must RESERVE room for the larger face, whichever it shows.
        let label = named(window, name)
            .downcast::<gtk::Button>()
            .expect("a button")
            .child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .expect("the face is a label");
        let (reserved_w, reserved_h) = label.size_request();
        for other in [face(name), face(name).replacen('\u{23f8}', "\u{25b6}", 1)] {
            let (w, h) = label.create_pango_layout(Some(&other)).pixel_size();
            assert!(
                reserved_w >= w && reserved_h >= h,
                "`{name}` reserves {reserved_w}x{reserved_h} px, under the {w}x{h} of {other:?}"
            );
        }
    }
    ui::set_preview_state(window, false, false, Default::default());
    paint();
}

#[test]
fn sec_05_cut_1_screen_toolbar_groups_are_linked_spaced_and_one_preview_is_lit() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
        let window = ui::build_window(app, &model, "Prepare");
        common::size_to_minimum(&window);
        window.present();
        ui::tab_button(&window, Page::Cut)
            .expect("the shell has a Cut tab")
            .emit_clicked();
        common::wait_for_stable_layout(&window);
        settle();
        check_the_groups_are_linked_and_spaced(&window);
        check_exactly_one_preview_is_lit(&window);
        check_play_and_pause_faces_are_one_size(&window);
        window.close();
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the toolbar round never ran");
}
