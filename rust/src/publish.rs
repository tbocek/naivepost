//! The upload text and the thumbnail state on disk: `produce/publish/publish.json`
//! (spec/01-project-and-files.md §5).
//!
//! The record itself is [`project::Publish`] — the same one §2 embeds in
//! `naivepost.json`, so there is not a file shape and a project shape that can drift
//! apart. This module is only about the file: where it lives, that its existence is the
//! whole flag, and what deleting it means.

use std::fs;
use std::path::Path;

use crate::layout;
use crate::project::{self, move_to_front, RawPublish};

/// Read `publish.json`. A project that has never run Produce has no file, which is the
/// same state as an empty record — nothing was written and nothing was drawn.
///
/// The file carries the two legacy keys §2 names for this object — `base`, the old
/// radio's index into the frames, and `title_off`, the old spelling of "not printed" —
/// and [`project::migrate_publish`] reads them forward exactly as §2's copy does. A save
/// never writes either again.
pub fn load(tree: &layout::Tree) -> Result<project::Publish, String> {
    let file = tree.publish_json();
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(project::Publish::default())
        }
        Err(err) => return Err(format!("{}: {err}", file.display())),
    };
    let raw: RawPublish =
        serde_json::from_str(&text).map_err(|err| format!("{}: {err}", file.display()))?;
    Ok(project::migrate_publish(raw))
}

/// The one function that writes `publish.json` (§5), beside the description and the two
/// pictures it belongs to.
///
/// §5's example lists every key while this record follows the prototype's `omitempty`
/// set (gui/publish.go:100-156) — same reasoning already recorded for §3 and §4: the
/// example illustrates the keys, and omitting what is unset keeps old files
/// byte-identical. Only `title_box` may be absent, since a project whose title band was
/// never dragged has one by default rather than one it stored.
pub fn save(publish: &project::Publish, tree: &layout::Tree) -> Result<(), String> {
    let text = serde_json::to_string_pretty(publish).map_err(|err| err.to_string())?;
    // Both paths come from layout, so there is one spelling of each — including which of
    // the two publish folders this project uses (§1: the legacy one is read for ever and
    // never migrated). Only their prefix inside the project is taken back off, which is
    // what lets write_file make the directory and set §1's 0644.
    let json = tree.publish_json();
    let rel = json
        .strip_prefix(tree.dir())
        .unwrap_or(Path::new("produce/publish/publish.json"));
    tree.write_file(rel, text.as_bytes())?;

    // The description beside the JSON is the copy the uploader selects and pastes, so it
    // travels with the file. Written only when there is one: an empty description would
    // leave a stale text on disk claiming the last run said something.
    if !publish.description.is_empty() {
        let txt = tree.description_txt();
        let rel = txt
            .strip_prefix(tree.dir())
            .unwrap_or(Path::new("produce/publish/description.txt"));
        tree.write_file(rel, format!("{}\n", publish.description).as_bytes())?;
    }
    Ok(())
}

/// Has the model written this session's text? A file, not a project flag: "an existing
/// `publish.json` means the upload text is written" (§5), so ▶ redraws and re-renders
/// without asking the model again. It is laid down before anything is drawn, which is why
/// a failed draw keeps the thinking rather than losing it.
pub fn is_written(tree: &layout::Tree) -> bool {
    tree.publish_json().exists()
}

/// Delete the publish folder — §5's "deleting the folder starts it over".
///
/// The whole folder, which is why this is a folder operation and not a file one: the
/// JSON, the description and both thumbnails go together, and leaving a picture behind
/// that no longer belongs to any text is worse than deleting one too many. A folder that
/// was never there is already started over.
pub fn start_over(tree: &layout::Tree) -> Result<(), String> {
    let dir = tree.publish_dir();
    // The fallback when neither publish folder exists yet is the legacy path, which is
    // inside the project; a project whose path resolved to its own root would delete the
    // session, and no §1 layout should ever make that true. Refuse rather than test it.
    if dir == tree.dir() {
        return Err(format!(
            "{}: the publish folder is the project itself",
            dir.display()
        ));
    }
    match fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("{}: {err}", dir.display())),
    }
}

/// The base the image model edits: the first frame. Order is the whole answer, which is
/// why there is no index beside it — a list and a pointer into it are two places for the
/// same fact, and they drift (§5).
pub fn base(publish: &project::Publish) -> Option<&str> {
    publish.frames.first().map(String::as_str)
}

/// The pictures to be referred to ("the ship from the second image"), in order.
pub fn references(publish: &project::Publish) -> &[String] {
    // One frame is a base with nothing to refer to; an empty list is neither.
    publish.frames.get(1..).unwrap_or_default()
}

/// Make that frame the base by moving it to the front, keeping the rest's order (§5).
/// `false`, and nothing moved, when the name is not one of the frames.
pub fn set_base(publish: &mut project::Publish, path: &str) -> bool {
    let Some(at) = publish.frames.iter().position(|f| f == path) else {
        return false;
    };
    publish.frames = move_to_front(publish.frames.clone(), at);
    true
}

// `own` — "the thumbnail is a chosen frame, not drawn" (§5) — needs no setter: it is a
// public field on the record, and wrapping a boolean assignment in a function would say
// nothing the field's own name does not.
