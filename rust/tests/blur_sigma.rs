// §10-parameters · P.eng.blurSigma — the frame-edge blur ("0.02·height, min 4"; prototype `blurSigma`,
// gui/produce.go:1310, applied at gui/produce.go:1331 as `math.Max(4, float64(b.h)*blurSigma)`).
//
// The blur is not applied to the picture. It is applied to an oversized copy of the picture that sits
// *behind* the frame, so a session whose aspect ratio does not match the chosen box shows a smeared
// backdrop rather than black bars or a readable second image around the edges. The sigma scales with
// the finished height so the smear looks like the same amount of blur at 720p and at 4K — a fixed
// pixel sigma would read as heavy fog on a small frame and as nothing at all on a large one. The floor
// exists because below 200 px of height 0.02·height drops under 4 and the edge stops being blurred at
// all: a hard border reappears exactly where a small preview most needs the seam hidden.
//
// What this file pins: the two constants and their single catalogue row; the fraction scaling with
// height with the round-up applied; where the floor binds and where it stops; the string the filter chain
// actually carries for a real frame size; and that 0.02 here is a proportion of height rather than the
// other 0.02s in the tree (a zoom rect's height-factor floor).

use naivepost::params;
use naivepost::produce_render as render;
use naivepost::project::Produce;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

/// A row by id across all five lists. `params::find` searches only `prepare()`, which cannot see a
/// §08 row, so the lookup goes through the chained lists instead.
fn row_for(id: &str) -> params::Param {
    all_rows()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{id} is not catalogued"))
}

/// A clip whose only interesting field is the box it will be rendered into.
fn clip_at(frame: (i32, i32)) -> render::Clip {
    render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 10.0,
        rate: 1.0,
        frame,
        ..Default::default()
    }
}

/// The video filter chain for one frame size, with or without the blurred backdrop ticked. This is the
/// public entry point `tests/produce_render_flow.rs` uses for filter chains (`render::video_chain`).
fn chain_for(frame: (i32, i32), blurred: bool) -> Vec<String> {
    let settings = Produce {
        blurred_edges: blurred,
        ..Produce::default()
    };
    render::video_chain(&clip_at(frame), &settings, None)
}

/// S1: `P.eng.blurSigma` = "0.02·height, min 4", held by the pair of constants `blur_sigma` reads and
/// catalogued once under the family §10 files it in.
#[test]
fn p_eng_blursigma_s1_the_value_and_its_single_home() {
    // P.eng.blurSigma — "frame-edge blur".
    assert_eq!(render::BLUR_SIGMA_FRACTION, 0.02);
    assert_eq!(render::BLUR_SIGMA_MIN, 4.0);

    let row = row_for("P.eng.blurSigma");
    assert_eq!(
        row.from,
        "produce_render::blur_sigma",
        "a row must name the function that owns the rule, not one of the two numbers alone"
    );
    // Hard-coded from spec/10-parameters.md line 209. The row itself is built from the constants
    // (`num(BLUR_SIGMA_FRACTION)`), so this assertion pins that the derived spelling still matches the
    // spec's characters rather than proving nothing: change either constant and this fails.
    assert_eq!(row.spelled, "0.02");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), render::BLUR_SIGMA_FRACTION);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.blurSigma"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // One row per id across all five lists — one row, one home.
    assert_eq!(
        all_rows()
            .iter()
            .filter(|param| param.id == "P.eng.blurSigma")
            .count(),
        1,
        "one row per id across the catalogue"
    );
}

/// S2: above the floor the sigma is the fraction of the finished height, rounded up to a whole pixel.
#[test]
fn p_eng_blursigma_s2_the_fraction_scales_with_height_and_ceil_is_applied() {
    // 0.02 * 1080 = 21.6. Truncated that is 21; ceil gives 22. Asserting 22 therefore pins the
    // round-up rather than a truncation, which is what keeps the blur at least as wide as the rule asked.
    assert_eq!(render::blur_sigma(1080), 22, "21.6 -> 22, not 21");
    assert_ne!(render::blur_sigma(1080), 21, "truncation would land here; the rule rounds up");
    // 0.02 * 720 = 14.4 -> 15 (truncate would be 14).
    assert_eq!(render::blur_sigma(720), 15, "14.4 -> 15, not 14");
    // 0.02 * 2160 = 43.2 -> 44 (truncate would be 43).
    assert_eq!(render::blur_sigma(2160), 44, "43.2 -> 44, not 43");

    // Linearity, taken where neither the floor nor the rounding interferes: both heights give a whole
    // number of pixels (500*0.02 = 10.0, 1000*0.02 = 20.0), so doubling the height doubles the sigma
    // exactly. Away from those points the ceil makes the relation sub-linear by design.
    assert_eq!(render::blur_sigma(500), 10);
    assert_eq!(render::blur_sigma(1000), 20);
    assert_eq!(render::blur_sigma(2 * 500), render::blur_sigma(500) * 2);
    // And a 4K frame really does smear twice as far as a 1080p one — the point of scaling by height.
    assert_eq!(render::blur_sigma(2 * 1080), render::blur_sigma(1080) * 2);
}

/// S3: the floor binds below 200 px of height and stops binding just above it.
#[test]
fn p_eng_blursigma_s3_the_floor_binds_below_200px_and_stops_there() {
    // The crossover is height 200: 0.02 * 200 = 4.0 exactly equals BLUR_SIGMA_MIN, and `.max` is
    // inclusive, so 200 sits on the floor rather than above it.
    assert_eq!(render::blur_sigma(200), 4, "0.02*200 = 4.0 == the floor");
    // Below it the fraction would be smaller than any useful blur: 199*0.02 = 3.98, 100*0.02 = 2.0,
    // 10*0.02 = 0.2. All three are raised to the floor instead of going soft-but-useless or vanishing.
    assert_eq!(render::blur_sigma(199), 4, "would be 3.98");
    assert_eq!(render::blur_sigma(100), 4, "would be 2.0");
    assert_eq!(render::blur_sigma(10), 4, "would be 0.2");

    // Why the floor exists: at a small height a 4-px sigma is roughly 2% of the frame, which is what
    // keeps the edge reading as deliberate blur instead of a hard border around the picture.
    let smallest_small = (1..=200).map(render::blur_sigma).min().unwrap();
    assert_eq!(smallest_small, render::BLUR_SIGMA_MIN as i32);

    // Directly: every height at or above the crossover yields at least the floor...
    for height in (200..=4320i32).step_by(7) {
        assert!(
            f64::from(render::blur_sigma(height)) >= render::BLUR_SIGMA_MIN,
            "{height}: {} fell under the floor",
            render::blur_sigma(height)
        );
    }
    // ...and the floor has stopped binding by 201: 0.02 * 201 = 4.02, rounded up to 5. So 200 is the
    // last height where the answer is the floor and 201 is the first where the fraction wins.
    assert_eq!(render::blur_sigma(201), 5, "4.02 -> 5: the fraction now exceeds the floor");
    assert!(render::blur_sigma(201) > render::blur_sigma(200));
}

/// S4: the value the rule derives is the value the filter chain carries — not a copy that can drift.
#[test]
fn p_eng_blursigma_s4_the_chain_carries_exactly_the_derived_sigma() {
    // Ticked on: a 1920x1080 frame asks gblur for blur_sigma(1080) = 22.
    let blurred = chain_for((1920, 1080), true);
    let want = format!("gblur=sigma={}", render::blur_sigma(1080));
    assert_eq!(want, "gblur=sigma=22", "the rule's own answer at 1080p");
    assert!(
        blurred.iter().any(|filter| filter.contains(&want)),
        "the chain must carry {want}, got {blurred:?}"
    );

    // The pieces around it, named: the picture is split so one copy stays sharp, the other is scaled to
    // COVER the frame (increase + centred crop) and smeared, then the sharp copy is laid back over the
    // middle of it. Only the background copy gets the gblur.
    assert!(blurred.iter().any(|f| f == "split=2[bg][fg]"), "{blurred:?}");
    let bg = blurred
        .iter()
        .find(|f| f.contains("gblur="))
        .expect("the blurred branch must push a background filter");
    assert!(bg.starts_with("[bg]scale="), "the smeared copy is the background: {bg}");
    assert!(bg.contains("force_original_aspect_ratio=increase"), "{bg}");
    assert!(bg.contains("crop=1920:1080"), "{bg}");
    assert!(
        blurred
            .iter()
            .any(|f| f == "[fg][bg]overlay=(W-w)/2:(H-h)/2"),
        "the sharp copy is centred over the smeared backdrop: {blurred:?}"
    );
    // Exactly one element blurs: the foreground is never touched.
    assert_eq!(
        blurred.iter().filter(|f| f.contains("gblur")).count(),
        1,
        "only the backdrop is blurred: {blurred:?}"
    );

    // Unticked: no gblur anywhere, so the floor cannot sneak in behind a disabled control.
    let plain = chain_for((1920, 1080), false);
    assert!(
        plain.iter().all(|filter| !filter.contains("gblur")),
        "blurred_edges off means no blur at all: {plain:?}"
    );
    // Same at a small frame, where the floor is what would otherwise be doing the work.
    let small_plain = chain_for((320, 180), false);
    assert!(
        small_plain.iter().all(|filter| !filter.contains("gblur")),
        "{small_plain:?}"
    );
    // And a small frame with it on still gets the floored sigma, proving the chain goes through the rule
    // at every size rather than carrying a literal tuned for 1080p.
    let small_blurred = chain_for((320, 180), true);
    assert!(
        small_blurred
            .iter()
            .any(|f| f.contains("gblur=sigma=4")),
        "a 180px-high frame takes the floor of 4: {small_blurred:?}"
    );
}

/// S5: neighbours by home, not by number. 0.02 appears elsewhere in this tree meaning something else.
#[test]
fn p_eng_blursigma_s5_a_proportion_of_height_not_another_kinds_002() {
    // fx_zoom::HF_MIN is also 0.02 but is a Rect clamp — the smallest *height factor* worth a zoom,
    // i.e. a magnification bound, not a distance in pixels. Same digits, different kind of thing: they
    // must stay separate constants in separate modules.
    assert_eq!(naivepost::fx_zoom::HF_MIN, 0.02);
    assert_eq!(
        render::BLUR_SIGMA_FRACTION, naivepost::fx_zoom::HF_MIN,
        "equal by coincidence of magnitude, not by shared meaning"
    );
    assert_ne!(
        stringify!(render::BLUR_SIGMA_FRACTION),
        stringify!(naivepost::fx_zoom::HF_MIN),
        "two homes, one each"
    );
    // HF_MIN has no §10 row of its own (per its own comment), so the check that matters is that the
    // blur's row does not point at it.
    let blur_row = row_for("P.eng.blurSigma");
    assert!(blur_row.from.contains("blur_sigma"), "{}", blur_row.from);
    assert!(!blur_row.from.contains("HF_MIN"), "{}", blur_row.from);
    assert!(!blur_row.from.contains("fx_zoom"), "{}", blur_row.from);
    assert!(
        all_rows()
            .iter()
            .all(|param| !param.from.contains("fx_zoom::HF_MIN")),
        "HF_MIN is §A.1's number with no §10 row pointing at it"
    );

    // The render's other controls keep distinct homes too: the audio limiter and the loudness target are
    // separate rules and must not share a `from` with the blur or with each other.
    let limiter = row_for("P.eng.clipLimiter");
    let loudness = row_for("P.eng.loudness");
    assert_eq!(limiter.from, "produce_render::LIMITER");
    assert_eq!(loudness.from, "produce_render::LOUDNORM");
    let froms = [blur_row.from, limiter.from, loudness.from];
    for (i, a) in froms.iter().enumerate() {
        for (j, b) in froms.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "pairwise distinct homes: {froms:?}");
            }
        }
    }

    // Unrelated by construction, noted rather than asserted: `render_fx::ZOOM_GRID_FPS` (30.0) is the
    // grid zoompan samples the camera on, and `render_fx::ZOOM_DEPTH_CAP` (10.0, src/render_fx.rs:270)
    // is a magnification ceiling. Neither derives a blur width — the earlier claim that this sigma came
    // from render_fx was wrong and the source comment has been corrected.
    assert_eq!(naivepost::render_fx::ZOOM_GRID_FPS, 30.0);
    assert_eq!(naivepost::render_fx::ZOOM_DEPTH_CAP, 10.0);
    assert_ne!(naivepost::render_fx::ZOOM_GRID_FPS, render::BLUR_SIGMA_FRACTION);
}
