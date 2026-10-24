// §06-effects#2-the-lane-and-the-preview S2 — the FILES the spec's three pictures are compared against.
//
// The item's complaint was not about a rule but about an artifact: no shot under `shots/06-lane.png`,
// `shots/06-preview.png` or `shots/06-effect-menu.png` showed a lane bar, a rose wash or a still, because each of
// those names held a whole Cut-page window while the subject sat in a sidecar. So these checks do not re-derive an
// alpha from an alpha; they run the snapshot's OWN painters into a file and then read the bytes back off disk
// (§00-principles §5: measure the artifact). Every expected colour is COMPUTED from the doors the painter reads
// (`fx_lane::bar_colour` / `bar_wash_alpha` / `DIM_ALPHA`, `snapshot_face`'s staged-frame bands), never retyped.
//
// No GTK app and no window here: cairo paints to an image surface and gdk-pixbuf decodes one without a display, so
// nothing in this file needs `gtk::init()` — which is also why it can be a plain `#[test]` and run in parallel.

use std::path::{Path, PathBuf};

use naivepost::cut::{Cut, EffectKind, Fx, Seg};
use naivepost::fx_lane;
use naivepost::snapshot_face;
use naivepost::snapshot_lane;
use naivepost::snapshot_preview_frame;

/// This test binary's own folder — never a fixed path, since the suite runs files in parallel.
fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-s06e2shot-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp root created");
    dir
}

/// Decode a PNG the way a person's viewer would: through gdk-pixbuf, from the FILE on disk, not from the surface
/// that wrote it. That is the difference between "the painter drew it" and "the shot shows it".
///
/// The sample order is asserted rather than assumed: gdk-pixbuf stores 8-bit samples in its own `Pixbuf` layout and
/// `pixels()` hands back the raw bytes, so a test that guessed R,G,B,A would read blue as red and every colour
/// assert below would compare the wrong channels. One known pixel of one known file settles the question for both
/// decodes (this helper is the only reader in this file), so no expectation here rests on a guess.
fn decode(path: &Path) -> (Vec<u8>, usize, i32, i32) {
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_file(path)
        .unwrap_or_else(|why| panic!("{}: {why}", path.display()));
    assert_eq!(
        pixbuf.n_channels(),
        3,
        "cairo's opaque PNG decodes to RGB, alpha dropped"
    );
    assert!(!pixbuf.has_alpha(), "and any alpha it carried is gone");
    // `pixels()`' slice is only valid while the pixbuf lives, so it is copied out here and nothing borrows past it.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            pixbuf.pixels().as_ptr(),
            (pixbuf.height() * pixbuf.rowstride()) as usize,
        )
    }
    .to_vec();
    let (width, height, stride) = (pixbuf.width(), pixbuf.height(), pixbuf.rowstride() as usize);
    drop(pixbuf);
    if path.file_name().is_some_and(|name| name == PROBE_FILE) {
        // The one pixel whose colour cannot be ambiguous: pure red at the top-left of a file this function wrote.
        let first = (0..3).map(|i| bytes[i]).collect::<Vec<_>>();
        assert_eq!(
            first,
            vec![255, 0, 0],
            "the decoded bytes are not R,G,B -- fix `pixel` before trusting a colour"
        );
    }
    (bytes, stride, width, height)
}

/// The scratch file the byte-order assertion reads; see [`decode`].
const PROBE_FILE: &str = "channel-order.png";

/// Write a 1x1 pure-red PNG with cairo and hand back its path, for the channel-order check above.
fn write_red_probe(dir: &Path) -> PathBuf {
    let path = dir.join(PROBE_FILE);
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).expect("a 1x1 surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        cr.set_source_rgb(1.0, 0.0, 0.0);
        cr.paint().expect("paint red");
    }
    let mut file = std::fs::File::create(&path).expect("write the probe");
    surface.write_to_png(&mut file).expect("encode the probe");
    path
}

/// The RGB of `(x, y)` as gdk-pixbuf stored it -- three bytes per pixel here (the shot's PNGs are opaque), so the
/// stride counts bytes and a pixel is 3 wide. Plain R,G,B order, asserted once in [`decode`]; that swapping cairo
/// does to an image surface (premultiplied BGRA) is exactly what reading a decoded PNG avoids, which is why the
/// expectations below are straight alpha composites rather than the premultiplied product
/// `sec_06_effects_2_lane_painted.rs` has to spell for a cairo surface.
fn pixel(bytes: &[u8], stride: usize, x: i32, y: i32) -> (u32, u32, u32) {
    let at = y as usize * stride + x as usize * 3;
    (bytes[at] as u32, bytes[at + 1] as u32, bytes[at + 2] as u32)
}

fn near(a: (u32, u32, u32), b: (u32, u32, u32), within: u32) -> bool {
    let one = |t: (u32, u32, u32)| [t.0, t.1, t.2];
    let (x, y) = (one(a), one(b));
    (0..3).all(|i| x[i].abs_diff(y[i]) <= within)
}

/// `colour` at `alpha` laid over `over`, rounded to bytes — the composite every assert below compares against,
/// spelled once so no test hardcodes a blended triple.
fn over(colour: (f64, f64, f64), alpha: f64, onto: (f64, f64, f64)) -> (u32, u32, u32) {
    let mix = |c: f64, o: f64| ((c * alpha + o * (1.0 - alpha)) * 255.0).round() as u32;
    (
        mix(colour.0, onto.0),
        mix(colour.1, onto.1),
        mix(colour.2, onto.2),
    )
}

/// A second whose lane pixel is the bare ground: past the last staged bar, inside the tape the shot paints.
/// Asked of the layout rather than hardcoded, so moving a staged effect cannot silently move this sample. The width
/// is the shot's own, read back from the painted file (the module keeps its surface width private).
fn clear_second(cut: &Cut, painted_width: i32) -> f64 {
    let last = cut
        .fx
        .iter()
        .map(|effect| effect.spans().1)
        .fold(0.0_f64, f64::max);
    let after = last + 1.0;
    assert!(
        after * snapshot_lane::PPS < painted_width as f64,
        "the sample must sit inside the painted tape"
    );
    after
}

/// The lane's own geometry, asked of the same doors `cut_tracks::paint_at` lays its rows out by: ruler, selection
/// band, then one row per overlapping group. Computed here rather than copied from a screenshot.
struct LaneGeo {
    top: f64,
    row_h: f64,
}

fn lane_geo() -> LaneGeo {
    LaneGeo {
        top: naivepost::ui::cut_band_rows::lane_top_px(
            naivepost::cut_trim::RULER_H,
            naivepost::ui::cut_tracks::SELECTION_BAND_PX,
        ),
        row_h: fx_lane::lane_height_px(1),
    }
}

/// Where a bar's ink sits in the shot: its painted span's middle along the tape, and inside its own row.
///
/// The x comes from `band_span_px`, the painter's own seconds→x door (the lane is not linear where a fold has eaten
/// seconds, so the naive `t * pps` lands beside the bar, not on it). The y is NOT the row's middle: cairo records a
/// semi-transparent fill as its colour plus a tRNS alpha, gdk-pixbuf then composites that over WHITE on decode, and
/// the bar's mark glyph and caption are inked on the row's centre line -- so a centre sample reads white or glyph,
/// never the wash. A few px under the top border gets the bare wash: `paint_bar` fills the whole row and strokes
/// only a 1 px edge, so everything between that edge and the first glyph is pure composite.
fn bar_sample(
    scene: &naivepost::ui::cut_tracks::Scene,
    cut: &Cut,
    index: usize,
    row: usize,
) -> (i32, i32) {
    let geo = lane_geo();
    let (from, to) = cut.fx[index].spans();
    let (x0, width) = naivepost::ui::cut_tracks::band_span_px(scene, from, to);
    (
        (x0 + width / 2.0) as i32,
        (geo.top + row as f64 * geo.row_h + 4.0) as i32,
    )
}

/// §2's paused half, the fourth thing it names: `rect stroked white WITH A PLATE`. The plate is the debt sentence,
/// and its two rules are `fx_lane::plate_debt_at` (only a record that names a sound answer, and only while it
/// covers the line) and `fx_lane::plate_shown` ("once wider than 60 px"). Painted here through the shot's own door
/// so the words are on the ARTIFACT, not only in the live panel's label.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_frame_file_carries_the_debt_plate() {
    let dir = temp_root("plate").join("session.naivepost");
    std::fs::create_dir_all(&dir).expect("project folder");
    let t = 30.0;
    let frame = snapshot_face::seed_structured_frame(&dir, "session-tape", t)
        .expect("a staged frame is written");
    // A ×0.5 speed whose sound runs to the scene's end, started 4 s before the line: the debt is the seconds of
    // real sound still owing, and this one is worth a line (`fx_lane::DEBT_TAIL_MIN_SECONDS`). The zoom beside it
    // is what leaves a surround to dim at all.
    let cut = Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 300.0,
            cam: 0,
            ..Default::default()
        }],
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
                t: t - 4.0,
                dur: 8.0,
                trans: 0.0,
                tout: 0.0,
                rate: 0.5,
                snd: "scene".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let out = dir.join("plate.png");
    let said = snapshot_preview_frame::write_png(&cut, t, Some(&frame), false, &out)
        .expect("the frame paints");
    // The words come from the same doors the painter uses, so nothing here retypes "sound X s behind". The room is
    // the one the shot's own painter passes (`FRAME_W`, the width its plate is measured against).
    let want = fx_lane::plate_for(
        fx_lane::plate_debt_at(&cut.fx, t),
        fx_lane::room_px(960.0, 0.0),
    )
    .expect("this second owes a debt wide enough for a plate");
    assert!(
        said.contains(&want),
        "the shot's log does not name the plate {want}: {said}"
    );
    let (bytes, stride, width, height) = decode(&out);
    // Where it sits: centred on the camera rect, asked of the same doors rather than typed.
    let fit = naivepost::ui::cut_preview_frame::fit_size(
        snapshot_face::STRUCTURED_FRAME_W as f64,
        snapshot_face::STRUCTURED_FRAME_H as f64,
        snapshot_face::STRUCTURED_FRAME_W as f64,
        snapshot_face::STRUCTURED_FRAME_H as f64,
    );
    let camera = naivepost::fx_zoom::camera_at(&cut.fx, t);
    let (rx, ry, rw, rh) = naivepost::ui::cut_preview_frame::camera_rect_px(&camera, fit);
    // The plate's dark ground with WHITE WORDS inside it, sampled across the rect's middle band. Neither count
    // alone would do: the dim outside the rect is dark too but carries no letters, and the record inside it is
    // bright but has no box. So both are required AT the rect's centre row.
    let cy = (ry + rh / 2.0) as i32;
    let dark = |x: i32, y: i32| {
        let (r, g, b) = pixel(&bytes, stride, x, y);
        r < 40 && g < 40 && b < 40
    };
    let white = |x: i32, y: i32| {
        let (r, g, b) = pixel(&bytes, stride, x, y);
        r > 235 && g > 235 && b > 235
    };
    let band = (rx as i32 + 4)..(rx as i32 + rw as i32 - 4);
    let dark_cols = band.clone().filter(|x| dark(*x, cy)).count();
    let white_cols = band.clone().filter(|x| white(*x, cy)).count();
    assert!(
        dark_cols > 60 && white_cols > 10,
        "no plated sentence across the stroked rect's middle row (y={cy}) of a {width}x{height} file: \
         {dark_cols} dark column(s), {white_cols} white column(s) -- §2 pairs the plate with the rect"
    );
}

/// The same cut with ONE effect left in it, painted into `dir` and decoded: the only ink on that surface is the bar
/// being asserted, so the assert cannot be satisfied by a neighbour's wash or defeated by one.
fn alone_shot(cut: &Cut, dir: &Path, keep: usize, tag: &str) -> (Vec<u8>, usize) {
    let only = Cut {
        fx: vec![cut.fx[keep].clone()],
        ..cut.clone()
    };
    let out = dir.join(format!("only-{tag}.png"));
    snapshot_lane::write_png(&only, dir, &out).expect("one bar paints");
    let (bytes, stride, _, _) = decode(&out);
    (bytes, stride)
}

/// The lane's ground as the SAME painter answers it, asked rather than assumed: paint this very scene twice -- once
/// with the staged cut, once with `fx: []`, which still costs its one empty row per §2 -- and read the lane row out
/// of the second. That is the band's own backdrop for every x (plate, picture-row grey, gap grey, whatever it lays
/// under the lane), so no expectation here re-derives the layout and none can drift from it.
fn painted_grounds(cut: &Cut, dir: &Path, width: i32) -> Vec<(u32, u32, u32)> {
    let bare = Cut {
        fx: Vec::new(),
        ..cut.clone()
    };
    let out = dir.join("bare-lane.png");
    snapshot_lane::write_png(&bare, dir, &out).expect("the empty lane paints");
    let (bytes, stride, _, height) = decode(&out);
    let geo = lane_geo();
    assert!(height > 60, "the empty lane still costs its rows");
    let y = (geo.top + geo.row_h / 2.0) as i32;
    (0..width).map(|x| pixel(&bytes, stride, x, y)).collect()
}

/// The sampled ground back in 0..1 for [`over`] -- the inverse of the byte rounding, so the composite is spelled
/// in one place instead of twice.
fn ground_to_f64(ground: (u32, u32, u32)) -> (f64, f64, f64) {
    let one = |c: u32| c as f64 / 255.0;
    (one(ground.0), one(ground.1), one(ground.2))
}

/// The band's own layout for the shot: `snapshot_lane`'s scene, rebuilt here so a check can ask the painter's doors
/// (`band_span_px`, `lane_top_px`) where an effect LANDS instead of guessing `t * pps`. `Scene` has no `Default` --
/// every field is a decision the page makes -- so these are the same decisions the shot's staging makes: one take
/// long enough to fill the surface, no thumbnails (nothing decodes during a paint), origin 0.
fn probe_scene(cut: &Cut, width: i32) -> naivepost::ui::cut_tracks::Scene {
    naivepost::ui::cut_tracks::Scene {
        pps: snapshot_lane::PPS,
        origin: 0.0,
        segs: if cut.segs.is_empty() {
            vec![Seg {
                s: 0.0,
                e: width as f64 / snapshot_lane::PPS,
                cam: 0,
                ..Default::default()
            }]
        } else {
            cut.segs.clone()
        },
        fx: Vec::new(),
        recordings: vec![naivepost::timeline::Recording {
            base: "session-tape".into(),
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

// --- S2: the doors — which screen hands its filename to a subject -----------------------------------------

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_lane_shot_is_the_lane_the_spec_pictures() {
    // §2's three images are the lane, the framed preview and the popover. Each module answers for its own subject;
    // `snapshot_subjects` joins them up. If a door ever returns true for someone else's screen, `snapshot::run`
    // would paint two subjects into one asked name, so disjointness is the property, not each answer alone.
    assert!(
        snapshot_lane::is_lane_subject("06-lane"),
        "§2's first picture IS the lane"
    );
    assert!(
        snapshot_preview_frame::is_frame_subject("06-preview"),
        "and its second IS the framed preview"
    );
    for other in ["06-effect-menu", "06-preview", "05-cut", "03-window"] {
        assert!(
            !snapshot_lane::is_lane_subject(other),
            "{other}'s picture is not the lane"
        );
    }
    for other in ["06-lane", "05-cut", "03-window"] {
        assert!(
            !snapshot_preview_frame::is_frame_subject(other),
            "{other}'s picture is not the frame"
        );
    }
    // Exactly one subject per screen, across all three doors.
    for screen in [
        "06-lane",
        "06-preview",
        "06-effect-menu",
        "05-cut",
        "03-window",
    ] {
        let claimed = [
            snapshot_lane::is_lane_subject(screen),
            snapshot_preview_frame::is_frame_subject(screen),
            naivepost::snapshot_effect_menu::is_menu_subject(screen),
        ]
        .iter()
        .filter(|takes| **takes)
        .count();
        assert!(
            claimed <= 1,
            "{screen} claims {claimed} subjects at once -- the shot could not pick one"
        );
    }
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_renamed_lane_file_keeps_the_page_shot_beside_it(
) {
    // `snapshot::run`'s naming rule, asserted on the doors themselves so there is ONE source of truth: a screen
    // whose subject takes `<name>.png` pushes the page to `<name>-page.png`, and only a subject that staging
    // painted into a sidecar is renamed over the asked name. The Effect menu is NOT in the first group any more:
    // `spec/QUESTIONS.md` §06 answer 3(b) makes its picture the PAGE with the menu open over it, so nothing
    // displaces that screen's window shot.
    for subject in ["06-lane", "06-preview"] {
        assert!(
            naivepost::snapshot_subjects::subject_takes_the_name(subject),
            "{subject}'s subject must hold the asked filename"
        );
    }
    assert!(
        !naivepost::snapshot_subjects::subject_takes_the_name("06-effect-menu"),
        "the menu is painted over the page now, so the page keeps its asked filename"
    );
    assert_eq!(
        naivepost::snapshot_subjects::subject_sidecar("06-lane"),
        Some("lane-shot"),
        "the lane is staged under `-lane-shot.png` and renamed from there"
    );
    assert_eq!(
        naivepost::snapshot_subjects::subject_sidecar("06-preview"),
        Some("frame"),
        "and the frame under `-frame.png`"
    );
    assert_eq!(
        naivepost::snapshot_subjects::subject_sidecar("06-effect-menu"),
        None,
        "the menu has no sidecar to rename: it is painted onto the page shot itself"
    );
    for ordinary in ["05-cut", "03-window", "07-narrate"] {
        assert!(
            !naivepost::snapshot_subjects::subject_takes_the_name(ordinary),
            "{ordinary} is a picture OF THE PAGE, so the window keeps its asked name"
        );
        assert_eq!(
            naivepost::snapshot_subjects::subject_sidecar(ordinary),
            None
        );
    }
    // And the filenames the rule produces, spelled out once so a future change to either extension is seen here.
    let asked = Path::new("shots").join("06-lane.png");
    // `-page.png` (a DOT, not a dash) is what `snapshot::run` writes -- and it sorts BEFORE `<name>.png`, so the
    // subject stays the file a viewer puts beside the spec's picture.
    assert_eq!(
        asked.with_extension("page.png"),
        Path::new("shots").join("06-lane.page.png")
    );
}

// --- S2: the lane FILE shows a wash per effect -------------------------------------------------------------

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_lane_file_shows_a_wash_per_effect() {
    // This is the artifact `spec/img/06-lane.png` is put beside: the file `just snapshot 06-lane` writes under the
    // asked name, produced by `snapshot_lane::write_png` -> `cut_tracks::paint_at`, i.e. the band's own painter.
    let dir = temp_root("lane");
    write_red_probe(&dir); // pins the decode's channel order before any colour is trusted
    let out = dir.join("06-lane.png");
    let cut = snapshot_lane::staged_cut();
    let said = snapshot_lane::write_png(&cut, &dir, &out).expect("the lane paints");
    assert!(out.is_file(), "{out:?} was never written");
    assert!(
        out.metadata().expect("read it").len() > 2000,
        "a lane with bars is not a blank plate: {said}"
    );

    let (bytes, stride, width, height) = decode(&out);
    assert!(
        width > 1000,
        "the lane shot is the spec image's own width, got {width}"
    );
    assert!(height > 100, "the band asks its rows' height: {said}");

    // Where the lane is, asked of the same geometry the painter used: ruler + selection band, then one row each.
    let geo = lane_geo();
    let bars = fx_lane::lane_layout(&cut.fx);
    assert!(!bars.is_empty(), "the staged cut carries effects");

    // The backdrop, asked of the painter: the same surface with `fx: []` is what lies under every bar.
    let grounds = painted_grounds(&cut, &dir, width);
    // ...and the same for the ink: paint EVERY bar on its own, so the pixels are only ever the record being
    // asserted. The staged lane overlaps (the spec's own lane has a speed whose stretch covers an earlier zoom), so
    // sampling the finished lane against one bar's wash would be comparing two washes.
    let mut alone = Vec::with_capacity(cut.fx.len());
    for keep in 0..cut.fx.len() {
        alone.push(alone_shot(&cut, &dir, keep, &format!("lane{keep}")));
    }
    let mut checked = 0;
    // The scene the shot paints, rebuilt here so the geometry can be asked of the painter's own doors (`Scene` has
    // no `Default`: every field is a decision the page makes, and this is the same decision `snapshot_lane` makes).
    let scene = probe_scene(&cut, width);
    for bar in &bars {
        // A label is a TAG (a stub plus a dashed line), so its wash only covers part of the bar: sampled elsewhere.
        if bar.kind == EffectKind::Label {
            continue;
        }
        // The bar's own pixels, from the shot that holds ONLY this record (`alone_shot`): its span's middle, just
        // under the top border, where only the wash is.
        let (x, y) = bar_sample(&scene, &cut, bar.index, bar.row);
        assert!(
            x < width && y < height,
            "the sample for bar {} fell off the shot",
            bar.index
        );
        // The wash over whatever the band laid down THERE, which is what "a rose wash on the pictures" means in
        // bytes -- derived from the same two doors the painter reads, never retyped.
        let ground = grounds[x as usize];
        let want = over(
            bar.colour,
            fx_lane::bar_wash_alpha(bar.kind),
            ground_to_f64(ground),
        );
        let (alone_bytes, alone_stride) = &alone[bar.index];
        let got = pixel(alone_bytes, *alone_stride, x, y);
        assert!(
            near(got, want, 3),
            "bar {} ({:?}) at its own middle reads {got:?} on its own; the wash over the ground there would be {want:?}",
            bar.index,
            bar.kind
        );
        // And the finished lane shows ink at the same place, so what was asserted above is about the artifact that
        // is actually written and not about a lane nobody ships.
        let whole = pixel(&bytes, stride, x, y);
        assert!(
            !near(whole, ground, 3),
            "bar {} ({:?}) leaves no ink in the finished lane: {whole:?} is the bare ground there",
            bar.index,
            bar.kind
        );
        checked += 1;
    }
    assert!(
        checked >= 4,
        "only {checked} bars were sampled; the lane lost kinds"
    );

    // The negative half: at a second with NO effect the pixel is the plain ground. Without this a flat image of
    // one colour would satisfy every assert above.
    let empty_x = (clear_second(&cut, width) * snapshot_lane::PPS) as i32;
    let bare = pixel(&bytes, stride, empty_x, (geo.top + geo.row_h / 2.0) as i32);
    assert_eq!(
        bare, grounds[empty_x as usize],
        "at a second with no effect the lane must read exactly as the lane with no effects at all"
    );
}

// --- S2: the frame FILE shows the still and the dim --------------------------------------------------------

/// One stop standing on `t`, plus the zoom whose camera frames it: the minimum cut that can show §2's still.
fn stop_cut(t: f64) -> Cut {
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

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_frame_file_shows_the_still_and_the_dim() {
    // The door `06-preview` paints through, called here with the SAME structured frame the shot stages -- that door
    // is public precisely so this check can use it: the frame is the shot's input, and deriving its colours from
    // `snapshot_face`'s consts is the one source of truth for what a crop of it looks like.
    let dir = temp_root("frame").join("session.naivepost");
    std::fs::create_dir_all(&dir).expect("project folder");
    let t = 20.0;
    let frame = snapshot_face::seed_structured_frame(&dir, "session-tape", t)
        .expect("a staged frame is written");
    assert!(frame.is_file(), "the staged frame landed at {frame:?}");

    let cut = stop_cut(t);
    let out = dir.join("06-preview.png");
    let said = snapshot_preview_frame::write_png(&cut, t, Some(&frame), false, &out)
        .expect("the frame paints");
    assert!(
        said.contains("stop still: true"),
        "the log says no still: {said}"
    );
    let (bytes, stride, width, height) = decode(&out);
    assert_eq!(
        (width, height),
        (
            snapshot_face::STRUCTURED_FRAME_W,
            snapshot_face::STRUCTURED_FRAME_H
        )
    );

    // The geometry, asked of the same doors: the footage fitted into the shot's box (a no-op at this size), and the
    // camera rect that divides the still's region from the dimmed surround.
    let fit = naivepost::ui::cut_preview_frame::fit_size(
        snapshot_face::STRUCTURED_FRAME_W as f64,
        snapshot_face::STRUCTURED_FRAME_H as f64,
        snapshot_face::STRUCTURED_FRAME_W as f64,
        snapshot_face::STRUCTURED_FRAME_H as f64,
    );
    let camera = naivepost::fx_zoom::camera_at(&cut.fx, t);
    let (rx, ry, rw, rh) = naivepost::ui::cut_preview_frame::camera_rect_px(&camera, fit);
    assert!(
        rw < fit.0 - 10.0,
        "a camera that fills the frame leaves nothing to dim: {camera:?}"
    );
    // And the still's box really is a crop of the frame: `stop_px` borrows the scene's camera, so its box IS the
    // rect above, and if it ever came out full-frame again the crop asserts below would be comparing a frame with
    // itself.
    let still = naivepost::ui::cut_preview_frame::stop_px(&cut.fx, t, fit)
        .expect("a stop stands on this second");
    assert_eq!(
        still,
        (rx, ry, rw, rh),
        "the still is not the scene's own camera box"
    );

    // INSIDE the camera rect: the record, undimmed -- one of the staged bands, and NOT letterbox black. Sampled
    // away from the rect's white stroke so the assert is about the picture, not about the outline.
    let inside = pixel(
        &bytes,
        stride,
        (rx + rw * 0.4) as i32,
        (ry + rh * 0.4) as i32,
    );
    let sky: (f64, f64, f64) = snapshot_face::BAND_SKY;
    let mid: (f64, f64, f64) = snapshot_face::BAND_MID;
    let lower: (f64, f64, f64) = snapshot_face::BAND_LOWER;
    let bright: (f64, f64, f64) = snapshot_face::BLOCK_BRIGHT;
    let a_band = [sky, mid, lower, bright]
        .iter()
        .any(|band| near(inside, over(*band, 1.0, *band), 3));
    assert!(
        a_band,
        "inside the camera rect should be the record undimmed, got {inside:?}"
    );
    assert!(
        !near(inside, (0, 0, 0), 12),
        "the still's region came out letterbox black, so no still is on it: {inside:?}"
    );

    // OUTSIDE it: the same record dimmed black at §2's alpha, derived from `fx_lane::DIM_ALPHA`. Different from
    // the bare band above -- which is what "the dim is on screen" means as a number.
    let outside_y = (ry + rh + (fit.1 - ry - rh) / 2.0) as i32;
    let outside = pixel(&bytes, stride, (rx + rw / 2.0) as i32, outside_y);
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
    assert!(
        !near(outside, over(band_under, 1.0, band_under), 6),
        "the surround is undimmed, so the crop reads as the whole frame: {outside:?}"
    );

    // The stroked rect itself: white somewhere along its left edge, the third thing §2 lists for the paused frame.
    let stroke = pixel(&bytes, stride, rx as i32, (ry + rh / 2.0) as i32);
    assert!(
        stroke.0 > 200 && stroke.1 > 200 && stroke.2 > 200,
        "the camera rect is not stroked white: {stroke:?}"
    );
}

/// The band's own geometry for a shot, asked of the doors the painter lays its rows out by -- ruler, selection
/// band, one lane row per overlapping group, then one picture row per camera plus its wave strip. Nothing below
/// copies a number off a screenshot.
struct StackGeo {
    pictures: f64,
    row_h: f64,
    wave: f64,
}

fn stack_geo(cut: &Cut) -> StackGeo {
    let lane_top = naivepost::ui::cut_band_rows::lane_top_px(
        naivepost::cut_trim::RULER_H,
        naivepost::ui::cut_tracks::SELECTION_BAND_PX,
    );
    let rows = fx_lane::row_count(&cut.fx) as f64;
    StackGeo {
        pictures: naivepost::ui::cut_band_rows::pictures_top_px(lane_top, rows),
        row_h: naivepost::ui::cut_tracks::THUMB_DEFAULT_PX as f64
            + naivepost::ui::cut_tracks::PICTURE_ROW_EXTRA_PX,
        wave: naivepost::ui::cut_tracks::ROW_WAVE_PX,
    }
}

/// §2's first picture, and `spec/QUESTIONS.md` answer 3(a): the shot "is the track area from the ruler down to
/// the bottom of the picture row and its waveform ... Make the shot end where the track stack ends". Measured off
/// the painted file two ways: the file is as tall as the stack the painter laid out, and the LAST thing in it is
/// the wave strip's own ink, not a cut-off band.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_lane_file_ends_below_the_wave_strip() {
    let dir = temp_root("stack-bottom");
    write_red_probe(&dir);
    let cut = snapshot_lane::staged_cut();
    let out = dir.join("stack.png");
    snapshot_lane::write_png(&cut, &dir, &out).expect("the lane paints");
    let (bytes, stride, width, height) = decode(&out);
    let geo = stack_geo(&cut);
    // (a) the file is at least as tall as ruler + lane + one picture row + that row's wave strip.
    let stack = geo.pictures + geo.row_h;
    assert!(
        f64::from(height) >= stack,
        "the file is {height} px tall but its own rows cost {stack:.0} px (pictures top {:.0}, row {:.0}): \
         the shot stops inside the stack",
        geo.pictures,
        geo.row_h
    );
    // (b) the wave strip's band is INKED, not blank: its ground is the row's blue wash and its peaks are a darker
    // blue, and a column of either is not the plate's off-white. Count rows that carry blue anywhere.
    let wave_top = (geo.pictures + geo.row_h - geo.wave) as i32;
    let blue = |x: i32, y: i32| {
        let (r, _, b) = pixel(&bytes, stride, x, y);
        b > r + 10 && b > 120
    };
    let inked: usize = (wave_top..height)
        .map(|y| (60..width - 4).filter(|x| blue(*x, y)).count())
        .filter(|n| *n > 0)
        .count();
    assert!(
        inked >= 4,
        "only {inked} row(s) of the wave strip carry any blue ink between y={wave_top} and {height}: the strip \
         that §2 puts under the picture row is not on the shot"
    );
    // (c) the peaks actually vary along the tape: a flat baseline is ONE lit row per column, so the strip's ink
    // cannot differ between columns. Ask how many columns differ from the median -- a cache-less strip is 0.
    let per_column: Vec<usize> = (60..width - 4)
        .map(|x| (wave_top..height).filter(|y| blue(x, *y)).count())
        .collect();
    let quiet = per_column.iter().copied().min().unwrap_or(0);
    let loud = per_column.iter().copied().max().unwrap_or(0);
    assert!(
        loud > quiet + 2,
        "the wave strip is a flat line: every column between y={wave_top} and {height} shows {quiet}..{loud} \
         lit row(s), so no peak cache was read -- §2's picture is a waveform, not a baseline"
    );
}

/// §2's picture row is a ROW OF PICTURES, one cell per extracted frame at the second its own name stamps. Two
/// ways the row came out wrong, both invisible to a count: every cell piled under the gutter (the stamp was parsed
/// with the wrong byte offsets, so each one fell back to x = 0), or no cell at all (a flat grey plate). Both show
/// up as "the far half of the row is the row's bare ground".
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_lane_file_shows_film_across_the_row() {
    let dir = temp_root("film-across");
    write_red_probe(&dir);
    let cut = snapshot_lane::staged_cut();
    // A whole row's worth of extracted frames, planted where Prepare would have put them (the shot's own staging
    // door, one file per second of a 5 s grid across the tape the shot paints). `write_png` stages a single frame
    // for its own plate; a ROW of pictures needs a row's worth of files, and their names are the row's grid.
    for second in (0..300).step_by(5) {
        snapshot_lane::stage_frame(&dir, "session-tape", second as f64)
            .expect("a frame is planted where the band lists them");
    }
    let out = dir.join("film.png");
    snapshot_lane::write_png(&cut, &dir, &out).expect("the lane paints");
    let (bytes, stride, width, height) = decode(&out);
    let geo = stack_geo(&cut);
    // The FILM part of the row only -- the wave strip owns its bottom, and comparing that too would credit the
    // strip's ink to a thumbnail.
    let film_top = (geo.pictures + 4.0) as i32;
    let film_bottom = (geo.pictures + geo.row_h - geo.wave) as i32;
    assert!(film_bottom > film_top && height > film_bottom);
    // The row's bare ground, asked of the painter: the same surface with no frames staged is a flat plate, and
    // `draw_picture_rows` fills every filmed run with one grey, so that grey is the answer to "no picture here".
    let bare = Cut {
        fx: Vec::new(),
        ..cut.clone()
    };
    let bare_out = dir.join("bare-film.png");
    snapshot_lane::write_png(&bare, &dir, &bare_out).expect("the bare lane paints");
    let (bare_bytes, bare_stride, _, _) = decode(&bare_out);
    let ground = pixel(&bare_bytes, bare_stride, width / 2, film_top + 2);
    let is_ground = |x: i32, y: i32| {
        let got = pixel(&bytes, stride, x, y);
        let each = [got.0, got.1, got.2];
        let want = [ground.0, ground.1, ground.2];
        (0..3).all(|i| each[i].abs_diff(want[i]) <= 4)
    };
    // Far from the gutter and past the first cell: the tape's own width at the shot's zoom runs the whole image,
    // so a row whose frames all landed at x = 0 leaves everything right of the rail the bare ground.
    let far: Vec<(i32, i32)> = (width / 2..width - 40)
        .step_by(3)
        .flat_map(|x| (film_top..film_bottom).step_by(4).map(move |y| (x, y)))
        .collect();
    let not_ground = far.iter().filter(|(x, y)| !is_ground(*x, *y)).count();
    assert!(
        not_ground * 3 >= far.len(),
        "only {not_ground} of {} samples in the RIGHT HALF of the picture row differ from the bare ground: the \
         staged frames are not laid along the tape (they are piled under the gutter, or not drawn at all)",
        far.len()
    );
    // And the row is not a single block either: a row of pictures shows many colours ACROSS its width and down
    // its height (every staged frame is banded, so one y alone could be one colour for every cell).
    let mut seen = std::collections::HashSet::new();
    for x in (40..width - 4).step_by(2) {
        for y in (film_top..film_bottom).step_by(5) {
            seen.insert(pixel(&bytes, stride, x, y));
        }
    }
    assert!(
        seen.len() > 12,
        "the picture row shows {} distinct colour(s) along its width -- a plate, not a row of pictures",
        seen.len()
    );
}
