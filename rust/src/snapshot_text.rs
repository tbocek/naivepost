//! The `06-text` screen (§F3.4): `spec/img/06-text.png` shows the caption **on** the preview — white bold
//! words with a dark edge, across the lower third of the frame — and the `Text at 1:21` form beside it with the
//! same words in its field.
//!
//! Those two things are two live states, and the app moves between them with one button: Apply is what publishes
//! the typed caption to the cut (and therefore to the painter), and Apply also closes the form. So the shot cannot
//! be one press; it is the two a person actually performs, in the order the picture reads them:
//!
//! 1. arm ❝ Text → land the lower-third box (a zero-travel press, §S2) → type the words into the form's own
//!    TextView → **Apply**, which writes the record through `record_edit`, so the lane, the clock and the frame
//!    all hold the caption the render would draw. Every one of those is a door the widgets fire, never state poked
//!    from here.
//! 2. arm ❝ Text again and land the box at the same second, which opens a fresh form for that second — the state
//!    the spec's right-hand column shows — and type the same words into it. A re-press rather than a reopened
//!    form because the form the person applied is gone by definition; the app has no "edit a placed caption" entry
//!    until F3.9's lane grip, and inventing one here to make a nicer picture would be staging a flow the spec
//!    does not have.
//!
//! A real picture is needed under the words, and the demo fixture ships no extracted frames, so this writes one
//! PNG through [`crate::snapshot_face::seed_snapshot_frame`] into the folder the page lists frames from — the
//! same door `05-cut`, `06-preview` and `05-lanes` use.
//!
//! Nothing here decides a rule. Every constant, sentence and box comes from `fx_text`; every write goes through
//! `ui`'s doors.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_line;
use crate::fx_text;
use crate::ui;

use crate::snapshot_face::seed_snapshot_frame;

/// The second the shot's red line stands on: 81.0, so the heading reads `Text at 1:21` exactly as the spec's
/// picture spells it. The caption is placed there, which is also where the form belongs.
const LINE_SECOND: f64 = 81.0;

/// The recording the preview searches for its frame, and the second of THAT FILE. Both named here because the
/// shot's caption has to sit inside the scene the frame comes from: a caption at 1:21 over a tape with no 1:21
/// paints words over an empty plate.
const TAPE: &str = "session-tape";

/// Stage the shot onto `window`, through the doors the widgets fire. Returns the sentences for the run's stdout,
/// one per step that did something, so the printed line is what the app said rather than what this file hoped.
pub(crate) fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    // The frame the words go over, written where the page will look for it, and the page pointed at that folder.
    seed_snapshot_frame(dir, TAPE, LINE_SECOND);
    ui::cut_picture::set_preview_camera_base(TAPE);
    ui::cut_picture::set_snapshot_dir(dir);
    // One scene long enough to hold the caption's seconds, so the lane, the rows and the clock all agree that
    // 1:21 is inside the cut. Seeded rather than typed: the fixture's own `cut.json` keeps one clip and `fx: []`.
    let shot = cut::Cut {
        segs: vec![cut::Seg { s: 0.0, e: 300.0, cam: 0, ..Default::default() }],
        ..Default::default()
    };
    ui::seed_review_cut(window, &shot);
    ui::set_session_recordings(&[crate::timeline::Recording {
        base: TAPE.to_string(),
        start: 0.0,
        end: 300.0,
    }]);
    ui::note_place(true);
    ui::set_line_position(window, cut_line::LinePos { t: LINE_SECOND });
    ui::set_playhead(window, LINE_SECOND);

    let mut said = Vec::new();
    // --- 1. the caption the frame paints: arm, land the lower third, type, Apply --------------------------
    let armed = ui::press_effect_item(window, cut::EffectKind::Text);
    assert!(ui::text_armed(), "snapshot: 06-text could not arm -- {armed}");
    said.push(format!("armed: {armed}"));
    // A zero-travel press: §S2 makes a click take the lower third, the wide box the spec's picture shows across
    // the bottom of the frame. The panel's own drag door is what a hand uses; `text_drag_ended` is the seam it
    // hands to, with the panel's pinned size as the frame the box is measured against.
    let landed = ui::text_drag_ended(window, (40.0, 40.0), (40.0, 40.0));
    let form = ui::text_form_open().expect("snapshot: 06-text landed no caption form");
    assert_eq!(form.on, fx_text::LOWER_THIRD, "a click must take the lower third");
    assert_eq!(form.t, LINE_SECOND, "the caption belongs at the line's second -- {landed}");
    ui::text_type_words(window, fx_text::SNAPSHOT_CAPTION);
    let applied = ui::press_text_apply(window);
    let placed = ui::review_cut_of(window)
        .fx
        .iter()
        .filter(|fx| fx.kind == "text")
        .count();
    assert_eq!(placed, 1, "Apply put the caption on the cut -- {applied}");
    assert!(ui::text_form_open().is_none(), "Apply closes the form it applied");
    said.push(format!("applied: {applied} caption -- {applied}"));

    // --- 2. the form the spec's right column shows: arm again, land at the same second, type ----------------
    let rearmed = ui::press_effect_item(window, cut::EffectKind::Text);
    assert!(ui::text_armed(), "snapshot: 06-text could not re-arm -- {rearmed}");
    let landed = ui::text_drag_ended(window, (40.0, 40.0), (40.0, 40.0));
    let open = ui::text_form_open().expect("snapshot: 06-text drew no caption form");
    assert_eq!(open.t, LINE_SECOND, "the form on screen is not for the line's second");
    assert_eq!(open.on, fx_text::LOWER_THIRD, "the reopened box is not the lower third");
    said.push(format!("re-armed and landed: {landed}"));
    ui::text_type_words(window, fx_text::SNAPSHOT_CAPTION);
    // Read back off the form's own TextView rather than echoing the constant: if the typing ever stopped reaching
    // the widget, this prints empty and the stdout line -- and the round's test -- say so.
    let field = words_in(window);
    assert_eq!(
        field, fx_text::SNAPSHOT_CAPTION,
        "snapshot: 06-text's words never reached the form's own words box"
    );
    // The form's STORED caption is still nothing: publishing it is Apply's job, and Apply would close the form
    // this screen exists to show. That is the app's rule, not a gap in the staging.
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

    // The line steps one second inside the caption for the repaint: `fx_lane::visibility_at` ramps the fade-in
    // from the record's own first second, so a line standing AT it paints the words at alpha 0.0 -- invisible.
    // The record's second stays 81.0, which is what the heading and the test read.
    ui::set_line_position(window, cut_line::LinePos { t: LINE_SECOND + 1.0 });
    ui::set_playhead(window, LINE_SECOND + 1.0);

    // Repaint through the page's own door, which is what a keystroke's refresh calls, so the picture and the lane
    // show the placed caption rather than the state from before Apply.
    ui::cut_view::repaint_page(window);
    let shown = ui::cut_picture::refresh(window);
    said.push(format!("frame painted: {shown}"));
    said
}

/// The caption form's own words, read off its TextView (the field IS a TextView, three rows tall, because Enter
/// has to start a new line inside it). `find_source_widget` is the page's own named lookup.
fn words_in(window: &adw::ApplicationWindow) -> String {
    ui::find_source_widget(window, "text-field-words")
        .and_then(|w| w.downcast::<gtk::TextView>().ok())
        .map(|view| {
            let (start, end) = view.buffer().bounds();
            view.buffer().text(&start, &end, true).to_string()
        })
        .unwrap_or_default()
}
