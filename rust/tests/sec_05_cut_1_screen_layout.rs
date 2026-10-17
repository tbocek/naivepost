//! §05-cut#1-screen — the Cut page's own ORDER, measured off a real window rather than eyeballed.
//!
//! `spec/inventory/cut.md` §A: "Page = vertical paned (380): top = horizontal paned (660) of preview
//! (… ) and the form column; bottom = toolbar, tracks, scrollbar." §1 counts the same top to bottom:
//! **1** preview with the reading column beside it, **5**-**22** the toolbar, **23**-**30** the tracks,
//! **31** the scrollbar. The widgets were built in the order the flows arrived, so before
//! `ui::cut_page_layout::arrange` existed the shot showed the tape at the top, one small plate of
//! preview under it, ~750 px of empty box, and the strip narrower than the band beside it.
//!
//! Every check reads NUMBERS out of the realized tree — child order from `first_child()`/
//! `next_sibling()`, positions from `compute_bounds()`, widths from `width()` — because a layout that
//! merely renders is not evidence: the broken page rendered fine and was wrong by 750 px.

use adw::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_ORDER: AtomicBool = AtomicBool::new(false);
static RAN_NO_GAP: AtomicBool = AtomicBool::new(false);
static RAN_WIDTHS: AtomicBool = AtomicBool::new(false);

/// How far below the toolbar's bottom edge the tape may start. §A puts the tracks DIRECTLY under the
/// toolbar; the arrangement bug left ~750 px of empty plate between them, so anything past this fails.
const MAX_TOOLBAR_TO_TAPE_PX: i32 = 260;

fn find_named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        for child in widget.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            if let Some(found) = walk(child, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

/// The page's own vertical box: the ancestor whose DIRECT children include the toolbar, which is where
/// every part of the Cut page was appended. Reading children here rather than walking the whole tree is
/// what makes "which comes first" a question with one answer.
fn page_box_of(toolbar: &gtk::Widget) -> gtk::Box {
    let mut candidate = toolbar.parent().expect("the toolbar sits in a box");
    loop {
        let holds = candidate
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .filter_map(|child| child.downcast::<gtk::Widget>().ok())
            .any(|child| &child == toolbar);
        if holds {
            break;
        }
        candidate = candidate.parent().expect("the toolbar is nested deeper than expected");
    }
    candidate
        .downcast::<gtk::Box>()
        .unwrap_or_else(|_| panic!("the Cut page's container is a gtk::Box"))
}

fn child_names(page_box: &gtk::Box) -> Vec<String> {
    page_box
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .collect()
}

/// A widget's top and bottom edge in window coordinates. `translate_coordinates` answers with the point
/// relative to the native surface, which is the one frame every widget on this page shares.
#[allow(deprecated)] // 4.12's replacement needs a surface node this tree does not hand out
fn edges(widget: &gtk::Widget) -> (i32, i32) {
    let native = widget.native().expect("the page is inside a realized window");
    let (_, y) = widget.translate_coordinates(&native, 0.0, 0.0).expect("the widget is laid out");
    let top = y as i32;
    (top, top + widget.height())
}

/// A widget's left edge in window coordinates, alongside its top edge.
#[allow(deprecated)] // same door as [`edges`]
fn left_edge(widget: &gtk::Widget) -> (i32, i32) {
    let native = widget.native().expect("the page is inside a realized window");
    let (x, y) = widget.translate_coordinates(&native, 0.0, 0.0).expect("the widget is laid out");
    (x as i32, y as i32)
}

/// A window on the Cut page over a session with footage, sized the way a real window is: a Cut-page
/// layout collapses to one column without an explicit size before `present()`.
fn cut_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-layout-w-{}", std::process::id()));
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
    window.set_default_size(1440, 900);
    window.present();
    naivepost::ui::tab_button(&window, naivepost::shell::Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    // Recordings BEFORE the cut: `set_session_recordings` borrows the page's own slot and a
    // `seed_review_cut` that follows re-reads it, so the order matters (the reverse panics `RefCell
    // already borrowed` inside `cut_folds`). Same order `tests/sec_05_cut_1_band_paints.rs` uses.
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

/// §A's order as numbers: `cut-top` above the toolbar, the toolbar above the tape, and the scrollbar
/// last of everything the arrangement places.
fn check_the_page_children_are_in_spec_order(app: &adw::Application) {
    let window = cut_page(app);
    let toolbar = find_named(&window, "cut-toolbar").expect("§A's toolbar is on the page");
    let page_box = page_box_of(&toolbar);
    let names = child_names(&page_box);
    let at = |name: &str| -> usize {
        names
            .iter()
            .position(|found| found == name)
            .unwrap_or_else(|| panic!("`{name}` is not a direct child of the page box: {names:?}"))
    };
    assert!(
        at("cut-top") < at("cut-toolbar"),
        "the preview/form paned comes before the toolbar: {names:?}"
    );
    assert!(
        at("cut-toolbar") < at("track-layers"),
        "the toolbar comes before the tape: {names:?}"
    );
    assert!(
        at("track-layers") < at("track-strip"),
        "the band and its overlay stack come before the trim strip: {names:?}"
    );
    let bar = at("track-scrollbar");
    for name in ["cut-toolbar", "track-layers", "track-strip", "camera-rows", "effects-lane"] {
        assert!(at(name) < bar, "`{name}` sits above the scrollbar: {names:?}");
    }
    RAN_ORDER.store(true, Ordering::SeqCst);
}

/// No wall of empty plate between the toolbar and the tape: the gap is a number read off the tree.
fn check_no_gap_between_toolbar_and_tape(app: &adw::Application) {
    let window = cut_page(app);
    let toolbar = find_named(&window, "cut-toolbar").expect("§A's toolbar is on the page");
    let layers = find_named(&window, "track-layers").expect("the band's layer stack is on the page");
    let (_, toolbar_bottom) = edges(&toolbar);
    let (tape_top, _) = edges(&layers);
    let gap = tape_top - toolbar_bottom;
    println!("GAP toolbar-bottom {toolbar_bottom} -> tape-top {tape_top} = {gap} px");
    assert!(
        (0..=MAX_TOOLBAR_TO_TAPE_PX).contains(&gap),
        "the tape starts {gap} px below the toolbar; \u{a7}A packs it directly under it"
    );
    RAN_NO_GAP.store(true, Ordering::SeqCst);
}

/// §1's seat for badge 1: the preview got its pane (not the 240 px thumbnail of the broken shot), the
/// reading column sits BESIDE it rather than stacked under it, and the eight rows keep their own width.
fn check_the_preview_outranks_the_reading_column(app: &adw::Application) {
    let window = cut_page(app);
    let preview = find_named(&window, "preview-panel").expect("§A's preview panel is on the page");
    let frame = find_named(&window, "cut-form-frame").expect("§A's form column is on the page");
    let grid = find_named(&window, "cut-form").expect("the idle readings grid is in the column");
    let (preview_w, grid_w) = (preview.width(), grid.width());
    let (preview_x, _) = left_edge(&preview);
    let (frame_x, _) = left_edge(&frame);
    println!("SEAT preview {preview_w} px at x={preview_x}, form column starts x={frame_x}, readings {grid_w} px");
    assert!(
        preview_w >= naivepost::ui::window::PREVIEW_PANEL_W as i32 + 300,
        "the preview holds its pane, not its {} px minimum: {preview_w} px",
        naivepost::ui::window::PREVIEW_PANEL_W as i32
    );
    assert!(
        frame_x >= preview_x + preview_w - 2,
        "the reading column starts to the RIGHT of the picture ({frame_x} vs {preview_x}+{preview_w})"
    );
    assert!(
        grid_w >= naivepost::ui::cut_page_layout::FORM_COLUMN_MIN_W,
        "the eight readings keep their own width: {grid_w} px"
    );
    RAN_WIDTHS.store(true, Ordering::SeqCst);
}

fn round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(naivepost::ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            println!("CHECK 1 the page's children are in \u{a7}A's order");
            check_the_page_children_are_in_spec_order(app);
            println!("CHECK 2 no empty wall between the toolbar and the tape");
            check_no_gap_between_toolbar_and_tape(app);
            println!("CHECK 3 the preview takes the wide half");
            check_the_preview_outranks_the_reading_column(app);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn sec_05_cut_1_screen_the_page_stacks_preview_then_toolbar_then_tracks_then_scrollbar() {
    common::pin_private_cwd();
    round();
    assert!(RAN_ORDER.load(Ordering::SeqCst), "the child-order check never ran");
}

#[test]
fn sec_05_cut_1_screen_the_tape_sits_directly_under_the_toolbar() {
    common::pin_private_cwd();
    round();
    assert!(RAN_NO_GAP.load(Ordering::SeqCst), "the toolbar-to-tape gap check never ran");
}

#[test]
fn sec_05_cut_1_screen_the_preview_gets_the_wide_half_of_the_top_row() {
    common::pin_private_cwd();
    round();
    assert!(RAN_WIDTHS.load(Ordering::SeqCst), "the preview-width check never ran");
}
