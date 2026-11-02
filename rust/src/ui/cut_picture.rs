//! §05-cut#1-screen badge **1** — the preview's picture. `spec/inventory/cut.md` §A asks for
//! "preview (GtkPicture contain-fit, min height 160, click toggles play)".

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout;
use crate::timeline;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::ui::{self, cut_tracks};

/// §A's own number: the picture never shrinks below this, however narrow the column gets.
pub const PICTURE_MIN_HEIGHT: i32 = 160;

/// §A's sentence for the click, spelled once here because it is this widget's tooltip.
pub const CLICK_TIP: &str = "click toggles play";

pub fn build() -> gtk::Button {
    let picture = gtk::Picture::new();
    picture.set_widget_name("preview-picture");
    // Contain-fit (§A), so a 16:9 frame letterboxes inside the panel instead of squashing.
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(-1, PICTURE_MIN_HEIGHT);
    picture.set_can_shrink(true);
    // The picture is the panel's content at whatever size the panel got (§A's paned gives it far
    // more than `PREVIEW_PANEL_W` now), so it fills that box and contain-fits the frame inside it
    // -- a picture that only asked for its minimum drew as a small plate in the middle of the
    // panel.
    picture.set_hexpand(true);
    picture.set_vexpand(true);

    let button = gtk::Button::new();
    button.set_widget_name("preview-picture-button");
    button.add_css_class("video-plate");
    button.set_tooltip_text(Some(CLICK_TIP));
    button.set_hexpand(true);
    button.set_vexpand(true);
    button.set_child(Some(&picture));
    button
}

fn picture(window: &adw::ApplicationWindow) -> Option<gtk::Picture> {
    find(window.content()?.upcast_ref(), "preview-picture")?
        .downcast()
        .ok()
}

fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Some(found) = find(&widget, name) {
            return Some(found);
        }
    }
    None
}

/// Put one extracted frame on the panel. So a line move never waits on a decode.
pub fn show_frame(window: &adw::ApplicationWindow, path: Option<&std::path::Path>) -> bool {
    let Some(picture) = picture(window) else {
        return false;
    };
    let shown = match path {
        Some(file) => match crate::ui::cut_picture_cache::picture(file) {
            crate::ui::cut_picture_cache::Decoded::Ready(held) => Some(held.texture),
            crate::ui::cut_picture_cache::Decoded::Coming => {
                owe_picture(&picture);
                return true;
            }
            crate::ui::cut_picture_cache::Decoded::Missing => None,
        },
        None => None,
    };
    CUT_PAGE.with(
        |CutPage {
             picture_owed: owed, ..
         }| owed.borrow_mut().take(),
    );
    match shown {
        Some(texture) => {
            picture.set_paintable(Some(&texture));
            CUT_PAGE.with(
                |CutPage {
                     frame_shown: slot, ..
                 }| *slot.borrow_mut() = path.map(|p| p.to_path_buf()),
            );
            true
        }
        None => {
            picture.set_paintable(None::<&gtk::gdk::Paintable>);
            CUT_PAGE.with(
                |CutPage {
                     frame_shown: slot, ..
                 }| *slot.borrow_mut() = None,
            );
            false
        }
    }
}

fn owe_picture(picture: &gtk::Picture) {
    CUT_PAGE.with(
        |CutPage {
             picture_owed: owed, ..
         }| {
            let mut owed = owed.borrow_mut();
            if owed.is_none() {
                *owed = Some(crate::ui::settle::owe("the Cut preview's picture", picture));
            }
        },
    );
}

pub fn repaint_landed() {
    let Some(window) = crate::ui::window::built_window() else {
        return;
    };
    if video_shown() {
        return;
    }
    refresh(&window);
}

pub fn frame_shown() -> Option<std::path::PathBuf> {
    CUT_PAGE.with(
        |CutPage {
             frame_shown: slot, ..
         }| slot.borrow().clone(),
    )
}

/// The frame the panel is showing, as the file it came from. Named for the same convention as
/// [`preview_asked_for_test`]: the program never reads it back.
pub fn frame_shown_for_test() -> Option<std::path::PathBuf> {
    CUT_PAGE.with(
        |CutPage {
             frame_shown: slot, ..
         }| slot.borrow().clone(),
    )
}

pub fn preview_recording_base() -> Option<String> {
    CUT_PAGE.with(
        |CutPage {
             preview_base: slot, ..
         }| slot.borrow().clone(),
    )
}

pub fn set_preview_camera_base(base: &str) {
    CUT_PAGE.with(
        |CutPage {
             preview_base: slot, ..
         }| *slot.borrow_mut() = Some(base.to_string()),
    );
}

pub fn preview_asked_for_test() -> Option<(String, f64)> {
    CUT_PAGE.with(
        |CutPage {
             preview_asked: slot,
             ..
         }| slot.borrow().clone(),
    )
}

pub fn refresh(window: &adw::ApplicationWindow) -> bool {
    // F2.12 S7 first: a card is not the session's own picture, so a line standing on one shows the
    // card's rendered frame and nothing else.
    if let Some((frame, held)) = crate::ui::cut_card_preview::card_frame(
        &ui::window::newest_review_cut(),
        // The FRAME's second, not the clock's: a line standing on a stop keeps showing that stop's
        // first frame (§F3.3 S4), and a card cannot be re-timed anyway.
        crate::cut_speed_play::picture_second(
            &ui::window::newest_review_cut().fx,
            ui::line_position().t,
        ),
    ) {
        // The held sentence belongs to the frame it explains: say it while the card is on the
        // panel, because everywhere else on this page the picture moves when the clock does.
        if let (Some(line), Some(status)) = (held, ui::find_status(window.upcast_ref())) {
            status.set_text(&line);
        }
        release_video();
        if frame.as_os_str().is_empty() {
            return true;
        }
        return show_frame(window, Some(&frame));
    }
    if video_shown() && crate::ui::cut_record_play::follow_in_video(window, ui::line_position().t) {
        return true;
    }
    release_video();
    let colouring = crate::ui::cut_cameras::row_spans(&ui::window::newest_review_cut());
    let recordings = if colouring.is_empty() {
        ui::page_recordings()
    } else {
        colouring
    };
    if recordings.is_empty() {
        return show_frame(window, None);
    }
    // The preview's row (the watched one, or the one the scene's lens picks) is a ROW of the
    // colouring, and a row holds every file one camera filmed one after another.
    let row = ui::preview_row();
    let t = crate::cut_speed_play::picture_second(
        &ui::window::newest_review_cut().fx,
        ui::line_position().t,
    );
    // A snapshot names the session it is rendering (`snapshot.rs`), because its folder is the
    // fixture and not the working copy a real session writes frames into.
    let named = preview_recording_base();
    let chosen: Option<&timeline::Recording> = match named {
        Some(base) => recordings.iter().find(|recording| recording.base == base),
        None => recording_at(&recordings, row, t),
    };
    let Some(recording) = chosen else {
        return show_frame(window, None);
    };
    let base = recording.base.clone();
    let Some(tree) = session_tree() else {
        return show_frame(window, None);
    };
    // The frames on disk carry the FILE's own stamps, so the line's session second has to be
    // converted before it can pick one.
    let at = timeline::local_at(
        // Same rule one door down: under a stop the panel shows the held frame, so that is the
        // second asked for.
        crate::cut_speed_play::picture_second(
            &ui::window::newest_review_cut().fx,
            ui::line_position().t,
        ),
        recording.start,
        0.0,
        timeline::shift_of(&ui::window::newest_review_cut(), &base),
    );
    CUT_PAGE.with(
        |CutPage {
             preview_asked: slot,
             ..
         }| *slot.borrow_mut() = Some((base.clone(), at)),
    );
    let Some(paths) = crate::ui::cut_picture_cache::frames_or_coming(&tree, &base) else {
        if let Some(picture) = picture(window) {
            owe_picture(&picture);
        }
        return true;
    };
    // Frames are named on the file's own clock: its start instant plus the frame's second in the
    // file (`frames::frame_file(start, at)`), and the first frame of every folder is second 0.
    let zero = paths
        .first()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .and_then(crate::clock::read_frame)
        .unwrap_or(0.0);
    let found = cut_tracks::frame_near(&paths, zero + at);
    let agreed =
        crate::effect_overlap::source_frame(first_footage_shape(), shown_shape(found.as_deref()));
    note_plate_shape(agreed);
    // §8 (`unrenderable still ">>> the stop frame at m:ss cannot be shown in the preview: <err>"`):
    // a stop holds a frame, and when the search above finds none the preview owes that sentence.
    crate::ui::cut_stop_frame::say_missing_still(
        window,
        &ui::window::newest_review_cut().fx,
        ui::line_position().t,
        found.is_some(),
    );
    show_frame(window, found.as_deref())
}

pub fn show_video(window: &adw::ApplicationWindow) {
    let (Some(picture), Some(paintable)) = (picture(window), crate::play_built_in::paintable())
    else {
        return;
    };
    if let Ok(paintable) = paintable.downcast::<gtk::gdk::Paintable>() {
        picture.set_paintable(Some(&paintable));
        CUT_PAGE.with(
            |CutPage {
                 video_shown: cell, ..
             }| cell.set(true),
        );
    }
}

pub fn video_shown() -> bool {
    CUT_PAGE.with(
        |CutPage {
             video_shown: cell, ..
         }| cell.get(),
    )
}

fn release_video() {
    if CUT_PAGE.with(
        |CutPage {
             video_shown: cell, ..
         }| cell.replace(false),
    ) {
        crate::play_built_in::stop();
    }
}

/// The recording the picture shows at session second `t` (the preview's row, as [`refresh`] chooses
/// it) and that second on the recording's own clock.
pub fn picture_source(t: f64) -> Option<(timeline::Recording, f64)> {
    let cut_ = ui::window::newest_review_cut();
    let colouring = crate::ui::cut_cameras::row_spans(&cut_);
    let recordings = if colouring.is_empty() {
        ui::page_recordings()
    } else {
        colouring
    };
    let recording = recording_at(&recordings, ui::preview_row(), t)?.clone();
    let local = timeline::local_at(
        t,
        recording.start,
        0.0,
        timeline::shift_of(&cut_, &recording.base),
    );
    Some((recording, local))
}

pub fn recording_at(
    recordings: &[timeline::Recording],
    row: usize,
    t: f64,
) -> Option<&timeline::Recording> {
    let rows = timeline::rows_for(recordings, &ui::window::newest_review_cut());
    let on_row: Vec<&timeline::Recording> = recordings
        .iter()
        .zip(rows)
        .filter(|(_, at)| *at == row)
        .map(|(recording, _)| recording)
        .collect();
    let pool: Vec<&timeline::Recording> = if on_row.is_empty() {
        recordings.iter().collect()
    } else {
        on_row
    };
    let distance = |r: &timeline::Recording| {
        if t < r.start {
            r.start - t
        } else if t > r.end {
            t - r.end
        } else {
            0.0
        }
    };
    pool.into_iter().min_by(|a, b| {
        distance(a)
            .partial_cmp(&distance(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn session_tree() -> Option<layout::Tree> {
    if let Some(dir) = snapshot_tree() {
        return layout::Tree::new(&dir).ok();
    }
    let dir = ui::line_root()?;
    layout::Tree::new(&dir).ok()
}

fn snapshot_tree() -> Option<std::path::PathBuf> {
    CUT_PAGE.with(
        |CutPage {
             snapshot_dir: slot, ..
         }| slot.borrow().clone(),
    )
}

pub fn set_snapshot_dir(dir: &std::path::Path) {
    CUT_PAGE.with(
        |CutPage {
             snapshot_dir: slot, ..
         }| *slot.borrow_mut() = Some(dir.to_path_buf()),
    );
}

pub fn snapshot_frame_tree() -> Option<layout::Tree> {
    snapshot_tree().and_then(|dir| {
        layout::Tree::new(&dir)
            .ok()
            .or_else(|| layout::Tree::new(crate::startup::session_dir(&dir)).ok())
    })
}
pub fn note_line_moved(window: &adw::ApplicationWindow, at: f64) {
    ui::set_line_position(window, crate::cut_line::LinePos { t: at });
    refresh(window);
}

/// The click (§A: "click toggles play").
fn shown_shape(path: Option<&std::path::Path>) -> Option<(i32, i32)> {
    let path = path?;
    if let Some(shape) = crate::ui::cut_picture_cache::held_shape(path) {
        return Some(shape);
    }
    if let Some(shape) = CUT_PAGE.with(|CutPage { shapes, .. }| shapes.borrow().get(path).copied())
    {
        return shape;
    }
    let shape = gtk::gdk_pixbuf::Pixbuf::file_info(path)
        .map(|(_, w, h)| (w, h))
        .filter(|(w, h)| *w > 0 && *h > 0);
    CUT_PAGE.with(|CutPage { shapes, .. }| shapes.borrow_mut().insert(path.to_path_buf(), shape));
    shape
}

fn first_footage_shape() -> Option<(i32, i32)> {
    let recordings = ui::page_recordings();
    let first = recordings.first()?;
    let tree = session_tree()?;
    shown_shape(
        crate::ui::cut_picture_cache::frames_of(&tree, &first.base)
            .first()
            .map(|path| path.as_path()),
    )
}

/// What the plate and every overlay on it are fitted to, as §9 says both sides must be.
pub fn plate_shape() -> (i32, i32) {
    CUT_PAGE
        .with(
            |CutPage {
                 plate_shape: slot, ..
             }| *slot.borrow(),
        )
        .unwrap_or(FALLBACK_SHAPE)
}

const FALLBACK_SHAPE: (i32, i32) = (
    crate::ui::window::PREVIEW_PANEL_W as i32,
    crate::ui::window::PREVIEW_PANEL_H as i32,
);

/// The same note for a caller that knows the plate's shape from a file rather than from the page's
/// own search: a snapshot staging its own frame (`snapshot_preview`) has the JPEG on disk and can
/// say what it measures, instead of leaving the plate on `FALLBACK_SHAPE` and letting one set of
/// fractions mean two different boxes in px.
pub fn note_plate_shape(shape: (i32, i32)) {
    CUT_PAGE.with(
        |CutPage {
             plate_shape: slot, ..
         }| *slot.borrow_mut() = Some(shape),
    );
}

pub fn wire_click(window: &adw::ApplicationWindow) {
    crate::ui::cut_picture_cache::on_landed(repaint_landed);
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
