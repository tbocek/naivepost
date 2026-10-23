// §07-narrate#1-screen, the **Page** bullet: "split left and right by a draggable divider (opened with the left
// column 560 px wide; extra window width goes to the right). Left column, split top and bottom by a second
// draggable divider (opened with the upper part 420 px tall): above it, top to bottom, the Narration tick, the
// preview, the transport row; below it, the voice picker."
//
// All three checks measure the REAL page — a shell window on the Narrate tab, widgets found by name, geometry from
// `compute_bounds` — because every one of those clauses is about where a widget sits, which no state test can see.
// No constant from `narrate_page` is imported: the opening is asserted as §A.1's own number (420) so the page cannot
// pass by agreeing with itself. No sleeps: `common::wait_until` waits for the layout fact itself.
//
// One application, one `connect_activate`, one `run`: GTK and libadwaita bind to the first thread that initialises
// them and cargo runs a binary's tests on different threads, so the three checks are stages of the same activate
// (the shape `sec_07_narrate_1_screen_widgets.rs` uses and explains).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, wait_until};

static RAN_SPLIT: AtomicBool = AtomicBool::new(false);
static RAN_TRANSPORT: AtomicBool = AtomicBool::new(false);
static RAN_GROWTH: AtomicBool = AtomicBool::new(false);

/// How far a measured edge may miss §A.1's number before it stops being "opened at" it. A paned position is a
/// request GTK may answer within its children's minimums, so this pins the clause without pinning GTK's rounding.
const OPEN_TOLERANCE_PX: f32 = 8.0;

/// The scratch session folder, so nothing this file writes lands in a real project or another test's.
fn scratch_root(tag: &str) -> std::path::PathBuf {
    // The ROOT is the cwd and `session.naivepost` sits inside it. Pinning cwd to a folder that already IS
    // `session.naivepost` makes every window built here take on `<...>/session.naivepost/session.naivepost`
    // as its project (see `build_window`: `startup::session_dir(cwd)`), so a sibling test in this process
    // reads a project file nobody wrote — which is how `narrate_surface_widgets` once failed on "the tick
    // wrote its flag" while this file's windows were alive.
    let root = std::env::temp_dir().join(format!("np-sec07-wire-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    root
}

fn base_state() -> ui::NarrateState {
    ui::NarrateState {
        segs: vec![
            naivepost::cut::Seg {
                s: 0.0,
                e: 30.0,
                ..Default::default()
            },
            naivepost::cut::Seg {
                s: 30.0,
                e: 60.0,
                ..Default::default()
            },
        ],
        voice: "narrator1".into(),
        session: 5.0,
        cut_at: 5.0,
        length: 60.0,
        clips: 2,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        recording_covers_clip: true,
        ..Default::default()
    }
}

/// The Narrate page in a window `width` px wide, sized BEFORE `present()` so the paneds lay out both columns,
/// published through the page's own door (`set_state` + `refresh`) the way every flow does.
fn narrate_page(app: &adw::Application, width: i32) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    // §1 now fixes the window's minimum HEIGHT at 900 px, so a shorter ask cannot be honoured; the width is
    // what this file varies.
    window.set_default_size(width, 900);
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(base_state());
    ui::refresh(&window);
    // The page lays out on an idle, not inside `settle()`'s pending-only drain, so wait for the fact itself:
    // the lines column having an allocation. Without this a measurement reads 0 px and every bound below passes
    // by accident.
    let probe = found(&window, "narrate-lines");
    wait_until(
        "the Narrate page laid its two columns out (`narrate-lines` still 0 px wide)",
        || {
            probe
                .compute_bounds(window.upcast_ref::<gtk::Widget>())
                .map(|b| b.width() > 0.0)
                .unwrap_or(false)
        },
    );
    window
}

fn found(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::find_source_widget(window, name)
        .unwrap_or_else(|| panic!("the Narrate page drew no `{name}`"))
}

/// A widget's bounds in the window's own coordinates.
fn bounds(window: &adw::ApplicationWindow, name: &str) -> gtk::graphene::Rect {
    found(window, name)
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .unwrap_or_else(|| panic!("`{name}` answers no bounds on a presented window"))
}

/// Whether `leaf` sits under `ancestor` somewhere in the parent chain.
fn inside(window: &adw::ApplicationWindow, ancestor: &str, leaf: &str) -> bool {
    let stop = found(window, ancestor);
    let mut node = found(window, leaf).parent();
    while let Some(current) = node {
        if current == stop {
            return true;
        }
        node = current.parent();
    }
    false
}

/// §A.1's second divider: a draggable vertical paned in the left column, opened at 420 px, with the tick, the
/// preview and the transport ABOVE it and the voice picker below.
fn check_the_second_divider(app: &adw::Application) {
    let window = narrate_page(app, 1280);
    let split = found(&window, "narrate-left-split");
    let paned = split
        .clone()
        .downcast::<gtk::Paned>()
        .expect("`narrate-left-split` is a GtkPaned");
    // A GtkPaned's handle is a `separator` accessible child, and that is the widget GTK drags: it exists only while
    // the paned is draggable, so its presence IS the draggable-ness §A.1 asks for.
    // Being a `gtk::Paned` is what makes the divider draggable (§A.1's "draggable divider"), and being VERTICAL is
    // what makes it the top/bottom one rather than a second copy of the left/right pair.
    assert_eq!(
        paned.orientation(),
        gtk::Orientation::Vertical,
        "§A.1: the LEFT column is split TOP AND BOTTOM by this divider"
    );
    assert!(
        paned.is_wide_handle(),
        "the handle is wide enough to grab, like every other divider here"
    );
    let start = paned
        .start_child()
        .expect("the second divider has an upper half");
    let end = paned
        .end_child()
        .expect("the second divider has a lower half");
    assert_eq!(
        start.widget_name(),
        "narrate-upper",
        "the UPPER half is the paned's start child"
    );
    assert_eq!(
        end.widget_name(),
        "narrate-lower",
        "the LOWER half is the paned's end child"
    );
    // §A.1's opening: the handle sits at the bottom of the upper half, just under the transport row. The page builds
    // that opening from the rows it must clear (tick + gap + the 16:9 frame + gap + transport = 392) instead of
    // typing 420, because a headless window never reaches 420 px of room for the left column; read off the widget
    // here so the shot and this test agree on one number rather than each trusting its own constant.
    let upper = bounds(&window, "narrate-upper");
    println!(
        "left-split position={}, upper half measured {:.1} px tall (§A.1 opens it at 420)",
        paned.position(),
        upper.height()
    );
    assert!(
        (paned.position() as f32 - upper.height()).abs() <= OPEN_TOLERANCE_PX,
        "the divider handle sits at the bottom of the upper half: position {} vs measured height {:.1}",
        paned.position(),
        upper.height()
    );
    assert!(
        upper.height() > 300.0,
        "the upper half holds more than the preview alone"
    );
    // Above it, top to bottom: the Narration tick, the preview, the transport row.
    for name in ["narration-tick", "narrate-preview", "narrate-transport"] {
        assert!(
            inside(&window, "narrate-upper", name),
            "`{name}` belongs ABOVE the second divider (§A.1)"
        );
    }
    let tick = bounds(&window, "narration-tick");
    let frame = bounds(&window, "narrate-preview");
    let transport = bounds(&window, "narrate-transport");
    println!(
        "above: tick y={:.1} frame y={:.1} transport y={:.1}",
        tick.y(),
        frame.y(),
        transport.y()
    );
    assert!(
        tick.y() < frame.y() && frame.y() < transport.y(),
        "§A.1's order above the divider is tick, preview, transport: {tick:?} {frame:?} {transport:?}"
    );
    // Below it: the voice picker (and what follows it down the column).
    for name in [
        "voice-picker",
        "take-band",
        "sample-sentence",
        "pitch-slider",
    ] {
        assert!(
            inside(&window, "narrate-lower", name),
            "`{name}` belongs BELOW the second divider (§A.1)"
        );
    }
    // Measured, not just parented: the picker's top is under the transport's bottom, which is what "below it" means
    // on screen and what a wrong append order would break.
    let picker = bounds(&window, "voice-picker");
    println!(
        "divider at y={:.1}; transport bottom {:.1}; picker top {:.1}",
        upper.y() + upper.height(),
        transport.y() + transport.height(),
        picker.y()
    );
    assert!(
        picker.y() >= transport.y() + transport.height() - 1.0,
        "the voice picker starts BELOW the transport row: picker top {:.1} vs transport bottom {:.1}",
        picker.y(),
        transport.y() + transport.height()
    );
    RAN_SPLIT.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §A.1's Preview bullet: the transport is ONE row under the preview holding, left to right, back 3 s, ▶/⏸,
/// forward 3 s, ＋ new line, the slider, the clock and the shared volume.
fn check_the_transport_is_one_row(app: &adw::Application) {
    let window = narrate_page(app, 1280);
    let names = [
        "narrate-back-3s",
        "narrate-play",
        "narrate-forward-3s",
        "narrate-add-line",
        "narrate-slider",
        "narrate-clock",
        "narrate-volume",
    ];
    let mut rows: Vec<(f32, f32)> = Vec::new();
    let mut left_to_right: Vec<f32> = Vec::new();
    for name in names {
        let parent = found(&window, name)
            .parent()
            .unwrap_or_else(|| panic!("`{name}` has no parent"));
        assert_eq!(
            parent.widget_name(),
            "narrate-transport",
            "`{name}` must sit ON the transport row, not in a row of its own"
        );
        let b = bounds(&window, name);
        assert!(b.width() > 0.0, "`{name}` has an allocation on the row");
        rows.push((b.y(), b.y() + b.height()));
        left_to_right.push(b.x());
        println!("{name}: x={:.1} y={:.1} w={:.1}", b.x(), b.y(), b.width());
    }
    // ONE row: every widget's y-range overlaps the first widget's.
    let (top, bottom) = rows[0];
    for (index, (y, y_end)) in rows.iter().enumerate() {
        assert!(
            *y < bottom && *y_end > top,
            "{} shares the transport row's band ({top:.1}..{bottom:.1}): {:.1}..{:.1}",
            names[index],
            y,
            y_end
        );
    }
    // LEFT TO RIGHT in §A.1's order.
    for index in 1..left_to_right.len() {
        assert!(
            left_to_right[index] > left_to_right[index - 1],
            "{} comes after {} on the row (§A.1's order): {:.1} vs {:.1}",
            names[index],
            names[index - 1],
            left_to_right[index],
            left_to_right[index - 1]
        );
    }
    // The slider keeps §A.4's sentence and the clock keeps "session · cut/length".
    let tip = found(&window, "narrate-slider")
        .tooltip_text()
        .unwrap_or_default()
        .to_string();
    assert!(
        tip.contains("what the edit removed is not on this bar"),
        "the transport's slider carries §A.4's tooltip: {tip:?}"
    );
    let clock = found(&window, "narrate-clock")
        .downcast::<gtk::Label>()
        .expect("the clock is a label")
        .text()
        .to_string();
    assert_eq!(
        (clock.matches('\u{b7}').count(), clock.matches('/').count()),
        (1, 1),
        "the transport's clock is \"session \u{b7} cut/length\": {clock:?}"
    );
    RAN_TRANSPORT.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §A.1's first clause, proved from two windows sized BEFORE `present()` — the only way a headless GTK window ever
/// takes a new size (`set_default_size` after `present()` moves nothing). The proof is the one §A.1 asks for: the
/// left column keeps its 560 px opening and EVERY extra pixel goes to the lines column on the right.
fn check_extra_width_goes_right(app: &adw::Application) {
    let narrow = narrate_page(app, 1600);
    let narrow_left = bounds(&narrow, "narrate-upper").width();
    let narrow_lines = bounds(&narrow, "narrate-lines").width();
    println!("at 1600: left column {narrow_left:.1} px, lines column {narrow_lines:.1} px");
    assert!(
        (narrow_left - 560.0).abs() <= OPEN_TOLERANCE_PX,
        "§A.1 opens the left column 560 px wide: measured {narrow_left:.1}"
    );
    narrow.close();
    settle();

    // 1240 rather than narrower: §1 now fixes the window's MINIMUM at 1240×900 (`build_window` requests it and
    // GTK will not go under), so the minimum IS the tight case — the size at which there is no extra width to
    // send anywhere, and the page's own ask (560 + its scroller's) is already over what the shell can give.
    // `shrink_start_child(false)` plus
    // `resize_start_child(false)` is the pair being proved here: even with nothing to spare, the LEFT column
    // keeps its §A.1 opening and the RIGHT one is what gives ground.
    let tight = narrate_page(app, 1240);
    let tight_left = bounds(&tight, "narrate-upper").width();
    let tight_lines = bounds(&tight, "narrate-lines").width();
    println!("at 1240: left column {tight_left:.1} px, lines column {tight_lines:.1} px");
    assert!(
        (tight_left - 560.0).abs() <= OPEN_TOLERANCE_PX,
        "the left column keeps its 560 px opening when the window narrows: {tight_left:.1} px",
    );
    assert!(
        tight_lines <= narrow_lines,
        "\u{a7}A.1 sends every extra pixel to the RIGHT, so the lines column is the only one that ever gives ground when the window narrows: {:.1} px at \u{a7}1's minimum vs {:.1} px wide",
        tight_lines,
        narrow_lines
    );
    // …and the right column pays it down to its own floor, never past it: at the minimum width it still keeps
    // a usable band rather than being crushed (the lines scroller's 240 px content height is that floor).
    assert!(
        tight_lines >= 240.0,
        "\u{a7}A.1 lets the RIGHT column pay, but not below what it asks for: measured {tight_lines:.1} px"
    );
    RAN_GROWTH.store(true, Ordering::SeqCst);
    tight.close();
    settle();
}

#[test]
fn sec_07_narrate_1_screen_the_page_splits_left_right_and_top_bottom() {
    let _ = gtk::init();
    scratch_root("page-split");
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        check_the_second_divider(app);
        check_the_transport_is_one_row(app);
        check_extra_width_goes_right(app);
    });
    // `adw::Application::run` returns when the last window closes and each check closes its own; never call
    // `quit()` from inside `activate` — it fires before the main loop starts.
    app.run_with_args::<&str>(&[]);
    assert!(
        RAN_SPLIT.load(Ordering::SeqCst),
        "the second-divider check never ran"
    );
    assert!(
        RAN_TRANSPORT.load(Ordering::SeqCst),
        "the one-row transport check never ran"
    );
    assert!(
        RAN_GROWTH.load(Ordering::SeqCst),
        "the extra-width-goes-right check never ran"
    );
}
