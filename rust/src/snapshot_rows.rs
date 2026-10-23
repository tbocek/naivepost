//! F2.10's own picture — `spec/img/05-rows.png`: TWO recordings running at the same time, one row each.
//!
//! No fixture on disk carries this figure: the demo project's `cut/cut.json` is ONE camera in three clips,
//! so a shot of it can only ever draw one row. The spec's caption says as much about its own screenshot ("the
//! lecture has no simultaneous recordings, so the second file was shifted 19 s back in a copy of the project
//! and the cut cleared"), which is exactly what [`stage`] lays out here instead — in a copy of the page's
//! state, not on disk.
//!
//! Nothing is poked. Every state change goes through the doors the page itself uses (`ui::set_session_recordings`,
//! `ui::seed_review_cut`, `ui::set_line_position`, `ui::press_watch_row`), and the strings returned are read
//! BACK OFF THE WIDGETS the page built, so what the recipe prints and what the PNG shows is what the app's own
//! rules say — a plate that reads `row 2` in this shot is a real bug in the row list, not a staging shortcut
//! papered over by labelling a button.
//!
//! The spec's numbered badges and where each comes from:
//! * **1** camera 1 (17-25-06, 0:00–0:37) — the first seeded recording, plate read off `watch-row-1`.
//! * **2** camera 2 (17-25-45) with its shift correction `−19.00 s` in the name plate — the second seeded
//!   recording, whose base carries the stamp and whose `shift` entry carries the correction; the plate is
//!   `cut_cam::name_plate`'s own string, read off `watch-row-2`.
//! * **3** the overlap 0:20–0:37 — falls out of the arithmetic below, coloured onto its own row by
//!   `timeline::rows_for`, which is the greedy colouring §B names.
//! * **4** the dashed outline on the watched row — `press_watch_row` adds the `watched-row` class.
//! * **5** the preview shows the watched row, not the cut — `cut_picture::set_preview_camera_base` names the
//!   WATCHED camera, and a frame of it is planted in the folder Prepare would have extracted into.
//! * **6** the one kept scene, taken from camera 1 — one `Seg` with `cam: 0`.
//! * **7** status "watching camera 2 — the cut shows camera 1 here; ▶ plays the cut" — the sentence
//!   `press_watch_row` returns and writes to the status row.

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
/// Camera 2's file stamp — badge **2**'s "camera 2 (17-25-45)". Stamped 39 s after camera 1.
pub const CAM2: &str = "2026-09-16_17-25-45";
/// How far camera 2 was moved back, which is the spec's own staging ("shifted 19 s back"). 39 − 19 = 20, so
/// camera 2 sits at 0:20 on the session clock and badge **3**'s overlap 0:20–0:37 is not a number staged here
/// but the consequence of the two stamps plus the one correction.
pub const CAM2_SHIFT: f64 = -19.0;
/// The FILE STAMP difference between the two cameras, which is the spec's own stamps read as a difference:
/// 17-25-45 sits 39 s after 17-25-06. Camera 2's place on the session clock is that minus the shift.
pub const CAM2_STAMP: f64 = 39.0;
/// The line's second of camera 2's OWN file, which is `timeline::local_at`'s answer for the line below:
/// `25 − 20 + (−19) = −14`. Planted there rather than at 0 because the panel asks by that number and
/// `frame_near` takes the nearest stamp AT OR BEFORE it -- a frame planted at 0 s would be "in the future" as
/// far as the search is concerned, and the plate would stay empty in a headless run where nothing else can
/// supply the session's real extracted frames.
pub const CAM2_FILE_SECOND: f64 = -14.0;
/// The microphone the shot needs a speaker badge and a gutter switch for (badges on S2's row). A separate
/// recording, so its name must be neither camera's: `cut_cameras::row_spans` keeps a span whose base the cut
/// already accounts for, and a lane named after a camera would swallow that camera's row.
pub const MIC: &str = "mic-lav";

/// Width the shot's preview keeps (the panel's own opening width; the tape is beside it, not under it).
pub const PREVIEW_SHOT_W: f64 = 240.0;
/// Height the shot's preview is pinned to. `grow_to` floors a request at `PREVIEW_FLOOR_H` (160) and
/// `PREVIEW_PANEL_H` (135), so this is the smallest box the panel will accept.
pub const PREVIEW_SHOT_H: f64 = 160.0;
/// The tape's box: short enough that the rows below it -- what this screen exists to show -- come near the top.
pub const SHOT_TRACKS_W: i32 = 1200;
pub const SHOT_TRACKS_H: i32 = 170;
/// Where the shot parks the page's vertical divider. §A opens it 380 px down, which suits the ~900 px-tall
/// window §1 describes; here it has to sit just above the tape for the rows to be reachable. Measured: 380 put
/// the two camera plates at y=775/813 of an 890 px frame (inside it, but far below where `spec/img/05-rows.png`
/// draws them); 160 -- the preview's own floor, so the pane is as short as its widgets allow -- lifts them to
/// y=555/593, which is where the reference shows both rows.
pub const SHOT_SPLIT_POS: i32 = 160;
/// How tall the shot asks the window to be. A headless GTK clamps a window to ~790 px of plate however
/// large the ask (measured: 800, 1500 and 2400 all came back the same), so the page cannot be made taller;
/// what brings F2.10's two camera plates into the painted frame is everything ABOVE them getting shorter,
/// which is what [`SHOT_SPLIT_POS`] and [`SHOT_UPPER_CAP_H`] do.
pub const SHOT_WINDOW_H: i32 = 790;

/// The ceiling the shot puts on the pane above the top/bottom divider. `arrange` opens that divider at §A's
/// 380 px AND hands the leftover height to the upper pane, so in an 800 px window the picture pane swallows
/// ~420 px and the camera rows land far below the painted pixels.
///
/// It has to clear the preview's own floor: `grow_to` refuses a shorter box than
/// [`crate::ui::cut_page_layout::PREVIEW_FLOOR_H`] (160) and the effects lane under the picture costs
/// `fx_lane::lane_height_px(1)`, so a cap below ~190 px is simply ignored by the widgets inside it.
pub const SHOT_UPPER_CAP_H: i32 = 200;
/// The ceiling on the pane BELOW the top/bottom divider — the tape, the strip, the recorders' band and the two
/// camera rows, which is what this picture is about. `GtkPaned::set_size_request` only sizes the START pane, so
/// without a request of its own the end child takes every pixel the page has left and runs off the bottom of an
/// 890 px headless window, leaving the rows painted nowhere (measured: y=-1188/-1150 with the upper pane capped).
/// Tall enough for §A's ruler (18) + one wave strip (30) + [`SHOT_STRIP_H`] + two camera rows at
/// `fx_lane`-ish height, so nothing inside it is squeezed to zero.
pub const SHOT_LOWER_CAP_H: i32 = 470;
/// How far down the shot scrolls the page, past the preview pane and the tape, to bring both camera rows into
/// the painted frame. Generous on purpose: `scroll_page` clamps it to the scroller's real travel.
/// UNUSED until the page really scrolls — see [`shot_covers`]' note on `upper == page_size`; raising this while
/// the scroller reports no travel changes nothing, so the levers above the rows are what move them.
pub const SHOT_SCROLL_DOWN: f64 = 900.0;
/// Where the shot parks the SHELL's page/bottom-bar divider (§1's other draggable divider). Its opening is
/// far below what this shot needs, and while the bottom bar keeps that share the page cannot scroll; see
/// [`raise_shell_split`].
pub const SHELL_PAGE_PX: i32 = 560;
/// The placeholder strip's height in the shot. It was built taller than §A's ruler (18 px) plus one wave
/// strip (30 px) because on the live page it stands in for a whole band; here the rows beneath it have to be
/// reachable inside ~790 px of window, so the shot asks for what §A actually lists.
pub const SHOT_STRIP_H: i32 = 52;
/// The height the shot asks the window for. A headless GTK settles BELOW this ask (measured: an ask of 1200
/// came back 1014 px, one of 800 came back 730), so the frame is smaller than asked either way; asking big is
/// what gets the tallest honest frame, and the rest is done by [`raise_shell_split`] and [`scroll_page`].
pub const SHOT_WINDOW_ASK_H: i32 = 1200;
/// The line's second: inside the overlap, so the watch sentence is owed (badge **7**) and the preview has a
/// kept scene to convert a second for.
pub const LINE_T: f64 = 25.0;
/// The same second on the PREVIEW's playhead, which is what the band's red line actually paints
/// (`band_scene_for` reads it off the player, not off the line state). Placed through `ui::set_playhead`, the
/// door a click on the tape fires, at the point the layout has settled.
pub const LINE_PLAYHEAD: f64 = 25.0;

/// The two recordings of the figure: camera 1 for its whole 37 s, camera 2 from its shifted start for the
/// same length of tape. Both bases are FILE STAMPS rather than `cam0`/`cam1` on purpose: a row's plate names
/// the recording that row speaks for (prototype `gui/cut.go`'s `rowNameVid` → `v.base`), and the cut's own
/// segments can only ever say `cam<N>`, so a seeded span survives the row list's filter only while the cut
/// cannot account for its base — see `cut_cameras::row_spans`. Naming them `cam0`/`cam1` here would put the
/// second row back to the unnamed `row 2` fallback badge **2** exists to replace.
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
/// camera 2's shift correction. The correction is stored under camera 2's stamp because that is what a hand
/// slide writes (§B: "shift map base→seconds") and it is the key `cut_cam::name_plate` looks up.
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

/// A row's top edge in the window's own coordinates, or `None` when no widget carries that name.
///
/// TEMPORARILY instrumented for this round's layout work: it also prints the row's own height and the heights
/// reported up its ancestor chain, because a row whose top sits above the painted frame can only be explained
/// by something ABOVE it in the page being taller than the window (or by an unmapped native answering 0).
/// The numbers go to stderr so the recipe's own stdout lines stay as they were.
pub fn row_top(window: &adw::ApplicationWindow, name: &str) -> Option<i32> {
    let widget = find_widget_by_name(window.upcast_ref(), name)?;
    let native = widget.native()?;
    // The window's own coordinates: `translate_coordinates` into the native is the walk GTK documents for
    // "where in the painted surface is this widget", and the frame `write_png` captures IS that surface.
    #[allow(deprecated)] // the shot's own walk; gtk 4.12's replacement needs a surface node
    let y = widget
        .translate_coordinates(&native, 0.0, 0.0)
        .map(|(_, y)| y as i32);
    y
}

/// The page's own scroll, driven through its `gtk::Adjustment`: the shell wraps every page in
/// `page-scroll-‹tab›`, so moving that adjustment is what a wheel or a trackpad swipe does to it.
///
/// This is how the shot reaches F2.10's two camera plates. Headless GTK clamps the window to ~790 px of plate
/// however large the ask (measured: 800, 1500 and 2400 all came back the same) while the Cut page is ~1015 px
/// tall, so the rows sit below the painted pixels at rest; scrolling down is exactly what a person does to see
/// them, and [`shot_covers`] then measures whether they really landed in the frame.
///
/// `dy` is asked of `gtk::Adjustment`, which CLAMPS into its own `[lower, upper - page_size]` band -- a band
/// that only exists once the scroller has been laid out with its final height, so this pumps the context after
/// writing rather than trusting one call before the map has happened.
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
    // Printed with the band it is clamped to, so a scroll that lands on 0 says whether there was travel.
    println!("scroll value {value:.0} of upper {upper:.0} page {page:.0}");
    value
}

/// The page's travel: how much further it CAN scroll, i.e. `upper - page_size` of its own adjustment. Printed
/// beside [`scroll_value`] so a shot that will not scroll says whether there was anything to scroll.
pub fn scroll_travel(window: &adw::ApplicationWindow) -> f64 {
    page_scroller(window).map_or(0.0, |scroller| {
        let adjustment = scroller.vadjustment();
        (adjustment.upper() - adjustment.page_size()).max(0.0)
    })
}

/// Pull the shell's page/bottom-bar divider up to free height for the page. §1 makes them "two halves of a
/// draggable divider", so this is a drag a user really makes — and headless nothing else makes it.
pub fn raise_shell_split(window: &adw::ApplicationWindow, page_px: i32) {
    // Found by the run bar it sits above rather than by a name it does not carry (same route as the shrink test).
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
    // The divider is the Paned that holds the bottom box DIRECTLY -- walk up until a parent's own children
    // include it, which is how the shell's divider and the page's own differ only by depth.
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

/// The shell's scroller around the visible page, found by the name `window_page_box` gives it for this tab.
fn page_scroller(window: &adw::ApplicationWindow) -> Option<gtk::ScrolledWindow> {
    // Named per page (`page-scroll-Cut`), so the walk takes whichever one the Cut page carries.
    ["page-scroll-Cut"]
        .iter()
        .find_map(|name| find_widget_by_name(window.upcast_ref(), name))
        .and_then(|widget| widget.downcast::<gtk::ScrolledWindow>().ok())
}

/// True when the band `shot_top`..`shot_top + shot_height` reaches every one of [`SHOT_ROWS`] in `window`:
/// each row's own top lies inside it AND inside the strip of page the shell's scroller is currently showing.
/// The second clause is what F2.10's picture needs and a bare top-in-band test could not say: headless GTK
/// settles the Cut page taller than the window, so rows at y=1015/1053 had their tops inside
/// `shot_top..shot_top + height` while the PNG showed a page with no camera row in it at all -- they were
/// scrolled out of `page-scroll-Cut`, where `write_png` cannot reach them. A row outside the scroller's
/// viewport is not in the shot, and neither is an absent row. Called by `snapshot::run`'s `05-rows` arm and by
/// `tests/suite/f2_10_rows_shot.rs`, which refuse the shot when this does not hold.
pub fn shot_covers(window: &adw::ApplicationWindow, shot_top: i32, shot_height: i32) -> bool {
    let bottom = shot_top + shot_height;
    SHOT_ROWS.iter().all(|name| {
        row_place(window, name)
            .is_some_and(|(top, painted)| top >= shot_top && top < bottom && painted)
    })
}

/// Row `name`'s own top and whether that top is inside the PIXELS this window actually paints. `None` for an
/// absent row: an absent row is not a measurable one.
///
/// The frame `write_png` captures is the window's own render node at the window's own size, so "painted" can
/// only mean: the row's top, translated into the native (which is what [`row_top`] returns), lies between the
/// top of the window and its bottom, AND the widget has a height to draw. Asking the shell's scroller instead
/// — which this did — is a claim about a DIFFERENT box: measured with the rows at y=-1188/-1150 in an 890 px
/// frame, the scroller-relative test still answered "painted" while the PNG showed no camera row at all, and
/// the shot passed while being wrong. A row scrolled above the top of the window is out of the picture exactly
/// as one below its bottom is.
fn row_place(window: &adw::ApplicationWindow, name: &str) -> Option<(i32, bool)> {
    let widget = find_widget_by_name(window.upcast_ref(), name)?;
    let top = row_top(window, name)?;
    let painted = top >= 0 && top < window.height() && widget.height() > 0;
    Some((top, painted))
}

/// Every named widget on the page with its top edge, newest last -- printed by `stage` so a failing shot says
/// WHICH part of the page is holding the camera rows down instead of leaving it to be guessed at.
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

/// Both rows' top edges, in the window's own coordinates, in [`SHOT_ROWS`] order. `i32::MAX` for a row no widget
/// carries that name -- an absent row is not a measurable one. The band the `05-rows` shot measures itself
/// against starts here; see [`shot_covers`].
pub fn row_tops(window: &adw::ApplicationWindow) -> [i32; 2] {
    [
        row_top(window, SHOT_ROWS[0]).unwrap_or(i32::MAX),
        row_top(window, SHOT_ROWS[1]).unwrap_or(i32::MAX),
    ]
}

/// Row 1 above row 2, which is **1** over **2** in `spec/img/05-rows.png`. `None` when either row is absent.
/// Checked by `snapshot::run`'s `05-rows` arm alongside [`shot_covers`].
pub fn rows_in_order(window: &adw::ApplicationWindow) -> Option<bool> {
    let first = row_top(window, SHOT_ROWS[0])?;
    let second = row_top(window, SHOT_ROWS[1])?;
    Some(first < second)
}

/// Stage the two-camera figure onto `window` and hand back what it staged, newest last, for the caller's
/// `snapshot:` line. Called from `snapshot::run` for `05-rows`.
pub fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    // The rows are the LAST thing on a Cut page ~1015 px tall, while a headless window clamps to ~790 px of plate
    // however large the ask (measured: asks of 800, 1500 and 2400 all came back the same), so no size request
    // lifts them into the PNG. What shortens the page is what sits ABOVE the rows -- the preview pane, the tape,
    // and where the page's divider sits -- and the gate on the result is [`shot_covers`], which measures the
    // rows' positions rather than the file's height.
    if let Some(panel) = ui::find_source_widget(window, "preview-panel") {
        crate::ui::cut_preview_panel::grow_to(&panel, PREVIEW_SHOT_W, PREVIEW_SHOT_H);
        // The panel vexpands inside the paned, so its request alone does not stop the upper pane growing;
        // the ceiling goes on the panel too and is what the cap below then holds.
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
    // The tape gets a short box: left alone it takes every pixel of the lower pane. The request goes on BOTH
    // the overlay and the band inside it, because `cut_tracks::tallies_the_band` re-raises the band's own
    // height request on every repaint (it must grow for extra lanes), and a child that asks for 170 px keeps
    // its parent asking for 170 px however small the parent's own request is.
    if let Some(layers) = find_widget_by_name(window.upcast_ref(), "track-layers") {
        layers.set_size_request(SHOT_TRACKS_W, SHOT_TRACKS_H);
        layers.set_vexpand(false);
    }
    if let Some(band) = find_widget_by_name(window.upcast_ref(), "track-band") {
        band.set_size_request(-1, SHOT_TRACKS_H);
    }
    // The placeholder strip is no longer part of the shot's stack (§1's tracks are ONE stack now), so it
    // costs nothing above the camera rows and needs no height ask of its own.
    // And the divider moves up. `arrange` opens it at §A's 380 px AND hands the leftover to the upper pane
    // (`resize_start_child`), so shrinking the widgets inside the panes is not enough -- GTK keeps 380 px above
    // the line whatever lives there. This is the drag a user would make.
    if let Some(paned) = find_widget_by_name(window.upcast_ref(), "cut-page-split")
        .and_then(|widget| widget.downcast::<gtk::Paned>().ok())
    {
        paned.set_position(SHOT_SPLIT_POS);
        // ...and BOTH panes are capped, or the leftover height goes to the picture and the rows stay below the
        // painted pixels however far the divider is dragged up. A `GtkPaned` SIZES ITS START PANE when a size
        // request goes on the paned itself (that is how `arrange` opens the divider at §A's 380 px), so one
        // request does both jobs -- but it sizes ONLY that one: measured, capping the paned alone left the
        // camera rows at y=-1188/-1150 while the preview pane kept ~420 px of leftover, because the end child
        // (the tape box below the divider) has no ceiling of its own and simply grew past the window. The end
        // child therefore gets its own request here, which is what pulls badges **1** and **2** up into the
        // frame. See [`SHOT_UPPER_CAP_H`] and [`SHOT_LOWER_CAP_H`].
        paned.set_size_request(-1, SHOT_UPPER_CAP_H);
        if let Some(end) = paned.end_child() {
            end.set_size_request(-1, SHOT_LOWER_CAP_H);
        }
    }
    // Declared BEFORE the seed: `set_session_recordings` marks the session as declared, so the seed below
    // cannot trade these spans for what its segments imply (`ui::cut_folds::SESSION_DECLARED`).
    ui::set_session_recordings(&recordings());
    // The microphone's file has to exist for its plate in the recorders' band to carry a channel layout —
    // `cut_lanes::channels_of` asks ffprobe and answers `None` for an absent file, which leaves the plate a
    // bare name. Same staging the `05-lanes` shot uses: a silent mono wav, written where resolution points.
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

    ui::seed_review_cut(window, &figure());
    // The band's red line: the band paints the PREVIEW's playhead (`band_scene_for`), so `set_line_position`
    // alone draws none. Placed through `ui::set_playhead`, the door a click on the tape fires, with the same
    // constant the rest of the figure uses so the picture and the state cannot drift apart.
    ui::set_playhead(window, LINE_PLAYHEAD);
    // Badge **5**: the preview shows the WATCHED row, so the frame planted is camera 2's and the base named
    // to the panel is camera 2's. One frame is enough — `cut_tracks::frame_near` takes the nearest stamp.
    seed_snapshot_frame(dir, CAM2, CAM2_FILE_SECOND);
    // And a frame of camera 1 at the line's second, so the other half of badge 5 -- a click handing the
    // preview back to the cut's own camera -- has a picture to show too.
    seed_snapshot_frame(dir, CAM1, LINE_T);
    ui::cut_picture::set_preview_camera_base(CAM2);
    ui::cut_picture::set_snapshot_dir(dir);
    // The red line inside the overlap, then the watch on row 2 — through the door a click on the plate fires,
    // so the outline, the status sentence and the frame all move the way a click moves them.
    ui::set_line_position(window, cut_line::LinePos { t: LINE_T });
    let watched = ui::press_watch_row(window, 1).unwrap_or_default();
    ui::refresh_camera_rows(window);
    // The page does NOT scroll: `window_page_box` gives `page-scroll-‹tab›` a `Never` vertical policy (§1: no
    // page scrolls as a whole at §1's minimum), which makes GtkScrolledWindow size itself to its child, so the
    // adjustment reports `upper == page_size` and every ask from [`scroll_page`] lands on 0 (measured). What
    // brings badges **1** and **2** into the painted ~790 px is therefore everything ABOVE them being short --
    // the pane caps and widget requests applied above, plus this drag of the shell's divider ([`SHELL_PAGE_PX`])
    // that hands the page the share the bottom bar was keeping. The two calls below are that drag and the
    // scroll ask a person would also make; they are harmless where there is no travel.
    crate::snapshot_face::pump_context(32);
    raise_shell_split(window, SHELL_PAGE_PX);
    scroll_page(window, SHOT_SCROLL_DOWN);
    crate::snapshot_face::pump_context(32);
    raise_shell_split(window, SHELL_PAGE_PX);
    scroll_page(window, SHOT_SCROLL_DOWN);
    crate::snapshot_face::pump_context(32);
    ui::refresh_camera_rows(window);

    // Read the plates back off the page rather than recomputing them: the printed string is then the label the
    // PNG shows, and a wrong plate shows up in stdout next to the picture instead of as an unexplained pixel.
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
