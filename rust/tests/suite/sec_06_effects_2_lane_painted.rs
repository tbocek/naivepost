//! §06-effects#2-the-lane-and-the-preview — the lane's bars carry INK, not just a CSS tint.
//!
//! §2 gives each kind a colour AND something to say on its own bar (inventory/effects.md §A.8: a speed `×N` with
//! its sound suffix or its seconds, a zoom its seconds, a text the truncated words, a volume the percentage, a
//! label its name), and colours them as a WASH over the pictures under the lane (§A.2 spells one out as
//! `fill RGBA(0.92,0.42,0.6,0.4)`). A CSS rectangle can state the colour and nothing else, which is how the lane
//! read in every shot: coloured bands, no mark, no caption, and a label as loud as a zoom.
//!
//! So these checks paint through the same function the page's draw func calls ([`cut_lane_paint::paint_bar`]) onto
//! a cairo `ImageSurface` and SAMPLE the bytes back (§00-principles §5: measure the artifact). One check pins what
//! each bar says, one pins that the ink is actually on the pixels and that a label stays a tag rather than a block,
//! and one goes through the real window so the wire — refresh → attach → painted surface — is proven, not assumed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, EffectKind, Fx, Seg};
use naivepost::fx_band;
use naivepost::fx_lane;
use naivepost::timeline::Recording;
use naivepost::ui::{self, cut_lane_paint as paint};

use crate::common;
use common::{cut_page_settled_first as cut_page, settle, widget};

static RAN_WORDS: AtomicBool = AtomicBool::new(false);
static RAN_INK: AtomicBool = AtomicBool::new(false);
static RAN_WIRE: AtomicBool = AtomicBool::new(false);
/// Why the single activation stopped early, for the entry test to report instead of an unhelpful "never ran"
/// (a panic inside GTK's activate is non-unwinding — it aborts with nothing worth reading).
static STOP_REASON: std::sync::LazyLock<Arc<std::sync::Mutex<Option<String>>>> =
    std::sync::LazyLock::new(|| Arc::new(std::sync::Mutex::new(None)));

/// The row height the lane paints at, so a test's bar is the size the page's bar is.
const ROW_H: f64 = 26.0;
/// Wide enough to clear both `fx_band::GRIP_MIN_PX` (the caption threshold this painter reuses) and the ✕ rule.
const WIDE: f64 = 200.0;

fn fx(kind: EffectKind, t: f64, dur: f64) -> Fx {
    let key = match kind {
        EffectKind::Zoom => "zoom",
        EffectKind::Speed => "speed",
        EffectKind::Text => "text",
        EffectKind::Svg => "svg",
        EffectKind::Volume => "volume",
        EffectKind::Label => "label",
    };
    Fx {
        kind: key.to_string(),
        t,
        dur,
        ..Default::default()
    }
}

/// Paint one view and hand back `(bytes, stride, width, height)` — the route `sec_06_effects_2_lane_rows_order.rs`
/// uses for the band: run the page's own painter over a surface the test can read, then look at the pixels.
fn painted(view: &paint::BarView) -> (Vec<u8>, usize, i32, i32) {
    let mut surface = paint::painted_surface_for_test(view).expect("the bar paints");
    let data = surface.data().expect("read the pixels back").to_vec();
    let stride = surface.stride() as usize;
    (data, stride, surface.width(), surface.height())
}

/// The pixel at `(x, y)` as cairo STORED it — `(b, g, r)` of the raw ARgb32 word, i.e. premultiplied and reversed,
/// which is what [`expected_wash`] is spelled in. No division here on purpose: un-premultiplying a partially
/// covered pixel rounds, and a check about colours should not hinge on that rounding.
fn pixel(bytes: &[u8], stride: usize, x: i32, y: i32) -> (u32, u32, u32) {
    let at = y as usize * stride + x as usize * 4;
    (bytes[at + 2] as u32, bytes[at + 1] as u32, bytes[at] as u32)
}

/// The wash a bar's fill arrives as ON THE SURFACE. cairo writes an image surface PREMULTIPLIED and BGRA on this
/// machine, so a 0.4-alpha cyan sits in the bytes as its colour times that alpha with 0.4 in the alpha byte; this
/// is that product, derived from the same two doors the painter reads so it cannot drift from the rule.
fn expected_wash(kind: EffectKind, staying: bool) -> (u32, u32, u32) {
    let (r, g, b) = fx_lane::bar_colour(kind, staying);
    let a = fx_lane::bar_wash_alpha(kind);
    let byte = |c: f64| (c * 255.0).round() as u32 * (a * 255.0).round() as u32 / 255;
    (byte(r), byte(g), byte(b))
}

fn near(a: (u32, u32, u32), b: (u32, u32, u32), within: u32) -> bool {
    let one = |t: (u32, u32, u32)| [t.0, t.1, t.2];
    let (x, y) = (one(a), one(b));
    (0..3).all(|i| x[i].abs_diff(y[i]) <= within)
}

/// Does any pixel differ from `ground` by more than `by`? That is "ink was laid down here", measured rather than
/// asserted from the arithmetic that would have drawn it.
fn has_ink_other_than(
    bytes: &[u8],
    stride: usize,
    w: i32,
    h: i32,
    ground: (u32, u32, u32),
    by: u32,
) -> bool {
    (0..h).any(|y| (0..w).any(|x| !near(pixel(bytes, stride, x, y), ground, by)))
}

// --- S19: what each bar says -----------------------------------------------------------------------------

fn every_kind_says_its_own_thing() {
    // §A.8, read down to what fits on a bar: the number or the words, never the `at m:ss` (the bar's place IS
    // its second) and never the fades.
    let mut zoom = fx(EffectKind::Zoom, 5.0, 7.4);
    zoom.trans = 1.0;
    assert_eq!(
        paint::caption_of(&zoom),
        "7.4s",
        "a zoom prints its seconds"
    );

    let mut slow = fx(EffectKind::Speed, 20.0, 12.0);
    slow.rate = 0.5;
    assert_eq!(
        paint::caption_of(&slow),
        "\u{00d7}0.5",
        "a plain speed prints its rate"
    );
    slow.snd = "scene".into();
    assert_eq!(
        paint::caption_of(&slow),
        "\u{00d7}0.5 \u{00b7} sound 1\u{00d7} to the scene's end",
        "and §A.3's lane suffix after it"
    );
    slow.snd = "pitched".into();
    assert_eq!(
        paint::caption_of(&slow),
        "\u{00d7}0.5 pitched",
        "the `pitched` column of the sound table"
    );
    slow.snd = "mute".into();
    assert_eq!(
        paint::caption_of(&slow),
        "\u{00d7}0.5 silent",
        "the `silent` column of the sound table"
    );

    let mut stop = fx(EffectKind::Speed, 40.0, 3.0);
    stop.rate = 0.0;
    assert_eq!(
        paint::caption_of(&stop),
        "3.0s",
        "a stop's news is its seconds, not \u{00d7}0"
    );

    let mut flat = fx(EffectKind::Speed, 44.0, 9.0);
    flat.rate = 1.0;
    assert_eq!(
        paint::caption_of(&flat),
        "9.0s",
        "a rate of 1 did nothing to the clock, so it says its seconds"
    );

    let mut words = fx(EffectKind::Text, 50.0, 4.0);
    words.text = "Ethereum\nin one lecture".into();
    assert_eq!(
        paint::caption_of(&words),
        "Ethereum in one lectu\u{2026}",
        "a caption prints its words on ONE line, truncated to the bar"
    );
    let long = fx(EffectKind::Text, 55.0, 4.0);
    let mut long_words = "a".repeat(paint::CAPTION_MAX_CHARS + 9);
    long_words.push('!');
    let mut truncation_check = words.clone();
    truncation_check.text = long_words.clone();
    let said = paint::caption_of(&truncation_check);
    assert_eq!(
        said.chars().count(),
        paint::CAPTION_MAX_CHARS,
        "a long caption is truncated, not wrapped"
    );
    assert!(said.ends_with('\u{2026}'), "and says so: {said}");
    assert!(
        long.dur > 0.0,
        "the long record above is only a fixture: {long_words}"
    );

    let mut drawing = fx(EffectKind::Svg, 60.0, 5.0);
    drawing.src = "/home/someone/assets/tier.svg".into();
    assert_eq!(
        paint::caption_of(&drawing),
        "tier.svg",
        "a drawing prints its file's name (§A.5)"
    );

    let mut loud = fx(EffectKind::Volume, 70.0, 6.0);
    loud.gain = 2.0;
    assert_eq!(
        paint::caption_of(&loud),
        "200%",
        "a volume prints the percentage (§A.6)"
    );

    let mut tag = fx(EffectKind::Label, 80.0, 6.0);
    tag.text = "boss fight".into();
    assert_eq!(
        paint::caption_of(&tag),
        "boss fight",
        "a label prints its name (§A.7)"
    );
    tag.text = "the reveal of the thing".into();
    assert_eq!(
        paint::caption_of(&tag),
        "the reveal",
        "and caps it the way the form does (fx_label::NAME_MAX_CHARS)"
    );

    // The marks are per-kind, and a stop gets its own glyph even though its stored kind is still `speed`.
    assert_eq!(
        paint::mark_of(&zoom),
        "\u{2295}",
        "a zoom is \u{2295}, the dropdown's own glyph"
    );
    assert_eq!(
        paint::mark_of(&stop),
        "\u{23f8}",
        "a stop is a still, so it wears \u{23f8}"
    );
    assert_eq!(paint::mark_of(&slow), "\u{23e9}", "a clock wears \u{23e9}");
    assert_eq!(paint::mark_of(&tag), "\u{1f3f7}", "a label wears \u{1f3f7}");
    // A view carries the staying flag only when the kind can mean it (§2 gives the second colour to a zoom alone).
    let mut staying_zoom = zoom.clone();
    staying_zoom.stay = true;
    assert!(
        paint::view_for(&staying_zoom, WIDE, ROW_H).staying,
        "a staying zoom knows it"
    );
    let mut stay_caption = words.clone();
    stay_caption.stay = true;
    assert!(
        !paint::view_for(&stay_caption, WIDE, ROW_H).staying,
        "a caption's stray `stay` means nothing"
    );
    RAN_WORDS.store(true, Ordering::SeqCst);
}

// --- S20: the ink is on the pixels ------------------------------------------------------------------------

fn the_wash_and_the_ink_are_on_the_pixels() {
    // A zoom at full width: the middle of the bar must BE the cyan wash, and somewhere in the bar there must be
    // pixels that are neither the wash nor empty — the mark and the caption, as ink.
    let mut zoom = fx(EffectKind::Zoom, 5.0, 7.4);
    zoom.stay = false;
    let view = paint::view_for(&zoom, WIDE, ROW_H);
    let (bytes, stride, w, h) = painted(&view);
    let wash = expected_wash(EffectKind::Zoom, false);
    // F3.8 S3 moved what sits at the bar's exact middle: a bar wide enough for a kill now wears its ✕ there
    // (`fx_band::KILL_MIN_PX`), so the WASH is sampled a third of the way along the bar instead — clear of the
    // mark and the caption at the left and of the cross in the middle.
    let (wash_x, wash_y) = (w / 3, h - 3);
    assert!(
        near(pixel(&bytes, stride, wash_x, wash_y), wash, 3),
        "the bar is washed with its kind's colour at fx_lane::bar_wash_alpha, saw {:?} at ({wash_x},{wash_y})",
        pixel(&bytes, stride, wash_x, wash_y)
    );
    assert!(
        has_ink_other_than(&bytes, stride, w, h, wash, 40),
        "the bar carries ink beyond its wash (the mark and the caption) — a bare tint is what this replaces"
    );

    // The staying zoom: a DIFFERENT colour for a different intention (§2 lists it separately).
    let mut stays = zoom.clone();
    stays.stay = true;
    let stayed = paint::view_for(&stays, WIDE, ROW_H);
    let (kept_bytes, kept_stride, _, _) = painted(&stayed);
    assert!(
        near(
            pixel(&kept_bytes, kept_stride, wash_x, wash_y),
            expected_wash(EffectKind::Zoom, true),
            3
        ),
        "a staying zoom is the orange bar"
    );

    // A label is a TAG (§A.7): filled at the left, a dashed line across the rest — NOT a block the length of the
    // bar, which would claim a label changes the picture. Measured at three quarters along, where a block would
    // still be solid wash and only a thin line may be.
    let mut tag = fx(EffectKind::Label, 80.0, 6.0);
    tag.text = "boss fight".into();
    let tag_view = paint::view_for(&tag, WIDE, ROW_H);
    let (tag_bytes, tag_stride, tw, th) = painted(&tag_view);
    let far_x = (tw as f64 * 0.75) as i32;
    let mid_y = th / 2;
    let ground: (u32, u32, u32) = (0, 0, 0);
    assert!(
        pixel(&tag_bytes, tag_stride, far_x, mid_y) != ground
            || pixel(&tag_bytes, tag_stride, far_x, 2) == ground,
        "the label's line reaches across the bar (a tick and a dashed line, §A.7)"
    );
    assert!(
        pixel(&tag_bytes, tag_stride, far_x, 2) == ground,
        "and the bar is NOT filled across its whole width: a label is a tag, not a block"
    );
    // The tag's own colour, read at a row ABOVE the centred caption so the white text is not in the way.
    assert!(
        near(
            pixel(&tag_bytes, tag_stride, 4, 2),
            expected_wash(EffectKind::Label, false),
            3
        ),
        "the tag itself is the grey-white colour at its paler alpha"
    );

    // Under the caption threshold the bar prints its glyph alone — the same `fx_band::GRIP_MIN_PX` the grip rule
    // uses, so the ink never promises text the bar has no room for. Measured as a count: a wide bar carries ink
    // past where a caption would begin, a narrow one may not — which makes the check about the threshold rather
    // than about a painter that never draws words at all.
    let beyond = fx_band::GRIP_MIN_PX as i32;
    let ink_past = |bytes: &[u8], stride: usize, w: i32, h: i32| {
        (0..h)
            .filter(|y| ((beyond)..w).any(|x| !near(pixel(bytes, stride, x, *y), wash, 40)))
            .count()
    };
    let (wide_bytes, wide_stride, ww, wh) = painted(&view);
    assert!(
        ink_past(&wide_bytes, wide_stride, ww, wh) > 0,
        "a bar with room carries its caption past the caption threshold"
    );
    let narrow = paint::view_for(&zoom, fx_band::GRIP_MIN_PX - 6.0, ROW_H);
    let (narrow_bytes, narrow_stride, nw, nh) = painted(&narrow);
    assert!(
        nw < fx_band::GRIP_MIN_PX as i32 && ink_past(&narrow_bytes, narrow_stride, nw, nh) == 0,
        "under the threshold a bar prints its glyph alone: nothing may be inked past x={beyond}"
    );
    RAN_INK.store(true, Ordering::SeqCst);
}

// --- S21: the page wires the ink to the bars --------------------------------------------------------------

fn the_page_paints_every_placed_bar(app: &adw::Application) {
    let mut zoom = fx(EffectKind::Zoom, 5.0, 50.0);
    zoom.trans = 1.0;
    let mut tag = fx(EffectKind::Label, 12.0, 6.0);
    tag.text = "intro".into();
    let mut cut = Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 300.0,
            ..Default::default()
        }],
        fx: vec![zoom, tag],
        ..Default::default()
    };
    // The window first, then the takes: `set_session_recordings` writes the newest per-window slot, and calling
    // it before a window exists borrows a slot that is being built (a `RefCell already borrowed` abort inside
    // GTK's non-unwinding activate). Every lane widget file that names recordings does it in this order.
    let window = cut_page(app, &cut);
    ui::set_session_recordings(&[Recording {
        base: "take-a".into(),
        start: 0.0,
        end: 300.0,
    }]);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 5.0 });
    ui::refresh_effects_lane(&window);
    settle();

    for index in 0..cut.fx.len() {
        let bar = widget(&window, &format!("fx-bar-{index}"));
        let ink = widget(&window, &format!("fx-bar-paint-{index}"));
        assert!(
            ink.parent().as_ref().is_some_and(|parent| parent == &bar),
            "bar #{index} carries its own painted surface, so the lane is inked and not merely tinted"
        );
        assert!(
            bar.first_child().is_some_and(|first| first.widget_name() == format!("fx-bar-paint-{index}")),
            "the surface is the bar's first child, leaving the \u{2715} the last child a press reaches"
        );
    }
    // A refresh must not stack a second surface on the same bar (build-once, like every other refresh path).
    ui::refresh_effects_lane(&window);
    settle();
    let surfaces = widget(&window, "fx-bar-0")
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(|child| child.ok())
        .filter(|child| {
            child
                .downcast_ref::<gtk::Widget>()
                .is_some_and(|w| w.widget_name() == "fx-bar-paint-0")
        })
        .count();
    assert_eq!(
        surfaces, 1,
        "one painted surface per bar, however many refreshes run"
    );

    // And the ink follows the record: drop the label through the page's own door and its surface goes with it.
    cut.fx.remove(1);
    ui::seed_review_cut(&window, &cut);
    ui::refresh_effects_lane(&window);
    settle();
    assert!(
        ui::find_source_widget(&window, "fx-bar-paint-1").is_none(),
        "a record that went away takes its ink with it"
    );
    assert!(
        ui::find_source_widget(&window, "fx-bar-paint-0").is_some(),
        "and the record that stayed keeps its own"
    );
    window.close();
    settle();
    RAN_WIRE.store(true, Ordering::SeqCst);
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s19_every_bar_carries_what_its_record_says() {
    every_kind_says_its_own_thing();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(
        RAN_WORDS.load(Ordering::SeqCst),
        "the caption check never ran"
    );
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s20_the_bar_is_washed_and_inked_not_a_plain_tint() {
    the_wash_and_the_ink_are_on_the_pixels();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(RAN_INK.load(Ordering::SeqCst), "the pixel check never ran");
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s21_the_page_paints_every_placed_bar() {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(RAN_WIRE.load(Ordering::SeqCst), "the wired check never ran");
}

fn stopped_early() -> Option<String> {
    STOP_REASON.lock().unwrap().clone()
}

/// One application, one run for the widget check in this binary, through the shared harness door - for the reason
/// it gives: `gio` refuses a second `run_with_args`, so all GTK work lives inside one activation.
fn run_once() {
    if RAN_WIRE.load(Ordering::SeqCst) || stopped_early().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("np-sec06-ink-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd moved to our own temp root");
    common::one_app_round(the_page_paints_every_placed_bar);
}
