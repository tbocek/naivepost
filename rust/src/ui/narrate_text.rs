//! F4.7 (**Text**) — what a typed row box means and when it reaches disk.
//!
//! The spec's Text bullet is four promises: `[tag] words` sets the delivery, a placement clears the
//! emotion, the write is autosaved 400 ms after typing, and it is flushed on tab leave, on window close,
//! and before Produce reads the file. The tag's meaning is already pure in [`crate::narrate_edit`]
//! ([`crate::narrate_edit::parsed_text`]); this file is the only place that connects it to a widget, and
//! it lives apart from `narrate_page` because the page is the shell of every other control and is at its
//! size budget.
//!
//! One rule worth stating because it decides the shape of everything here: **the widget holds the truth
//! while someone is typing**. The entry is what they are looking at, so a flush re-reads every box by name
//! rather than trusting the held state, and nothing in the write path repaints the rows — a refresh mid-word
//! would steal the cursor from the middle of a sentence.

use crate::narrate_edit;
use crate::narrate_screen;
use crate::ui::narrate_page;
use adw::prelude::*;
use gtk4 as gtk;

/// §F4.7 (**Text**): "autosaved 400 ms after typing". The beat itself is [`crate::shell::NARRATION_AUTOSAVE`]
/// (catalogued as `narrate.autosaveMs`); this only converts it for the timer, so one number owns the rule.
fn autosave_ms() -> u64 {
    crate::shell::NARRATION_AUTOSAVE.as_millis() as u64
}

/// Whether this window owes a narration write, read off the newest shell (the same slot
/// `window::mark_narration_owed` raises). The autosave, the close and Produce's read all answer to it.
fn write_owed() -> bool {
    crate::ui::window::WINDOW_SHELLS.with(|shells| {
        shells
            .borrow()
            .last()
            .map(|shell| shell.borrow().narration_pending.owe())
            .unwrap_or(false)
    })
}

/// Settle the flag once the bytes are out, so a later door does not write them twice.
fn clear_owed() {
    if let Some(shell) = crate::ui::window::WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned()) {
        shell.borrow_mut().narration_pending.flush();
    }
}

/// Apply one row box to its line and into the window's held record, and say what it did.
///
/// Both copies move together on purpose: `window.rs` keeps the lines it flushes on a tab leave in its own
/// held [`crate::narration::Narration`] (S5 refits them there), so an edit that updated only the page's
/// state would be overwritten by the next switch. The page's state is what the rows are drawn from; the
/// held record is what reaches disk. One parse feeds both.
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
            narrate_screen::tag_moves_line(entry.s, entry.e, requested - entry.s, entry.at);
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
    // ...and into the window's copy, which is the one the tab-leave flush writes.
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

/// Typing in a row box. Every keystroke lands the parse in the record at once, marks the write owed on
/// this window's shell (the flag the tab-leave flush already reads), and re-arms the 400 ms autosave so a
/// burst of typing costs one write rather than one per character.
///
/// The guard matters: `refresh` reseeds every box with [`narrate_screen::write_box`], which fires
/// `changed` on its own. Without comparing first, a repaint would count as typing and owe a write nobody
/// asked for.
pub fn note_typing(window: &adw::ApplicationWindow, index: usize, typed: &str) -> String {
    let s = narrate_page::read_state();
    let Some(entry) = s.entries.get(index).cloned() else {
        return String::new();
    };
    let parsed = narrate_edit::parsed_text(typed, &entry.emotion);
    if parsed.words == entry.text && parsed.emotion == entry.emotion && parsed.pos == entry.pos {
        return String::new();
    }
    let said = apply_box(window, index, typed);
    crate::ui::window::mark_narration_owed(window);
    // The beat is re-armed per keystroke: each new timeout replaces the wait, and the one that survives a
    // pause is the one that writes. No repaint in the timer, for the reason stated in the module header.
    glib::timeout_add_local(
        std::time::Duration::from_millis(autosave_ms()),
        {
            let w = window.clone();
            move || {
                flush_owed(&w);
                glib::ControlFlow::Break
            }
        },
    );
    said
}

/// Write whatever is owed, reading EVERY row box by name first so a half-typed line that never got its own
/// commit still reaches the file. Returns `None` when nothing was owed, so the three doors (the autosave's
/// timer, window close, Produce's read) can all be called unconditionally and stay silent when clean.
///
/// Tab leave has its own flush in `window.rs`'s switch handler over the window's held record; this is the
/// door for the two places that have no tab switch to hang on.
pub fn flush_owed(window: &adw::ApplicationWindow) -> Option<String> {
    if !write_owed() {
        return None;
    }
    let s = narrate_page::read_state();
    for index in 0..s.entries.len() {
        let Some(found) = narrate_page::widget_in(window, &format!("line-text-{index}"))
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
        else {
            continue;
        };
        apply_quietly(index, &found.text());
    }
    clear_owed();
    let tree = crate::ui::window::narrate_session_tree()?;
    let record = crate::narration::Narration {
        entries: narrate_page::read_state().entries,
        // The `silent` list is not the page's to invent: read the file back and keep its markers, so a
        // flush of text cannot erase the clips a delete remembered as deliberately silent.
        silent: crate::narration::load(&tree)
            .map(|existing| existing.silent)
            .unwrap_or_default(),
    };
    match crate::narration::save(&record, &tree) {
        Ok(()) => Some("the line was written to narration.json".to_string()),
        Err(why) => {
            crate::ui::window::log_line(&format!(
                "!!! could not write {} -- {why}",
                tree.narration_json().display()
            ));
            Some("could not write the line \u{2014} see log".to_string())
        }
    }
}

/// Copy the page's lines into the window's held record, so every door that flushes (the beat, a tab
/// leave, the close, Produce) writes what the rows actually show. That copy is what S5 refits on arrival;
/// an edit that skipped it would be overwritten by the next switch.
fn carry_into_held_record() {
    let entries = narrate_page::read_state().entries;
    crate::ui::window::HELD_NARRATION.with(|slot| {
        if let Some(held) = slot.borrow().last().cloned() {
            held.borrow_mut().entries = entries;
        }
    });
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
