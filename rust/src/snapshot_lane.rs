//! The `06-lane` screen's second image: the effects lane painted on its OWN, at the size `spec/img/06-lane.png`
//! is drawn (1240 x 200), rather than as a strip inside a full Cut-page window.
//!
//! §2 shows this item as three pictures and the first of them is not "the Cut page": it is the lane itself — the
//! ruler, the kept bars, one row of effect bars carrying their marks and captions, the picture row and the wave
//! strips under it. A whole window clamps to ~790 px of plate headless and puts the lane somewhere in the middle
//! of it, so the shot could never be compared with the spec's image at all. So this paints the SAME scene the page
//! paints (`cut_tracks::paint_at`, the door `sec_06_effects_2_lane_rows_order.rs` also uses) onto a cairo surface
//! of the spec's own shape and writes that file — no widget is built, and every colour, row and caption comes from
//! the same painters the widgets use.
//!
//! The sidecar precedent is `snapshot_effect_menu`, and it now reads the other way for BOTH of them: the subject
//! the spec pictures takes `<name>.png` (that file IS what §2 drew) and the window shot goes beside it as
//! `<name>-page.png`.

use std::path::Path;

use crate::cut::{Cut, Fx, Seg};
use crate::layout;
use crate::timeline::Recording;
use crate::ui::cut_tracks::{self, Scene};
/// The spec's own width for this image (`spec/img/06-lane.png` is 1240 x 200). Not a tuning value: it is the
/// width the spec drew, so the two images can be put side by side. The HEIGHT is not fixed here — the band asks
/// what its own rows cost (see [`write_png`]), and forcing 200 px on a taller scene only cut the picture row off.
const SHOT_W: i32 = 1240;
/// Pixels per second the spec's lane is drawn at: its ruler runs 0:00 to 5:00 across the image's width, which is
/// the same 4 px/s the Cut page uses (`crate::ui::window::SELECT_SURFACE_PPS`) — so the bars here are the bars
/// there, and the staging in `snapshot_effects::paint` (50 s = 200 px) lands in the same place.
pub const PPS: f64 = 4.0;

/// Is this screen's SUBJECT the lane itself? §06-effects#2's first picture (`spec/img/06-lane.png`) is the lane
/// at 1240 x 200 -- ruler, kept bars, the effects lane with its washes, the picture rows -- not a Cut-page window,
/// so the lane takes `<name>.png` and the page goes beside it. The door is the same shape as
/// [`crate::snapshot_effect_menu::is_menu_subject`] for the same reason: `snapshot::run` asks, rather than matching
/// a screen name against a hardcoded list of its own.
pub fn is_lane_subject(screen: &str) -> bool {
    screen == "06-lane"
}

/// Paint the lane scene into `<out>` at [`SHOT_W`] wide and as tall as its own rows. `dir` is the project folder
/// the snapshot was asked to render: the picture row's frames and the wave strip's envelope cache are staged there
/// first (see [`stage_frame`] / [`stage_envelope`]) and read back through the SAME doors the live page reads them
/// (`cut_tracks::frame_paths`, `cache/waves/<base>.wave`), so this image cannot show content the app would not have.
/// Returns the log sentence.
pub fn write_png(cut: &Cut, dir: &Path, out: &Path) -> Result<String, String> {
    // The shot asks for film in its picture row; a live page never sets this (§7: no decode inside a draw).
    cut_tracks::allow_frame_decoding();
    // Name this shot's folder as the project the painters read from, so the wave strip's own reader
    // (`cut_trim_press::cached_peaks`) looks for `cache/waves/…` BESIDE this project rather than in the working
    // copy beside the launch — which for a test's temp root is nowhere, and the strip came out a flat line.
    crate::ui::cut_picture::set_snapshot_dir(dir);
    let thumb_px = cut_tracks::THUMB_DEFAULT_PX;
    let base = "session-tape";
    // Staged here rather than by each caller so every producer of this image — `just snapshot 06-lane` and the
    // checks that decode it — goes through one door and gets the same content.
    stage_frame(dir, base, 0.0);
    if let Err(why) = stage_envelope(dir, base) {
        eprintln!("snapshot: 06-lane wave cache | {why}");
    }
    let mut scene = scene(cut);
    // What the band may show, listed through the painter's own door. Empty when nothing is there, which draws no
    // pictures rather than inventing black (§1's prototype note).
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
        // Read back off the PNG's own IHDR rather than repeating what was asked for: §00-principles §5 says
        // measure the artifact, and this is the sentence a person compares against `spec/img/06-lane.png`.
        {
            let (w, h) = crate::snapshot_face::png_size(out);
            format!("{w}x{h} px on disk")
        }
    ))
}

/// The frames each recording can show, listed through the painter's own door — the same tree
/// [`cut_tracks::band_height_for_window`] counts its rows from, so the height asked and the pictures drawn come
/// from one source. Empty when the page has no frame tree (nothing is invented, §1's prototype note).
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

/// Paint THIS window's track area — ruler down to the bottom of the last picture row and its wave strip — into
/// `out`, by cropping the live page rather than repainting a scene.
///
/// §2's first picture is that stack as a user sees it, and `spec/QUESTIONS.md` answer 3(a) says where it ends:
/// "06-lane is the track area from the ruler down to the bottom of the picture row and its waveform, with the dark
/// gutter running the whole height at the left and the fold-all − at its top… Make the shot end where the track
/// stack ends, not at a fixed height." A cairo surface painted by [`write_png`] cannot show the gutter's controls,
/// because `fold-all-button` and the per-lane sound switches are WIDGETS (`gutter-badges`, `gutter-switches`,
/// overlaid on `track-layers`), so they exist only in the page's own render node. Hence the crop: the pixels are
/// the ones the page painted, including every widget on the rail.
///
/// The band's top edge and its height come off the real rects (`track-layers`) and off the same doors the painter
/// uses for where its rows end ([`cut_tracks::band_height_for_window`]), so the frame cannot stop inside a row
/// again. Returns the log sentence naming the box that was cut.
pub fn paint_track_area(window: &adw::ApplicationWindow, out: &Path) -> Result<String, String> {
    use adw::prelude::*;
    let layers = crate::ui::find_source_widget(window, "track-layers")
        .ok_or("the Cut page drew no track-layers overlay")?;
    let scene = crate::ui::cut_view::band_scene_for(window);
    // The band's own laid-out height: what the painter was asked to fit, and what §2's stack costs. Asked of the
    // SAME door the installed band paints from (frames included) rather than measured off the widget, so a row an
    // edit added is counted even if GTK has not caught up with the request yet.
    let tall = cut_tracks::band_height_for_window(window).max(1);
    let wide = layers.width().max(1);
    let (x, y) = crate::snapshot_face::widget_offset(window, &layers)?;
    // The whole track stack is PAINTED at its own row geometry rather than cropped out of the window's
    // allocation. A crop alone came out 132 px tall — it stopped inside the film row, with no waveform strip —
    // because GTK clips a `DrawingArea` to what the page was given, and the page is not given what the band
    // asks (`spec/QUESTIONS.md` §06 answer 3(a): the shot must "end where the track stack ends", and the
    // stack's height is the painter's own number, `band_height_for_window`, the door `attach` fits from).
    // Same painter the page's draw func calls, so no colour, row or caption can disagree with the page.
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
    // The gutter's CONTROLS are widgets (`gutter-badges`, `gutter-switches`), so no painter can put them here:
    // the page's own pixels for that column are blitted on top, for however much of it the page did draw. The
    // ink underneath comes from the same painter, so the two halves cannot disagree about where a row is.
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
    // How many frames the page's own door could list, printed with the box: a row that came out flat grey is
    // either an empty list (wrong folder — this number is 0) or a painter that drew too few cells (this is
    // large), and the two need different fixes. Read off the same door `install` was handed.
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

/// The tree every file this shot reads and writes lives under: `snapshot::run` hands its screens the copied
/// session, and a test's temp root gets the same shape so one set of doors serves both callers.
fn staged_tree(dir: &Path) -> Result<layout::Tree, String> {
    layout::Tree::new(session_folder(dir))
}

/// The folder the program's own doors resolve as the session for the files this shot stages and reads back.
/// A page's `cache/…` and `prepare/inputs/…` live under the folder the window took on, and there are two doors to
/// agree with: `Tree::new(dir)` when a caller hands over the session itself (§07's band staging passes a `Tree`
/// built on `narrate_session_tree()`), and `startup::session_dir(current_dir())` for the wave strip's own reader
/// (`cut_trim_press::cached_peaks`), which resolves the working copy beside the launch. A bare temp root gets the
/// `session.naivepost` shape inside it, so one set of doors serves both callers.
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

/// Stage one extracted frame where Prepare would have put one, through the program's own door
/// (`snapshot_face::seed_structured_frame`), so `cut_tracks::frame_paths` finds a real JPEG on the grid and the
/// picture row carries film instead of flat grey. Nothing is painted here: this only puts the file where the
/// painter already knows to look.
///
/// ONE frame, because that is what the band shows: `draw_picture_rows` picks the single frame nearest the view's
/// left edge and marks it at that second — planting more files changes which one lands, not how many are drawn.
pub fn stage_frame(dir: &Path, base: &str, second: f64) -> Option<std::path::PathBuf> {
    let tree = staged_tree(dir).ok()?;
    crate::snapshot_face::seed_structured_frame(tree.dir(), base, second)
}

/// Stage this take's loudness envelope into `cache/waves/<base>.wave` the way the app itself does: write the
/// decoded mono audio where `wave_build::ensure` looks for it, then let that door fold the peaks and write the
/// cache (`wave_build::ensure` -> `wave::write`), which is also what stamps it against the reading. `draw_row_wave_strips`
/// -> `cut_trim_press::paint_row_wave` then reads that same cache back through `cached_peaks`; without it the strip
/// draws a flat baseline. Nothing here paints — this only puts files where the painter already knows to look.
///
/// `cached_peaks` stamps the cache against the CACHE's own (length, 0) and resolves the session from the CURRENT
/// WORKING DIRECTORY (`startup::session_dir(current_dir())`), so a shot taken from a launch whose working copy is
/// this project reads what is written here; a check pointing elsewhere gets a flat strip and says so.
///
/// The audio is the tone-burst wav `snapshot::write_silence_wav` produces — the same stand-in §07's Narrate band is
/// staged with (`snapshot_narrate::stage_envelope`) — at the rate the fold assumes. It is real audio the program's
/// own decoder reads, not a scripted picture.
pub fn stage_envelope(dir: &Path, lane: &str) -> Result<(), String> {
    let tree = staged_tree(dir)?;
    let decoded = tree.voice16k_wav(lane);
    let rate = crate::transcribe::SAMPLE_RATE;
    // The decoded file is what the fold READS, so it has to exist first: `write_silence_wav` only writes an
    // existing file (`File::options().open`), it does not create one.
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

/// The scene: the same cut the window was seeded with, at the spec's zoom, over one take long enough to fill the
/// image. Thumbnails come from [`write_png`], which lists what the folder actually holds; here they are empty
/// because nothing decodes during a paint (§7).
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

/// The cut `crate::ui::cut_tracks` will paint for the shot: §2's list of
/// what the lane shows — a zoom, a staying zoom, a speed with its rate and sound answer, a text, a volume and a
/// label — each wide enough at [`PPS`] to carry its mark and its caption. This is the staging the window shot uses
/// too (`snapshot_effects::paint` seeds the same records), so the two images cannot show different lanes.
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
