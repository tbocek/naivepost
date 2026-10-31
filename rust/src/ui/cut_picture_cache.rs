//! §05-cut §1 (Pictures) under §03-shell §1 "The window thread never waits": the preview picture and band film rows are painted from what workers already read; a paint stats at most a folder or file, and the page repaints when a worker lands.
//! Three caches, each keyed so a changed file is read again:
//! - a frame folder's listing, by change time ([`frames_of`]);
//! - a decoded frame, by size and change time ([`picture`]), one decode at a time, newest wanted next;
//! - a card's rendered frames, by frame folder ([`card_frames`]).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::layout;

type Stamp = (u64, Option<SystemTime>);

fn stamp_of(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

#[derive(Clone)]
pub struct Picture {
    pub texture: gtk::gdk::Texture,
    pub wide: i32,
    pub high: i32,
}

pub enum Decoded {
    Ready(Picture),
    Coming,
    Missing,
}

thread_local! {
    static LISTS: std::cell::RefCell<HashMap<PathBuf, (Stamp, Vec<PathBuf>)>> =
        std::cell::RefCell::new(HashMap::new());
    static LISTING: std::cell::RefCell<HashSet<PathBuf>> = std::cell::RefCell::new(HashSet::new());
    static PICTURES: std::cell::RefCell<HashMap<PathBuf, (Stamp, Option<Picture>)>> =
        std::cell::RefCell::new(HashMap::new());
    static DECODING: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static NEXT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static RENDERING: std::cell::RefCell<HashSet<PathBuf>> = std::cell::RefCell::new(HashSet::new());
    static ON_LANDED: std::cell::Cell<Option<fn()>> = const { std::cell::Cell::new(None) };
    static LISTED_HERE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn on_landed(repaint: fn()) {
    ON_LANDED.with(|slot| slot.set(Some(repaint)));
}

fn landed() {
    if let Some(repaint) = ON_LANDED.with(|slot| slot.get()) {
        repaint();
    }
    crate::ui::cut_picture_rows::repaint_bands();
    crate::ui::produce_paint::repaint_plates();
    crate::ui::narrate_picture::repaint_landed();
}

/// One recording's extracted frames, oldest first: the listing held for this folder version, else the previous one while a worker relists. A missing folder is no frames.
pub fn frames_of(tree: &layout::Tree, base: &str) -> Vec<PathBuf> {
    frames_or_coming(tree, base).unwrap_or_default()
}

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

/// The decoded frame at `path` if a worker has decoded this version; otherwise asks for it and answers [`Decoded::Coming`].
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

const KEPT_PICTURES: usize = 240;

pub fn held_shape(path: &Path) -> Option<(i32, i32)> {
    PICTURES.with(|pictures| {
        pictures
            .borrow()
            .get(path)
            .and_then(|(_, held)| held.as_ref().map(|picture| (picture.wide, picture.high)))
    })
}

pub enum Card {
    Ready(Vec<PathBuf>),
    Coming,
    Failed,
}

/// A card's rendered frames in `dir`: listed if there, else rendered on a worker by `render`; the page repaints when they land.
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

pub fn forget_card(dir: &Path) {
    LISTS.with(|lists| lists.borrow_mut().remove(dir));
}

pub fn note_listed_here() {
    LISTED_HERE.with(|count| count.set(count.get() + 1));
}

pub fn listed_here_for_test() -> usize {
    LISTED_HERE.with(|count| count.get())
}

/// For a test: whether this version of the frame at `path` is decoded and held.
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
