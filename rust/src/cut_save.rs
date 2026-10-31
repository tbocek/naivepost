//! §05-cut#7-rules bullet 7 — `cut.json is written by one function`.

use crate::cut::{self, Cut};
use crate::layout;

thread_local! {
    static GESTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn write_cut(cut_: &Cut) -> Result<(), String> {
    if gesture_in_flight() {
        return Ok(());
    }
    let dir = crate::startup::open_dir();
    let tree = layout::Tree::new(&dir).map_err(|err| err.to_string())?;
    cut::save(cut_, &tree)
}

pub fn write_cut_at(cut_: &Cut, tree: &layout::Tree) -> Result<(), String> {
    if gesture_in_flight() {
        return Ok(());
    }
    cut::save(cut_, tree)
}

pub fn exists() -> bool {
    let dir = crate::startup::open_dir();
    match layout::Tree::new(&dir) {
        Ok(tree) => cut::exists(&tree),
        Err(_) => false,
    }
}

pub fn begin_gesture() {
    GESTURE.with(|cell| cell.set(true));
}

pub fn end_gesture() {
    GESTURE.with(|cell| cell.set(false));
}

pub fn gesture_in_flight() -> bool {
    GESTURE.with(|cell| cell.get())
}
