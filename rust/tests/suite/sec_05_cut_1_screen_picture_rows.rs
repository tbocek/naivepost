//! §05-cut#1-screen — the Cut page's picture row lays its grid out from what the FOLDER actually holds.
//!
//! The row is painted one cell per extracted frame, so two facts decide whether it reads as pictures at all: the
//! spacing comes from the folder's own stamp names (not a guessed frame rate), and every listed frame marks its
//! own cell instead of leaving a flat band. Both are measured off the painted surface — `snapshot_lane::write_png`
//! is the same door `just snapshot 06-lane` writes its shot with — and compared against the SAME row painted with
//! no frames listed, which is what a flat band is byte for byte.

use std::path::{Path, PathBuf};

use naivepost::cut::Cut;
use naivepost::fx_lane;
use naivepost::snapshot_lane;
use naivepost::ui::cut_tracks;

/// This test binary's own folder — never a fixed path, since the suite runs files in parallel.
fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-cutcuts-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp root created");
    dir
}

/// Decode a PNG the way a person's viewer would: through gdk-pixbuf, from the FILE on disk.
fn decode(path: &Path) -> Decoded {
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_file(path)
        .unwrap_or_else(|why| panic!("{}: {why}", path.display()));
    assert_eq!(
        pixbuf.n_channels(),
        3,
        "cairo's opaque PNG decodes to RGB, alpha dropped"
    );
    // `pixels()`' slice is only valid while the pixbuf lives, so it is copied out and nothing borrows past it.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            pixbuf.pixels().as_ptr(),
            (pixbuf.height() * pixbuf.rowstride()) as usize,
        )
    }
    .to_vec();
    let decoded = Decoded {
        bytes,
        stride: pixbuf.rowstride() as usize,
        width: pixbuf.width(),
        height: pixbuf.height(),
    };
    drop(pixbuf);
    decoded
}

struct Decoded {
    bytes: Vec<u8>,
    stride: usize,
    width: i32,
    height: i32,
}

impl Decoded {
    fn pixel(&self, x: i32, y: i32) -> (u32, u32, u32) {
        assert!(
            x < self.width && y < self.height,
            "sample ({x},{y}) fell off a {}x{} shot",
            self.width,
            self.height
        );
        let at = y as usize * self.stride + x as usize * 3;
        (
            self.bytes[at] as u32,
            self.bytes[at + 1] as u32,
            self.bytes[at + 2] as u32,
        )
    }
}

/// Write a 1x1 pure-red PNG with cairo, decode it and assert the byte order is R,G,B — the pin every sample below
/// rests on. A test that guessed B,G,R would read blue as red and every compare would be against the wrong channel.
fn pin_channel_order(dir: &Path) {
    let path = dir.join("channel-order.png");
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).expect("a 1x1 surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        cr.set_source_rgb(1.0, 0.0, 0.0);
        let _ = cr.paint();
    }
    let mut file = std::fs::File::create(&path).expect("write the probe");
    surface.write_to_png(&mut file).expect("encode the probe");
    assert_eq!(
        decode(&path).pixel(0, 0),
        (255, 0, 0),
        "the decoded bytes are not R,G,B -- fix `pixel` first"
    );
}

/// The lane's own geometry, asked of the same doors `cut_tracks::paint_at` lays its rows out by.
struct LaneGeo {
    pictures: f64,
}

fn lane_geo(lane_rows: usize) -> LaneGeo {
    let top = naivepost::ui::cut_band_rows::lane_top_px(
        naivepost::cut_trim::RULER_H,
        cut_tracks::SELECTION_BAND_PX,
    );
    LaneGeo {
        pictures: naivepost::ui::cut_band_rows::pictures_top_px(top, lane_rows.max(1) as f64),
    }
}

/// Frame names, spelled the only way that means anything here: `clock::frame_name`.
fn stamps(at: &[f64]) -> Vec<PathBuf> {
    at.iter()
        .map(|second| PathBuf::from(format!("{}.jpg", naivepost::clock::frame_name(*second))))
        .collect()
}

/// §F1.6 / §1: the row's spacing is the FOLDER's own, measured through the same parse that places a cell.
#[test]
fn sec_05_cut_1_screen_the_picture_row_measures_its_grid_from_the_folder() {
    let per_second = stamps(&[0.0, 1.0, 2.0]);
    assert_eq!(cut_tracks::frame_step_seconds(&per_second), 1.0);
    let quarter = stamps(&[0.0, 0.25, 0.5]);
    assert!(
        (cut_tracks::frame_step_seconds(&quarter) - 0.25).abs() < 1e-9,
        "a 250 ms grid must measure as 0.25 s"
    );
    // Nothing to measure — one entry, none, or names that are not stamps. All answer 0, which tells the caller to
    // take the FIRST batch rather than aim at a grid it cannot know.
    assert_eq!(cut_tracks::frame_step_seconds(&stamps(&[3.0])), 0.0);
    assert_eq!(cut_tracks::frame_step_seconds(&[]), 0.0);
    let junk: Vec<PathBuf> = vec![PathBuf::from("f000001.jpg"), PathBuf::from("f000002.jpg")];
    assert_eq!(cut_tracks::frame_step_seconds(&junk), 0.0);
}

/// §1: the listed frames mark their own cells — a row with frames in it is NOT the flat band an empty lane paints.
#[test]
fn sec_05_cut_1_screen_a_listed_frame_marks_its_own_cell() {
    // The painted row: a flat band shows ONE colour across its width however many frames are listed, so counting
    // distinct samples along the row is what separates "pictures" from "grey".
    let dir = temp_root("row-cells");
    pin_channel_order(&dir);
    let cut = snapshot_lane::staged_cut();
    let out = dir.join("row.png");
    snapshot_lane::write_png(&cut, &dir, &out).expect("the lane paints");
    let shot = decode(&out);
    let bars = fx_lane::lane_layout(&cut.fx);
    let geo = lane_geo(bars.iter().map(|b| b.row).max().unwrap_or(0) + 1);
    // Compared against the SAME row painted with no frames listed: that is what a flat band is, byte for byte, and
    // it is asked of the painter rather than assumed — an empty lane costs one row (`fx_lane::row_count`), so the
    // bare cut's `pictures_top_px` would be a different y and the comparison would prove nothing.
    let bare = Cut {
        fx: Vec::new(),
        ..cut.clone()
    };
    let bare_out = dir.join("bare-row.png");
    snapshot_lane::write_png(&bare, &dir, &bare_out).expect("the bare lane paints");
    let bare_shot = decode(&bare_out);
    // The row's FILM part only: the wave strip owns the bottom of every row (`Rows::wave_top`), so a sample down
    // there differs between the two surfaces for reasons that have nothing to do with thumbnails. Both surfaces are
    // painted from the same cut, so their rows begin at the same y and a difference means "a frame changed this".
    let film_bottom = geo.pictures + cut_tracks::THUMB_DEFAULT_PX as f64 - cut_tracks::ROW_WAVE_PX;
    let ys: Vec<i32> = ((geo.pictures + 4.0) as i32..film_bottom as i32)
        .step_by(6)
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut differing = 0usize;
    for x in (0..shot.width).step_by(4) {
        for y in &ys {
            seen.insert(shot.pixel(x, *y));
            if shot.pixel(x, *y) != bare_shot.pixel(x, *y) {
                differing += 1;
            }
        }
    }
    // How much of the row the listed frames CAN cover: one cell per frame, each `step * pps` wide, at the lane
    // shot's own zoom. Asked of the frames `write_png` itself staged (the same door it paints from), so the bound
    // tracks the fixture rather than a guess about it.
    naivepost::ui::cut_picture::set_snapshot_dir(&dir);
    let paths = naivepost::ui::cut_picture::snapshot_frame_tree()
        .map(|tree| cut_tracks::frame_paths(&tree, "session-tape"))
        .unwrap_or_default();
    let step = cut_tracks::frame_step_seconds(&paths);
    let covered = (paths.len() as f64 * step * snapshot_lane::PPS).min(shot.width as f64);
    let want = (ys.len() as f64 * covered / 4.0) as usize / 2;
    assert!(
        !paths.is_empty(),
        "the lane shot staged no frames for its own picture row"
    );
    assert!(
        seen.len() > 3,
        "the picture row shows {} distinct colour(s) across its width -- a flat band, not a row of pictures",
        seen.len()
    );
    assert!(
        differing > want,
        "only {differing} sample(s) of the row differ from the same row painted with NO frames listed; the {} \
         listed frames at a {:.2} s grid should mark about {:.0} px of the row",
        paths.len(),
        step,
        covered
    );
    let _ = std::fs::remove_dir_all(&dir);
}
