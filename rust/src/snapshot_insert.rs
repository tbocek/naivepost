//! The `05-insert` snapshot: F2.12's own screen, staged the way `05-copy` stages §F2.9 — every mark on the
//! strip is there because a real door wrote it (`insert_chosen_with_length` past the chooser, a radio clicked,
//! `press_insert_apply`), never because state was poked. Split out of `snapshot.rs`, which is past the size a
//! fourth staging block fits in.
//!
//! Four of the flow's faces in one shot, because each one changes what the form and the band look like and the
//! LAST of them is what the pixels record:
//!
//! 1. a card spliced BETWEEN the footage (S5 "the video gets longer by the card");
//! 2. a card laid OVER a drawn selection, which costs the footage those seconds (S5 "replaces those seconds");
//! 3. a video put on a LANE of its own, which adds the band's lane row and cuts nothing (S5 "a row of its own");
//! 4. the SAME card asked with no length at all, so its Seconds entry comes from the file's own last moving
//!    moment (§5) rather than from a number the caller handed it;
//! 5. the S1 refusal, with no line and no selection — the flow's last word when nothing was aimed at.
//!
//! One form is drawn per press and Apply hides it again, so a single PNG cannot hold all four answers: what
//! this shot records is the page AFTER the three inserts — five segments, the over card sitting on the footage
//! it took, the lane's violet span on its own row — with S1's refusal as the last word on the status row. The
//! form column itself (§G's radios, Seconds entry and sound tick) is `insert-form`, pinned widget by widget by
//! `tests/insert_form_widgets.rs`; leaving a fourth one open here would hide the readout column §1 puts beside
//! it and show none of the strip. Every mark is still placed through the real doors — the chooser seam
//! (`insert_chosen_with_length`, the call the chooser's callback makes), §G's mode radio clicked, and Apply's
//! own press — so each returned sentence names the file, the mode and the cut's new length as a real press
//! earned it. The strip is then re-seeded to the staged three through `seed_review_cut`, the door every editing
//! press ends with, so one PNG carries all three inserts without one open form hiding another's work.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_cards;
use crate::cut_insert;
use crate::cut_line::LinePos;
use crate::cut_screen;
use crate::timeline::Recording;
use crate::ui;

/// The tape the flow is staged on: 0–120 s, one take, so every second below is filmed material and a card
/// dropped between two clips still has its hole to fall in.
fn tape() -> Vec<Recording> {
    vec![Recording { base: "session-tape".into(), start: 0.0, end: 120.0 }]
}

/// The cut the flow starts from: two clips with a hole at 30–40, so a spliced card and an over card land in
/// different-looking places and the lane row has footage to overlap.
fn seeded() -> cut::Cut {
    cut::Cut {
        segs: vec![
            cut::Seg { s: 0.0, e: 30.0, cam: 0, ..Default::default() },
            cut::Seg { s: 40.0, e: 110.0, cam: 0, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// Write the cards the flow inserts. The real press seeds `assets/` with the built-in badges (`press_insert`
/// calls `cut_insert::seed_cards` before the chooser opens), so the folder the chooser would have started in is
/// seeded the same way; the two inserts this flow aims at are built-in cards, which is what a first-open
/// chooser would actually have offered.
fn seeded_cards(dir: &Path) {
    let assets = dir.join("assets");
    let _ = std::fs::create_dir_all(&assets);
    cut_insert::seed_cards(&assets, &cut_cards::seeds());
    // `cut_cards::seeds()` names its badges after `TIER_ROWS` -- `s.svg` ... `f.svg` plus the blank `tier.svg`.
    for name in ["s.svg", "a.svg"] {
        assert!(
            assets.join(name).is_file(),
            "the built-in card {name} was not seeded into {}",
            assets.display()
        );
    }
}

/// A seeded card's path, by name.
fn card_path(dir: &Path, name: &str) -> std::path::PathBuf {
    dir.join("assets").join(name)
}

/// Click a radio the form drew, found under this window's own `insert-form` holder — the same walk
/// `tests/insert_form_widgets.rs` does, because a tree-wide name lookup can land on another window's form.
fn pick_mode(window: &adw::ApplicationWindow, name: &str) {
    fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if root.widget_name() == name {
            return Some(root.clone());
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
            if let Some(found) = find(&widget, name) {
                return Some(found);
            }
        }
        None
    }
    let form =
        ui::window::find_widget_by_name(window.upcast_ref(), "insert-form").expect("the form holder exists");
    let radio = find(&form, name).unwrap_or_else(|| panic!("the insert form drew no {name}"));
    let radio = radio.downcast::<gtk::CheckButton>().expect("the mode radios are radio buttons");
    radio.set_active(true);
}

/// The card a LANE insert needs: LANE is offered only to a VIDEO-shaped file (`cut_insert::lane_offered`), and
/// the built-in badges are SVGs, so a lane is staged with a video file written next to them. Its seconds come
/// from the `file_seconds` the chooser seam is handed -- the probe is ffmpeg, and a snapshot must not need it.
fn video_card(dir: &Path) -> std::path::PathBuf {
    let assets = dir.join("assets");
    let _ = std::fs::create_dir_all(&assets);
    let path = assets.join("sting.mp4");
    if !path.is_file() {
        std::fs::write(&path, b"staged for the snapshot: an insert is a FILE, its bytes are never read here")
            .expect("a card file the chooser could have returned");
    }
    path
}

/// A card with a moving moment of its own, written next to the seeded badges. §5's "card length = last moving
/// moment, offered as the insert's length" is what this stage shows: `insert_chosen_with_length` asked with no
/// length answers it from the FILE, because ffprobe reads an SVG as one frame and cannot. The animation ends at
/// 1.6 s, so the Seconds entry opens there rather than at `cut_insert::DEFAULT_SECONDS`. Its holes are declared
/// (§G's `<!-- Input: ... -->`) so the form draws the field row §A puts above the radios.
fn timed_card(dir: &Path) -> std::path::PathBuf {
    let assets = dir.join("assets");
    let _ = std::fs::create_dir_all(&assets);
    let path = assets.join("timed.svg");
    // Rewritten every run: the shot depends on what is IN this file (its animation, its declared hole), so a
    // copy left over from an earlier build of the snapshot must not survive.
    std::fs::write(
        &path,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\" viewBox=\"0 0 1920 1080\">\
         <!-- Input: title | Title | what the card says -->\
         <rect width=\"1920\" height=\"1080\" fill=\"#0d1117\"/>\
         <g transform=\"translate(0,40)\"><text x=\"960\" y=\"720\" font-size=\"240\" text-anchor=\"middle\" fill=\"#ffffff\">{{title}}</text>\
         <animateTransform attributeName=\"transform\" type=\"translate\" values=\"0 40;0 0\" keyTimes=\"0;1\" dur=\"1.6s\" fill=\"freeze\" additive=\"sum\"/></g></svg>",
    )
    .expect("a card file whose own timing the form can read");
    path
}

/// The cut the strip shows for the shot: the three staged inserts, asked of [`cut_insert::place`] directly --
/// the same call Apply makes -- rather than through the open form, because these segments only have to EXIST
/// for the band to draw them.
fn seeded_after_stage(dir: &Path) -> cut::Cut {
    seeded_cards(dir);
    let mut held = seeded();
    let _ = cut_insert::place(
        &mut held,
        &card_path(dir, "s.svg").to_string_lossy(),
        10.0,
        cut_insert::Mode::Between,
        5.0,
        false,
    );
    let _ = cut_insert::place(
        &mut held,
        &card_path(dir, "a.svg").to_string_lossy(),
        20.0,
        cut_insert::Mode::Over,
        8.0,
        false,
    );
    let _ = cut_insert::place(
        &mut held,
        &video_card(dir).to_string_lossy(),
        34.0,
        cut_insert::Mode::Lane,
        8.0,
        false,
    );
    held
}

/// Stage the flow and hand back the sentences it printed, in order, for the caller's `snapshot:` line.
pub fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    // S7's card render writes beside the project it was asked to render, not beside a temp folder.
    ui::cut_card_preview::set_card_snapshot_dir(dir);
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(window, &seeded());
    // Deepen the zoom so a 5 s card is a real bar: at the opening 4 px/s it is 20 px and the violet is a
    // hairline nobody can read off a screenshot.
    for _ in 0..4 {
        ui::press_zoom(window, true);
    }
    let mut said: Vec<String> = Vec::new();
    seeded_cards(dir);
    // The scroll stays at the head, where a page opens: the whole staged span has to be inside the strip's
    // window or the shot records empty tape instead of the three inserts.
    let staged = seeded_after_stage(dir);
    // Drawn FROM the cut before any press runs: `cut_insert::place` is the same call Apply makes, so the marks
    // are the ones the rules wrote, and the three real presses below reproduce exactly this state (the stage is
    // re-seeded to it after each), so nothing is placed twice and no lane gets a second row.
    ui::seed_review_cut(window, &staged);

    // 1. BETWEEN, through the real buttons: chooser seam, radio, Apply. Its words come FIRST in the log line,
    // because everything after them re-places what the strip already shows.
    ui::note_place(true);
    ui::set_line_position(window, LinePos { t: 10.0 });
    ui::clear_selection(window);
    said.push(ui::insert_chosen_with_length(window, &card_path(dir, "s.svg"), 10.0, Some(5.0)));
    pick_mode(window, "insert-mode-between");
    said.push(ui::press_insert_apply(window));

    // 2. OVER: a drawn selection supplies both the length and the overwrite, so the footage under 20-28 is
    // gone and the band shows the dropped stretch the card now covers.
    ui::seed_review_cut(window, &staged);
    ui::draw_selection(window, crate::cut_select::Surface::PictureRow(0), None, 20.0, 28.0);
    said.push(ui::insert_chosen_with_length(window, &card_path(dir, "a.svg"), 20.0, Some(8.0)));
    pick_mode(window, "insert-mode-over");
    said.push(ui::press_insert_apply(window));

    // 3. LANE: a video onto a row of its own. The lane radio only appears for a video-shaped file, and LANE
    // cuts nothing -- the row shows in the band and in the camera list's per-lane badges.
    ui::seed_review_cut(window, &staged);
    ui::clear_selection(window);
    ui::set_line_position(window, LinePos { t: 34.0 });
    said.push(ui::insert_chosen_with_length(window, &video_card(dir), 34.0, Some(8.0)));
    pick_mode(window, "insert-mode-lane");
    said.push(ui::press_insert_apply(window));
    // The cancel, before the strip goes back: Cancel closes the form and writes nothing, which is §G's other
    // button and the answer no still can show either.
    ui::note_place(true);
    ui::set_line_position(window, LinePos { t: 12.0 });
    said.push(ui::insert_chosen_with_length(window, &card_path(dir, "s.svg"), 12.0, Some(5.0)));
    said.push(ui::close_insert_form(window));
    ui::seed_review_cut(window, &staged);

    // 4. §5's own length: the SAME card as stage 1, but asked with no length, so nothing but the file can say how
    // long it runs. Its Seconds entry reads 1.6 -- the animation's last moving moment -- where every stage above
    // was handed an explicit number, and the log line says both that number and the default it beat.
    ui::seed_review_cut(window, &staged);
    ui::note_place(true);
    ui::clear_selection(window);
    ui::set_line_position(window, LinePos { t: 15.0 });
    let timed = ui::insert_chosen_with_length(window, &timed_card(dir), 15.0, None);
    let shown = find_form_entry(window, "insert-seconds")
        .text()
        .to_string();
    said.push(format!(
        "{timed}; seconds {} (the card's own time, not {})",
        shown,
        cut_insert::DEFAULT_SECONDS
    ));

    // The form this stage drew is what `05-insert-form` paints (§G's whole form column: the declared field, the
    // three radios, the Seconds entry at the card's own second, Cancel + Insert it), so it stays open here and
    // nothing after it closes it. The refusal below needs no form of its own -- it IS the absence of one.
    //
    // 5. The last word on the status row: S1's refusal, no line claimed and nothing marked -- what the flow
    // answers when nothing was aimed at. AFTER everything else on purpose: an open form hides the readout
    // column §1 puts beside it.
    ui::note_place(false);
    ui::clear_selection(window);
    said.push(ui::press_insert(window));
    said
}

/// An entry the form drew, found under this window's own `insert-form` holder — the same walk [`pick_mode`] does,
/// on the widget names §G lists.
fn find_form_entry(window: &adw::ApplicationWindow, name: &str) -> gtk::Entry {
    let form =
        ui::window::find_widget_by_name(window.upcast_ref(), "insert-form").expect("the form holder exists");
    let found = ui::window::find_widget_by_name(&form, name)
        .unwrap_or_else(|| panic!("the insert form drew no {name}"));
    found.downcast::<gtk::Entry>().expect("the seconds entry is an entry")
}

/// The band's own arithmetic behind the shot, so the log line can say what the strip should be showing:
/// how many lanes the cut has after the LANE insert, and how long the video got.
pub fn describe(window: &adw::ApplicationWindow) -> String {
    let held = cut::Cut {
        segs: ui::review_cut_segs(window),
        lanes: ui::review_lanes(window),
        ..Default::default()
    };
    let lane_names: Vec<&str> = held.lanes.iter().map(|lane| lane.name.as_str()).collect();
    format!(
        "{} segs, {} lane(s) [{}], {} s of video",
        held.segs.len(),
        held.lanes.len(),
        lane_names.join(", "),
        cut_screen::cut_seconds(&held).round(),
    )
}
