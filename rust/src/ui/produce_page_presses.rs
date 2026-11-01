//! The Produce page's presses on its images and its words: each changes the state and keeps it.

use crate::narrate_preview_media::source_name as short;
use crate::produce_screen as screen;
use crate::ui::narrate_page::say;
use crate::ui::produce_page::*;

/// **1** ＋ Add image…: a ninth picture is refused rather than dropping one (§1).
pub fn press_add_image(window: &adw::ApplicationWindow, path: &str) -> String {
    let added = mutate(|s| screen::add_image(&mut s.frames, path));
    if !added {
        return say(
            window,
            &format!(
                "{} images is the most the image model takes \u{2014} remove one first",
                screen::MAX_IMAGES
            ),
        );
    }
    refresh(window);
    say(window, &format!("image added: {}", short(path)))
}

pub fn press_make_base(window: &adw::ApplicationWindow, index: usize) -> String {
    let moved = mutate(|s| {
        if index == 0 || index >= s.frames.len() {
            return false;
        }
        let path = s.frames.remove(index);
        s.frames.insert(0, path);
        true
    });
    if !moved {
        return say(window, "that slot is already the base");
    }
    refresh(window);
    say(
        window,
        "base changed \u{2014} the image model edits the first picture now",
    )
}

pub fn press_change_image(window: &adw::ApplicationWindow, index: usize, path: &str) -> String {
    let changed = mutate(|s| screen::change_image(&mut s.frames, index, path));
    if !changed {
        return say(window, "there is no such slot to change");
    }
    refresh(window);
    say(window, &format!("image changed: {}", short(path)))
}

/// Remove one slot; removing the base makes the next picture the base.
pub fn press_remove_image(window: &adw::ApplicationWindow, index: usize) -> String {
    let removed = mutate(|s| {
        if index >= s.frames.len() {
            return None;
        }
        Some(s.frames.remove(index))
    });
    let Some(path) = removed else {
        return say(window, "there is no such slot to remove");
    };
    refresh(window);
    if read_state().frames.is_empty() {
        return say(window, screen::EMPTY_IMAGES);
    }
    say(window, &format!("image removed: {}", short(&path)))
}

pub fn press_words_reword(window: &adw::ApplicationWindow, index: usize, text: &str) -> String {
    let removed = text.trim().is_empty();
    let done = mutate(|s| {
        if removed {
            return screen::remove_words(&mut s.texts, index);
        }
        match s.texts.get_mut(index) {
            Some(mark) => {
                screen::set_words(mark, text);
                true
            }
            None => false,
        }
    });
    if !done {
        return say(window, "no such word box on the thumbnail");
    }
    keep_page_publish();
    refresh(window);
    say(
        window,
        if removed {
            "words removed"
        } else {
            "words reworded"
        },
    )
}

pub fn press_words_remove(window: &adw::ApplicationWindow, index: usize) -> String {
    let done = mutate(|s| screen::remove_words(&mut s.texts, index));
    if !done {
        return say(window, "no such word box on the thumbnail");
    }
    keep_page_publish();
    refresh(window);
    say(window, "words removed")
}

/// A box dragged onto the picture prints words into it, through the `place_words` rule the logic tests drive.
pub fn press_words_place(
    window: &adw::ApplicationWindow,
    cx: f64,
    cy: f64,
    wf: f64,
    hf: f64,
    text: &str,
) -> String {
    mutate(|s| {
        let mut mark = crate::project::TextMark::default();
        screen::place_words(&mut mark, cx, cy, wf, hf, text);
        s.texts.push(mark);
    });
    keep_page_publish();
    refresh(window);
    say(window, &format!("words placed: {}", text.trim()))
}

pub fn press_words_move(
    window: &adw::ApplicationWindow,
    hold: crate::ui::produce_words_press::Hold,
    at: crate::project::TitleBox,
) -> String {
    use crate::ui::produce_words_press::Hold;
    let done = mutate(|s| match hold {
        Hold::Title => {
            s.title_box = Some(at);
            true
        }
        Hold::Mark(index) => match s.texts.get_mut(index) {
            Some(mark) => {
                (mark.cx, mark.cy, mark.wf, mark.hf) = (at.cx, at.cy, at.wf, at.hf);
                true
            }
            None => false,
        },
    });
    if !done {
        return say(window, "no such word box on the thumbnail");
    }
    keep_page_publish();
    refresh(window);
    say(
        window,
        match hold {
            Hold::Title => "title moved",
            Hold::Mark(_) => "words moved",
        },
    )
}

/// The title band's ✎: reword, or take off when emptied. Counts as seeding either way, so a line taken off stays off.
pub fn press_title_words(window: &adw::ApplicationWindow, text: &str) -> String {
    let words = text.trim().to_string();
    let removed = words.is_empty();
    mutate(|s| {
        s.thumb_title = words;
        s.title_seeded = true;
    });
    keep_page_publish();
    refresh(window);
    say(
        window,
        if removed {
            "title taken off the picture"
        } else {
            "title on the picture reworded"
        },
    )
}
