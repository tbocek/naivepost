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

/// Paint the lane scene into `<out>` at [`SHOT_W`] wide and as tall as its own rows. Returns the log sentence.
pub fn write_png(cut: &Cut, out: &Path) -> Result<String, String> {
    let thumb_px = cut_tracks::THUMB_DEFAULT_PX;
    let scene = scene(cut);
    // The band's own height at this thumbnail size is what the painter lays rows out against: ruler + selection
    // band + one lane row per overlapping group + a picture row with its wave strip. Asking for LESS than that
    // would not shrink the drawing — `paint_at` lays its rows out by the scene, so a 200 px ask over a 268 px
    // scene simply cut the picture row off mid-way, which is what an earlier version of this shot showed.
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
        "lane painted at {} px/s: {} effect bars in {} rows, {} px tall",
        PPS,
        cut.fx.len(),
        crate::fx_lane::row_count(&cut.fx),
        height
    ))
}

/// The scene: the same cut the window was seeded with, at the spec's zoom, over one take long enough to fill the
/// image. Thumbnails are left out — nothing decodes during a paint (§7), and the spec's own row of slides is
/// footage this container has no frames of; the ground and the wave strips carry the comparison instead.
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
