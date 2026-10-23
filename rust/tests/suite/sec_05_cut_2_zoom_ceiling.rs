//! §05-cut#2 §2 — the two zoom numbers that are NOT a fit: "zoom 4 px/s at open … ceiling 240 px/s".
//!
//! `sec05_cut_2_zoom_fit.rs` pins the FLOOR (the filmed-length fit of this session in this view) and that
//! the stored zoom moves the drawn geometry. This file pins the other end of the same sentence, as numbers
//! rather than through a window: what the strip opens at, and the depth no door is allowed to pass.
//!
//! These are the doors the Cut toolbar's `zoom-in-button` / `zoom-out-button` actually press: wired in
//! `ui::window` beside the `Tool` table (`[("zoom-out-button", false), ("zoom-in-button", true)]`), and the
//! number the layout reads back is `ui::cut_view::live_pps(view)` = `pps().clamp(floor_pps(view), ZOOM_MAX)`.
//! So the ceiling below is not a constant nobody consults — it is applied on the live path, here and there.

use naivepost::cut_screen::{self, ZOOM_AT_OPEN, ZOOM_MAX, ZOOM_STEP};

/// The wheel's factor base, so a test that deepens by "a lot" says how much.
fn deep() -> f64 {
    ZOOM_STEP.powi(6)
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s3_the_open_zoom_is_four_px_per_second(
) {
    // §2: "zoom 4 px/s at open". A number, because the page draws with it before anything is measured.
    assert_eq!(
        ZOOM_AT_OPEN, 4.0,
        "the strip opens at four pixels to the second"
    );

    // An empty session answers the opening zoom rather than 0 or infinity: a cut with no footage still has
    // a timeline, and a zoom of zero divides by itself on the first draw. Both degenerate inputs, both ways.
    assert_eq!(
        fit(0.0, 1200.0),
        ZOOM_AT_OPEN,
        "nothing filmed opens at 4, not at 0"
    );
    assert_eq!(
        fit(-5.0, 1200.0),
        ZOOM_AT_OPEN,
        "and a negative length is the same nothing"
    );
    assert_eq!(
        fit(300.0, 0.0),
        ZOOM_AT_OPEN,
        "a view of no pixels is not an infinite zoom"
    );
    assert_eq!(fit(300.0, -1.0), ZOOM_AT_OPEN, "nor is a negative one");

    // And when there IS a filmed length, the fit is the honest division — which is where the floor's job
    // starts and this constant's ends (pinned as a rule in `sec05_cut_2_zoom_fit.rs`).
    assert_eq!(
        fit(300.0, 1200.0),
        4.0,
        "a five-minute tape fits 1200 px at exactly the opening zoom"
    );
    assert_ne!(
        fit(3600.0, 1200.0),
        ZOOM_AT_OPEN,
        "an hour does not: the fit moved off the constant"
    );
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s4_the_ceiling_is_240_px_per_second_and_no_door_passes_it(
) {
    // §2: "ceiling 240 px/s". Past it a second stops being a thing you can point at, so no door goes over.
    assert_eq!(ZOOM_MAX, 240.0, "the deepest zoom the spec names");

    // + from just under the ceiling lands ON it and then stays: eight more presses change nothing.
    let one_step_up = cut_screen::zoom_up(200.0, 1.0);
    assert_eq!(
        one_step_up, ZOOM_MAX,
        "200 × {} would be 250; the ceiling holds it at 240",
        ZOOM_STEP
    );
    let mut pps = one_step_up;
    for _ in 0..8 {
        pps = cut_screen::zoom_up(pps, 1.0);
        assert_eq!(pps, ZOOM_MAX, "pressing + past the ceiling never passes it");
    }
    // Even a start already above the ceiling comes back down to it rather than climbing.
    assert_eq!(
        cut_screen::zoom_up(500.0, 1.0),
        ZOOM_MAX,
        "a stored zoom above the ceiling is clamped, not kept"
    );

    // The wheel's around-the-cursor door clamps identically, and its scroll-back stays on screen.
    let (next, origin) = cut_screen::zoom_around(120.0, deep(), 800.0, 100.0, 1.0);
    assert_eq!(
        next, ZOOM_MAX,
        "the wheel cannot deepen past the ceiling either"
    );
    assert!(
        origin >= 0.0,
        "and the scroll back never goes negative: {origin}"
    );

    // − respects the same ceiling from above: it walks down toward the floor, never sideways past the top.
    assert_eq!(
        cut_screen::zoom_down(600.0, 1.0),
        ZOOM_MAX,
        "− from above the ceiling stops at it"
    );
    assert_eq!(
        cut_screen::zoom_down(ZOOM_MAX, 1.0),
        ZOOM_MAX / ZOOM_STEP,
        "and then steps normally"
    );

    // The step itself is the spec's factor, so a rung count means the depth it means.
    assert_eq!(
        ZOOM_STEP, 1.25,
        "one rung is ×1.25, the buttons' factor and the wheel's base"
    );
}

/// One call, named for what it asks: the floor for a filmed length across a view.
fn fit(filmed_seconds: f64, view_px: f64) -> f64 {
    cut_screen::fit_pps(filmed_seconds, view_px)
}
