//! The one thing the `04-prepare` picture cannot show by itself: an EDITED prompt row.
//!
//! §1's badge **2** is the heading row — "title, "edited — kept in your settings" mark, then at the
//! row's right end the row picker and Reset" — and badge **4** says Reset is "live only while this
//! machine holds an edit". Both are painted by [`crate::ui::bench_paint::paint_marks`] from
//! [`crate::bench::edited`], which asks the settings folder whether it holds an override for the row.
//! A snapshot presses no keys, so on a clean machine the shot could only ever draw the title, the picker
//! and a greyed Reset: the two widgets §1 pictures there were invisible by construction.
//!
//! So the shot makes the state real rather than drawing the label: it writes an override through
//! [`crate::settings::write_prompt`], the SAME door a keystroke goes through (`bench::store` calls it),
//! so `bench::edited` answers true of its own accord, the mark shows because the row really is edited,
//! and Reset really restores the shipped wording when pressed.

use adw::prelude::*;
use gtk4 as gtk;

use crate::{bench, settings, ui};

/// Make the bench row `index` genuinely edited on this machine, before the window is built.
///
/// The bench reads its settings through [`settings::from_environment`], so the shot has to give that
/// door a home it may write into; where the environment names none (a bare container), a private folder
/// under the temp dir becomes `$XDG_CONFIG_HOME`. Nothing is invented: the stored text is the shipped
/// wording plus one line, exactly what typing that line would have stored.
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
    // The bench asks `from_environment`, so pointing it at a folder of our own is the only way the mark
    // can be earned rather than painted. Per-process name: several snapshots may run side by side.
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
///
/// §1's badges **2**-**4** are that one row, and in a full-window shot it is ~30 px of a 900 px picture:
/// the mark's glyph (U+270E) is a hand-drawn pencil DejaVu Sans draws as a barely visible outline, so at
/// the window's scale neither the mark nor Reset's liveness could be read. The row therefore gets its own
/// picture, painted from THE LIVE WIDGET (`bench-heading`, found by name in the real window) rather than
/// copied or restated: whatever [`crate::ui::bench_paint::paint_marks`] decided is what appears here, which
/// is why this runs after the window has been laid out. Same precedent as the picker list and the effect
/// menu, both of which also cannot be read in the whole-window shot.
pub fn paint_heading(
    window: &adw::ApplicationWindow,
    out: &std::path::Path,
) -> Result<String, String> {
    let heading = ui::find_source_widget(window, "bench-heading")
        .ok_or("the Prepare page drew no bench-heading row")?;
    // What the row says right now, printed so a red run tells a hidden mark from an unreadable glyph.
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
    // A bare top level, as `snapshot_prompt_picker::paint` learned: an adw widget only lays out inside an
    // `adw::ApplicationWindow`, and this holder belongs to no application. The row is RE-PARENTED into it
    // (a widget has one parent) and handed back afterwards, so the page keeps its bench.
    let parent = heading.parent();
    let holder = gtk::Window::new();
    holder.set_decorated(false);
    holder.set_default_size(HEADING_SHOT_W, HEADING_SHOT_H);
    holder.set_child(Some(&heading));
    holder.present();
    crate::snapshot_face::pump_context(64);
    // The frame is taken at THIS size, not the window's: `snapshot_face::write_png` asks `shot_size`, which
    // reports the asked 1240x900 for any widget once a shot has been requested, and a 900 px canvas behind a
    // 34 px row buried the heading in background. Restored afterwards so nothing else inherits the ask.
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

// The row's own pixels come from `snapshot_face::write_png`, asked for this size through `set_shot_size`.
