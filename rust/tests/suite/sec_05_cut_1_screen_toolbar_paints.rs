//! §05-cut#1-screen — the two things the Cut shot had never shown: every control §1 numbers **5**-**22**
//! PAINTED inside its own toolbar group (not merely inside the bar, which a group's scroller can still clip),
//! and badge **31**, the scrollbar, with its geared thumb once the tape outgrows the view.
//!
//! `spec/05-cut.md` §1 lists both, and `spec/img/05-cut.png` draws both. The rules themselves live in
//! `cut_view` (`fits`, `thumb_geared`, `DRAG_PER_SCREENFUL_PX`) and were already right; what was wrong was
//! how the shot got there. These checks drive the page's own doors (the toolbar's − / + buttons, which is
//! what `snapshot.rs` presses too) rather than poking state, so a future change that forces a widget visible
//! or hides one the geometry asks for fails here.

use adw::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_PAINTS: AtomicBool = AtomicBool::new(false);
static RAN_S31: AtomicBool = AtomicBool::new(false);

/// §1 items **5**-**22**, by the names every flow's tests already use.
const BAR_CONTROLS: [(&str, &str); 18] = [
    ("play-recording-button", "5 \u{25b6} the recording"),
    ("play-cut-button", "6 \u{25b6}\u{2702} the cut"),
    ("review-cuts-button", "7 \u{25b6}\u{2702}\u{2702} review every cut"),
    ("line-step-back-five", "8 frame steps"),
    ("preview-volume", "9 preview volume"),
    ("add-button", "10 Add"),
    ("split-button", "11 Split"),
    ("remove-button", "12 Remove"),
    ("copy-button", "13 Copy"),
    ("paste-button", "14 Paste"),
    ("insert-button", "15 Insert"),
    ("lane-button", "16 Lane"),
    ("effect-button", "17 Effect"),
    ("undo-button", "18 Undo"),
    ("redo-button", "19 Redo"),
    ("revert-button", "20 Revert"),
    ("clear-cut-button", "21 Clear"),
    ("zoom-in-button", "22 zoom +"),
];


/// A Cut page over a session with footage, at §1's own minimum size — the size at which the toolbar must
/// show everything, and the size `just snapshot 05-cut` renders at. Shape of
/// `sec_05_cut_1_screen_toolbar_fits.rs::cut_page`.
fn cut_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-paint-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");
    let session = naivepost::startup::session_dir(&root);
    let mut seed = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    seed.sources.iter_mut().for_each(|source| source.footage = true);
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a real project on disk");

    let window = naivepost::ui::build_window(app, &seed, "Prepare");
    hold_last_window(window.clone());
    window.set_default_size(1240, 900);
    window.present();
    naivepost::ui::tab_button(&window, naivepost::shell::Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    naivepost::ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "lecture".into(),
        start: 0.0,
        end: 120.0,
    }]);
    let seeded = naivepost::cut::Cut {
        segs: vec![naivepost::cut::Seg { s: 0.0, e: 60.0, ..Default::default() }],
        ..Default::default()
    };
    naivepost::ui::seed_review_cut(&window, &seeded);
    naivepost::ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 10.0 });
    settle();
    window
}

/// The scroller inside a control's `group-‹name›` box: its bounds are the hole the buttons show through, so
/// a control past its right edge is PAINTED clipped even while it sits inside the bar.
fn viewport_of(
    window: &adw::ApplicationWindow,
    widget: &gtk::Widget,
) -> (String, f64, f64) {
    let mut owner: Option<gtk::Widget> = Some(widget.clone().upcast());
    while let Some(node) = owner {
        let name = node.widget_name().to_string();
        if name.starts_with("group-") && !name.ends_with("-row") {
            let scroll = node
                .downcast_ref::<gtk::Box>()
                .and_then(|b| b.first_child())
                .unwrap_or_else(|| node.clone());
            let bounds = scroll
                .compute_bounds(window.upcast_ref::<gtk::Widget>())
                .expect("the group's scroller has bounds once laid out");
            return (name, f64::from(bounds.x()), f64::from(bounds.x() + bounds.width()));
        }
        owner = node.parent();
    }
    panic!("no `group-*` ancestor above `{}`", widget.widget_name());
}

/// Badge check: every §1 control's right edge is inside its OWN group's viewport, not just inside the bar.
fn check_every_control_paints_inside_its_group(app: &adw::Application) {
    let window = cut_page(app);
    let mut clipped: Vec<String> = Vec::new();
    for (name, what) in BAR_CONTROLS {
        let widget = common::find_named(&window, name)
            .unwrap_or_else(|| panic!("\u{a7}1 item {what} is missing from the page: `{name}`"));
        let bounds = widget
            .compute_bounds(window.upcast_ref::<gtk::Widget>())
            .unwrap_or_else(|| panic!("`{name}` ({what}) has no bounds -- never laid out"));
        let right = f64::from(bounds.x() + bounds.width());
        let (group, left, group_right) = viewport_of(&window, &widget);
        println!(
            "PAINT {name}: right={right:.0} inside {group} viewport {left:.0}..{group_right:.0} \
             mapped={} visible={}",
            widget.is_mapped(),
            widget.is_visible()
        );
        if !widget.is_visible() || !widget.is_mapped() {
            clipped.push(format!("{name} ({what}) is not painted"));
        } else if right > group_right + 1.0 {
            clipped.push(format!(
                "{name} ({what}) ends at {right:.0}, past {group}'s viewport {left:.0}..{group_right:.0}"
            ));
        }
    }
    assert!(
        clipped.is_empty(),
        "{} of {} \u{a7}1 controls are CLIPPED inside their own group: {}",
        clipped.len(),
        BAR_CONTROLS.len(),
        clipped.join("; ")
    );
    RAN_PAINTS.store(true, Ordering::SeqCst);
    window.close();
}

/// Badge **31**, both branches of §A's rule, driven through the toolbar's own − / + buttons (the doors a
/// click reaches), and the gearing §B asks for: one screenful is always `DRAG_PER_SCREENFUL_PX` of drag.
fn check_the_scrollbar_paints_a_geared_thumb(app: &adw::Application) {
    // `cut_view::PPS` and `ORIGIN` are process-wide: this file's sibling check never moves them, but other
    // files in ONE test binary do (`cut_view_scroll.rs` leaves the zoom at 40 px/s and the view scrolled), so
    // the ladder starts where §2 opens the page, at its start. Otherwise a session that already overflows at
    // a leftover zoom makes "zooming in shows the bar" unfalsifiable -- the floor check below would start high.
    naivepost::ui::cut_view::set_pps(naivepost::cut_screen::ZOOM_AT_OPEN);
    naivepost::ui::cut_view::set_origin(0.0, 1e9, 1.0);
    let window = cut_page(app);
    let bar = common::find_named(&window, "track-scrollbar")
        .expect("badge 31 is on the page")
        .downcast::<gtk::Scrollbar>()
        .expect("`track-scrollbar` is a scrollbar");
    let zoom_in = common::find_named(&window, "zoom-in-button").expect("\u{a7}1 item 22 is on the bar");
    let zoom_out = common::find_named(&window, "zoom-out-button").expect("\u{a7}1 item 22's twin is on the bar");
    let view = naivepost::ui::cut_view::surface_width(&window);

    // Walk all the way down with the page's own − door: the ladder's floor is the film length fitting.
    for _ in 0..20 {
        zoom_out.emit_by_name::<()>("clicked", &[]);
    }
    common::wait_until("`track-scrollbar` has been laid out", || bar.width() >= 0);
    let (total_floor, view_floor) = naivepost::ui::cut_view::extents(view);
    println!("S31 at floor: total {total_floor} px vs view {view_floor} px");
    assert!(
        naivepost::ui::cut_view::fits(total_floor, view_floor),
        "at the zoom floor the whole session fits the view: {total_floor} <= {view_floor}"
    );
    assert!(!bar.is_visible(), "\u{a7}A: the bar is hidden when everything fits");

    // Zoom in through the button until the tape outgrows the view.
    for _ in 0..12 {
        zoom_in.emit_by_name::<()>("clicked", &[]);
        let (total, _) = naivepost::ui::cut_view::extents(view);
        if !naivepost::ui::cut_view::fits(total, view) {
            break;
        }
    }
    common::wait_until("the scrollbar is shown once the tape overflows", || bar.is_visible());
    let (total, view_now) = naivepost::ui::cut_view::extents(view);
    assert!(
        !naivepost::ui::cut_view::fits(total, view_now),
        "zooming in over the toolbar's own + door made the tape wider than the view: {total} > {view_now}"
    );
    let adjustment = bar.adjustment();
    let geared_bar = naivepost::ui::cut_view::gear_px_for_test();
    let geared = naivepost::ui::cut_view::thumb_geared(total, geared_bar);
    println!(
        "S31 zoomed: total {total} px over view {view_now} px; bar length {geared_bar} px; \
         geared fraction {geared}; page_size {}; DRAG_PER_SCREENFUL_PX {}",
        adjustment.page_size(),
        naivepost::ui::cut_view::DRAG_PER_SCREENFUL_PX
    );
    assert!(bar.is_visible(), "so badge 31 shows");
    assert!(
        (adjustment.page_size() - view_now * geared).abs() < 1.0,
        "the thumb is the GEARED one \u{2014} {:?} of the bar, i.e. {:.0} px of drag per screenful \
         \u{2014} not a proportional sliver: page_size {}, expected {}",
        geared,
        naivepost::ui::cut_view::DRAG_PER_SCREENFUL_PX,
        adjustment.page_size(),
        view_now * geared
    );
    assert_eq!(
        naivepost::ui::cut_view::DRAG_PER_SCREENFUL_PX, 40.0,
        "\u{a7}B spells the gearing as 40 px of drag per screenful"
    );

    // And back down: the same door that showed the bar hides it again, because the geometry says so.
    // Measured against `zoom_surface_width`, the width the ladder's own floor is worked out over (the −
    // handler says so in `window_history_zoom::press_zoom`): the band beside it is as wide as the paned
    // makes it, which is wider than the window whenever another page's painted widgets ask for more, and a
    // fit asked of that width is a statement about the band, not about §A's rule.
    for _ in 0..20 {
        zoom_out.emit_by_name::<()>("clicked", &[]);
    }
    settle();
    let ladder = naivepost::ui::cut_view::zoom_surface_width(&window);
    let (total_back, view_back) = naivepost::ui::cut_view::extents(ladder);
    assert!(
        naivepost::ui::cut_view::fits(total_back, view_back),
        "walking − back to the floor fits the session again: {total_back} <= {view_back}"
    );
    assert!(!bar.is_visible(), "and the bar hides again at the floor");
    RAN_S31.store(true, Ordering::SeqCst);
    window.close();
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 every \u{a7}1 control 5-22 paints inside its own group");
        check_every_control_paints_inside_its_group(app);
        println!("CHECK 2 badge 31 shows a geared thumb when the tape overflows, and hides at the floor");
        check_the_scrollbar_paints_a_geared_thumb(app);
    });
}

#[test]
fn sec_05_cut_1_screen_toolbar_paints_each_numbered_control_in_its_group() {
    common::pin_private_cwd();
    round();
    assert!(RAN_PAINTS.load(Ordering::SeqCst), "check 1 did not run");
}

#[test]
fn sec_05_cut_1_screen_s31_the_scrollbar_paints_a_geared_thumb_when_the_tape_overflows() {
    common::pin_private_cwd();
    round();
    assert!(RAN_S31.load(Ordering::SeqCst), "check 2 did not run");
}
