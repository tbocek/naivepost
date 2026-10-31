//! F2.10's own picture — `spec/img/05-rows.png`: TWO recordings running at the same time, one row each.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_line;
use crate::snapshot_face::seed_snapshot_frame;
use crate::timeline::Recording;
use crate::ui;
use crate::ui::window::find_widget_by_name;

/// Camera 1's file stamp: `spec/05-cut.md`'s badge **1** names the recording "camera 1 (17-25-06, 0:00–0:37)",
/// and §1's footage files carry `YYYY-MM-DD_HH-MM-SS` in their names.
pub const CAM1: &str = "2026-09-16_17-25-06";
/// Camera 2's file stamp — badge **2**'s "camera 2 (17-25-45)".
pub const CAM2: &str = "2026-09-16_17-25-45";
/// How far camera 2 was moved back, which is the spec's own staging ("shifted 19 s back").
pub const CAM2_SHIFT: f64 = -19.0;
/// The FILE STAMP difference between the two cameras, which is the spec's own stamps read as a difference:
pub const CAM2_STAMP: f64 = 39.0;
/// The line's second of camera 2's OWN file, which is `timeline::local_at`'s answer for the line below:
pub const CAM2_FILE_SECOND: f64 = -14.0;
/// The microphone the shot needs a speaker badge and a gutter switch for (badges on S2's row).
pub const MIC: &str = "mic-lav";

/// Width the shot's preview keeps (the panel's own opening width; the tape is beside it, not under it).
pub const PREVIEW_SHOT_W: f64 = 240.0;
/// Height the shot's preview is pinned to.
pub const PREVIEW_SHOT_H: f64 = 160.0;
/// The tape's box: short enough that the rows below it -- what this screen exists to show -- come near the top.
pub const SHOT_TRACKS_W: i32 = 1200;
pub const SHOT_TRACKS_H: i32 = 170;
/// Where the shot parks the page's vertical divider.
pub const SHOT_SPLIT_POS: i32 = 160;
/// How tall the shot asks the window to be.
pub const SHOT_WINDOW_H: i32 = 790;

/// The ceiling the shot puts on the pane above the top/bottom divider.
pub const SHOT_UPPER_CAP_H: i32 = 200;
/// The ceiling on the pane BELOW the top/bottom divider — the tape, the strip, the recorders' band and the two
/// camera rows, which is what this picture is about.
pub const SHOT_LOWER_CAP_H: i32 = 470;
/// How far down the shot scrolls the page, past the preview pane and the tape, to bring both camera rows into
/// the painted frame.
pub const SHOT_SCROLL_DOWN: f64 = 900.0;
/// Where the shot parks the SHELL's page/bottom-bar divider (§1's other draggable divider).
pub const SHELL_PAGE_PX: i32 = 560;
/// The placeholder strip's height in the shot.
pub const SHOT_STRIP_H: i32 = 52;
/// The height the shot asks the window for.
pub const SHOT_WINDOW_ASK_H: i32 = 1200;
/// The line's second: inside the overlap, so the watch sentence is owed (badge **7**) and the preview has a
/// kept scene to convert a second for.
pub const LINE_T: f64 = 25.0;
/// The same second on the PREVIEW's playhead, which is what the band's red line actually paints
/// (`band_scene_for` reads it off the player, not off the line state).
pub const LINE_PLAYHEAD: f64 = 25.0;

/// The two recordings of the figure: camera 1 for its whole 37 s, camera 2 from its shifted start for the
/// same length of tape.
fn recordings() -> Vec<Recording> {
    vec![
        Recording {
            base: CAM1.to_string(),
            start: 0.0,
            end: 37.0,
        },
        // Where the file sits after the correction (0:39 stamped, 19 s back = 0:20), and the same length of
        // tape camera 1 ran.
        Recording {
            base: CAM2.to_string(),
            start: CAM2_STAMP + CAM2_SHIFT,
            end: CAM2_STAMP + CAM2_SHIFT + 37.0,
        },
    ]
}

/// The cut: ONE kept scene, 0–37, taken from camera 1 (badge **6**), plus the microphone lying under it and
/// camera 2's shift correction.
fn figure() -> cut::Cut {
    cut::Cut {
        segs: vec![cut::Seg {
            s: 0.0,
            e: 37.0,
            cam: 0,
            ..Default::default()
        }],
        lanes: vec![cut::Lane {
            name: MIC.to_string(),
            src: format!("project:sources/{MIC}.wav"),
            at: 12.0,
            off: 0.0,
            dur: 45.0,
        }],
        shift: [(CAM2.to_string(), CAM2_SHIFT)].into_iter().collect(),
        ..Default::default()
    }
}

/// The rows a shot must bring into view: `camera-row-1` and `camera-row-2`, by name.
pub const SHOT_ROWS: [&str; 2] = ["camera-row-1", "camera-row-2"];

/// A camera row's top edge in the window's own coordinates (`camera-row-<n>`, counted from 1), or `None` when the
/// band has no such row.
pub fn row_top(window: &adw::ApplicationWindow, name: &str) -> Option<i32> {
    let row: usize = name
        .strip_prefix("camera-row-")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)?;
    let scene = crate::ui::cut_view::band_scene_for();
    if row >= scene.row_count.max(1) {
        return None;
    }
    let band = find_widget_by_name(window.upcast_ref(), "track-band")?;
    let native = band.native()?;
    // The window's own coordinates: `translate_coordinates` into the native is the walk GTK documents for
    // "where in the painted surface is this widget", and the frame `write_png` captures IS that surface.
    #[allow(deprecated)] // the shot's own walk; gtk 4.12's replacement needs a surface node
    let (_, band_top) = band.translate_coordinates(&native, 0.0, 0.0)?;
    let rows = crate::ui::cut_tracks::rows_for_press(&scene);
    Some((band_top + rows.pictures + row as f64 * rows.row_h) as i32)
}

/// The page's own scroll, driven through its `gtk::Adjustment`: the shell wraps every page in
/// `page-scroll-‹tab›`, so moving that adjustment is what a wheel or a trackpad swipe does to it.
pub fn scroll_page(window: &adw::ApplicationWindow, dy: f64) {
    let Some(scroller) = page_scroller(window) else {
        return;
    };
    let adjustment = scroller.vadjustment();
    let top = (adjustment.upper() - adjustment.page_size()).max(0.0);
    adjustment.set_value((adjustment.value() + dy).clamp(0.0, top));
    crate::snapshot_face::pump_context(32);
}

/// How far the page had scrolled when the shot was taken -- printed by `stage`, because "the rows are in the
/// frame" is a claim about the scroll position as much as about the layout.
pub fn scroll_value(window: &adw::ApplicationWindow) -> f64 {
    let adjustment = page_scroller(window).map(|s| s.vadjustment());
    let (value, upper, page) = match adjustment {
        Some(a) => (a.value(), a.upper(), a.page_size()),
        None => return 0.0,
    };
    println!("scroll value {value:.0} of upper {upper:.0} page {page:.0}");
    value
}

/// The page's travel: how much further it CAN scroll, i.e. `upper - page_size` of its own adjustment.
pub fn scroll_travel(window: &adw::ApplicationWindow) -> f64 {
    page_scroller(window).map_or(0.0, |scroller| {
        let adjustment = scroller.vadjustment();
        (adjustment.upper() - adjustment.page_size()).max(0.0)
    })
}

/// Pull the shell's page/bottom-bar divider up to free height for the page.
pub fn raise_shell_split(window: &adw::ApplicationWindow, page_px: i32) {
    fn find_run_row(node: &gtk::Widget) -> Option<gtk::Widget> {
        if node.widget_name() == "run-bar" {
            return node.parent();
        }
        for child in node
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
        {
            if let Some(widget) = child.downcast_ref::<gtk::Widget>() {
                if let Some(found) = find_run_row(widget) {
                    return Some(found);
                }
            }
        }
        None
    }
    let Some(bottom) = find_run_row(window.upcast_ref()) else {
        return;
    };
    let mut candidate = bottom.parent();
    while let Some(paned) = candidate {
        let holds = paned
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .any(|child| child == bottom.clone().upcast::<glib::Object>());
        if holds {
            if let Some(split) = paned.downcast_ref::<gtk::Paned>() {
                split.set_position(page_px);
            }
            break;
        }
        candidate = paned.parent();
    }
    crate::snapshot_face::pump_context(32);
}

fn page_scroller(window: &adw::ApplicationWindow) -> Option<gtk::ScrolledWindow> {
    ["page-scroll-Cut"]
        .iter()
        .find_map(|name| find_widget_by_name(window.upcast_ref(), name))
        .and_then(|widget| widget.downcast::<gtk::ScrolledWindow>().ok())
}

/// True when the band `shot_top`..`shot_top + shot_height` reaches every one of [`SHOT_ROWS`] in `window`:
pub fn shot_covers(window: &adw::ApplicationWindow, shot_top: i32, shot_height: i32) -> bool {
    let bottom = shot_top + shot_height;
    SHOT_ROWS.iter().all(|name| {
        row_place(window, name)
            .is_some_and(|(top, painted)| top >= shot_top && top < bottom && painted)
    })
}

fn row_place(window: &adw::ApplicationWindow, name: &str) -> Option<(i32, bool)> {
    let band = find_widget_by_name(window.upcast_ref(), "track-band")?;
    let top = row_top(window, name)?;
    let painted = top >= 0 && top < window.height() && band.height() > 0;
    Some((top, painted))
}

fn page_tops(window: &adw::ApplicationWindow) -> Vec<String> {
    [
        "preview-panel",
        "cut-toolbar",
        "track-layers",
        "track-strip",
        "selection-row",
        "camera-rows",
    ]
    .iter()
    .filter_map(|name| row_top(window, name).map(|top| format!("{name} y={top}")))
    .collect()
}

/// Both rows' top edges, in the window's own coordinates, in [`SHOT_ROWS`] order.
pub fn row_tops(window: &adw::ApplicationWindow) -> [i32; 2] {
    [
        row_top(window, SHOT_ROWS[0]).unwrap_or(i32::MAX),
        row_top(window, SHOT_ROWS[1]).unwrap_or(i32::MAX),
    ]
}

/// Row 1 above row 2, which is **1** over **2** in `spec/img/05-rows.png`.
pub fn rows_in_order(window: &adw::ApplicationWindow) -> Option<bool> {
    let first = row_top(window, SHOT_ROWS[0])?;
    let second = row_top(window, SHOT_ROWS[1])?;
    Some(first < second)
}

/// Stage the two-camera figure onto `window` and hand back what it staged, newest last, for the caller's
/// `snapshot:` line.
pub fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    if let Some(panel) = ui::find_source_widget(window, "preview-panel") {
        crate::ui::cut_preview_panel::grow_to(&panel, PREVIEW_SHOT_W, PREVIEW_SHOT_H);
        panel.set_size_request(PREVIEW_SHOT_W as i32, PREVIEW_SHOT_H as i32);
        panel.set_vexpand(false);
    }
    // The reading column keeps §A's opening width, so the preview beside it stays a picture and not a 240 px
    // sliver: `cut_page_layout::arrange` only seats the handle, and headless GTK settles a paned at its minima.
    if let Some(top) = find_widget_by_name(window.upcast_ref(), "cut-top")
        .and_then(|widget| widget.downcast::<gtk::Paned>().ok())
    {
        top.set_position(crate::ui::cut_page_layout::SELF_SPLIT_PREVIEW_PX);
    }
    // The tape gets a short box: left alone it takes every pixel of the lower pane.
    if let Some(layers) = find_widget_by_name(window.upcast_ref(), "track-layers") {
        layers.set_size_request(SHOT_TRACKS_W, SHOT_TRACKS_H);
        layers.set_vexpand(false);
    }
    if let Some(band) = find_widget_by_name(window.upcast_ref(), "track-band") {
        band.set_size_request(-1, SHOT_TRACKS_H);
    }
    if let Some(paned) = find_widget_by_name(window.upcast_ref(), "cut-page-split")
        .and_then(|widget| widget.downcast::<gtk::Paned>().ok())
    {
        paned.set_position(SHOT_SPLIT_POS);
        // ...and BOTH panes are capped, or the leftover height goes to the picture and the rows stay below the
        // painted pixels however far the divider is dragged up.
        paned.set_size_request(-1, SHOT_UPPER_CAP_H);
        if let Some(end) = paned.end_child() {
            end.set_size_request(-1, SHOT_LOWER_CAP_H);
        }
    }
    // Declared BEFORE the seed: `set_session_recordings` marks the session as declared, so the seed below
    // cannot trade these spans for what its segments imply (`ui::cut_folds::SESSION_DECLARED`).
    ui::set_session_recordings(&recordings());
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let wav = crate::layout::resolve(
        &root,
        &crate::startup::session_dir(&root),
        &format!("project:sources/{MIC}.wav"),
    );
    if let Some(folder) = wav.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = crate::snapshot::write_silence_wav(&wav, 8000, 8000 * 45);

    ui::seed_review_cut(&figure());
    // The band's red line: the band paints the PREVIEW's playhead (`band_scene_for`), so `set_line_position`
    // alone draws none.
    ui::set_playhead(window, LINE_PLAYHEAD);
    seed_snapshot_frame(dir, CAM2, CAM2_FILE_SECOND);
    seed_snapshot_frame(dir, CAM1, LINE_T);
    ui::cut_picture::set_preview_camera_base(CAM2);
    ui::cut_picture::set_snapshot_dir(dir);
    ui::set_line_position(window, cut_line::LinePos { t: LINE_T });
    let watched = ui::press_watch_row(window, 1).unwrap_or_default();
    ui::refresh_camera_rows(window);
    // The page does NOT scroll: `window_page_box` gives `page-scroll-‹tab›` a `Never` vertical policy (§1: no
    // page scrolls as a whole at §1's minimum), which makes GtkScrolledWindow size itself to its child, so the
    // adjustment reports `upper == page_size` and every ask from [`scroll_page`] lands on 0 (measured).
    crate::snapshot_face::pump_context(32);
    raise_shell_split(window, SHELL_PAGE_PX);
    scroll_page(window, SHOT_SCROLL_DOWN);
    crate::snapshot_face::pump_context(32);
    raise_shell_split(window, SHELL_PAGE_PX);
    scroll_page(window, SHOT_SCROLL_DOWN);
    crate::snapshot_face::pump_context(32);
    ui::refresh_camera_rows(window);

    let plate = |name: &str| {
        find_widget_by_name(window.upcast_ref(), name)
            .and_then(|widget| widget.downcast::<gtk::Button>().ok())
            .map(|button| button.label().unwrap_or_default().to_string())
            .unwrap_or_else(|| format!("<{name} missing>"))
    };
    vec![
        format!("plate 1 {}", plate("watch-row-1")),
        format!("plate 2 {}", plate("watch-row-2")),
        format!("line {:.2}", LINE_T),
        format!(
            "scrolled {:.0}/{:.0} px",
            scroll_value(window),
            scroll_travel(window)
        ),
        page_tops(window).join(" "),
        watched,
    ]
}
