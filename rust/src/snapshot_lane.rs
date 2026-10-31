//! The `06-lane` screen's second image: the effects lane painted on its OWN, at the size `spec/img/06-lane.png`
//! is drawn (1240 x 200), rather than as a strip inside a full Cut-page window.
//!
//! A whole window clamps to ~790 px headless and buries the lane, so this paints the SAME scene the page paints (`cut_tracks::paint_at`, also used by
//! `sec_06_effects_2_lane_rows_order.rs`) onto a cairo surface of the spec's shape; no widget is built. As for `snapshot_effect_menu`, the subject takes `<name>.png` and the window shot goes beside it as `<name>-page.png`.

use std::path::Path;

use crate::cut::{Cut, Fx, Seg};
use crate::layout;
use crate::timeline::Recording;
use crate::ui::cut_tracks::{self, Scene};
/// The spec's own width for this image (`spec/img/06-lane.png` is 1240 x 200). The HEIGHT is the band's own rows (see [`write_png`]); forcing 200 px cut the picture row off.
const SHOT_W: i32 = 1240;
/// Pixels per second the spec's lane is drawn at (ruler 0:00 to 5:00 across the width): the Cut page's `crate::ui::window::SELECT_SURFACE_PPS`, so the staging in `snapshot_effects::paint` lands in the same place.
pub const PPS: f64 = 4.0;

/// Is this screen's SUBJECT the lane itself (§06-effects#2's first picture)? Then it takes `<name>.png` and the page goes beside it. Same shape as
/// [`crate::snapshot_effect_menu::is_menu_subject`]: `snapshot::run` asks rather than matching a hardcoded list.
pub fn is_lane_subject(screen: &str) -> bool {
    screen == "06-lane"
}

/// Paint the lane scene into `<out>` at [`SHOT_W`] wide and as tall as its own rows. `dir` is the project folder: frames and the wave envelope cache are staged there
/// ([`stage_frame`] / [`stage_envelope`]) and read back through the live page's doors, so the image cannot show content the app would not have. Returns the log sentence.
pub fn write_png(cut: &Cut, dir: &Path, out: &Path) -> Result<String, String> {
    // The shot asks for film in its picture row; a live page never sets this (§7: no decode inside a draw).
    cut_tracks::allow_frame_decoding();
    // Name this shot's folder as the project the painters read from, so `cut_trim_press::cached_peaks` finds `cache/waves/…` beside it, not in the launch's working copy (a flat strip).
    crate::ui::cut_picture::set_snapshot_dir(dir);
    let thumb_px = cut_tracks::THUMB_DEFAULT_PX;
    let base = "session-tape";
    // Staged here so every producer of this image goes through one door.
    stage_frame(dir, base, 0.0);
    if let Err(why) = stage_envelope(dir, base) {
        eprintln!("snapshot: 06-lane wave cache | {why}");
    }
    let mut scene = scene(cut);
    scene.thumbs = staged_tree(dir)
        .ok()
        .map(|tree| vec![(base.to_string(), cut_tracks::frame_paths(&tree, base))])
        .unwrap_or_default();
    let height = cut_tracks::band_height_px(&scene, thumb_px).ceil() as i32;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, SHOT_W, height)
        .map_err(|why| format!("the lane surface: {why}"))?;
    {
        let cr = cairo::Context::new(&surface).map_err(|why| format!("the lane context: {why}"))?;
        cut_tracks::paint_at(&cr, &scene, SHOT_W as f64, height as f64, thumb_px);
    }
    let mut file = std::fs::File::create(out).map_err(|why| format!("{}: {why}", out.display()))?;
    surface
        .write_to_png(&mut file)
        .map_err(|why| format!("writing the lane: {why}"))?;
    Ok(format!(
        "lane painted at {} px/s: {} effect bars in {} rows, {} px tall; file {}",
        PPS,
        cut.fx.len(),
        crate::fx_lane::row_count(&cut.fx),
        height,
        // Read back off the PNG's own IHDR: §00-principles §5 says measure the artifact.
        {
            let (w, h) = crate::snapshot_face::png_size(out);
            format!("{w}x{h} px on disk")
        }
    ))
}

/// The frames each recording can show, through the painter's own door ([`cut_tracks::band_height_for_window`] counts rows from the same tree). Empty when the page has no frame tree.
fn frame_lists(scene: &Scene) -> Vec<(String, Vec<std::path::PathBuf>)> {
    scene
        .recordings
        .iter()
        .map(|recording| {
            (
                recording.base.clone(),
                crate::ui::cut_picture::snapshot_frame_tree()
                    .map(|tree| cut_tracks::frame_paths(&tree, &recording.base))
                    .unwrap_or_default(),
            )
        })
        .collect()
}

/// Paint THIS window's track area (ruler down to the bottom of the last picture row and its wave strip) into `out`, by cropping the live page.
///
/// `spec/QUESTIONS.md` answer 3(a): the shot is the track area with the gutter at the left, ending where the track stack ends. A cairo surface from [`write_png`]
/// cannot show the gutter's controls (`fold-all-button`, per-lane sound switches), which are WIDGETS in the page's render node; hence the crop.
///
/// The band's top edge and height come off the real rects (`track-layers`) and [`cut_tracks::band_height_for_window`], so the frame cannot stop inside a row. Returns the log sentence naming the box cut.
pub fn paint_track_area(window: &adw::ApplicationWindow, out: &Path) -> Result<String, String> {
    use adw::prelude::*;
    let layers = crate::ui::find_source_widget(window, "track-layers")
        .ok_or("the Cut page drew no track-layers overlay")?;
    let scene = crate::ui::cut_view::band_scene_for();
    // The band's laid-out height from the SAME door the installed band paints from (frames included), so a row an edit added is counted even if GTK has not caught up.
    let tall = cut_tracks::band_height_for_window().max(1);
    let wide = layers.width().max(1);
    let (x, y) = crate::snapshot_face::widget_offset(window, &layers)?;
    // The whole stack is PAINTED at its own row geometry, not cropped: a crop came out 132 px tall (no waveform strip) because GTK clips a `DrawingArea` to what the page was given,
    // not what the band asks (`spec/QUESTIONS.md` §06 answer 3(a)). Same painter as the page's draw func.
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, wide, tall)
        .map_err(|why| format!("the track-area surface: {why}"))?;
    {
        let cr = cairo::Context::new(&surface).map_err(|why| why.to_string())?;
        let mut painted_scene = scene.clone();
        painted_scene.thumbs = frame_lists(&scene);
        cut_tracks::paint_at(
            &cr,
            &painted_scene,
            wide as f64,
            tall as f64,
            cut_tracks::THUMB_DEFAULT_PX,
        );
    }
    // The gutter's CONTROLS are widgets, so the page's own pixels for that column are blitted on top; the ink underneath comes from the same painter.
    let inked = tall.min(layers.height()).max(0);
    if inked > 0 {
        let full = crate::snapshot_face::render_surface(
            window.upcast_ref(),
            window.width().max(1),
            window.height().max(1),
        )?;
        let cr = cairo::Context::new(&surface).map_err(|why| why.to_string())?;
        cr.set_source_surface(&full, -(f64::from(x)), -(f64::from(y)))
            .map_err(|why| why.to_string())?;
        cr.rectangle(0.0, 0.0, crate::cut_screen::GUTTER_PX, inked as f64);
        cr.clip();
        cr.paint()
            .map_err(|why| format!("the gutter rail's pixels: {why}"))?;
    }
    let mut file = std::fs::File::create(out).map_err(|why| format!("{}: {why}", out.display()))?;
    surface
        .write_to_png(&mut file)
        .map_err(|why| format!("writing the track area: {why}"))?;
    // How many frames the page's door could list, printed with the box: 0 means a wrong folder, a large number means too few cells drawn.
    let listed = crate::ui::cut_picture::snapshot_frame_tree()
        .map(|tree| {
            scene
                .recordings
                .first()
                .map_or(0, |r| cut_tracks::frame_paths(&tree, &r.base).len())
        })
        .unwrap_or(0);
    Ok(format!(
        "track area cropped from the live page at x={x},y={y}: {wide}x{tall} px \
         (ruler + selection band + {} lane row(s) + {} row(s) of film with their wave strips; \
         {} frame(s) listed by the band's door)",
        crate::fx_lane::row_count(&scene.fx),
        scene.row_count.max(1) + crate::ui::cut_tracks::lane_rows(&scene),
        listed,
    ))
}

/// The tree every file this shot reads and writes lives under; a test's temp root gets the same shape.
fn staged_tree(dir: &Path) -> Result<layout::Tree, String> {
    layout::Tree::new(session_folder(dir))
}

/// The folder the program's doors resolve as the session for staged files. Two doors must agree: `Tree::new(dir)` when a caller hands over the session, and
/// `startup::session_dir(current_dir())` for the wave strip's reader (`cut_trim_press::cached_peaks`). A bare temp root gets the `session.naivepost` shape inside it.
fn session_folder(dir: &Path) -> std::path::PathBuf {
    let session_beside_launch =
        crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    if dir == session_beside_launch {
        return dir.to_path_buf();
    }
    if dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".naivepost"))
    {
        return dir.join("session.naivepost");
    }
    dir.join("session.naivepost")
}

/// The tape length the lane shot paints, in seconds: the whole surface at [`PPS`].
const TAPE_SECONDS: f64 = SHOT_W as f64 / PPS;

/// Stage one extracted frame where Prepare would have put one, through `snapshot_face::seed_structured_frame`, so `cut_tracks::frame_paths` finds a real JPEG. Nothing is painted here.
///
/// ONE frame: `draw_picture_rows` picks the single frame nearest the view's left edge; more files change which lands, not how many are drawn.
pub fn stage_frame(dir: &Path, base: &str, second: f64) -> Option<std::path::PathBuf> {
    let tree = staged_tree(dir).ok()?;
    crate::snapshot_face::seed_structured_frame(tree.dir(), base, second)
}

/// Stage this take's loudness envelope into `cache/waves/<base>.wave` as the app does: write decoded mono audio where `wave_build::ensure` looks, then let it fold peaks and write the cache.
/// `cut_trim_press::paint_row_wave` reads that cache back through `cached_peaks`; without it the strip is a flat baseline. Nothing here paints.
///
/// `cached_peaks` resolves the session from the CURRENT WORKING DIRECTORY (`startup::session_dir(current_dir())`), so a shot taken from a launch whose
/// working copy is this project reads what is written here; elsewhere gets a flat strip.
///
/// The audio is the tone-burst wav `snapshot::write_silence_wav` produces (as `snapshot_narrate::stage_envelope`), at the rate the fold assumes.
pub fn stage_envelope(dir: &Path, lane: &str) -> Result<(), String> {
    let tree = staged_tree(dir)?;
    let decoded = tree.voice16k_wav(lane);
    let rate = crate::transcribe::SAMPLE_RATE;
    // The fold READS the decoded file, so it must exist first: `write_silence_wav` only writes an existing file.
    if let Some(parent) = decoded.parent() {
        std::fs::create_dir_all(parent).map_err(|why| format!("{}: {why}", parent.display()))?;
    }
    crate::snapshot::write_silence_wav(
        &decoded,
        rate as u32,
        ((TAPE_SECONDS + 1.0) * rate as f64) as u32,
    )
    .map_err(|why| format!("{}: {why}", decoded.display()))?;
    crate::wave_build::ensure(&tree, lane, &decoded, &mut |_| {})
}

/// The scene: the cut the window was seeded with, at the spec's zoom, over one take long enough to fill the image. Thumbnails are empty: nothing decodes during a paint (§7).
fn scene(cut: &Cut) -> Scene {
    Scene {
        pps: PPS,
        origin: 0.0,
        segs: if cut.segs.is_empty() {
            vec![Seg {
                s: 0.0,
                e: SHOT_W as f64 / PPS,
                cam: 0,
                ..Default::default()
            }]
        } else {
            cut.segs.clone()
        },
        fx: cut.fx.clone(),
        recordings: vec![Recording {
            base: "session-tape".to_string(),
            start: 0.0,
            end: 400.0,
        }],
        recording_rows: Vec::new(),
        marks: Default::default(),
        folds: Vec::new(),
        selection: None,
        watched_row: None,
        row_count: 1,
        shift: Default::default(),
        lanes: Vec::new(),
        lane_plates: Vec::new(),
        thumbs: Vec::new(),
        line: 0.0,
    }
}

/// The cut `crate::ui::cut_tracks` paints for the shot: §2's list of what the lane shows (zoom, staying zoom, speed with rate and sound answer, text, volume, label),
/// each wide enough at [`PPS`] for its mark and caption. The window shot seeds the same records (`snapshot_effects::paint`).
pub fn staged_cut() -> Cut {
    Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 300.0,
            cam: 0,
            ..Default::default()
        }],
        fx: vec![
            Fx {
                kind: "zoom".to_string(),
                t: 5.0,
                dur: 50.0,
                trans: 1.0,
                ..Default::default()
            },
            Fx {
                kind: "zoom".to_string(),
                t: 62.0,
                dur: 40.0,
                stay: true,
                ..Default::default()
            },
            Fx {
                kind: "speed".to_string(),
                t: 108.0,
                dur: 60.0,
                rate: 0.5,
                snd: "scene".to_string(),
                ..Default::default()
            },
            Fx {
                kind: "text".to_string(),
                t: 175.0,
                dur: 40.0,
                text: "Ethereum in one lecture".to_string(),
                ..Default::default()
            },
            Fx {
                kind: "volume".to_string(),
                t: 220.0,
                dur: 40.0,
                gain: 2.0,
                ..Default::default()
            },
            Fx {
                kind: "label".to_string(),
                t: 265.0,
                dur: 8.0,
                text: "intro".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}
