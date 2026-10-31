//! The upload text and the thumbnail state on disk: `produce/publish/publish.json`
//! (spec/01-project-and-files.md §5).

use std::fs;
use std::path::Path;

use crate::layout;
use crate::project::{self, move_to_front, RawPublish};

/// Read `publish.json`.
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
pub fn save(publish: &project::Publish, tree: &layout::Tree) -> Result<(), String> {
    let text = serde_json::to_string_pretty(publish).map_err(|err| err.to_string())?;
    // Both paths come from layout, so there is one spelling of each — including which of
    // the two publish folders this project uses (§1: the legacy one is read for ever and
    // never migrated).
    let json = tree.publish_json();
    let rel = json
        .strip_prefix(tree.dir())
        .unwrap_or(Path::new("produce/publish/publish.json"));
    tree.write_file(rel, text.as_bytes())?;

    if !publish.description.is_empty() {
        let txt = tree.description_txt();
        let rel = txt
            .strip_prefix(tree.dir())
            .unwrap_or(Path::new("produce/publish/description.txt"));
        tree.write_file(rel, format!("{}\n", publish.description).as_bytes())?;
    }
    Ok(())
}

/// Has the model written this session's text?
pub fn is_written(tree: &layout::Tree) -> bool {
    tree.publish_json().exists()
}

/// Delete the publish folder — §5's "deleting the folder starts it over".
pub fn start_over(tree: &layout::Tree) -> Result<(), String> {
    let dir = tree.publish_dir();
    // The fallback when neither publish folder exists yet is the legacy path, which is
    // inside the project; a project whose path resolved to its own root would delete the
    // session, and no §1 layout should ever make that true.
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

/// The base the image model edits: the first frame.
pub fn base(publish: &project::Publish) -> Option<&str> {
    publish.frames.first().map(String::as_str)
}

/// The pictures to be referred to ("the ship from the second image"), in order.
pub fn references(publish: &project::Publish) -> &[String] {
    publish.frames.get(1..).unwrap_or_default()
}

/// Make that frame the base by moving it to the front, keeping the rest's order (§5).
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
