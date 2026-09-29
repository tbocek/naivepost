//! F4.7's row edits: the time field, ＋ below, and 🗑.
//!
//! These three verbs live apart from the page because the page is already the shell of every other
//! control; what they share is not a widget but the record — each one moves or removes a line and then
//! has to say what that did to the clip underneath. They reach the page through `narrate_page`'s own
//! helpers (`read_state`, `mutate`, `refresh`, `say`, `widget_in`) so there is still exactly one copy
//! of the state, and every rule they apply lives in [`crate::narrate_edit`].

use crate::narrate_details;
use crate::narrate_edit;
use crate::narrate_screen;
use crate::narration::Entry;
use crate::ui::narrate_page;
use adw::prelude::*;
use gtk4 as gtk;

/// §A.5-1's tooltip for the row's time entry, kept here because this module is the one that reads and
/// writes that box: the person needs to know a second inside another clip MOVES the line there, while
/// one outside the cut comes back as a refusal plus the old number written back into the same box.
pub const TIME_FIELD_TIP: &str = "when this line's audio starts (mm:ss.s on the session clock) \u{2014} after the dash is when it stops speaking. A time inside another clip moves the line there; one outside the cut is refused and the box goes back to where the line really is";

/// **18** ＋ a line below this one. Inserts a NEW row under the clicked one — the clicked row's own
/// second is not moved, which is what "below" means.
pub fn press_line_add_below(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = narrate_page::read_state();
    let Some(entry) = s.entries.get(index) else {
        return narrate_page::say(window, narrate_details::nothing_picked());
    };
    // The gap depends on whether the row above has audio to wait for (§F4.7): 0.5 s after its words,
    // 1.2 s flat when there are none.
    let audio_end = entry.s + entry.at;
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
            });
            narrate_page::refresh(window);
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
        record_silence(clip_s, clip_e);
    }
    narrate_page::refresh(window);
    narrate_page::say(window, &said)
}

/// **14** the row's time field, typed. The widget is read here and the seconds go to
/// [`narrate_edit::move_line`]; everything about where the line lands is the rule module's.
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
            });
            // §F4.7: "A move emptying the old clip marks it deliberately silent, like a delete."
            if old != (target.s, target.e) {
                record_silence(old.0, old.1);
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
/// ([`crate::narration::load`] / [`crate::narration::save`]) — no second format writer. A delete
/// after a delete must not write the same clip twice, so the already-marked answer decides.
fn record_silence(clip_s: f64, clip_e: f64) {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return;
    };
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
