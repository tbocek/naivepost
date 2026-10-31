//! The `05-insert` snapshot: F2.12's screen, staged like `05-copy` (§F2.9): every mark is written by a real door (`insert_chosen_with_length`, a radio click, `press_insert_apply`), never by poking state. Split out of `snapshot.rs`.
//! Faces of the flow: 1. a card spliced BETWEEN the footage (S5); 2. a card OVER a drawn selection (S5); 3. a video on a LANE of its own (S5); 4. the same card asked with no length, so its Seconds entry comes from the file (§5); 5. the S1 refusal with no line and no selection.
//! One PNG records the page after the three inserts, with the S1 refusal last on the status row. The form column is `insert-form`, pinned by `tests/suite/insert_form_widgets.rs`; a fourth open form would hide the readout column. The strip is re-seeded to the staged three through `seed_review_cut`.

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

/// The tape: 0–120 s, one take, so a card dropped between clips has its hole.
fn tape() -> Vec<Recording> {
    vec![Recording {
        base: "session-tape".into(),
        start: 0.0,
        end: 120.0,
    }]
}

/// The starting cut: two clips with a hole at 30–40, so spliced and over cards land differently and the lane has footage to overlap.
fn seeded() -> cut::Cut {
    cut::Cut {
        segs: vec![
            cut::Seg {
                s: 0.0,
                e: 30.0,
                cam: 0,
                ..Default::default()
            },
            cut::Seg {
                s: 40.0,
                e: 110.0,
                cam: 0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

/// Write the cards the flow inserts: the assets folder is seeded as `press_insert` does (`cut_insert::seed_cards`), and the two inserts are built-in cards.
fn seeded_cards(dir: &Path) {
    let assets = dir.join("assets");
    let _ = std::fs::create_dir_all(&assets);
    cut_insert::seed_cards(&assets, &cut_cards::seeds());
    for name in ["s.svg", "a.svg"] {
        assert!(
            assets.join(name).is_file(),
            "the built-in card {name} was not seeded into {}",
            assets.display()
        );
    }
}

fn card_path(dir: &Path, name: &str) -> std::path::PathBuf {
    dir.join("assets").join(name)
}

/// Click a radio under this window's own `insert-form` holder, as `tests/suite/insert_form_widgets.rs` does; a tree-wide name lookup can hit another window's form.
fn pick_mode(window: &adw::ApplicationWindow, name: &str) {
    fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if root.widget_name() == name {
            return Some(root.clone());
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else {
                continue;
            };
            if let Some(found) = find(&widget, name) {
                return Some(found);
            }
        }
        None
    }
    let form = ui::window::find_widget_by_name(window.upcast_ref(), "insert-form")
        .expect("the form holder exists");
    let radio = find(&form, name).unwrap_or_else(|| panic!("the insert form drew no {name}"));
    let radio = radio
        .downcast::<gtk::CheckButton>()
        .expect("the mode radios are radio buttons");
    radio.set_active(true);
}

/// The card a LANE insert needs: LANE is offered only to a VIDEO-shaped file (`cut_insert::lane_offered`), so a video is written next to the SVG badges; its seconds come from the `file_seconds` the seam is handed (a snapshot must not need ffprobe).
fn video_card(dir: &Path) -> std::path::PathBuf {
    let assets = dir.join("assets");
    let _ = std::fs::create_dir_all(&assets);
    let path = assets.join("sting.mp4");
    if !path.is_file() {
        std::fs::write(
            &path,
            b"staged for the snapshot: an insert is a FILE, its bytes are never read here",
        )
        .expect("a card file the chooser could have returned");
    }
    path
}

/// A card with a sound of its own: a video with an audio stream (§S4's tick is drawn only when there is sound; `cut_insert::sound_open` refuses bare audio or a LANE). `window_insert::file_has_sound` asks ffprobe, so a stub of bytes answers "no sound".
fn sound_card(dir: &Path) -> std::path::PathBuf {
    let assets = dir.join("assets");
    let _ = std::fs::create_dir_all(&assets);
    let wav = assets.join("sting-src.wav");
    if crate::snapshot::write_silence_wav(&wav, 8000, 8000 * 3).is_err() {
        std::fs::write(&wav, b"staged silent sting").expect("a staged wav");
    }
    let path = assets.join("sting-card.mp4");
    if !path.is_file() {
        let _ = crate::subprocess::run(
            "ffmpeg",
            &[
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=320x240:d=3:r=10",
                "-i",
                &wav.to_string_lossy(),
                "-shortest",
                "-pix_fmt",
                "yuv420p",
                &path.to_string_lossy(),
            ],
            |_| {},
        );
    }
    if path.is_file() {
        path
    } else {
        // No ffmpeg here: fall back to the bare WAV; only the tick is absent (§S4).
        wav
    }
}

/// A card with a moving moment of its own. §5's "card length = last moving moment" is shown: with no length `insert_chosen_with_length` asks the FILE (ffprobe reads an SVG as one frame). The animation ends at 1.6 s, not `cut_insert::DEFAULT_SECONDS`; its holes are declared (§G's `<!-- Input: ... -->`) so the field row appears.
fn timed_card(dir: &Path) -> std::path::PathBuf {
    let assets = dir.join("assets");
    let _ = std::fs::create_dir_all(&assets);
    let path = assets.join("timed.svg");
    // Rewritten every run so a stale copy cannot survive into the shot.
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

/// The cut the strip shows: the three staged inserts from [`cut_insert::place`] directly (the call Apply makes); they only have to EXIST for the band to draw them.
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

pub fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    ui::cut_card_preview::set_card_snapshot_dir(dir);
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(window, &seeded());
    // Deepen the zoom so a 5 s card is a real bar (at 4 px/s it is a hairline).
    for _ in 0..4 {
        ui::press_zoom(window, true);
    }
    let mut said: Vec<String> = Vec::new();
    seeded_cards(dir);
    // Scroll stays at the head so the whole staged span is inside the window.
    let staged = seeded_after_stage(dir);
    // Drawn FROM the cut before any press: `cut_insert::place` is Apply's call, so the three real presses below reproduce this state (re-seeded after each).
    ui::seed_review_cut(window, &staged);

    // 1. BETWEEN via the real buttons. Its words come FIRST in the log line.
    ui::note_place(true);
    ui::set_line_position(window, LinePos { t: 10.0 });
    ui::clear_selection(window);
    said.push(ui::insert_chosen_with_length(
        window,
        &card_path(dir, "s.svg"),
        10.0,
        Some(5.0),
    ));
    pick_mode(window, "insert-mode-between");
    said.push(ui::press_insert_apply(window));

    // 2. OVER: a drawn selection supplies length and overwrite.
    ui::seed_review_cut(window, &staged);
    ui::draw_selection(
        window,
        crate::cut_select::Surface::PictureRow(0),
        None,
        20.0,
        28.0,
    );
    said.push(ui::insert_chosen_with_length(
        window,
        &card_path(dir, "a.svg"),
        20.0,
        Some(8.0),
    ));
    pick_mode(window, "insert-mode-over");
    said.push(ui::press_insert_apply(window));

    // 3. LANE: a video onto a row of its own; the lane radio appears only for a video-shaped file and cuts nothing.
    ui::seed_review_cut(window, &staged);
    ui::clear_selection(window);
    ui::set_line_position(window, LinePos { t: 34.0 });
    said.push(ui::insert_chosen_with_length(
        window,
        &video_card(dir),
        34.0,
        Some(8.0),
    ));
    pick_mode(window, "insert-mode-lane");
    said.push(ui::press_insert_apply(window));
    // Cancel closes the form and writes nothing (§G's other button).
    ui::note_place(true);
    ui::set_line_position(window, LinePos { t: 12.0 });
    said.push(ui::insert_chosen_with_length(
        window,
        &card_path(dir, "s.svg"),
        12.0,
        Some(5.0),
    ));
    said.push(ui::close_insert_form(window));
    ui::seed_review_cut(window, &staged);

    // 4. §5's own length and §S4's sound tick: the same card as stage 1 with no length (Seconds reads 1.6), then re-opened on a card that brings a sound. The form left open is what `05-insert-form` paints.
    ui::seed_review_cut(window, &staged);
    ui::note_place(true);
    ui::clear_selection(window);
    ui::set_line_position(window, LinePos { t: 15.0 });
    let timed = ui::insert_chosen_with_length(window, &timed_card(dir), 15.0, None);
    let shown = find_form_entry(window, "insert-seconds").text().to_string();
    said.push(format!(
        "{timed}; seconds {} (the card's own time, not {})",
        shown,
        cut_insert::DEFAULT_SECONDS
    ));
    // The tick-bearing form, found under THIS window's drawing holder (one unscoped name exists per tab).
    ui::set_line_position(window, LinePos { t: 15.0 });
    let stung = ui::insert_chosen_with_length(window, &sound_card(dir), 15.0, Some(3.0));
    let tick_drawn =
        crate::ui::window::find_widget_by_name(window.upcast_ref(), "insert-sound-tick").is_some();
    // The proof: the tick is on screen or the log says so.
    assert!(
        tick_drawn,
        "\u{a7}S4's sound tick was not drawn for a card that brings a sound of its own"
    );
    said.push(format!("{stung}; sound tick drawn"));

    // The form drawn here is what `05-insert-form` paints, so it stays open.
    // 5. The last word on the status row: S1's refusal, AFTER everything else because an open form hides the readout column (§1).
    ui::note_place(false);
    ui::clear_selection(window);
    said.push(ui::press_insert(window));
    said
}

/// An entry the form drew, found as [`pick_mode`] does, on the widget names §G lists.
fn find_form_entry(window: &adw::ApplicationWindow, name: &str) -> gtk::Entry {
    let form = ui::window::find_widget_by_name(window.upcast_ref(), "insert-form")
        .expect("the form holder exists");
    let found = ui::window::find_widget_by_name(&form, name)
        .unwrap_or_else(|| panic!("the insert form drew no {name}"));
    found
        .downcast::<gtk::Entry>()
        .expect("the seconds entry is an entry")
}

/// The band's arithmetic behind the shot, for the log line: lanes after the LANE insert and the video's new length.
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
