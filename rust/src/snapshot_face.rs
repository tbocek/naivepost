//! The pieces every snapshot shares: read the run bar's ▶ back off its widgets, paint one widget into a
//! PNG, open the shell's log, and put one frame on disk where Prepare would have extracted it. Split out of
//! `snapshot.rs`, which is past the size another block fits in; nothing here decides a rule -- each reads or
//! paints what the app already drew.

use std::path::Path;

use adw::prelude::*;
use glib::translate::ToGlibPtr;
use gtk4 as gtk;

use crate::layout;
use crate::ui;

/// The bar's ▶ as the picture sees it: its icon, its tooltip, and whether ⏹ can be pressed — the three
/// properties `paint_run_bar` is the only writer of, read back off the widgets rather than recomputed
/// from [`crate::run::controls`] so a missed repaint shows up as a wrong face here.
pub(crate) fn face_of(window: &adw::ApplicationWindow) -> (String, String, bool) {
    let play = ui::play_button(window).expect("the run bar has a ▶");
    (
        play.icon_name().unwrap_or_default().to_string(),
        play.tooltip_text().unwrap_or_default().to_string(),
        ui::stop_button(window).expect("the run bar has a ⏹").is_sensitive(),
    )
}

/// Run the main context until nothing is pending, at most `rounds` rounds. Bounded and non-blocking: a snapshot
/// must not park on a context with nothing in it. Shared with [`write_face_png`], which needed the same.
pub(crate) fn pump_context(rounds: usize) {
    let context = glib::MainContext::default();
    for _ in 0..rounds {
        while context.pending() {
            context.iteration(false);
        }
    }
}

/// Draw the run bar's ▶ at a legible size into a PNG on its own. A whole-window shot cannot show which
/// glyph the button wears: at 1334 px wide the icon is ~16 px and reads as a blue blob whether it is ▶ or
/// ⏸, so the face this screen exists to record is invisible in it. The widget has to be laid out first --
/// one painted inside a window that was never mapped produces no render node -- hence its own top level,
/// the way `03-new-confirm` paints a dialog rather than the window behind it.
pub(crate) fn write_face_png(face: &gtk::Button, out: &Path) -> Result<(), String> {
    // A copy, not the bar's own button: reparenting a widget out of a mapped window leaves it with no
    // render node, which is what made an earlier attempt of this print "window produced no render node".
    // The copy carries the same icon and tooltip because those ARE the face being recorded.
    let glyph = gtk::Button::from_icon_name(face.icon_name().unwrap_or_default().as_str());
    if let Some(tip) = face.tooltip_text() {
        glyph.set_tooltip_text(Some(tip.as_str()));
    }
    for class in face.css_classes() {
        glyph.add_css_class(class.as_str());
    }
    let holder = gtk::Window::new();
    holder.set_default_size(96, 72);
    holder.set_child(Some(&glyph));
    holder.present();
    // The map must happen before the frame is taken; `write_png` reads the widget's allocated size.
    pump_context(64);
    let result = write_png(glyph.upcast_ref(), out);
    holder.close();
    result
}

/// Open the shell's log and paint what has been logged, through the same door a run repaints it from.
///
/// `paint_progress` is the only thing that expands the expander AND fills its body — it reads openness
/// off the run bar, so a snapshot with no run open cannot get an open log any other way. This hands it
/// the window's own widgets and the idle queue, asking for the one state a real run shows: expanded.
pub(crate) fn show_the_log(window: &adw::ApplicationWindow) -> Result<(), String> {
    let progress = ui::progress_bar(window).ok_or("the shell has no progress bar")?;
    let expander = ui::log_expander(window).ok_or("the shell has no log expander")?;
    let view = ui::log_view(window).ok_or("the shell's log has no text view")?;
    let queue = ui::run_queue(window).ok_or("the shell has no run queue")?;
    let status = ui::find_status(window.upcast_ref()).ok_or("the shell has no status line")?;
    ui::window::paint_progress(
        &progress,
        &expander,
        &view,
        &queue.borrow(),
        crate::ui::shell_beats::StatusLine { status: &status, owned: false },
        true,
    );
    Ok(())
}

/// Write one extracted frame into the folder Prepare would have put it in, so a Cut-page shot has a real
/// picture to show. PNG bytes under the `.jpg` name `cut_tracks::frame_paths` lists (Prepare's own stamping
/// rule, `frames.rs` S4): gdk-pixbuf decodes by content, and no JPEG encoder is linked here.
pub(crate) fn seed_snapshot_frame(dir: &std::path::Path, base: &str, second: f64) {
    let Ok(tree) = layout::Tree::new(dir) else { return };
    let dir = tree.frames_dir(base);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{}.jpg", crate::clock::frame_name(second)));
    // A frame already on disk (one Prepare really extracted) is the better picture; write only when absent.
    if path.exists() {
        return;
    }
    let Ok(surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, 160, 90) else {
        return;
    };
    {
        let Ok(cr) = cairo::Context::new(&surface) else { return };
        cr.set_source_rgb(0.08, 0.35, 0.6);
        let _ = cr.paint();
    }
    let Ok(mut file) = std::fs::File::create(&path) else { return };
    let _ = surface.write_to_png(&mut file);
}

/// The staged frame's own size: the shape of §06's preview image (960 x 540), so `fit_size` is a no-op and the
/// still's box can be told from the whole frame. Not a tuning value -- it is the size the shot is compared at.
//
// PUBLIC because this is the INPUT the `06-preview` shot paints over, and §00-principles §5 says to measure the
// artifact: a check that wants to prove "the still's pixels differ from the dimmed surround" has to know what the
// picture underneath was, and deriving it from these same consts is the one source of truth for that.
pub const STRUCTURED_FRAME_W: i32 = 960;
pub const STRUCTURED_FRAME_H: i32 = 540;
/// The three full-width bands and the one off-centre block. These are staged INPUT pixels for a headless shot:
/// nothing decodes video in this container, so the picture behind the preview has to be written here. A FLAT
/// colour ([`seed_snapshot_frame`]) cannot show a camera crop -- every fraction of it is the same colour as all
/// the rest -- so these numbers exist to make a crop distinguishable: three bands of clearly different lightness
/// plus one bright block that only exists in the upper-left half of the frame.
pub const BAND_SKY: (f64, f64, f64) = (0.78, 0.86, 0.94);
pub const BAND_MID: (f64, f64, f64) = (0.42, 0.5, 0.44);
pub const BAND_LOWER: (f64, f64, f64) = (0.12, 0.14, 0.18);
pub const BLOCK_BRIGHT: (f64, f64, f64) = (0.98, 0.82, 0.2);

/// Write a frame with STRUCTURE where Prepare would have extracted one, and hand back the path.
///
/// Same contract as [`seed_snapshot_frame`] -- into `tree.frames_dir(base)`, a real extracted frame wins if one is
/// already there, `None` when nothing could be written -- but at [`STRUCTURED_FRAME_W`] x [`STRUCTURED_FRAME_H`]
/// and banded, because the `06-preview` screen has to show a stop's STILL, which `cut_preview_frame::stop_px`
/// frames as the camera's FRACTION of the footage. Over a flat frame that fraction is indistinguishable from the
/// whole picture, which is how the shot came out claiming a still while showing none.
pub fn seed_structured_frame(dir: &std::path::Path, base: &str, second: f64) -> Option<std::path::PathBuf> {
    let tree = layout::Tree::new(dir).ok()?;
    let folder = tree.frames_dir(base);
    std::fs::create_dir_all(&folder).ok()?;
    let path = folder.join(format!("{}.jpg", crate::clock::frame_name(second)));
    if path.exists() {
        return Some(path);
    }
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, STRUCTURED_FRAME_W, STRUCTURED_FRAME_H).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        let (w, h) = (STRUCTURED_FRAME_W as f64, STRUCTURED_FRAME_H as f64);
        for (band, (r, g, b)) in [BAND_SKY, BAND_MID, BAND_LOWER].iter().enumerate() {
            cr.set_source_rgb(*r, *g, *b);
            cr.rectangle(0.0, band as f64 * h / 3.0, w, h / 3.0 + 1.0);
            let _ = cr.fill();
        }
        // One block that exists in no other part of the frame: inside the sky band, left of centre.
        cr.set_source_rgb(BLOCK_BRIGHT.0, BLOCK_BRIGHT.1, BLOCK_BRIGHT.2);
        cr.rectangle(w * 0.1, h * 0.08, w * 0.22, h * 0.2);
        let _ = cr.fill();
    }
    let mut file = std::fs::File::create(&path).ok()?;
    surface.write_to_png(&mut file).ok()?;
    Some(path)
}

/// Paint `widget` into a PNG. Called once the first frame is ready.
///
/// A cairo surface, not `gsk::Renderer::render_texture`: that one needs a realized
/// renderer, i.e. a native surface, and the headless run has none to offer. Any widget will do —
/// the confirm dialog's screen paints the dialog, not the window behind it.
pub(crate) fn write_png(widget: &gtk::Widget, out: &Path) -> Result<(), String> {
    let width = u32::try_from(widget.width()).unwrap_or(1).max(1);
    let height = u32::try_from(widget.height()).unwrap_or(1).max(1);

    let paintable = gtk::WidgetPaintable::new(Some(widget.upcast_ref::<gtk::Widget>()));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    let node = snapshot.to_node().ok_or("window produced no render node")?;

    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, height as i32)
        .map_err(|err| err.to_string())?;
    {
        let cr = cairo::Context::new(&surface).map_err(|err| err.to_string())?;
        // gsk_render_node_draw is the one call that paints a render node onto an
        // arbitrary cairo context; no realized renderer needed.
        unsafe { gsk::ffi::gsk_render_node_draw(node.as_ref().to_glib_none().0, cr.to_raw_none()) };
    }
    let mut file = std::fs::File::create(out).map_err(|err| err.to_string())?;
    surface.write_to_png(&mut file).map_err(|err| err.to_string())
}
