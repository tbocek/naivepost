//! §05-cut §1 (Pictures) under §03-shell §1, "The window thread never waits": the preview's picture and the band's
//! film rows are painted from what workers already read. A line move, a playback tick and a band paint stat a
//! folder or a file at most; listing a frame folder (thousands of files), decoding a frame and rendering a card's
//! frames all happen on a worker, and the page repaints when the answer lands. The first rewrite listed the folder
//! twice and decoded up to three full-size JPEGs on every line move, ten times a second during playback, and the
//! band listed every recording's folder on every paint.
//!
//! Three caches, each keyed so a changed file is read again rather than shown stale:
//! - a frame folder's listing, by the folder's change time ([`frames_of`]);
//! - a decoded frame, by the file's size and change time ([`picture`]), one decode at a time, the newest wanted
//!   frame next, so playback never queues up frames it has already passed;
//! - a card's rendered frames, by its frame folder ([`card_frames`]).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::layout;

/// A file or folder as a cache knows it: size and change time.
type Stamp = (u64, Option<SystemTime>);

fn stamp_of(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// A decoded frame: the texture the picture shows, and its shape.
#[derive(Clone)]
pub struct Picture {
    pub texture: gtk::gdk::Texture,
    pub wide: i32,
    pub high: i32,
}

/// What [`picture`] answers.
pub enum Decoded {
    Ready(Picture),
    /// A worker is decoding it (or will next); the caller keeps what is up.
    Coming,
    /// No such file, or it does not decode.
    Missing,
}

thread_local! {
    static LISTS: std::cell::RefCell<HashMap<PathBuf, (Stamp, Vec<PathBuf>)>> =
        std::cell::RefCell::new(HashMap::new());
    static LISTING: std::cell::RefCell<HashSet<PathBuf>> = std::cell::RefCell::new(HashSet::new());
    static PICTURES: std::cell::RefCell<HashMap<PathBuf, (Stamp, Option<Picture>)>> =
        std::cell::RefCell::new(HashMap::new());
    /// The frame a worker is decoding now, and the newest one wanted after it.
    static DECODING: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static NEXT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static RENDERING: std::cell::RefCell<HashSet<PathBuf>> = std::cell::RefCell::new(HashSet::new());
    /// Called when anything lands: the page repaints its picture and its band.
    static ON_LANDED: std::cell::Cell<Option<fn()>> = const { std::cell::Cell::new(None) };
    /// How many folder listings this thread has run itself, for the test that proves a paint runs none.
    static LISTED_HERE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The page's repaint, installed by the window: called whenever a listing, a decode or a card render lands.
pub fn on_landed(repaint: fn()) {
    ON_LANDED.with(|slot| slot.set(Some(repaint)));
}

fn landed() {
    if let Some(repaint) = ON_LANDED.with(|slot| slot.get()) {
        repaint();
    }
    crate::ui::cut_picture_rows::repaint_bands();
}

/// One recording's extracted frames, oldest first, for a paint or a line move: the listing held for this
/// version of the folder, else the one held before (empty the first time) while a worker lists it again. A
/// missing folder is no frames, at once.
pub fn frames_of(tree: &layout::Tree, base: &str) -> Vec<PathBuf> {
    frames_or_coming(tree, base).unwrap_or_default()
}

/// [`frames_of`], telling "nothing listed yet, a worker is on it" (`None`) apart from "no frames" (empty).
pub fn frames_or_coming(tree: &layout::Tree, base: &str) -> Option<Vec<PathBuf>> {
    let dir = tree.frames_dir(base);
    let Some(stamp) = stamp_of(&dir) else {
        return Some(Vec::new());
    };
    let held = LISTS.with(|lists| lists.borrow().get(&dir).cloned());
    if let Some((at, paths)) = &held {
        if *at == stamp {
            return Some(paths.clone());
        }
    }
    if LISTING.with(|listing| listing.borrow_mut().insert(dir.clone())) {
        let (tree, base, key) = (tree.clone(), base.to_string(), dir.clone());
        crate::ui::background::run(
            "cut-frames",
            move || crate::ui::cut_tracks::frame_paths(&tree, &base),
            move |paths| {
                LISTING.with(|listing| listing.borrow_mut().remove(&key));
                LISTS.with(|lists| lists.borrow_mut().insert(key, (stamp, paths)));
                landed();
            },
        );
    }
    held.map(|(_, paths)| paths)
}

/// The frame at `path`, decoded, if a worker has decoded this version of it; otherwise it is asked for (one
/// decode at a time, the newest ask next) and [`Decoded::Coming`] answers. Reading the file's size and time is
/// the only disk work this does.
pub fn picture(path: &Path) -> Decoded {
    let Some(stamp) = stamp_of(path) else {
        return Decoded::Missing;
    };
    if let Some((at, held)) = PICTURES.with(|pictures| pictures.borrow().get(path).cloned()) {
        if at == stamp {
            return match held {
                Some(picture) => Decoded::Ready(picture),
                None => Decoded::Missing,
            };
        }
    }
    let busy = DECODING.with(|decoding| decoding.borrow().is_some());
    if busy {
        if DECODING.with(|decoding| decoding.borrow().as_deref() != Some(path)) {
            NEXT.with(|next| *next.borrow_mut() = Some(path.to_path_buf()));
        }
    } else {
        decode(path.to_path_buf(), stamp);
    }
    Decoded::Coming
}

/// Hand one frame to a worker; when it lands, the next wanted one (if any) goes.
fn decode(path: PathBuf, stamp: Stamp) {
    DECODING.with(|decoding| *decoding.borrow_mut() = Some(path.clone()));
    let file = path.clone();
    crate::ui::background::run(
        "cut-picture",
        move || {
            gtk::gdk_pixbuf::Pixbuf::from_file(&file)
                .ok()
                .and_then(|pixbuf| crate::ui::cut_picture_rows::frame_pixels(&pixbuf))
        },
        move |pixels| {
            let held = pixels.map(|(bytes, wide, high)| Picture {
                texture: gtk::gdk::MemoryTexture::new(
                    wide,
                    high,
                    gtk::gdk::MemoryFormat::B8g8r8a8Premultiplied,
                    &glib::Bytes::from_owned(bytes),
                    wide as usize * 4,
                )
                .upcast(),
                wide,
                high,
            });
            PICTURES.with(|pictures| {
                let mut pictures = pictures.borrow_mut();
                // A playback walks thousands of frames; keep the newest few hundred, not every one.
                if pictures.len() > KEPT_PICTURES {
                    pictures.clear();
                }
                pictures.insert(path, (stamp, held));
            });
            DECODING.with(|decoding| *decoding.borrow_mut() = None);
            if let Some(next) = NEXT.with(|next| next.borrow_mut().take()) {
                if let Some(next_stamp) = stamp_of(&next) {
                    decode(next, next_stamp);
                }
            }
            landed();
        },
    );
}

/// How many decoded frames are held before the cache starts over: a few hundred full frames at most.
const KEPT_PICTURES: usize = 240;

/// A decoded frame's shape when it is held, without decoding anything.
pub fn held_shape(path: &Path) -> Option<(i32, i32)> {
    PICTURES.with(|pictures| {
        pictures
            .borrow()
            .get(path)
            .and_then(|(_, held)| held.as_ref().map(|picture| (picture.wide, picture.high)))
    })
}

/// What [`card_frames`] answers.
pub enum Card {
    Ready(Vec<PathBuf>),
    /// A worker is rendering them.
    Coming,
    /// The render failed: the panel stays on the footage.
    Failed,
}

/// A card's rendered frames in `dir`: listed when they are there, else rendered on a worker by `render`, the
/// page repainting when they land. The render is `cut_card_preview::render_card`, unchanged.
pub fn card_frames(
    dir: &Path,
    render: impl FnOnce() -> Result<Vec<PathBuf>, String> + Send + 'static,
) -> Card {
    if let Some((_, paths)) = LISTS.with(|lists| lists.borrow().get(dir).cloned()) {
        return if paths.is_empty() {
            Card::Failed
        } else {
            Card::Ready(paths)
        };
    }
    if RENDERING.with(|rendering| rendering.borrow_mut().insert(dir.to_path_buf())) {
        let key = dir.to_path_buf();
        crate::ui::background::run(
            "cut-card",
            move || render().unwrap_or_default(),
            move |paths| {
                RENDERING.with(|rendering| rendering.borrow_mut().remove(&key));
                LISTS.with(|lists| lists.borrow_mut().insert(key, ((0, None), paths)));
                landed();
            },
        );
    }
    Card::Coming
}

/// Forget a card's frames: the card file changed, so its frames are rendered again.
pub fn forget_card(dir: &Path) {
    LISTS.with(|lists| lists.borrow_mut().remove(dir));
}

/// For a test: count this thread's own folder listings. A paint or a line move must add none.
pub fn note_listed_here() {
    LISTED_HERE.with(|count| count.set(count.get() + 1));
}

/// For a test: how many folder listings ran on this thread (the window's) so far.
pub fn listed_here_for_test() -> usize {
    LISTED_HERE.with(|count| count.get())
}

/// For a test: whether this version of the frame at `path` is decoded and held. A line move never decodes, so a
/// frame the line just moved onto answers `false` until its worker lands.
pub fn picture_held_for_test(path: &Path) -> bool {
    let Some(stamp) = stamp_of(path) else {
        return false;
    };
    PICTURES.with(|pictures| {
        pictures
            .borrow()
            .get(path)
            .is_some_and(|(at, held)| *at == stamp && held.is_some())
    })
}
