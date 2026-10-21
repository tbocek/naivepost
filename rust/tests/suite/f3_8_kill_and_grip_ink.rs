// F3.8 S1/S3 — the two press affordances a lane bar has to SHOW, not only answer.
//
// §A.9 states both against the bar's DRAWN width: end grips when it is ≥ `fx_band::GRIP_MIN_PX` (30) and the ✕
// in its MIDDLE when ≥ `fx_band::KILL_MIN_PX` (32). The rules were answered on press all along; what was missing
// was the ink — `cut_lane_paint::paint_bar` drew a wash, an edge, a mark and a caption and no cross at all, so
// `shots/06-lane.png` (which is painted by that same door through `cut_tracks`) showed bars with nothing to
// press, and in the widget lane the ✕ button sat AFTER the full-width ink in a horizontal box, so GTK laid it
// out past the bar's right edge instead of over its middle.
//
// So these checks read PIXELS off the shared painter (`painted_surface_for_test`, the door built for exactly
// this) and, in the wire check, measure where the kill button actually lands inside its bar. One application,
// one entry `#[test]`, no sleeps — the shape `band_hold_widgets.rs` uses.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, EffectKind, Fx, Seg};
use naivepost::fx_band;
use naivepost::fx_lane;
use naivepost::ui;

use crate::common;
use common::{release_last_window, settle, status_text, widget_in};

/// A row tall, the height the lane lays every bar at.
const ROW_H: f64 = 26.0;

static RAN_WIRE: AtomicBool = AtomicBool::new(false);
static STOP_REASON: LazyLock<std::sync::Arc<std::sync::Mutex<Option<String>>>> =
    LazyLock::new(|| std::sync::Arc::new(std::sync::Mutex::new(None)));

macro_rules! step {
    ($which:expr, $body:expr) => {{
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)).is_err() {
            *STOP_REASON.lock().unwrap() = Some(format!("{which} panicked", which = $which));
            return;
        }
    }};
}

fn stopped_early() -> Option<String> {
    STOP_REASON.lock().unwrap().clone()
}

/// `(red, green, blue)` of the pixel at `(x, y)` of a painted surface.
fn rgb(data: &[u8], width: i32, x: i32, y: i32) -> (u8, u8, u8) {
    let start = ((y * width + x) * 4) as usize;
    // cairo ARGB32 = BGRA bytes on little-endian; alpha is ignored here because every mark checked is opaque.
    (data[start + 2], data[start + 1], data[start])
}

/// The darkest ink in a painted surface (the bar's own edge and grip ticks are `colour × 0.6`, so this finds them).
fn min_channel_sum(data: &[u8], width: i32, row: i32, from: i32, to: i32) -> u32 {
    (from.max(0)..to.min(width))
        .map(|x| {
            let (r, g, b) = rgb(data, width, x, row);
            r as u32 + g as u32 + b as u32
        })
        .min()
        .unwrap_or(u32::MAX)
}

/// Paint one view and hand back its pixels plus its width, so a check can look at rows without repeating the
/// surface dance.
fn paint(view: &naivepost::ui::cut_lane_paint::BarView) -> (Vec<u8>, i32, i32) {
    let surface = naivepost::ui::cut_lane_paint::painted_surface_for_test(view).expect("a surface");
    let width = surface.width();
    let height = surface.height();
    let data = surface.take_data().expect("the pixels are readable");
    (data.to_vec(), width, height)
}

fn whiteish((r, g, b): (u8, u8, u8)) -> bool {
    r >= 220 && g >= 220 && b >= 220
}

/// Is any pixel in this x-range of this row white? Used for "the cross is there" / "it is not".
fn any_white_in(data: &[u8], width: i32, row: i32, from: i32, to: i32) -> bool {
    (from.max(0)..to.min(width)).any(|x| whiteish(rgb(data, width, x, row)))
}

/// The bar's own mark glyph, asked of the same door the painter uses so a check never hardcodes "⊕" and drifts
/// when the marks change: the mark is white ink too, and naming it says WHERE the white pixels a narrow bar is
/// allowed to carry actually are.
fn mark_of_zoom() -> String {
    naivepost::ui::cut_lane_paint::mark_of(&Fx { kind: "zoom".into(), ..Default::default() }).to_string()
}

fn zoom_view(width: f64) -> naivepost::ui::cut_lane_paint::BarView {
    let effect = Fx {
        kind: "zoom".into(),
        t: 0.0,
        dur: width / 4.0,
        ..Default::default()
    };
    naivepost::ui::cut_lane_paint::view_for(&effect, width, ROW_H)
}

/// S3 (`The ✕ in the band's middle (≥ 32 px)`): a wide bar draws its cross CENTRED, not merely somewhere.
fn f3_8_s3_a_wide_bar_draws_its_cross_in_the_middle() {
    let view = zoom_view(200.0);
    let (data, width, height) = paint(&view);
    let middle = height / 2;
    let centre = width / 2;
    assert!(
        any_white_in(&data, width, middle, centre - 3, centre + 4),
        "no white cross within 3 px of the bar's middle ({centre}) on row {middle}"
    );
    // And it is the MIDDLE that carries it: far from the centre of a zoom bar there is only wash and the mark.
    assert!(
        !any_white_in(&data, width, middle, centre + 40, centre + 70),
        "white pixels 40-70 px right of centre mean the glyph is not centred"
    );
}

/// S3: under `killMinPx` there is no cross to find, and the drawn rule cannot drift from the threshold.
fn f3_8_s3_a_bar_under_killminpx_draws_no_cross() {
    assert_eq!(fx_band::KILL_MIN_PX, 32.0);
    assert!(fx_band::kill_open(32.0), "32 px opens the kill");
    assert!(!fx_band::kill_open(31.9), "and 31.9 does not");

    let narrow = fx_band::KILL_MIN_PX - 1.0;
    let view = zoom_view(narrow);
    let (_, width, _) = paint(&view);
    assert!(!fx_band::kill_open(view.width), "the door opened for a {} px bar", view.width);
    // A bar under the threshold gets no cross of its own. It IS too narrow to print a caption (§A.8 uses
    // `GRIP_MIN_PX`), but its MARK glyph is white ink and on a 31 px bar it reaches past the middle — so what is
    // pinned is measured, not assumed: the same width with NO mark and no text (so nothing but wash and edge) may
    // not be white at its centre, while the wide bar must be.
    let bare = paint(&naivepost::ui::cut_lane_paint::BarView {
        mark: "",
        text: String::new(),
        ..zoom_view(narrow)
    });
    assert!(
        !any_white_in(&bare.0, bare.1, bare.2 / 2, width / 2 - 2, width / 2 + 3),
        "a {} px bar carrying only its wash shows white at its middle, where {:?} would have to be blamed",
        narrow as i32,
        mark_of_zoom()
    );
    // And the wide bar proves the cross really is there where this one has none.
    let wide = paint(&zoom_view(200.0));
    assert!(
        any_white_in(&wide.0, wide.1, wide.2 / 2, wide.1 / 2 - 3, wide.1 / 2 + 4),
        "the 200 px bar has no cross to compare against"
    );
}

/// S1 (`ends are grips when the band is ≥ 30 px`): the two end ticks show, and only when the rule says grips exist.
fn f3_8_s1_a_bar_with_grips_shows_two_end_ticks() {
    assert_eq!(fx_band::GRIP_MIN_PX, 30.0);
    let view = zoom_view(200.0);
    let (data, width, height) = paint(&view);
    let middle = height / 2;
    // The grip tick is drawn INSIDE the bar's own 1 px border, at x 2 and w-3 (`GRIP_TICK_PX`), in the darker
    // tone that border wears. The border itself is dark everywhere along the edge, so the comparison is with the
    // same distance in from the OTHER end of a bar that has no marks at all — a bare wash — not with the middle
    // of this one, whose caption ink would make any pixel look light.
    let bare = paint(&naivepost::ui::cut_lane_paint::BarView {
        mark: "",
        text: String::new(),
        ..zoom_view(200.0)
    });
    let at_left = min_channel_sum(&data, width, middle, 2, 3);
    let at_right = min_channel_sum(&data, width, middle, width - 3, width - 2);
    // A bare bar of the same size carries only its border at those two columns; the marked one must be darker
    // there because the tick sits just inside it.
    let bare_left = min_channel_sum(&bare.0, bare.1, bare.2 / 2, 2, 3);
    assert!(at_left <= bare_left, "no grip tick at the left end: {at_left} vs bare {bare_left}");
    assert!(at_right <= bare_left, "no grip tick at the right end: {at_right} vs bare {bare_left}");
    assert!(
        at_left < 400,
        "the left grip column reads as untouched wash: {at_left}"
    );
    // The ticks are AT THE ENDS: the middle of such a bar carries the cross, not a tick.
    assert!(
        whiteish(rgb(&data, width, width / 2, middle)),
        "the middle row should hold the cross, not bare wash"
    );

    // Under the grip threshold neither end is grabbable, so neither is marked.
    let thin = zoom_view(fx_band::GRIP_MIN_PX - 1.0);
    let (thin_data, thin_w, thin_h) = paint(&thin);
    let row = thin_h / 2;
    let thin_edge = min_channel_sum(&thin_data, thin_w, row, 1, 4);
    let thin_inside = min_channel_sum(&thin_data, thin_w, row, 8, thin_w - 8);
    assert!(
        thin_edge >= thin_inside,
        "a {} px bar marked a grip it does not have: edge {thin_edge} vs inside {thin_inside}",
        thin_w
    );
}

/// A label is §A.7's TAG beside a dashed line: it carries no kill of its own, so it wears neither mark.
fn f3_8_s3_a_label_tag_draws_neither() {
    let effect = Fx {
        kind: "label".into(),
        t: 0.0,
        dur: 50.0,
        text: "intro".into(),
        ..Default::default()
    };
    let view = naivepost::ui::cut_lane_paint::view_for(&effect, 200.0, ROW_H);
    assert_eq!(view.kind, EffectKind::Label);
    let (data, width, height) = paint(&view);
    let middle = height / 2;
    assert!(
        !any_white_in(&data, width, middle, width / 2 - 6, width / 2 + 7),
        "a label's tag drew a kill cross: {:?}",
        rgb(&data, width, width / 2, middle)
    );
    // The tag itself still paints (§A.7): colour at the left stub, and the dashed line reaches across.
    assert!(
        rgb(&data, width, 3, middle).2 > 0,
        "the label's tag stub did not paint at all"
    );
}

// --- the wire ------------------------------------------------------------------------------------------------------

/// Two records on separate rows: a 50 s zoom (200 px, kill open) and a 5 s speed (20 px, kill closed).
fn two_records() -> Cut {
    Cut {
        segs: vec![Seg { s: 0.0, e: 300.0, cam: 0, ..Default::default() }],
        fx: vec![
            Fx { kind: "zoom".into(), t: 0.0, dur: 50.0, ..Default::default() },
            Fx { kind: "speed".into(), t: 0.0, dur: 5.0, rate: 2.0, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// S3 through the page: the ✕ a person SEES is the ink in the bar's middle, and the button that answers the
/// press sits over that same middle — then removing takes the record out and one ↶ puts it back.
fn f3_8_s3_the_cross_in_a_bar_is_the_button_that_removes_it(app: &adw::Application) {
    let cut = two_records();
    let window = common::cut_page(app, &cut);
    ui::refresh_effects_lane(&window);
    settle();

    let bar = common::widget(&window, "fx-bar-0");
    assert!(
        widget_in(&window, "fx-remove-0").is_some(),
        "the 200 px bar carries no kill target"
    );
    assert!(
        widget_in(&window, "fx-remove-1").is_none(),
        "the 20 px bar offered a kill the rules refuse"
    );
    // The ink is the bar's main child, so the overlay is exactly as wide as the wash (the ✕ overlaps, never adds).
    assert_eq!(
        bar.first_child().map(|child| child.widget_name().to_string()),
        Some("fx-bar-paint-0".to_string()),
        "the bar lost its ink when it became an overlay"
    );

    // Where the kill target actually landed, measured in the window's own coordinates. Headless and unallocated
    // these rects can come back empty, and then the alignment is asked of the layout property instead — both say
    // the same thing: the target is centred over its bar, which a Box could not do.
    let kill = common::widget(&window, "fx-remove-0");
    // Both widgets live in this window's tree, so their bounds — each relative to its own nearest surface
    // ancestor — are directly comparable. Unallocated headless they can come back empty, and then the centring is
    // asked of the layout property instead: same claim, one of the two always answers.
    let bar_rect = bar.compute_bounds(&window);
    let kill_rect = kill.compute_bounds(&window);
    if let (Some(b), Some(k)) = (bar_rect, kill_rect) {
        let bar_mid = b.x() + b.width() / 2.0;
        let kill_mid = k.x() + k.width() / 2.0;
        assert!(
            (bar_mid - kill_mid).abs() <= 3.0,
            "the kill target sits {:.1} px off its bar's middle ({bar_mid} vs {kill_mid})",
            (bar_mid - kill_mid).abs()
        );
    } else {
        assert_eq!(
            kill.halign(),
            gtk::Align::Center,
            "unallocated headless, and the kill target is not centred either"
        );
    }

    // Press it: the state the logic test checks, not a painted pixel.
    let before = ui::review_cut_of(&window).fx.len();
    common::click(&window, "fx-remove-0");
    assert_eq!(ui::review_cut_of(&window).fx.len(), before - 1, "the ✕ removed nothing");
    assert_eq!(status_text(&window), fx_band::removed("Zoom"), "not the kill sentence");
    common::click(&window, "undo-button");
    assert_eq!(ui::review_cut_of(&window).fx.len(), before, "one ↶ did not bring it back");
    // And the bar survived the round trip with its ink still its own.
    assert_eq!(
        common::widget(&window, "fx-bar-0")
            .first_child()
            .map(|child| child.widget_name().to_string()),
        Some("fx-bar-paint-0".to_string()),
        "after undo the bar no longer carries its ink"
    );
    RAN_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn round(app: &adw::Application) {
    step!(
        "f3_8_s3_the_cross_in_a_bar_is_the_button_that_removes_it",
        f3_8_s3_the_cross_in_a_bar_is_the_button_that_removes_it(app)
    );
    release_last_window();
}

#[test]
fn f3_8_s3_the_lane_draws_the_targets_it_answers() {
    let root = std::env::temp_dir().join(format!("np-f38-ink-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    f3_8_s3_a_wide_bar_draws_its_cross_in_the_middle();
    f3_8_s3_a_bar_under_killminpx_draws_no_cross();
    f3_8_s1_a_bar_with_grips_shows_two_end_ticks();
    f3_8_s3_a_label_tag_draws_neither();
    common::one_app_round(round);

    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(RAN_WIRE.load(Ordering::SeqCst), "the wire check never ran");
    // The row height the marks are sized inside comes from the lane's own door, not a guess above.
    assert_eq!(fx_lane::lane_height_px(1), ROW_H);
}
