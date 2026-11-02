//! §05-cut §1 (Pictures) under §03-shell §1 "The window thread never waits": the preview picture and band film rows are painted from what workers already read; a paint stats at most a folder or file, and the page repaints when a worker lands.
//! Three caches, each keyed so a changed file is read again:
//! - a frame folder's listing, by change time ([`frames_of`]);
//! - a decoded frame, by size and change time ([`picture`]), one decode at a time, newest wanted next;
//! - a card's rendered frames, by frame folder ([`card_frames`]).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::layout;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};

pub(crate) type Stamp = (u64, Option<SystemTime>);

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

pub fn on_landed(repaint: fn()) {
    CUT_PAGE.with(
        |CutPage {
             on_landed: slot, ..
         }| slot.set(Some(repaint)),
    );
}

fn landed() {
    if let Some(repaint) = CUT_PAGE.with(
        |CutPage {
             on_landed: slot, ..
         }| slot.get(),
    ) {
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

/// Each recording's frames folder as it stands now: what a walk over all of them can be kept against.
pub type FolderStamps = Vec<Option<Stamp>>;

pub fn folder_stamps(tree: &layout::Tree, bases: &[String]) -> FolderStamps {
    bases
        .iter()
        .map(|base| stamp_of(&tree.frames_dir(base)))
        .collect()
}

/// For a test: how many frame folders are being listed right now.
pub fn folders_listing_for_test() -> usize {
    CUT_PAGE.with(|CutPage { listing, .. }| listing.borrow().len())
}

pub fn frames_or_coming(tree: &layout::Tree, base: &str) -> Option<Vec<PathBuf>> {
    let dir = tree.frames_dir(base);
    let Some(stamp) = stamp_of(&dir) else {
        return Some(Vec::new());
    };
    let held = CUT_PAGE.with(|CutPage { lists, .. }| lists.borrow().get(&dir).cloned());
    if let Some((at, paths)) = &held {
        if *at == stamp {
            return Some(paths.clone());
        }
    }
    if CUT_PAGE.with(|CutPage { listing, .. }| listing.borrow_mut().insert(dir.clone())) {
        let (tree, base, key) = (tree.clone(), base.to_string(), dir.clone());
        crate::ui::background::run(
            "cut-frames",
            move || crate::ui::cut_tracks::frame_paths(&tree, &base),
            move |paths| {
                CUT_PAGE.with(|CutPage { listing, .. }| listing.borrow_mut().remove(&key));
                CUT_PAGE
                    .with(|CutPage { lists, .. }| lists.borrow_mut().insert(key, (stamp, paths)));
                // Listings land in bursts, a whole lecture's folders at once: the last of them repaints for all.
                if CUT_PAGE.with(|CutPage { listing, .. }| listing.borrow().is_empty()) {
                    landed();
                }
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
    if let Some((at, held)) =
        CUT_PAGE.with(|CutPage { pictures, .. }| pictures.borrow().get(path).cloned())
    {
        if at == stamp {
            return match held {
                Some(picture) => Decoded::Ready(picture),
                None => Decoded::Missing,
            };
        }
    }
    let busy = CUT_PAGE.with(|CutPage { decoding, .. }| decoding.borrow().is_some());
    if busy {
        if CUT_PAGE.with(|CutPage { decoding, .. }| decoding.borrow().as_deref() != Some(path)) {
            CUT_PAGE.with(|CutPage { next, .. }| *next.borrow_mut() = Some(path.to_path_buf()));
        }
    } else {
        decode(path.to_path_buf(), stamp);
    }
    Decoded::Coming
}

fn decode(path: PathBuf, stamp: Stamp) {
    CUT_PAGE.with(|CutPage { decoding, .. }| *decoding.borrow_mut() = Some(path.clone()));
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
            CUT_PAGE.with(|CutPage { pictures, .. }| {
                let mut pictures = pictures.borrow_mut();
                if pictures.len() > KEPT_PICTURES {
                    pictures.clear();
                }
                pictures.insert(path, (stamp, held));
            });
            CUT_PAGE.with(|CutPage { decoding, .. }| *decoding.borrow_mut() = None);
            if let Some(next) = CUT_PAGE.with(|CutPage { next, .. }| next.borrow_mut().take()) {
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
    CUT_PAGE.with(|CutPage { pictures, .. }| {
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
    if let Some((_, paths)) =
        CUT_PAGE.with(|CutPage { lists, .. }| lists.borrow().get(dir).cloned())
    {
        return if paths.is_empty() {
            Card::Failed
        } else {
            Card::Ready(paths)
        };
    }
    if CUT_PAGE.with(|CutPage { rendering, .. }| rendering.borrow_mut().insert(dir.to_path_buf())) {
        let key = dir.to_path_buf();
        crate::ui::background::run(
            "cut-card",
            move || render().unwrap_or_default(),
            move |paths| {
                CUT_PAGE.with(|CutPage { rendering, .. }| rendering.borrow_mut().remove(&key));
                CUT_PAGE.with(|CutPage { lists, .. }| {
                    lists.borrow_mut().insert(key, ((0, None), paths))
                });
                landed();
            },
        );
    }
    Card::Coming
}

pub fn note_listed_here() {
    CUT_PAGE.with(
        |CutPage {
             listed_here: count, ..
         }| count.set(count.get() + 1),
    );
}

pub fn listed_here_for_test() -> usize {
    CUT_PAGE.with(
        |CutPage {
             listed_here: count, ..
         }| count.get(),
    )
}

/// For a test: whether this version of the frame at `path` is decoded and held.
pub fn picture_held_for_test(path: &Path) -> bool {
    let Some(stamp) = stamp_of(path) else {
        return false;
    };
    CUT_PAGE.with(|CutPage { pictures, .. }| {
        pictures
            .borrow()
            .get(path)
            .is_some_and(|(at, held)| *at == stamp && held.is_some())
    })
}
