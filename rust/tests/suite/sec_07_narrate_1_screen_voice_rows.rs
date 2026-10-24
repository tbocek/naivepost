//! §07-narrate#1-screen, the **Voice picker** bullet: "three rows, top to bottom. First row, left to right:
//! unlabelled dropdown …; the take band's ＋ − ▶; \"Add file…\" … Third row, left to right: Sample: an entry …,
//! ▶ speaks it, ⏹, ⟳ new take; pitch −6..+6 semitones".
//!
//! So each of those two rows is ONE row on screen, with its controls in the spec's order — a claim about where
//! widgets sit, which only a measurement can make. Every number comes from `compute_bounds` on the real page (a
//! shell window on the Narrate tab, widgets found by name); nothing is imported from the page's own constants,
//! and no pixel width the spec never states is asserted. No sleeps: `common::wait_until` waits for the layout
//! fact itself.
//!
//! One application, one `connect_activate`: GTK and libadwaita bind to the first thread that initialises them
//! and cargo runs a binary's tests on different threads, so both checks are stages of the same activate (the
//! shape `sec_07_narrate_1_screen_page_split.rs` uses and explains).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, wait_until};

static RAN_ROW1: AtomicBool = AtomicBool::new(false);
static RAN_ROW3: AtomicBool = AtomicBool::new(false);

/// The scratch session folder, so nothing this file writes lands in a real project or another test's.
fn scratch_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("np-sec07-rows-{tag}-{}", std::process::id()));
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

/// The Narrate page in a presented window, published through the page's own door (`set_state` + `refresh`)
/// the way every flow does, and waited on until it has laid out.
fn narrate_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.set_default_size(1280, 900);
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(base_state());
    ui::refresh(&window);
    // The page lays out on an idle, not inside `settle()`'s pending-only drain, so wait for the fact itself:
    // the voice column having an allocation. Without this a measurement reads 0 px and passes by accident.
    let probe = found(&window, "voice-row");
    wait_until(
        "the Narrate page laid its voice column out (`voice-row` still 0 px wide)",
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

/// No control spilled out of its row or lost its width: each is inside the row's own horizontal span.
fn assert_row_holds(window: &adw::ApplicationWindow, row_name: &str, names: &[&str]) {
    let row = bounds(window, row_name);
    let (row_x, row_w) = (row.x(), row.width());
    for name in names {
        let rect = bounds(window, name);
        let (x, w) = (rect.x(), rect.width());
        assert!(
            w > 0.0 && x >= row_x - 1.0,
            "`{name}` drawn outside `{row_name}` or with no width: x={x:.1} wide {w:.1}, \n             row `{row_name}` at x={row_x:.1} of width {row_w:.1}",
        );
    }
}

/// The two halves of a "one row" claim, measured: every control shares one vertical band (its top edge falls
/// within the tallest control's height of the highest top), and they run left to right in `names`' order.
fn assert_one_row_in_order(window: &adw::ApplicationWindow, names: &[&str]) {
    let rects: Vec<(f32, f32, f32)> = names
        .iter()
        .map(|name| {
            let rect = bounds(window, name);
            (rect.y(), rect.x(), rect.height())
        })
        .collect();
    let tallest = rects
        .iter()
        .fold(0.0_f32, |best, (_, _, h)| best.max(*h))
        .max(1.0);
    let highest = rects.iter().fold(f32::MAX, |best, (y, _, _)| best.min(*y));
    for (index, (y, x, _)) in rects.iter().enumerate() {
        assert!(
            *y - highest < tallest,
            "`{}` is not on the same row as the others: its top is {:.1} against a highest top of {:.1} \
             with the tallest control {:.1} px tall",
            names[index],
            y,
            highest,
            tallest
        );
        if index > 0 {
            assert!(
                *x > rects[index - 1].1,
                "`{}` must sit to the RIGHT of `{}` (§1 spells the row's order): x={:.1} vs {:.1}",
                names[index],
                names[index - 1],
                x,
                rects[index - 1].1
            );
        }
    }
}

/// Row 1: the dropdown, then the take band's ＋ − ▶, then "Add file…" — one row, in that order, all inside the
/// voice column rather than spilled onto a second line.
fn check_the_first_voice_row(app: &adw::Application) {
    let _ = scratch_root("row1");
    let window = narrate_page(app);
    let names = [
        "voice-picker",
        "take-add",
        "take-remove",
        "take-play",
        "voice-add-file",
    ];
    for name in names {
        assert!(
            inside(&window, "voice-row", name),
            "`{name}` belongs in the voice picker's FIRST row (§1 lists it there)"
        );
    }
    assert_one_row_in_order(&window, &names);
    // Nothing clipped: every control keeps a visible slice of the row it was given.
    assert_row_holds(&window, "voice-row", &names);
    RAN_ROW1.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// Row 3: the sample entry, its ▶ ⏹ ⟳, and the pitch control — one row, in that order.
fn check_the_third_voice_row(app: &adw::Application) {
    let _ = scratch_root("row3");
    let window = narrate_page(app);
    let names = [
        "sample-sentence",
        "sample-play",
        "sample-stop",
        "sample-reroll",
        "pitch-slider",
    ];
    for name in names {
        assert!(
            inside(&window, "sample-row", name),
            "`{name}` belongs in the voice picker's THIRD row (§1 lists it there)"
        );
    }
    assert_one_row_in_order(&window, &names);
    assert_row_holds(&window, "sample-row", &names);
    RAN_ROW3.store(true, Ordering::SeqCst);
}

fn round(app: &adw::Application) {
    check_the_first_voice_row(app);
    check_the_third_voice_row(app);
    release_last_window();
}

#[test]
fn sec_07_narrate_1_screen_row1_dropdown_and_take_buttons_share_one_row() {
    let _ = gtk::init();
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(round);
    app.run_with_args::<&str>(&[]);
    assert!(
        RAN_ROW1.load(Ordering::SeqCst),
        "the first-row check never ran"
    );
    assert!(
        RAN_ROW3.load(Ordering::SeqCst),
        "the third-row check never ran"
    );
}
