//! F4.7 (**Text**) — what a typed row box means and when it reaches disk.

use crate::narrate_edit;
use crate::narrate_screen;
use crate::ui::narrate_page;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};
use adw::prelude::*;
use gtk4 as gtk;

/// Whether this window owes a narration write, read off the newest shell (the same slot
/// `window::mark_narration_owed` raises). The autosave, the close and Produce's read all answer to it.
fn write_owed() -> bool {
    WINDOW_PAGE.with(
        |WindowPage {
             window_shells: shells,
             ..
         }| {
            shells
                .borrow()
                .last()
                .map(|shell| shell.borrow().narration_pending.owe())
                .unwrap_or(false)
        },
    )
}

/// Settle the flag once the bytes are out, so a later door does not write them twice.
fn clear_owed() {
    if let Some(shell) = WINDOW_PAGE.with(
        |WindowPage {
             window_shells: shells,
             ..
         }| shells.borrow().last().cloned(),
    ) {
        shell.borrow_mut().narration_pending.flush();
    }
}

/// Apply one row box to its line and into the window's held record, and say what it did.
pub fn apply_box(window: &adw::ApplicationWindow, index: usize, typed: &str) -> String {
    let s = narrate_page::read_state();
    let Some(entry) = s.entries.get(index).cloned() else {
        return narrate_page::say(window, crate::narrate_details::nothing_picked());
    };
    let parsed = narrate_edit::parsed_text(typed, &entry.emotion);
    // A `[tag @N]` asks for a second. The clamp is the clip's own bounds read off the LIVE entry, and a
    // refused second leaves the line exactly where it was (§A.5: the box is not a move command that can
    // half-succeed).
    let mut said = String::new();
    let mut moved_to: Option<f64> = None;
    if let Some(requested) = parsed.moves_to {
        // The tag names a second on the session clock; `tag_moves_line` owns the clamp out of the clip's
        // last second, so this call site only converts between the two clocks.
        let (offset, refused) =
            narrate_screen::tag_moves_line(entry.s, entry.e, requested, entry.at);
        if refused {
            said = format!(
                "{} is outside this clip \u{2014} the line stays at {}",
                crate::tools::mm_ss(requested),
                narrate_screen::time_field(entry.s + entry.at)
            );
        } else {
            moved_to = Some(offset);
        }
    }
    narrate_page::mutate(|state| {
        if let Some(row) = state.entries.get_mut(index) {
            row.text = parsed.words.clone();
            row.emotion = parsed.emotion.clone();
            row.pos = parsed.pos.clone();
            if let Some(at) = moved_to {
                row.at = at;
            }
        }
    });
    carry_into_held_record();
    if said.is_empty() {
        said = match parsed.moves_to {
            Some(_) => format!(
                "line updated and moved to {}",
                narrate_screen::time_field(entry.s + moved_to.unwrap_or(entry.at))
            ),
            None => "line updated".to_string(),
        };
    }
    narrate_page::say(window, &said)
}

pub fn note_typing(window: &adw::ApplicationWindow, index: usize, typed: &str) -> String {
    let s = narrate_page::read_state();
    let Some(entry) = s.entries.get(index).cloned() else {
        return String::new();
    };
    let parsed = narrate_edit::parsed_text(typed, &entry.emotion);
    if parsed.words == entry.text
        && parsed.emotion == entry.emotion
        && parsed.pos == entry.pos
        && parsed.moves_to.is_none()
    {
        return String::new();
    }
    let said = apply_box(window, index, typed);
    crate::ui::window::mark_narration_owed();
    crate::ui::shell_beats::arm_autosave(window);
    said
}

pub fn note_structure_change(window: &adw::ApplicationWindow) {
    carry_into_held_record();
    crate::ui::window::mark_narration_owed();
    let _ = flush_owed(window);
}

pub fn flush_owed(window: &adw::ApplicationWindow) -> Option<String> {
    if !write_owed() {
        return None;
    }
    let s = narrate_page::read_state();
    for index in 0..s.entries.len() {
        let Some(found) = narrate_page::widget_in(window, &format!("line-text-{index}"))
            .and_then(|w| w.downcast::<gtk::TextView>().ok())
        else {
            continue;
        };
        apply_quietly(index, &crate::ui::narrate_row::box_text(&found));
    }
    clear_owed();
    let tree = crate::ui::window::narrate_session_tree()?;
    let record = crate::narration::Narration {
        entries: narrate_page::read_state().entries,
        silent: crate::narration::load(&tree)
            .map(|existing| existing.silent)
            .unwrap_or_default(),
    };
    match crate::narration::save(&record, &tree) {
        Ok(()) => Some("the line was written to narration.json".to_string()),
        Err(why) => {
            // Said on the status line too: a structure change's caller drops this function's answer.
            crate::ui::window_logs::say_failure(&format!(
                "!!! could not write {} -- {why}",
                tree.narration_json().display()
            ));
            Some("could not write the line \u{2014} see log".to_string())
        }
    }
}

fn carry_into_held_record() {
    let entries = narrate_page::read_state().entries;
    WINDOW_PAGE.with(
        |WindowPage {
             held_narration: slot,
             ..
         }| {
            if let Some(held) = slot.borrow().last().cloned() {
                held.borrow_mut().entries = entries;
            }
        },
    );
}

/// The parse applied without printing anything: a flush writes N boxes and owes the status line none of them.
fn apply_quietly(index: usize, typed: &str) {
    let s = narrate_page::read_state();
    let Some(entry) = s.entries.get(index).cloned() else {
        return;
    };
    let parsed = narrate_edit::parsed_text(typed, &entry.emotion);
    narrate_page::mutate(|state| {
        if let Some(row) = state.entries.get_mut(index) {
            row.text = parsed.words;
            row.emotion = parsed.emotion;
            row.pos = parsed.pos;
        }
    });
    carry_into_held_record();
}
