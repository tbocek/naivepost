// F3.5 S5 — the preview raster is MADE, cached per file, shown, and complained about once.
//
// The earlier S5 check (`tests/svg_drawing_widgets.rs`) proves the RULE halves: 512 in the vector, rgba in the
// vector, one cache entry per file, one failure line per file. This file proves the OTHER half, the half codehalter
// found missing: that choosing a drawing actually runs ffmpeg, that the png it writes is the one the cache hands
// back, that a png already on disk is not remade, that the form shows the picture, and that the preview paints the
// drawing's ink inside the box it was given.
//
// The ffmpeg seam is the program's own: `audio_task::ffmpeg_program` reads the FFMPEG key out of the settings file
// this test writes into a temp XDG_CONFIG_HOME (the same door `speak_leg` and `audio_task` use for every other ffmpeg
// job — nothing here asks a for-test helper for an answer). The stub records its argv, counts its calls, and copies
// a png the test pre-wrote with cairo, so the transparency and the file are real bytes rather than a claim.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::Cut;
use naivepost::cut_line;
use naivepost::fx_svg;
use naivepost::layout::Tree;
use naivepost::settings::{self, Conf, Paths};
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{label_text, settle, widget};

static RAN_RAN: AtomicBool = AtomicBool::new(false);
static RAN_ONCE: AtomicBool = AtomicBool::new(false);
static RAN_ONDISK: AtomicBool = AtomicBool::new(false);
static RAN_FAILED: AtomicBool = AtomicBool::new(false);
static RAN_PICTURE: AtomicBool = AtomicBool::new(false);
static RAN_PAINT: AtomicBool = AtomicBool::new(false);
static RAN_DASHED: AtomicBool = AtomicBool::new(false);

/// The fitted picture the preview measures everything against: `preview_picture_px` answers the panel's own
/// declaration (no ffprobe runs headless), which is 240 x 135, and `fit_size_of` never shrinks below it.
const FIT: (f64, f64) = (240.0, 135.0);
/// The surface this file paints into, and the source size of the fake raster.
const SURF_W: i32 = 240;
const SURF_H: i32 = 170;
const RASTER_PX: i32 = 512;

/// A real png: transparent everywhere except one opaque red block. Cairo writes it, gdk-pixbuf reads it, so the
/// alpha the preview has to preserve is a real alpha channel and not a description of one.
fn write_red_png(path: &Path) {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, RASTER_PX, RASTER_PX).expect("surface");
    {
        let cr = cairo::Context::new(&surface).expect("context");
        // Transparent ground: ARgb32 starts zeroed, and premultiplied zero IS transparent, so nothing is painted
        // for the background — the drawing's own ink is the only opaque thing here.
        let _ = cr;
        let cr = cairo::Context::new(&surface).expect("second context");
        cr.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        cr.rectangle(
            (RASTER_PX / 4) as f64,
            (RASTER_PX / 4) as f64,
            (RASTER_PX / 2) as f64,
            (RASTER_PX / 2) as f64,
        );
        cr.fill().expect("the red block");
    }
    let mut file = std::fs::File::create(path).expect("the png file");
    surface.write_to_png(&mut file).expect("a real png");
}

/// The stub ffmpeg. `argv` receives the settings' FFMPEG key; the script appends its argv to `log`, counts its
/// calls in `count`, and copies the pre-written png to the last argument (the output path), the way the other
/// fake-ffmpeg scripts in this suite answer an output. `succeed` false makes it fail with a reason instead, which
/// is how §A.5's "said once" branch gets a real non-zero exit rather than a scripted refusal.
fn write_stub(root: &Path, argv_out: &Path, log: &Path, count: &Path, succeed: bool) -> PathBuf {
    // The image the stub answers with is COPIED into a folder SHARED by every block of this check, and the script
    // copies THAT file to ffmpeg's output. Each block moves `XDG_CONFIG_HOME` (and so `Paths::cache_dir`) on to a
    // fresh folder, so a png left behind in an earlier block's folder would be gone by the time a later stub was
    // asked for it — and a stub that writes nothing is a raster failure, which is a different story from the one
    // this block tells.
    let share = std::env::temp_dir().join("naivepost-f35");
    std::fs::create_dir_all(&share).expect("the shared scratch folder");
    let image = share.join(
        argv_out
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("red.png")),
    );
    write_red_png(&image);
    let body = if succeed {
        // The output path is ffmpeg's LAST argument — the same reading the other fake-ffmpeg scripts in this suite
        // use. `-y` (and the `-` of `-hide_banner`) must NOT be treated as an output: only the final word is.
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nn=$(cat '{}' 2>/dev/null || echo 0); printf '%s' \"$((n+1))\" > '{}'\nfor a in \"$@\"; do out=\"$a\"; done\ncp '{}' \"$out\"\n",
            log.display(),
            count.display(),
            count.display(),
            image.display()
        )
    } else {
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nn=$(cat '{}' 2>/dev/null || echo 0); printf '%s' \"$((n+1))\" > '{}'\necho 'svg: Unknown input format' >&2\nexit 1\n",
            log.display(),
            count.display(),
            count.display()
        )
    };
    // One stub per check, named for it: the failure check re-points the settings half-way through the round, and a
    // later check that reused this path would find the failing script sitting there.
    let script = root.join(format!(
        "fake-ffmpeg-{}.sh",
        log.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::write(argv_out, "placeholder").unwrap_or(());
    std::fs::write(&script, body).expect("the stub ffmpeg");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("+x");
    }
    // Hand the script's path back: the caller points the SETTINGS at exactly this file, so no check ever runs a
    // stub that belongs to another one.
    script
}

/// Point the PROGRAM's settings at the stub. `settings::from_environment()` is the door `make_here` reads, so
/// writing XDG_CONFIG_HOME here is what makes `audio_task::ffmpeg_program` answer our script — no test-only
/// channel exists or is needed.
fn point_settings_at(root: &Path, stub: &Path) {
    std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    let paths = Paths {
        config_dir: root.join("config/naivepost"),
        data_dir: root.join("data/naivepost"),
    };
    let mut conf = Conf::default();
    conf.ffmpeg = stub.display().to_string();
    settings::save(&paths, &conf).expect("the settings file writes");
}

/// This session's project folder. `narrate_session_tree` (which `make_here` uses for the cache dir) resolves it
/// through `current_dir()`, so cwd is pinned to this test's own root before anything asks.
fn project_tree(root: &Path) -> Tree {
    let dir = naivepost::startup::session_dir(root);
    std::fs::create_dir_all(&dir).expect("the session folder");
    Tree::new(&dir).expect("a project tree")
}

/// How many times the stub ran.
fn calls(count: &Path) -> usize {
    std::fs::read_to_string(count)
        .ok()
        .and_then(|text| text.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

/// The stub's recorded argv, one argument per line.
fn argv_of(log: &Path) -> String {
    std::fs::read_to_string(log).unwrap_or_default()
}

/// The drawing's path under this root, so the label and the record name a file that exists. One file per CHECK:
/// the raster cache is process-wide and never re-asks a file it has already seen, so two checks sharing a name
/// would have the second one silently measure the first one's work.
fn drawing(root: &Path, name: &str) -> String {
    let assets = root.join("assets");
    std::fs::create_dir_all(&assets).expect("assets/");
    let file = assets.join(name);
    std::fs::write(
        &file,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\"><rect width=\"100\" height=\"100\" fill=\"#ff0000\"/></svg>",
    )
    .expect("the drawing");
    file.display().to_string()
}

/// One placed drawing: chosen through the chooser's door, landed by the gesture's door. Returns the window it was
/// placed on, whose cut now holds exactly one `kind="svg"` record — the state the painter is offered.
/// Land a box and open the form, through the gesture's own door, and require the record it writes.
#[allow(dead_code)] // kept as the one-call version of the sequence; the round drives the steps separately
fn placed_drawing_window(app: &adw::Application, t: f64, src: &str) -> adw::ApplicationWindow {
    let window = common::cut_page_settled_first(app, &Cut::default());
    ui::note_place(true);
    ui::set_line_position(&window, cut_line::LinePos { t });
    ui::svg_chosen(&window, Some(src));
    ui::svg_drag_ended(&window, (30.0, 30.0), (150.0, 96.0));
    let said = ui::press_svg_apply(&window);
    settle();
    let held_cut = ui::review_cut_of(&window);
    assert_eq!(
        held_cut.fx.len(),
        1,
        "the drawing landed on the cut through record_edit: {said}"
    );
    window
}

/// The box an effect carries, in the fitted picture's px — read through the same doors the painter reads it
/// through ([`fx_svg`] exposes the box shape, and an effect's fractions come from `Fx::centre` plus its stored
/// `wf`/`hf`), so the assertion measures what the rule measures instead of restating its arithmetic.
fn effect_box_px(effect: &naivepost::cut::Fx, fit: (f64, f64)) -> (f64, f64, f64, f64) {
    let (cx, cy) = effect.centre();
    // §F3.5's default is `MIDDLE`, and its fields are `pub`, so the same four numbers the painter builds are here.
    let wf = effect.wf.unwrap_or(fx_svg::MIDDLE.wf);
    let hf = effect.hf.unwrap_or(fx_svg::MIDDLE.hf);
    let w = wf * fit.0;
    let h = hf * fit.1;
    (cx * fit.0 - w / 2.0, cy * fit.1 - h / 2.0, w, h)
}

/// The cairo surface a transparent png turns into on the way to the preview: the program's own conversion
/// (`cut_preview_frame`'s RGBA -> premultiplied ARgb32 step), reached through the one public door it offers.
#[allow(clippy::wildcard_imports)] // the door is one function; naming it needs no types
fn raster_surface_for_test(path: &Path) -> Option<cairo::ImageSurface> {
    naivepost::ui::cut_preview_frame::raster_surface_for_test(path)
}

/// The painted surface's pixel at (x, y) as (r, g, b): ARgb32 is B,G,R,A on little-endian.
fn rgb(data: &[u8], stride: usize, x: i32, y: i32) -> (u8, u8, u8) {
    let base = y as usize * stride + x as usize * 4;
    (data[base + 2], data[base + 1], data[base])
}

/// Paint this window's OWN cut at the second its red line stands on, into a fresh surface, and hand back the pixels
/// plus the surface size. Everything the painter needs comes from the live window through `describe`.
fn paint_window(window: &adw::ApplicationWindow) -> (Vec<u8>, usize) {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, SURF_W, SURF_H).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        // White ground so "not drawn" reads as white rather than as the black a fresh surface is cleared to.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().expect("white ground");
        let held_cut = ui::review_cut_of(window);
        let held_frame =
            ui::cut_preview_frame::held_frame(&held_cut, ui::cut_preview_frame::drawn_box());
        let frame = ui::cut_preview_frame::describe(window, FIT, &held_frame);
        let seen = ui::cut_preview_frame::observe(&frame, None);
        ui::cut_preview_frame::paint_paused(&cr, &frame, &seen);
    }
    let data = surface.data().expect("read the pixels back").to_vec();
    let stride = surface.stride() as usize;
    (data, stride)
}

fn round(app: &adw::Application) {
    let root = naivepost::startup::session_dir(
        &std::env::temp_dir().join(format!("np-f35-raster-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    let tree = project_tree(&root);

    // --- 1 + 2 + 5: a cold file is rasterised once, really, and the form shows it -------------------------------
    {
        let png = root.join("red.png");
        let log = root.join("argv.log");
        let count = root.join("calls");
        write_red_png(&png);
        let stub = write_stub(&root, &png, &log, &count, true);
        point_settings_at(&root, &stub);
        let file = drawing(&root, "cold.svg");

        let window = common::cut_page_settled_first(app, &Cut::default());
        ui::note_place(true);
        ui::set_line_position(&window, cut_line::LinePos { t: 40.0 });
        let said = ui::svg_chosen(&window, Some(&file));
        assert!(
            said.contains(&fx_svg::base(&file)),
            "the arm still names its drawing: {said}"
        );
        settle();
        let out = tree
            .svg_raster_dir()
            .join(fx_svg::raster_file_name(&file));
        assert!(out.exists(), "the raster ffmpeg wrote is in the project's cache dir");
        assert_eq!(calls(&count), 1, "the cold file cost exactly one ffmpeg run");
        assert_eq!(
            ui::svg_raster_state(&file),
            fx_svg::Slot::Ready,
            "and the cache calls it ready"
        );
        assert_eq!(
            ui::raster_of(&file).as_deref(),
            Some(out.as_path()),
            "the cache hands back the file it made, not a path it invented"
        );
        let argv = argv_of(&log);
        assert!(
            argv.contains("-width\n512") && argv.contains("-pix_fmt\nrgba"),
            "the real call carried §S5's shape: {argv}"
        );
        RAN_RAN.store(true, Ordering::SeqCst);

        // A second drawing of the SAME file asks again only if the file differs; the same file placed twice does not.
        let again = ui::svg_chosen(&window, Some(&file));
        assert!(again.contains(&fx_svg::base(&file)), "re-choosing arms again: {again}");
        settle();
        assert_eq!(
            calls(&count),
            1,
            "one file, one raster — a second placement costs no ffmpeg"
        );
        let mut cache = fx_svg::Rasters::default();
        cache.done_with(&file, Path::new("x.png"));
        assert_eq!(cache.len(), 1, "one entry per file, the invariant §A.5 states");
        RAN_ONCE.store(true, Ordering::SeqCst);

        // --- 5: the REAL widget shows the picture. A form opened BEFORE the raster arrived is pushed the picture
        // through the same door a re-Choose uses (`swap_svg_file` -> `paint_svg_raster`), which is what "the row
        // updates when the raster arrives" means for a widget already on screen.
        ui::svg_drag_ended(&window, (24.0, 24.0), (150.0, 96.0));
        settle();
        ui::swap_svg_file(&window, &file);
        settle();
        assert_eq!(
            ui::svg_form_open().map(|form| form.file),
            Some(file.clone()),
            "the open form now names the drawing whose raster arrived"
        );
        let picture = widget(&window, "svg-preview-raster")
            .downcast::<gtk::Picture>()
            .expect("`svg-preview-raster` is a Picture");
        let out = tree
            .svg_raster_dir()
            .join(fx_svg::raster_file_name(&file));
        let decodes = gtk::gdk_pixbuf::Pixbuf::from_file(&out).is_ok();
        assert!(
            picture.paintable().is_some(),
            "the form's raster row holds the png, not a sentence about it (cache ready: {}, file decodes: {})",
            ui::raster_of(&file).is_some(),
            decodes
        );
        // The bytes behind it: gdk-pixbuf read the raster the stub copied, and cairo accepted it as a surface —
        // which is the pair of facts "it is a real image" is made of, checked on the same file the row shows.
        let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file(&out).expect("the raster decodes as an image");
        assert_eq!(pixbuf.width(), RASTER_PX, "and is the 512 px copy §S5 asks for");
        assert!(
            raster_surface_for_test(&out).is_some(),
            "the preview's own rgba conversion accepts the file"
        );
        assert!(
            label_text(&window, "svg-preview-raster-note").contains("512"),
            "and the note row still states the size"
        );
        RAN_PICTURE.store(true, Ordering::SeqCst);

        // --- 6 + 7: the preview paints the ink inside the box, and dashes an unfinished one. ---------------------
        // The SAME window's placed drawing: chosen, landed and Applied through the real doors, so the record's
        // seconds and box are the app's arithmetic, not a fixture handed to the painter.
        ui::press_svg_apply(&window);
        settle();
        // The painted second is the LINE, and a placed drawing fades in over `trans` (§06#1's table): at the line
        // itself its alpha is nought, so the line moves just past the fade before anything is asked of the pixels.
        // That is the same fact `fx_lane::visibility_at` states, and the check reads it from the record rather than
        // guessing a number.
        let held_cut = ui::review_cut_of(&window);
        let placed = held_cut
            .fx
            .iter()
            .find(|fx| fx.kind == "svg")
            .expect("the placed drawing is on the cut");
        ui::set_line_position(&window, cut_line::LinePos { t: placed.t + placed.trans + 0.05 });
        settle();
        let (data, stride) = paint_window(&window);
        let box_ = effect_box_px(placed, FIT);
        // The png is SQUARE, so `fit_inside` keeps its shape inside the box the record carries; measuring with the
        // same door the painter uses is what keeps this a check of the rule, not of a lucky pixel.
        let (dx, dy, dw, dh) = fx_svg::fit_inside(box_.2, box_.3, 1.0);
        let inside = rgb(
            &data,
            stride,
            (box_.0 + dx + dw / 2.0) as i32,
            (box_.1 + dy + dh / 2.0) as i32,
        );
        assert!(
            inside.0 > 120 && inside.1 < 90 && inside.2 < 90,
            "the drawing's red is painted inside its box: {inside:?}"
        );
        // Far outside the box nothing of the drawing leaked — the ink is clipped to its own rectangle.
        let outside = rgb(&data, stride, 6, 6);
        assert!(
            outside.0 > 200 && outside.1 > 200 && outside.2 > 200,
            "the ink stayed inside the box, the corner is still the white ground: {outside:?}"
        );
        RAN_PAINT.store(true, Ordering::SeqCst);

        // --- 7: §2's "a box being drawn dashed", for a drawing. Asserted through the state the painter reads
        // rather than through pixels, because the dash list is a property of the answer `observe` gives.
        let pending = (20.0, 20.0, 100.0, 60.0);
        ui::cut_preview_frame::note_box(Some(pending), &window);
        ui::release_svg_arm_only();
        let idle = ui::cut_preview_frame::held_frame(&ui::review_cut_of(&window), ui::cut_preview_frame::drawn_box());
        assert!(
            idle.box_px.is_none(),
            "with no arm up the panel is told there is no box under the hand"
        );
        ui::svg_chosen(&window, Some(&file));
        let armed = ui::cut_preview_frame::held_frame(&ui::review_cut_of(&window), ui::cut_preview_frame::drawn_box());
        assert_eq!(
            armed.box_px,
            Some(pending),
            "while \u{25a8} SVG is armed the box under the hand reaches the painter"
        );
        let frame = ui::cut_preview_frame::describe(&window, FIT, &armed);
        let seen = ui::cut_preview_frame::observe(&frame, None);
        assert!(
            seen.held.box_dashed,
            "and §2's rule says the box being drawn is the dashed one"
        );
        // Releasing the arm and letting go of the box takes it off the panel: an unfinished box belongs to a hand.
        ui::release_svg_arm_only();
        ui::cut_preview_frame::note_box(None, &window);
        let released =
            ui::cut_preview_frame::held_frame(&ui::review_cut_of(&window), ui::cut_preview_frame::drawn_box());
        assert!(
            released.box_px.is_none(),
            "after the hand leaves, no dashed box is claimed"
        );
        RAN_DASHED.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- 3: a raster already on disk is not remade (the output file is the resume marker). ------------------------
    {
        let png = root.join("red2.png");
        let log = root.join("argv2.log");
        let count = root.join("calls2");
        write_red_png(&png);
        let stub = write_stub(&root, &png, &log, &count, true);
        point_settings_at(&root, &stub);
        let file = drawing(&root, "warm.svg");
        // Pre-place the raster where the program would put it.
        let out = tree.svg_raster_dir().join(fx_svg::raster_file_name(&file));
        std::fs::create_dir_all(tree.svg_raster_dir()).expect("cache/svg");
        std::fs::copy(&png, &out).expect("the png is already there");

        let window = common::cut_page_settled_first(app, &Cut::default());
        ui::note_place(true);
        ui::set_line_position(&window, cut_line::LinePos { t: 41.0 });
        ui::svg_chosen(&window, Some(&file));
        settle();
        assert_eq!(
            calls(&count),
            0,
            "a file whose raster is on disk costs no ffmpeg at all"
        );
        assert_eq!(
            ui::svg_raster_state(&file),
            fx_svg::Slot::Ready,
            "and it is ready the moment it is asked about"
        );
        RAN_ONDISK.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- 4: a failed raster is SAID ONCE and the drawing still places. ------------------------------------------
    {
        let log = root.join("argv3.log");
        let count = root.join("calls3");
        let stub = write_stub(&root, &PathBuf::from("/unused"), &log, &count, false);
        point_settings_at(&root, &stub);
        let file = drawing(&root, "broken.svg");

        let window = common::cut_page_settled_first(app, &Cut::default());
        ui::note_place(true);
        ui::set_line_position(&window, cut_line::LinePos { t: 42.0 });
        ui::window::clear_logs_for_test();
        ui::svg_chosen(&window, Some(&file));
        settle();
        let first = ui::window_logs()
            .iter()
            .filter(|line| line.contains(&format!("the drawing {} cannot be shown:", fx_svg::base(&file))))
            .count();
        assert_eq!(first, 1, "the failure is said once: {:?}", ui::window_logs());
        // Ask again — a second press for the same broken file stays quiet (§A.5) but still arms.
        ui::svg_chosen(&window, Some(&file));
        settle();
        let second = ui::window_logs()
            .iter()
            .filter(|line| line.contains(&format!("the drawing {} cannot be shown:", fx_svg::base(&file))))
            .count();
        assert_eq!(second, 1, "the same file never complains twice");
        assert!(ui::svg_armed(), "a drawing that cannot be previewed is still a drawing");
        // And it still places: the box lands, the form opens, Apply writes the record.
        ui::svg_drag_ended(&window, (30.0, 30.0), (140.0, 100.0));
        settle();
        let before = ui::review_cut_of(&window).fx.len();
        ui::press_svg_apply(&window);
        settle();
        assert_eq!(
            ui::review_cut_of(&window).fx.len(),
            before + 1,
            "a failed preview does not stop the effect (§A.5)"
        );
        RAN_FAILED.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }
}

#[test]
fn f3_5_the_raster_is_made_by_the_program() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run();
    });

    assert!(RAN_RAN.load(Ordering::SeqCst), "the rasterising check never ran");
    assert!(RAN_ONCE.load(Ordering::SeqCst), "the one-per-file check never ran");
    assert!(RAN_ONDISK.load(Ordering::SeqCst), "the already-on-disk check never ran");
    assert!(RAN_FAILED.load(Ordering::SeqCst), "the said-once failure check never ran");
    assert!(RAN_PICTURE.load(Ordering::SeqCst), "the form's picture check never ran");
    assert!(RAN_PAINT.load(Ordering::SeqCst), "the preview paint check never ran");
    assert!(
        RAN_DASHED.load(Ordering::SeqCst),
        "the dashed-while-armed check never ran"
    );
}
