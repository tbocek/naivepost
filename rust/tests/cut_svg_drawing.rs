//! F3.5 SVG drawing by hand — `spec/06-effects.md` F3.5, steps S1–S5, with §A.5 of `spec/inventory/effects.md`.
//!
//! One rule per test, named after the step it belongs to. A drawing is a text with ink instead of words (§A.5), so
//! most of what these tests pin is a number or a sentence F3.4 already owns — asserted here at the svg's own call
//! site rather than re-derived: the box maths, the 0.3 s floor and fades and `clampFades` belong to
//! [`naivepost::fx_text`] / [`naivepost::cut_speed`], the arm's tail and the length default to
//! [`naivepost::fx_zoom`]. What is this flow's own: the file asked for *before* the arm, the refusal wording a
//! drawing gets, the middle rather than the lower third, the ffmpeg raster at 512 px in rgba, one raster per file and
//! one log line per failing file. Pixel comparisons go through a tolerance; `assert_eq!` is for constants, arrays and
//! strings.

use naivepost::cut::Fx;
use naivepost::fx_svg::{self, Chose, Press, Slot};
use naivepost::fx_text::{self, Box_};
use naivepost::fx_zoom;
use naivepost::params;
use naivepost::tools;

/// The preview is a 640 × 360 window on a 1920 × 1080 finished frame: the box's fractions are always of the FINISHED
/// frame (§06#1) while the hand drags in PREVIEW pixels, so keeping the two apart is what stops an assertion passing
/// for the wrong reason. Same pair `cut_text_caption.rs` uses.
const FRAME: (f64, f64) = (640.0, 360.0);

fn form(fields: (&str, f64)) -> fx_svg::Form {
    fx_svg::Form {
        t: 6.0,
        dur: fields.1,
        trans: fx_text::FADE_SECONDS,
        tout: fx_text::FADE_SECONDS,
        curve: fx_zoom::CURVE_CHOICES[0].to_string(),
        file: fields.0.to_string(),
        on: fx_svg::MIDDLE,
    }
}

// --- S1: press ▨ SVG -----------------------------------------------------------------------------------------------

/// S1 (`no line? → "click a track first — the drawing needs a moment to appear at"`): the red line is asked for
/// before the file is, so nobody spends a chooser trip on a second that is not there. The wording is this button's
/// own — §F3.1 lists the family of per-button refusals and zoom's says "the effect needs a moment to happen at", so
/// the two are asserted apart or they would quietly become one sentence.
#[test]
fn f3_5_s1_no_line_refuses_before_the_chooser() {
    assert_eq!(fx_svg::press(None), Press::Refused(fx_svg::NO_LINE));
    assert_eq!(
        fx_svg::NO_LINE,
        "click a track first \u{2014} the drawing needs a moment to appear at"
    );
    assert_ne!(fx_svg::NO_LINE, fx_zoom::NO_LINE, "a drawing asks in a drawing's words");

    // A line and the dialog opens — for the file, not yet for the gesture.
    assert_eq!(fx_svg::press(Some(0.0)), Press::OpenChooser);
    assert_eq!(fx_svg::press(Some(75.5)), Press::OpenChooser);

    // What it opens: §A.5's title, its `svg`-only filter, and the shared root-level cards folder. The directory is
    // another module's rule, so it is asserted through that module rather than re-spelled here.
    assert_eq!(fx_svg::CHOOSE_TITLE, "Choose a drawing to lay over the video");
    assert_eq!(fx_svg::CHOOSE_FILTER, "SVG drawing");
    let root = std::path::Path::new("/home/dev/eth");
    assert_eq!(fx_svg::chooser_dir(root), naivepost::layout::assets_dir(root));
    assert_eq!(
        fx_svg::chooser_dir(root),
        naivepost::cut_insert::chooser_dir(root)
    );
}

// --- S2: the file first, then the arm ------------------------------------------------------------------------------

/// S2 (`a file chosen? → no → "choose a drawing and it goes on the picture"`): cancelling places nothing — there is
/// no default drawing to fall back on, and arming a gesture with no ink would put a box down for an empty record. A
/// blank answer is the same answer as none: the file row can hold a space.
#[test]
fn f3_5_s2_the_file_is_asked_first_and_a_cancel_places_nothing() {
    assert_eq!(fx_svg::chose(Some("logo.svg")), Chose::Armed("logo.svg".into()));
    for nothing in [None, Some(""), Some("   ")] {
        assert_eq!(
            fx_svg::chose(nothing),
            Chose::Refused(fx_svg::NO_FILE),
            "{nothing:?} is no ink"
        );
    }
    assert_eq!(fx_svg::NO_FILE, "choose a drawing and it goes on the picture");

    // The refusal reaches the record too: a form applied with an empty file places nothing (§F3.5's last line).
    assert_eq!(
        fx_svg::apply(&form(("", 2.0))),
        Err(fx_svg::NO_FILE.to_string())
    );
}

/// S2b: the arm is not a toggle. ⊕ Zoom's and ❝ Text's dropdown entries disarm on a second press because there is
/// nothing new to place; ▨ SVG asks for a file in between, and the prototype clears its arm flag on this path with
/// the comment "never a toggle-off: a file was just chosen on purpose" — which is why `chose` takes no
/// `already_armed` at all. Asserted as the absence of that parameter's effect: any number of fresh files stays armed.
#[test]
fn f3_5_s2b_choosing_a_file_never_toggles_the_arm_off() {
    let mut armed = fx_svg::chose(Some("first.svg"));
    for file in ["second.svg", "third.svg"] {
        armed = match (armed, fx_svg::chose(Some(file))) {
            (Chose::Armed(_), again @ Chose::Armed(_)) => again,
            (before, _) => panic!("a chosen file must not disarm: {before:?}"),
        };
    }
    assert_eq!(armed, Chose::Armed("third.svg".into()));

    // The name shown on the lane and in the arm sentence: a card's query is part of which card it is, so it stays —
    // unlike `cut_insert::kind`, which has to strip it to find the extension.
    assert_eq!(fx_svg::base(""), "(no file)");
    assert_eq!(fx_svg::base("   "), "(no file)");
    assert_eq!(fx_svg::base("/x/tier.svg?S=Dust II"), "tier.svg?S=Dust II");
    assert_eq!(fx_svg::base("/home/dev/eth/assets/logo.svg"), "logo.svg");
}

/// S2 (`arms ("Drag the box <file> goes in — …")`): the whole gesture sentence, verbatim — the em dash after
/// "goes in", the semicolon (not a full stop) before "the drawing keeps its own shape", and the trailing space that
/// joins it to the shared tail. The file is named in it, so the arm says what is about to be drawn, not merely that
/// something is.
#[test]
fn f3_5_s3_the_arm_names_the_file() {
    assert_eq!(
        fx_svg::arm_head("logo.svg"),
        "Drag the box logo.svg goes in \u{2014} anywhere on the picture, any shape; the drawing keeps its own shape \
         inside it. A click puts one across the middle. "
    );

    // The tail is ⊕ Zoom's — as in F3.4, both flows say the same thing about the red line and the marked stretch, so
    // "3 s" is written down once for all of them. Asserted through the function that owns it.
    assert_eq!(
        fx_svg::arm_words("logo.svg", None),
        format!("{}{}", fx_svg::arm_head("logo.svg"), fx_zoom::arm_tail(None))
    );
    let marked = fx_svg::arm_words("tier.svg?S=Dust II", Some((12.0, 42.0)));
    assert!(marked.starts_with(&fx_svg::arm_head("tier.svg?S=Dust II")), "{marked}");
    // §A.4's own quotation of the shared tail: the app's seconds-and-length spelling of the stretch.
    assert!(marked.contains("00:12 \u{2013} 00:42, 30.0 s"), "{marked}");
}

/// S3 (§A.5 `default box centred {0.5,0.5,0.6,0.6}`; §1 `svg: middle`): where a drawing lands when no box was drawn.
/// The middle rather than the lower third a caption gets — a drawing is as often the subject as a decoration, and the
/// middle is the one place that is not a guess about which. [`Fx::centre`] already answers `{0.5, 0.5}` for an svg
/// that stored nothing; this pins that the two agree.
#[test]
fn f3_5_s3b_the_default_box_is_the_middle_not_the_lower_third() {
    assert_eq!(fx_svg::MIDDLE, Box_ { cx: 0.5, cy: 0.5, wf: 0.6, hf: 0.6 });
    assert_ne!(fx_svg::MIDDLE.cy, fx_text::LOWER_THIRD.cy);
    assert_ne!(fx_svg::MIDDLE.wf, fx_text::LOWER_THIRD.wf);

    let bare = Fx { kind: "svg".into(), ..Default::default() };
    assert_eq!(bare.centre(), (fx_svg::MIDDLE.cx, fx_svg::MIDDLE.cy));
    // …and a caption with no box still goes to the lower third, so one default did not eat the other.
    let caption = Fx { kind: "text".into(), ..Default::default() };
    assert_eq!(caption.centre(), (fx_text::LOWER_THIRD.cx, fx_text::LOWER_THIRD.cy));

    // S1's other rule (§A.5 "same as text"): the box is measured against the output frame, so the camera layer holds
    // its place while a drawing is placed — asserted through text's own answer, and against zoom's opposite one.
    assert!(fx_svg::camera_layer_stays_up());
    assert!(fx_text::camera_layer_stays_up());
    assert!(fx_zoom::whole_source_shown(true));
    assert!(!fx_zoom::whole_source_shown(false));

    // A click takes the middle, and a stretch is the length: 3 s from the red line otherwise.
    let clicked = fx_svg::place(naivepost::fx_text::BoxChoice::Click, "logo.svg", Some(12.0), None, FRAME).unwrap();
    assert_eq!((clicked.cx.unwrap(), clicked.cy.unwrap()), (0.5, 0.5));
    assert_eq!((clicked.wf.unwrap(), clicked.hf.unwrap()), (0.6, 0.6));
    let stretched =
        fx_svg::place(naivepost::fx_text::BoxChoice::Click, "logo.svg", Some(12.0), Some((30.0, 47.5)), FRAME)
            .unwrap();
    assert_eq!((stretched.t, stretched.dur), (30.0, 17.5));
    // With neither a selection nor a line there is no second to place it at: nothing is placed.
    assert!(fx_svg::place(naivepost::fx_text::BoxChoice::Click, "logo.svg", None, None, FRAME).is_none());
}

// --- S4: the form --------------------------------------------------------------------------------------------------

/// S4 (`Form "SVG at m:ss": file name + Choose…, Length, fades, Curve`): the title through [`tools::mm_ss`] — the
/// app's one spelling of a second — and the fields in the order the form reads them, with the file first because it
/// is what a drawing is made of. A valid form writes an svg record: `src` carries the file, `text` stays empty (§1
/// keeps the two apart on purpose), and Linear writes no `ease` key so an untouched file re-saves byte-identically.
#[test]
fn f3_5_s4_the_form() {
    assert_eq!(fx_svg::form_title(75.0), format!("SVG at {}", tools::mm_ss(75.0)));
    assert!(fx_svg::form_title(0.0).starts_with("SVG at "));
    assert_eq!(
        fx_svg::FORM_FIELDS,
        ["File", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"]
    );
    assert_eq!(fx_svg::CHOOSE_BUTTON, "Choose\u{2026}");

    let written = fx_svg::apply(&form(("assets/logo.svg", 2.0))).expect("a good form");
    assert_eq!((written.kind.as_str(), written.src.as_str()), ("svg", "assets/logo.svg"));
    assert_eq!((written.t, written.dur), (6.0, 2.0));
    // fades 0.3 — P.policy.effectDefaultFades, whose row reads text/svg 0.3.
    assert_eq!((written.trans, written.tout), (fx_text::FADE_SECONDS, fx_text::FADE_SECONDS));
    assert_eq!(fx_text::FADE_SECONDS, 0.3);
    // An svg's content is its file: the words field stays empty so a path can never read as text.
    assert_eq!(written.text, "");
    assert_eq!(written.ease, fx_zoom::curve_stored("Linear"));
    assert_eq!(written.ease, "");
    let (cx, cy, hf, wf) = fx_svg::MIDDLE.stored();
    assert_eq!(
        (written.cx.unwrap(), written.cy.unwrap(), written.hf.unwrap(), written.wf.unwrap()),
        (cx, cy, hf, wf)
    );

    // t/dur come from the selection when there is one, else 3 s from the line — P.policy.effectDefaultSeconds, whose
    // row names zoom, text and svg together, so it is one constant rather than a second 3.0.
    assert_eq!(fx_zoom::DEFAULT_SECONDS, 3.0);
    let from_line = fx_svg::place(naivepost::fx_text::BoxChoice::Click, "logo.svg", Some(9.0), None, FRAME).unwrap();
    assert_eq!((from_line.t, from_line.dur), (9.0, fx_zoom::DEFAULT_SECONDS));

    // A re-chosen file in the form wins over whatever the arm took, and it is stored trimmed: a trailing space typed
    // into the row is not part of anyone's filename.
    let rechosen = fx_svg::apply(&form(("  assets/other.svg  ", 2.0))).unwrap();
    assert_eq!(rechosen.src, "assets/other.svg");
}

/// S4 (`length ≥ 0.3`; `No file → not placed`): the floor is §6's "text/svg 0.3" — [`fx_text::MIN_SECONDS`], not a
/// second copy of it — so the bound itself is taken and a hundredth under it is refused naming that number. Two
/// overrunning fades are shared by §A.2's `clampFades`, the same arithmetic F3.4 pins at 0.15/0.15 for two 0.3 s
/// defaults over a 0.3 s band: three forms cannot disagree about how a band is split.
#[test]
fn f3_5_s4b_no_file_is_not_placed_and_a_short_length_names_its_floor() {
    assert_eq!(fx_text::MIN_SECONDS, 0.3);
    for blank in ["", " ", "\t"] {
        assert_eq!(
            fx_svg::apply(&form((blank, 2.0))),
            Err(fx_svg::NO_FILE.to_string()),
            "{blank:?} is no ink"
        );
    }

    assert!(fx_svg::apply(&form(("logo.svg", fx_text::MIN_SECONDS))).is_ok());
    // The floor spelled as §10 spells a number — no unit on `0.3` — because the message quotes the constant.
    let short = fx_svg::apply(&form(("logo.svg", 0.29))).unwrap_err();
    assert!(short.contains("0.29 s"), "{short}");
    assert!(short.contains("(0.3) s"), "{short}");

    let tight = fx_svg::apply(&form(("logo.svg", 0.3))).unwrap();
    // Equal asks share equally: half of the band each, which is what §A.2's `share` computes.
    assert_eq!((tight.trans, tight.tout), (0.15, 0.15));
    assert!(tight.trans + tight.tout <= 0.3 + 1e-12);

    // A one-sided overrun is the same rule with different numbers: 3.0 + 0.5 over a 1 s band shares it in that ratio,
    // and neither fade goes negative.
    let one_sided = fx_svg::apply(&fx_svg::Form {
        dur: 1.0,
        trans: 3.0,
        tout: 0.5,
        ..form(("logo.svg", 1.0))
    })
    .unwrap();
    assert!(one_sided.trans > one_sided.tout);
    assert_eq!((one_sided.trans, one_sided.tout), (6.0 / 7.0, 1.0 / 7.0));
}

// --- S5: the preview's raster ---------------------------------------------------------------------------------------

/// S5 (`Preview raster via ffmpeg, 512 px, transparent`): how big and in what format the preview's copy is rendered.
/// The librsvg input options set the size the vector is drawn at — librsvg otherwise uses the document's declared
/// size — and rgba keeps a transparent background transparent instead of laying a black card over the video. The
/// number is catalogued once, as `effects.previewRasterPx`: §10 lists it only as an implicit constant (`svgPreviewPx`),
/// so the id takes the bare prefix like `effects.aspectHoldSeconds` does.
#[test]
fn f3_5_s5_the_preview_raster_is_512_px_and_transparent() {
    assert_eq!(fx_svg::PREVIEW_RASTER_PX, 512.0); // effects.previewRasterPx

    let args = fx_svg::ffmpeg_raster("ffmpeg", "/x/assets/logo.svg", "out.png");
    for flag in [
        "-width", "512", "-height", "512", "-keep_ar", "1", "-pix_fmt", "rgba", "-f", "image2", "-c:v", "png",
    ] {
        assert!(args.iter().any(|arg| arg == flag), "{flag} missing from {args:?}");
    }
    assert_eq!(args.first().map(String::as_str), Some("ffmpeg"));
    // The file is the input and the output comes last: one vector, no shell string to get wrong.
    let at = args.iter().position(|arg| arg == "/x/assets/logo.svg").expect("the drawing is the input");
    assert_eq!(args[at - 1], "-i");
    assert_eq!(args.last().map(String::as_str), Some("out.png"));

    // Catalogued once, with the same number the code reads.
    let row = params::cut()
        .into_iter()
        .find(|param| param.id == "effects.previewRasterPx")
        .expect("the raster's size is catalogued");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), fx_svg::PREVIEW_RASTER_PX);
    assert_eq!(row.from, "fx_svg::PREVIEW_RASTER_PX");
    let listed: Vec<String> = params::cut().into_iter().map(|param| param.id.to_string()).collect();
    assert_eq!(listed.iter().filter(|id| *id == "effects.previewRasterPx").count(), 1, "catalogued twice");
}

/// S5 (`cached per file`; §A.5 `failure logged once`): one raster per FILE rather than one per effect — the same logo
// in six places is one ffmpeg run — and a drawing that cannot be read says so once, because the preview asks again on
/// every frame it draws and a log full of one line hides what else broke.
#[test]
fn f3_5_s5b_one_raster_per_file_and_one_log_line_per_file() {
    let mut rasters = fx_svg::Rasters::default();
    assert!(rasters.is_empty());

    // First ask takes on the job; a second, while it is still running, does not start another.
    assert_eq!(rasters.request("logo.svg"), Slot::NeedsRaster);
    assert_eq!(rasters.request("logo.svg"), Slot::Busy);

    // A failure is remembered and never retried — a broken svg does not mend between preview frames — and it is
    // printed once, naming the drawing as `base` spells it rather than by its long path.
    let first = rasters.failure_line("/x/assets/logo.svg", "ffmpeg drew nothing");
    assert_eq!(first.as_deref(), Some("the drawing logo.svg cannot be shown: ffmpeg drew nothing"));
    assert_eq!(rasters.failure_line("/x/assets/logo.svg", "ffmpeg drew nothing"), None);
    assert_eq!(rasters.request("/x/assets/logo.svg"), Slot::Failed);

    // A different file is a different question and still gets its line. (Three entries: the logo was asked about
    // under its bare name before it failed under its path, and the cache keys on the path as given — two paths are
    // two files until something says otherwise.)
    let other = rasters.failure_line("badge.svg", "no such file");
    assert_eq!(other.as_deref(), Some("the drawing badge.svg cannot be shown: no such file"));
    assert_eq!(rasters.len(), 3);

    // Two effects on one logo share one entry, and a finished raster is Ready for both of them.
    let mut shared = fx_svg::Rasters::default();
    assert_eq!(shared.request("logo.svg"), Slot::NeedsRaster);
    assert_eq!(shared.request("logo.svg"), Slot::Busy);
    shared.done("logo.svg");
    assert_eq!(shared.request("logo.svg"), Slot::Ready);
    assert_eq!(shared.request("logo.svg"), Slot::Ready);
    assert_eq!(shared.len(), 1, "one raster per file");

    // A file that succeeded is not asked for again even after another file fails.
    shared.done("second.svg");
    assert_eq!(shared.request("second.svg"), Slot::Ready);
}

/// S5 (`the drawing keeps its own shape inside it`): the rectangle a drawing is drawn into — as large as it can be
/// inside the box, aspect kept, centred on the axis the fit left short. Never bigger than the box on either axis, and
/// an unknown aspect returns the box rather than a zero-sized draw for a drawer to guard against. One function for
/// preview and render, which is [`fx_text::preview_is_the_render`]'s bargain restated for ink.
#[test]
fn f3_5_s5c_the_drawing_keeps_its_own_shape_inside_the_box() {
    // A wide drawing in a square box: full width, half as tall, hanging equally above and below.
    let (x, y, w, h) = fx_svg::fit_inside(100.0, 100.0, 2.0);
    assert!((w - 100.0).abs() < 1e-9 && (h - 50.0).abs() < 1e-9, "{w}x{h}");
    assert!(x.abs() < 1e-9 && (y - 25.0).abs() < 1e-9, "({x}, {y})");

    // A tall one: full height, half as wide, centred left-to-right.
    let (tx, ty, tw, th) = fx_svg::fit_inside(100.0, 100.0, 0.5);
    assert!((tw - 50.0).abs() < 1e-9 && (th - 100.0).abs() < 1e-9, "{tw}x{th}");
    assert!((tx - 25.0).abs() < 1e-9 && ty.abs() < 1e-9, "({tx}, {ty})");

    // A square drawing in a square box fills it; in a wide box it is full height and the short axis is the WIDTH, so
    // the leftover air is left and right — 100 × 100 of the 200's width, 50 px either side.
    let (sx, sy, sw, sh) = fx_svg::fit_inside(100.0, 100.0, 1.0);
    assert!((sw - 100.0).abs() < 1e-9 && (sh - 100.0).abs() < 1e-9 && sx.abs() < 1e-9 && sy.abs() < 1e-9);
    let (wx, wy, ww, wh) = fx_svg::fit_inside(200.0, 100.0, 1.0);
    assert!((ww - 100.0).abs() < 1e-9 && (wh - 100.0).abs() < 1e-9, "{ww}x{wh}");
    assert!((wx - 50.0).abs() < 1e-9 && wy.abs() < 1e-9, "({wx}, {wy})");

    // Never over the box on either axis, whatever comes in — and the aspect survives every one of them.
    for (bw, bh, aspect) in [(640.0, 360.0, 1.777_8), (360.0, 640.0, 0.562_5), (100.0, 1.0, 4.0), (1.0, 100.0, 0.25)] {
        let (_, _, w, h) = fx_svg::fit_inside(bw, bh, aspect);
        assert!(w <= bw + 1e-9 && h <= bh + 1e-9, "{w}x{h} in {bw}x{bh}");
        assert!((w / h - aspect).abs() <= aspect * 1e-9, "aspect lost: {w}/{h} vs {aspect}");
    }

    // Unknown shape, or no box to fit into: the box itself, not a zero-sized draw.
    let (ux, uy, uw, uh) = fx_svg::fit_inside(100.0, 80.0, 0.0);
    assert!((uw - 100.0).abs() < 1e-9 && (uh - 80.0).abs() < 1e-9 && ux.abs() < 1e-9 && uy.abs() < 1e-9);
    let (_, _, zw, zh) = fx_svg::fit_inside(0.0, 0.0, 1.0);
    assert!(zw.abs() < 1e-9 && zh.abs() < 1e-9);
}
