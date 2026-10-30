//! §08-produce.md **§1 Screen** — the Produce page's GEOMETRY, measured off a real window.
//!
//! §1 says where the parts sit: "split left and right by a draggable divider opened at half: two columns
//! side by side, not one above the other. Left column, top to bottom: the Images heading (\"Add image…\"
//! at its right end), the images row, Edit instruction, Negative prompt, the Thumbnail heading (⤓ and ↻
//! at its right end), the thumbnail. Right column (at least 360 px wide), top to bottom: the Title
//! heading (↻ at its right end), the title entry, the YouTube description heading and box, a separator,
//! then the encoder settings under their Transcode heading (⤓ and ↻ at its right end), in a scroller."
//! And §A adds that the encoder settings are "laid out as label + control pairs in this order, left to
//! right and then down, several to a line".
//!
//! Every check reads NUMBERS out of the realized tree — `compute_bounds`, `start_child`/`end_child`,
//! parent walks — because a page that merely renders can be wrong: before this item the same page opened
//! its divider at 560 px, stacked every encoder setting on its own line, and had no separator at all.
//! Same reasoning as `sec_04_prepare_1_screen_split.rs`.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

const ITEM: &str = "sec_08_produce_1_screen";

/// How far the handle may miss "half" before it stops being opened at half: the paned's own position is a
/// request GTK answers within each column's minimum, so this pins the clause without pinning GTK's rounding.
const HALF_TOLERANCE_PX: f32 = 40.0;

/// Two controls count as sharing a line when their y-ranges overlap by more than this: enough to join a
/// label with its own control, small enough that rows on neighbouring grid lines never merge.
const SAME_LINE_PX: f32 = 12.0;

/// How far two encoder widgets may overlap in x before it is an overlap: whole pixels of GTK rounding, not a
/// pair sitting on top of another.
const OVERLAP_TOLERANCE_PX: f32 = 2.0;

static RAN_SPLIT: AtomicBool = AtomicBool::new(false);
static RAN_ORDER: AtomicBool = AtomicBool::new(false);
static RAN_PAIRS: AtomicBool = AtomicBool::new(false);
static RAN_FITS: AtomicBool = AtomicBool::new(false);
static RAN_PICTURE: AtomicBool = AtomicBool::new(false);
static RAN_READABLE: AtomicBool = AtomicBool::new(false);
static RAN_NO_OVERLAP: AtomicBool = AtomicBool::new(false);

fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&common::fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &model, naivepost::shell::Page::Produce.label());
    // Sized before `present()`: a headless window laid out at its natural size would hand the paned a
    // different width than §1's, and the handle's position is the thing being measured.
    window.set_default_size(1280, 800);
    window.present();
    // GTK allocates children on an idle after `present()`; wait for the layout fact itself rather than a
    // guessed number of turns — the settings grid having an allocation means the whole page has been laid out.
    let probe = widget(&window, "encoder-settings");
    common::wait_until(
        "the Produce page laid itself out (`encoder-settings` still 0 px wide)",
        || {
            probe
                .compute_bounds(window.upcast_ref::<gtk::Widget>())
                .map(|b| b.width() > 0.0)
                .unwrap_or(false)
        },
    );
    window
}

fn widget(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::find_source_widget(window, name).unwrap_or_else(|| panic!("{ITEM}: no widget named {name}"))
}

/// Bounds against the window, printed so the log carries the numbers a check decided on.
fn bounds(window: &adw::ApplicationWindow, what: &str) -> gtk::graphene::Rect {
    let w = widget(window, what);
    let b = w
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .unwrap_or_else(|| panic!("{ITEM}: {what} is not laid out"));
    println!(
        "PRODUCE {what} x={:.1} y={:.1} w={:.1} h={:.1}",
        b.x(),
        b.y(),
        b.width(),
        b.height()
    );
    b
}

/// §1's first sentence: a horizontal `gtk::Paned` (that is the draggable divider, and horizontal is what
/// rules out "one above the other"), opened at HALF, with the right column keeping §A's 360 px floor.
fn check_the_page_is_split_in_two(window: &adw::ApplicationWindow) {
    let split = widget(window, "produce-split");
    let paned = split
        .clone()
        .downcast::<gtk::Paned>()
        .expect("{ITEM}: produce-split is a Paned, which is what makes the divider draggable");
    assert_eq!(
        paned.orientation(),
        gtk::Orientation::Horizontal,
        "{ITEM}: the divider splits left and right, not top and bottom"
    );
    let start = paned
        .start_child()
        .expect("{ITEM}: the paned has a start child");
    let end = paned
        .end_child()
        .expect("{ITEM}: the paned has an end child");
    assert_eq!(
        start.widget_name(),
        "produce-left",
        "{ITEM}: the LEFT column is the paned's start half"
    );
    assert_eq!(
        end.widget_name(),
        "produce-right",
        "{ITEM}: the RIGHT column is the paned's end half"
    );

    let split_box = bounds(window, "produce-split");
    let left = bounds(window, "produce-left");
    let right = bounds(window, "produce-right");
    // SIDE BY SIDE: same top edge, and the left column ends where the right one begins.
    assert!(
        (left.y() - right.y()).abs() <= 2.0,
        "{ITEM}: the columns are not side by side — left at y={:.1}, right at y={:.1}",
        left.y(),
        right.y()
    );
    assert!(
        left.x() + left.width() <= right.x() + 2.0,
        "{ITEM}: the left column overlaps or follows the right one ({left:?} vs {right:?})"
    );
    // OPENED AT HALF, read BOTH ways: the position the page asked for, and the split the measurement shows.
    // The handle is a few px wide, so the left column's right edge lands just past the halfway mark; both
    // readings are allowed `HALF_TOLERANCE_PX` of that.
    let half = split_box.width() / 2.0;
    println!(
        "SPLIT position={} paned width={:.1} half={:.1} measured boundary={:.1}",
        paned.position(),
        split_box.width(),
        half,
        left.x() + left.width()
    );
    assert!(
        (paned.position() as f32 - half).abs() <= HALF_TOLERANCE_PX,
        "{ITEM}: the divider opens at HALF — position {} against a paned {:.1} px wide (half {:.1})",
        paned.position(),
        split_box.width(),
        half
    );
    assert!(
        ((left.x() + left.width()) - (split_box.x() + half)).abs() <= HALF_TOLERANCE_PX,
        "{ITEM}: the boundary between the columns sits at the paned's middle: left ends {:.1}, paned middle {:.1}",
        left.x() + left.width(),
        split_box.x() + half
    );
    // §A: "Right column (at least 360 px wide)".
    assert!(
        right.width() >= 360.0,
        "{ITEM}: §A gives the written column a 360 px minimum: measured {:.1}",
        right.width()
    );
    RAN_SPLIT.store(true, Ordering::SeqCst);
}

/// The names and headings one column holds, in the order they actually appear top to bottom. Headings are
/// unnamed `gtk::Label`s wearing the `heading` class, so they are picked up by walking the column's own
/// children in order — which is also what proves they belong to THIS column and not its sibling. Two of
/// §1's parts are ROWS rather than widgets with a name ("the Images heading (\"Add image…\" at its right
/// end)"), so a row counts under the named control it carries.
fn column_order(window: &adw::ApplicationWindow, column: &str) -> Vec<String> {
    let mut found: Vec<(f32, String)> = Vec::new();
    let root = widget(window, column);
    let mut cursor = root.first_child();
    while let Some(child) = cursor {
        // A heading is a bare `gtk::Label` wearing the `heading` class; anything else counts under its own
        // name, and a nameless row (the heading-plus-button rows §1 describes) under the first NAMED control
        // inside it. The default GtkLabel name is not a name, so it must be filtered before the class test.
        let named = |node: &gtk::Widget| {
            let name = node.widget_name();
            (!name.is_empty() && !name.starts_with("Gtk")).then(|| name.to_string())
        };
        let entry = if child.has_css_class("heading") {
            child
                .downcast_ref::<gtk::Label>()
                .and_then(|l| l.text().into())
                .map(|t| format!("heading:{t}"))
        } else if child.is::<gtk::Overlay>() {
            // The thumbnail's slot is the plate's overlay: §1 names that entry by the PLATE inside it
            // (`thumbnail-picture`), so the wrapper never shows in the order — its child's name does.
            // It comes BEFORE the named test because the wrapper carries a name of its own.
            let inner = child.first_child().and_then(|c| named(&c));
            Some(inner.unwrap_or_default())
        } else if let Some(name) = named(&child) {
            Some(name)
        } else if child.is::<gtk::ScrolledWindow>() {
            // §A puts the encoder settings in a scroller; that is a part of the column's order, not a wrapper
            // to be seen through.
            Some("scroller".to_string())
        } else {
            // A bare row: named for the first control inside it that has a name. Only a horizontal box is
            // treated as a row — a vertical wrapper (the scroller holding the settings grid) is not one.
            // An `Overlay` counts the same way: the thumbnail's slot is the plate's overlay, and §1 names
            // that entry by the plate inside it (`thumbnail-picture`), not by the wrapper.
            let is_row = child
                .downcast_ref::<gtk::Box>()
                .is_some_and(|b| b.orientation() == gtk::Orientation::Horizontal);
            is_row
                .then(|| first_named_row_child(&child))
                .flatten()
                .map(|inner| format!("row:{inner}"))
        };
        if let Some(entry) = entry {
            let y = child
                .compute_bounds(window.upcast_ref::<gtk::Widget>())
                .map(|b| b.y())
                .unwrap_or(0.0);
            found.push((y, entry));
        }
        cursor = child.next_sibling();
    }
    found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    found.into_iter().map(|(_, entry)| entry).collect()
}

/// The first named descendant of a heading-plus-button row, which is how such a row gets a stable label in
/// the order list (`add-image-button`, `title-suggest`, …).
fn first_named_row_child(node: &gtk::Widget) -> Option<String> {
    if node.widget_name().is_empty() || node.widget_name().starts_with("Gtk") {
        let mut cursor = node.first_child();
        while let Some(child) = cursor {
            if let Some(found) = first_named_row_child(&child) {
                return Some(found);
            }
            cursor = child.next_sibling();
        }
        None
    } else {
        Some(node.widget_name().to_string())
    }
}

fn assert_order(found: &[String], want: &[&str], column: &str) {
    println!("{column} column, top to bottom: {found:?}");
    let positions: Vec<usize> = want
        .iter()
        .map(|part| {
            found
                .iter()
                .position(|entry| entry == part)
                .unwrap_or_else(|| panic!("{ITEM}: {column} column never drew {part}: {found:?}"))
        })
        .collect();
    for index in 1..positions.len() {
        assert!(
            positions[index] > positions[index - 1],
            "{ITEM}: {column} column order is wrong — {} comes before {}: {found:?}",
            want[index - 1],
            want[index]
        );
    }
}

/// A long description scrolls in its box: it must not raise what the page asks for, or the divider above the bottom
/// bar cannot move up.
fn check_a_long_description_keeps_the_page_small(window: &adw::ApplicationWindow) {
    let right = widget(window, "produce-right");
    let view: gtk::TextView = widget(window, "description-box").downcast().expect("a text view");
    let least = right.measure(gtk::Orientation::Vertical, -1).0;
    view.buffer().set_text(&"one line of a long description\n".repeat(200));
    common::settle();
    assert_eq!(right.measure(gtk::Orientation::Vertical, -1).0, least);
    view.buffer().set_text("");
}

/// §1's two lists, top to bottom, plus the two clauses that name a container: the separator BELOW the
/// description box, and the encoder grid inside a scroller.
fn check_both_columns_hold_spec_order(window: &adw::ApplicationWindow) {
    let left = column_order(window, "produce-left");
    assert_order(
        &left,
        &[
            "row:add-image-button",
            "publish-images",
            "heading:Edit instruction",
            "edit-instruction",
            "heading:Negative prompt",
            "negative-prompt",
            "row:thumbnail-export",
            "thumbnail-picture",
        ],
        "left",
    );
    let right = column_order(window, "produce-right");
    assert_order(
        &right,
        &[
            "row:title-suggest",
            "title-entry",
            "heading:YouTube description",
            "description-scroller",
            "produce-written-split",
            "row:transcode-save",
            // The grid is inside its scroller, so it never appears among the column's own children; §A asks
            // for exactly that ("the encoder settings … in a scroller"), and the check below proves it.
            "scroller",
        ],
        "right",
    );
    // The separator sits BELOW the description box, which is what "…, a separator, then the encoder
    // settings" means on screen; the order list above already ranks them, this prints the gap.
    let box_bottom = {
        let b = bounds(window, "description-box");
        b.y() + b.height()
    };
    let separator_top = bounds(window, "produce-written-split").y();
    println!("description box bottom {box_bottom:.1}; separator top {separator_top:.1}");
    assert!(
        separator_top >= box_bottom - 1.0,
        "{ITEM}: the separator is below the YouTube description box: {separator_top:.1} vs {box_bottom:.1}"
    );
    // §A puts the encoder settings "in a scroller": walk up from the grid for a ScrolledWindow.
    let mut node = widget(window, "encoder-settings").parent();
    let mut in_scroller = false;
    while let Some(current) = node {
        if current.is::<gtk::ScrolledWindow>() {
            in_scroller = true;
            break;
        }
        node = current.parent();
    }
    assert!(
        in_scroller,
        "{ITEM}: the encoder settings live inside a ScrolledWindow (§A)"
    );
    RAN_ORDER.store(true, Ordering::SeqCst);
}

/// Every named setting control under `node`, with its measured top and left. A `Grid`'s children ARE the
/// cells, so this walks one level plus whatever a cell itself holds (the Translate row's tick box).
fn collect_controls(
    window: &adw::ApplicationWindow,
    node: &gtk::Widget,
    origin: gtk::graphene::Rect,
    out: &mut Vec<(f32, f32, String)>,
) {
    let name = node.widget_name();
    if name.starts_with("setting-") || name.starts_with("tick-") {
        if let Some(b) = node.compute_bounds(window.upcast_ref::<gtk::Widget>()) {
            out.push((b.y() - origin.y(), b.x() - origin.x(), name.to_string()));
        }
        return;
    }
    let mut cursor = node.first_child();
    while let Some(child) = cursor {
        collect_controls(window, &child, origin, out);
        cursor = child.next_sibling();
    }
}

/// The name a row's CONTROL wears on the page, spelled the way `produce_page` spells it: the three tick
/// rows have their own names (§A words them), every other row follows `setting-<row>`.
fn control_name(row: &str) -> String {
    match row {
        "Frame timing" => "tick-peak-rate-vfr".to_string(),
        "Channels" => "tick-mono".to_string(),
        "Frame edges" => "tick-blurred".to_string(),
        other => ui::produce_page::row_widget(other),
    }
}

/// The drawn encoder controls grouped into LINES: topmost first, left to right within a line. A control
/// joins the current line when its top is within `SAME_LINE_PX` of that line's tallest top.
///
/// Measured against the GRID, not the window: the grid's scroller is `Automatic` on the horizontal axis (the
/// page scrolls as a whole), so absolute x lands far outside the column and only RELATIVE positions mean
/// anything.
fn encoder_lines(window: &adw::ApplicationWindow) -> Vec<Vec<(f32, f32, String)>> {
    let grid = widget(window, "encoder-settings");
    let origin = grid
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .expect("{ITEM}: the encoder grid is laid out");
    println!("grid origin x={:.1} y={:.1}", origin.x(), origin.y());
    let mut placed: Vec<(f32, f32, String)> = Vec::new();
    collect_controls(window, &grid, origin, &mut placed);
    assert!(
        placed.len() >= naivepost::produce_screen::settings_rows_shown().len(),
        "{ITEM}: every settings row draws a named control: {}",
        placed.len()
    );
    placed.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap()
            .then(a.1.partial_cmp(&b.1).unwrap())
    });
    let mut lines: Vec<Vec<(f32, f32, String)>> = Vec::new();
    for control in placed {
        match lines.last_mut() {
            Some(line) => {
                let line_top = line.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max);
                if (control.0 - line_top).abs() <= SAME_LINE_PX {
                    line.push(control);
                } else {
                    lines.push(vec![control]);
                }
            }
            None => lines.push(vec![control]),
        }
    }
    lines
}

/// §A's "label + control pairs … left to right and then down, SEVERAL TO A LINE": print the grouping, and
/// read the lines back as SETTINGS_ROWS.
fn check_encoder_pairs_share_lines(window: &adw::ApplicationWindow) {
    let lines = encoder_lines(window);
    for (index, line) in lines.iter().enumerate() {
        let names: Vec<&str> = line.iter().map(|c| c.2.as_str()).collect();
        let ys: Vec<f32> = line.iter().map(|c| c.0).collect();
        let xs: Vec<f32> = line.iter().map(|c| c.1).collect();
        println!("encoder line {}: {names:?} y={ys:?} x={xs:?}", index + 1);
    }
    // SEVERAL TO A LINE: at least one line carries more than one pair. One-per-line would fail here.
    let widest = lines.iter().map(Vec::len).max().unwrap_or(0);
    assert!(
        widest > 1,
        "{ITEM}: §A lays the settings \"several to a line\"; every control sits on a line of its own: {:?}",
        lines.iter().map(|l| l.len()).collect::<Vec<_>>()
    );
    // THREE PAIRS TO A LINE is what `spec/img/08-produce.png` draws; a fourth would mean the budget leaked.
    assert!(
        widest <= 3,
        "{ITEM}: no encoder line holds more than three pairs: {widest} in {:?}",
        lines.iter().map(|l| l.len()).collect::<Vec<_>>()
    );
    // LEFT TO RIGHT AND THEN DOWN: reading the lines in order reproduces §A's row order exactly.
    let read_back: Vec<String> = lines
        .iter()
        .flat_map(|l| l.iter().map(|c| c.2.clone()))
        .collect();
    println!("read back: {read_back:?}");
    let expected: Vec<String> = naivepost::produce_screen::settings_rows_shown()
        .iter()
        .map(|row| control_name(row))
        .collect();
    assert_eq!(
        read_back, expected,
        "{ITEM}: the encoder grid does not read as §A's rows, left to right and then down"
    );
    RAN_PAIRS.store(true, Ordering::SeqCst);
}

/// The picture's grouping, line by line: `spec/img/08-produce.png` draws THREE label+control pairs on its
/// first two lines and TWO on each line after. That is the arrangement `produce_page_layout` must produce,
/// and the only reading of "several to a line" that matches the drawing — a grid that puts two pairs per row
/// passes "several" but is not this page.
///
/// shot shows five lines; this fixture has narration ON, which
/// adds the Game audio slider (§F4.8 hides it, never the value), making six. So the Game-audio line is
/// checked by POSITION (after Subtitles, before Quality) rather than by pinning who shares its row.
fn check_encoder_lines_match_the_picture(window: &adw::ApplicationWindow) {
    let lines = encoder_lines(window);
    let names: Vec<Vec<&str>> = lines
        .iter()
        .map(|line| line.iter().map(|c| c.2.as_str()).collect())
        .collect();
    for (index, line) in names.iter().enumerate() {
        println!("picture line {}: {line:?}", index + 1);
    }
    let want = |index: usize, expected: &[&str]| {
        assert_eq!(
            names.get(index).map(Vec::as_slice),
            Some(expected),
            "{ITEM}: §1's picture line {} is {expected:?} — every line read as: {names:?}",
            index + 1
        );
    };
    // Items 11: three dropdown pairs across the first line, then Resolution / Frame rate / Audio.
    want(0, &["setting-container", "setting-codec", "setting-preset"]);
    want(
        1,
        &["setting-resolution", "setting-frame-rate", "setting-audio"],
    );
    // Item 12: Subtitles beside Translate — the widest control on the line, so no third pair fits.
    want(2, &["setting-subtitles", "setting-translate"]);
    // No line holds a fourth pair: that would mean the slot budget leaked.
    for (index, line) in names.iter().enumerate() {
        assert!(
            line.len() <= 3,
            "{ITEM}: picture line {} holds {} pairs: {line:?}",
            index + 1,
            line.len()
        );
    }
    // Item 13 beside item 14: the CRF slider shares its line with Frame timing, exactly as the picture
    // draws them, and Channels shares one with Frame edges.
    let shares = |a: &str, b: &str| {
        names
            .iter()
            .any(|line| line.contains(&a) && line.contains(&b))
    };
    assert!(
        shares("setting-quality--crf", "tick-peak-rate-vfr"),
        "{ITEM}: Quality (CRF) and Frame timing are on one line in the picture; lines read {names:?}"
    );
    assert!(
        shares("tick-mono", "tick-blurred"),
        "{ITEM}: Channels and Frame edges share a line in the picture; lines read {names:?}"
    );
    // The Game-audio line sits where §A's order puts it — after Subtitles, before Quality — without
    // claiming a grouping the picture cannot show (it has no Game audio at all).
    let line_of = |name: &str| names.iter().position(|line| line.contains(&name));
    let subtitles = line_of("setting-subtitles").expect("{ITEM}: Subtitles drew a line");
    let quality = line_of("setting-quality--crf").expect("{ITEM}: Quality drew a line");
    if naivepost::produce_screen::settings_rows_shown().contains(&"Game audio") {
        let game =
            line_of("setting-game-audio").expect("{ITEM}: narration on means a Game audio line");
        assert!(
            game > subtitles && game < quality,
            "{ITEM}: the Game audio row belongs between Subtitles and Quality, got line {game} against \
             subtitles {subtitles} and quality {quality}: {names:?}"
        );
    }
    RAN_PICTURE.store(true, Ordering::SeqCst);
}

/// §A's default for one dropdown row, spelled from the same table the page seeds its pickers from.
fn screen_default(row: &str) -> String {
    naivepost::produce_screen::defaults()
        .iter()
        .find(|(name, _)| *name == row)
        .map(|(_, value)| value.to_string())
        .unwrap_or_default()
}

/// Check 5 measures the SHOT's own staging: the page `just snapshot 08-produce` paints is built from the
/// state `snapshot_produce::paint` publishes (7 slots' worth of settings at §A's defaults, narration ON),
/// not from the bare fixture state, so the last window of the round is published like the shot's is before
/// anything is measured off it. Nothing here needs the shot's file paths, only its settings.
fn shot_like_state() -> ui::produce_page::ProduceState {
    let mut state = ui::produce_page::read_state();
    for (row, value) in naivepost::produce_screen::defaults() {
        match row {
            "Container" => state.container = value.to_string(),
            "Codec" => state.codec = value.to_string(),
            "Preset" => state.preset = value.to_string(),
            "Resolution" => state.resolution = value.to_string(),
            "Frame rate" => state.frame_rate = value.to_string(),
            "Audio" => state.audio = value.to_string(),
            "Subtitles" => state.subtitles = value.to_string(),
            _ => (),
        }
    }
    state.game_volume = 0.22;
    state.crf = 24.0;
    state.blurred_edges = true;
    state
}

/// Rough width of one character in the page's font, measured from the labels this grid draws ("Quality
/// (CRF):" is 13 chars in the ~78 px `LABEL_PX` allows). Enough to catch a dropdown cut to a third of what
/// it shows; not a measurement, which is what the failure message says.
const CHOICE_PX_PER_CHAR: f64 = 6.0;

/// §A's longest choice, spelled once so a check and a comment agree about what the dropdowns must show.
const WIDEST_CHOICE: &str = "none in the video";

/// What the setting's DROPDOWN shows, read off the widget the grid drew: its chosen item and its measured
/// width. This is the check the rendered `shots/08-produce.png` cannot give a headless run — the window shot
/// shows the whole page, the settings sit at its bottom edge, and codehalter hands the same PNG back for any
/// re-render — so the readability of the three-pairs-to-a-line grid is measured where it can be.
///
/// §A's longest value is "none in the video"; a dropdown squeezed under that prints "none in the vid…", which
/// is a row a person cannot read and a grid that is too wide for its column wearing it out.
fn check_shot_settings_are_readable(window: &adw::ApplicationWindow) {
    ui::produce_page::set_state(shot_like_state());
    ui::produce_page::refresh(window);
    common::settle();
    let right = bounds(window, "produce-right");
    let right_end = right.x() + right.width();
    // §A's longest value is "none in the video" and the shot's state holds it, so the Subtitles dropdown is
    // the widest row in the grid and the first to ellipsize at three pairs to a line.
    let subtitles_box = bounds(window, &ui::produce_page::row_widget("Subtitles"));
    let need = WIDEST_CHOICE.len() as f64 * CHOICE_PX_PER_CHAR;
    println!(
        "READ widest choice {WIDEST_CHOICE:?}: dropdown {:.1} px, needs about {need:.0} px",
        subtitles_box.width()
    );
    assert!(
        subtitles_box.width() as f64 >= need,
        "{ITEM}: {WIDEST_CHOICE:?} would be cut short — the Subtitles dropdown is {:.1} px and reads at \
         about {need:.0} px ({CHOICE_PX_PER_CHAR} px per character is an estimate, not a measurement)",
        subtitles_box.width()
    );
    for row in [
        "Container",
        "Codec",
        "Preset",
        "Resolution",
        "Frame rate",
        "Audio",
        "Subtitles",
    ] {
        let name = ui::produce_page::row_widget(row);
        let picker = widget(window, &name)
            .downcast::<gtk::DropDown>()
            .unwrap_or_else(|_| panic!("{ITEM}: {name} is not a dropdown"));
        let shown = picker
            .model()
            .and_then(|model| {
                model
                    .downcast::<gtk::StringList>()
                    .ok()
                    .and_then(|list| list.string(picker.selected()))
            })
            .map(|text| text.to_string())
            .unwrap_or_default();
        let box_ = bounds(window, &name);
        // What the row must show is the choice §A's defaults hold — nothing ellipsized, nothing cut.
        println!("READ {row}: shows {shown:?} at {:.1} px", box_.width());
        assert!(
            box_.x() >= right.x() - 1.0 && box_.x() + box_.width() <= right_end + 1.0,
            "{ITEM}: {row}'s dropdown is outside the right column: {box_:?} against {right:?}"
        );
        // The item's own width at the page's font, asked of the label GTK draws inside the dropdown.
        let want = picker
            .first_child()
            .and_then(|child| child.first_child())
            .map(|label| label.measure(gtk::Orientation::Horizontal, -1).1)
            .unwrap_or(0);
        assert!(
            shown == screen_default(row) || shown.is_empty() || want as f32 <= box_.width() + 1.0,
            "{ITEM}: {row}'s dropdown prints {shown:?} cut short — it needs {want} px and has {:.1}",
            box_.width()
        );
    }
    RAN_READABLE.store(true, Ordering::SeqCst);
}

/// The cells of the encoder settings, in attach order: `encoder-settings` holds one grid for the three-pair
/// lines and one for the two-pair lines (§1: "three on each of the first two lines, two on each line after
/// them"), and a grid's own children ARE its cells.
fn grid_cells(settings: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut cells = Vec::new();
    let mut grid = settings.first_child();
    while let Some(g) = grid {
        let mut cell = g.first_child();
        while let Some(c) = cell {
            cell = c.next_sibling();
            cells.push(c);
        }
        grid = g.next_sibling();
    }
    cells
}

/// One cell of the grid: what it is, and where it sits inside `produce-right`'s own x-range.
fn collect_cells(
    window: &adw::ApplicationWindow,
    node: &gtk::Widget,
    out: &mut Vec<(f32, f32, String)>,
) {
    let name = node.widget_name();
    if name.starts_with("setting-") || name.starts_with("tick-") {
        if let Some(b) = node.compute_bounds(window.upcast_ref::<gtk::Widget>()) {
            out.push((b.x(), b.width(), name.to_string()));
        }
        return;
    }
    let mut cursor = node.first_child();
    while let Some(child) = cursor {
        collect_cells(window, &child, out);
        cursor = child.next_sibling();
    }
}

/// §A's other sentence about the encoder settings: "each label sits directly left of its own control, and no
/// label or control overlaps another". The pairing half is already pinned by [`check_encoder_rows_fit_the_column`]
/// and the grouping by [`check_encoder_lines_match_the_picture`]; what neither looks at is two DIFFERENT pairs on
/// one line — a wide control (the Subtitles dropdown has to be as wide as "none in the video") reaching into the
/// cell the next pair's label was placed in. So: every grid cell that takes part in a pair (the row labels plus
/// every `setting-`/`tick-` control), grouped into lines by the same `SAME_LINE_PX` rule the other checks use,
/// and no two of them overlapping in x.
fn check_no_encoder_control_overlaps_another(window: &adw::ApplicationWindow) {
    let grid = widget(window, "encoder-settings");
    // (top, left, right, name) for every cell of a pair. A `Grid`'s children ARE the cells, so this walk is one
    // level deep (the Translate menu is one control, its ticks in its popover).
    let mut cells: Vec<(f32, f32, f32, String)> = Vec::new();
    // The grids' OWN children are the cells — one level, no descent into a cell (the Translate menu is one
    // control, and walking into its popover would add labels that belong to nothing).
    for cell in grid_cells(&grid) {
        let Some(b) = cell.compute_bounds(window.upcast_ref::<gtk::Widget>()) else {
            continue;
        };
        let name = cell.widget_name();
        let label = cell.downcast_ref::<gtk::Label>();
        let shown = label.map(|l| l.text().to_string()).unwrap_or_default();
        // A row's label is a `GtkLabel` whose text ends in ':' (§A words every row "Container:", "Frame
        // timing:", …). Found by type and text, not by an empty name: GTK 4 answers an unnamed widget's name
        // with its class name ("GtkLabel"), so an empty-name test found no label at all.
        let is_control = name.starts_with("setting-") || name.starts_with("tick-");
        let is_label = !is_control && label.is_some() && shown.ends_with(':');
        if !(is_control || is_label) {
            continue;
        }
        cells.push((
            b.y(),
            b.x(),
            b.x() + b.width(),
            if is_label {
                format!("label {shown:?}")
            } else {
                name.to_string()
            },
        ));
    }
    // One control per §A row plus at least one label per row (the grid also carries the two headings that share
    // a line with a pair, so labels are counted as they are found, not assumed).
    let controls = cells
        .iter()
        .filter(|c| c.3.starts_with("setting-") || c.3.starts_with("tick-"))
        .count();
    let labels = cells.len() - controls;
    assert!(
        controls >= naivepost::produce_screen::settings_rows_shown().len() && labels >= 2,
        "{ITEM}: every settings row draws a named control and its own label: {controls} controls, {labels} \
         labels in {} cells",
        cells.len()
    );
    // Lines, topmost first, by the file's own rule: a cell joins the current line when its top is within
    // `SAME_LINE_PX` of that line's highest top.
    let mut sorted = cells.clone();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut lines: Vec<Vec<(f32, f32, f32, String)>> = Vec::new();
    for cell in sorted {
        match lines.last_mut() {
            Some(line) => {
                let line_top = line.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max);
                if (cell.0 - line_top).abs() <= SAME_LINE_PX {
                    line.push(cell);
                } else {
                    lines.push(vec![cell]);
                }
            }
            None => lines.push(vec![cell]),
        }
    }
    for (index, line) in lines.iter().enumerate() {
        let names: Vec<&str> = line.iter().map(|c| c.3.as_str()).collect();
        println!("OVERLAP line {}: {names:?}", index + 1);
        for (i, a) in line.iter().enumerate() {
            for b in line.iter().skip(i + 1) {
                let clear = a.2 <= b.1 + OVERLAP_TOLERANCE_PX || b.2 <= a.1 + OVERLAP_TOLERANCE_PX;
                assert!(
                    clear,
                    "{ITEM}: {} and {} overlap on one encoder line: {} spans {:.1}..{:.1} at y={:.1}, \
                     {} spans {:.1}..{:.1} at y={:.1}",
                    a.3,
                    b.3,
                    a.3,
                    a.1,
                    a.2,
                    a.0,
                    b.3,
                    b.1,
                    b.2,
                    b.0
                );
            }
        }
    }
    RAN_NO_OVERLAP.store(true, Ordering::SeqCst);
}

/// §A's settings must FIT their column. The grid used to ask for 760 px of width inside a ~596 px half,
/// so the last items were cut at the right edge ("…Codec: h264 …P…", "Quality (CRF):" with no slider, and
/// no Preset row at all) and the Title ↻ sat off-screen too. Every drawn control now has to land inside
/// `produce-right`, which is the only reading of "several to a line" that a person can actually read.
fn check_encoder_rows_fit_the_column(window: &adw::ApplicationWindow) {
    let right = bounds(window, "produce-right");
    let right_end = right.x() + right.width();
    let grid = widget(window, "encoder-settings");
    let mut cells: Vec<(f32, f32, String)> = Vec::new();
    collect_cells(window, &grid, &mut cells);
    assert!(
        cells.len() >= naivepost::produce_screen::settings_rows_shown().len(),
        "{ITEM}: every settings row draws a named control: {}",
        cells.len()
    );
    // Each LABEL sits directly left of its OWN control (§1). The grid's children come out in attach order —
    // label, control, label, control — so every EVEN cell is a label and the cell after it is its control:
    // that control must start at or after the label's right edge (a label whose control starts to its LEFT
    // is one pair sitting on top of another).
    let mut pairs: Vec<(f32, f32, f32, String)> = Vec::new();
    for cell in grid_cells(&grid) {
        let Some(b) = cell.compute_bounds(window.upcast_ref::<gtk::Widget>()) else {
            continue;
        };
        pairs.push((
            b.y(),
            b.x(),
            b.x() + b.width(),
            cell.widget_name().to_string(),
        ));
    }
    // Attach order, NOT sorted by x: the pairs are attached label-then-control, which is the only order
    // that says which control belongs to which label.
    for i in (0..pairs.len()).step_by(2) {
        let Some((ly, lx, ledges, lname)) = pairs.get(i) else {
            continue;
        };
        let Some(next) = pairs.get(i + 1) else { break };
        if (next.0 - ly).abs() > SAME_LINE_PX {
            continue; // a lone label on its line: nothing of its own to compare against
        }
        assert!(
            next.1 >= ledges - 1.0,
            "{ITEM}: the label {lname} overlaps its control {}: it spans {lx:.1}..{ledges:.1}, the \
             control starts {:.1}",
            next.3, next.1
        );
    }
    for (x, width, name) in &cells {
        println!(
            "FIT {name} x={x:.1} right edge={:.1} (column ends {:.1})",
            x + width,
            right_end
        );
        assert!(
            *x >= right.x() - 1.0 && x + width <= right_end + 1.0,
            "{ITEM}: {name} is cut at the column's edge: {}..{} against {right:?}",
            x,
            x + width
        );
    }
    // The two rows the clipped shot lost entirely, named rather than counted: Preset was never drawn, and
    // the Title ↻ sat past the edge so item 8 read as a bare heading.
    for name in [
        ui::produce_page::row_widget("Preset"),
        "title-suggest".to_string(),
    ] {
        let b = bounds(window, &name);
        assert!(b.width() > 0.0, "{ITEM}: {name} draws nothing");
        assert!(
            b.x() >= right.x() - 1.0 && b.x() + b.width() <= right_end + 1.0,
            "{ITEM}: {name} sits outside the right column: {b:?} against {right:?}"
        );
    }
    RAN_FITS.store(true, Ordering::SeqCst);
}

/// §08-produce#1-screen §1, measured. ONE application for the whole file: GTK and libadwaita bind to the
/// first thread that initialises them and cargo runs a binary's tests on different threads, so all three
/// checks are stages of one `connect_activate` (`common::one_app_round`, whose `Once` makes the second and
/// third call free). Each check closes its own window; `run_with_args` returns when the last one goes.
fn layout_round() {
    common::one_app_round(|app| {
        let window = produce_window(app);
        check_the_page_is_split_in_two(&window);
        window.close();
        common::settle();

        let window = produce_window(app);
        check_both_columns_hold_spec_order(&window);
        check_a_long_description_keeps_the_page_small(&window);
        window.close();
        common::settle();

        let window = produce_window(app);
        check_encoder_pairs_share_lines(&window);
        window.close();
        common::settle();

        let window = produce_window(app);
        check_encoder_rows_fit_the_column(&window);
        window.close();
        common::settle();

        let window = produce_window(app);
        check_no_encoder_control_overlaps_another(&window);
        window.close();
        common::settle();

        let window = produce_window(app);
        check_encoder_lines_match_the_picture(&window);
        window.close();
        common::settle();

        let window = produce_window(app);
        ui::produce_page::hold_window_for_test(&window);
        check_shot_settings_are_readable(&window);
        window.close();
        common::settle();
    });
}

#[test]
fn sec_08_produce_1_screen_s1_the_page_is_split_in_two_side_by_side() {
    common::pin_private_cwd();
    layout_round();
    assert!(
        RAN_SPLIT.load(Ordering::SeqCst),
        "{ITEM} s1: the split check never ran"
    );
}

#[test]
fn sec_08_produce_1_screen_s1_both_columns_hold_spec_order_top_to_bottom() {
    common::pin_private_cwd();
    layout_round();
    assert!(
        RAN_ORDER.load(Ordering::SeqCst),
        "{ITEM} s1: the column-order check never ran"
    );
}

#[test]
fn sec_08_produce_1_screen_s1_encoder_pairs_share_lines_in_order() {
    common::pin_private_cwd();
    layout_round();
    assert!(
        RAN_PAIRS.load(Ordering::SeqCst),
        "{ITEM} s1: the encoder-pairs check never ran"
    );
}

/// §A's "label + control pairs … several to a line" is only worth anything if the line fits the column.
#[test]
fn sec_08_produce_1_screen_s11_the_encoder_rows_fit_their_column() {
    common::pin_private_cwd();
    layout_round();
    assert!(
        RAN_FITS.load(Ordering::SeqCst),
        "{ITEM} s11: the encoder-fit check never ran"
    );
}

/// §1 items 11–14 as `spec/img/08-produce.png` draws them: three pairs on the first two lines, two after.
#[test]
fn sec_08_produce_1_screen_s1_encoder_lines_match_the_spec_picture() {
    common::pin_private_cwd();
    layout_round();
    assert!(
        RAN_PICTURE.load(Ordering::SeqCst),
        "{ITEM} s1: the picture-grouping check never ran"
    );
}

/// Three pairs to a line only counts if each pair still READS: every dropdown shows its whole value and
/// stays inside the column.
#[test]
fn sec_08_produce_1_screen_s1_encoder_settings_stay_readable_at_three_to_a_line() {
    common::pin_private_cwd();
    layout_round();
    assert!(
        RAN_READABLE.load(Ordering::SeqCst),
        "{ITEM} s1: the readability check never ran"
    );
}

/// §A: "each label sits directly left of its own control, and no label or control overlaps another" — the half
/// about TWO PAIRS sharing a line, which no other check in this file looks at.
#[test]
fn sec_08_produce_1_screen_s1_no_encoder_control_overlaps_another() {
    common::pin_private_cwd();
    layout_round();
    assert!(
        RAN_NO_OVERLAP.load(Ordering::SeqCst),
        "{ITEM} s1: the overlap check never ran"
    );
}
