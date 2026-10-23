//! §05-cut#1-screen §1 badges **24** and **25** on the placeholder `track-strip`: where its ruler marks
//! fall, what they say, and that a kept scene really draws its bar there.
//!
//! `sec_05_cut_1_band_paints.rs` paints the PAINTED band (`cut_tracks`); nothing until now painted the
//! surface the trim and move gestures live on (`window_trim_strip::paint_track_strip`). That strip is the one
//! the wheel's + and −, the scrollbar and both drags convert against, so its ruler is the ruler the hand
//! reads — a mark at the wrong second there is a border the eye is told to grab at the wrong place.
//!
//! The checks paint through the real entry point and read bytes back: a layout that merely renders is not
//! evidence, and a colour applied to the wrong span only shows up as a number.

use adw::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};
use naivepost::{
    cut, cut_screen, cut_trim,
    tools::mm_ss,
    ui::{cut_view, window_trim_strip},
};

use crate::common;

/// The one GTK round this binary is allowed: tests run on their own threads and GTK refuses work off the
/// main one, so the window case exists inside a single `run_with_args` emission (the shape
/// `sec_05_cut_1_band_paints.rs` uses).
static GTK_ROUND: std::sync::Once = std::sync::Once::new();
static STRIP_RAN: AtomicBool = AtomicBool::new(false);

#[test]
fn sec_05_cut_1_screen_s25_a_kept_scene_draws_its_bar_on_the_strip() {
    GTK_ROUND.call_once(|| {
        let app = adw::Application::builder()
            .application_id(naivepost::ui::APP_ID)
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_a_kept_scene_draws_its_bar(app);
            app.quit();
        });
        let _ = app.run_with_args::<String>(&[]);
    });
    assert!(STRIP_RAN.load(Ordering::SeqCst), "the GTK round ran the strip case");
}

/// One kept footage scene.
fn seg(s: f64, e: f64) -> cut::Seg {
    cut::Seg {
        s,
        e,
        cam: 0,
        ..Default::default()
    }
}

/// The strip's own declared width: what `strip_pps` fits the session into, so the numbers below and the
/// painter agree without GTK having to allocate anything.
const STRIP_W: i32 = 480;
const STRIP_H: i32 = 100;
/// §2's zoom at open — the constant this strip is built with, pinned rather than assumed: a ruler test means
/// nothing if the surface is drawn at some other scale.
const OPEN_PPS: f64 = cut_screen::ZOOM_AT_OPEN;
/// A row inside the ruler band (RULER_H is 16 px), where only the ticks are inked.
const RULER_Y: i32 = 8;
const GROUND: (f64, f64, f64) = (1.0, 1.0, 1.0);

/// Paint the strip's real draw over a white ground and hand back a `(x, y) -> (r, g, b)` reader.
/// ARgb32 is B,G,R,A on little-endian.
fn painted() -> impl Fn(i32, i32) -> (u8, u8, u8) {
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, STRIP_W, STRIP_H).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a cairo context");
        cr.set_source_rgb(GROUND.0, GROUND.1, GROUND.2);
        cr.paint().expect("ground");
        window_trim_strip::paint_track_strip(&cr, STRIP_W, STRIP_H);
    }
    let data = surface.data().expect("pixels read back").to_vec();
    let stride = surface.stride() as usize;
    move |x: i32, y: i32| -> (u8, u8, u8) {
        // usize, not i32: the checks probe x-3 as well as x+3, and a mark at x 0 would otherwise overflow
        // the index rather than report what it saw.
        let base = y as usize * stride + x.max(0) as usize * 4;
        (data[base + 2], data[base + 1], data[base])
    }
}

/// The strip opens at §2's open zoom, so the marks below are predictable. This strip deliberately keeps its
/// own zoom rather than the PAINTED band's live one (`cut_strip_view::pps`'s note).
fn strip_pps() -> f64 {
    let pps = cut_view::strip_pps();
    assert!(
        (pps - OPEN_PPS).abs() < 0.01,
        "the strip opens at §2's {OPEN_PPS} px/s so the ruler's marks are predictable; it reads {pps}"
    );
    pps
}

/// §1 badge 24: the strip's marks sit at `cut_trim::ruler_ticks` — every whole-second label over the filmed
/// length at the strip's zoom — and each is 1 px of ink with the ground beside it, not a filled row.
#[test]
fn sec_05_cut_1_screen_s24_the_strips_ruler_marks_every_whole_second_label() {
    let pps = strip_pps();
    let at = painted();
    let ticks = cut_trim::ruler_ticks(600.0, pps, naivepost::ui::window::RULER_EVERY_SECONDS);
    let every = naivepost::ui::window::RULER_EVERY_SECONDS;
    assert!(
        ticks.len() > 1,
        "a 600 s session at the open zoom has several whole-second labels, not one"
    );
    assert_eq!(
        ticks[1].1,
        mm_ss(every),
        "the second mark is spelled by the app's one m:ss clock for the ruler — the tenth the playhead \
         clock carries would print `00:300`"
    );
    for (x, label) in ticks.iter().take(3) {
        let x = *x as i32;
        assert!(
            x + 3 < STRIP_W,
            "the mark at {label} ({x} px) should be inside the {STRIP_W} px strip"
        );
        let on = at(x, RULER_Y);
        // Ground on BOTH sides, each on the far side from the edge: the first mark sits at x 0, where the
        // pixel to its left is the clamped first column and would read as ink with nothing drawn there.
        let (side, other) = if x < 4 {
            (x + 4, x + 9)
        } else {
            (x - 4, x + 4)
        };
        assert_ne!(on, at(side, RULER_Y), "the {label} mark at x {x} is inked, with ground beside it");
        assert_ne!(
            on,
            at(other, RULER_Y),
            "and ground on its other side too: a filled ruler row would pass a single-pixel check"
        );
    }
}

/// §1 badge **25** — `green bar of a kept scene`: on this strip the bar row carries ink across every
/// kept scene's own span and none of it across the dropped gap between two scenes. The spans are the same
/// boxes the painter indexes (`cut_view::strip_clip_boxes_for_test`), so a bar drawn at the wrong second, or
/// one that fills the whole row regardless of the cut, fails here rather than looking fine.
///
/// One case in one GTK round: the window's cut slot is process-shared, so two `#[test]`s seeding it would
/// race for the same slot.
fn check_a_kept_scene_draws_its_bar(app: &adw::Application) {
    // Two kept scenes with a dropped stretch between them: at the strip's own zoom they land well inside the
    // 480 px shot, which is what makes "ink on the span, no bright bar in the gap" a comparison.
    let seeded = cut::Cut {
        segs: vec![seg(0.0, 30.0), seg(45.0, 90.0)],
        ..Default::default()
    };
    let window = common::cut_page(app, &seeded);
    common::settle();
    let at = painted();
    let row_y = (cut_trim::RULER_H + cut_trim::BAR_H * 0.5) as i32;
    let boxes = cut_view::strip_clip_boxes_for_test();
    assert_eq!(boxes.len(), 2, "two kept scenes draw two boxes: {boxes:?}");
    for b in &boxes {
        let from = b.x.round() as i32;
        let to = (b.x + b.w).round() as i32;
        assert!(to - from >= 2, "box {} is {} px wide, nothing to sample", b.index, b.w);
        let inside = (from.max(0)..to.min(STRIP_W))
            .filter(|x| at(*x, row_y) != (255, 255, 255))
            .count();
        assert!(
            inside > (to - from).max(0) as usize / 2,
            "the kept scene at x {from}..{to} draws its bar across its own span; only {inside} px inked",
        );
    }
    // And the dropped stretch between them takes no KEPT bar: the strip dims it (§F2.2 S6, darker than both
    // its neighbours), so what is inked there must be dimmer than the green beside it, never as bright.
    let gap_from = (boxes[0].x + boxes[0].w).round() as i32;
    let gap_to = boxes[1].x.round() as i32;
    assert!(gap_to - gap_from >= 2, "the staged gap is {} px", gap_to - gap_from);
    let lum = |p: (u8, u8, u8)| -> f64 {
        (0.2126 * p.0 as f64 + 0.7152 * p.1 as f64 + 0.0722 * p.2 as f64) / 255.0
    };
    let kept_edge = lum(at(gap_from - 4, row_y));
    let gap_worst = (gap_from..gap_to).map(|x| lum(at(x, row_y))).fold(0.0f64, f64::max);
    assert!(
        gap_worst < kept_edge,
        "a dropped stretch draws no kept bar: the brightest pixel in the gap x {gap_from}..{gap_to} reads \
         {gap_worst:.3} against the kept bar's {kept_edge:.3} beside it"
    );
    window.close();
    STRIP_RAN.store(true, Ordering::SeqCst);
}
