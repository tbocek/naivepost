//! §05-cut#1-screen badge **1** — the preview's picture. `spec/inventory/cut.md` §A asks for "preview
//! (GtkPicture contain-fit, min height 160, click toggles play)". The panel already drew the camera rect,
//! the dim and the overlays, but nothing behind them: no frame of the session was ever shown, and a click on
//! the preview did nothing. This module owns the frame and the press; every number about WHICH second and
//! WHICH recording comes from the doors the page already keeps (`line_position`, `watched_row`,
//! `page_recordings`) and from Prepare's own extracted frames through `cut_tracks::frame_paths`/`frame_near`.
//!
//! Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout;
use crate::timeline;
use crate::ui::{self, cut_tracks};

/// §A's own number: the picture never shrinks below this, however narrow the column gets.
pub const PICTURE_MIN_HEIGHT: i32 = 160;

/// §A's sentence for the click, spelled once here because it is this widget's tooltip.
pub const CLICK_TIP: &str = "click toggles play";

/// The clickable host around the picture.
///
/// DECISION: §A names a `GtkPicture` and says the preview is clicked, and is silent on which widget takes
/// the press. A `gtk::Picture` ignores button events, so the press goes to a flat `gtk::Button` that wraps
/// it — the picture stays the thing that shows the frame, the button is only how a click arrives, and a test
/// can fire it the toolkit's way (`emit_by_name("clicked")`) instead of synthesising coordinates.
pub fn build() -> gtk::Button {
    let picture = gtk::Picture::new();
    picture.set_widget_name("preview-picture");
    // Contain-fit (§A), so a 16:9 frame letterboxes inside the panel instead of squashing.
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(-1, PICTURE_MIN_HEIGHT);
    picture.set_can_shrink(true);

    let button = gtk::Button::new();
    button.set_widget_name("preview-picture-button");
    button.set_tooltip_text(Some(CLICK_TIP));
    button.set_child(Some(&picture));
    button
}

/// The picture widget of THIS window, newest window winning like every other per-window reader here.
fn picture(window: &adw::ApplicationWindow) -> Option<gtk::Picture> {
    find(window.content()?.upcast_ref(), "preview-picture")?.downcast().ok()
}

fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find(&widget, name) {
            return Some(found);
        }
    }
    None
}

/// Put one extracted frame on the panel. Returns whether anything is showing: a path that does not decode
/// (or none at all) clears the picture rather than leaving a stale frame up, so the plate reads as empty
/// tape instead of a broken image. The load is gdk-pixbuf's real decoder over a real file — the same
/// library GTK itself uses for a `Picture`, and already in the lock through the `gtk` crate.
pub fn show_frame(window: &adw::ApplicationWindow, path: Option<&std::path::Path>) -> bool {
    let Some(picture) = picture(window) else {
        return false;
    };
    let shown = match path {
        Some(file) => gtk::gdk_pixbuf::Pixbuf::from_file(file)
            .map(|pixbuf| gtk::gdk::Texture::for_pixbuf(&pixbuf))
            .ok(),
        None => None,
    };
    match shown {
        Some(texture) => {
            picture.set_paintable(Some(&texture));
            true
        }
        None => {
            picture.set_paintable(None::<&gtk::gdk::Paintable>);
            false
        }
    }
}

thread_local! {
    /// The recording a snapshot named for the preview's frame search. See [`preview_recording_base`].
    static PREVIEW_BASE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    /// The folder a snapshot named for that search. See [`snapshot_tree`].
    static SNAPSHOT_DIR: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// The recording whose frames the preview may show, when someone has named one (the snapshot bench renders a
/// fixture folder that carries no extracted frames of its own). `None` means answer from the session's
/// recordings, which is what a live window does.
pub fn preview_recording_base() -> Option<String> {
    PREVIEW_BASE.with(|slot| slot.borrow().clone())
}

/// Name the recording for the preview's frame search. Only the snapshot bench calls this.
pub fn set_preview_camera_base(base: &str) {
    PREVIEW_BASE.with(|slot| *slot.borrow_mut() = Some(base.to_string()));
}

/// Which frame belongs on the panel right now: the recording of the row the preview is watching (falling
/// back to the first recording of the session), at the second the red line stands on. Both halves are asked
/// of the doors that hold them, so moving the line or clicking another row changes the picture without this
/// module remembering anything.
pub fn refresh(window: &adw::ApplicationWindow) -> bool {
    let recordings = ui::page_recordings();
    if recordings.is_empty() {
        return show_frame(window, None);
    }
    // The watched row indexes the scene's rows; `scene_from` lays them in the recording order, so the row
    // number picks the recording rather than needing a second mapping.
    let row = ui::watched_row().unwrap_or(0);
    // A snapshot names the session it is rendering (`snapshot.rs`), because its folder is the fixture and
    // not the working copy a real session writes frames into. Empty in every other case, so a live window
    // still answers from its own recordings.
    let named = preview_recording_base();
    let base = match named {
        Some(base) => Some(base),
        None => recordings
            .get(row)
            .or(recordings.first())
            .map(|recording| recording.base.clone()),
    };
    let Some(base) = base else {
        return show_frame(window, None);
    };
    let Some(tree) = session_tree(window) else {
        return show_frame(window, None);
    };
    let paths = cut_tracks::frame_paths(&tree, &base);
    let at = ui::line_position(window).t;
    let found = cut_tracks::frame_near(&paths, at);
    show_frame(window, found.as_deref())
}

/// The folder this window is working on. `line_root` is the page's own per-window slot for its folder (the
/// red line's file lives beside it), which is the same folder the band lists its frames from — so there is
/// exactly one answer to "which project" in a window, and no second copy of that decision here.
/// A snapshot names its own folder: the bench renders a fixture, while `line_root` follows the working copy
/// the app opens, and those are different directories by design.
fn session_tree(window: &adw::ApplicationWindow) -> Option<layout::Tree> {
    if let Some(dir) = snapshot_tree() {
        return layout::Tree::new(&dir).ok();
    }
    let dir = ui::line_root(window)?;
    layout::Tree::new(&dir).ok()
}

/// Set by the snapshot bench only: the folder whose frames the preview may show. See
/// [`set_preview_camera_base`], which names the recording inside it.
fn snapshot_tree() -> Option<std::path::PathBuf> {
    SNAPSHOT_DIR.with(|slot| slot.borrow().clone())
}

/// Name the folder the preview searches for frames. Only the snapshot bench calls this.
pub fn set_snapshot_dir(dir: &std::path::Path) {
    SNAPSHOT_DIR.with(|slot| *slot.borrow_mut() = Some(dir.to_path_buf()));
}
/// Move THIS window's red line and repaint the frame that goes with it. `window.rs` owns where the line is
/// stored; this is the one line its owner has to add, so a placed line always changes what the preview shows
/// without the widget layer deciding anything about seconds.
pub fn note_line_moved(window: &adw::ApplicationWindow, at: f64) {
    ui::set_line_position(window, crate::cut_line::LinePos { t: at });
    refresh(window);
}

/// The click (§A: "click toggles play"). Forwards to the transport's OWN door with the session's filmed
/// stretches, which is what makes a preview with nothing filmed refuse for the reason F2.1 gives rather
/// than starting a blank player. Then the frame follows the line the press left the preview on.
pub fn wire_click(window: &adw::ApplicationWindow) {
    let Some(root) = window.content() else { return };
    let Some(button) = find(root.upcast_ref(), "preview-picture-button")
        .and_then(|widget| widget.downcast::<gtk::Button>().ok())
    else {
        return;
    };
    let win = window.clone();
    button.connect_clicked(move |_| {
        let runs = timeline::filmed_runs(&ui::page_recordings())
            .iter()
            .map(|(start, end)| (*start, *end))
            .collect::<Vec<_>>();
        let _said = ui::press_play_recording(&win, &runs);
        refresh(&win);
    });
}
