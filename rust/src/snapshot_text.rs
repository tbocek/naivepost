//! The `06-text` screen (§F3.4): `spec/img/06-text.png` shows the caption **on** the preview (white bold words
//! with a dark edge, lower third) and the `Text at 1:21` form beside it with the same words in its field.
//!
//! Those are two live states and Apply (which publishes the caption to the cut and closes the form) moves
//! between them, so the shot is the two things a person performs, in order:
//!
//! 1. arm ❝ Text → land the lower-third box (zero-travel press, §S2) → type into the form's TextView →
//!    **Apply**, which writes the record through `record_edit`. Every step is a door the widgets fire.
//! 2. arm ❝ Text again and land at the same second, opening a fresh form (the app has no "edit a placed
//!    caption" entry until F3.9's lane grip) → type the same words into it.
//!
//! The demo fixture ships no frames, so one PNG is written through [`crate::snapshot_face::seed_snapshot_frame`]
//! into the folder the page lists frames from. Constants, sentences and boxes come from `fx_text`.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_line;
use crate::fx_text;
use crate::ui;

use crate::snapshot_face::seed_snapshot_frame;

/// The second the shot's red line stands on: 81.0, so the heading reads `Text at 1:21` as the spec's picture spells it.
const LINE_SECOND: f64 = 81.0;

/// The recording the preview searches for its frame, and the second of THAT FILE: the caption must sit inside
/// the scene the frame comes from, or words paint over an empty plate.
const TAPE: &str = "session-tape";

/// Stage the shot onto `window`, through the doors the widgets fire. Returns the sentences for the run's stdout,
/// one per step that did something.
pub(crate) fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    // The frame the words go over, written where the page will look for it.
    seed_snapshot_frame(dir, TAPE, LINE_SECOND);
    ui::cut_picture::set_preview_camera_base(TAPE);
    ui::cut_picture::set_snapshot_dir(dir);
    // One scene long enough to hold the caption's seconds (the fixture's `cut.json` keeps one clip and `fx: []`).
    let shot = cut::Cut {
        segs: vec![cut::Seg {
            s: 0.0,
            e: 300.0,
            cam: 0,
            ..Default::default()
        }],
        ..Default::default()
    };
    ui::seed_review_cut(&shot);
    ui::set_session_recordings(&[crate::timeline::Recording {
        base: TAPE.to_string(),
        start: 0.0,
        end: 300.0,
    }]);
    ui::note_place(true);
    ui::set_line_position(window, cut_line::LinePos { t: LINE_SECOND });
    ui::set_playhead(window, LINE_SECOND);

    let mut said = Vec::new();
    let armed = ui::press_effect_item(window, cut::EffectKind::Text);
    assert!(
        ui::text_armed(),
        "snapshot: 06-text could not arm -- {armed}"
    );
    said.push(format!("armed: {armed}"));
    // A zero-travel press: §S2 makes a click take the lower third. `text_drag_ended` is the seam the panel's drag
    // hands to, with the panel's pinned size as the frame the box is measured against.
    let landed = ui::text_drag_ended(window, (40.0, 40.0), (40.0, 40.0));
    let form = ui::text_form_open().expect("snapshot: 06-text landed no caption form");
    assert_eq!(
        form.on,
        fx_text::LOWER_THIRD,
        "a click must take the lower third"
    );
    assert_eq!(
        form.t, LINE_SECOND,
        "the caption belongs at the line's second -- {landed}"
    );
    ui::text_type_words(window, fx_text::SNAPSHOT_CAPTION);
    let applied = ui::press_text_apply(window);
    let placed = ui::window_preview::newest_review_cut()
        .fx
        .iter()
        .filter(|fx| fx.kind == "text")
        .count();
    assert_eq!(placed, 1, "Apply put the caption on the cut -- {applied}");
    assert!(
        ui::text_form_open().is_none(),
        "Apply closes the form it applied"
    );
    said.push(format!("applied: {applied} caption -- {applied}"));

    let rearmed = ui::press_effect_item(window, cut::EffectKind::Text);
    assert!(
        ui::text_armed(),
        "snapshot: 06-text could not re-arm -- {rearmed}"
    );
    let landed = ui::text_drag_ended(window, (40.0, 40.0), (40.0, 40.0));
    let open = ui::text_form_open().expect("snapshot: 06-text drew no caption form");
    assert_eq!(
        open.t, LINE_SECOND,
        "the form on screen is not for the line's second"
    );
    assert_eq!(
        open.on,
        fx_text::LOWER_THIRD,
        "the reopened box is not the lower third"
    );
    said.push(format!("re-armed and landed: {landed}"));
    ui::text_type_words(window, fx_text::SNAPSHOT_CAPTION);
    // Read back off the form's own TextView rather than echoing the constant, so broken typing prints empty.
    let field = words_in(window);
    assert_eq!(
        field,
        fx_text::SNAPSHOT_CAPTION,
        "snapshot: 06-text's words never reached the form's own words box"
    );
    // The form's STORED caption is still nothing: publishing is Apply's job, which would close the form this screen shows.
    assert!(
        open.words.is_empty(),
        "the form on screen must be the one BEFORE Apply -- a stored caption means Apply ran and closed it"
    );
    said.push(format!(
        "form: {} | {} s | fades {}/{} | words: {field}",
        fx_text::form_title(open.t),
        open.dur,
        open.trans,
        open.tout
    ));
    said.push(format!("box: {:?}", open.on.stored()));

    // The line steps one second inside the caption: `fx_lane::visibility_at` ramps the fade-in from the record's
    // first second, so a line AT it paints alpha 0.0. The record's second stays 81.0.
    ui::set_line_position(
        window,
        cut_line::LinePos {
            t: LINE_SECOND + 1.0,
        },
    );
    ui::set_playhead(window, LINE_SECOND + 1.0);

    // Repaint through the page's own door (as a keystroke's refresh does).
    ui::cut_view::repaint_page(window);
    let shown = ui::cut_picture::refresh(window);
    said.push(format!("frame painted: {shown}"));
    said
}

/// The caption form's own words, read off its TextView (three rows tall so Enter starts a new line in it).
fn words_in(window: &adw::ApplicationWindow) -> String {
    ui::find_source_widget(window, "text-field-words")
        .and_then(|w| w.downcast::<gtk::TextView>().ok())
        .map(|view| {
            let (start, end) = view.buffer().bounds();
            view.buffer().text(&start, &end, true).to_string()
        })
        .unwrap_or_default()
}
