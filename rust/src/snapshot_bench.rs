//! The one thing the `04-prepare` picture cannot show by itself: an EDITED prompt row.

use adw::prelude::*;
use gtk4 as gtk;

use crate::{bench, settings, ui};

/// Make the bench row `index` genuinely edited on this machine, before the window is built.
pub fn stage_edited_row(index: usize) {
    let Some(row) = bench::ROWS.get(index) else {
        println!("snapshot: no bench row {index} to stage");
        return;
    };
    let Some(paths) = settings_home() else {
        println!("snapshot: no settings folder to stage an edited prompt in");
        return;
    };
    let shipped = bench::shipped(row);
    // The row's own door, not a hand-written file: this is what `bench::store` runs on a keystroke, so the
    // mark appears because the row really is edited and Reset really restores. A User Context row (empty
    // key) belongs to the project and stores nothing -- staging one would write no override.
    let mut project = crate::project::Project::default();
    let edited = format!("{shipped}\nEdited on this machine for the screenshot.\n");
    match bench::store(&paths, &mut project, row, &edited) {
        Ok(()) if bench::edited(&paths, row) => println!(
            "snapshot: staged an edited prompt for row {index} ({})",
            row.title
        ),
        Ok(()) => println!(
            "snapshot: row {index} ({}) holds no per-machine wording -- nothing to stage",
            row.title
        ),
        Err(why) => println!("snapshot: could not stage row {index} -- {why}"),
    }
}

/// This machine's settings folder, or a private one this shot owns.
fn settings_home() -> Option<settings::Paths> {
    if let Some(paths) = settings::from_environment() {
        return Some(paths);
    }
    let folder = std::env::temp_dir().join(format!("naivepost-snapshot-{}", std::process::id()));
    if std::fs::create_dir_all(&folder).is_err() {
        return None;
    }
    std::env::set_var("XDG_CONFIG_HOME", &folder);
    settings::from_environment()
}

/// Width of the heading-row picture: wide enough for "‹Describe› prompt", the mark's sentence, a row name
/// and Reset beside it, measured so nothing is clipped.
const HEADING_SHOT_W: i32 = 700;
/// Its height: the row plus a band of background, so the widgets' extents are visible and not clipped.
const HEADING_SHOT_H: i32 = 60;
/// §1's window size, which every shell screenshot is taken at (`snapshot.rs` packs it into the same slot
/// before this runs; restoring it here is what keeps the page shot beside this one at that size).
const SHELL_SHOT_W: i32 = 1240;
const SHELL_SHOT_H: i32 = 900;

/// Paint the bench's heading row — title, mark, picker, Reset — into `out`.
pub fn paint_heading(
    window: &adw::ApplicationWindow,
    out: &std::path::Path,
) -> Result<String, String> {
    let heading = ui::find_source_widget(window, "bench-heading")
        .ok_or("the Prepare page drew no bench-heading row")?;
    let said = |name: &str| {
        ui::find_source_widget(window, name)
            .map(|widget| {
                format!(
                    "{name}={}",
                    if widget.is_visible() {
                        "shown"
                    } else {
                        "hidden"
                    }
                )
            })
            .unwrap_or_else(|| format!("{name}=absent"))
    };
    let report = ["prepare-title", "bench-mark", "bench-reset"]
        .iter()
        .map(|name| said(name))
        .collect::<Vec<_>>()
        .join(", ");
    let parent = heading.parent();
    let holder = gtk::Window::new();
    holder.set_decorated(false);
    holder.set_default_size(HEADING_SHOT_W, HEADING_SHOT_H);
    holder.set_child(Some(&heading));
    holder.present();
    crate::snapshot_face::pump_context(64);
    crate::snapshot_face::set_shot_size(HEADING_SHOT_W, HEADING_SHOT_H);
    let result = crate::snapshot_face::write_png(heading.upcast_ref(), out);
    // …and the page shot beside this one goes back to the size its caller asked for (the slot is
    // process-wide). It has to be re-read from `--out`'s own numbers here, because the ask was packed once
    // before this closure and `asked_shot_wh` now holds MY size, not the window's.
    crate::snapshot_face::set_shot_size(
        window.width().max(SHELL_SHOT_W),
        window.height().max(SHELL_SHOT_H),
    );
    holder.close();
    // Hand the row back to the page it came from before reporting, so a failure here cannot leave the
    // window without its bench heading.
    if let Some(box_) = parent
        .as_ref()
        .and_then(|node| node.downcast_ref::<gtk::Box>())
    {
        box_.append(&heading);
    }
    result.map(|()| report)
}
