//! F4.7's row edits: the time field, ＋ below, and 🗑.

use crate::narrate_details;
use crate::narrate_edit;
use crate::narrate_screen;
use crate::narration::Entry;
use crate::ui::narrate_page;
use adw::prelude::*;
use gtk4 as gtk;

pub const TIME_FIELD_TIP: &str = "when this line's audio starts (mm:ss.s on the session clock) \u{2014} after the dash is when it stops speaking. A time inside another clip moves the line there; one outside the cut is refused and the box goes back to where the line really is";

fn take_seconds(
    tree: &crate::layout::Tree,
    entries: &[crate::narration::Entry],
    index: usize,
) -> Option<f64> {
    let wav = crate::narrate_preview_leg::take_file(tree, entries, index)?;
    crate::voice_ref::wav_seconds(&wav)
}

/// **18** ＋ a line below this one.
pub fn press_line_add_below(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = narrate_page::read_state();
    let Some(entry) = s.entries.get(index) else {
        return narrate_page::say(window, narrate_details::nothing_picked());
    };
    // The gap depends on whether the row above has audio to wait for (§F4.7): 0.5 s after its words,
    // 1.2 s flat when there are none.
    let spoken = crate::ui::window::narrate_session_tree()
        .and_then(|tree| take_seconds(&tree, &s.entries, index))
        .unwrap_or(0.0);
    let audio_end = entry.s + entry.at + spoken;
    let clip_end = entry.e;
    match narrate_edit::add_below(audio_end, clip_end, narrate_edit::below_gap(entry)) {
        Ok(at) => {
            narrate_page::mutate(|state| {
                let Some(row) = state.entries.get(index) else {
                    return;
                };
                let fresh = Entry {
                    s: row.s,
                    e: row.e,
                    at: at - row.s,
                    ..Default::default()
                };
                state.entries.insert(index + 1, fresh);
                crate::narration::sort_entries(&mut state.entries);
            });
            narrate_page::refresh(window);
            // §F4.7: the new row is part of the narration record, so it is written like a typed line.
            crate::ui::narrate_text::note_structure_change(window);
            narrate_page::say(
                window,
                &format!("a line added below at {}", narrate_screen::time_field(at)),
            )
        }
        Err(refused) => narrate_page::say(window, &refused),
    }
}

/// **19** 🗑 remove this line.
pub fn press_line_remove(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = narrate_page::read_state();
    let Some(entry) = s.entries.get(index) else {
        return narrate_page::say(window, narrate_details::nothing_picked());
    };
    let clip_s = entry.s;
    let clip_e = entry.e;
    // §F4.7: the longer sentence and the silent marker behind it are owed only when this was the
    // clip's last line; with a sibling still on it the clip still speaks.
    let last_of_clip = narrate_edit::delete_leaves_clip_silent(&s.entries, entry);
    let said = narrate_screen::remove_line(!last_of_clip, clip_s);
    narrate_page::mutate(|state| {
        state.entries.remove(index);
    });
    if last_of_clip {
        // §F4.7: the marker question is asked of the FILE, so the row that just went has to be out of it
        // first — hence the write before the marker, not after.
        crate::ui::narrate_text::note_structure_change(window);
        record_silence(window, clip_s, clip_e);
    } else {
        crate::ui::narrate_text::note_structure_change(window);
    }
    narrate_page::refresh(window);
    narrate_page::say(window, &said)
}

/// **14** the row's time field, typed.
pub fn press_line_move(window: &adw::ApplicationWindow, index: usize, typed: &str) -> String {
    let s = narrate_page::read_state();
    let Some(entry) = s.entries.get(index).cloned() else {
        return narrate_page::say(window, narrate_details::nothing_picked());
    };
    let Some(requested) = narrate_edit::parse_clock(typed) else {
        restore_time_field(window, index, &entry);
        return narrate_page::say(
            window,
            &narrate_screen::time_refused(0.0, entry.s, entry.e, entry.s + entry.at),
        );
    };
    let target = narrate_edit::clip_holding(&s.segs, requested).unwrap_or_default();
    match narrate_edit::move_line(&entry, &target, requested) {
        narrate_edit::MoveResult::Moved { at } => {
            let old = (entry.s, entry.e);
            narrate_page::mutate(|state| {
                if let Some(row) = state.entries.get_mut(index) {
                    row.s = target.s;
                    row.e = target.e;
                    row.at = at;
                }
                crate::narration::sort_entries(&mut state.entries);
            });
            // §F4.7: a move rewrites the entry's bounds in the same file — owed like any other edit,
            // and written BEFORE the emptied-clip marker for the reason given on `record_silence`.
            if old != (target.s, target.e) {
                crate::ui::narrate_text::note_structure_change(window);
                record_silence(window, old.0, old.1);
            } else {
                crate::ui::narrate_text::note_structure_change(window);
            }
            narrate_page::refresh(window);
            narrate_page::say(window, &narrate_edit::moved_said(&target, at))
        }
        narrate_edit::MoveResult::Refused { said } => {
            restore_time_field(window, index, &entry);
            narrate_page::say(window, &said)
        }
    }
}

/// Put the box back to the second the line actually sits at, so a refusal reads as an answer rather
/// than as the app ignoring what was typed (§1: "a time in a gap is refused and written back").
fn restore_time_field(window: &adw::ApplicationWindow, index: usize, entry: &Entry) {
    if let Some(field) = narrate_page::widget_in(window, &format!("line-time-{index}"))
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        field.set_text(&narrate_screen::time_field(entry.s + entry.at));
    }
}

/// Write the emptied clip into `narration.json`'s `silent` list through the file's own pair
/// ([`crate::narration::load`] / [`crate::narration::save`]) — no second format writer.
fn record_silence(window: &adw::ApplicationWindow, clip_s: f64, clip_e: f64) {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return;
    };
    crate::ui::narrate_text::note_structure_change(window);
    let Some(emptied) = narrate_edit::emptied_clip(&tree, clip_s, clip_e) else {
        return;
    };
    if emptied.already_marked {
        return;
    }
    let Ok(mut record) = crate::narration::load(&tree) else {
        return;
    };
    record.silent.push(emptied.marker());
    if let Err(why) = crate::narration::save(&record, &tree) {
        crate::ui::window::log_line(&format!("!!! could not mark the clip silent -- {why}"));
    }
}
