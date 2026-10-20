// §06-effects#2-the-lane-and-the-preview S2/S3 — the two HALVES §2 puts on screen, proven as ARTIFACTS.
//
// The item's complaint was that the lane existed only as numbers: every earlier test asked a function what colour
// or what row something should be, and nothing on screen displayed a placed effect. So this file measures pixels.
// Each check paints through the SAME door `just snapshot` writes its shot with — `snapshot_lane::write_png` (the
// band's own painter, `cut_tracks::paint_at`) and `snapshot_preview_frame::write_png` (the preview's own
// `describe → observe → paint_paused`) — then reads the bytes back off disk through gdk-pixbuf. §00-principles §5:
// measure the artifact, never the surface that wrote it.
//
// Every expected colour is COMPUTED from the doors the painter itself reads (`fx_lane::bar_colour` /
// `bar_wash_alpha` / `DIM_ALPHA`, `snapshot_face`'s staged bands), never retyped here, so a palette change moves
// the expectation and the shot together.
//
// No window is built and no app is run: cairo paints to an image surface and gdk-pixbuf decodes one without a
// display, so these are plain `#[test]`s and run in parallel with the widget files. The widget half of the wire is
// `sec_06_effects_2_lane_preview_wired.rs`; the one interactive check here (a caption through ❝ Text's dropdown
// item) lives in the sibling file that owns the single `Application::run`.

use std::path::{Path, PathBuf};

use naivepost::cut::{Cut, EffectKind, Fx, Seg};
use naivepost::fx_lane;
use naivepost::snapshot_face;
use naivepost::snapshot_lane;
use naivepost::snapshot_preview_frame;
use naivepost::ui::{cut_preview_frame, cut_tracks};
use naivepost::ui::cut_tracks::{Scene};

/// This test binary's own folder — never a fixed path, since the suite runs files in parallel.
fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-s06e2surf-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp root created");
    dir
}

/// Decode a PNG the way a person's viewer would: through gdk-pixbuf, from the FILE on disk.
///
/// The sample order is asserted, not assumed: one known pixel of one known file settles it for every colour check
/// below, because a test that guessed R,G,B would read blue as red and every assert would compare the wrong
/// channels. cairo's opaque PNG decodes to 3 channels with the alpha dropped, so a pixel is 3 bytes wide.
fn decode(path: &Path) -> Decoded {
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_file(path).unwrap_or_else(|why| panic!("{}: {why}", path.display()));
    assert_eq!(pixbuf.n_channels(), 3, "cairo's opaque PNG decodes to RGB, alpha dropped");
    assert!(!pixbuf.has_alpha(), "and any alpha it carried is gone");
    // `pixels()`' slice is only valid while the pixbuf lives, so it is copied out and nothing borrows past it.
    let bytes =
        unsafe { std::slice::from_raw_parts(pixbuf.pixels().as_ptr(), (pixbuf.height() * pixbuf.rowstride()) as usize) }
            .to_vec();
    let decoded = Decoded { bytes, stride: pixbuf.rowstride() as usize, width: pixbuf.width(), height: pixbuf.height() };
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
        assert!(x < self.width && y < self.height, "sample ({x},{y}) fell off a {}x{} shot", self.width, self.height);
        let at = y as usize * self.stride + x as usize * 3;
        (self.bytes[at] as u32, self.bytes[at + 1] as u32, self.bytes[at + 2] as u32)
    }
}

/// Write a 1x1 pure-red PNG with cairo, decode it and assert the byte order is R,G,B — the pin every colour below
/// rests on. Called once per check that samples a colour.
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
    assert_eq!(decode(&path).pixel(0, 0), (255, 0, 0), "the decoded bytes are not R,G,B -- fix `pixel` first");
}

fn near(a: (u32, u32, u32), b: (u32, u32, u32), within: u32) -> bool {
    let one = |t: (u32, u32, u32)| [t.0, t.1, t.2];
    let (x, y) = (one(a), one(b));
    (0..3).all(|i| x[i].abs_diff(y[i]) <= within)
}

/// `colour` at `alpha` laid over `over`, rounded to bytes — the composite every assert compares against, spelled
/// once so no test hardcodes a blended triple.
fn over(colour: (f64, f64, f64), alpha: f64, onto: (f64, f64, f64)) -> (u32, u32, u32) {
    let mix = |c: f64, o: f64| ((c * alpha + o * (1.0 - alpha)) * 255.0).round() as u32;
    (mix(colour.0, onto.0), mix(colour.1, onto.1), mix(colour.2, onto.2))
}

/// A ground back in 0..1 for [`over`] — the inverse of the byte rounding, so the composite is spelled once.
fn ground_to_f64(ground: (u32, u32, u32)) -> (f64, f64, f64) {
    let one = |c: u32| c as f64 / 255.0;
    (one(ground.0), one(ground.1), one(ground.2))
}

/// The band's own layout for the lane shot: `snapshot_lane`'s scene, rebuilt here so a check can ask the painter's
/// doors (`band_span_px`) where an effect LANDS instead of guessing `t * pps`. `Scene` has no `Default` — every
/// field is a decision the page makes — so these are the same decisions `snapshot_lane::scene` makes.
fn probe_scene(cut: &Cut, width: i32) -> Scene {
    Scene {
        pps: snapshot_lane::PPS,
        origin: 0.0,
        segs: if cut.segs.is_empty() {
            vec![Seg { s: 0.0, e: width as f64 / snapshot_lane::PPS, cam: 0, ..Default::default() }]
        } else {
            cut.segs.clone()
        },
        fx: Vec::new(),
        recordings: vec![naivepost::timeline::Recording { base: "session-tape".into(), start: 0.0, end: 400.0 }],
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

/// The lane's own geometry, asked of the same doors `cut_tracks::paint_at` lays its rows out by.
struct LaneGeo {
    top: f64,
    row_h: f64,
    pictures: f64,
}

fn lane_geo(lane_rows: usize) -> LaneGeo {
    let top = naivepost::ui::cut_band_rows::lane_top_px(naivepost::cut_trim::RULER_H, cut_tracks::SELECTION_BAND_PX);
    LaneGeo {
        top,
        row_h: fx_lane::lane_height_px(1),
        pictures: naivepost::ui::cut_band_rows::pictures_top_px(top, lane_rows.max(1) as f64),
    }
}

/// The band's own backdrop for every x: the same scene painted with `fx: []` (which still costs its one empty row
/// per §2), read at the lane row. Asked of the painter, so no expectation re-derives the layout or drifts from it.
fn painted_grounds(cut: &Cut, dir: &Path, width: i32, lane_rows: usize) -> Vec<(u32, u32, u32)> {
    let bare = Cut { fx: Vec::new(), ..cut.clone() };
    let out = dir.join("bare-lane.png");
    snapshot_lane::write_png(&bare, &out).expect("the empty lane paints");
    let shot = decode(&out);
    let geo = lane_geo(lane_rows);
    let y = (geo.top + geo.row_h / 2.0) as i32;
    (0..width).map(|x| shot.pixel(x, y)).collect()
}

/// The same cut with ONE effect left in it, painted into `dir` and decoded: the only ink on that surface is the bar
/// being asserted, so the assert can be neither satisfied by a neighbour's wash nor defeated by one. The staged
/// lane overlaps (a speed's stretch covers an earlier zoom), which is why sampling the finished lane against a
/// single bar's wash would be comparing two washes.
fn alone_shot(cut: &Cut, dir: &Path, keep: usize, tag: &str) -> Decoded {
    let only = Cut { fx: vec![cut.fx[keep].clone()], ..cut.clone() };
    let out = dir.join(format!("only-{tag}.png"));
    snapshot_lane::write_png(&only, &out).expect("one bar paints");
    decode(&out)
}

/// Where a bar's bare wash sits in the shot: its painted span's middle along the tape (asked of `band_span_px`,
/// the painter's own seconds→x door), and a few px under its row's top border — `paint_bar` fills the whole row
/// and inks its mark and caption on the row's centre line, so a centre sample reads glyph or white, never wash.
fn bar_sample(scene: &Scene, cut: &Cut, index: usize, row: usize) -> (i32, i32) {
    let geo = lane_geo(1);
    let (from, to) = cut.fx[index].spans();
    let (x0, width) = cut_tracks::band_span_px(scene, from, to);
    ((x0 + width / 2.0) as i32, (geo.top + row as f64 * geo.row_h + 4.0) as i32)
}

// --- the LANE as a rendered surface ---------------------------------------------------------------------------

/// §2 (`Rows ... an effect going on the topmost row where it fits`; `Colours: ...`): the file
/// `just snapshot 06-lane` writes shows one bar per kind, each in its own colour, between the selection band above
/// and the picture rows below.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s19_the_lane_file_shows_every_kind_in_its_colour() {
    let dir = temp_root("lane-colours");
    pin_channel_order(&dir);
    let cut = snapshot_lane::staged_cut();
    let out = dir.join("06-lane.png");
    let said = snapshot_lane::write_png(&cut, &out).expect("the lane paints");
    let shot = decode(&out);
    let bars = fx_lane::lane_layout(&cut.fx);
    assert!(!bars.is_empty(), "the staged cut carries effects: {said}");
    let scene = probe_scene(&cut, shot.width);
    let grounds = painted_grounds(&cut, &dir, shot.width, bars.iter().map(|b| b.row).max().unwrap_or(0) + 1);

    let mut seen_kinds = 0;
    for bar in &bars {
        // A label is a TAG (`fx_lane::bar_wash_alpha(Label)` covers only its stub), so its wash is asserted in the
        // sibling file that already samples the tag; here every full-wash kind is measured over its own span.
        if bar.kind == EffectKind::Label {
            continue;
        }
        let alone = alone_shot(&cut, &dir, bar.index, &format!("k{}", bar.index));
        let (x, y) = bar_sample(&scene, &cut, bar.index, bar.row);
        let ground = grounds[x as usize];
        let want = over(bar.colour, fx_lane::bar_wash_alpha(bar.kind), ground_to_f64(ground));
        assert!(
            near(alone.pixel(x, y), want, 3),
            "bar {} ({:?}) at its own middle reads {:?}; the wash over the ground there would be {want:?}",
            bar.index,
            bar.kind,
            alone.pixel(x, y),
        );
        // And the finished lane carries ink at the same pixel, so what was asserted is about the file that ships.
        assert!(
            !near(shot.pixel(x, y), ground, 3),
            "bar {} ({:?}) leaves no ink in the finished lane",
            bar.index,
            bar.kind
        );
        seen_kinds += 1;
    }
    assert!(seen_kinds >= 5, "only {seen_kinds} bars were sampled; the lane lost kinds");

    // The colours §2 names, one by one, through the door the painter reads: a staying zoom is the orange neighbour
    // of the cyan one, and a speed's rose is the rose of the wash over the pictures.
    let zoom = bars.iter().find(|b| b.kind == EffectKind::Zoom && !b.staying).expect("a zoom bar");
    let staying = bars.iter().find(|b| b.staying).expect("a staying zoom");
    let speed = bars.iter().find(|b| b.kind == EffectKind::Speed).expect("a speed bar");
    assert_eq!(zoom.colour, fx_lane::bar_colour(EffectKind::Zoom, false));
    assert_eq!(staying.colour, fx_lane::bar_colour(EffectKind::Zoom, true));
    assert_ne!(zoom.colour, staying.colour, "a staying zoom and a zoom are the same bar colour");
    assert_eq!(speed.colour, fx_lane::bar_colour(EffectKind::Speed, false));
}

/// §2 (the lane "sits between the selection band above and the picture rows below"): the painted bar lands in that
/// gap, and its wash also reaches DOWN onto the picture row, which is what "a rose wash on the pictures" is in
/// bytes. A bar painted anywhere else would satisfy the colour check above and still not be the lane.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s20_the_wash_lies_between_the_band_and_the_pictures() {
    let dir = temp_root("lane-rows");
    pin_channel_order(&dir);
    let cut = snapshot_lane::staged_cut();
    let out = dir.join("06-lane.png");
    snapshot_lane::write_png(&cut, &out).expect("the lane paints");
    let shot = decode(&out);
    let bars = fx_lane::lane_layout(&cut.fx);
    let geo = lane_geo(bars.iter().map(|b| b.row).max().unwrap_or(0) + 1);
    let scene = probe_scene(&cut, shot.width);
    let grounds = painted_grounds(&cut, &dir, shot.width, bars.iter().map(|b| b.row).max().unwrap_or(0) + 1);

    // The lane's top is BELOW the selection band's bottom: ruler + selection band, asked of the same doors.
    let band_bottom = naivepost::cut_trim::RULER_H + cut_tracks::SELECTION_BAND_PX;
    assert_eq!(geo.top, band_bottom, "the lane must begin where the selection band ends");
    assert!(geo.pictures > geo.top, "the picture rows must begin below the lane");

    // The speed's bar: inside the lane (below its top, above the picture rows) ...
    let speed = bars.iter().find(|b| b.kind == EffectKind::Speed).expect("a speed bar");
    let (x, y) = bar_sample(&scene, &cut, speed.index, speed.row);
    assert!(
        (y as f64) >= geo.top && (y as f64) < geo.pictures,
        "the speed bar's sample y {y} fell outside the lane ({} .. {})",
        geo.top,
        geo.pictures
    );
    let alone = alone_shot(&cut, &dir, speed.index, "speed");
    let want = over(speed.colour, fx_lane::bar_wash_alpha(EffectKind::Speed), ground_to_f64(grounds[x as usize]));
    assert!(near(alone.pixel(x, y), want, 3), "the speed's wash is not over its own span: {:?}", alone.pixel(x, y));

    // ... and its rose wash over the PICTURE row under the lane, wherever the seconds overlap. This one is a
    // DIRECTION check, not an exact blend: that row carries more than the grey `draw_picture_rows` lays down
    // (`draw_row_wave_strips` and `draw_recording_edges` ink the same band), so no single door hands back "the
    // ground under the pictures" and an alpha compare there would chase a layer instead of the rule §2 states.
    let (from, to) = cut.fx[speed.index].spans();
    let (x0, width) = cut_tracks::band_span_px(&scene, from, to);
    let picture_x = (x0 + width / 2.0) as i32;
    let picture_y = (geo.pictures + 4.0) as i32;
    let bare = decode(&dir.join("bare-lane.png"));
    let washed = alone.pixel(picture_x, picture_y);
    let ground = bare.pixel(picture_x, picture_y);
    // The wash reaches DOWN onto the pictures (§2): the pixel changed at all, and moved toward rose — red up,
    // green down, whatever the row underneath was. A grey smudge or a black band cannot do both.
    assert!(!near(washed, ground, 6), "the speed's wash leaves the picture row untouched: {washed:?} == {ground:?}");
    assert!(
        washed.0 > ground.0 && washed.1 < ground.1,
        "the change on the picture row is not rose-tinted: {ground:?} -> {washed:?}"
    );
}

// --- the PREVIEW as a rendered surface ------------------------------------------------------------------------

/// One stop standing on `t`, plus the zoom whose camera frames it: the minimum cut that can show §2's still.
fn stop_cut(t: f64) -> Cut {
    Cut {
        segs: vec![Seg { s: 0.0, e: 300.0, cam: 0, ..Default::default() }],
        fx: vec![
            Fx {
                kind: "zoom".to_string(),
                t: t - 4.0,
                dur: 12.0,
                trans: 0.0,
                tout: 0.0,
                cx: Some(0.3),
                cy: Some(0.35),
                hf: Some(0.5),
                ..Default::default()
            },
            Fx {
                kind: "speed".to_string(),
                t: t - 1.0,
                dur: 3.0,
                trans: 0.0,
                tout: 0.0,
                rate: 0.0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

/// §2 (`Preview (paused): outside the camera rect dimmed black 0.45; rect stroked white ...; A stop's still:
/// rendered from the scene's own camera, fitted on the footage's transform`): the file
/// `just snapshot 06-preview` writes shows the record inside the crop undimmed, the surround dimmed, the white
/// rect, and the still as a CROP of the staged frame rather than a flat plate.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s21_the_preview_file_shows_crop_dim_and_still() {
    let dir = temp_root("frame-crop").join("session.naivepost");
    std::fs::create_dir_all(&dir).expect("project folder");
    pin_channel_order(&dir);
    let t = 20.0;
    let frame = snapshot_face::seed_structured_frame(&dir, "session-tape", t).expect("a staged frame is written");
    assert!(frame.is_file(), "the staged frame landed at {frame:?}");

    let cut = stop_cut(t);
    let out = dir.join("06-preview.png");
    let said = snapshot_preview_frame::write_png(&cut, t, Some(&frame), false, &out).expect("the frame paints");
    assert!(said.contains("stop still: true"), "the log says no still: {said}");
    let shot = decode(&out);

    // Geometry asked of the same doors: the footage fitted into the shot's box (a no-op at this size) and the
    // camera rect that divides the still's region from the dimmed surround.
    let fit = cut_preview_frame::fit_size(
        snapshot_face::STRUCTURED_FRAME_W as f64,
        snapshot_face::STRUCTURED_FRAME_H as f64,
        snapshot_face::STRUCTURED_FRAME_W as f64,
        snapshot_face::STRUCTURED_FRAME_H as f64,
    );
    let camera = naivepost::fx_zoom::camera_at(&cut.fx, t);
    let (rx, ry, rw, rh) = cut_preview_frame::camera_rect_px(&camera, fit);
    assert!(rw < fit.0 - 10.0, "a camera that fills the frame leaves nothing to dim: {camera:?}");
    let still = cut_preview_frame::stop_px(&cut.fx, t, fit).expect("a stop stands on this second");
    assert_eq!(still, (rx, ry, rw, rh), "the still is not the scene's own camera box");

    let sky: (f64, f64, f64) = snapshot_face::BAND_SKY;
    let mid: (f64, f64, f64) = snapshot_face::BAND_MID;
    let lower: (f64, f64, f64) = snapshot_face::BAND_LOWER;
    let bright: (f64, f64, f64) = snapshot_face::BLOCK_BRIGHT;

    // INSIDE the rect: the record undimmed — one of the staged bands, and NOT letterbox black. Away from the rect's
    // white stroke, so the assert is about the picture, not the outline.
    let inside = shot.pixel((rx + rw * 0.4) as i32, (ry + rh * 0.4) as i32);
    let a_band = [sky, mid, lower, bright].iter().any(|band| near(inside, over(*band, 1.0, *band), 3));
    assert!(a_band, "inside the camera rect should be the record undimmed, got {inside:?}");
    assert!(!near(inside, (0, 0, 0), 12), "the still's region came out letterbox black, so no still is on it: {inside:?}");

    // OUTSIDE it: the same record dimmed black at §2's alpha, derived from `fx_lane::DIM_ALPHA` — different from
    // the bare band above, which is what "the dim is on screen" means as a number.
    let outside_y = (ry + rh + (fit.1 - ry - rh) / 2.0) as i32;
    let outside = shot.pixel((rx + rw / 2.0) as i32, outside_y);
    let band_under = [(0usize, sky), (1, mid), (2, lower)]
        .iter()
        .find(|(index, _)| {
            let top = (*index) as f64 * fit.1 / 3.0;
            (outside_y as f64) >= top && (outside_y as f64) < top + fit.1 / 3.0
        })
        .map(|(_, band)| *band)
        .unwrap_or(mid);
    let dimmed = over((0.0, 0.0, 0.0), fx_lane::DIM_ALPHA, band_under);
    assert!(
        near(outside, dimmed, 4),
        "outside the camera rect must be the record dimmed black at {}, got {outside:?}, expected {dimmed:?}",
        fx_lane::DIM_ALPHA
    );
    assert!(!near(outside, over(band_under, 1.0, band_under), 6), "the surround is undimmed, so the crop reads as the whole frame: {outside:?}");

    // The stroked rect itself: white somewhere along its left edge — the third thing §2 lists for the paused frame.
    let stroke = shot.pixel(rx as i32, (ry + rh / 2.0) as i32);
    assert!(stroke.0 > 200 && stroke.1 > 200 && stroke.2 > 200, "the camera rect is not stroked white: {stroke:?}");
}

/// §2 (`visible overlays at real alpha`) for the caption: a text over its seconds paints words INSIDE the crop, and
/// the same caption outside its seconds paints none — the alpha answer has to show up as ink and its absence.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s22_the_caption_shows_inside_and_not_outside() {
    let dir = temp_root("caption").join("session.naivepost");
    std::fs::create_dir_all(&dir).expect("project folder");
    pin_channel_order(&dir);
    let t = 30.0;
    let frame = snapshot_face::seed_structured_frame(&dir, "session-tape", t).expect("a staged frame is written");
    let caption = Fx {
        kind: "text".to_string(),
        t: t - 2.0,
        dur: 6.0,
        trans: 0.0,
        tout: 0.0,
        text: "Ethereum in one lecture".to_string(),
        ..Default::default()
    };
    let cut = Cut {
        segs: vec![Seg { s: 0.0, e: 300.0, cam: 0, ..Default::default() }],
        fx: vec![caption.clone()],
        ..Default::default()
    };
    // Alpha asked of the view-model first: the same second the shot paints must be a VISIBLE overlay there.
    let scene_paused = fx_lane::paused_scene(&cut.fx, t, None, false);
    assert!(
        scene_paused.overlays.iter().any(|(_, _, alpha)| *alpha > 0.9),
        "the caption is not visible at its own second: {:?}",
        scene_paused.overlays
    );

    let on = dir.join("caption-on.png");
    snapshot_preview_frame::write_png(&cut, t, Some(&frame), false, &on).expect("the frame paints");
    let off = dir.join("caption-off.png");
    snapshot_preview_frame::write_png(&cut, t + 40.0, Some(&frame), false, &off).expect("the frame paints");
    let (on_shot, off_shot) = (decode(&on), decode(&off));
    assert_eq!((on_shot.width, on_shot.height), (off_shot.width, off_shot.height));

    // The caption's default box (§F3.4 S2's lower third) measured in the shot's own px, the way the painter
    // measures it: fractions of the FITTED footage. Any pixel that differs between the two frames is the caption.
    let fit = cut_preview_frame::fit_size(
        snapshot_face::STRUCTURED_FRAME_W as f64,
        snapshot_face::STRUCTURED_FRAME_H as f64,
        snapshot_face::STRUCTURED_FRAME_W as f64,
        snapshot_face::STRUCTURED_FRAME_H as f64,
    );
    let box_ = naivepost::fx_text::LOWER_THIRD.to_px(fit.0, fit.1);
    let mut ink = 0;
    for y in (box_.1 as i32..(box_.1 + box_.3) as i32).step_by(2) {
        for x in (box_.0 as i32..(box_.0 + box_.2) as i32).step_by(2) {
            if !near(on_shot.pixel(x, y), off_shot.pixel(x, y), 6) {
                ink += 1;
            }
        }
    }
    assert!(ink > 40, "the caption drew {ink} inked samples inside its box; §2 wants visible overlays at real alpha");
}

// --- the Effect menu's rows, as the snapshot stages them ------------------------------------------------------

/// §2's third picture is the popover's six rows in `fx_lane::MENU_ORDER`. `sec_06_effects_2_staging.rs` drives
/// the same door and asserts the rows themselves; what is pinned here is the DISJOINT claim each subject makes on
/// its own filename — the property that decides whether `shots/06-effect-menu.png` holds the menu or a page.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s23_each_subject_holds_its_own_spec_filename() {
    use naivepost::snapshot_subjects;
    for screen in ["06-lane", "06-preview", "06-effect-menu"] {
        assert!(snapshot_subjects::subject_takes_the_name(screen), "{screen}'s subject must hold the asked filename");
    }
    for ordinary in ["05-cut", "03-window", "07-narrate"] {
        assert!(!snapshot_subjects::subject_takes_the_name(ordinary), "{ordinary} is a picture OF THE PAGE");
        assert_eq!(snapshot_subjects::subject_sidecar(ordinary), None);
    }
    for screen in ["06-lane", "06-preview", "06-effect-menu", "05-cut", "03-window"] {
        let claimed = [
            snapshot_lane::is_lane_subject(screen),
            snapshot_preview_frame::is_frame_subject(screen),
            naivepost::snapshot_effect_menu::is_menu_subject(screen),
        ]
        .iter()
        .filter(|takes| **takes)
        .count();
        assert!(claimed <= 1, "{screen} claims {claimed} subjects at once -- the shot could not pick one");
    }
}
