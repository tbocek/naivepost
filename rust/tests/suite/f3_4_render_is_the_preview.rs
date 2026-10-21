// F3.4 S4 (`same function for preview and render`) — the RENDER half of that bargain.
//
// The preview side has had it since its own round: `ui::cut_preview_caption` asks `fx_text` for the size, the lines,
// the baselines and the edge's thickness. The render side did not: `render_fx::title_svg` invented `font-size = h/12`,
// one unwrapped `<text>` and a fixed 12-px stroke, so a caption that wrapped to three lines on the panel came out of
// the video as one unscaled line with a halo nobody chose. These checks pin the sheet against the same doors the
// preview reads, so the two halves cannot drift apart again without one of them failing.
//
// The last check is the wire: this item opens on a click (Effect ▾ → ❝ Text), so it places a caption through the real
// dropdown row, the preview's zero-travel press and the form's Apply BUTTON, then renders THAT record and compares the
// sheet with what the preview would paint. One application, one entry `#[test]`, no sleeps — the shape
// `f3_4_text_box_preview_widgets.rs` uses.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

use adw::prelude::*;
use naivepost::cut::{Cut, EffectKind, Fx, Seg};
use naivepost::fx_text;
use naivepost::produce_render::Clip;
use naivepost::render_fx;
use naivepost::timeline::Recording;
use naivepost::ui;

use crate::common;
use common::{release_last_window, settle};

/// The output frame these checks render at — the pair `sec_06_effects_4_render.rs` builds its clips at too.
const FRAME: (i32, i32) = (1920, 1080);

/// Words long enough that the lower third cannot hold them in one line: exactly the case the old render got wrong.
const WORDS: &str = "Ethereum in one lecture, covering gas, wallets and Layer 2 rollups";

/// A longer paragraph for the smaller box: the same words only break into two lines across a wide lower third, so the
/// narrow box's check has to say more of them to be a wrapped case too.
const MANY_WORDS: &str = "Ethereum in one lecture, covering gas, wallets, seed phrases, rollups, bridges, \
                          audits and why mainnet still matters to anyone who sends money";

/// Where the caption the widgets place stands, inside [`tape_cut`]'s one scene and filmed by [`declare_tape`]'s take.
const LINE: f64 = 20.0;

static RAN_WIRE: AtomicBool = AtomicBool::new(false);
static STOP_REASON: LazyLock<std::sync::Arc<std::sync::Mutex<Option<String>>>> =
    LazyLock::new(|| std::sync::Arc::new(std::sync::Mutex::new(None)));

/// Run one widget check and remember why it stopped, rather than letting a panic take the binary down silently.
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

/// Pull an attribute's value out of the sheet — read back off the document, never recomputed from the builder.
fn attr(svg: &str, name: &str) -> String {
    let key = format!("{name}=\"");
    let from = svg.find(&key).unwrap_or_else(|| panic!("no {name:?} in the sheet: {svg}"));
    let rest = &svg[from + key.len()..];
    rest.split('"').next().unwrap_or_default().to_string()
}

/// Every baseline the sheet draws, in order.
fn text_lines(svg: &str) -> Vec<(String, f64)> {
    svg.split("<text ")
        .skip(1)
        .map(|chunk| {
            let y = attr(chunk, "y").parse::<f64>().expect("a baseline is a number");
            let body = chunk.split('>').nth(1).expect("a text element has content");
            (body.split('<').next().unwrap_or_default().to_string(), y)
        })
        .collect()
}

/// A caption record: words, four seconds, and optionally the box a hand drew.
fn caption(words: &str, box_: Option<(f64, f64, f64, f64)>) -> Fx {
    let mut effect = Fx {
        kind: "text".into(),
        t: 10.0,
        dur: 4.0,
        trans: 0.3,
        tout: 0.3,
        text: words.to_string(),
        ..Default::default()
    };
    if let Some((cx, cy, wf, hf)) = box_ {
        effect.cx = Some(cx);
        effect.cy = Some(cy);
        effect.wf = Some(wf);
        effect.hf = Some(hf);
    }
    effect
}

/// S4 (`preview = render`): the sheet's size, its line count and every baseline ARE `fx_text`'s answers for the box,
/// not a near-duplicate of them. More than one line here — one unscaled line was the bug.
fn f3_4_s4_the_render_sheet_uses_the_shared_fitter() {
    // The lower third of an HD frame, spelled the way the render spells it: through its own box door.
    let (bw, bh) = {
        let (_x, _y, w, h) = naivepost::produce_fx_chain::box_px(&caption(WORDS, None), FRAME);
        (w, h)
    };
    let (size, lines) = fx_text::fit(WORDS, bw as f64, bh as f64);
    assert!(lines.len() >= 2, "these words must wrap here or the check proves nothing: {lines:?}");
    let svg = render_fx::title_svg(WORDS, bw, bh);
    assert_eq!(attr(&svg, "font-size"), format!("{size:.3}"), "the fitted size, asked of the shared fitter");
    let drawn = text_lines(&svg);
    assert_eq!(drawn.len(), lines.len(), "one <text> per fitted line: {drawn:?} vs {lines:?}");
    let want = fx_text::baselines(0.0, bh as f64, size, lines.len());
    for (index, (_, y)) in drawn.iter().enumerate() {
        assert!(
            (y - want[index]).abs() < 0.001,
            "line {index} sits at {y}, not the shared baseline {}",
            want[index]
        );
    }
    // And the preview agrees on the break: same words, same box fractions, same number of lines.
    let (_, _, preview_lines) = ui::cut_preview_caption::layout(&caption(WORDS, None), (FRAME.0 as f64, FRAME.1 as f64));
    assert_eq!(preview_lines.len(), drawn.len(), "the preview breaks the words differently: {preview_lines:?}");
}

/// S4 (`a dark dilated edge`, `bold sans-serif`): the stroke is the shared constant scaled to the type, never a fixed
/// 12; the face is the one the preview selects; and user words are escaped, because they are user words.
fn f3_4_s4_the_render_edge_scales_with_the_type() {
    let big = (1536i32, 173i32); // about the lower third of an HD frame
    let (big_size, big_lines) = fx_text::fit(MANY_WORDS, big.0 as f64, big.1 as f64);
    assert!(big_lines.len() >= 3, "the wide box must hold a wrapped case: {big_lines:?}");
    let sheet = render_fx::title_svg(MANY_WORDS, big.0, big.1);
    assert_eq!(attr(&sheet, "stroke-width"), format!("{:.3}", fx_text::render_stroke_width(big_size)));
    assert_ne!(attr(&sheet, "stroke-width"), "12", "the fixed halo is gone");

    // A hopeless box lands on the 7 pt floor, and its edge shrinks with it — the tell the fixed 12 could never pass.
    let tiny = (60i32, 24i32);
    let (small_size, small_lines) = fx_text::fit(MANY_WORDS, tiny.0 as f64, tiny.1 as f64);
    assert_eq!(small_size, fx_text::MIN_POINTS, "a small box takes the floor");
    assert!(small_lines.len() >= 3, "and it still breaks the words: {} lines", small_lines.len());
    let thin = render_fx::title_svg(MANY_WORDS, tiny.0, tiny.1);
    assert_eq!(attr(&thin, "stroke-width"), format!("{:.3}", fx_text::render_stroke_width(small_size)));
    assert!(attr(&thin, "stroke-width").parse::<f64>().unwrap() < 4.0, "and it is far under the old 12");

    assert!(sheet.contains("font-family=\"sans-serif\""), "{sheet}");
    assert!(sheet.contains("font-weight=\"bold\""), "§S4 says bold sans: {sheet}");
    assert!(sheet.contains("stroke-linejoin=\"round\""), "round-joined: {sheet}");
    // §4's paint order: outline written before fill, or the letters eat their own edge on a bright picture.
    assert!(
        sheet.find("stroke=\"black\"").expect("the outline") < sheet.find("fill=\"white\"").expect("the letters"),
        "{sheet}"
    );
    assert!(!sheet.contains("<rect"), "transparent, no background plate: {sheet}");

    // Words are typed by a person: `&` and `<` go in escaped, never as markup.
    let quoted = render_fx::title_svg("gas & <rollups>", 1536, 173);
    assert!(quoted.contains("gas &amp; &lt;rollups&gt;"), "{quoted}");
    let bare = quoted.split("<text").nth(1).expect("one line");
    assert!(!bare.contains(" & "), "a bare ampersand survived: {quoted}");
}

/// S4 + §4's looped input: empty words still get a sheet. ffmpeg has to be handed a file it can open, so the box is
/// there and transparent rather than missing — but nothing is printed on it.
fn f3_4_s4_no_words_is_a_blank_sheet_not_a_missing_one() {
    let sheet = render_fx::title_svg("   ", 1536, 173);
    assert!(sheet.starts_with("<svg"), "{sheet}");
    assert!(sheet.contains("width=\"1536\" height=\"173\""), "{sheet}");
    assert!(!sheet.contains("<text"), "nothing to say, nothing drawn: {sheet}");
}

// --- the wire ------------------------------------------------------------------------------------------------------

/// A cut with one scene long enough to hold a caption at [`LINE`], so the placement door finds its ground.
fn tape_cut() -> Cut {
    Cut {
        segs: vec![Seg { s: 0.0, e: 300.0, cam: 0, ..Default::default() }],
        ..Default::default()
    }
}

/// Declare the take the way the page declares one, so the preview has footage (§F2.1's "nothing was filmed" refusal
/// otherwise stops the transport and the placement).
fn declare_tape() {
    ui::set_session_recordings(&[Recording { base: "session-tape".to_string(), start: 0.0, end: 300.0 }]);
}

/// Place a caption the way a person does — arm ❝ Text through the dropdown row, land a box with a zero-travel press
/// (that IS the click, so it takes the lower third), type into the form, press its Apply BUTTON — and hand back the
/// record Apply wrote.
fn place_caption(window: &adw::ApplicationWindow, words: &str) -> Fx {
    ui::note_place(true);
    ui::set_line_position(window, naivepost::cut_line::LinePos { t: LINE });
    ui::set_playhead(window, LINE);
    let armed = ui::press_effect_item(window, EffectKind::Text);
    assert!(ui::text_armed(), "❝ Text did not arm -- {armed}");
    let landed = ui::text_drag_ended(window, (40.0, 40.0), (40.0, 40.0));
    assert!(ui::text_form_open().is_some(), "landing the box drew no form -- {landed}");
    settle();
    ui::text_type_words(window, words);
    // The button, fired the way a click fires it, so the wire is what is proven and not the door behind it.
    common::click(window, "text-apply-button");
    let cut_ = ui::review_cut_of(window);
    let placed = cut_
        .fx
        .iter()
        .rev()
        .find(|fx| fx.kind == "text")
        .cloned()
        .unwrap_or_else(|| panic!("Apply put no caption on the cut"));
    assert_eq!(placed.text, words, "Apply stored different words");
    placed
}

/// S4 through the user's path: the caption the click-and-form produced is rendered, and the sheet comes back fitted by
/// the same fitter the preview paints from. The preview measures its box against the PANEL and the render against the
/// OUTPUT FRAME — what §S4 pins is the one function, so what is compared is the size each side asks `fx_text` for over
/// its own box, and the number of lines both agree the words break into.
fn f3_4_s4_a_caption_placed_by_the_widgets_renders_fitted(app: &adw::Application) {
    let window = common::cut_page(app, &tape_cut());
    declare_tape();
    let placed = place_caption(&window, WORDS);
    assert!(
        (placed.wf, placed.hf) == (Some(fx_text::LOWER_THIRD.wf), Some(fx_text::LOWER_THIRD.hf)),
        "a click stores the lower third's own box: {:?} x {:?}",
        placed.wf,
        placed.hf
    );

    // Render it: the same door a run walks, writing its title sheet into its own scratch folder.
    let scratch = common::temp_root("f34-render-sheet");
    let clip = Clip {
        no: 1,
        seg_s: placed.t - 1.0,
        on_screen: 10.0,
        rate: 1.0,
        frame: FRAME,
        src: FRAME,
        on_recording: true,
        scratch: scratch.clone(),
        fx: vec![placed.clone()],
        ..Default::default()
    };
    let built = naivepost::produce_fx_chain::for_clip(&clip, &clip.fx, clip.src, &scratch);
    let (_path, sheet) = built.title_files.first().expect("the caption's sheet was written for ffmpeg");
    std::fs::write(scratch.join("title.svg"), sheet).expect("and it goes to disk before ffmpeg is asked for it");
    let read_back = std::fs::read_to_string(scratch.join("title.svg")).expect("and is really there");

    // The box the render measured: its own door, the frame's fractions of the lower third. `fx_text::fit` over THAT
    // box is the one layout both halves ask — same module as `fx_text::MIN_POINTS`, same answer as the preview's.
    let (bw, bh) = {
        let (_x, _y, w, h) = naivepost::produce_fx_chain::box_px(&placed, FRAME);
        (w, h)
    };
    let (size, lines) = fx_text::fit(&placed.text, bw as f64, bh as f64);
    assert!(lines.len() >= 2, "the placed caption does not even wrap in the render's box: {lines:?}");
    assert_eq!(attr(&read_back, "font-size"), format!("{size:.3}"), "the render asks the shared fitter");
    assert_eq!(text_lines(&read_back).len(), lines.len(), "and prints every line it returns");
    assert_eq!(attr(&read_back, "stroke-width"), format!("{:.3}", fx_text::render_stroke_width(size)));

    // The preview side, over ITS box: same fitter, same break of the same words.
    let fit_panel = ui::cut_preview_frame::fit_size_of(&window, naivepost::ui::window::PREVIEW_PANEL_W, naivepost::ui::window::PREVIEW_PANEL_H);
    let (_, preview_size, preview_lines) = ui::cut_preview_caption::layout(&placed, fit_panel);
    assert!(preview_size > 0.0 && !preview_lines.is_empty(), "the preview painted nothing");
    assert_eq!(preview_lines.len(), lines.len(), "the two halves break the words differently: {preview_lines:?} vs {lines:?}");
    RAN_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn round(app: &adw::Application) {
    step!(
        "f3_4_s4_a_caption_placed_by_the_widgets_renders_fitted",
        f3_4_s4_a_caption_placed_by_the_widgets_renders_fitted(app)
    );
    release_last_window();
}

/// The one entry test: the widget check runs inside this single `Application::run`
/// (`common::one_app_round` quits as soon as the round returns), and the logic checks above ran beside it.
#[test]
fn f3_4_s4_the_render_lays_words_as_the_preview_does() {
    let root = std::env::temp_dir().join(format!("np-f34-render-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    f3_4_s4_the_render_sheet_uses_the_shared_fitter();
    f3_4_s4_the_render_edge_scales_with_the_type();
    f3_4_s4_no_words_is_a_blank_sheet_not_a_missing_one();
    common::one_app_round(round);

    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(RAN_WIRE.load(Ordering::SeqCst), "the wire check never ran");
}
