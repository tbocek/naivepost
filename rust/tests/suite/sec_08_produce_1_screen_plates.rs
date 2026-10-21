//! §08-produce#1-screen, numbers **2** and **7**: the Produce page's painted plates.
//!
//! Two things are pinned here that no other file of this item touches. Where a word box goes on the
//! thumbnail is arithmetic (`ui::produce_paint::layout_marks`), and it is arithmetic a wrong round can
//! get silently wrong: the title band printed over every mark, or a mark near an edge drawn half off the
//! picture. Those are checked as plain numbers, no window. Then the dashed box's PLACE is checked in a
//! real tree, because §A says the box is ON the picture while §1 pins the left column's child order —
//! so the overlay that makes both true may only appear while a mark exists.
//!
//! The widget round follows `produce_screen_widgets.rs`: one application, one `Application::run`, cwd
//! pinned first (the page resolves its session through it), each check closing its window before the next
//! is built, and `settle()` rather than any sleep.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::TextMark;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::produce_paint::layout_marks;

use crate::common;

const ITEM: &str = "sec_08_produce_1_screen";
const TOLERANCE: f64 = 0.01;

static RAN_BAND: AtomicBool = AtomicBool::new(false);
static RAN_MARK: AtomicBool = AtomicBool::new(false);
static RAN_ON_PICTURE: AtomicBool = AtomicBool::new(false);
static RAN_NO_WRAPPER: AtomicBool = AtomicBool::new(false);
static RAN_ALLOCATION: AtomicBool = AtomicBool::new(false);

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < TOLERANCE
}

fn mark(cx: f64, cy: f64, wf: f64, hf: f64) -> TextMark {
    TextMark { cx, cy, wf, hf, text: "words".into() }
}

/// The title band: one box across the top, inset from both edges, and NOTHING when there is no title to print.
#[test]
fn sec_08_produce_1_screen_s7_the_title_band_is_one_inset_box_only_while_a_title_prints() {
    let (w, h) = (400.0, 200.0);
    let boxes = layout_marks(&[], "a title", true, w, h);
    assert_eq!(boxes.len(), 1, "{ITEM} s7: a title prints exactly one band");
    let (x, y, bw, bh, text) = boxes[0].clone();
    assert!(near(y, h * 0.06), "{ITEM} s7: band top {} != {}", y, h * 0.06);
    assert!(near(bw, w - 2.0 * (w * 0.06)), "{ITEM} s7: band width {bw} is not inset on both sides");
    assert!(bh > 0.0 && x > 0.0, "{ITEM} s7: band {x},{y} {bw}x{bh} is not a box inside the plate");
    assert_eq!(text, "a title", "{ITEM} s7: the band must carry the title itself");

    assert!(layout_marks(&[], "a title", false, w, h).is_empty(),
        "{ITEM} s7: a retitle must not repaint the picture, so no band once the title has printed");
    assert!(layout_marks(&[], "", true, w, h).is_empty(),
        "{ITEM} s7: an empty title prints no band");
    RAN_BAND.store(true, Ordering::SeqCst);
}

/// A mark's box keeps its size and its centre, and a box hanging off the plate is pulled back inside it.
#[test]
fn sec_08_produce_1_screen_s7_a_mark_keeps_its_centre_and_stays_inside_the_plate() {
    let (w, h) = (400.0, 200.0);
    let boxes = layout_marks(std::slice::from_ref(&mark(0.5, 0.25, 0.5, 0.2)), "", false, w, h);
    assert_eq!(boxes.len(), 1, "{ITEM} s7: one mark, one box");
    let (x, y, bw, bh, _) = boxes[0];
    assert!(near(bw, 200.0) && near(bh, 40.0), "{ITEM} s7: box {bw}x{bh} is not wf/hf of the plate");
    assert!(near(x + bw / 2.0, 200.0), "{ITEM} s7: box centre x {} != cx*w", x + bw / 2.0);
    assert!(near(y + bh / 2.0, 50.0), "{ITEM} s7: box centre y {} != cy*h", y + bh / 2.0);

    let hung = layout_marks(std::slice::from_ref(&mark(0.95, 0.5, 0.5, 0.2)), "", false, w, h)[0].clone();
    assert!(hung.0 >= 0.0 && hung.0 + hung.2 <= w, "{ITEM} s7: a mark at the edge drew off the plate: {hung:?}");
    RAN_MARK.store(true, Ordering::SeqCst);
}

fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    common::release_last_window();
    let model = naivepost::project::load(&common::fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    // The page's own root slot must point at THIS window: `refresh` repaints the window it finds there,
    // and an earlier check's closed window would get the marks instead of the one being measured.
    ui::produce_page::hold_window_for_test(&window);
    common::hold_last_window(window.clone());
    window.present();
    common::settle();
    window
}

fn widget(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::produce_page::widget_in(window, name).unwrap_or_else(|| panic!("{ITEM}: no widget {name}"))
}

fn publish(state: ui::produce_page::ProduceState, window: &adw::ApplicationWindow) {
    ui::produce_page::set_state(state);
    ui::produce_page::refresh(window);
    common::settle();
}

fn with_mark(texts: Vec<TextMark>, frames: Vec<String>) -> ui::produce_page::ProduceState {
    ui::produce_page::ProduceState { texts, frames, ..Default::default() }
}

fn plate_round() {
    common::pin_private_cwd();
    // `one_app_round` runs its checks inside ONE `Application::run`, so only the first test that asks
    // gets a window: every later call returns with its closure never called. The rounds therefore live
    // in one shared pass, each behind its own flag — which is also what makes them run in this file's order.
    ROUNDS.with(|done| {
        if done.get() {
            return;
        }
        done.set(true);
        common::one_app_round(|app| {
            on_picture(&produce_window(app));
            no_wrapper(&produce_window(app));
            allocation(&produce_window(app));
        });
    });
}

thread_local! {
    static ROUNDS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[test]
fn sec_08_produce_1_screen_s7_the_dashed_box_lives_on_the_picture() {
    plate_round();
    assert!(RAN_ON_PICTURE.load(Ordering::SeqCst), "{ITEM} s7: the on-picture check never ran");
}

#[test]
fn sec_08_produce_1_screen_s7_no_dashed_box_while_no_mark_exists() {
    plate_round();
    assert!(RAN_NO_WRAPPER.load(Ordering::SeqCst), "{ITEM} s7: the no-mark check never ran");
}

#[test]
fn sec_08_produce_1_screen_s2_both_plates_are_laid_out_to_be_painted() {
    plate_round();
    assert!(RAN_ALLOCATION.load(Ordering::SeqCst), "{ITEM} s2: the allocation check never ran");
}

fn on_picture(window: &adw::ApplicationWindow) {
    publish(with_mark(vec![mark(0.5, 0.25, 0.5, 0.2)], vec![]), window);
    // `place_overlay_box` finds its names through the page's root slot, which is THIS window (what `wire`
    // published), so the walk starts at the box in this window's tree.
    let root = ui::produce_page::page_root().unwrap_or_else(|| panic!("{ITEM} s7: no page root"));
    let box_on_picture = find_in(&root, "words-box-0")
        .unwrap_or_else(|| panic!("{ITEM} s7: publishing one mark drew no ✎ box under the page root"));
    // "Over the picture" is proved by climbing from the box: its parent is the float holder, and THAT
    // holder's parent is the overlay that also holds the plate — so the plate and the box share a parent.
    let holder = box_on_picture.parent().expect("{ITEM} s7: the ✎ box has no parent");
    let overlay = holder
        .parent()
        .unwrap_or_else(|| panic!("{ITEM} s7: the word-box holder floats in nothing"));
    let drawn = overlay
        .downcast_ref::<gtk::Overlay>()
        .and_then(|o| o.child())
        .unwrap_or_else(|| panic!("{ITEM} s7: {} holds no picture", overlay.widget_name()));
    assert_eq!(drawn.widget_name(), "thumbnail-picture",
        "{ITEM} s7: the ✎ box is over {}, not over the picture", drawn.widget_name());
    assert!(drawn.has_css_class("produce-plate"),
        "{ITEM} s7: what the box floats over is not the painted plate");
    RAN_ON_PICTURE.store(true, Ordering::SeqCst);
    window.close();
    common::settle();
}

fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut cursor = node.first_child();
    while let Some(current) = cursor {
        if let Some(found) = find_in(&current, name) {
            return Some(found);
        }
        cursor = current.next_sibling();
    }
    None
}


fn no_wrapper(window: &adw::ApplicationWindow) {
    publish(with_mark(vec![], vec![]), window);
    // §1's clause this pins is the left column's ORDER — the thumbnail sits where §1 puts it. §A's float
    // holder is always there (a `DrawingArea` can hold no children at all); what must be absent with no
    // marks is any dashed box claiming one.
    let plate = widget(window, "thumbnail-picture");
    // The plate's parent is the overlay; the holder is the overlay's other child, beside the plate.
    let overlay = plate
        .parent()
        .unwrap_or_else(|| panic!("{ITEM} s7: the plate is in no overlay"));
    let mut holder = None;
    let mut cursor = overlay.first_child();
    while let Some(child) = cursor {
        if child.widget_name() == "thumbnail-word-boxes" {
            holder = Some(child);
            break;
        }
        cursor = child.next_sibling();
    }
    let holder = holder.expect("{ITEM} s7: the plate's overlay has no word-box holder");
    // The column still calls the thumbnail's slot by the plate's name, which is what §1's order list reads.
    assert_eq!(column_entry_of(window), "thumbnail-picture",
        "{ITEM} s7: the left column's thumbnail slot is {}", column_entry_of(window));
    let mut boxes = 0;
    let mut cursor = holder.first_child();
    while let Some(child) = cursor {
        if child.widget_name().starts_with("words-box-") {
            boxes += 1;
        }
        cursor = child.next_sibling();
    }
    assert_eq!(boxes, 0, "{ITEM} s7: a removed mark left its dashed box on the picture");
    RAN_NO_WRAPPER.store(true, Ordering::SeqCst);
    window.close();
    common::settle();
}

/// What the left column calls the thumbnail's slot: the order list §1's other check reads it by the name
/// of the first NAMED control inside the slot, which is the plate inside its overlay.
fn column_entry_of(window: &adw::ApplicationWindow) -> String {
    let column = widget(window, "produce-left");
    let plate = widget(window, "thumbnail-picture");
    let slot = plate
        .parent()
        .unwrap_or_else(|| panic!("{ITEM} s7: the plate is in no slot"));
    if slot.parent().as_ref() == Some(&column) && !plate.widget_name().is_empty() {
        return plate.widget_name().to_string();
    }
    String::new()
}

fn allocation(window: &adw::ApplicationWindow) {
    let frames = (1..=3).map(|n| format!("project:produce/publish/frame-{n}.jpg")).collect();
    publish(with_mark(vec![], frames), window);
    for name in ["thumbnail-picture", "image-picture-0"] {
        let plate = widget(window, name);
        let wide = plate
            .compute_bounds(window.upcast_ref::<gtk::Widget>())
            .map(|b| b.width())
            .unwrap_or(0.0);
        assert!(wide > 0.0, "{ITEM} s2: {name} was never allocated, so nothing paints on it");
    }
    RAN_ALLOCATION.store(true, Ordering::SeqCst);
    window.close();
    common::settle();
}
